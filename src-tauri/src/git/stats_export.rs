//! `stats_export` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The JSON / HTML report generation moved into [`corvus_git::stats`] (it has no
//! Tauri or git-binary coupling — pure string building over [`RepoStats`]). This
//! module re-exports the entry points so existing `crate::git::stats_export::*`
//! paths keep resolving: the stats broker handler calls [`export_to_file`], and
//! `git_provider::security_export` reuses [`LOGO_SVG`] for its own report header.

pub use corvus_git::stats::{export_to_file, LOGO_SVG};
