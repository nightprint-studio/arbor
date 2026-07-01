//! Canonical entry point for `arbor-studio-yaml`'s public API.
//!
//! Call sites reach this crate through `arbor_studio_yaml::prelude::...`.

pub use crate::{backend, backend_with_schema, parse_to_value};
pub use crate::refactor_ops::YamlRefactor;
pub use crate::simple::YamlFormat;

// Re-export the schema-routing knob the caller constructs the backend with,
// so a consumer can `use arbor_studio_yaml::prelude::*;` for both the
// factory and its argument type.
pub use arbor_studio_core::prelude::SchemaRouting;
