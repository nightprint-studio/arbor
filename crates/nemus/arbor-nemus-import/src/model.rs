//! The neutral note model shared by L1 (producer) and L2 (consumer).
//!
//! Time is measured in **cycles** — nemus's native unit (1 cycle = 1 bar by
//! default). Using [`Time`] (exact rationals) here, not floats, means quantised
//! onsets land on slot boundaries with no drift, so the emitter can read the
//! grid straight off the denominators.

use arbor_nemus_pattern::prelude::Time;

/// A single note, positioned and sized in cycles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    /// Onset, in cycles from the start of the song.
    pub start: Time,
    /// Duration, in cycles (always `> 0`).
    pub dur: Time,
    /// Pitch as a MIDI note number (`60` = C4). For drum tracks this is the GM
    /// percussion key, mapped to a sound name only at emit time.
    pub pitch: i32,
    /// Velocity, normalised to `0..=1`.
    pub vel: f64,
}

impl Note {
    /// Onset of the note's end (`start + dur`).
    pub fn end(&self) -> Time {
        self.start + self.dur
    }
}

/// One track of notes. Either a pitched part or a drum part — never mixed, so
/// the emitter can pick `n(...)` vs `s(...)` per track without per-note checks.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteTrack {
    /// Human-facing track name (from the MIDI track name, or a generated one).
    pub name: String,
    /// Drum part (GM channel 10) → emitted as a sound island `s(...)`.
    pub is_drum: bool,
    /// Notes, sorted by `(start, pitch)`.
    pub notes: Vec<Note>,
}

/// A transcribed song: the L1 output and the L2 input.
#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    /// One entry per (non-empty) MIDI part.
    pub tracks: Vec<NoteTrack>,
    /// Tempo, in **cycles per second** (nemus's `cps`).
    pub cps: f64,
    /// Total length in whole cycles (ceiling of the last note's end).
    pub len_cycles: u32,
}

/// Tunable knobs for the whole MIDI → `.nemus` pipeline.
#[derive(Clone, Copy, Debug)]
pub struct ImportOptions {
    /// Beats per cycle — the bar length. `4` maps a 4/4 bar to one cycle.
    pub beats_per_cycle: u32,
    /// Quantisation grid: subdivisions per cycle (e.g. `16` = sixteenth-note
    /// grid). `0` disables quantisation (raw timing kept).
    pub grid: u32,
    /// Run key/scale detection and emit scale degrees where every note of a
    /// track fits the detected scale.
    pub detect_key: bool,
    /// Fold simultaneous notes into chord symbols (`c4'min7`) where recognised.
    pub detect_chords: bool,
    /// Collapse a repeating run of cycles into one alternation (`<...>`).
    pub detect_loops: bool,
    /// The octave that scale degree `0` resolves to — must match the evaluator's
    /// `default_octave` for degree emission to be pitch-accurate (nemus default
    /// is `4`, i.e. C4 = 60).
    pub ref_octave: i32,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            beats_per_cycle: 4,
            grid: 16,
            detect_key: true,
            detect_chords: true,
            detect_loops: true,
            ref_octave: 4,
        }
    }
}
