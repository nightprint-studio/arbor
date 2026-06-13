//! The sampler: a [`SampleBank`] of resident decoded audio (shared as `Arc`) and
//! a [`SamplePlayer`] that reads one resident buffer with pitch via variable-rate
//! linear-interpolation resampling.
//!
//! ## Real-time discipline
//!
//! Decoding allocates and does IO, so it happens on a **non-RT** path
//! ([`SampleBank::load`] / [`SampleBank::insert`]) and the result is stored as
//! `Arc<[f32]>`. The RT callback only ever *reads* a resident `Arc` (cloning an
//! `Arc` is a refcount bump, no allocation) — if a sample isn't resident yet
//! when a voice triggers, the renderer falls back to the synth instead of
//! blocking on a decode.
//!
//! Pitch is done by stepping through the buffer at a fractional rate
//! (`ratio = source_rate/device_rate × 2^(semitones/12)`), interpolating between
//! neighbouring samples. This is the classic sampler approach: RT-safe, no
//! per-voice allocation, and exactly the "resampling couples pitch + duration"
//! semantics the design specifies for `shift` / `speed`. (`rubato` stays for the
//! offline, fixed-ratio sample-rate conversion path.)

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::decode::DecodedAudio;
use crate::error::Result;
use crate::sfz::LoopMode;

/// A resident sample: mono `f32` audio plus the rate it was decoded at. Shared
/// immutably with the RT thread via `Arc`.
#[derive(Clone, Debug)]
pub struct Sample {
    /// Mono samples, kept resident for the RT path to read.
    pub data: Arc<[f32]>,
    /// The sample's native rate (for device-rate compensation).
    pub sample_rate: u32,
}

impl Sample {
    /// Wrap already-decoded audio.
    pub fn from_decoded(d: DecodedAudio) -> Self {
        Sample {
            data: Arc::from(d.samples.into_boxed_slice()),
            sample_rate: d.sample_rate,
        }
    }

    /// Number of sample frames.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// A content-addressed cache of decoded samples, keyed by resolved path.
///
/// Owned off the RT thread. The renderer is handed cheap `Arc` clones of the
/// resident [`Sample`]s (via [`get`](SampleBank::get)); it never touches this map
/// from the callback.
#[derive(Clone, Debug, Default)]
pub struct SampleBank {
    resident: HashMap<String, Sample>,
}

impl SampleBank {
    /// An empty bank.
    pub fn new() -> Self {
        SampleBank::default()
    }

    /// Decode `path` (if not already resident) and keep it. Non-RT.
    pub fn load(&mut self, path: &Path) -> Result<Sample> {
        let key = path.to_string_lossy().into_owned();
        if let Some(s) = self.resident.get(&key) {
            return Ok(s.clone());
        }
        let decoded = DecodedAudio::load(path)?;
        let sample = Sample::from_decoded(decoded);
        self.resident.insert(key, sample.clone());
        Ok(sample)
    }

    /// Insert pre-decoded audio under `key` (used by tests and the registry when
    /// it already holds decoded data).
    pub fn insert(&mut self, key: impl Into<String>, sample: Sample) {
        self.resident.insert(key.into(), sample);
    }

    /// A resident sample by key, or `None` (caller falls back to the synth).
    pub fn get(&self, key: &str) -> Option<Sample> {
        self.resident.get(key).cloned()
    }

    /// Whether `key` is resident.
    pub fn contains(&self, key: &str) -> bool {
        self.resident.contains_key(key)
    }

    /// All resident keys (for the registry's suffix-match SFZ sample resolution).
    /// Not RT-hot — used only the first time a region resolves.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.resident.keys().map(String::as_str)
    }
}

/// Looping parameters resolved for a player (frames, in the *resident* buffer).
#[derive(Clone, Copy, Debug)]
pub struct LoopSpec {
    pub mode: LoopMode,
    pub start: u64,
    pub end: u64,
}

/// An RT-safe playback cursor over one resident [`Sample`].
///
/// Holds an `Arc` to the buffer (a refcount, not the data) and a fractional read
/// position advanced by `ratio` each output sample. `ratio` bakes in both the
/// device-rate compensation and the pitch shift, so a single multiply-add per
/// sample covers `shift` + `speed` + native-rate matching.
#[derive(Clone, Debug)]
pub struct SamplePlayer {
    data: Arc<[f32]>,
    /// Fractional read position in source frames.
    pos: f64,
    /// Source frames consumed per output sample.
    ratio: f64,
    /// Loop spec, if the region loops.
    loop_spec: Option<LoopSpec>,
    /// Set once the player has read past the end (or release for one-shots).
    done: bool,
    /// Whether a sustained loop should keep looping (cleared on release).
    held: bool,
}

impl SamplePlayer {
    /// Build a player.
    ///
    /// * `sample` — resident audio.
    /// * `device_rate` — output sample rate.
    /// * `semitones` — total pitch shift (note offset + `shift`).
    /// * `speed` — playback speed factor (`speed`), couples pitch + duration.
    /// * `offset` — start position in source frames (SFZ `offset`).
    /// * `loop_spec` — looping behaviour, if any.
    pub fn new(
        sample: &Sample,
        device_rate: f32,
        semitones: f32,
        speed: f32,
        offset: u64,
        loop_spec: Option<LoopSpec>,
    ) -> Self {
        let rate_ratio = sample.sample_rate as f64 / device_rate as f64;
        let pitch_ratio = 2.0_f64.powf(semitones as f64 / 12.0);
        let ratio = rate_ratio * pitch_ratio * speed.max(0.001) as f64;
        SamplePlayer {
            data: sample.data.clone(),
            pos: offset as f64,
            ratio,
            loop_spec,
            done: sample.is_empty(),
            held: true,
        }
    }

    /// Whether playback has finished (cursor past the end and not looping).
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Signal note-off: a `loop_sustain` region stops looping and plays its tail.
    pub fn release(&mut self) {
        self.held = false;
    }

    /// Produce the next mono sample, advancing the cursor; returns `0.0` once
    /// done. Linear interpolation between adjacent source frames.
    pub fn next_sample(&mut self) -> f32 {
        if self.done {
            return 0.0;
        }
        let len = self.data.len();
        if len == 0 {
            self.done = true;
            return 0.0;
        }

        let i = self.pos.floor() as usize;
        let frac = (self.pos - i as f64) as f32;
        let a = self.data.get(i).copied().unwrap_or(0.0);
        let b = self.data.get(i + 1).copied().unwrap_or(a);
        let out = a + (b - a) * frac;

        self.pos += self.ratio;
        self.wrap_or_finish(len);
        out
    }

    /// Apply loop wrap or mark done once the cursor passes the end.
    fn wrap_or_finish(&mut self, len: usize) {
        let end_frame = len as f64;
        match self.loop_spec {
            Some(spec)
                if self.should_loop(spec) && spec.end > spec.start && spec.end as f64 <= end_frame =>
            {
                if self.pos >= spec.end as f64 {
                    let loop_len = (spec.end - spec.start) as f64;
                    // Preserve fractional overshoot across the wrap.
                    self.pos = spec.start as f64 + (self.pos - spec.end as f64) % loop_len;
                }
            }
            _ => {
                if self.pos >= end_frame {
                    self.done = true;
                }
            }
        }
    }

    /// Whether the region should currently loop given its mode + held state.
    fn should_loop(&self, spec: LoopSpec) -> bool {
        match spec.mode {
            LoopMode::LoopContinuous => true,
            LoopMode::LoopSustain => self.held,
            LoopMode::NoLoop | LoopMode::OneShot => false,
        }
    }
}
