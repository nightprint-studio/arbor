//! Canonical entry point for `merula-import`'s public API.
//!
//! Workspace convention: reach the public surface through `prelude` rather than
//! per-module paths. The submodules stay `pub` for rustdoc navigation only.

// ── Errors ───────────────────────────────────────────────────────────────────
pub use crate::error::{ImportError, Result};

// ── Model ────────────────────────────────────────────────────────────────────
pub use crate::model::{ImportOptions, Note, NoteTrack, Song};

// ── One-call conversion ──────────────────────────────────────────────────────
pub use crate::convert::{midi_to_merula, midi_to_song, smf_to_merula};

// ── Layers (for callers that drive the pipeline by hand or test in isolation) ─
pub use crate::chords::{recognize as recognize_chord, Chord};
pub use crate::emit::song_to_merula;
pub use crate::gm_drum::sound_for_key;
pub use crate::key::{degree_of, detect as detect_key, DetectedKey};
pub use crate::quantize::quantize_song;
pub use crate::transcode::{from_bytes as transcode_bytes, from_smf as transcode_smf};
