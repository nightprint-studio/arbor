//! Canonical entry point for `bennu-jakarta`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_jakarta::prelude::...`. In practice a host needs [`ValidationExtension`] and nothing
//! else — everything below it is reached through the [`FrameworkExtension`] trait.
//!
//! [`FrameworkExtension`]: bennu_ext::prelude::FrameworkExtension

// The extension itself — what a host registers.
pub use crate::ext::{
    known_constraints, origin_label, ValidationExtension, CODE_MISSING_BUNDLE, CODE_POINTLESS,
    CODE_UNKNOWN_KEY, CODE_WRONG_TYPE,
};

// Which bundles the validator reads, and what named them.
pub use crate::bundles::{discover, Discovery, Origin, Site, ValidationBundle, SPEC_BASE};

// The vocabulary.
pub use crate::constraints::{
    constraint, is_constraint, is_provider_key, provider_keys, verdict, Accepts, Constraint,
    Verdict, CONSTRAINTS,
};

// Reading a source.
pub use crate::refs::{
    at_offset, constraints_in, key_prefix_at, keys_in_message, ConstraintUse, KeyRef,
    MessageLiteral,
};
