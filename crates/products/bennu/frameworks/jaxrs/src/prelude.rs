//! Canonical entry point for `bennu-jaxrs`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_jaxrs::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the prelude
//! is the canonical call-site path.

// The extension itself — what a host registers — and the codes it reports under.
pub use crate::checks::{
    CODE_DUPLICATE_ROUTE, CODE_GET_WITH_BODY, CODE_NON_PUBLIC_RESOURCE_METHOD,
    CODE_SEVERAL_ENTITY_PARAMS, CODE_UNKNOWN_PATH_PARAM,
};
pub use crate::ext::JaxRsExtension;

// The model a scan produces.
pub use crate::model::{
    AppPrefix, Context as JaxRsContext, Implementor, JaxRsModel, Lit, Locators, Param, PathSpec,
    ResourceMethod, Site, Src, Unit,
};

// Building it, and reading one buffer against it.
pub use crate::extract::{extract as extract_resource_methods, Env as ExtractEnv};
pub use crate::index::{build as build_model, read_buffer, relevant as mentions_jaxrs};

// The application: its path and what it deploys.
pub use crate::prefix::{applications, Applications, Registration};

// Path templates.
pub use crate::paths::{join as join_route, route_key, templates as path_templates, Template};

// Annotation resolution.
pub use crate::known::{CustomVerb, TABLE as JAXRS_ANNOTATIONS, VERBS as JAXRS_VERBS};
