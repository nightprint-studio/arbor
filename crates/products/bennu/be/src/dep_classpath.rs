//! Dependency-classpath sourcing for the validation / completion index.
//!
//! Resolves a Maven project's `~/.m2` dependency jars and exposes them as a `ClassSource`, so the
//! resolver's dependency tier (`bennu_query`'s `ClasspathIndex`) can decode library types (Spring,
//! servlet, Hibernate, Struts, …) — not just the JDK + project sources.
//!
//! Two levels of caching keep this cheap:
//!   * the resolved **jar LIST** is persisted to disk keyed by the pom's mtime, so
//!     `mvn dependency:build-classpath` (seconds) runs at most once per pom across sessions;
//!   * the decoded **members** of each dep class are memoized (lazily, on first touch) to a
//!     per-project file by `JdkMemberIndex::persistent` — keyed by the resolved jar set, so a
//!     changed dependency set starts a fresh memo and never serves a stale decode.
//!
//! Non-fatal by construction: a project with no `pom.xml`, no resolvable dep jars, or a failed
//! Maven resolve returns `None`, and the resolver degrades to JDK + project exactly as before.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use bennu_classpath::prelude::{
    find_jdk_home, resolve_maven_classpath, source_from_jars, ClassSource, MavenResolveOpts,
};

/// The dependency tier for a project: an opened dep-jars source + the per-project memo path its
/// decoded members persist to. Handed to `NativeJavaProvider::for_project`.
pub struct DepClasspath {
    /// The dependency jars behind one `ClassSource` (JDK-free — the JDK is a separate tier).
    pub source: Box<dyn ClassSource>,
    /// The per-project, per-jar-set memo file the decoded dep members persist to.
    pub memo_path: PathBuf,
    /// The resolved dep jar paths (absolute) — surfaced to the index inspector's Jars list, so
    /// the count reflects exactly what the resolver loaded (not the Build's `target/` artifact).
    pub jars: Vec<String>,
}

/// Resolve the project's dependency jars (from the on-disk list cache when fresh, else via Maven) and
/// build the dependency tier. `None` when the project has no `pom.xml`, no resolvable dep jars, or the
/// resolve failed — the caller then builds a JDK-only provider.
pub fn resolve_dep_classpath(root: &Path, jdk_version: &str) -> Option<DepClasspath> {
    let pom = root.join("pom.xml");
    let pom_mtime = std::fs::metadata(&pom)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();

    // Fresh cached jar list → skip Maven entirely; else resolve once and persist the list.
    let jars = match load_list(root, pom_mtime) {
        Some(jars) => jars,
        None => {
            let jars = resolve_via_maven(root, jdk_version)?;
            save_list(root, pom_mtime, &jars);
            jars
        }
    };
    if jars.is_empty() {
        return None;
    }

    let paths: Vec<PathBuf> = jars.iter().map(PathBuf::from).collect();
    let source: Box<dyn ClassSource> = Box::new(source_from_jars(&paths));
    let memo_path = memo_path_for(root, &jars);
    Some(DepClasspath { source, memo_path, jars })
}

/// Run Maven's `dependency:build-classpath` (offline, pointed at the project's JDK) and return the
/// resolved jar paths as strings. `None` on no pom / resolve failure / no jars — logging the reason
/// so a "0 jars" state in the inspector is diagnosable.
fn resolve_via_maven(root: &Path, jdk_version: &str) -> Option<Vec<String>> {
    if !root.join("pom.xml").is_file() {
        return None;
    }
    let mut opts = MavenResolveOpts::default(); // offline
    // Resolve the REAL launcher: on Windows Maven ships `mvn.cmd`, and a bare `Command::new("mvn")`
    // only finds `mvn.exe` — so `"mvn"` silently fails to spawn (this is why deps showed 0 jars).
    opts.mvn_path = find_mvn_launcher();
    if let Some(jh) = find_jdk_home(jdk_version) {
        opts.java_home = Some(jh);
    }
    match resolve_maven_classpath(root, &opts) {
        Ok(cp) if !cp.jars.is_empty() => {
            Some(cp.jars.iter().map(|p| p.display().to_string()).collect())
        }
        Ok(cp) => {
            eprintln!(
                "bennu-be: Maven resolved 0 dependency jars for {} ({} unresolved entries) — index \
                 runs JDK-only. Build the project once so its deps land in ~/.m2 (offline resolve).",
                root.display(),
                cp.unresolved.len()
            );
            None
        }
        Err(e) => {
            eprintln!(
                "bennu-be: Maven dependency resolve failed for {} ({e}) — index runs JDK-only. \
                 Is Maven installed / on PATH? (launcher tried: {})",
                root.display(),
                opts.mvn_path
            );
            None
        }
    }
}

