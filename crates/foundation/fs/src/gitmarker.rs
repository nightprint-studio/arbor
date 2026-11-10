//! Cheap "is there a git repository around here?" probe — a few `stat`s, no git library.
//!
//! The File Explorer asks this before borrowing `corvus-be` for badges: outside any repository
//! (and with no repository one level down) there is nothing to colour, so the git backend is
//! never started.

use std::path::Path;

/// Children inspected at most — a folder with thousands of entries must not turn the probe into a scan.
const MAX_CHILDREN: usize = 400;

/// `dir` holds a `.git` entry (a directory, or the file linked worktrees and submodules use).
fn has_git_entry(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// `path` is inside a repository (itself or an ancestor holds `.git`), or is a folder of
/// projects (an immediate child holds `.git`).
pub fn near_git_repo(path: &str) -> bool {
    let start = Path::new(path);
    if start.ancestors().any(has_git_entry) {
        return true;
    }
    let Ok(children) = std::fs::read_dir(start) else {
        return false;
    };
    children
        .flatten()
        .take(MAX_CHILDREN)
        .any(|c| c.file_type().map(|t| t.is_dir()).unwrap_or(false) && has_git_entry(&c.path()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A scratch directory under the OS temp dir, removed on drop.
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("arbor-fs-gitmarker-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Scratch(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_folder_inside_a_repo_is_near() {
        let s = Scratch::new("inside");
        fs::create_dir_all(s.0.join(".git")).unwrap();
        fs::create_dir_all(s.0.join("src/deep")).unwrap();
        assert!(near_git_repo(s.0.join("src/deep").to_str().unwrap()));
    }

    #[test]
    fn a_worktree_git_file_counts() {
        let s = Scratch::new("worktree");
        fs::write(s.0.join(".git"), "gitdir: elsewhere").unwrap();
        assert!(near_git_repo(s.0.to_str().unwrap()));
    }

    #[test]
    fn a_folder_of_projects_is_near() {
        let s = Scratch::new("projects");
        fs::create_dir_all(s.0.join("proj-a/.git")).unwrap();
        fs::create_dir_all(s.0.join("plain")).unwrap();
        assert!(near_git_repo(s.0.to_str().unwrap()));
    }

    #[test]
    fn a_plain_folder_is_not() {
        let s = Scratch::new("plain");
        fs::create_dir_all(s.0.join("a/b")).unwrap();
        // The temp dir itself could sit inside a repo on an odd machine; only assert on a clean tree.
        if !Path::new(s.0.to_str().unwrap()).ancestors().any(has_git_entry) {
            assert!(!near_git_repo(s.0.to_str().unwrap()));
        }
    }
}
