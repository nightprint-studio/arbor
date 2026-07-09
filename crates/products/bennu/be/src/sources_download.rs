//! "Download sources" for a Maven dependency — the pure helpers + the `mvn dependency:get`
//! shell-out behind the decompiled-tab **Download sources** banner.
//!
//! When Ctrl+B lands on a library type that has no attached sources (only a decompiled stub),
//! the FE offers a one-click download. This module locates the dependency's `~/.m2` jar, derives
//! its Maven coordinates, and fetches the `-sources.jar` via `mvn dependency:get` (run in the
//! project dir, so it honours the project's configured repositories — a corporate Nexus, not just
//! Maven Central). The stateful orchestration (job registration, cache refresh, FE event) lives in
//! [`crate::index_service`]; this module is the pure/IO leaf so its parsing is unit-testable.

use std::path::{Path, PathBuf};
use std::process::Command;

use arbor_process_ext::prelude::NoWindowExt;
use bennu_classpath::prelude::{ClassSource, JarSource, JavaSourceZip};

/// Maven coordinates parsed from a jar's `~/.m2` path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gav {
    pub group: String,
    pub artifact: String,
    pub version: String,
}

impl Gav {
    /// The `mvn dependency:get -Dartifact=…` coordinate for the SOURCES classifier.
    pub fn sources_artifact(&self) -> String {
        format!("{}:{}:{}:jar:sources", self.group, self.artifact, self.version)
    }

    /// A compact `artifact:version` label for the job / banner.
    pub fn label(&self) -> String {
        format!("{}:{}", self.artifact, self.version)
    }
}

/// Parse the Maven coordinates from a dependency jar living under a local `~/.m2/repository`.
///
/// Layout: `…/repository/<group/with/slashes>/<artifactId>/<version>/<artifactId>-<version>.jar`.
/// The version is the jar's parent dir, the artifactId its grandparent, and the group is the path
/// between the `repository` marker and the artifactId dir. `None` when the path isn't shaped like a
/// local-repo artifact (no `repository` segment, or too shallow) — the caller then reports that the
/// coordinates couldn't be determined instead of guessing.
pub fn gav_from_m2_jar(jar: &Path) -> Option<Gav> {
    let comps: Vec<String> =
        jar.iter().map(|c| c.to_string_lossy().into_owned()).collect();
    if comps.len() < 4 {
        return None;
    }
    // `repository` anchors the group path. Fall back to the last-resort layout parse when a custom
    // local-repo dir isn't literally named "repository" is intentionally NOT attempted — without the
    // marker the group boundary is ambiguous.
    let repo_idx = comps.iter().rposition(|c| c == "repository")?;
    let last = comps.len() - 1; // filename
    let version_idx = last.checked_sub(1)?;
    let artifact_idx = last.checked_sub(2)?;
    if artifact_idx <= repo_idx {
        return None;
    }
    let group = comps[repo_idx + 1..artifact_idx].join(".");
    let artifact = comps[artifact_idx].clone();
    let version = comps[version_idx].clone();
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    Some(Gav { group, artifact, version })
}

/// The `-sources.jar` sibling of a dependency jar (`foo-1.2.3.jar` → `foo-1.2.3-sources.jar`),
/// or `None` when the path has no `.jar` name. Pure string surgery — doesn't check existence.
pub fn sources_jar_sibling(jar: &Path) -> Option<PathBuf> {
    let stem = jar.file_stem()?.to_string_lossy().into_owned(); // "foo-1.2.3"
    let sib = format!("{stem}-sources.jar");
    Some(jar.with_file_name(sib))
}

/// Whether `binary` names a genuinely JDK-only type (`java/…`, `sun/…`, …) — so the "Download
/// sources" affordance is NOT offered for it (a JDK class's sources come from `src.zip`, never
/// Maven). Note `javax/` and `jakarta/` are deliberately EXCLUDED: those are largely third-party
/// jars (`javax.servlet`, `jakarta.*`) whose sources ARE on Maven — and any JDK-bundled `javax`
/// (Swing, JAXP) is served from the JDK `src.zip` before the download banner is ever considered.
pub fn is_jdk_package(binary: &str) -> bool {
    const JDK_PREFIXES: &[&str] =
        &["java/", "jdk/", "sun/", "com/sun/", "org/w3c/", "org/xml/sax", "org/ietf/", "org/omg/"];
    JDK_PREFIXES.iter().any(|p| binary.starts_with(p))
}

