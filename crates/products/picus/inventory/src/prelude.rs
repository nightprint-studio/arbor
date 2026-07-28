//! Canonical entry point for `picus-inventory`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `picus_inventory::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the diff always goes through here.

pub use crate::build::Inventory;
pub use crate::entry::{ObjectEntry, ObjectSite};
pub use crate::input::{coverage_key, ParsedProject, ParsedScript, Placement};
pub use crate::kind::InventoryKind;
pub use crate::wire::InventoryObject;
