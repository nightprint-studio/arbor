//! The DSP core: voice pool + mixer + effects + registry + sample bank.
//!
//! `Renderer` is **transport-agnostic** — it neither opens a stream nor owns a
//! ring buffer. It is driven two ways against the exact same code path:
//! - the real-time cpal callback (`crate::stream`) drains the ring and calls
//!   [`Renderer::process`] for each device buffer;
//! - the engine's offline render driver calls [`Renderer::process`] block by
//!   block in non-real-time and writes the output to WAV.
//!
//! ## Signal flow
//!
//! ```text
//! voices ─┬─▶ per-track strip (gain, mute) ─┐
//!         │                                  ├─▶ master sum ─▶ limiter ─▶ out
//!         └─▶ × room ─▶ reverb send bus ─────┘
//! ```
//!
//! ## Real-time discipline
//!
//! `process` never allocates, locks, or does IO. Pools and the per-track scratch
//! buffer are sized in [`new`](Renderer::new) / [`configure_tracks`]; sample data
//! and SFZ instruments are loaded ahead of time on a non-RT path
//! ([`registry_mut`](Renderer::registry_mut) / [`preload_file`]) and only read in
//! the callback. A `File` voice whose sample isn't resident falls back to the
//! synth rather than blocking.

use std::collections::BinaryHeap;
use std::path::Path;

use crate::effects::{Limiter, Reverb};
use crate::registry::{Registry, ResolvedVoice, SampleParams};
use crate::sampler::{Sample, SampleBank};
use crate::seam::{AudioCommand, Frame, TrackConfig, VoiceEvent, VoiceSource};
use crate::voice::{Voice, VoicePool};
use arbor_grove_pattern::prelude::SourceKind;

/// Default voice-pool capacity (design: 128). A `const` now; user-configurable
/// later through the renderer constructor.
pub const DEFAULT_VOICE_CAPACITY: usize = 128;

/// One mixer strip: a linear gain and a mute flag.
#[derive(Clone, Copy, Debug)]
struct Strip {
    gain: f32,
    muted: bool,
}

impl Default for Strip {
    fn default() -> Self {
        Strip {
            gain: 1.0,
            muted: false,
        }
    }
}

/// A voice waiting to start at a specific absolute frame (sample-accurate
/// within-block onset). Ordered so the *earliest* start pops first.
#[derive(Debug)]
struct Pending {
    start_frame: u64,
    event: VoiceEvent,
}

impl PartialEq for Pending {
    fn eq(&self, other: &Self) -> bool {
        self.start_frame == other.start_frame
    }
}
impl Eq for Pending {}
impl PartialOrd for Pending {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Pending {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse: `BinaryHeap` is a max-heap, we want the soonest start on top.
        other.start_frame.cmp(&self.start_frame)
    }
}

/// Owns all sounding state and turns commands + time into samples.
pub struct Renderer {
    sample_rate: u32,
    /// Absolute output frame of the *next* frame to render.
    clock: u64,
    pool: VoicePool,
    strips: Vec<Strip>,
    /// Reused per-track dry accumulator (one L/R per strip), cleared each frame.
    track_dry: Vec<Frame>,
    reverb: Reverb,
    limiter: Limiter,
    registry: Registry,
    /// Resident audio for `File` (`sample`/`audio`) voices, keyed by path.
    files: SampleBank,
    /// Voices scheduled to start later within the current/next block.
    pending: BinaryHeap<Pending>,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("sample_rate", &self.sample_rate)
            .field("clock", &self.clock)
            .field("active_voices", &self.pool.active())
            .field("tracks", &self.strips.len())
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// Build a renderer for `sample_rate`, laying out a mixer strip per track.
    pub fn new(sample_rate: u32, tracks: &[TrackConfig]) -> Self {
        Renderer::with_capacity(sample_rate, tracks, DEFAULT_VOICE_CAPACITY)
    }

