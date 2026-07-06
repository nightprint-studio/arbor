//! [`resolve_maven_classpath`] — source a Maven project's dependency-jar bytecode
//! from `~/.m2` so member-access completion reaches framework/library types
//! (Spring, servlet, Hibernate, Struts…), not just the JDK + project sources.
//!
//! This layers in **behind the same** [`ClassSource`](crate::source::ClassSource) /
//! [`MultiSource`](crate::jdk::MultiSource) / member-index API as the JDK
//! bootclasspath (docs §10: "dep jars layer in behind `ClassSource`"). The JDK path in
//! [`crate::jdk`] is untouched — a project with no resolvable deps degrades exactly to
//! the JDK-only behavior.
//!
//! ## How a project becomes a dep-augmented member index
//!
//! ```no_run
//! use bennu_classpath::prelude::*;
//! use std::path::Path;
//!
//! // 1. The JDK bootclasspath for the project's language level (Phase 1).
//! let jdk = resolve_jdk_classpath("1.8").unwrap();
//!
//! // 2. The project's dependency jars, resolved via Maven and cached by pom mtime.
//! let mut cache = MavenClasspathCache::new();
//! let deps = cache
//!     .get(Path::new("/path/to/project"), &MavenResolveOpts::default())
//!     .unwrap();
//!
//! // 3. Layer deps behind the JDK into one source, then a member index.
//! let source = deps.augment(jdk);                 // JDK probed first, then dep jars
//! let index = SourceMemberIndex::new(source);
//! let members = index.members_of("javax/servlet/http/HttpServletRequest"); // now Some(_)
//! ```
//!
//! ## Partial failure is non-fatal (docs §8)
//!
//! Some deps live on a private repo and won't resolve; `dependency:build-classpath`
//! may exit non-zero yet still write the classpath it *could* resolve. We always read
//! the output file, collect existing jars as sources, and record non-existent entries
//! as [`MavenClasspath::unresolved`] — a normal "unresolved" state, never a hard error
//! (only a total absence of any output file on a failed run is surfaced as `Err`).
//!
//! ## Cost & caching
//!
//! `build-classpath` shells out to Maven (seconds). [`MavenClasspathCache`] keys the
//! resolved classpath on the pom's mtime, so a re-resolve within a session is free
//! until the pom changes.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use crate::jdk::MultiSource;
use crate::source::{ClassSource, JarSource};

/// Options for running Maven's `dependency:build-classpath`.
#[derive(Debug, Clone)]
pub struct MavenResolveOpts {
    /// The Maven launcher: `"mvn"` (found on `PATH`) or an absolute path to
    /// `mvn`/`mvn.cmd`.
    pub mvn_path: String,
    /// `JAVA_HOME` to export for the Maven child, so the project's JDK (e.g. JDK 8) is
    /// used regardless of the ambient one. `None` inherits the environment.
    pub java_home: Option<PathBuf>,
    /// Run Maven **offline** (`-o`): resolve only from the local `~/.m2` cache — fast
    /// and deterministic, no network. Defaults to `true`; set `false` for a first-time
    /// resolve that may need to download.
    pub offline: bool,
}

impl Default for MavenResolveOpts {
    fn default() -> Self {
        Self { mvn_path: "mvn".to_string(), java_home: None, offline: true }
    }
}

impl MavenResolveOpts {
    /// Start from defaults with an explicit Maven launcher.
    pub fn with_mvn(mvn_path: impl Into<String>) -> Self {
        Self { mvn_path: mvn_path.into(), ..Self::default() }
    }

    /// Set the `JAVA_HOME` exported to the Maven child (builder-style).
    pub fn java_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.java_home = Some(home.into());
        self
    }

    /// Toggle offline (`-o`) resolution (builder-style).
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }
}

/// The outcome of resolving a project's dependency classpath.
#[derive(Debug, Clone)]
pub struct MavenClasspath {
    /// Absolute paths to dep jars that exist on disk (openable as [`JarSource`]).
    pub jars: Vec<PathBuf>,
    /// Classpath entries Maven emitted that do NOT exist on disk (private-repo /
    /// unresolved deps). Non-fatal — kept for reporting.
    pub unresolved: Vec<PathBuf>,
    /// Whether `mvn` exited 0. `false` means partial (see [`unresolved`](Self::unresolved))
    /// — the resolved jars are still usable.
    pub mvn_ok: bool,
}

