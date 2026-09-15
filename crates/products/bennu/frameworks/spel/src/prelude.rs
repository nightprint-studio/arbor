//! Canonical entry point for `bennu-spel`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_spel::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

// Property placeholders — `${key:default}`, nestable.
pub use crate::placeholder::{
    issues as placeholder_issues, placeholder_at, placeholders, Placeholder, PlaceholderIssue,
};

// SpEL — `#{ … }`, tokenized into spans + bean/variable references + factual issues.
pub use crate::spel::{
    bean_ref_at, expression_at, expressions, issues as spel_issues, SpelExpr, SpelIssue, SpelRef,
    SpelToken, TokenKind,
};
