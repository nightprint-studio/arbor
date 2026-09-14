//! Canonical entry point for `bennu-mockito`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_mockito::prelude::...`.

// The extension itself — what a host registers — with the diagnostic codes and intention ids it
// emits, for a caller that keys on them.
pub use crate::ext::{
    MockitoExtension, CODE_MIXED_MATCHERS, CODE_NOT_INITIALISED, CODE_UNFINISHED_STUBBING,
    CODE_UNFINISHED_VERIFICATION, INTENTION_ADD_EXTENSION, INTENTION_WRAP_IN_EQ,
};
