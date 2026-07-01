//! Canonical entry point for `arbor-studio-core`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_studio_core::prelude::...`. The generic engine submodules
//! (`history`, `diff`, …) are filled in Stage 2; until then this
//! re-exports the trait plus, for convenience, the studio DTO prelude
//! so a backend impl can `use arbor_studio_core::prelude::*;` for both
//! the trait and the shapes it returns.

pub use crate::backend::StudioFormatBackend;

// Generic engine modules — call sites reach them through the prelude
// (e.g. `arbor_studio_core::prelude::edit_expr::compile`).
pub use crate::diff;
pub use crate::edit_expr;
pub use crate::history;
pub use crate::history::History;
pub use crate::persist;
pub use crate::query;
pub use crate::refactor;
// Refactor seam types backends implement / construct directly.
pub use crate::refactor::{
    BulkOp, CoerceOutcome, CoerceSkip, DefScopeStyle, OpenDocState, RefactorOps, RenameDefInput,
    RenameUsageInput, SetValue,
};

// Stage 3 — generic scaffolding for the simple formats (TOML/YAML/.properties).
pub use crate::default_backend::{DefaultBackend, SchemaRouting};
pub use crate::schema::SchemaProvider;
pub use crate::simple::{ParseOutcome, SimpleFormat, SimpleMutation};

// Re-export the DTO surface so backends get the trait + its data shapes
// from one glob import.
pub use arbor_studio_types::prelude::*;
