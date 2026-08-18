//! Canonical entry point for `bennu-bevy`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_bevy::prelude::...`.
//! The submodules stay `pub` for rustdoc navigation, but the prelude is the canonical call-site
//! path.

// The extension the host registers — in practice the only thing outside this crate needs.
pub use crate::ext::BevyExtension;

// The model, for anything that wants to ask the questions itself rather than render a catalog.
pub use crate::build::build;
pub use crate::conflict::{warnable, Conflict, Ordering, OrderIndex, Reason, MAX_PAIRWISE};

// What one open buffer gets. Re-exported as the module rather than as two free functions, because
// `gutter` and `diagnostics` are names half the workspace could claim and a glob import of this
// prelude should not be where they collide.
pub use crate::editor;
pub use crate::model::{
    access_keys, Access, AccessKind, BevyModel, Filter, Role, SystemDecl, TypeDecl,
};

// The parameters whose body is not in the scan, and what they stand for.
pub use crate::wrappers::{lookup as wrapper_effect, Effect as WrapperEffect};
