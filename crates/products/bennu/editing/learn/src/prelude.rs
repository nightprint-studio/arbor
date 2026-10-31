//! Canonical entry point for `bennu-learn`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_learn::prelude::...`. The submodules
//! stay `pub` for rustdoc navigation, but the prelude is the canonical call-site path.

// What became of a ghost text.
pub use crate::edit::{classify, shape_of, Layout, Outcome, Shape};

// Counters that fade, and the rates read off them.
pub use crate::tally::{Tally, HALF_LIFE_DAYS, MIN_SAMPLES};

// The learner and its switches.
pub use crate::store::{Event, Learner, Settings};

// How a project lays out its code, and text conformed to it.
pub use crate::style::{conform, FormatCounts, FormatStyle, Indent};