    /// Build a renderer with an explicit voice-pool capacity.
    pub fn with_capacity(sample_rate: u32, tracks: &[TrackConfig], capacity: usize) -> Self {
        let strip_count = tracks.len().max(1);
        Renderer {
            sample_rate,
            clock: 0,
            pool: VoicePool::new(capacity),
            strips: vec![Strip::default(); strip_count],
            track_dry: vec![[0.0; 2]; strip_count],
            reverb: Reverb::new(sample_rate as f32),
            limiter: Limiter::new(0.95, 0.05, sample_rate as f32),
            registry: Registry::new(),
            files: SampleBank::new(),
            // Pre-reserve so per-block `push`es don't grow the heap in the RT
            // callback (one look-ahead window is a few hundred events at most).
            pending: BinaryHeap::with_capacity(capacity * 4),
        }
    }

    /// Frames per second this renderer produces.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The current absolute frame clock (next frame to be rendered).
    pub fn now_frame(&self) -> u64 {
        self.clock
    }

    /// Mutable access to the sound registry for **non-RT** setup (load a manifest,
    /// install presets). Never call from the audio callback.
    pub fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    /// Replace the registry wholesale (non-RT).
    pub fn set_registry(&mut self, registry: Registry) {
        self.registry = registry;
    }

    /// Decode a `sample`/`audio` file and keep it resident so `File` voices for
    /// `path` play instead of falling back to the synth. **Non-RT** (IO + alloc).
    pub fn preload_file(&mut self, path: &Path) -> crate::error::Result<()> {
        self.files.load(path)?;
        Ok(())
    }

    /// Drain the **due** commands, render exactly `out.len()` frames into `out`,
    /// and advance the internal frame clock by that many frames.
    ///
    /// `commands` yields the commands scheduled up to the end of this block;
    /// voice `start_frame`s inside the block are honoured sample-accurately
    /// (a voice starting mid-block stays silent until its frame).
    pub fn process(&mut self, commands: &mut dyn Iterator<Item = AudioCommand>, out: &mut [Frame]) {
        let block_end = self.clock + out.len() as u64;

        // 1. Apply commands. Voice triggers are queued (sample-accurate); mixer
        //    / transport commands take effect immediately.
        for cmd in commands {
            self.apply_command(cmd);
        }

        // 2. Render frame by frame, starting any pending voices at their frame.
        //    `start_due_voices` pops every voice with start_frame ≤ the current
        //    frame, so by block end nothing due this block is left pending; a
        //    voice whose start lands in a later block simply waits.
        for (i, frame_out) in out.iter_mut().enumerate() {
            let frame = self.clock + i as u64;
            self.start_due_voices(frame);
            *frame_out = self.render_frame(frame);
        }

        self.clock = block_end;
    }

    /// Number of currently sounding voices (for meters / tests).
    pub fn active_voices(&self) -> usize {
        self.pool.active()
    }

    // ── internals ────────────────────────────────────────────────────────────

    fn apply_command(&mut self, cmd: AudioCommand) {
        match cmd {
            AudioCommand::Voice(ev) => {
                // A voice already overdue (start ≤ clock) starts at the block top.
                let start = ev.start_frame.max(self.clock);
                self.pending.push(Pending {
                    start_frame: start,
                    event: ev,
                });
            }
            AudioCommand::ConfigureTracks(tracks) => self.configure_tracks(&tracks),
            AudioCommand::SetTrackGain(i, g) => {
                if let Some(s) = self.strips.get_mut(i as usize) {
                    s.gain = g;
                }
            }
            AudioCommand::SetTrackMute(i, m) => {
                if let Some(s) = self.strips.get_mut(i as usize) {
                    s.muted = m;
                }
            }
            AudioCommand::StopAll => {
                self.pool.release_all();
                self.pending.clear();
            }
        }
    }

    /// (Re)lay the mixer strips, preserving existing gain/mute where indices line
    /// up. Resizes the per-track scratch buffer to match.
    fn configure_tracks(&mut self, tracks: &[TrackConfig]) {
        let count = tracks.len().max(1);
        let mut next = vec![Strip::default(); count];
        for (i, slot) in next.iter_mut().enumerate() {
            if let Some(old) = self.strips.get(i) {
                *slot = *old;
            }
        }
        self.strips = next;
        self.track_dry = vec![[0.0; 2]; count];
    }

