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
//! requested level wins.
//!
//! Scope: **JDK bootclasspath only.** Dependency-jar sourcing from `~/.m2` is out of
//! Phase 1 (docs); those will layer in later as additional [`JarSource`]s behind the
//! same trait — no change to this module's shape.

use std::fs;
use std::path::{Path, PathBuf};

use crate::source::{ClassSource, JarSource, JimageSource};

/// A [`ClassSource`] that tries several sources in order (first hit wins). Used for
/// the JDK-8 bootclasspath (rt.jar + resources.jar + ext jars); reusable for a
/// project classpath (project classes + dependency jars) in a later phase.
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
}

/// Build a [`ClassSource`] for the JDK bootclasspath matching `version`
/// (`"1.8"`/`"8"` → rt.jar + ext + resources; `"9"`+ → jimage). `Err` when no
/// matching JDK is installed or the expected container is missing.
pub fn resolve_jdk_classpath(version: &str) -> Result<Box<dyn ClassSource>, String> {
    let major = requested_major(version)
        .ok_or_else(|| format!("unrecognised Java version string: {version:?}"))?;

    let home = find_jdk_for(major)
        .ok_or_else(|| format!("no installed JDK found for Java {major}"))?;

    if major <= 8 {
        resolve_jdk8(&home)
    } else {
        resolve_jimage(&home)
    }
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
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let p = PathBuf::from(home);
        if p.is_dir() {
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

/// JDK 8: rt.jar + resources.jar + every ext jar, chained. `jre/lib/*` on a JDK (the
/// JRE nested inside the JDK); a bare JRE has the same layout without the `jre/`
/// level, so we probe both.
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
    sources.push(Box::new(JarSource::open(&rt)?));

    // resources.jar is part of the boot set; include it when present (harmless if it
    // has no classes).
    let resources = lib.join("resources.jar");
    if resources.is_file() {
        sources.push(Box::new(JarSource::open(&resources)?));
    }

    // Extension jars (charsets, locales, nashorn, …). Sorted for a deterministic
    // probe order.
    let ext = lib.join("ext");
    if let Ok(entries) = fs::read_dir(&ext) {
        let mut ext_jars: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "jar").unwrap_or(false))
            .collect();
        ext_jars.sort();
        for jar in ext_jars {
            // A broken ext jar shouldn't sink the whole classpath; skip it.
            if let Ok(src) = JarSource::open(&jar) {
                sources.push(Box::new(src));
            }
        }
    }

    Ok(Box::new(MultiSource::new(sources)))
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
