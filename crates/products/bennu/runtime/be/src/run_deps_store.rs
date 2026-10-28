//! The launch classpaths resolved so far, kept between sessions.
//!
//! Resolving a module's run classpath is a `mvn dependency:build-classpath` — a JVM start-up, every
//! pom read and every dependency walked — and the first ▷ of every session used to pay it again
//! even with no pom touched. What is stored is only what the in-memory cache already trusts:
//! the entries Maven returned for a complete answer, together with the fingerprint of the poms they
//! were resolved under. A stored answer is never served on faith — the same checks apply as to a
//! fresh one (the poms' fingerprint still matches, every entry still exists on disk), so an edited
//! pom or a purged `~/.m2` sends the launch back to Maven exactly as before.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

/// One cached resolution: the module (`rel`, empty = the root), the Maven scope, the poms'
/// fingerprint at the time, and the classpath entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoredDeps {
    pub(crate) rel: String,
    pub(crate) scope: String,
    pub(crate) fingerprint: u64,
    pub(crate) entries: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize, Default)]
struct Stored {
    version: u32,
    deps: Vec<StoredDeps>,
}

fn path_for(root: &str) -> PathBuf {
    arbor_core::prelude::bennu_data_dir()
        .join("run-deps")
        .join(format!("{:016x}.json", crate::build_freshness::fnv1a(root)))
}

/// What an earlier session stored for `root`; empty when there is nothing, or it cannot be read.
pub(crate) fn load(root: &str) -> Vec<StoredDeps> {
    let Ok(bytes) = std::fs::read(path_for(root)) else { return Vec::new() };
    decode(&bytes)
}

/// Replace what is stored for `root`. Best-effort: failing to save costs the next session one
/// resolution, never a launch.
pub(crate) fn save(root: &str, deps: &[StoredDeps]) {
    let path = path_for(root);
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Some(bytes) = encode(deps) else { return };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Forget what is stored for `root`.
pub(crate) fn remove(root: &str) {
    let _ = std::fs::remove_file(path_for(root));
}

fn encode(deps: &[StoredDeps]) -> Option<Vec<u8>> {
    serde_json::to_vec(&Stored { version: VERSION, deps: deps.to_vec() }).ok()
}

fn decode(bytes: &[u8]) -> Vec<StoredDeps> {
    match serde_json::from_slice::<Stored>(bytes) {
        Ok(stored) if stored.version == VERSION => stored.deps,
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<StoredDeps> {
        vec![
            StoredDeps {
                rel: String::new(),
                scope: "runtime".into(),
                fingerprint: 0xdead_beef,
                entries: vec![PathBuf::from("/m2/a/a.jar"), PathBuf::from("/p/core/target/classes")],
            },
            StoredDeps { rel: "web".into(), scope: String::new(), fingerprint: 7, entries: vec![] },
        ]
    }

    #[test]
    fn stored_resolutions_round_trip() {
        let bytes = encode(&sample()).expect("encodes");
        assert_eq!(decode(&bytes), sample());
    }

    #[test]
    fn a_file_in_another_format_is_ignored_not_trusted() {
        let mut stored = Stored { version: VERSION + 1, deps: sample() };
        let bytes = serde_json::to_vec(&stored).unwrap();
        assert!(decode(&bytes).is_empty());
        stored.version = VERSION;
        assert_eq!(decode(&serde_json::to_vec(&stored).unwrap()), sample());
    }

    #[test]
    fn garbage_reads_as_nothing_stored() {
        assert!(decode(b"").is_empty());
        assert!(decode(b"not json").is_empty());
        assert!(decode(b"{\"version\":1}").is_empty(), "a file without its list is not a record");
    }

    #[test]
    fn two_roots_never_share_a_file() {
        assert_ne!(path_for("/p/one"), path_for("/p/two"));
        assert_eq!(path_for("/p/one"), path_for("/p/one"));
    }
}
