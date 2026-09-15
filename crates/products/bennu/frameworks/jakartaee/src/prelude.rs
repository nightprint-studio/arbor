//! Canonical entry point for `bennu-jakartaee`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_jakartaee::prelude::...`.

// The extension itself — what a host registers — and the codes it raises.
pub use crate::checks::{
    CODE_AMBIGUOUS, CODE_DUPLICATE_URL, CODE_FINAL_FIELD, CODE_INVALID_EJB, CODE_INVALID_URL,
    CODE_NOT_A_SERVLET, CODE_STATIC_FIELD, CODE_UNPROXYABLE, CODE_UNSATISFIED,
};
pub use crate::ext::JakartaEeExtension;

// The project model and its parts.
pub use crate::archive::{discovery_mode, ArchiveMode};
pub use crate::beans::{Bean, BeanOrigin};
pub use crate::inject::{InjectKind, InjectionPoint};
pub use crate::matching::{Fit, Resolution};
pub use crate::model::Model;
pub use crate::qualifiers::{CustomQualifier, Qualifiers};
pub use crate::types::TypeRef;

// Servlets and filters.
pub use crate::web::{parse_web_xml, pattern_problem, UrlPattern, WebComponent, WebKind};
