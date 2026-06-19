//! `svg_export` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The SVG renderer for the full commit graph moved into
//! [`corvus_git::graph_svg`] alongside the graph DTOs it consumes. This module
//! re-exports its surface so existing `crate::git::svg_export::*` callers (the
//! `export_graph_svg` job in `ipc/corvus/graph.rs`) keep compiling unchanged.
//! It is pure (no Tauri coupling), so there is nothing to inject — the export
//! returns the same `Result<(), String>` the job layer expects.

pub use corvus_git::graph_svg::{generate_svg_to_file, ThemeColors};
