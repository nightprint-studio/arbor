//! The library-bean scan, remembered on disk — **per artifact**, not per project.
//!
//! Opening a jar and decoding its classes is the whole cost of the feature, and it buys an
//! answer that changes only when the jar does. An in-memory cache loses that on every
//! restart; this keeps it, so the second launch of the day pays nothing.
//!
//! Keyed by the **jar**, not by the project, because two projects that both depend on
//! `com.acme:shared:1.2` are asking the identical question about the identical bytes.
//!
//! ## What invalidates an entry
//!
//! Two things can change, and forgetting either produces a stale answer that looks
//! authoritative:
//!
//! 1. **The jar.** Not its *version*: an internal module is a `-SNAPSHOT` for its whole
//!    life and gets reinstalled over itself daily, so a version-keyed cache would answer
//!    with yesterday's beans forever — and those are exactly the artifacts anyone
//!    allowlists. The fingerprint is therefore **modification time + size**, which every
//!    build tool uses for this and which a reinstall always changes. A content hash would
//!    be stricter but means reading the whole jar, which is the cost being avoided.
//!
//! 2. **The code that reads it.** A new stereotype, a corrected bean-name convention, a
//!    condition rendered differently — every one of those changes the right answer for a
//!    jar that did not move. [`SCHEMA`] is bumped when that happens and every entry
//!    written by an older reader is ignored. This is the half that is easy to leave out
//!    and impossible to notice: the cache would keep serving answers the current code
//!    would never produce.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::library_beans::LibraryBeanGroupDto;

/// The extraction's schema. **Bump this whenever what `bennu-spring`'s `beans_of_class`
/// returns for unchanged bytes changes** — a new stereotype, a different bean name, a new
/// field on the DTO. Every entry written by an older reader is then ignored and re-scanned.
const SCHEMA: u32 = 1;

/// What identifies "this jar, unchanged".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Fingerprint {
    /// Modification time, whole seconds since the epoch. Seconds and not nanoseconds: the
    /// sub-second part is not preserved by every filesystem or archive tool, and a
    /// fingerprint that changes when nothing did is a cache that never hits.
    mtime_secs: u64,
    len: u64,
}

impl Fingerprint {
    /// `None` when the jar cannot be stat'd — it was deleted or is unreadable, and either
    /// way there is nothing to remember about it.
    fn of(jar: &Path) -> Option<Self> {
        let meta = fs::metadata(jar).ok()?;
        let mtime_secs = meta
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
            // A pre-epoch mtime is nonsense but not a reason to fail; it just never matches
            // a real one, so the jar is simply re-scanned every time.
            .unwrap_or(0);
        Some(Self { mtime_secs, len: meta.len() })
    }
}

/// One artifact's remembered scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    schema: u32,
    fingerprint: Fingerprint,
    /// The jar this describes, written for a human reading the cache dir — the filename is
    /// a hash, so without this a stale entry is unattributable.
    jar: String,
    groups: Vec<LibraryBeanGroupDto>,
}

/// `bennu_data_dir()/library-beans/<hash-of-jar-path>.json`.
///
/// One file per artifact rather than one index: a single dependency changing then rewrites
/// one small file instead of the whole set, and a corrupt entry costs one artifact rather
/// than all of them.
fn entry_path(jar: &str) -> PathBuf {
    // The same FNV-1a the index base uses — short, filesystem-safe, and collision-resistant
    // enough for a local cache whose worst case is one needless re-scan.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in jar.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    arbor_core::prelude::bennu_data_dir()
        .join("library-beans")
        .join(format!("{hash:016x}.json"))
}

/// The remembered scan for `jar`, or `None` when there is none, it was written by an older
/// reader, or the jar has changed since.
///
/// Every failure is a miss: a corrupt or unreadable entry means "scan it again", never an
/// error. A cache that can fail a request is worse than no cache.
pub fn load(jar: &str) -> Option<Vec<LibraryBeanGroupDto>> {
    let current = Fingerprint::of(Path::new(jar))?;
    let text = fs::read_to_string(entry_path(jar)).ok()?;
    let entry: Entry = serde_json::from_str(&text).ok()?;
    (entry.schema == SCHEMA && entry.fingerprint == current).then_some(entry.groups)
}

/// Remember `groups` as the scan of `jar`. Best-effort: a cache that cannot be written is a
/// slower next launch, not a failure, so nothing here is reported.
pub fn store(jar: &str, groups: &[LibraryBeanGroupDto]) {
    let Some(fingerprint) = Fingerprint::of(Path::new(jar)) else { return };
    let entry = Entry {
        schema: SCHEMA,
        fingerprint,
        jar: jar.to_string(),
        groups: groups.to_vec(),
    };
    let Ok(text) = serde_json::to_string(&entry) else { return };
    let path = entry_path(jar);
    if let Some(dir) = path.parent() {
        if fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let _ = fs::write(path, text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_jar(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("bennu-lbcache-{name}"));
        let mut f = fs::File::create(&path).expect("create temp jar");
        f.write_all(bytes).expect("write temp jar");
        path
    }

    #[test]
    fn a_missing_jar_has_no_fingerprint() {
        assert!(Fingerprint::of(Path::new("/does/not/exist.jar")).is_none());
    }

    /// Size is half the fingerprint, so a jar reinstalled with different content in the
    /// same second is still seen as changed.
    #[test]
    fn size_is_part_of_the_fingerprint() {
        let short = temp_jar("short.jar", b"ab");
        let long = temp_jar("long.jar", b"abcdefgh");
        let a = Fingerprint::of(&short).expect("short");
        let b = Fingerprint::of(&long).expect("long");
        assert_ne!(a, b);
        let _ = fs::remove_file(short);
        let _ = fs::remove_file(long);
    }

    /// A miss must never be an error: a jar that is gone, an entry that was never written.
    #[test]
    fn an_absent_entry_is_a_miss_not_a_failure() {
        assert!(load("/does/not/exist.jar").is_none());
    }

    #[test]
    fn different_jars_get_different_entry_files() {
        assert_ne!(entry_path("/m2/a/x.jar"), entry_path("/m2/b/x.jar"));
    }

    /// The half that is easy to leave out: the reader changing invalidates entries the jar
    /// did not touch, or the cache serves answers the current code would never produce.
    #[test]
    fn an_entry_from_an_older_reader_is_rejected() {
        let jar = temp_jar("schema.jar", b"contents");
        let jar_str = jar.to_string_lossy().to_string();
        let stale = Entry {
            schema: SCHEMA - 1,
            fingerprint: Fingerprint::of(&jar).expect("fingerprint"),
            jar: jar_str.clone(),
            groups: Vec::new(),
        };
        let path = entry_path(&jar_str);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).expect("cache dir");
        }
        fs::write(&path, serde_json::to_string(&stale).expect("serialize")).expect("write");

        assert!(load(&jar_str).is_none(), "an older schema must not be served");

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(jar);
    }
}
