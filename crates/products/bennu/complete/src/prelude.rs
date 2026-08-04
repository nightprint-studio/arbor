//! Canonical entry point for `bennu-complete`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_complete::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

// Where the caret is, in bytes: the line, the indentation, the token it would replace.
pub use crate::caret::{
    indent_of, line_at, line_end, line_number, line_prefix, line_start, safe_offset, token_before,
    within,
};

// Collecting candidates. `Proposals` is the one a provider holds; `Proposal` is what it offers.
pub use crate::collect::{Proposal, Proposals, DEFAULT_CAP};

// Which candidates a prefix admits, and the ghost-text rule.
pub use crate::prefix::{common_prefix, continuation, matches, matches_ignore_case, unique_continuation};
