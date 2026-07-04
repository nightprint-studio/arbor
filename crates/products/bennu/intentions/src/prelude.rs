//! Canonical entry point for `bennu-intentions`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_intentions::prelude::...`.

// The aggregation seam the editor calls — every applicable intention at the caret.
pub use crate::intentions::{intentions_at, Offer};

// The individual transforms (also usable directly / from tests).
pub use crate::log_param::{parameterize_log_call, LogParamRewrite};
pub use crate::np_equals::{np_safe_equals, EqualsSwap};
pub use crate::simplify::{
    simplify_boolean_compare, simplify_negated_comparison, simplify_size_check,
};
pub use crate::Edit;
