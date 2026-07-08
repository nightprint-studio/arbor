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
//! JDK discovery: `JAVA_HOME` first, then every child of `C:/Program Files/Java`
//! (and the x86 variant). Each candidate's language level is read from its `release`
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
/// classpath (the provider + the rename engine). A legacy project targeting Java 8 on a
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
    candidate_jdks()
        .into_iter()
        .filter_map(|home| jdk_major(&home).map(|m| (home, m)))
        .max_by_key(|&(_, m)| m)
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

/// Candidate JDK homes: `JAVA_HOME` first (if set), then each child directory of the
/// standard Windows install roots, deduplicated by path.
fn candidate_jdks() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    // User-configured extra homes first (highest priority — an explicit setting).
    if let Ok(extra) = EXTRA_JDK_HOMES.read() {
        for p in extra.iter() {
            if p.is_dir() && !out.contains(p) {
                out.push(p.clone());
            }
        }
    }
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let p = PathBuf::from(home);
        if p.is_dir() && !out.contains(&p) {
            out.push(p);
        }
    }
    for root in ["C:/Program Files/Java", "C:/Program Files (x86)/Java"] {
        if let Ok(entries) = fs::read_dir(root) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
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
}
