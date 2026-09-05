//! [`resolve_jdk_classpath`] — locate an installed JDK matching the language level
//! and expose its bootclasspath as a single [`ClassSource`](crate::source::ClassSource).
//!
//! Level → container:
//!
//! - `"1.8"` / `"8"` → a JDK-8 install: `jre/lib/rt.jar` + `jre/lib/resources.jar` +
//!   every `jre/lib/ext/*.jar`, chained behind one [`MultiSource`].
//! - `"9"`+ / `"21"` → a modular JDK: `lib/modules` jimage via
//!   [`JimageSource`](crate::source::JimageSource), probing `java.base` first (the
//!   JDK core), then the common platform modules.
//!
//! JDK discovery: the user-configured extra homes, then `JAVA_HOME`, then every JDK
//! under the platform's standard install roots — Windows `Program Files` vendor dirs,
//! macOS `/Library/Java/JavaVirtualMachines` bundles, Linux `/usr/lib/jvm`, the Homebrew
//! `openjdk` formula, and the per-user version-manager / IDE locations (see
//! [`jdk_install_roots`]). Each candidate's language level is read from its `release`
//! file (`JAVA_VERSION`); the first candidate whose major version matches the
//! requested level wins. When none matches, [`resolve_jdk_classpath`] falls back to the
//! newest installed JDK (so a Java-8 project still resolves the standard library on a
//! machine that only has a modern JDK) rather than failing.
//!
//! Scope: **JDK bootclasspath only.** Dependency-jar sourcing from `~/.m2` lives in
//! [`crate::maven`], which layers those dep jars in as additional [`JarSource`]s
//! behind the same [`MultiSource`] — no change to this module's shape.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::source::{ClassSource, JarSource, JimageSource};
use crate::sources::JavaSourceZip;

/// User-configured extra JDK home directories (settings `jdk_paths`), consulted by
/// [`candidate_jdks`] on top of `JAVA_HOME` + the standard install roots — for a JDK
/// installed somewhere non-standard. Set by the be from config at startup / on save.
static EXTRA_JDK_HOMES: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// Replace the user-configured extra JDK home directories. Called by the be layer with the
/// `jdk_paths` config on startup and whenever the settings are saved.
pub fn set_extra_jdk_homes(homes: Vec<PathBuf>) {
    if let Ok(mut g) = EXTRA_JDK_HOMES.write() {
        *g = homes;
    }
}

/// A [`ClassSource`] that tries several sources in order (first hit wins). Used for
/// the JDK-8 bootclasspath (rt.jar + resources.jar + ext jars) and, behind the same
/// trait, for a dep-augmented project classpath — the JDK probed first, then the
/// `~/.m2` dependency jars ([`crate::maven::MavenClasspath::augment`]).
pub struct MultiSource {
    sources: Vec<Box<dyn ClassSource>>,
}

impl MultiSource {
    pub fn new(sources: Vec<Box<dyn ClassSource>>) -> Self {
        Self { sources }
    }
}

