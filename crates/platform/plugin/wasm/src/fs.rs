//! The read-only `fs` capability — what a guest may read from disk, and the gate in front of it.
//!
//! A guest has no WASI filesystem (see [`crate::guest::WasiState`]: the context is built empty).
//! When one needs a file — a language backend reading the mod's UI documents and translations —
//! it asks the host through `arbor:extensions/fs`, and the answer comes from here.
//!
//! ## One place
//!
//! Every rule is in [`GuestCaps::check_fs_read`], and the two operations ([`GuestCaps::fs_read_file`],
//! [`GuestCaps::fs_list_dir`]) cannot touch the disk without passing it: the checked, canonical
//! path is the only thing they accept. The effect is plain `std::fs` reads and so lives next to
//! the gate instead of behind [`crate::services::HostServices`]: there is nothing for an
//! embedder to decide, and a shell that had to re-implement a read would be a shell that could
//! forget a rule.
//!
//! ## The rules, in order
//!
//! 1. `[permissions] fs` must be `read` or `write`. This interface never writes, whatever the
//!    level: `write` only means the *package* may also write through other APIs.
//! 2. The lexical policy shared with the Lua host (`arbor_plugin_types::fs_scope`): the active
//!    repository plus `fs_scope`, `..` folded **before** the comparison.
//! 3. The path is canonicalised and checked against the *canonical* roots, so a symlink inside
//!    the repo that points outside it is refused ([`FsError::SymlinkEscape`]), and a root that is
//!    itself reached through a symlink (`/tmp` on macOS) still matches.
//! 4. A file read is capped at [`MAX_READ_BYTES`] and must be UTF-8 text without NUL bytes.
//! 5. A directory listing is capped at [`MAX_LIST_ENTRIES`], and an entry that is a symlink out
//!    of the allowed area is left out rather than reported with its target's size.
//!
//! ## How the guest learns the active repository
//!
//! It does not. The host sets it for the duration of each call ([`GuestCaps::set_repo`], from
//! the shell's cached active-tab path), so a persistent guest that outlives a tab switch is
//! checked against the repository open *now*, and the repository cannot change under a call in
//! flight. With no repository open, only `fs_scope` roots are reachable.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use arbor_plugin_types::prelude::{
    fs_allowed_roots, fs_is_unrestricted, fs_resolve_in_scope, AccessLevel, FsScopeDenial,
};

use crate::caps::GuestCaps;

/// The most a single `read-file` returns. A UI document or a translation table is a few
/// kilobytes; the largest real one is far under this, and a guest asking for more is asking for
/// something that is not a document.
pub const MAX_READ_BYTES: u64 = 4 * 1024 * 1024;

/// The most entries a single `list-dir` returns.
pub const MAX_LIST_ENTRIES: usize = 20_000;

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsEntry {
    pub name: String,
    pub is_dir: bool,
    /// Bytes for a file; whatever the platform reports for a directory.
    pub size: u64,
    /// Milliseconds since the Unix epoch; `None` when the platform cannot say. "Unknown" and
    /// "1970" lead to opposite decisions in a cache, so they are not the same value.
    pub modified_ms: Option<u64>,
}

/// Why a read was refused or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    /// `[permissions] fs` is not `read` or higher.
    NoPermission { plugin: String },
    /// A relative path with no active repository and no extra root.
    NoRepo,
    /// The path is not under the active repository or an `fs_scope` root.
    OutsideScope { path: String, scope: String },
    /// The path is inside the scope by name but resolves outside it (a symlink).
    SymlinkEscape { path: String },
    NotFound { path: String },
    NotAFile { path: String },
    NotADirectory { path: String },
    TooLarge { path: String, size: u64, limit: u64 },
    /// Not UTF-8 text (invalid UTF-8, or it contains NUL bytes).
    NotText { path: String },
    TooManyEntries { path: String, limit: usize },
    /// Anything else the OS said.
    Io { path: String, message: String },
}

impl FsError {
    fn io(path: &Path, e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => FsError::NotFound { path: path.display().to_string() },
            _ => FsError::Io { path: path.display().to_string(), message: e.to_string() },
        }
    }
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsError::NoPermission { plugin } => write!(
                f,
                "'{plugin}' requires `fs` permission to read files. Add to plugin.toml:\n\
                 [permissions]\nfs = \"read\""
            ),
            FsError::NoRepo => write!(
                f,
                "no active repository (default sandbox requires an open repo)"
            ),
            FsError::OutsideScope { path, scope } => {
                write!(f, "path '{path}' is outside the allowed scope ({scope})")
            }
            FsError::SymlinkEscape { path } => {
                write!(f, "path '{path}' resolves outside the allowed scope (symbolic link)")
            }
            FsError::NotFound { path } => write!(f, "'{path}' does not exist"),
            FsError::NotAFile { path } => write!(f, "'{path}' is not a file"),
            FsError::NotADirectory { path } => write!(f, "'{path}' is not a directory"),
            FsError::TooLarge { path, size, limit } => {
                write!(f, "'{path}' is {size} bytes, over the {limit}-byte read limit")
            }
            FsError::NotText { path } => write!(f, "'{path}' is not UTF-8 text"),
            FsError::TooManyEntries { path, limit } => {
                write!(f, "'{path}' has more than {limit} entries")
            }
            FsError::Io { path, message } => write!(f, "reading '{path}': {message}"),
        }
    }
}

