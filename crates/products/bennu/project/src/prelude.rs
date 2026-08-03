//! Canonical entry point for `bennu-project`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_project::prelude::...`. The submodules stay `pub` for rustdoc navigation,
//! but the prelude is the canonical call-site path.

pub use crate::capability::detect as detect_capabilities;
pub use crate::cargo::{
    expand_members as expand_cargo_members, parse as parse_cargo_manifest, CargoManifest,
};
pub use crate::encoding::{
    decode as decode_bytes, decode_for_index, encode as encode_text, has_crlf, normalize_newlines,
    project_encoding_label, restore_crlf, source_encoding_label, IndexDecode, UTF8,
};
pub use crate::error::ProjectError;
pub use crate::jdk::detect as detect_jdk;
pub use crate::model::{file_stamp, open_project, read_file, write_file, OpenOptions};
pub use crate::pom::{parse as parse_pom, Pom};
pub use crate::tree::build as build_tree;
