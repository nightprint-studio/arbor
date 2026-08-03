//! Canonical entry point for `bennu-spring`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_spring::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.
//!
//! In practice a host needs exactly one of these — [`SpringExtension`] — and reaches
//! everything else through the [`FrameworkExtension`] trait. The model types are exported
//! for the backend handlers that shape them into wire payloads.
//!
//! [`FrameworkExtension`]: bennu_ext::prelude::FrameworkExtension

// The extension itself — what a host registers.
pub use crate::ext::SpringExtension;

// The model a query answers from.
pub use crate::model::{
    canonical_key_segment, default_bean_name, join_paths, line_at, path_variables, simple_name,
    strip_generics, BeanDef, BeanKind, ConfigBinding, Endpoint, EndpointParam, InjectionKind,
    InjectionPoint, SpringModel, TypeInfo,
};

// `@ConfigurationProperties` → the full key each bound field binds.
pub use crate::config_props::bindings as config_bindings;

// The reverse index: which Java / XML sites read a configuration key.
pub use crate::model::{BeanCondition, PropertyUsage};
pub use crate::usages::{canonical_key, property_usages};

// Property sources — the configuration side, including the pick-your-file behaviour.
pub use crate::props::{
    is_property_file, parse_property_file, profile_of, PropertyEntry, PropertyFile, PropertyFormat,
    PropertySources,
};

// Bean derivation (the unit a scan produces, and the three collectors).
pub use crate::beans::{
    annotation_beans, injection_points, resolve_type, type_index, xml_beans, JavaUnit,
};

// Request mappings.
pub use crate::endpoints::endpoints;

// Annotation-origin resolution — whether a `@Service` is Spring's or the project's own.
pub use crate::known::{find as find_annotation, has as has_annotation, is as is_annotation, is_any as is_any_annotation};

// The Java scan this crate runs for itself.
pub use crate::scan::{looks_spring_relevant, scan_java, AnnFacts, AnnString, JavaFacts, TypeFacts};

// Bean XML parsing + positional queries.
pub use crate::xml::{
    attribute_at, is_spring_bean_xml, parse_bean_xml, XmlAttrHit, XmlBean, XmlBeanFile, XmlBeanRef,
    XmlProperty,
};

// Expression colouring, shared by both file kinds.
pub use crate::highlight::{expression_highlights, path_var_highlights};
