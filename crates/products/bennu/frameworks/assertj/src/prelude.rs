//! Canonical entry point for `bennu-assertj`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_assertj::prelude::...`.

// The extension itself — what a host registers — and the codes and intention ids it answers with.
pub use crate::ext::{
    AssertJExtension, CODE_ASSERTS_NOTHING, CODE_DEDICATED, CODE_SOFT_NEVER_ASSERTED,
    INTENTION_ADD_ASSERT_ALL, INTENTION_DEDICATED, INTENTION_FROM_JUNIT, INTENTION_FROM_JUNIT_FILE,
};
