//! Canonical entry point for `bennu-pomedit`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_pomedit::prelude::...`.

// The mechanics.
pub use crate::edit::{apply, Edit};

// The writes.
pub use crate::write::{add_module, packaging_of, set_packaging, set_property};

// The Surefire selector — what a pom pins, and the rewrite that unpins it.
pub use crate::surefire::{
    convert_pinned_suite, forkcount_zero_in, surefire_test_in, SuiteConversion, SurefireTest,
};

// Generating a new module.
pub use crate::scaffold::{
    module_pom, relative_path_for, source_dirs, valid_artifact_id, ModuleSpec, ParentCoords,
};
