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

use std::collections::{HashMap, HashSet};
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

/// The per-module file `dependency:build-classpath` writes its classpath into. **Relative** on
/// purpose — see [`resolve_maven_classpath`].
const OUTPUT_FILE_NAME: &str = "bennu-classpath.txt";

/// Run `mvn dependency:build-classpath` for the project rooted at `project_dir`
/// (must contain a `pom.xml`) and collect the resolved dependency classpath.
///
/// Maven writes the classpath (an OS-separated list of jar paths) to a file under each module's
/// `target/`; we read those files rather than parse `-q` stdout, so log noise is irrelevant. A
/// non-zero Maven exit is **not** fatal on its own: as long as an output file was written (partial
/// resolution), it is read and its entries split into existing jars vs
/// [`MavenClasspath::unresolved`].
///
/// ## Multi-module: one file per module, then the union
///
/// `mdep.outputFile` used to be passed as an **absolute** path, and that quietly broke every
/// multi-module project. `build-classpath` runs once per module of the reactor, and every module
/// wrote to the *same* absolute file — so each overwrote the previous one and what survived was
/// whichever module Maven happened to build last. Worse, a reactor root is usually `<packaging>pom`
/// with no dependencies of its own, so the "resolved classpath" could end up essentially empty.
/// Opening a class in any other module then found none of its dependencies, and every library type
/// in it was reported unresolvable — thousands of errors on a project that compiles.
///
/// Passing a **relative** name makes Maven resolve it per-module, so each writes into its own
/// `target/`. We then read every file the run produced and take the **union**, deduplicated: the
/// index serves one project, and a type is either on some module's classpath or nowhere.
///
/// Stale files are removed across the whole tree first, so a module Maven fails on can't contribute
/// last session's answer.
///
/// A sibling module's own artifact may appear on another module's classpath — as `target/classes` (a
/// directory) or as its jar in `~/.m2`. Either is harmless: a directory fails to open as a jar and is
/// skipped, and the module's types are indexed from source anyway, which is the better tier.
pub fn resolve_maven_classpath(
    project_dir: &Path,
    opts: &MavenResolveOpts,
) -> Result<MavenClasspath, String> {
    let pom = project_dir.join("pom.xml");
    if !pom.is_file() {
        return Err(format!("no pom.xml in {}", project_dir.display()));
    }

    // Best-effort: clear every stale output under the tree so a module whose resolve fails this run
    // can't have last run's file read as a success.
    for stale in find_output_files(project_dir) {
        let _ = fs::remove_file(stale);
    }

    let mut cmd = Command::new(&opts.mvn_path);
    cmd.current_dir(project_dir)
        .arg("-q")
        .arg("dependency:build-classpath")
        // RELATIVE: resolved against each module's own basedir, so a reactor writes one file per
        // module instead of N modules racing to overwrite one path.
        .arg(format!("-Dmdep.outputFile=target/{OUTPUT_FILE_NAME}"))
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

    // Read every file the run produced, even on a non-zero exit: build-classpath commonly writes
    // the deps it *could* resolve before failing on a private-repo one.
    let produced = find_output_files(project_dir);
    if produced.is_empty() {
        // Nothing written anywhere → surface a readable reason from the stderr tail.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr.lines().rev().take(6).collect::<Vec<_>>().join(" | ");
        return Err(format!(
            "mvn build-classpath produced no output file (exit {:?}). stderr tail: {tail}",
            output.status.code()
        ));
    }

    let (jars, unresolved) = classify_entries(union_entries(&produced));

    Ok(MavenClasspath { jars, unresolved, mvn_ok })
}

/// The deduplicated union of the classpath entries written in `files`, in first-seen order.
///
/// One `dependency:build-classpath` run over a reactor writes one file per module, and the same
/// third-party jar appears in most of them. Deduplicating here — *before* [`classify_entries`] pays
/// one `stat` per entry — keeps a wide reactor from restat'ing the same jar dozens of times. The order
/// is first-seen-stable because the resolver's decode memo is keyed on the jar set.
///
/// An unreadable file is skipped rather than failing the union: a module whose file we can't read
/// costs its own deps, not everybody else's.
fn union_entries(files: &[PathBuf]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for file in files {
        let Ok(raw) = fs::read_to_string(file) else { continue };
        for entry in split_entries(&raw) {
            let entry = entry.trim().to_string();
            if !entry.is_empty() && seen.insert(entry.clone()) {
                out.push(entry);
            }
        }
    }
    out
}

/// Every `*/target/bennu-classpath.txt` under `root` (the root's own included), for a reactor of any
/// nesting depth.
///
/// A bounded walk that only ever descends into a directory that could hold a module: a `target/` is
/// entered just to read the file, and the usual noise dirs are skipped. Depth-capped because a
/// module tree is shallow by construction and an unbounded walk of a large repo to find a handful of
/// files would be the wrong trade.
fn find_output_files(root: &Path) -> Vec<PathBuf> {
    /// Deep enough for `root/group/subgroup/module/target/file`; deeper reactors are vanishingly rare.
    const MAX_DEPTH: usize = 6;
    let mut out = Vec::new();
    collect_output_files(root, MAX_DEPTH, &mut out);
    out.sort(); // deterministic union order regardless of filesystem enumeration
    out
}

