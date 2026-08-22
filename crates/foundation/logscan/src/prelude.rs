//! Canonical entry point for `arbor-logscan`'s public API.
//!
//! Workspace convention: call sites reach this crate through `arbor_logscan::prelude::...`
//! (or one `use arbor_logscan::prelude::*;`). The submodules stay public for rustdoc
//! navigation, but they are not the path a host should import from.
//!
//! A host needs, in practice, three names: [`RuleSet`] to say which dialect, [`LogReader`]
//! to read the stream, and [`Link`] to decide what a click does. [`Line::pieces`] gives it
//! something to render. The rest is here for hosts writing their own rules.

pub use crate::ansi::{strip, StyleRun};
pub use crate::common::{common_continues, diagnostic_level, level_of};
pub use crate::java::{class_of, is_synthetic, java_continues, method_of, outer_class};
pub use crate::model::{Colour, Level, Line, Link, Piece, Span, Style, Token};
pub use crate::reader::{interpret, LogReader};
pub use crate::rule::{indented, FnRule, Hit, Part, Rule, RuleSet};
pub use crate::scan::{is_boundary, scan, token_end};