impl MavenClasspath {
    /// Count of dep jars that exist on disk and can be sourced.
    pub fn resolved_count(&self) -> usize {
        self.jars.len()
    }

    /// Count of classpath entries Maven emitted that are missing from disk.
    pub fn unresolved_count(&self) -> usize {
        self.unresolved.len()
    }

    /// Open each resolved jar as a [`JarSource`], skipping any that fail to open (a
    /// corrupt/unsupported jar must not sink the whole classpath — same policy as the
    /// JDK ext jars in [`crate::jdk`]). Returns the sources plus the count that failed
    /// to open.
    pub fn jar_sources(&self) -> (Vec<Box<dyn ClassSource>>, usize) {
        let mut sources: Vec<Box<dyn ClassSource>> = Vec::new();
        let mut open_failures = 0usize;
        for jar in &self.jars {
            match JarSource::open(jar) {
                Ok(src) => sources.push(Box::new(src)),
                Err(_) => open_failures += 1,
            }
        }
        (sources, open_failures)
    }

    /// Layer the dep jars **behind** an existing base source (typically the JDK
    /// bootclasspath from [`resolve_jdk_classpath`](crate::jdk::resolve_jdk_classpath))
    /// into one [`MultiSource`]. The base is probed **first** (the real JDK core wins
    /// over any shaded copy bundled in a dependency), then the dep jars in classpath
    /// order. Jars that fail to open are skipped.
    pub fn augment(&self, base: Box<dyn ClassSource>) -> MultiSource {
        let (dep_sources, _) = self.jar_sources();
        let mut all: Vec<Box<dyn ClassSource>> = Vec::with_capacity(1 + dep_sources.len());
        all.push(base);
        all.extend(dep_sources);
        MultiSource::new(all)
    }

    /// The dep jars alone as a [`MultiSource`], with no JDK base — for callers that
    /// chain the JDK elsewhere or want to inspect only dependency types.
    pub fn into_source(&self) -> MultiSource {
        let (dep_sources, _) = self.jar_sources();
        MultiSource::new(dep_sources)
    }
}

/// Run `mvn dependency:build-classpath` for the project rooted at `project_dir`
/// (must contain a `pom.xml`) and collect the resolved dependency classpath.
///
/// Maven writes the classpath (an OS-separated list of jar paths) to a file under the
/// project's `target/`; we read that file rather than parse `-q` stdout, so log noise
/// is irrelevant. A non-zero Maven exit is **not** fatal on its own: as long as an
/// output file was written (partial resolution), it is read and its entries split into
/// existing jars vs [`MavenClasspath::unresolved`].
pub fn resolve_maven_classpath(
    project_dir: &Path,
    opts: &MavenResolveOpts,
) -> Result<MavenClasspath, String> {
    let pom = project_dir.join("pom.xml");
    if !pom.is_file() {
        return Err(format!("no pom.xml in {}", project_dir.display()));
    }

    // Write the classpath to a temp file inside the project's target dir (created by
    // the plugin if absent). Using a file avoids parsing `-q` stdout.
    let out_file = project_dir.join("target").join("bennu-classpath.txt");
    // Best-effort: remove a stale file so a total mvn failure can't be read as success.
    let _ = fs::remove_file(&out_file);

    let mut cmd = Command::new(&opts.mvn_path);
    cmd.current_dir(project_dir)
        .arg("-q")
        .arg("dependency:build-classpath")
        .arg(format!("-Dmdep.outputFile={}", out_file.display()))
        // Don't let one unresolvable artifact abort the reactor before writing.
        .arg("-Dmdep.ignoreMissing=true")
        .arg("--fail-never")
        .arg("--batch-mode");
    if opts.offline {
        cmd.arg("-o");
    }
    if let Some(jh) = &opts.java_home {
        cmd.env("JAVA_HOME", jh);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("spawn mvn ({}): {e}", opts.mvn_path))?;
    let mvn_ok = output.status.success();

    // Read the classpath file even on non-zero exit: build-classpath commonly writes
    // the deps it *could* resolve before failing on a private-repo one.
    let raw = match fs::read_to_string(&out_file) {
        Ok(s) => s,
        Err(_) if !mvn_ok => {
            // No file AND mvn failed → surface a readable reason from the stderr tail.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let tail: String = stderr.lines().rev().take(6).collect::<Vec<_>>().join(" | ");
            return Err(format!(
                "mvn build-classpath produced no output file (exit {:?}). stderr tail: {tail}",
                output.status.code()
            ));
        }
        Err(e) => return Err(format!("read {}: {e}", out_file.display())),
    };

    let (jars, unresolved) = split_classpath(&raw);
    Ok(MavenClasspath { jars, unresolved, mvn_ok })
}

