//! Canonical entry point for `bennu-generate`'s public API.

pub use crate::code::{Imports, JavaFile, Lines};
pub use crate::context::{
    ClassInfo, EndpointInfo, EndpointParam, FieldInfo, GenContext, ProjectInfo, Version,
};
pub use crate::generators::http_client::type_names;
pub use crate::layout::{path_of, source_roots, SourceRoots};
pub use crate::names::{camel, kebab, pascal, snake};
pub use crate::output::{Extra, GenFile, MemberTarget, Members, Output, SourceRoot, WireTarget, Wiring};
pub use crate::wire::{annotation_insert, Insert};
pub use crate::registry::{all, find, run, Generator, GeneratorMeta, Target};
