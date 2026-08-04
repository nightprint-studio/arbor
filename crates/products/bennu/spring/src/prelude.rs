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

// The configuration vocabulary: what Spring and the project's libraries say their properties
// are. `METADATA_ENTRY` is the jar entry a host looks for; `builtin_index` is the stand-in
// used until it has read any.
pub use crate::metadata::{
    builtin_index, is_metadata_path, HintValue, MetadataIndex, PropertyMeta,
    ADDITIONAL_METADATA_ENTRY, METADATA_ENTRY,
};

// A key rendered as the environment variable that overrides it, in each paste-ready form.
pub use crate::env::{env_var, env_var_name, EnvVar};

// The property-file side of the editor's answers, for the handlers that route to it.
pub use crate::props_intel::{env_var_at, is_property_source};

// Beans declared inside an allowlisted dependency, read from bytecode. Their own tier and
// their own type on purpose — a library bean is a declaration Spring may or may not act on,
// and merging it into the project's model would state it as a fact.
pub use crate::library_beans::{
    beans_of_class, beans_of_classes, LibraryBean, LibraryBeanAllowlist, LibraryBeanGroup,
};
