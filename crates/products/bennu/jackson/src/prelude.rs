//! Canonical entry point for `bennu-jackson`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_jackson::prelude::...`.

// The extension itself — what a host registers.
pub use crate::ext::{
    JacksonExtension, CODE_CONTRADICTORY, CODE_DUPLICATE, CODE_UNNAMED_CREATOR,
    CODE_UNREACHABLE,
};

// A class read as the JSON it produces.
pub use crate::dto::{dtos_in, Dto, MemberKind, Property};

// A `@JsonCreator` whose arguments Jackson cannot name — the defect that lives in two files.
pub use crate::creator::{
    creators_in, keeps_parameter_names, parameter_names, ParameterNames, UnnamedCreator,
};