impl std::error::Error for FsError {}

impl From<FsScopeDenial> for FsError {
    fn from(d: FsScopeDenial) -> Self {
        match d {
            FsScopeDenial::NoRepo => FsError::NoRepo,
            FsScopeDenial::Outside { path, scope } => {
                FsError::OutsideScope { path: path.display().to_string(), scope }
            }
        }
    }
}

impl GuestCaps {
    /// The gate. Returns the **canonical** path the read must use.
    ///
    /// Everything else in this file takes the return value, never the guest's string.
    pub fn check_fs_read(&self, path: &str) -> Result<PathBuf, FsError> {
        if self.fs_level() < AccessLevel::Read {
            return Err(FsError::NoPermission { plugin: self.plugin().to_string() });
        }
        let abs = fs_resolve_in_scope(self.repo(), self.fs_scope(), Path::new(path))?;
        let canonical = std::fs::canonicalize(&abs).map_err(|e| FsError::io(&abs, e))?;
        if !fs_is_unrestricted(self.fs_scope()) && !self.within_canonical_roots(&canonical) {
            return Err(FsError::SymlinkEscape { path: path.to_string() });
        }
        Ok(canonical)
    }

    /// Whether an already-canonical path is under one of the (canonicalised) allowed roots.
    fn within_canonical_roots(&self, canonical: &Path) -> bool {
        fs_allowed_roots(self.repo(), self.fs_scope())
            .iter()
            .filter_map(|r| std::fs::canonicalize(r).ok())
            .any(|r| canonical.starts_with(r))
    }

    /// Read one file as UTF-8 text.
    pub fn fs_read_file(&self, path: &str) -> Result<String, FsError> {
        let canonical = self.check_fs_read(path)?;
        let shown = canonical.display().to_string();
        let file = std::fs::File::open(&canonical).map_err(|e| FsError::io(&canonical, e))?;
        let meta = file.metadata().map_err(|e| FsError::io(&canonical, e))?;
        if !meta.is_file() {
            return Err(FsError::NotAFile { path: shown });
        }
        if meta.len() > MAX_READ_BYTES {
            return Err(FsError::TooLarge { path: shown, size: meta.len(), limit: MAX_READ_BYTES });
        }
        // `take(limit + 1)`: the size above was read before the bytes, and a file that grows
        // in between must still not get past the cap.
        let mut bytes = Vec::new();
        file.take(MAX_READ_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| FsError::io(&canonical, e))?;
        if bytes.len() as u64 > MAX_READ_BYTES {
            return Err(FsError::TooLarge {
                path: shown,
                size: bytes.len() as u64,
                limit: MAX_READ_BYTES,
            });
        }
        if bytes.contains(&0) {
            return Err(FsError::NotText { path: shown });
        }
        String::from_utf8(bytes).map_err(|_| FsError::NotText { path: shown })
    }

