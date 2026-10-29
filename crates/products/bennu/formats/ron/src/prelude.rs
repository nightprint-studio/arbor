//! Canonical entry point for `bennu-ron`'s public API.

pub use crate::directive::{directive_span, schema_hint};
pub use crate::scan::{scan, Field, Head, Kind, Node, Scalar};
pub use crate::syntax::{bracket_diagnostic, syntax_diagnostics};
pub use crate::validate::validate;
pub use crate::{RonDiagnostic, RonSeverity};

// The schema side, re-exported so a consumer needs one crate name to load and validate.
pub use arbor_studio_ron::prelude::SchemaHint;
pub use arbor_studio_ron::schema::load as load_schema;
pub use arbor_studio_types::prelude::Schema;
