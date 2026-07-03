//! Canonical entry point for `bennu-project`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_project::prelude::...`. The submodules stay `pub` for rustdoc navigation,
//! but the prelude is the canonical call-site path.

pub use crate::capability::detect as detect_capabilities;
pub use crate::encoding::{
    decode as decode_bytes, decode_for_index, encode as encode_text, project_encoding_label,
    source_encoding_label, IndexDecode,
};
pub use crate::error::ProjectError;
pub use crate::jdk::detect as detect_jdk;
pub use crate::model::{open_project, read_file, write_file, OpenOptions};
pub use crate::pom::{parse as parse_pom, Pom};
pub use crate::tree::build as build_tree;
