//! Canonical entry point for `arbor-nemus-pattern`'s public API.
//!
//! Workspace convention: every Arbor library crate exposes its public surface
//! through a `prelude` module. Consumers reach types through
//! `arbor_nemus_pattern::prelude::...` (or `use ...prelude::*;` once per file)
//! rather than the per-feature submodule paths. The submodules stay `pub` for
//! rustdoc navigation only.
//!
//! Transforms (`fast`, `slow`, `rev`, `gain`, …) are inherent methods on
//! [`Pattern`] and need no import — they are available wherever `Pattern` is.

// ── Core types ──────────────────────────────────────────────────────────────
pub use crate::control::{ControlMap, HoldSpec, SourceKind};
pub use crate::error::{PatternError, Result};
pub use crate::hap::Hap;
pub use crate::pattern::Pattern;
pub use crate::pitch::{parse_note, Scale, MIDDLE_C};
pub use crate::span::{SourceSpan, TimeSpan};
pub use crate::tempo::TempoMap;
pub use crate::time::Time;

// ── Combinators: composition ─────────────────────────────────────────────────
pub use crate::combinators::compose::{
    arrange, cat, cycles, fastcat, par, polymeter, pure, section, section_layout, seq, silence,
    slowcat, stack, timecat, track, track_with_sections, tracks, Section, SectionSpan, Track,
    Tracks,
};

// ── Combinators: patternised postfix args (mini-notation `bd*<2 3>`) ──────────
pub use crate::combinators::patterned::{euclid_with, fast_with, slow_with};

// ── Combinators: continuous signal sources + `.range` ────────────────────────
pub use crate::combinators::signal::{isaw, saw, sine, square, tri};

// ── Combinators: generative & voice param ────────────────────────────────────
pub use crate::combinators::generative::{choose, rand};
pub use crate::combinators::voice::Param;

// ── Combinators: file sources ────────────────────────────────────────────────
pub use crate::combinators::source::{audio, sample};

// ── RNG (for engine/consumers needing the same deterministic stream) ─────────
pub use crate::rng::{time_to_index, time_to_rand};
