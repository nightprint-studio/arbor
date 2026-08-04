//! Reading a coordinate back off a jar in the local repository.
//!
//! Maven's local repository is not a cache with opaque keys — it is the coordinate, written as a
//! directory path:
//!
//! ```text
//! ~/.m2/repository/org/springframework/spring-web/5.3.27/spring-web-5.3.27.jar
//!                  └────── groupId ─────┘ └artifactId┘ └version┘
//! ```
//!
//! So a resolved classpath — which is all Maven hands back — can be turned into coordinates
//! without running anything, which is the whole reason the panel can show what a project *got*
//! next to what it *asked for*.
//!
//! ## The one uncertain part, and how it fails
//!
//! The artifactId and the version are the last two directories, always. The groupId is everything
//! between the repository root and the artifactId — and where that root is depends on
//! `maven.repo.local`, which can point anywhere. It is found by looking for a `repository` or
//! `.m2` segment, the layout of every machine that has not been deliberately reconfigured.
//!
//! When neither is there the groupId is left **empty** rather than guessed from a segment count:
//! a row reading `:commons-io 2.13.0` is obviously partial, while `com.acme:commons-io` would be
//! a confident lie.

use std::path::Path;

/// A coordinate read off a jar path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JarCoord {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    /// The classifier, when the file name carries one (`spring-core-5.3.27-tests.jar`).
    pub classifier: String,
}

impl JarCoord {
    pub fn coord(&self) -> String {
        format!("{}:{}", self.group_id, self.artifact_id)
    }
}

/// The coordinate a repository jar path encodes, or `None` when the path is not laid out like one
/// (a `target/classes` directory, a jar pointed at directly by a `system`-scoped dependency).
pub fn coord_of(path: &Path) -> Option<JarCoord> {
    let segments: Vec<String> =
        path.iter().map(|s| s.to_string_lossy().replace('\\', "/")).collect();
    // `<…>/<group…>/<artifactId>/<version>/<file>` — four segments at the very least.
    if segments.len() < 4 {
        return None;
    }
    let file = segments.last()?;
    let version = &segments[segments.len() - 2];
    let artifact_id = &segments[segments.len() - 3];

    // The layout check: the file is named after the two directories above it. It is what tells a
    // repository jar from any other jar that happens to be three levels deep, and it is also how
    // the classifier is found.
    let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
    let head = format!("{artifact_id}-{version}");
    let classifier = match stem.strip_prefix(&head) {
        Some("") => String::new(),
        Some(rest) => rest.trim_start_matches('-').to_string(),
        None => return None,
    };

    Some(JarCoord {
        group_id: group_from(&segments[..segments.len() - 3]),
        artifact_id: artifact_id.clone(),
        version: version.clone(),
        classifier,
    })
}

/// The groupId from the path segments above the artifact directory: everything after the
/// repository root, dot-joined. Empty when the root cannot be identified — see the module docs.
fn group_from(above: &[String]) -> String {
    let root = above
        .iter()
        .rposition(|s| s == "repository")
        .or_else(|| above.iter().rposition(|s| s == ".m2"));
    match root {
        Some(i) => above[i + 1..].join("."),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn coord(p: &str) -> Option<JarCoord> {
        coord_of(&PathBuf::from(p))
    }

    #[test]
    fn a_repository_jar_is_its_own_coordinate() {
        let c = coord("C:/Users/u/.m2/repository/org/springframework/spring-web/5.3.27/spring-web-5.3.27.jar")
            .unwrap();
        assert_eq!(c.group_id, "org.springframework");
        assert_eq!(c.artifact_id, "spring-web");
        assert_eq!(c.version, "5.3.27");
        assert!(c.classifier.is_empty());
        assert_eq!(c.coord(), "org.springframework:spring-web");
    }

    #[test]
    fn a_single_segment_group_and_a_unix_repository_both_work() {
        let c = coord("/home/u/.m2/repository/junit/junit/4.13.2/junit-4.13.2.jar").unwrap();
        assert_eq!((c.group_id.as_str(), c.artifact_id.as_str()), ("junit", "junit"));
    }

    #[test]
    fn a_classifier_is_read_off_the_file_name() {
        let c = coord("/r/.m2/repository/org/x/core/1.0/core-1.0-tests.jar").unwrap();
        assert_eq!(c.classifier, "tests");
        assert_eq!(c.version, "1.0");
    }

    /// A version like `1.0-SNAPSHOT` is a hyphenated string in both the directory and the file
    /// name; splitting the file name on `-` instead of matching the directories gets it wrong.
    #[test]
    fn a_hyphenated_version_is_not_mistaken_for_a_classifier() {
        let c = coord("/r/.m2/repository/com/acme/portale-core/2.4.0-SNAPSHOT/portale-core-2.4.0-SNAPSHOT.jar")
            .unwrap();
        assert_eq!(c.artifact_id, "portale-core");
        assert_eq!(c.version, "2.4.0-SNAPSHOT");
        assert!(c.classifier.is_empty());
    }

    #[test]
    fn a_path_that_is_not_repository_shaped_is_refused_rather_than_guessed() {
        // A module's own build output, which does appear on a reactor classpath.
        assert!(coord("C:/proj/module/target/classes").is_none());
        // A jar whose name has nothing to do with the directories above it.
        assert!(coord("C:/libs/vendor/1.0/something-else.jar").is_none());
        assert!(coord("x.jar").is_none());
    }

    /// A relocated local repository (`-Dmaven.repo.local`) still yields the artifact and the
    /// version, and says nothing about the group rather than inventing one.
    #[test]
    fn an_unrecognisable_repository_root_costs_the_group_and_nothing_else() {
        let c = coord("D:/build-cache/org/x/widget/3.1/widget-3.1.jar").unwrap();
        assert_eq!((c.artifact_id.as_str(), c.version.as_str()), ("widget", "3.1"));
        assert!(c.group_id.is_empty());
    }
}