    /// List one directory, sorted by name.
    pub fn fs_list_dir(&self, path: &str) -> Result<Vec<FsEntry>, FsError> {
        let canonical = self.check_fs_read(path)?;
        let shown = canonical.display().to_string();
        if !canonical.is_dir() {
            return Err(FsError::NotADirectory { path: shown });
        }
        let read = std::fs::read_dir(&canonical).map_err(|e| FsError::io(&canonical, e))?;
        let unrestricted = fs_is_unrestricted(self.fs_scope());
        let mut out = Vec::new();
        for item in read {
            let item = item.map_err(|e| FsError::io(&canonical, e))?;
            // A name that is not UTF-8 cannot be handed to a guest as a string, and a guest
            // could not ask for it back; leaving it out is more honest than lossy-renaming it.
            let Some(name) = item.file_name().to_str().map(str::to_owned) else { continue };
            let is_link = item.file_type().map(|t| t.is_symlink()).unwrap_or(false);
            if is_link && !unrestricted {
                // Same rule as a direct read: a link is listed only if its target is reachable.
                match std::fs::canonicalize(item.path()) {
                    Ok(target) if self.within_canonical_roots(&target) => {}
                    _ => continue,
                }
            }
            // `metadata` follows links, so a link to a file reports the file.
            let Ok(meta) = std::fs::metadata(item.path()) else { continue };
            if out.len() >= MAX_LIST_ENTRIES {
                return Err(FsError::TooManyEntries { path: shown, limit: MAX_LIST_ENTRIES });
            }
            out.push(FsEntry {
                name,
                is_dir: meta.is_dir(),
                size: meta.len(),
                modified_ms: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A scratch directory under the system temp dir, removed on drop. Hand-rolled: this crate
    /// has no `tempfile`, and a dev-dependency is not worth one test helper.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "arbor-wasm-fs-{tag}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
        fn write(&self, rel: &str, text: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
            p
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn caps(level: AccessLevel, repo: &Path, scope: Vec<String>) -> GuestCaps {
        GuestCaps::new("t", vec![], vec![])
            .with_fs(level, scope)
            .with_repo(Some(repo.to_path_buf()))
    }

    #[test]
    fn a_file_inside_the_repo_is_read() {
        let s = Scratch::new("ok");
        let f = s.write("a/b.ron", "(x: 1)");
        let c = caps(AccessLevel::Read, s.path(), vec![]);
        assert_eq!(c.fs_read_file(f.to_str().unwrap()).unwrap(), "(x: 1)");
    }

    #[test]
    fn a_relative_path_resolves_against_the_repo() {
        let s = Scratch::new("rel");
        s.write("a/b.ron", "hi");
        let c = caps(AccessLevel::Read, s.path(), vec![]);
        assert_eq!(c.fs_read_file("a/b.ron").unwrap(), "hi");
    }

    #[test]
    fn without_the_fs_permission_nothing_is_read() {
        let s = Scratch::new("noperm");
        let f = s.write("a.txt", "x");
        let c = caps(AccessLevel::None, s.path(), vec![]);
        assert!(matches!(c.fs_read_file(f.to_str().unwrap()), Err(FsError::NoPermission { .. })));
        assert!(matches!(c.fs_list_dir(s.path().to_str().unwrap()), Err(FsError::NoPermission { .. })));
    }

    #[test]
    fn write_level_still_only_reads_here() {
        // The interface has no write function at all; this documents that `write` is accepted
        // as "at least read".
        let s = Scratch::new("wr");
        let f = s.write("a.txt", "x");
        let c = caps(AccessLevel::Write, s.path(), vec![]);
        assert_eq!(c.fs_read_file(f.to_str().unwrap()).unwrap(), "x");
    }

    #[test]
    fn a_path_outside_the_repo_is_refused() {
        let repo = Scratch::new("in");
        let other = Scratch::new("out");
        let f = other.write("secret.txt", "s");
        let c = caps(AccessLevel::Read, repo.path(), vec![]);
        assert!(matches!(c.fs_read_file(f.to_str().unwrap()), Err(FsError::OutsideScope { .. })));
    }

    #[test]
    fn dotdot_cannot_climb_out_of_the_repo() {
        let outer = Scratch::new("dd");
        outer.write("secret.txt", "s");
        let repo = outer.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let c = caps(AccessLevel::Read, &repo, vec![]);
        assert!(matches!(c.fs_read_file("../secret.txt"), Err(FsError::OutsideScope { .. })));
        let spelled = format!("{}/sub/../../secret.txt", repo.display());
        assert!(matches!(c.fs_read_file(&spelled), Err(FsError::OutsideScope { .. })));
    }

    #[test]
    fn an_fs_scope_root_is_allowed_besides_the_repo() {
        let repo = Scratch::new("r");
        let extra = Scratch::new("x");
        let f = extra.write("ok.txt", "yes");
        let c = caps(AccessLevel::Read, repo.path(), vec![extra.path().display().to_string()]);
        assert_eq!(c.fs_read_file(f.to_str().unwrap()).unwrap(), "yes");
    }

    #[test]
    fn with_no_repo_only_scope_roots_are_reachable() {
        let extra = Scratch::new("norepo");
        let f = extra.write("ok.txt", "yes");
        let c = GuestCaps::new("t", vec![], vec![])
            .with_fs(AccessLevel::Read, vec![extra.path().display().to_string()]);
        assert_eq!(c.fs_read_file(f.to_str().unwrap()).unwrap(), "yes");
        assert!(matches!(c.fs_read_file("/etc/hosts"), Err(FsError::OutsideScope { .. })));

        let c = GuestCaps::new("t", vec![], vec![]).with_fs(AccessLevel::Read, vec![]);
        assert!(matches!(c.fs_read_file("rel.txt"), Err(FsError::NoRepo)));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_leaves_the_repo_is_refused() {
        let outer = Scratch::new("ln");
        outer.write("secret.txt", "s");
        let repo = outer.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::os::unix::fs::symlink(outer.path().join("secret.txt"), repo.join("link.txt")).unwrap();
        let c = caps(AccessLevel::Read, &repo, vec![]);
        assert!(matches!(c.fs_read_file("link.txt"), Err(FsError::SymlinkEscape { .. })));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_directory_that_leaves_the_repo_is_refused() {
        let outer = Scratch::new("lnd");
        outer.write("other/x.txt", "s");
        let repo = outer.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::os::unix::fs::symlink(outer.path().join("other"), repo.join("d")).unwrap();
        let c = caps(AccessLevel::Read, &repo, vec![]);
        assert!(matches!(c.fs_read_file("d/x.txt"), Err(FsError::SymlinkEscape { .. })));
        assert!(matches!(c.fs_list_dir("d"), Err(FsError::SymlinkEscape { .. })));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_stays_inside_the_repo_is_followed() {
        let s = Scratch::new("lni");
        s.write("real.txt", "r");
        std::os::unix::fs::symlink(s.path().join("real.txt"), s.path().join("link.txt")).unwrap();
        let c = caps(AccessLevel::Read, s.path(), vec![]);
        assert_eq!(c.fs_read_file("link.txt").unwrap(), "r");
    }

    #[cfg(unix)]
    #[test]
    fn a_listing_leaves_out_a_link_that_leaves_the_repo() {
        let outer = Scratch::new("lnl");
        outer.write("secret.txt", "s");
        let repo = outer.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("a.txt"), "a").unwrap();
        std::os::unix::fs::symlink(outer.path().join("secret.txt"), repo.join("link.txt")).unwrap();
        let c = caps(AccessLevel::Read, &repo, vec![]);
        let names: Vec<_> = c.fs_list_dir(".").unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["a.txt"]);
    }

    #[test]
    fn a_missing_file_is_not_found() {
        let s = Scratch::new("nf");
        let c = caps(AccessLevel::Read, s.path(), vec![]);
        assert!(matches!(c.fs_read_file("nope.txt"), Err(FsError::NotFound { .. })));
        assert!(matches!(c.fs_list_dir("nope"), Err(FsError::NotFound { .. })));
    }

    #[test]
    fn a_file_over_the_cap_is_refused_without_reading_it() {
        let s = Scratch::new("big");
        let f = s.path().join("big.txt");
        let file = std::fs::File::create(&f).unwrap();
        file.set_len(MAX_READ_BYTES + 1).unwrap();
        let c = caps(AccessLevel::Read, s.path(), vec![]);
        assert!(matches!(c.fs_read_file("big.txt"), Err(FsError::TooLarge { .. })));
    }

    #[test]
    fn a_file_exactly_at_the_cap_is_read() {
        let s = Scratch::new("cap");
        s.write("cap.txt", &"a".repeat(MAX_READ_BYTES as usize));
        let c = caps(AccessLevel::Read, s.path(), vec![]);
        assert_eq!(c.fs_read_file("cap.txt").unwrap().len() as u64, MAX_READ_BYTES);
    }

    #[test]
    fn binary_and_non_utf8_files_are_refused() {
        let s = Scratch::new("bin");
        std::fs::write(s.path().join("a.bin"), [0xff, 0xfe, 0x41]).unwrap();
        std::fs::write(s.path().join("b.bin"), b"ok\0ok").unwrap();
        let c = caps(AccessLevel::Read, s.path(), vec![]);
        assert!(matches!(c.fs_read_file("a.bin"), Err(FsError::NotText { .. })));
        assert!(matches!(c.fs_read_file("b.bin"), Err(FsError::NotText { .. })));
    }

    #[test]
    fn a_directory_is_not_a_file_and_a_file_is_not_a_directory() {
        let s = Scratch::new("kind");
        s.write("d/f.txt", "x");
        let c = caps(AccessLevel::Read, s.path(), vec![]);
        assert!(matches!(c.fs_read_file("d"), Err(FsError::NotAFile { .. })));
        assert!(matches!(c.fs_list_dir("d/f.txt"), Err(FsError::NotADirectory { .. })));
    }

    #[test]
    fn a_listing_carries_name_kind_size_and_time_sorted() {
        let s = Scratch::new("ls");
        s.write("b.txt", "12345");
        s.write("a.txt", "1");
        s.write("sub/x.txt", "x");
        let c = caps(AccessLevel::Read, s.path(), vec![]);
        let l = c.fs_list_dir(".").unwrap();
        let names: Vec<_> = l.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "b.txt", "sub"]);
        assert_eq!(l[1].size, 5);
        assert!(!l[1].is_dir && l[2].is_dir);
        assert!(l[0].modified_ms.is_some_and(|m| m > 0));
    }

    #[test]
    fn the_checked_path_is_canonical() {
        let s = Scratch::new("canon");
        s.write("a/b.txt", "x");
        let c = caps(AccessLevel::Read, s.path(), vec![]);
        let p = c.check_fs_read("a/./b.txt").unwrap();
        assert_eq!(p, std::fs::canonicalize(s.path().join("a/b.txt")).unwrap());
    }
}