/// The Maven launcher as an absolute path, preferring the Windows batch launchers
/// (`mvn.cmd`/`mvn.bat`) — a bare `Command::new("mvn")` only locates `mvn.exe`, so a Maven install
/// that ships only `mvn.cmd` (the norm on Windows) would never spawn. Scans `PATH`; falls back to
/// the bare `"mvn"` (correct on Unix, or when a real `mvn`/`mvn.exe` is on PATH).
pub(crate) fn find_mvn_launcher() -> String {
    let names: &[&str] =
        if cfg!(windows) { &["mvn.cmd", "mvn.bat", "mvn.exe", "mvn"] } else { &["mvn"] };
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            for &name in names {
                let cand = dir.join(name);
                if cand.is_file() {
                    return cand.display().to_string();
                }
            }
        }
    }
    "mvn".to_string()
}

// ── on-disk jar-list cache (keyed by pom mtime) ─────────────────────────────────

/// `bennu_data_dir()/dep-classpath/<root-hash>.json` — the persisted resolved jar list for `root`.
fn list_cache_path(root: &Path) -> PathBuf {
    arbor_core::prelude::bennu_data_dir()
        .join("dep-classpath")
        .join(format!("{}.json", fnv(root.to_string_lossy().as_bytes())))
}

/// The cached jar list for `root`, but only when its recorded pom mtime matches `pom_mtime` (else the
/// deps may have changed → re-resolve). `None` on a missing / stale / unreadable cache.
fn load_list(root: &Path, pom_mtime: u64) -> Option<Vec<String>> {
    load_list_from(&list_cache_path(root), pom_mtime)
}

/// Persist the resolved jar list for `root` with its pom mtime (best-effort — a write failure just
/// means the next session re-runs Maven).
fn save_list(root: &Path, pom_mtime: u64, jars: &[String]) {
    save_list_to(&list_cache_path(root), pom_mtime, jars);
}

/// The pure read of a jar-list cache FILE (path-injectable, so the mtime-gating is unit-testable
/// without the profile-scoped `bennu_data_dir`).
fn load_list_from(path: &Path, pom_mtime: u64) -> Option<Vec<String>> {
    let bytes = std::fs::read(path).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if v.get("pom_mtime").and_then(|m| m.as_u64()) != Some(pom_mtime) {
        return None;
    }
    let jars = v.get("jars")?.as_array()?;
    Some(jars.iter().filter_map(|j| j.as_str().map(str::to_string)).collect())
}

/// The pure write of a jar-list cache FILE (path-injectable, best-effort).
fn save_list_to(path: &Path, pom_mtime: u64, jars: &[String]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let value = serde_json::json!({ "pom_mtime": pom_mtime, "jars": jars });
    if let Ok(bytes) = serde_json::to_vec(&value) {
        let _ = std::fs::write(path, bytes);
    }
}

/// `bennu_data_dir()/dep-index/<root-and-jarset-hash>.json` — the per-project decoded-members memo.
/// Keyed by the project root AND the (sorted) resolved jar set, so a changed dependency set starts a
/// fresh memo file rather than serving a stale decode of a since-removed jar.
fn memo_path_for(root: &Path, jars: &[String]) -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("dep-index").join(memo_file_name(root, jars))
}

/// The pure `<hash>.json` file name for a project's dependency memo — hashes the root plus the
/// SORTED jar set, so jar order doesn't matter but a changed set gives a fresh name.
fn memo_file_name(root: &Path, jars: &[String]) -> String {
    let mut hash = fnv_u64(root.to_string_lossy().as_bytes());
    let mut sorted: Vec<&String> = jars.iter().collect();
    sorted.sort();
    for j in sorted {
        hash = fnv_mix(hash, j.as_bytes());
    }
    format!("{hash:016x}.json")
}

// ── tiny FNV-1a hashing (filesystem-safe cache keys; mirrors index_service) ──────

fn fnv_u64(bytes: &[u8]) -> u64 {
    fnv_mix(0xcbf29ce484222325, bytes)
}

fn fnv_mix(mut hash: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn fnv(bytes: &[u8]) -> String {
    format!("{:016x}", fnv_u64(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_pom_resolves_to_none() {
        let dir = std::env::temp_dir().join(format!("bennu-deps-nopom-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(resolve_dep_classpath(&dir, "1.8").is_none());
    }

    #[test]
    fn memo_name_changes_with_jar_set() {
        let root = Path::new("C:/proj");
        let a = memo_file_name(root, &["x.jar".to_string(), "y.jar".to_string()]);
        // Same jars, different order → SAME memo name (sorted before hashing).
        let a2 = memo_file_name(root, &["y.jar".to_string(), "x.jar".to_string()]);
        assert_eq!(a, a2);
        // A different jar set → a different memo name.
        let b = memo_file_name(root, &["x.jar".to_string(), "z.jar".to_string()]);
        assert_ne!(a, b);
        // A different root → a different memo name.
        assert_ne!(a, memo_file_name(Path::new("C:/other"), &["x.jar".to_string(), "y.jar".to_string()]));
    }

    #[test]
    fn list_cache_roundtrips_and_respects_mtime() {
        let dir = std::env::temp_dir().join(format!("bennu-deps-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("list.json");
        let jars = vec!["a.jar".to_string(), "b.jar".to_string()];
        save_list_to(&path, 42, &jars);
        assert_eq!(load_list_from(&path, 42), Some(jars));
        // A different pom mtime invalidates the cache.
        assert_eq!(load_list_from(&path, 43), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
