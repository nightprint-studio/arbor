//! The jars a person asks to have in the server's `lib/`.
//!
//! The list in the configuration holds jars and folders. A folder stands for the `.jar` files directly
//! in it — not below — so pointing at a drivers directory is one entry, and a jar added to it later is
//! there on the next launch. What is here only decides *which files*; copying them is `prepare_base`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(super) struct LibSet {
    pub jars: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

/// The jars `entries` name, in order, with a sentence for each entry that names nothing usable.
/// `resolve` turns an entry into a path (absolute, or relative to the project root).
pub(super) fn expand(entries: &[String], resolve: impl Fn(&str) -> PathBuf) -> LibSet {
    let mut set = LibSet { jars: Vec::new(), warnings: Vec::new() };
    for entry in entries.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        let path = resolve(entry);
        if path.is_dir() {
            let found = jars_in(&path);
            if found.is_empty() {
                set.warnings.push(format!("The server library folder has no .jar files: {}\n", path.display()));
            }
            for jar in found {
                add(&mut set.jars, jar);
            }
        } else if path.is_file() && is_jar(&path) {
            add(&mut set.jars, path);
        } else if path.is_file() {
            set.warnings.push(format!("A server library must be a .jar or a folder of them: {}\n", path.display()));
        } else {
            set.warnings.push(format!("A server library is not there: {}\n", path.display()));
        }
    }
    set
}

/// Add `jar` unless a jar of the same file name is already in — the server's `lib/` is one flat
/// folder, so a second file of that name would only overwrite the first.
pub(super) fn add(jars: &mut Vec<PathBuf>, jar: PathBuf) {
    if !jars.iter().any(|j| j.file_name() == jar.file_name()) {
        jars.push(jar);
    }
}

/// One sentence per library that is going in twice under two versions. Tomcat loads both from `lib/`
/// and the one that wins is whichever the class loader meets first — a bug that reads as random.
pub(super) fn conflicts(jars: &[PathBuf]) -> Vec<String> {
    let mut by_artifact: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for name in jars.iter().filter_map(|j| j.file_name().and_then(|n| n.to_str())) {
        by_artifact.entry(artifact_of(name)).or_default().push(name);
    }
    by_artifact
        .into_iter()
        .filter(|(_, names)| names.len() > 1)
        .map(|(artifact, names)| {
            format!("The server's lib/ gets more than one version of {artifact}: {}. Only one of them is used.\n", names.join(", "))
        })
        .collect()
}

/// `postgresql-42.7.3.jar` → `postgresql`: the name up to the first `-` that starts a version.
fn artifact_of(file_name: &str) -> &str {
    let stem = file_name.strip_suffix(".jar").unwrap_or(file_name);
    let bytes = stem.as_bytes();
    (1..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'-' && bytes[i + 1].is_ascii_digit())
        .map_or(stem, |i| &stem[..i])
}

fn is_jar(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("jar"))
}

/// The `.jar` files directly in `dir`, by name.
fn jars_in(dir: &Path) -> Vec<PathBuf> {
    let mut jars: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_jar(p))
        .collect();
    jars.sort();
    jars
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bennu-libs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_artifact_is_the_name_before_the_version() {
        assert_eq!(artifact_of("postgresql-42.7.3.jar"), "postgresql");
        assert_eq!(artifact_of("jakarta.ws.rs-api-2.1.6.jar"), "jakarta.ws.rs-api");
        assert_eq!(artifact_of("ojdbc8.jar"), "ojdbc8");
        assert_eq!(artifact_of("-1.jar"), "-1");
    }

    #[test]
    fn a_folder_contributes_the_jars_directly_in_it() {
        let dir = scratch("folder");
        std::fs::write(dir.join("b.jar"), b"").unwrap();
        std::fs::write(dir.join("a.JAR"), b"").unwrap();
        std::fs::write(dir.join("notes.txt"), b"").unwrap();
        std::fs::create_dir_all(dir.join("below")).unwrap();
        std::fs::write(dir.join("below").join("c.jar"), b"").unwrap();

        let set = expand(&[dir.to_string_lossy().into_owned()], |p| PathBuf::from(p));
        let names: Vec<_> = set.jars.iter().map(|j| j.file_name().unwrap().to_str().unwrap().to_string()).collect();
        assert_eq!(names, ["a.JAR", "b.jar"]);
        assert!(set.warnings.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_is_missing_or_not_a_jar_is_said_and_skipped() {
        let dir = scratch("bad");
        std::fs::write(dir.join("readme.txt"), b"").unwrap();
        let entries = [
            dir.join("readme.txt").to_string_lossy().into_owned(),
            dir.join("gone.jar").to_string_lossy().into_owned(),
            "  ".to_string(),
        ];
        let set = expand(&entries, |p| PathBuf::from(p));
        assert!(set.jars.is_empty());
        assert_eq!(set.warnings.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_jar_of_the_same_file_name_is_left_out() {
        let mut jars = vec![PathBuf::from("/a/x-1.jar")];
        add(&mut jars, PathBuf::from("/b/x-1.jar"));
        add(&mut jars, PathBuf::from("/b/y-1.jar"));
        assert_eq!(jars.len(), 2);
    }

    #[test]
    fn two_versions_of_one_library_are_reported() {
        let jars = [PathBuf::from("/a/postgresql-42.2.5.jar"), PathBuf::from("/b/postgresql-42.7.3.jar"), PathBuf::from("/c/ojdbc8.jar")];
        let said = conflicts(&jars);
        assert_eq!(said.len(), 1);
        assert!(said[0].contains("postgresql") && said[0].contains("42.2.5") && said[0].contains("42.7.3"));
    }
}