impl ClassSource for MultiSource {
    fn class_bytes(&self, binary_name: &str) -> Result<Option<Vec<u8>>, String> {
        for s in &self.sources {
            if let Some(bytes) = s.class_bytes(binary_name)? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }

    fn class_names(&self) -> Vec<String> {
        // Union across every chained source (rt.jar + resources + ext, or the single jimage).
        let mut out = Vec::new();
        for s in &self.sources {
            out.extend(s.class_names());
        }
        out
    }
}

/// Build a [`ClassSource`] for the JDK bootclasspath matching `version`
/// (`"1.8"`/`"8"` → rt.jar + ext + resources; `"9"`+ → jimage). Prefers an exact-major
/// JDK; when none is installed, **falls back to the newest installed JDK** rather than
/// failing. `Err` only when NO JDK is installed at all (or its container is missing).
///
/// Why the fallback: completion, go-to-declaration and find-usages all build off this
/// classpath (the provider + the semantic engine). A legacy project targeting Java 8 on a
/// machine that only has a modern JDK (17/21) would otherwise silently lose all three —
/// while the pure-source class index still works — a confusing half-broken state. The core
/// `java.*` API is largely forward-compatible, so a newer JDK answers member resolution fine.
pub fn resolve_jdk_classpath(version: &str) -> Result<Box<dyn ClassSource>, String> {
    let major = requested_major(version)
        .ok_or_else(|| format!("unrecognised Java version string: {version:?}"))?;

    let (home, resolved_major) = match find_jdk_for(major) {
        Some(home) => (home, major),
        None => {
            let (home, m) = best_available_jdk().ok_or_else(|| {
                format!("no JDK installed (project targets Java {major}); install a JDK")
            })?;
            eprintln!(
                "bennu-classpath: no JDK for Java {major} installed; falling back to Java {m}"
            );
            (home, m)
        }
    };

    if resolved_major <= 8 {
        resolve_jdk8(&home)
    } else {
        resolve_jimage(&home)
    }
}

/// The newest installed JDK (highest language level) plus its major version, or `None` when
/// no JDK is installed. The fallback when no JDK matches the project's exact level.
fn best_available_jdk() -> Option<(PathBuf, u32)> {
    let candidates = candidate_jdks();
    let best = candidates
        .iter()
        .filter_map(|home| jdk_major(home).map(|m| (home.clone(), m)))
        .max_by_key(|&(_, m)| m);
    // "No JDK found" on a machine with three of them installed is a sentence with nowhere to go:
    // the user can see their JDKs and we cannot, and nothing on either side says which directories
    // were actually looked in. One line, only on the failing path, and the next occurrence answers
    // itself.
    if best.is_none() {
        eprintln!(
            "bennu-classpath: no JDK found. Probed {} candidate home(s): {}. Roots searched: {}",
            candidates.len(),
            if candidates.is_empty() {
                "none".to_string()
            } else {
                candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            },
            jdk_install_roots()
                .iter()
                .map(|r| r.dir.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    best
}

/// A snapshot of how [`resolve_jdk_classpath`] would resolve `version` — what the FE turns
/// into a titlebar warning (no JDK installed) or a Problems entry (a fallback / wrong-version
/// JDK). Computed WITHOUT building the classpath (a cheap FS probe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JdkStatus {
    /// The major level the project requested (`None` if the version string was unparseable).
    pub requested_major: Option<u32>,
    /// The install home of the JDK that would be used (exact match or fallback), if any.
    pub resolved_home: Option<PathBuf>,
    /// The major level of the JDK that would be used, if any.
    pub resolved_major: Option<u32>,
    /// True when an exact-major JDK was found (no fallback needed).
    pub exact: bool,
    /// True when at least one JDK is installed (an exact one or a fallback candidate).
    pub any_installed: bool,
}

/// Compute the JDK resolution status for `version` — mirrors [`resolve_jdk_classpath`]'s
/// exact-then-newest-fallback decision, for the FE's JDK diagnostics. Never builds a
/// classpath (no jar/jimage opens); just probes which JDKs are installed.
pub fn jdk_status(version: &str) -> JdkStatus {
    let requested_major = requested_major(version);
    let exact_home = requested_major.and_then(find_jdk_for);
    let best = best_available_jdk();
    let (resolved_home, resolved_major, exact) = match exact_home {
        Some(home) => (Some(home), requested_major, true),
        None => match &best {
            Some((home, m)) => (Some(home.clone()), Some(*m), false),
            None => (None, None, false),
        },
    };
    JdkStatus { requested_major, resolved_home, resolved_major, exact, any_installed: best.is_some() }
}

/// The major language level a version string requests, e.g. `"1.8"`/`"8"` → 8,
/// `"21"` → 21, `"9"` → 9. `None` when unparseable.
fn requested_major(version: &str) -> Option<u32> {
    let v = version.trim();
    // Legacy `1.N` form: the level is N.
    if let Some(rest) = v.strip_prefix("1.") {
        return rest.split(['.', '_', '-']).next()?.parse().ok();
    }
    v.split(['.', '_', '-']).next()?.parse().ok()
}

/// The major version of a JDK from its `release` file (`JAVA_VERSION="21.0.6"` → 21,
/// `JAVA_VERSION="1.8.0_442"` → 8). `None` if the file is missing/unreadable.
fn jdk_major(home: &Path) -> Option<u32> {
    let release = fs::read_to_string(home.join("release")).ok()?;
    let line = release.lines().find(|l| l.starts_with("JAVA_VERSION"))?;
    let quoted = line.split('=').nth(1)?.trim().trim_matches('"');
    requested_major(quoted)
}

/// Candidate JDK homes in priority order: the user-configured extras, then `JAVA_HOME`, then every
/// JDK under the platform's standard install roots — deduplicated, and each normalized to the
/// directory that actually holds `release` (see [`normalize_jdk_home`]).
fn candidate_jdks() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    // User-configured extra homes first (highest priority — an explicit setting).
    if let Ok(extra) = EXTRA_JDK_HOMES.read() {
        for p in extra.iter() {
            push_jdk_home(p, &mut out);
        }
    }
    if let Ok(home) = std::env::var("JAVA_HOME") {
        push_jdk_home(Path::new(&home), &mut out);
    }
    for root in jdk_install_roots() {
        let Ok(entries) = fs::read_dir(&root.dir) else { continue };
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| match root.name_contains {
                Some(needle) => {
                    p.file_name().is_some_and(|n| n.to_string_lossy().contains(needle))
                }
                None => true,
            })
            .collect();
        // Sorted so which same-level JDK wins doesn't depend on filesystem enumeration order.
        children.sort();
        for child in children {
            push_jdk_home(&child, &mut out);
        }
    }
    out
}

/// Normalize `dir` to a JDK home and append it to `out`, unless it isn't a JDK or is already there.
fn push_jdk_home(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Some(home) = normalize_jdk_home(dir) {
        if !out.contains(&home) {
            out.push(home);
        }
    }
}

/// Resolve a discovered directory to the actual JDK **home** — the directory that holds `release`
/// (which is what [`jdk_major`] reads and what a `JAVA_HOME` export must point at).
///
/// Three shapes exist in the wild and only the first one is the directory you find by listing an
/// install root:
///   * the home itself — Windows and Linux installs, and a hand-set `JAVA_HOME`;
///   * a macOS **bundle**, whose home is nested at `Contents/Home` — everything under
///     `/Library/Java/JavaVirtualMachines`, i.e. every Temurin / Zulu / Corretto install on a Mac;
///   * a Homebrew *formula* directory, which wraps such a bundle under `libexec`.
///
/// `None` when no candidate holds a `release` file — i.e. the directory isn't a JDK. Normalizing
/// `JAVA_HOME` and the user's configured extras through here too is deliberate: pointing either at a
/// macOS bundle rather than at `Contents/Home` is an easy and previously silent mistake.
fn normalize_jdk_home(dir: &Path) -> Option<PathBuf> {
    for rel in ["", "Contents/Home", "libexec/openjdk.jdk/Contents/Home"] {
        let home = if rel.is_empty() { dir.to_path_buf() } else { dir.join(rel) };
        if home.join("release").is_file() {
            return Some(home);
        }
    }
    None
}

/// One directory whose children are JDK installs, with an optional filter on the child name.
struct JdkRoot {
    dir: PathBuf,
    /// When set, only children whose name contains this are probed. For a shared prefix like
    /// Homebrew's `opt/`, where a few `openjdk@N` formulae sit among hundreds of unrelated ones,
    /// this is the difference between three probes and a thousand.
    name_contains: Option<&'static str>,
}

/// The directories whose children are JDK installs, across platforms.
///
/// This used to be the two Windows `Program Files` roots and nothing else, so on macOS and Linux the
/// only discoverable JDK was whatever `JAVA_HOME` pointed at — and a desktop app launched from
/// Finder / the Dock / a desktop launcher inherits the system's minimal environment, not the user's
/// shell profile, so `JAVA_HOME` is typically **unset** there. The result was an empty JDK tier:
/// `java.lang.String` itself didn't resolve, and the whole project reported thousands of
/// "cannot resolve" errors that looked like anything except a missing JDK.
///
/// A root that doesn't exist costs one failed `read_dir`, so listing every plausible location beats
/// guessing at the platform.
fn jdk_install_roots() -> Vec<JdkRoot> {
    /// Roots whose every child is a candidate JDK.
    const SYSTEM_ROOTS: [&str; 12] = [
        // Windows: the vendor directories under Program Files.
        "C:/Program Files/Java",
        "C:/Program Files (x86)/Java",
        "C:/Program Files/Eclipse Adoptium",
        "C:/Program Files/Amazon Corretto",
        "C:/Program Files/Microsoft",
        "C:/Program Files/Zulu",
        // macOS: every installed JVM is a bundle under this one directory (the `Contents/Home`
        // nesting is `normalize_jdk_home`'s job).
        "/Library/Java/JavaVirtualMachines",
        // Linux: distro packages and unpacked vendor tarballs.
        "/usr/lib/jvm",
        "/usr/lib64/jvm",
        "/usr/java",
        "/opt/java",
        "/opt/jdk",
    ];
    /// Per-user roots, relative to the home directory: version managers, and the JDKs an IDE
    /// downloads for you (`~/.jdks` is IntelliJ's).
    const USER_ROOTS: [&str; 5] = [
        ".jdks",
        ".sdkman/candidates/java",
        ".asdf/installs/java",
        ".gradle/jdks",
        "Library/Java/JavaVirtualMachines",
    ];
    /// Homebrew's `openjdk` FORMULA — as opposed to the Temurin/Zulu *casks*, which land in
    /// `/Library/Java/JavaVirtualMachines` above. The formula installs into its own prefix and
    /// deliberately does NOT register itself system-wide, so it's invisible unless we look here.
    const BREW_PREFIXES: [&str; 2] = ["/opt/homebrew/opt", "/usr/local/opt"];

    let mut roots: Vec<JdkRoot> = Vec::new();
    for dir in SYSTEM_ROOTS {
        roots.push(JdkRoot { dir: PathBuf::from(dir), name_contains: None });
    }
    for prefix in BREW_PREFIXES {
        roots.push(JdkRoot { dir: PathBuf::from(prefix), name_contains: Some("jdk") });
    }
    if let Some(home) = user_home() {
        for rel in USER_ROOTS {
            roots.push(JdkRoot { dir: home.join(rel), name_contains: None });
        }
    }
    roots
}

/// The current user's home directory, from the environment (`HOME`, or `USERPROFILE` on Windows).
///
/// `bennu-classpath` is a leaf crate with no platform-dirs dependency, and these are the variables
/// such a crate would read anyway. Shared with the Maven-launcher discovery in `bennu-be`, which has
/// the same "a GUI process has no shell profile" problem.
pub fn user_home() -> Option<PathBuf> {
    for var in ["HOME", "USERPROFILE"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return Some(PathBuf::from(v));
            }
        }
    }
    None
}

