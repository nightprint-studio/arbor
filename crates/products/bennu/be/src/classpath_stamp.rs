//! One definition of **"this classpath, unchanged"**, for every cache built on top of it.
//!
//! Several things here are computed from a project's dependency jars and then remembered:
//! the diagnostic cache, the dependency member memo, the decompiled source views, the
//! library-bean scan. Each is only valid while the jars it was computed from are the same
//! jars — and each was answering that question its own way, or not at all.
//!
//! The trap they all share is that **a jar's identity is not its path**. An internal module
//! is a `-SNAPSHOT` for its whole life: `mvn install` rewrites it in place, daily, with
//! different bytes under the same name in the same place. A cache keyed by the path — or by
//! the coordinate, or by the resolved jar *set* — sees nothing change and keeps serving what
//! it computed last week. And those are exactly the artifacts anyone is actively editing, so
//! it is the case that matters most and the one that stays wrong the longest.
//!
//! So identity is **path + modification time + size**. That is what every build tool uses,
//! it is one `stat` per jar, and an in-place reinstall always changes at least one of the
//! two. A content hash would be stricter and means reading every jar in full, which is the
//! cost all of this exists to avoid.

use std::path::Path;
use std::time::UNIX_EPOCH;

/// What identifies one jar's contents without reading them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JarStamp {
    /// Modification time, whole seconds since the epoch. Seconds and not nanoseconds: the
    /// sub-second part survives neither every filesystem nor every archive tool, and a
    /// stamp that changes when nothing did is a cache that never hits.
    pub mtime_secs: u64,
    pub len: u64,
}

/// The stamp of the jar at `path`, or `None` when it cannot be stat'd — deleted, or on a
/// mount that went away. Callers read `None` as "I know nothing about this", which is the
/// safe direction: it re-scans rather than trusting something stale.
pub fn jar_stamp(path: &Path) -> Option<JarStamp> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime_secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        // A pre-epoch mtime is nonsense, but not a reason to fail: it simply never matches a
        // real one, so the entry is re-computed every time rather than served wrongly.
        .unwrap_or(0);
    Some(JarStamp { mtime_secs, len: meta.len() })
}

/// The **classpath epoch**: one number standing for "this JDK, and these jars, with these
/// contents". Everything cached against the classpath keys off it, so one comparison
/// answers whether any of it can still be served.
///
/// Order-independent — the jar *set* defines it, not the order Maven happened to resolve
/// them in, or an unchanged project would get a new epoch on a reordered resolve and throw
/// away a cache that was perfectly good.
///
/// A jar that cannot be stat'd contributes its path and a marker, not nothing: a
/// dependency that has just disappeared is a real change, and reading it as "no
/// information" would leave the epoch unmoved.
pub fn classpath_epoch(jdk: &str, jars: &[String]) -> u64 {
    let mut stamped: Vec<String> = jars
        .iter()
        .map(|jar| match jar_stamp(Path::new(jar)) {
            Some(s) => format!("{jar}\u{1}{}\u{1}{}", s.mtime_secs, s.len),
            None => format!("{jar}\u{1}absent"),
        })
        .collect();
    stamped.sort_unstable();

    let mut seed = format!("{jdk}\0");
    for entry in &stamped {
        seed.push_str(entry);
        seed.push('\0');
    }
    bennu_intel::prelude::source_hash(&seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_jar(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("bennu-stamp-{name}"));
        let mut f = std::fs::File::create(&path).expect("create temp jar");
        f.write_all(bytes).expect("write temp jar");
        path
    }

    #[test]
    fn a_missing_jar_has_no_stamp() {
        assert!(jar_stamp(Path::new("/does/not/exist.jar")).is_none());
    }

    /// The whole point: same path, different contents — the case a path-keyed or
    /// coordinate-keyed epoch cannot see, and the one that happens every time somebody
    /// reinstalls the module they are working on.
    #[test]
    fn a_jar_rewritten_in_place_changes_the_epoch() {
        let jar = temp_jar("inplace.jar", b"first contents");
        let jars = vec![jar.to_string_lossy().to_string()];
        let before = classpath_epoch("17", &jars);

        let mut f = std::fs::File::create(&jar).expect("rewrite");
        f.write_all(b"second contents, a different length").expect("rewrite");
        drop(f);

        assert_ne!(before, classpath_epoch("17", &jars), "same path, different bytes");
        let _ = std::fs::remove_file(jar);
    }

    /// An unchanged project must keep its epoch across a reordered resolve, or the cache is
    /// thrown away for nothing.
    #[test]
    fn the_epoch_is_order_independent() {
        let a = temp_jar("order-a.jar", b"a");
        let b = temp_jar("order-b.jar", b"bb");
        let (pa, pb) = (a.to_string_lossy().to_string(), b.to_string_lossy().to_string());
        assert_eq!(
            classpath_epoch("17", &[pa.clone(), pb.clone()]),
            classpath_epoch("17", &[pb, pa]),
        );
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn the_jdk_level_is_part_of_it() {
        assert_ne!(classpath_epoch("8", &[]), classpath_epoch("17", &[]));
    }

    /// A dependency vanishing is a change, not an absence of information.
    #[test]
    fn a_disappeared_jar_still_moves_the_epoch() {
        let present = classpath_epoch("17", &[]);
        let absent = classpath_epoch("17", &["/does/not/exist.jar".to_string()]);
        assert_ne!(present, absent);
    }
}