/// Open a list of dependency jar paths as one [`MultiSource`], skipping any that fail to open (a
/// corrupt/unsupported jar must not sink the whole tier — same policy as [`MavenClasspath::jar_sources`]).
/// For a caller (e.g. the index service) that already knows the resolved jar paths — from a persisted
/// classpath cache — and wants a ready [`ClassSource`](crate::source::ClassSource) without re-running
/// Maven.
pub fn source_from_jars(jars: &[PathBuf]) -> MultiSource {
    let mut sources: Vec<Box<dyn ClassSource>> = Vec::new();
    for jar in jars {
        if let Ok(src) = JarSource::open(jar) {
            sources.push(Box::new(src));
        }
    }
    MultiSource::new(sources)
}

/// Split a build-classpath string into existing jars vs non-existent entries.
fn split_classpath(raw: &str) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut jars = Vec::new();
    let mut unresolved = Vec::new();
    for entry in split_entries(raw) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let p = PathBuf::from(entry);
        if p.is_file() {
            jars.push(p);
        } else {
            unresolved.push(p);
        }
    }
    (jars, unresolved)
}

/// Split a classpath string into entries. Windows uses `;` (unambiguous). A `:`
/// classpath (Unix) must NOT split a drive-letter colon (`C:\...`): a `:` is only a
/// separator when it is not the second char of a `<letter>:\` / `<letter>:/` drive
/// prefix at an entry boundary.
fn split_entries(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    if raw.contains(';') {
        return raw.split(';').map(|s| s.to_string()).collect();
    }

    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if b != b':' {
            continue;
        }
        let prev_is_letter = i >= 1 && bytes[i - 1].is_ascii_alphabetic();
        let letter_at_entry_start = i == start + 1; // entry begins "X:"
        let next_is_slash =
            i + 1 < bytes.len() && (bytes[i + 1] == b'\\' || bytes[i + 1] == b'/');
        if prev_is_letter && letter_at_entry_start && next_is_slash {
            continue; // drive letter, not a separator
        }
        out.push(raw[start..i].to_string());
        start = i + 1;
    }
    out.push(raw[start..].to_string());
    out
}

// ── caching by pom mtime ─────────────────────────────────────────────────────

/// A per-session cache of resolved Maven classpaths, keyed by the project's pom path
/// and invalidated when the pom's mtime changes. `build-classpath` costs seconds; this
/// makes a re-resolve within a session free until the pom is edited.
#[derive(Default)]
pub struct MavenClasspathCache {
    entries: HashMap<PathBuf, CacheEntry>,
}

struct CacheEntry {
    pom_mtime: SystemTime,
    classpath: MavenClasspath,
}

impl MavenClasspathCache {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Resolve (or return the cached) dependency classpath for `project_dir`.
    /// Recomputes only when the pom's mtime differs from the cached one.
    pub fn get(
        &mut self,
        project_dir: &Path,
        opts: &MavenResolveOpts,
    ) -> Result<MavenClasspath, String> {
        let pom = project_dir.join("pom.xml");
        let mtime = fs::metadata(&pom)
            .and_then(|m| m.modified())
            .map_err(|e| format!("stat {}: {e}", pom.display()))?;

        if let Some(hit) = self.entries.get(&pom) {
            if hit.pom_mtime == mtime {
                return Ok(hit.classpath.clone());
            }
        }

        let classpath = resolve_maven_classpath(project_dir, opts)?;
        self.entries
            .insert(pom.clone(), CacheEntry { pom_mtime: mtime, classpath: classpath.clone() });
        Ok(classpath)
    }