/// Find an installed JDK whose language level matches `major`.
fn find_jdk_for(major: u32) -> Option<PathBuf> {
    candidate_jdks().into_iter().find(|home| jdk_major(home) == Some(major))
}

/// The install home (the dir to export as `JAVA_HOME`) of an installed JDK matching the
/// language-level `version` string (`"1.8"` / `"8"` / `"17"`). `None` when the version
/// is unparseable or no matching JDK is installed. Used by the build/run shell-out
/// (`bennu-be`) to point `mvn` / `javac` / `java` at the project's JDK.
pub fn find_jdk_home(version: &str) -> Option<PathBuf> {
    let major = requested_major(version)?;
    find_jdk_for(major)
}

/// Locate the **source** archive for the JDK matching `version` — `lib/src.zip` (JDK 9+) or
/// `src.zip` at the JDK root (JDK 8) — mirroring [`resolve_jdk_classpath`]'s exact-then-newest
/// discovery (so a Java-8 project on a modern-JDK-only machine still gets sources). `None` when no
/// JDK is installed OR the chosen JDK ships no sources (a bare JRE, or a JDK installed without the
/// `src.zip` component). The caller then falls back to the signatures-only decompiled stub — the
/// real `.java` (method bodies, locals, lambdas) is strictly better for the "go to source" view,
/// but it's a best-effort enhancement, never required.
pub fn resolve_jdk_sources(version: &str) -> Option<JavaSourceZip> {
    let major = requested_major(version)?;
    let home = find_jdk_for(major).or_else(|| best_available_jdk().map(|(home, _)| home))?;
    // JDK 9+ keeps sources under `lib/`; JDK 8 puts `src.zip` at the install root. Probe both.
    for rel in ["lib/src.zip", "src.zip"] {
        let path = home.join(rel);
        if path.is_file() {
            if let Ok(zip) = JavaSourceZip::open(&path) {
                return Some(zip);
            }
        }
    }
    None
}

