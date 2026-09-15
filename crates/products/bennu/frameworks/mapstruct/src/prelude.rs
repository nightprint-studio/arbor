//! Canonical entry point for `bennu-mapstruct`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_mapstruct::prelude::...`.

// The extension itself — what a host registers.
pub use crate::ext::MapStructExtension;

// The diagnostics and their codes.
pub use crate::checks::{
    findings, unmapped, Finding, CODE_CONFLICTING, CODE_DUPLICATE_TARGET, CODE_UNKNOWN_SOURCE,
    CODE_UNKNOWN_TARGET, CODE_UNMAPPED,
};

// The Alt+Enter fixes and their ids.
pub use crate::fixes::{edit_distance, intentions as mapstruct_intentions, INTENT_DID_YOU_MEAN, INTENT_IGNORE_UNMAPPED};

// Completion, go-to and hover inside a mapping path.
pub use crate::editor::{
    completions as mapping_path_completions, hover as mapping_path_hover, navigate as mapping_path_navigate,
};

// Mappers, analysed.
pub use crate::mapper::{
    mappers_in, simple_type, Mapper, MappingMethod, MethodKind, Policy, SourceParam, ANNOTATIONS,
};

// The mapping annotations as read off the tree.
pub use crate::annotations::{method_node, read_method, Elem, Lit, MappingAnn, MethodAnns};

// The project model.
pub use crate::model::{build as build_model, ConfigInfo, Impl, ImplMethod, Knowledge, Model};

// Mapping paths.
pub use crate::paths::{segments, source_root, type_at, walk, Segment, Side, SourceRoot, Walk};

// Properties and the type table.
pub use crate::properties::{decapitalize, type_info, Access, Origin, Property, Site};
pub use crate::table::{Scope, TypeInfo, TypeRef, TypeTable, TypeView};

// The Mappers panel.
pub use crate::catalog::{line_of, rows as catalog_rows};