    /// Whether a fresh (mtime-valid) entry is cached for this project.
    pub fn is_cached(&self, project_dir: &Path) -> bool {
        let pom = project_dir.join("pom.xml");
        match (self.entries.get(&pom), fs::metadata(&pom).and_then(|m| m.modified())) {
            (Some(hit), Ok(mtime)) => hit.pom_mtime == mtime,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::members::MemberIndex;

    // ── Pure classpath-splitting (no mvn / no deps needed) ───────────────────

    #[test]
    fn split_windows_classpath_keeps_drive_letters() {
        let raw = r"C:\a\x.jar;C:\b\y.jar";
        let entries = split_entries(raw);
        assert_eq!(entries, vec![r"C:\a\x.jar".to_string(), r"C:\b\y.jar".to_string()]);
    }

    #[test]
    fn split_unix_classpath() {
        let raw = "/home/u/.m2/a.jar:/home/u/.m2/b.jar";
        let entries = split_entries(raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], "/home/u/.m2/a.jar");
    }

    #[test]
    fn split_unix_path_list_with_windows_drive_entries() {
        // A `:`-joined list whose entries carry Windows drive prefixes must not split
        // on the drive colon.
        let raw = r"C:\a\x.jar:C:\b\y.jar";
        let entries = split_entries(raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], r"C:\a\x.jar");
        assert_eq!(entries[1], r"C:\b\y.jar");
    }

    #[test]
    fn nonexistent_entries_go_to_unresolved() {
        let raw = concat!(r"C:\definitely\missing\nope-1.0.jar", ";", r"C:\also\gone-2.0.jar");
        let (jars, unresolved) = split_classpath(raw);
        assert!(jars.is_empty());
        assert_eq!(unresolved.len(), 2);
    }

    #[test]
    fn empty_entries_skipped() {
        let (jars, unresolved) = split_classpath(";;  ;");
        assert!(jars.is_empty());
        assert!(unresolved.is_empty());
    }

    // ── Cache semantics (no mvn: exercised via a missing-pom project) ─────────

    #[test]
    fn cache_get_errors_without_pom_and_records_nothing() {
        let mut cache = MavenClasspathCache::new();
        let dir = std::env::temp_dir().join("bennu-no-pom-xyz");
        let _ = fs::create_dir_all(&dir);
        let opts = MavenResolveOpts::default();
        // No pom.xml → stat fails → Err, and nothing is cached.
        assert!(cache.get(&dir, &opts).is_err());
        assert!(!cache.is_cached(&dir));
    }

    // ── mvn-backed integration (skips gracefully when mvn/deps absent) ────────

    /// Resolve a real Maven project when `mvn` + a populated `~/.m2` are available;
    /// otherwise skip (the leaf crate must build/test on a machine without Maven).
    #[test]
    fn maven_resolve_real_project_when_available() {
        let project =
            Path::new("C:/Sviluppo/Mio/temp/disposable-projects/PortaleAppalti");
        if !project.join("pom.xml").is_file() {
            eprintln!("SKIP maven_resolve: no test project");
            return;
        }
        // Only attempt when a Maven launcher is discoverable.
        let mvn = ["C:/Sviluppo/Software/apache-maven-3.9.9/bin/mvn.cmd", "mvn"]
            .into_iter()
            .find(|p| *p == "mvn" || Path::new(p).is_file());
        let Some(mvn) = mvn else {
            eprintln!("SKIP maven_resolve: no mvn");
            return;
        };
        let opts = MavenResolveOpts::with_mvn(mvn)
            .java_home("C:/Program Files/Java/jdk8u442-b06");

        let mut cache = MavenClasspathCache::new();
        let Ok(cp) = cache.get(project, &opts) else {
            eprintln!("SKIP maven_resolve: resolve failed (mvn/deps unavailable)");
            return;
        };
        // Partial or full, we should have at least one dep jar to source.
        assert!(cp.resolved_count() > 0, "expected some resolved dep jars");
        assert!(cache.is_cached(project), "second get should be a cache hit");

        // A servlet type on the classpath must now resolve members.
        let source = cp.into_source();
        let idx = crate::members::SourceMemberIndex::new(source);
        if let Some(req) =
            idx.members_of("javax/servlet/http/HttpServletRequest")
        {
            assert!(
                req.methods.iter().any(|m| m.name == "getHeader"),
                "HttpServletRequest.getHeader should be present"
            );
        } else {
            eprintln!("note: servlet-api not in this ~/.m2; dep sourcing still exercised");
        }
    }
}
