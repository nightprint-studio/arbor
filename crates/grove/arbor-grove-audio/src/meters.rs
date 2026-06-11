//! Out-of-band audio telemetry: the level meters, voice count, and DSP load the
//! shell reads for the grove footer + mixer.
//!
//! This is the **same discipline as the playhead** ([`crate::stream`]): the
//! real-time callback *writes* a set of shared atomics each device buffer, and a
//! non-RT reader (the shell, ~30 fps) *snapshots* them. It is strictly a tap —
//! nothing here ever feeds back into rendering, and neither side allocates or
//! locks. It is **not** part of the engine↔audio command seam ([`crate::seam`]),
//! which stays frozen; this is additive, like the master-peak tap before it.
//!
//! Storage is lock-free atomics: each `f32` is held as its bit pattern in an
//! [`AtomicU32`] (an `f32` store/load is a single 32-bit word, so a reader never
//! sees a torn value — at worst a one-buffer-old one, which is exactly right for
//! a meter). Per-track peaks live in a fixed-size array ([`MAX_METER_TRACKS`]);
//! arrangements never approach the cap, and any strips beyond it are still
//! reflected in the master meter.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::seam::Frame;

/// Maximum number of per-track meters the tap carries. A music arrangement won't
/// come close; strips past this simply aren't individually metered (the master
/// meter still sums them).
pub const MAX_METER_TRACKS: usize = 64;

/// Shared, lock-free audio telemetry written by the real-time callback and read
/// out-of-band by the shell. Construct with [`MeterTap::new`] (returns an `Arc`
/// shared between the callback and the [`StreamSink`](crate::stream::StreamSink)).
#[derive(Debug)]
pub struct MeterTap {
    /// Master output peak `[L, R]` (post-limiter), each an `f32` as bits.
    master: [AtomicU32; 2],
    /// Per-track post-fader peak `[L, R]`, one slot per strip up to the cap.
    tracks: [[AtomicU32; 2]; MAX_METER_TRACKS],
    /// How many entries of `tracks` are live (= strip count, capped).
    track_count: AtomicUsize,
    /// Currently sounding voice count.
    voices: AtomicU32,
    /// DSP load `0..1`: callback compute time / buffer wall-time (smoothed),
    /// stored as `f32` bits.
    dsp_load: AtomicU32,
}

/// A consistent-enough read of the [`MeterTap`] for one front-end frame. Built on
/// the non-RT side; the per-track `Vec` allocates on the *reader*, never in the
/// callback.
#[derive(Clone, Debug, PartialEq)]
pub struct MeterSnapshot {
    /// Master output peak `[left, right]`, `0.0..~1.0`.
    pub master: [f32; 2],
    /// Per-track post-fader peak `[left, right]`, indexed by mixer strip.
    pub tracks: Vec<[f32; 2]>,
    /// Sounding voices right now.
    pub voices: u32,
    /// DSP load `0.0..~1.0` (1.0 ≈ the callback is using its whole time budget).
    pub dsp_load: f32,
}

impl MeterTap {
    /// A zeroed tap, shared (`Arc`) between the audio callback and the engine-
    /// facing [`StreamSink`](crate::stream::StreamSink).
    pub fn new() -> Arc<Self> {
        Arc::new(MeterTap {
            master: [AtomicU32::new(0), AtomicU32::new(0)],
            tracks: std::array::from_fn(|_| [AtomicU32::new(0), AtomicU32::new(0)]),
            track_count: AtomicUsize::new(0),
            voices: AtomicU32::new(0),
            dsp_load: AtomicU32::new(0),
        })
    }

    // ── RT writers (called from the audio callback) ────────────────────────────

    /// Store the master peak.
    pub fn store_master(&self, v: Frame) {
        self.master[0].store(v[0].to_bits(), Ordering::Relaxed);
        self.master[1].store(v[1].to_bits(), Ordering::Relaxed);
    }

    /// Read the held master peak (for the callback's own decay ballistics).
    pub fn load_master(&self) -> Frame {
        [
            f32::from_bits(self.master[0].load(Ordering::Relaxed)),
            f32::from_bits(self.master[1].load(Ordering::Relaxed)),
        ]
    }

    /// Store one track's peak. Out-of-range indices are ignored.
    pub fn store_track(&self, i: usize, v: Frame) {
        if let Some(slot) = self.tracks.get(i) {
            slot[0].store(v[0].to_bits(), Ordering::Relaxed);
            slot[1].store(v[1].to_bits(), Ordering::Relaxed);
        }
    }

    /// Read one track's held peak (for the callback's own decay ballistics).
    pub fn load_track(&self, i: usize) -> Frame {
        match self.tracks.get(i) {
            Some(slot) => [
                f32::from_bits(slot[0].load(Ordering::Relaxed)),
                f32::from_bits(slot[1].load(Ordering::Relaxed)),
            ],
            None => [0.0, 0.0],
        }
    }

    /// Set how many track slots are live (clamped to the cap).
    pub fn store_track_count(&self, n: usize) {
        self.track_count.store(n.min(MAX_METER_TRACKS), Ordering::Relaxed);
    }

    /// Store the sounding-voice count.
    pub fn store_voices(&self, n: u32) {
        self.voices.store(n, Ordering::Relaxed);
    }

    /// Store the DSP load `0..1`.
    pub fn store_dsp_load(&self, v: f32) {
        self.dsp_load.store(v.to_bits(), Ordering::Relaxed);
    }

    /// Read the held DSP load (for the callback's own smoothing).
    pub fn load_dsp_load(&self) -> f32 {
        f32::from_bits(self.dsp_load.load(Ordering::Relaxed))
    }

    // ── Non-RT reader (called from the shell) ──────────────────────────────────

    /// Snapshot the current telemetry for one front-end frame.
    pub fn snapshot(&self) -> MeterSnapshot {
        let n = self.track_count.load(Ordering::Relaxed).min(MAX_METER_TRACKS);
        MeterSnapshot {
            master: self.load_master(),
            tracks: (0..n).map(|i| self.load_track(i)).collect(),
            voices: self.voices.load(Ordering::Relaxed),
            dsp_load: self.load_dsp_load(),
        }
    }
}
