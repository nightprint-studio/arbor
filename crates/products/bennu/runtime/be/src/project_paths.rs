//! Resolving a tool's `file` argument against its `root` — the one place that decides what a
//! relative path means.
//!
//! The MCP policy layer already refuses an *absolute* path outside the projects the user has
//! opened (`src-tauri/src/mcp/policy.rs`, `candidate_paths` + `is_within`). It deliberately lets
//! relative strings through, on the reasoning that a tool resolves them against a root that was
//! itself checked. That only holds if the tool really does resolve them against the root — and a
//! bare `fs::read(file)` resolves against the backend process's working directory instead, which is
//! both wrong (a relative path that should work does not) and an escape (`..\..\x` reads whatever
//! it names). So every handler that takes `root` + `file` goes through here first.
//!
//! An absolute `file` is returned untouched: the editor legitimately opens files outside the
//! project (dependency sources, another workspace member), and the MCP layer has already judged it.

use std::path::{Component, Path, PathBuf};

/// `file` made absolute against `root`. A relative `file` may not climb out with `..`.
pub(crate) fn in_root(root: &str, file: &str) -> Result<String, String> {
    if file.is_empty() {
        return Err("no file given".to_string());
    }
    let path = Path::new(file);
    if path.is_absolute() {
        return Ok(file.to_string());
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("`{file}` climbs out of the project with `..` — give a path inside {root}"));
    }
    if root.is_empty() {
        return Err(format!("`{file}` is relative and no project root was given to resolve it against"));
    }
    Ok(PathBuf::from(root).join(path).to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absolute_path_is_left_to_the_policy_layer() {
        let abs = if cfg!(windows) { "C:/p/A.java" } else { "/p/A.java" };
        assert_eq!(in_root("/other", abs).unwrap(), abs);
    }

    #[test]
    fn a_relative_path_resolves_against_the_root() {
        assert_eq!(in_root("/p", "src/A.java").unwrap(), "/p/src/A.java");
    }

    #[test]
    fn a_relative_path_cannot_climb_out() {
        assert!(in_root("/p", "../x").is_err());
        assert!(in_root("/p", "src/../../x").is_err());
    }

    #[test]
    fn nothing_is_not_a_file() {
        assert!(in_root("/p", "").is_err());
    }
}