/// JDK 8: every boot jar in `jre/lib/` (rt.jar first, then jce.jar / jsse.jar / charsets.jar /
/// resources.jar / …) plus every `jre/lib/ext/` jar, chained. The JDK-8 platform is split across
/// several jars, not just rt.jar — `javax.crypto` (jce.jar) / `javax.net.ssl` (jsse.jar) would be
/// missed otherwise. `jre/lib/*` on a JDK (the JRE nested inside the JDK); a bare JRE has the same
/// layout without the `jre/` level, so we probe both.
fn resolve_jdk8(home: &Path) -> Result<Box<dyn ClassSource>, String> {
    let lib = if home.join("jre/lib/rt.jar").is_file() {
        home.join("jre/lib")
    } else {
        home.join("lib")
    };

    let rt = lib.join("rt.jar");
    if !rt.is_file() {
        return Err(format!("rt.jar not found under {}", lib.display()));
    }

    let mut sources: Vec<Box<dyn ClassSource>> = Vec::new();
    // rt.jar first — it holds the vast majority of the platform and is the hottest probe.
    sources.push(Box::new(JarSource::open(&rt)?));
    // Then EVERY OTHER boot jar in `jre/lib/`. JDK 8 splits the platform across several jars, not just
    // rt.jar: `javax.crypto` lives in `jce.jar`, `javax.net.ssl` in `jsse.jar`, extra charsets in
    // `charsets.jar`, plus `resources.jar` / `sunrsasign.jar` / `jfr.jar`. Loading only rt.jar
    // (+resources) missed them, so those `javax.*` types didn't resolve even though `java.*` did.
    push_jars_in(&lib, "rt.jar", &mut sources);
    // Extension jars (nashorn, localedata, …).
    push_jars_in(&lib.join("ext"), "", &mut sources);

    Ok(Box::new(MultiSource::new(sources)))
}

