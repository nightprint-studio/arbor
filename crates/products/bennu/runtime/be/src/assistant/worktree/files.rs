//! The disk side of an isolated copy: walking a tree, copying it, and taking a manifest of what is in it.
//!
//! Everything here is plain `std::fs` and runs off the request worker (the callers are on a blocking
//! thread). Noise — version control, dependency folders, build output — is skipped by the same rules the
//! project tree and the text search use, so a copy is the project's sources, not its `node_modules`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use bennu_assistant::prelude::{fnv64_update, Base, Manifest, FNV_SEED};

use super::super::text_tools::SKIPPED_DIRS;

/// The largest project a plain (non-git) copy accepts, in bytes and in files.
pub(super) const MAX_COPY_BYTES: u64 = 512 * 1024 * 1024;
pub(super) const MAX_COPY_FILES: usize = 40_000;
/// A file bigger than this is not part of a copy at all (data files, archives): it is neither copied nor
/// compared, so it can never be overwritten by an apply.
pub(super) const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// What a walk found.
#[derive(Debug, Default)]
pub(super) struct Walked {
    /// Relative path (forward slashes) → absolute path, sorted.
    pub files: BTreeMap<String, PathBuf>,
    pub bytes: u64,
}

/// Why a walk stopped early.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Overflow {
    Bytes,
    Files,
}

fn is_skipped_dir(parent: &Path, name: &str) -> bool {
    SKIPPED_DIRS.contains(&name) || bennu_project::prelude::is_tree_noise_dir(parent, name)
}

/// Every regular file under `root` that a copy would include. With `cap`, stops (and says why) once the
/// bytes or the file count pass it, so sizing a huge folder costs a bounded walk.
pub(super) fn walk(root: &Path, cap: Option<(u64, usize)>) -> Result<Walked, (Overflow, Walked)> {
    let mut out = Walked::default();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if kind.is_dir() {
                if !is_skipped_dir(&dir, &name) {
                    stack.push(path);
                }
                continue;
            }
            // A `.git` file marks a linked worktree; symlinks and sockets are not followed or copied.
            if !kind.is_file() || name == ".git" {
                continue;
            }
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if len > MAX_FILE_BYTES {
                continue;
            }
            let Some(rel) = relative(root, &path) else { continue };
            out.bytes += len;
            out.files.insert(rel, path);
            if let Some((max_bytes, max_files)) = cap {
                if out.bytes > max_bytes {
                    return Err((Overflow::Bytes, out));
                }
                if out.files.len() > max_files {
                    return Err((Overflow::Files, out));
                }
            }
        }
    }
    Ok(out)
}

fn relative(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root).ok().map(|p| p.to_string_lossy().replace('\\', "/"))
}

/// Size and hash of one file, read in blocks.
pub(super) fn identity(path: &Path) -> std::io::Result<Base> {
    let mut file = fs::File::open(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    let (mut hash, mut len) = (FNV_SEED, 0u64);
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash = fnv64_update(hash, &buf[..n]);
        len += n as u64;
    }
    Ok(Base { len, hash })
}

/// The identity of every file in `walked`, calling `progress(done, total)` now and then.
pub(super) fn manifest_of(walked: &Walked, progress: &dyn Fn(usize, usize)) -> Manifest {
    let total = walked.files.len();
    let mut out = Manifest::new();
    for (i, (rel, path)) in walked.files.iter().enumerate() {
        if let Ok(base) = identity(path) {
            out.insert(rel.clone(), base);
        }
        if i % 200 == 0 {
            progress(i, total);
        }
    }
    progress(total, total);
    out
}

/// Copy every file of `walked` (rooted at `from`) to the same relative place under `to`.
pub(super) fn copy_all(walked: &Walked, to: &Path, progress: &dyn Fn(usize, usize)) -> Result<(), String> {
    let total = walked.files.len();
    for (i, (rel, src)) in walked.files.iter().enumerate() {
        let dst = to.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        fs::copy(src, &dst).map_err(|e| format!("could not copy {}: {e}", src.display()))?;
        if i % 200 == 0 {
            progress(i, total);
        }
    }
    progress(total, total);
    Ok(())
}

/// The nearest folder at or above `start` that holds a `.git` entry (a folder, or the file of a linked worktree).
pub(super) fn git_root_of(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|d| d.join(".git").exists()).map(Path::to_path_buf)
}

/// Remove a directory tree, retrying a few times: on Windows a file a scanner or an indexer still has open
/// makes the first attempt fail.
pub(super) fn remove_tree(dir: &Path) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..5 {
        match fs::remove_dir_all(dir) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                last = e.to_string();
                std::thread::sleep(std::time::Duration::from_millis(150 * (attempt + 1)));
            }
        }
    }
    Err(format!("could not remove {}: {last}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bennu-wt-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for (f, text) in files {
            let full = dir.join(f);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, text).unwrap();
        }
        dir
    }

    #[test]
    fn a_walk_skips_noise_and_linked_worktree_markers() {
        let dir = tree("walk", &[("src/A.java", "a"), ("node_modules/x/y.js", "n"), (".git", "gitdir: x"), ("pom.xml", "<p/>"), ("target/classes/A.class", "c")]);
        let walked = walk(&dir, None).unwrap();
        let rels: Vec<&str> = walked.files.keys().map(String::as_str).collect();
        assert_eq!(rels, ["pom.xml", "src/A.java"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_capped_walk_stops_and_says_why() {
        let dir = tree("cap", &[("a.txt", "12345"), ("b.txt", "12345"), ("c.txt", "12345")]);
        assert_eq!(walk(&dir, Some((8, 100))).unwrap_err().0, Overflow::Bytes);
        assert_eq!(walk(&dir, Some((1000, 2))).unwrap_err().0, Overflow::Files);
        assert!(walk(&dir, Some((1000, 10))).is_ok());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_copy_has_the_same_manifest_as_its_source() {
        let src = tree("copy-src", &[("src/A.java", "class A {}"), ("README.md", "hi")]);
        let dst = std::env::temp_dir().join(format!("bennu-wt-files-copy-dst-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dst);
        let walked = walk(&src, None).unwrap();
        copy_all(&walked, &dst, &|_, _| {}).unwrap();
        let (a, b) = (manifest_of(&walked, &|_, _| {}), manifest_of(&walk(&dst, None).unwrap(), &|_, _| {}));
        assert_eq!(a, b);
        assert_eq!(a.len(), 2);
        fs::write(dst.join("README.md"), "changed").unwrap();
        assert_ne!(manifest_of(&walk(&dst, None).unwrap(), &|_, _| {}), a);
        let _ = fs::remove_dir_all(src);
        let _ = fs::remove_dir_all(dst);
    }

    #[test]
    fn the_git_root_is_the_nearest_folder_with_a_git_entry() {
        let dir = tree("gitroot", &[(".git/HEAD", "ref"), ("app/src/A.java", "a")]);
        assert_eq!(git_root_of(&dir.join("app/src")), Some(dir.clone()));
        let _ = fs::remove_dir_all(dir);
    }
}
