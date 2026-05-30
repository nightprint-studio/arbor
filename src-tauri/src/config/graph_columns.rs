//! Commit-graph column layout — order, widths, visibility.
//!
//! Persisted in its own TOML file (`~/.config/arbor/graph_columns.toml`)
//! rather than the general `config.toml`, because the column layout is a
//! self-contained UI concern with no overlap with the rest of `AppConfig`.
//! Keeping it standalone also lets the file be reset or hand-edited
//! without risking the main settings.
//!
//! The host-wide TOML defines a single ordered `columns` list. Each entry
//! carries an `id` (one of `graph`, `refs`, `subject`, `author`, `date`,
//! `hash`), a width in pixels, and a `visible` flag. The `graph` column —
//! the SVG lane renderer — is a regular entry and can be reordered like
//! every other column.
//!
//! Width semantics depend on the id (mirrored in the frontend renderer):
//!   * `graph`   — the value is treated as the *maximum* width the SVG
//!                 track is allowed to occupy. The track auto-sizes to
//!                 `svgW + 12` (the natural width of the lane diagram plus
//!                 a small gutter) and caps at this value. When the lane
//!                 count would make the natural width exceed the cap, the
//!                 cap softens so no lanes get clipped.
//!   * `subject` — the value is the *minimum*; the frontend renders the
//!                 track as `minmax(width, 1fr)`, so the column flex-grows
//!                 past the minimum to absorb whatever space is left.
//!   * everything else — fixed track width.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphColumnsConfig {
    /// Ordered column list. Index 0 is the leftmost column; the last entry
    /// is the rightmost. Includes the special `graph` column (the SVG lane
    /// renderer) alongside the text columns — `graph` can be reordered
    /// like any other entry. See [`GraphColumn`] for the per-column fields.
    #[serde(default = "default_columns")]
    pub columns: Vec<GraphColumn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphColumn {
    /// Stable id. Known values: `graph`, `refs`, `subject`, `author`,
    /// `date`, `hash`. `graph` renders the SVG lane diagram; everything
    /// else is a text cell.
    pub id: String,
    /// Track width in px.
    ///
    /// Semantics vary by column id:
    /// * `graph`   — *maximum* width the SVG track is allowed to occupy.
    ///   The frontend auto-sizes the track to `svgW + 12` and caps at
    ///   this value; when the natural lane count would exceed the cap,
    ///   the cap softens so no lanes get clipped.
    /// * `subject` — *minimum* width; the column flex-grows past it via
    ///   `minmax(width, 1fr)` to absorb whatever space is left.
    /// * everything else — fixed track width.
    pub width: u32,
    #[serde(default = "default_true")]
    pub visible: bool,
}

fn default_true() -> bool { true }

fn default_columns() -> Vec<GraphColumn> {
    vec![
        GraphColumn { id: "graph".into(),   width: 480, visible: true },
        GraphColumn { id: "refs".into(),    width: 220, visible: true },
        GraphColumn { id: "subject".into(), width: 280, visible: true },
        GraphColumn { id: "author".into(),  width: 160, visible: true },
        GraphColumn { id: "date".into(),    width: 150, visible: true },
        GraphColumn { id: "hash".into(),    width:  80, visible: true },
    ]
}

impl Default for GraphColumnsConfig {
    fn default() -> Self {
        Self { columns: default_columns() }
    }
}

pub fn config_path() -> PathBuf {
    arbor_core::prelude::arbor_config_path("graph_columns.toml")
}

pub fn load() -> GraphColumnsConfig {
    let path = config_path();
    if !path.exists() {
        return GraphColumnsConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => GraphColumnsConfig::default(),
    }
}

pub fn save(config: &GraphColumnsConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}
