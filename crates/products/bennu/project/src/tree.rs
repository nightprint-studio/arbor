//! The project file tree (docs §5 #16).
//!
//! Builds a [`TreeNode`] rooted at a directory. Lazy-friendly: [`build`] takes a
//! `max_depth` so a large legacy tree (1200+ files — docs §8) isn't materialised in
//! one shot; the FE fetches deeper levels on expand. Noise dirs (`target`, `.git`,
//! `node_modules`) are skipped. Entries are sorted dirs-first then by name, the
//! conventional tree order.

use std::path::Path;

use bennu_proto::prelude::TreeNode;

use crate::error::ProjectError;

/// Build the tree rooted at `root`, descending at most `max_depth` levels. A
/// directory at the depth limit is returned with empty `children` (the FE re-requests
/// it). `root` must be a directory.
pub fn build(root: &Path, max_depth: usize) -> Result<TreeNode, ProjectError> {
    if !root.is_dir() {
        return Err(ProjectError::NotADirectory(root.display().to_string()));
    }
    Ok(build_node(root, max_depth))
}

fn build_node(path: &Path, depth_left: usize) -> TreeNode {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let is_dir = path.is_dir();
    let mut children = Vec::new();

    if is_dir && depth_left > 0 {
        if let Ok(entries) = std::fs::read_dir(path) {
            let mut kids: Vec<_> = entries
                .flatten()
                .filter(|e| {
                    let n = e.file_name();
                    let n = n.to_string_lossy();
                    n != "target" && n != ".git" && n != "node_modules"
                })
                .collect();
            // Dirs first, then alphabetical (case-insensitive).
            kids.sort_by(|a, b| {
                let ad = a.path().is_dir();
                let bd = b.path().is_dir();
                bd.cmp(&ad).then_with(|| {
                    a.file_name()
                        .to_string_lossy()
                        .to_lowercase()
                        .cmp(&b.file_name().to_string_lossy().to_lowercase())
                })
            });
            for kid in kids {
                children.push(build_node(&kid.path(), depth_left - 1));
            }
        }
    }

    TreeNode { name, path: path.display().to_string(), is_dir, children }
}
