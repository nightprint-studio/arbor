//! Canonical entry point for `bennu-project`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_project::prelude::...`. The submodules stay `pub` for rustdoc navigation,
//! but the prelude is the canonical call-site path.

pub use crate::capability::detect as detect_capabilities;
pub use crate::encoding::{decode as decode_bytes, project_encoding_label};
pub use crate::error::ProjectError;
pub use crate::jdk::detect as detect_jdk;
pub use crate::model::{open_project, read_file, OpenOptions};
pub use crate::pom::{parse as parse_pom, Pom};
pub use crate::tree::build as build_tree;
