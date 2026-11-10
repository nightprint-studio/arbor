//! Mounted-volume detection for the explorer's Devices list: which volumes are
//! attached *right now* ([`list_mounts`]) and what changed between two snapshots
//! ([`diff_mounts`], pure).
//!
//! Deliberately cheap and probe-free — Windows reads the `GetLogicalDrives`
//! bitmask, Unix lists the conventional mount parents — so a watcher can sample it
//! every couple of seconds without waking sleeping removable media.

use crate::entry::FsRoot;

/// What changed between two mount snapshots, keyed by `FsRoot::path`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MountDiff {
    pub added:   Vec<String>,
    pub removed: Vec<String>,
}

impl MountDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// Paths present in `next` but not `prev` (`added`) and vice versa (`removed`).
/// Order within each list follows the input order.
pub fn diff_mounts(prev: &[FsRoot], next: &[FsRoot]) -> MountDiff {
    let missing_from = |haystack: &[FsRoot], needle: &FsRoot| !haystack.iter().any(|r| r.path == needle.path);
    MountDiff {
        added:   next.iter().filter(|r| missing_from(prev, r)).map(|r| r.path.clone()).collect(),
        removed: prev.iter().filter(|r| missing_from(next, r)).map(|r| r.path.clone()).collect(),
    }
}

/// The volumes attached right now: drive letters on Windows; the entries under
/// `/Volumes` (macOS) or `/media`, `/media/<user>`, `/run/media/<user>` (Linux).
/// Excludes the system root (`/`), which is always present.
pub fn list_mounts() -> Vec<FsRoot> {
    list_mounts_impl()
}

#[cfg(target_os = "windows")]
fn list_mounts_impl() -> Vec<FsRoot> {
    crate::roots::enumerate_drives()
}

#[cfg(not(target_os = "windows"))]
fn list_mounts_impl() -> Vec<FsRoot> {
    let mut parents: Vec<std::path::PathBuf> = vec!["/Volumes".into(), "/media".into()];
    if let Some(user) = std::env::var_os("USER").or_else(|| std::env::var_os("LOGNAME")) {
        parents.push(std::path::Path::new("/media").join(&user));
        parents.push(std::path::Path::new("/run/media").join(&user));
    }
    let mut out: Vec<FsRoot> = Vec::new();
    for parent in parents {
        let Ok(rd) = std::fs::read_dir(&parent) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            // `/media/<user>` is itself a listed parent — not a volume.
            if parents_contains_dir(&path) || !path.is_dir() { continue; }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { continue; }
            let path = path.to_string_lossy().to_string();
            if out.iter().all(|r| r.path != path) {
                out.push(FsRoot { name, path, kind: "drive".to_string() });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// True for the per-user subfolders that are scanned as parents in their own right.
#[cfg(not(target_os = "windows"))]
fn parents_contains_dir(path: &std::path::Path) -> bool {
    let Some(user) = std::env::var_os("USER").or_else(|| std::env::var_os("LOGNAME")) else { return false };
    path == std::path::Path::new("/media").join(&user) || path == std::path::Path::new("/run/media").join(&user)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(p: &str) -> FsRoot {
        FsRoot { name: p.to_string(), path: p.to_string(), kind: "drive".to_string() }
    }

    #[test]
    fn identical_snapshots_have_no_diff() {
        let a = vec![root("C:\\"), root("D:\\")];
        assert!(diff_mounts(&a, &a.clone()).is_empty());
    }

    #[test]
    fn detects_added_drive() {
        let d = diff_mounts(&[root("C:\\")], &[root("C:\\"), root("E:\\")]);
        assert_eq!(d.added, vec!["E:\\"]);
        assert!(d.removed.is_empty());
    }

    #[test]
    fn detects_removed_drive() {
        let d = diff_mounts(&[root("C:\\"), root("E:\\")], &[root("C:\\")]);
        assert_eq!(d.removed, vec!["E:\\"]);
        assert!(d.added.is_empty());
    }

    #[test]
    fn detects_swap_and_ignores_reordering() {
        let d = diff_mounts(&[root("D:\\"), root("E:\\")], &[root("F:\\"), root("D:\\")]);
        assert_eq!(d.added, vec!["F:\\"]);
        assert_eq!(d.removed, vec!["E:\\"]);
        assert!(diff_mounts(&[root("D:\\"), root("E:\\")], &[root("E:\\"), root("D:\\")]).is_empty());
    }
}