/// Open every `.jar` directly in `dir` (non-recursive, sorted for a deterministic order), skipping the
/// file named `skip` (already opened) and any broken jar. No-op when `dir` isn't a directory.
fn push_jars_in(dir: &Path, skip: &str, sources: &mut Vec<Box<dyn ClassSource>>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut jars: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "jar").unwrap_or(false))
        .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some(skip))
        .collect();
    jars.sort();
    for jar in jars {
        // A broken jar shouldn't sink the whole classpath; skip it.
        if let Ok(src) = JarSource::open(&jar) {
            sources.push(Box::new(src));
        }
    }
}

/// JDK 9+: the `lib/modules` jimage. Probes a broad set of core modules so common
/// `java.*`/`javax.*` classes (sql, xml, naming, …) resolve, not just `java.base`.
fn resolve_jimage(home: &Path) -> Result<Box<dyn ClassSource>, String> {
    let modules_file = home.join("lib/modules");
    if !modules_file.is_file() {
        return Err(format!("jimage not found at {}", modules_file.display()));
    }
    let src = JimageSource::open(&modules_file)?.with_modules(default_probe_modules());
    Ok(Box::new(src))
}

/// The module probe order for a modular JDK: `java.base` first (covers the vast
/// majority), then the common platform modules a typical app touches.
fn default_probe_modules() -> Vec<String> {
    [
        "java.base",
        "java.sql",
        "java.xml",
        "java.naming",
        "java.desktop",
        "java.logging",
        "java.management",
        "java.net.http",
        "java.rmi",
        "java.compiler",
        "java.instrument",
        "java.scripting",
        "java.security.jgss",
        "java.transaction.xa",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_major_parses_all_forms() {
        assert_eq!(requested_major("1.8"), Some(8));
        assert_eq!(requested_major("8"), Some(8));
        assert_eq!(requested_major("1.8.0_442"), Some(8));
        assert_eq!(requested_major("9"), Some(9));
        assert_eq!(requested_major("11"), Some(11));
        assert_eq!(requested_major("21"), Some(21));
        assert_eq!(requested_major("21.0.6"), Some(21));
        assert_eq!(requested_major("garbage"), None);
    }

    // ── JDK-home normalization (the three install shapes) ─────────────────────

    /// Lay out a fake JDK: a `release` file at `home_rel` under a fresh temp dir. Returns the dir the
    /// *discovery* would hand to `normalize_jdk_home` (the top of the install), and the real home.
    fn fake_jdk(tag: &str, home_rel: &str, version: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("bennu-jdk-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let home = if home_rel.is_empty() { dir.clone() } else { dir.join(home_rel) };
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join("release"), format!("JAVA_VERSION=\"{version}\"\n")).unwrap();
        (dir, home)
    }

    #[test]
    fn normalizes_a_plain_home() {
        let (dir, home) = fake_jdk("plain", "", "21.0.11");
        assert_eq!(normalize_jdk_home(&dir), Some(home));
        assert_eq!(jdk_major(&dir), Some(21));
        let _ = fs::remove_dir_all(&dir);
    }

    /// The macOS shape: listing `/Library/Java/JavaVirtualMachines` yields the BUNDLE, whose home is
    /// two levels down. Not descending into it is what left a Mac with an empty JDK tier.
    #[test]
    fn normalizes_a_macos_bundle() {
        let (dir, home) = fake_jdk("bundle", "Contents/Home", "1.8.0_492");
        assert_eq!(normalize_jdk_home(&dir), Some(home.clone()));
        assert_eq!(jdk_major(&home), Some(8));
        // The bundle root itself has no `release` — the whole point of the normalization.
        assert_eq!(jdk_major(&dir), None);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The Homebrew `openjdk` formula shape.
    #[test]
    fn normalizes_a_homebrew_formula_dir() {
        let (dir, home) = fake_jdk("brew", "libexec/openjdk.jdk/Contents/Home", "17.0.9");
        assert_eq!(normalize_jdk_home(&dir), Some(home));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_directory_without_a_release_file_is_not_a_jdk() {
        let dir = std::env::temp_dir().join(format!("bennu-jdk-none-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bin")).unwrap();
        assert_eq!(normalize_jdk_home(&dir), None);
        assert_eq!(normalize_jdk_home(Path::new("/definitely/not/here")), None);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The Homebrew prefixes hold hundreds of unrelated formulae, so they MUST carry a name filter —
    /// without it every `candidate_jdks()` call (and `jdk_status` is polled) would probe them all.
    #[test]
    fn shared_prefix_roots_are_name_filtered() {
        let roots = jdk_install_roots();
        for shared in ["/opt/homebrew/opt", "/usr/local/opt"] {
            let root = roots
                .iter()
                .find(|r| r.dir == Path::new(shared))
                .unwrap_or_else(|| panic!("{shared} is no longer scanned"));
            assert!(root.name_contains.is_some(), "{shared} would be scanned unfiltered");
        }
    }

    /// `push_jdk_home` deduplicates, so `JAVA_HOME` pointing at an install that the root scan also
    /// finds yields one candidate, not two.
    #[test]
    fn push_jdk_home_deduplicates() {
        let (dir, home) = fake_jdk("dedup", "Contents/Home", "21.0.11");
        let mut out = Vec::new();
        push_jdk_home(&dir, &mut out);
        push_jdk_home(&dir, &mut out);
        push_jdk_home(&home, &mut out); // the already-normalized form of the same JDK
        assert_eq!(out, vec![home]);
        let _ = fs::remove_dir_all(&dir);
    }
}
