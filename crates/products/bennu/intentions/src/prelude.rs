//! Canonical entry point for `bennu-intentions`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_intentions::prelude::...`.

// The aggregation seam the editor calls — every applicable intention at the caret.
pub use crate::intentions::{intentions_at, Offer};

// The individual transforms (also usable directly / from tests).
pub use crate::import_class::insert_import_edit;
pub use crate::log_param::{parameterize_log_call, LogParamRewrite};
pub use crate::np_equals::{np_safe_equals, EqualsSwap};
pub use crate::simplify::{
    simplify_boolean_compare, simplify_negated_comparison, simplify_size_check,
};
pub use crate::Edit;

// Generating an override: the rendered method and where in the class body it goes.
pub use crate::override_stub::{class_body_insertion, render_override, OverrideSpec};

// Quick-fixes — the repair attached to a diagnostic, keyed by its `code` and span.
pub use crate::quick_fix::{fixes_for, Fix};

// The Java formatter — re-indentation and whitespace tidying, as line-range edits.
pub use crate::format::{format_edits, format_source, FormatStyle};