fn collect_output_files(dir: &Path, depth_left: usize, out: &mut Vec<PathBuf>) {
    let candidate = dir.join("target").join(OUTPUT_FILE_NAME);
    if candidate.is_file() {
        out.push(candidate);
    }
    if depth_left == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // `target` is handled by the `candidate` probe above; the rest is noise a module never hides in.
        if matches!(name.as_ref(), "target" | ".git" | "node_modules" | ".idea" | "src") {
            continue;
        }
        collect_output_files(&entry.path(), depth_left - 1, out);
    }
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
    classify_entries(split_entries(raw))
}

/// Partition classpath entries into "exists on disk" (openable as a [`JarSource`]) and "doesn't"
/// (private-repo / unresolved deps, kept for reporting). Blank entries are dropped.
///
/// Split out from [`split_classpath`] so the multi-module union can dedup entries before paying one
/// `stat` each, while both paths still classify identically.
fn classify_entries<I: IntoIterator<Item = String>>(entries: I) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut jars = Vec::new();
    let mut unresolved = Vec::new();
    for entry in entries {
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

    // ── Multi-module discovery + union (no mvn needed) ────────────────────────

    /// Write `text` to `path`, creating parents.
    fn seed(path: &Path, text: &str) {
        let _ = fs::create_dir_all(path.parent().unwrap());
        fs::write(path, text).unwrap();
    }

    /// A reactor fixture: the root plus two modules (one nested) each carrying an output file, plus
    /// two decoys under directories the walk must not enter.
    fn reactor_fixture(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("bennu-reactor-{tag}"));
        let _ = fs::remove_dir_all(&root);
        seed(&root.join("pom.xml"), "<project/>");
        seed(&root.join("target").join(OUTPUT_FILE_NAME), "/m2/shared.jar:/m2/root.jar");
        seed(&root.join("api/target").join(OUTPUT_FILE_NAME), "/m2/shared.jar:/m2/api.jar");
        seed(&root.join("group/impl/target").join(OUTPUT_FILE_NAME), "/m2/impl.jar");
        // Decoys: a `target` inside `src` (skipped dir) and one under `.git`.
        seed(&root.join("src/main/java/target").join(OUTPUT_FILE_NAME), "/m2/decoy-src.jar");
        seed(&root.join(".git/x/target").join(OUTPUT_FILE_NAME), "/m2/decoy-git.jar");
        root
    }

    /// The bug this fixes: `dependency:build-classpath` runs once per reactor module, so the resolve
    /// has to read *every* module's file — reading one meant a module's deps were simply absent and
    /// every library type in it was unresolvable.
    #[test]
    fn find_output_files_collects_every_module() {
        let root = reactor_fixture("find");
        let found = find_output_files(&root);
        assert_eq!(found.len(), 3, "root + api + group/impl: {found:?}");
        assert!(found.windows(2).all(|w| w[0] <= w[1]), "sorted for a stable union: {found:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_output_files_skips_src_and_noise_dirs() {
        let root = reactor_fixture("noise");
        let found = find_output_files(&root);
        let joined = found.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" ");
        assert!(!joined.contains("src"), "a target under src/ is not a module: {joined}");
        assert!(!joined.contains(".git"), "{joined}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn union_deduplicates_the_jar_shared_by_two_modules() {
        let root = reactor_fixture("union");
        let entries = union_entries(&find_output_files(&root));
        assert_eq!(
            entries.iter().filter(|e| e.ends_with("shared.jar")).count(),
            1,
            "root and api both list it: {entries:?}"
        );
        for expected in ["/m2/root.jar", "/m2/api.jar", "/m2/impl.jar"] {
            assert!(entries.iter().any(|e| e == expected), "missing {expected}: {entries:?}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn union_of_nothing_is_empty() {
        assert!(union_entries(&[]).is_empty());
        // A path that doesn't exist is skipped, not fatal.
        assert!(union_entries(&[PathBuf::from("/definitely/missing/bennu-classpath.txt")]).is_empty());
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
    ///
    /// The project comes from `BENNU_TEST_MAVEN_PROJECT` and the JDK from
    /// `BENNU_TEST_JAVA_HOME`, because a checkout path is one machine's: hard-coded,
    /// this skipped silently everywhere else and looked like a passing test.
    #[test]
    fn maven_resolve_real_project_when_available() {
        let Ok(project) = std::env::var("BENNU_TEST_MAVEN_PROJECT") else {
            eprintln!("SKIP maven_resolve: BENNU_TEST_MAVEN_PROJECT not set");
            return;
        };
        let project = std::path::PathBuf::from(project);
        let project = project.as_path();
        if !project.join("pom.xml").is_file() {
            eprintln!("SKIP maven_resolve: no pom.xml at BENNU_TEST_MAVEN_PROJECT");
            return;
        }
        // Only attempt when a Maven launcher is discoverable — `mvn` on PATH, or
        // one named outright.
        let launcher = std::env::var("BENNU_TEST_MVN").unwrap_or_else(|_| "mvn".to_string());
        let mvn = [launcher.as_str(), "mvn"]
            .into_iter()
            .find(|p| *p == "mvn" || Path::new(p).is_file());
        let Some(mvn) = mvn else {
            eprintln!("SKIP maven_resolve: no mvn");
            return;
        };
        let mut opts = MavenResolveOpts::with_mvn(mvn);
        if let Ok(java_home) = std::env::var("BENNU_TEST_JAVA_HOME") {
            opts = opts.java_home(java_home);
        }

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
