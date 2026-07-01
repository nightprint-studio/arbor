//! Canonical entry point for `arbor-studio-toml`'s public API.
//!
//! Call sites reach this crate through `arbor_studio_toml::prelude::...`.

pub use crate::{backend, backend_with_schema, parse_to_value};
pub use crate::simple::TomlFormat;
pub use crate::refactor_ops::TomlRefactor;

// Re-export the schema-routing knob the caller constructs the backend with,
// so a consumer can `use arbor_studio_toml::prelude::*;` for both the factory
// and its argument type.
pub use arbor_studio_core::prelude::SchemaRouting;
