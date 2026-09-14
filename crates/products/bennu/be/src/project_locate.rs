//! Where a project is, when the question is asked from a path rather than from an open project.
//!
//! Two pure filesystem answers the project-health surface needs: which directory a stray file's
//! project should be opened at, and where a folder picker should start when a project's root has
//! vanished. Kept apart from the handlers (`project_health`) so they are testable without a slot.

use std::path::{Path, PathBuf};

/// The directory a project for `file` should be opened at: the outermost of the contiguous chain
/// of ancestors holding a `pom.xml`.
///
/// Outermost rather than nearest because a file sits in a module, and opening the module on its
/// own loses every sibling it resolves against — the reactor root is the project. The chain stops
/// at the first ancestor without a pom, so an unrelated pom higher up the disk is never reached.
pub fn enclosing_maven_root(file: &Path) -> Option<PathBuf> {
    let mut found: Option<PathBuf> = None;
    for dir in file.ancestors().skip(1) {
        if dir.join("pom.xml").is_file() {
            found = Some(dir.to_path_buf());
        } else if found.is_some() {
            break;
        }
    }
    found
}

/// The nearest ancestor of `path` (itself included) that is an existing directory — where a
/// folder picker should start when `path` has vanished.
pub fn nearest_existing_dir(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|p| !p.as_os_str().is_empty() && p.is_dir()).map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory unique to one test, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("bennu-locate-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn pom(&self, rel: &str) {
            let p = self.0.join(rel);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("pom.xml"), "<project></project>").unwrap();
        }

        fn file(&self, rel: &str) -> PathBuf {
            let f = self.0.join(rel);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(&f, "class A {}").unwrap();
            f
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A file inside a module of an unopened reactor proposes the reactor root, not the module.
    #[test]
    fn the_enclosing_project_is_the_outermost_pom_of_the_chain() {
        let s = Scratch::new("enclosing");
        s.pom("");
        s.pom("core");
        let file = s.file("core/src/main/java/A.java");
        assert_eq!(enclosing_maven_root(&file), Some(s.0.clone()));
    }

    #[test]
    fn a_file_under_no_pom_has_no_enclosing_project() {
        let s = Scratch::new("nowhere");
        let file = s.file("loose/A.java");
        assert_eq!(enclosing_maven_root(&file), None);
    }

    #[test]
    fn a_vanished_path_starts_the_picker_at_its_nearest_surviving_parent() {
        let s = Scratch::new("vanished");
        let gone = s.0.join("renamed-away").join("deeper");
        assert_eq!(nearest_existing_dir(&gone), Some(s.0.clone()));
        assert_eq!(nearest_existing_dir(&s.0), Some(s.0.clone()));
    }
}
