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
//! Non-fatal by construction: a project with no `pom.xml`, no resolvable dep jars, or a failed Maven
//! resolve leaves the resolver on JDK + project exactly as before. But "non-fatal" is not the same as
//! "fine": for a *Maven* project a missing dependency tier means every library type reads as "cannot
//! resolve", so [`DepOutcome`] separates "doesn't apply" from "failed, and here's why" and the caller
//! tells the user about the second.

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

/// What resolving the dependency tier produced. The three cases are genuinely different to the user,
/// which a bare `Option` conflated: a Cargo or plain-source project simply has no Maven tier, whereas a
/// `pom.xml` project that ends up with zero dependency jars is *broken* — every library type in it
/// will read as "cannot resolve" — and the reason has to reach the user, not just stderr.
pub enum DepOutcome {
    /// No `pom.xml` — the Maven dependency tier doesn't apply. Silent, and not a problem.
    NotApplicable,
    /// The tier is ready.
    Resolved(DepClasspath),
    /// A Maven project whose dependencies could NOT be resolved; the string is a user-facing reason.
    Failed(String),
}

/// Resolve the project's dependency jars (from the on-disk list cache when fresh, else via Maven) and
/// build the dependency tier. See [`DepOutcome`] — the caller builds a JDK-only provider for anything
/// other than [`DepOutcome::Resolved`], and surfaces the reason when it's a failure.
pub fn resolve_dep_classpath(root: &Path, jdk_version: &str) -> DepOutcome {
    if !root.join("pom.xml").is_file() {
        return DepOutcome::NotApplicable;
    }
    let Some(pom_mtime) = poms_mtime(root) else {
        return DepOutcome::Failed("the project's pom.xml could not be read".to_string());
    };

    // Fresh cached jar list → skip Maven entirely; else resolve once and persist the list.
    let jars = match load_list(root, pom_mtime) {
        Some(jars) => jars,
        None => match resolve_via_maven(root, jdk_version) {
            Ok(jars) => {
                save_list(root, pom_mtime, &jars);
                jars
            }
            Err(reason) => return DepOutcome::Failed(reason),
        },
    };
    if jars.is_empty() {
        return DepOutcome::Failed("no dependency jars resolved".to_string());
    }

    let paths: Vec<PathBuf> = jars.iter().map(PathBuf::from).collect();
    let source: Box<dyn ClassSource> = Box::new(source_from_jars(&paths));
    let memo_path = memo_path_for(root, &jars);
    DepOutcome::Resolved(DepClasspath { source, memo_path, jars })
}

/// Run Maven's `dependency:build-classpath` (offline, pointed at the project's JDK) and return the
/// resolved jar paths as strings. `Err` carries a short user-facing reason — a "0 jars" state has to be
/// diagnosable from the UI, not only from the process's stderr.
fn resolve_via_maven(root: &Path, jdk_version: &str) -> Result<Vec<String>, String> {
    let mut opts = MavenResolveOpts::default(); // offline
    // Resolve the REAL launcher: on Windows Maven ships `mvn.cmd`, and a bare `Command::new("mvn")`
    // only finds `mvn.exe` — so `"mvn"` silently fails to spawn (this is why deps showed 0 jars).
    opts.mvn_path = find_mvn_launcher(root);
    if let Some(jh) = find_jdk_home(jdk_version) {
        opts.java_home = Some(jh);
    }
    match resolve_maven_classpath(root, &opts) {
        Ok(cp) if !cp.jars.is_empty() => {
            Ok(cp.jars.iter().map(|p| p.display().to_string()).collect())
        }
        Ok(cp) => {
            eprintln!(
                "bennu-be: Maven resolved 0 dependency jars for {} ({} unresolved entries) — index \
                 runs JDK-only. Build the project once so its deps land in ~/.m2 (offline resolve).",
                root.display(),
                cp.unresolved.len()
            );
            Err(format!(
                "Maven resolved no dependency jars ({} entries missing from ~/.m2). Build the \
                 project once so its dependencies are downloaded — the resolve runs offline.",
                cp.unresolved.len()
            ))
        }
        Err(e) => {
            eprintln!(
                "bennu-be: Maven dependency resolve failed for {} ({e}) — index runs JDK-only. \
                 Is Maven installed / on PATH? (launcher tried: {})",
                root.display(),
                opts.mvn_path
            );
            Err(format!("Maven could not be run ({}): {e}", opts.mvn_path))
        }
    }
}

/// The Maven launcher for `root`, as an absolute path where one can be found.
///
/// Four sources, in order:
///   1. **`PATH`** — preferring the Windows batch launchers (`mvn.cmd`/`mvn.bat`), because a bare
///      `Command::new("mvn")` only locates `mvn.exe` and a Maven install that ships only `mvn.cmd`
///      (the norm on Windows) would never spawn.
///   2. **Well-known install directories** ([`mvn_bin_dirs`]) — a desktop app launched from Finder /
///      the Dock / a desktop launcher inherits the system's minimal environment, *not* the user's
///      shell profile, so a Homebrew (`/opt/homebrew/bin`), MacPorts or SDKMAN Maven is invisible to
///      the `PATH` scan above even though `mvn` works fine in a terminal. That made the dependency
///      tier fail instantly, and the only trace was a line on stderr.
///   3. **The project's own Maven wrapper** (`mvnw`) — last, because it works even when Maven isn't
///      installed at all, but a cold wrapper *downloads* its distribution; an installed `mvn` is the
///      better answer whenever there is one.
///   4. The bare `"mvn"`, letting the child process resolve it.
pub(crate) fn find_mvn_launcher(root: &Path) -> String {
    let names: &[&str] =
        if cfg!(windows) { &["mvn.cmd", "mvn.bat", "mvn.exe", "mvn"] } else { &["mvn"] };
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            if let Some(hit) = first_launcher(&dir, names) {
                return hit;
            }
        }
    }
    for dir in mvn_bin_dirs() {
        if let Some(hit) = first_launcher(&dir, names) {
            return hit;
        }
    }
    let wrapper = root.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
    if wrapper.is_file() {
        return wrapper.display().to_string();
    }
    "mvn".to_string()
}

