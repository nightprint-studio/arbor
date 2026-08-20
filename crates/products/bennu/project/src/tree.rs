//! The project file tree (docs §5 #16).
//!
//! Builds a [`TreeNode`] rooted at a directory. Lazy-friendly: [`build`] takes a
//! `max_depth` so a large legacy tree (1200+ files — docs §8) isn't materialised in
//! one shot; the FE fetches deeper levels on expand. Noise dirs (`target`, `.git`,
//! `node_modules`) are skipped. Entries are sorted dirs-first then by name, the
//! conventional tree order.
//!
//! Every node also carries **what it is to the project**: hidden by convention, and
//! ignored by git (see [`crate::ignored`]). Both are marks, not filters — the tree
//! greys them out and keeps showing them, because a stale ignored artifact you cannot
//! see is a stale artifact you cannot explain.

use std::path::Path;

use bennu_proto::prelude::TreeNode;

use crate::error::ProjectError;
use crate::ignored::IgnoreStack;

/// Directories never walked. Not a "hidden" list: these are build output and machinery
/// with five-figure file counts, and materialising them would cost more than the rest
/// of the tree put together. Everything git merely *ignores* is walked and marked.
const SKIP_DIRS: [&str; 3] = ["target", ".git", "node_modules"];

/// Build the tree rooted at `root`, descending at most `max_depth` levels. A
/// directory at the depth limit is returned with empty `children` (the FE re-requests
/// it). `root` must be a directory.
pub fn build(root: &Path, max_depth: usize) -> Result<TreeNode, ProjectError> {
    if !root.is_dir() {
        return Err(ProjectError::NotADirectory(root.display().to_string()));
    }
    // Seeded from the repository root, so a lazy expansion deep inside the tree still
    // honours the `.gitignore` at the top — where the rules that matter live.
    let mut ignores = IgnoreStack::at(root);
    let root_ignored = ignores.is_ignored(root, true);
    Ok(build_node(root, max_depth, &mut ignores, root_ignored))
}

/// `inherited` is whether an ancestor is already ignored. Git has no way to un-ignore a
/// file under an ignored directory (it never descends into one), so this short-circuits
/// the match instead of asking again at every level.
fn build_node(
    path: &Path,
    depth_left: usize,
    ignores: &mut IgnoreStack,
    inherited: bool,
) -> TreeNode {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let is_dir = path.is_dir();
    let mut children = Vec::new();

    if is_dir && depth_left > 0 {
        let pushed = ignores.enter(path);
        if let Ok(entries) = std::fs::read_dir(path) {
            let mut kids: Vec<_> = entries
                .flatten()
                .filter(|e| !SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
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
                let kid_path = kid.path();
                let kid_ignored =
                    inherited || ignores.is_ignored(&kid_path, kid_path.is_dir());
                children.push(build_node(&kid_path, depth_left - 1, ignores, kid_ignored));
            }
        }
        ignores.pop(pushed);
    }

    TreeNode {
        hidden: is_hidden(path, &name),
        ignored: inherited,
        name,
        path: path.display().to_string(),
        is_dir,
        children,
    }
}

/// Whether the platform considers the entry hidden.
///
/// A leading dot everywhere, plus the real attribute on Windows — where `.env` is a
/// perfectly visible file and `Thumbs.db` is not, and the name says neither.
fn is_hidden(path: &Path, name: &str) -> bool {
    if name.starts_with('.') && name != "." && name != ".." {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if let Ok(md) = path.metadata() {
            return md.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0;
        }
    }
    #[cfg(not(windows))]
    let _ = path;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!("bennu-tree-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn write(&self, rel: &str, body: &str) {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        fn dir(&self, rel: &str) {
            std::fs::create_dir_all(self.0.join(rel)).unwrap();
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn find<'a>(node: &'a TreeNode, name: &str) -> Option<&'a TreeNode> {
        if node.name == name {
            return Some(node);
        }
        node.children.iter().find_map(|c| find(c, name))
    }

    #[test]
    fn marks_ignored_entries_without_hiding_them() {
        let t = TempDir::new("mark");
        t.dir(".git");
        t.write(".gitignore", "*.class\ndist/\n");
        t.write("Main.java", "");
        t.write("Main.class", "");
        t.write("dist/app.jar", "");

        let tree = build(&t.0, 8).unwrap();
        assert!(!find(&tree, "Main.java").unwrap().ignored);
        assert!(find(&tree, "Main.class").unwrap().ignored, "gitignored file is marked");
        assert!(find(&tree, "dist").unwrap().ignored, "gitignored dir is marked");
        assert!(
            find(&tree, "app.jar").unwrap().ignored,
            "an entry under an ignored dir inherits the mark — git never descends to \
             re-decide"
        );
    }

    #[test]
    fn marks_dotfiles_hidden() {
        let t = TempDir::new("hidden");
        t.write(".editorconfig", "");
        t.write("pom.xml", "");
        let tree = build(&t.0, 4).unwrap();
        assert!(find(&tree, ".editorconfig").unwrap().hidden);
        assert!(!find(&tree, "pom.xml").unwrap().hidden);
    }

    #[test]
    fn skip_dirs_are_not_walked() {
        let t = TempDir::new("skip");
        t.write("target/classes/A.class", "");
        t.write("src/A.java", "");
        let tree = build(&t.0, 8).unwrap();
        assert!(find(&tree, "target").is_none());
        assert!(find(&tree, "A.java").is_some());
    }
}
