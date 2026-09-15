//! Which version of a tool this project is actually on.
//!
//! ## Why the pom and not the classpath
//!
//! The resolved classpath would be exact, and it is not available here: an extension is handed the
//! project's *text*, and the jar set belongs to the host. The poms are: they are files, they are
//! already parsed by the crate next door, and `bennu-maven`'s effective-pom reader folds in the
//! parent chain, the `${…}` properties and `<dependencyManagement>` — which between them cover the
//! two ways a version is normally written down (`<lombok.version>` in a parent, or a BOM).
//!
//! ## Why "I don't know" is a first-class answer
//!
//! Because it is the common case on a real tree, and the *only* honest one. Lombok can arrive
//! transitively; a Gradle project has no pom at all; a corporate parent may pin the version three
//! poms up in a repository this machine has never fetched. Every consumer of this module treats
//! `None` as *offer everything and gate nothing* — a key too many, never a key missing.

use std::path::Path;

use bennu_maven::prelude::{compare_versions, effective_of_buffer, reactor, LocalRepo};

/// The highest version of any of `artifacts` that this project's poms resolve to.
///
/// Highest, and not first-found, because a reactor is normally consistent and where it is not the
/// newer module is the one whose config file would otherwise be gated too tightly. Under-report the
/// gate, as everywhere else here.
pub fn resolve(root: &Path, artifacts: &[(&str, &str)]) -> Option<String> {
    let repo = LocalRepo::discover();
    let mut best: Option<String> = None;
    for (dir, _) in reactor(root) {
        let path = dir.join("pom.xml");
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let effective = effective_of_buffer(&repo, &path, &String::from_utf8_lossy(&bytes));

        let declared = effective.dependencies.iter().find_map(|d| {
            matches(artifacts, &d.coord.group_id, &d.coord.artifact_id)
                .then(|| d.coord.version.clone())
        });
        // A pom that only *manages* the coordinate has still said which version this project is on
        // — that is exactly what a `<dependencyManagement>` in a parent, or an imported BOM, is
        // for. Second, because a module that declares its own version overrides it.
        let managed = || {
            effective.managed.iter().find_map(|(key, m)| {
                let mut parts = key.split(':');
                let (Some(g), Some(a)) = (parts.next(), parts.next()) else { return None };
                matches(artifacts, g, a).then(|| m.version.clone())
            })
        };

        let found = declared.or_else(managed);
        // A version left as a `${…}` nobody could expand is not a version. Reporting it would gate
        // the vocabulary on a string, and `compare_versions` would read it as `0`.
        let Some(found) = found.filter(|v| !v.is_empty() && !v.contains("${")) else { continue };
        if best.as_deref().map_or(true, |b| compare_versions(&found, b).is_gt()) {
            best = Some(found);
        }
    }
    best
}

fn matches(artifacts: &[(&str, &str)], group: &str, artifact: &str) -> bool {
    artifacts.iter().any(|(g, a)| *g == group && *a == artifact)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a pom and read the version back out of it — the round trip the extension makes on
    /// every reindex.
    fn version_in(pom: &str, artifacts: &[(&str, &str)]) -> Option<String> {
        // A counter and not a timestamp. These tests run in parallel in one process, and
        // `SystemTime::now()` is coarse enough on macOS that two of them starting together got the
        // same directory name — so one test's pom overwrote the other's and it read back a version
        // it never wrote. Intermittent, and it looked like a resolver bug rather than a test one.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "bennu-toolconf-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pom.xml"), pom).unwrap();
        let found = resolve(&dir, artifacts);
        let _ = std::fs::remove_dir_all(&dir);
        found
    }

    const LOMBOK: &[(&str, &str)] = &[("org.projectlombok", "lombok")];

    #[test]
    fn a_declared_version_is_read() {
        let found = version_in(
            r#"<project><groupId>a</groupId><artifactId>b</artifactId><version>1</version>
               <dependencies><dependency>
                 <groupId>org.projectlombok</groupId><artifactId>lombok</artifactId>
                 <version>1.18.30</version>
               </dependency></dependencies></project>"#,
            LOMBOK,
        );
        assert_eq!(found.as_deref(), Some("1.18.30"));
    }

    #[test]
    fn a_version_written_as_a_property_is_expanded() {
        // The normal shape of a real pom, and the reason this goes through the effective-pom reader
        // rather than reading `<version>` off the declaration.
        let found = version_in(
            r#"<project><groupId>a</groupId><artifactId>b</artifactId><version>1</version>
               <properties><lombok.version>1.18.24</lombok.version></properties>
               <dependencies><dependency>
                 <groupId>org.projectlombok</groupId><artifactId>lombok</artifactId>
                 <version>${lombok.version}</version>
               </dependency></dependencies></project>"#,
            LOMBOK,
        );
        assert_eq!(found.as_deref(), Some("1.18.24"));
    }

    #[test]
    fn a_managed_version_counts_when_the_dependency_writes_none() {
        let found = version_in(
            r#"<project><groupId>a</groupId><artifactId>b</artifactId><version>1</version>
               <dependencyManagement><dependencies><dependency>
                 <groupId>org.projectlombok</groupId><artifactId>lombok</artifactId>
                 <version>1.18.20</version>
               </dependency></dependencies></dependencyManagement>
               <dependencies><dependency>
                 <groupId>org.projectlombok</groupId><artifactId>lombok</artifactId>
               </dependency></dependencies></project>"#,
            LOMBOK,
        );
        assert_eq!(found.as_deref(), Some("1.18.20"));
    }

    #[test]
    fn a_project_that_never_names_the_artifact_answers_nothing() {
        let found = version_in(
            r#"<project><groupId>a</groupId><artifactId>b</artifactId><version>1</version></project>"#,
            LOMBOK,
        );
        assert_eq!(found, None);
    }

    #[test]
    fn an_unexpandable_placeholder_is_not_a_version() {
        // `compare_versions` would read `${nope}` as 0 and gate the whole vocabulary away.
        let found = version_in(
            r#"<project><groupId>a</groupId><artifactId>b</artifactId><version>1</version>
               <dependencies><dependency>
                 <groupId>org.projectlombok</groupId><artifactId>lombok</artifactId>
                 <version>${nope}</version>
               </dependency></dependencies></project>"#,
            LOMBOK,
        );
        assert_eq!(found, None);
    }

    #[test]
    fn a_root_with_no_pom_answers_nothing_rather_than_failing() {
        assert_eq!(resolve(Path::new("/definitely/not/a/project"), LOMBOK), None);
    }
}