/// Find the dependency jar (among the project's resolved `~/.m2` jars) that contains the class
/// `binary`, by probing each for the `.class`. `None` when no resolved jar owns it (e.g. a JDK type,
/// or the deps aren't resolved). O(jars) opens — used only on the one-shot download click.
pub fn find_owning_jar(dep_jars: &[String], binary: &str) -> Option<PathBuf> {
    for jar in dep_jars {
        let path = PathBuf::from(jar);
        if let Ok(src) = JarSource::open(&path) {
            if matches!(src.class_bytes(binary), Ok(Some(_))) {
                return Some(path);
            }
        }
    }
    None
}

/// Open every dependency `-sources.jar` that already exists on disk (siblings of the resolved dep
/// jars) as a [`JavaSourceZip`]. The cached pool the decompiled-tab go-to consults for REAL library
/// source before falling back to a stub. A dep with no sources jar simply isn't in the pool.
pub fn open_dep_source_zips(dep_jars: &[String]) -> Vec<JavaSourceZip> {
    let mut out = Vec::new();
    for jar in dep_jars {
        if let Some(sib) = sources_jar_sibling(Path::new(jar)) {
            if sib.is_file() {
                if let Ok(zip) = JavaSourceZip::open(&sib) {
                    out.push(zip);
                }
            }
        }
    }
    out
}

/// Run `mvn dependency:get` for the sources artifact, in the project dir (so the project's
/// repositories/credentials apply) and pointed at the project JDK. ONLINE by design — the whole
/// point is to fetch a jar not yet in `~/.m2`. Returns `(success, merged output)` for the job log.
pub fn run_mvn_get_sources(
    root: &Path,
    mvn_path: &str,
    java_home: Option<&Path>,
    gav: &Gav,
) -> Result<(bool, String), String> {
    let mut cmd = Command::new(mvn_path);
    cmd.current_dir(root)
        .arg("-q")
        .arg("--batch-mode")
        .arg("dependency:get")
        .arg(format!("-Dartifact={}", gav.sources_artifact()));
    if let Some(jh) = java_home {
        cmd.env("JAVA_HOME", jh);
    }
    cmd.no_window();
    let out = cmd.output().map_err(|e| format!("spawn mvn ({mvn_path}): {e}"))?;
    let merged = {
        let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
        let err = String::from_utf8_lossy(&out.stderr);
        if !err.is_empty() {
            s.push_str(&err);
        }
        s
    };
    Ok((out.status.success(), merged))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gav_parses_a_standard_m2_path() {
        let jar = Path::new(
            "C:/Users/u/.m2/repository/org/springframework/spring-core/5.3.20/spring-core-5.3.20.jar",
        );
        let gav = gav_from_m2_jar(jar).expect("parse");
        assert_eq!(gav.group, "org.springframework");
        assert_eq!(gav.artifact, "spring-core");
        assert_eq!(gav.version, "5.3.20");
        assert_eq!(gav.sources_artifact(), "org.springframework:spring-core:5.3.20:jar:sources");
        assert_eq!(gav.label(), "spring-core:5.3.20");
    }

    #[test]
    fn gav_none_without_repository_marker() {
        assert!(gav_from_m2_jar(Path::new("C:/tmp/spring-core-5.3.20.jar")).is_none());
    }

    #[test]
    fn sources_sibling_inserts_classifier() {
        let sib = sources_jar_sibling(Path::new("/r/org/foo/1.0/foo-1.0.jar")).unwrap();
        assert_eq!(sib, PathBuf::from("/r/org/foo/1.0/foo-1.0-sources.jar"));
    }

    #[test]
    fn jdk_packages_are_recognised() {
        assert!(is_jdk_package("java/util/Optional"));
        assert!(is_jdk_package("sun/misc/Unsafe"));
        // javax/jakarta are NOT treated as JDK — they're downloadable deps (javax.servlet, jakarta.*).
        assert!(!is_jdk_package("javax/servlet/http/HttpServletRequest"));
        assert!(!is_jdk_package("org/springframework/util/StringUtils"));
        assert!(!is_jdk_package("com/acme/Foo"));
    }
}
