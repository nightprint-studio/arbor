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
use bennu_deps::prelude::{coord_of, Coord};

/// The `mvn dependency:get -Dartifact=…` coordinate for an artifact's **sources**.
///
/// The one Maven-CLI-shaped string in this module: five colon-separated fields, `jar:sources` at
/// the end. Kept here rather than on [`Coord`] because it is a command-line spelling and not a
/// property of the coordinate.
pub fn sources_artifact(coord: &Coord) -> String {
    format!("{}:{}:{}:jar:sources", coord.group_id, coord.artifact_id, coord.version)
}

/// A compact `artifact:version` label for the job / banner.
pub fn artifact_label(coord: &Coord) -> String {
    format!("{}:{}", coord.artifact_id, coord.version)
}

/// The Maven coordinates of a dependency jar living in the local repository.
///
/// **The repository is asked where it is first.** This was its own parse, with its own rule: it
/// insisted on a literal `repository` path segment and gave up without one, so on a machine with a
/// relocated local repository (`-Dmaven.repo.local`, or a `<localRepository>` in `settings.xml`)
/// every "Download sources" reported *couldn't determine the coordinates* — for a jar the index had
/// loaded perfectly well. Resolving the root and reading the path under it makes the group exact
/// wherever the repository lives; [`coord_of`]'s marker trick stays as the fallback for a jar that
/// is somehow outside it.
///
/// `None` for a path that is not laid out like a repository artifact (`target/classes`, or a jar a
/// `system`-scoped dependency points straight at), and `None` when the group could not be
/// determined either way — `dependency:get` needs a whole coordinate, and half of one would fetch
/// nothing while looking like it tried.
pub fn gav_from_m2_jar(jar: &Path) -> Option<Coord> {
    let repo = bennu_maven::prelude::LocalRepo::discover();
    let coord = repo.coord_at(jar).or_else(|| coord_of(jar))?;
    coord.is_complete().then_some(coord)
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
    gav: &Coord,
) -> Result<(bool, String), String> {
    let mut cmd = Command::new(mvn_path);
    cmd.current_dir(root)
        .arg("-q")
        .arg("--batch-mode")
        .arg("dependency:get")
        .arg(format!("-Dartifact={}", sources_artifact(gav)));
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
        assert_eq!(gav.ga(), "org.springframework:spring-core");
        assert_eq!(gav.version, "5.3.20");
        assert_eq!(sources_artifact(&gav), "org.springframework:spring-core:5.3.20:jar:sources");
        assert_eq!(artifact_label(&gav), "spring-core:5.3.20");
    }

    /// A jar that is not laid out like a repository artifact has no coordinate to fetch sources
    /// for, and half a coordinate would fetch nothing while looking like it tried.
    #[test]
    fn gav_none_for_a_path_that_is_not_repository_shaped() {
        assert!(gav_from_m2_jar(Path::new("C:/tmp/spring-core-5.3.20.jar")).is_none());
        assert!(gav_from_m2_jar(Path::new("C:/proj/module/target/classes")).is_none());
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