    /// Start every pending voice whose `start_frame` equals `frame`.
    fn start_due_voices(&mut self, frame: u64) {
        while let Some(p) = self.pending.peek() {
            if p.start_frame <= frame {
                let Pending { event, .. } = self.pending.pop().expect("peeked");
                self.spawn_voice(event, frame);
            } else {
                break;
            }
        }
    }

    /// Resolve an event's source + build a [`Voice`] and add it to the pool.
    fn spawn_voice(&mut self, ev: VoiceEvent, frame: u64) {
        let resolved = self.resolve_source(&ev);
        let release_at = ev.dur_frames.map(|d| frame.saturating_add(d));
        let voice = Voice::build(
            ev.id,
            ev.track,
            resolved,
            ev.note,
            &ev.params,
            release_at,
            self.sample_rate as f32,
        );
        self.pool.insert(voice);
    }

    /// Turn a [`VoiceSource`] into a concrete [`ResolvedVoice`]; falls back to the
    /// synth for unresolved names and unresident files. RT-safe (reads resident
    /// state only).
    fn resolve_source(&self, ev: &VoiceEvent) -> ResolvedVoice {
        match &ev.source {
            VoiceSource::Named { sound, inst, .. } => {
                self.registry
                    .resolve(sound.as_deref(), inst.as_deref(), ev.note, ev.params.vel)
            }
            VoiceSource::File { path, kind } => self.resolve_file(path, *kind),
        }
    }

    /// Resolve a `File` source against the resident file bank; fall back to the
    /// synth if not yet decoded. A `Sustained` stem plays whole from its start;
    /// a `OneShot` plays the hit and decays (both via the sample player — the
    /// distinction is just the release behaviour the engine drives via
    /// `dur_frames`).
    fn resolve_file(&self, path: &str, kind: SourceKind) -> ResolvedVoice {
        match self.files.get(path).or_else(|| self.files_get_by_suffix(path)) {
            Some(sample) => ResolvedVoice::Sample {
                sample,
                region: file_region(kind),
            },
            None => ResolvedVoice::Synth(self.registry.fallback()),
        }
    }

    /// Files are keyed by the absolute path used at preload; the engine may send
    /// the source-relative path. Resolve by suffix match as a fallback.
    fn files_get_by_suffix(&self, path: &str) -> Option<Sample> {
        let needle = path.replace('\\', "/");
        let key = self
            .files
            .keys()
            .find(|k| k.replace('\\', "/").ends_with(&needle))
            .map(|k| k.to_string())?;
        self.files.get(&key)
    }

    /// Render and mix one frame at absolute `frame`.
    fn render_frame(&mut self, frame: u64) -> Frame {
        // Clear per-track + send accumulators.
        for d in &mut self.track_dry {
            *d = [0.0; 2];
        }
        let mut send = [0.0f32; 2];

        // Sum all voices into their track strips + the reverb send.
        self.pool.process_sample(frame, &mut self.track_dry, &mut send);

        // Apply per-strip gain/mute and sum to the master.
        let mut master = [0.0f32; 2];
        for (strip, dry) in self.strips.iter().zip(self.track_dry.iter()) {
            if strip.muted {
                continue;
            }
            master[0] += dry[0] * strip.gain;
            master[1] += dry[1] * strip.gain;
        }

        // Reverb send → wet → fold back into master.
        let wet = self.reverb.process(send);
        master[0] += wet[0];
        master[1] += wet[1];

        // Master limiter.
        self.limiter.process(master)
    }
}

/// SFZ-style playback params for a plain `File` source: full-range, native pitch.
/// `Sustained` stems ring out; the engine's `dur_frames` (or natural end) drives
/// release. We give a tiny release so stopping doesn't click.
fn file_region(_kind: SourceKind) -> SampleParams {
    SampleParams {
        // `pitch_keycenter` is irrelevant when the engine sends no note; an
        // unpitched File plays at native pitch (note=None → no offset).
        release: 0.01,
        ..SampleParams::default()
    }
}
