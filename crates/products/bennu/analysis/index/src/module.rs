//! Which **module** a source file belongs to, and the alias keys that let two modules declare the
//! same type.
//!
//! The index is one store for a whole reactor, keyed by binary name — and a binary name is not unique
//! across modules: a legacy overlay, a vendored copy or a test fixture can declare `p/Foo` in two of
//! them, each its own class. The fst keeps the first record under a duplicate key, so every file in
//! the reactor read *that* `Foo`, whichever module it was in.
//!
//! The fix is deliberately not a second index. A type whose binary name is declared in **more than
//! one module** gets one extra key per declaring module — `<MARK><module><MARK><binary>` — pointing
//! at that module's own record, written by [`IndexBuilder::persist`](crate::builder::IndexBuilder).
//! The plain key stays exactly as it was (first wins), so everything that does not know about
//! modules reads what it always read, and a project with no duplicates has no alias at all.
//!
//! The marker leads the key so the aliases sort together at the front of the fst and never fall
//! inside a package prefix scan (`p/` finds `p/Foo`, not `<MARK>…<MARK>p/Foo`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Leads and separates the parts of an alias key. A control character: no module path and no
/// binary name contains it.
pub const ALIAS_MARK: char = '\u{1}';

/// The fst key under which `module`'s own copy of `binary` is filed. See the module doc.
pub fn module_alias_key(module: &str, binary: &str) -> String {
    format!("{ALIAS_MARK}{module}{ALIAS_MARK}{binary}")
}

/// A directory, written the one way index time and validation time can both reach: forward
/// slashes, and case-folded on Windows where `C:\proj` and `c:\proj` are the same place.
fn module_id(dir: &Path) -> String {
    let s = dir.to_string_lossy().replace('\\', "/");
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// The module `file` belongs to: its nearest ancestor directory holding a `pom.xml`, as a
/// [`module_id`]. `None` for a file under no pom — it then simply has no module of its own.
///
/// One `stat` per directory on the way up; use a [`ModuleCache`] when asking for many files.
pub fn module_of(file: &Path) -> Option<String> {
    file.parent()?
        .ancestors()
        .find(|dir| dir.join("pom.xml").is_file())
        .map(module_id)
}

/// [`module_of`] for a whole project's files: remembers each directory's answer, so a few thousand
/// sources cost a few hundred `stat` walks rather than one per file.
#[derive(Default)]
pub struct ModuleCache {
    dirs: Mutex<HashMap<PathBuf, Option<String>>>,
}

impl ModuleCache {
    /// The module `file` belongs to — same answer as [`module_of`].
    pub fn module_of(&self, file: &Path) -> Option<String> {
        let start = file.parent()?;
        let mut dirs = self.dirs.lock().unwrap_or_else(|p| p.into_inner());
        let mut walked: Vec<&Path> = Vec::new();
        let mut answer: Option<String> = None;
        for dir in start.ancestors() {
            if let Some(hit) = dirs.get(dir) {
                answer = hit.clone();
                break;
            }
            walked.push(dir);
            if dir.join("pom.xml").is_file() {
                answer = Some(module_id(dir));
                break;
            }
        }
        // Every directory walked past resolves to the same module as the one that stopped the walk —
        // a directory is in the module of its nearest pom-bearing ancestor, itself included.
        for dir in walked {
            dirs.insert(dir.to_path_buf(), answer.clone());
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-idx-module-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_file_belongs_to_its_nearest_pom() {
        let root = tree("nearest");
        let api = root.join("api");
        let src = api.join("src/main/java/p");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(root.join("pom.xml"), "<project/>").unwrap();
        std::fs::write(api.join("pom.xml"), "<project/>").unwrap();

        assert_eq!(module_of(&src.join("A.java")), Some(module_id(&api)));
        assert_eq!(module_of(&root.join("Top.java")), Some(module_id(&root)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_cache_agrees_with_the_walk_and_a_file_under_no_pom_has_none() {
        let root = tree("cache");
        let a = root.join("a/src/p");
        let b = root.join("b/src/p");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(root.join("a/pom.xml"), "<project/>").unwrap();

        let cache = ModuleCache::default();
        // Twice: the second answer comes from the directories the first one filled in.
        for _ in 0..2 {
            assert_eq!(cache.module_of(&a.join("X.java")), module_of(&a.join("X.java")));
            assert_eq!(cache.module_of(&a.join("X.java")), Some(module_id(&root.join("a"))));
        }
        // `b` has no pom — and neither has anything above it in this temp tree.
        assert_eq!(cache.module_of(&b.join("Y.java")), module_of(&b.join("Y.java")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_alias_key_never_sits_inside_a_package_prefix() {
        let key = module_alias_key("/w/api", "p/Foo");
        assert!(!key.starts_with("p/"));
        assert!(key.starts_with(ALIAS_MARK));
        assert!(key.ends_with("p/Foo"));
    }
}