/// The first of `names` that exists as a file directly in `dir`.
fn first_launcher(dir: &Path, names: &[&str]) -> Option<String> {
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file()).map(|p| p.display().to_string())
}

/// Directories that hold a `mvn` launcher on a typical developer machine, for when `PATH` doesn't
/// carry it (see [`find_mvn_launcher`]). A directory that doesn't exist costs one failed `is_file`.
fn mvn_bin_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    // An explicit Maven home wins over any guess.
    for var in ["MAVEN_HOME", "M2_HOME"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                out.push(PathBuf::from(v).join("bin"));
            }
        }
    }
    out.push(PathBuf::from("/opt/homebrew/bin")); // Homebrew, Apple silicon
    out.push(PathBuf::from("/usr/local/bin")); // Homebrew on Intel, and manual installs
    out.push(PathBuf::from("/opt/local/bin")); // MacPorts
    out.push(PathBuf::from("/usr/share/maven/bin")); // Debian / Ubuntu package
    out.push(PathBuf::from("/opt/maven/bin"));
    if let Some(home) = bennu_classpath::prelude::user_home() {
        out.push(home.join(".sdkman/candidates/maven/current/bin"));
    }
    out
}

/// Drop the persisted jar-list cache for `root`, so the next [`resolve_dep_classpath`] re-runs Maven
/// instead of serving the recorded list.
///
/// Called by a **manual** index rebuild, which the user reaches for precisely when the dependency
/// tier looks wrong. Without this the rebuild could never recover from a bad list: the cache is keyed
/// on pom mtimes, so nothing the user could do short of editing a pom (or finding the cache directory)
/// would invalidate it.
pub(crate) fn clear_list_cache(root: &Path) {
    let _ = std::fs::remove_file(list_cache_path(root));
}

// ── on-disk jar-list cache (keyed by pom mtime) ─────────────────────────────────

/// The freshness stamp for the whole project's poms: the **newest** `pom.xml` mtime under `root`.
///
/// Not just the root pom, and that is the fix: in a multi-module project the dependencies live in the
/// MODULE poms, so keying the cache on the root's mtime alone meant adding a dependency to a module
/// never invalidated anything — the stale jar list was served forever and the new library stayed
/// unresolvable until the root pom happened to be touched.
///
/// The max (rather than a hash of all of them) is enough: any edit to any pom moves it forward. Same
/// bounded walk the classpath collector uses, so a deep reactor is covered and a large repo isn't
/// crawled. `None` when no pom is readable at all.
fn poms_mtime(root: &Path) -> Option<u64> {
    /// Matches the classpath collector's depth — the same reactor shape.
    const MAX_DEPTH: usize = 6;
    let mut newest: Option<u64> = None;
    collect_pom_mtimes(root, MAX_DEPTH, &mut newest);
    newest
}

fn collect_pom_mtimes(dir: &Path, depth_left: usize, newest: &mut Option<u64>) {
    if let Some(secs) = std::fs::metadata(dir.join("pom.xml"))
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
    {
        if newest.is_none_or(|cur| secs > cur) {
            *newest = Some(secs);
        }
    }
    if depth_left == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), "target" | ".git" | "node_modules" | ".idea" | "src") {
            continue;
        }
        collect_pom_mtimes(&entry.path(), depth_left - 1, newest);
    }
}

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

    /// A project with no `pom.xml` has no Maven tier — and that must stay SILENT (a Cargo or plain
    /// source project isn't broken), which is the distinction `DepOutcome` exists to keep.
    #[test]
    fn no_pom_is_not_applicable_rather_than_a_failure() {
        let dir = std::env::temp_dir().join(format!("bennu-deps-nopom-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(matches!(resolve_dep_classpath(&dir, "1.8"), DepOutcome::NotApplicable));
    }

    /// The Maven launcher must never come back empty: the bare `"mvn"` is the documented last resort,
    /// so a caller always has something to spawn (and a spawn error to report).
    #[test]
    fn mvn_launcher_always_yields_something() {
        let dir = std::env::temp_dir().join(format!("bennu-deps-mvn-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(!find_mvn_launcher(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With no Maven anywhere on PATH or in a well-known directory, the project's own wrapper is used
    /// — the case of a machine that has never had Maven installed.
    #[test]
    fn mvn_wrapper_is_used_when_present() {
        // Only meaningful when the host has no `mvn` of its own; skip rather than assert a false thing.
        let bare = std::env::temp_dir().join(format!("bennu-deps-bare-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&bare);
        if find_mvn_launcher(&bare) != "mvn" {
            return; // this machine has a real Maven — the wrapper is correctly not preferred
        }
        let wrapper = bare.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
        std::fs::write(&wrapper, "#!/bin/sh\n").unwrap();
        assert_eq!(find_mvn_launcher(&bare), wrapper.display().to_string());
        let _ = std::fs::remove_dir_all(&bare);
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
