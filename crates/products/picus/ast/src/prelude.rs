//! Canonical entry point for `picus-ast`'s public API.
//!
//! Workspace convention: call sites (`picus-emit`, `picus-be`, later
//! `picus-rewrite`) reach this crate through `picus_ast::prelude::...`. The shared
//! vocabulary from `picus-types` is re-exported here so a script-side call site
//! imports one prelude rather than two.

pub use picus_types::prelude::{Column, EngineKind};

pub use crate::dml::{DmlModel, DmlOperation, DmlRow, VersionTableConfig};
pub use crate::target::{FolderRole, Target, TargetGuards, TargetWrap, VersionGuard};
