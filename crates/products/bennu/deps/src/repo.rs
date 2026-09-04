//! The coordinate — and the local repository layout that is one written down.
//!
//! Maven's local repository is not a cache with opaque keys. It **is** the coordinate, spelled as a
//! directory path:
//!
//! ```text
//! ~/.m2/repository/org/springframework/spring-web/5.3.27/spring-web-5.3.27.jar
//!                  └────── groupId ─────┘ └artifactId┘ └version┘
//! ```
//!
//! So a resolved classpath — which is all Maven hands back — becomes coordinates without running
//! anything, and a coordinate becomes a path to `stat` without an index. Both directions live here,
//! **together and on purpose**: they are inverses, and the day they disagree is the day a jar that
//! is sitting in the repository reads as missing.
//!
//! That is not hypothetical. This was three implementations before it was one — a jar-path reader in
//! the sources downloader that insisted on a literal `repository` segment, this one that left the
//! group blank when it could not find the root, and a third that built paths in the other direction
//! without consulting either. Each was right about something the others got wrong, which is exactly
//! the shape a duplicated rule has just before it costs an afternoon.
//!
//! ## The two ways to read a path, and why both exist
//!
//! [`coord_of`] is for a path arriving **from outside** — a classpath entry, a jar the index
//! loaded — where nobody said where the repository root is. It finds the root by looking for a
//! `repository` or `.m2` segment, which is the layout of every machine that has not been
//! deliberately reconfigured; when neither is there the groupId is left **empty** rather than
//! guessed from a segment count, because a row reading `:commons-io 2.13.0` is obviously partial
//! while `com.acme:commons-io` would be a confident lie.
//!
//! [`coord_under`] is for a path **found by walking a known root**, where there is nothing to guess:
//! every segment between the root and the artifact directory is the group, full stop. A relocated
//! repository (`-Dmaven.repo.local=/fast/m2`) is answered exactly by this one and only partially by
//! the other, which is the whole reason they are two functions and not one with a flag.

use std::path::{Path, PathBuf};

/// A Maven coordinate — as much of one as whoever produced it knew.
///
/// `packaging` and `classifier` are part of the identity because they change the **file**: `tests`
/// and `jakarta` classifiers are ordinary in a legacy tree, and a `<type>pom</type>` dependency (a
/// BOM) has no jar at all — a check that looked for one would report every BOM in the project as
/// missing.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Coord {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    /// The classifier, when there is one (`spring-core-5.3.27-tests.jar`).
    pub classifier: String,
    /// `<type>`. Empty is read as `jar`, which is Maven's own default and what half the poms in the
    /// world rely on.
    pub packaging: String,
}

impl Coord {
    pub fn new(
        group_id: impl Into<String>,
        artifact_id: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            group_id: group_id.into(),
            artifact_id: artifact_id.into(),
            version: version.into(),
            ..Self::default()
        }
    }

    /// `groupId:artifactId` — the identity a version conflict is resolved on.
    pub fn ga(&self) -> String {
        format!("{}:{}", self.group_id, self.artifact_id)
    }

    /// Maven's own conflict key: the coordinate **without** the version, type and classifier
    /// included — `spring-core` and `spring-core:tests` are two artifacts, not one.
    pub fn key(&self) -> String {
        format!("{}:{}:{}:{}", self.group_id, self.artifact_id, self.kind(), self.classifier)
    }

    /// What a person reads, and what a diagnostic message says.
    pub fn gav(&self) -> String {
        let mut s = format!("{}:{}", self.group_id, self.artifact_id);
        if !self.classifier.is_empty() {
            s.push(':');
            s.push_str(&self.classifier);
        }
        if !self.version.is_empty() {
            s.push(':');
            s.push_str(&self.version);
        }
        s
    }

    /// The packaging, defaulted.
    pub fn kind(&self) -> &str {
        if self.packaging.is_empty() {
            "jar"
        } else {
            &self.packaging
        }
    }

    /// The file extension the artifact is stored under.
    ///
    /// Not the same as the packaging for the types that lie about it: a `test-jar` is a `.jar` with
    /// a `tests` classifier, and a `bundle` (the OSGi packaging half of Apache uses) is a plain
    /// `.jar`. Getting this wrong marks a dependency that is sitting right there as missing.
    pub fn extension(&self) -> &str {
        match self.kind() {
            "test-jar" | "bundle" | "maven-plugin" | "ejb" => "jar",
            other => other,
        }
    }

    /// The classifier the **file** carries, which for a `test-jar` is `tests` even though the
    /// dependency never writes one.
    pub fn file_classifier(&self) -> &str {
        match (self.kind(), self.classifier.as_str()) {
            ("test-jar", "") => "tests",
            (_, c) => c,
        }
    }

    /// Whether this names something with no jar to look for: a BOM, or a parent.
    pub fn is_pom(&self) -> bool {
        self.kind() == "pom"
    }

    pub fn is_complete(&self) -> bool {
        !self.group_id.is_empty() && !self.artifact_id.is_empty() && !self.version.is_empty()
    }

    /// The artifact's own file name (`spring-web-5.3.27.jar`, `spring-core-5.3.27-tests.jar`).
    pub fn file_name(&self) -> String {
        let mut name = format!("{}-{}", self.artifact_id, self.version);
        let classifier = self.file_classifier();
        if !classifier.is_empty() {
            name.push('-');
            name.push_str(classifier);
        }
        name.push('.');
        name.push_str(self.extension());
        name
    }

    /// The `.pom` beside it — always a real file, and the one that says what the artifact drags in.
    pub fn pom_file_name(&self) -> String {
        format!("{}-{}.pom", self.artifact_id, self.version)
    }

    /// `<group as dirs>/<artifactId>/<version>` under a repository root.
    pub fn version_dir(&self, repo_root: &Path) -> PathBuf {
        let mut p = self.artifact_dir(repo_root);
        p.push(&self.version);
        p
    }

    /// `<group as dirs>/<artifactId>` under a repository root — the directory the versions sit in.
    pub fn artifact_dir(&self, repo_root: &Path) -> PathBuf {
        let mut p = repo_root.to_path_buf();
        for segment in self.group_id.split('.').filter(|s| !s.is_empty()) {
            p.push(segment);
        }
        p.push(&self.artifact_id);
        p
    }

    /// Where the artifact itself would be under a repository root.
    pub fn file_in(&self, repo_root: &Path) -> PathBuf {
        self.version_dir(repo_root).join(self.file_name())
    }

    /// Where its `.pom` would be.
    pub fn pom_in(&self, repo_root: &Path) -> PathBuf {
        self.version_dir(repo_root).join(self.pom_file_name())
    }
}

/// The coordinate a repository path encodes, with the repository root **unknown** — a classpath
/// entry, a jar the index loaded. `None` when the path is not laid out like one (a `target/classes`
/// directory, a jar a `system`-scoped dependency points straight at).
///
/// See the module docs for why the groupId can come back empty here and never does in
/// [`coord_under`].
pub fn coord_of(path: &Path) -> Option<Coord> {
    let segments: Vec<String> =
        path.iter().map(|s| s.to_string_lossy().replace('\\', "/")).collect();
    let (artifact_id, version, classifier, packaging) = tail(&segments)?;
    Some(Coord {
        group_id: group_from(&segments[..segments.len() - 3]),
        artifact_id,
        version,
        classifier,
        packaging,
    })
}

/// The coordinate a path **under a known repository root** encodes — every segment between the two
/// is the group, with nothing to guess at.
///
/// Accepts either the artifact file or its version **directory**, because the two callers have one
/// each: a classpath entry is a file, and a repository walk stops at the directory.
pub fn coord_under(repo_root: &Path, path: &Path) -> Option<Coord> {
    let relative = path.strip_prefix(repo_root).ok()?;
    let segments: Vec<String> =
        relative.iter().map(|s| s.to_string_lossy().replace('\\', "/")).collect();

    // A file first, because [`tail`]'s layout check is decisive: it succeeds only when the last
    // segment really is named after the two directories above it. No `stat`, and no ambiguity — a
    // version directory cannot pass it, and an artifact file cannot fail it.
    if let Some((artifact_id, version, classifier, packaging)) = tail(&segments) {
        return Some(Coord {
            group_id: segments[..segments.len() - 3].join("."),
            artifact_id,
            version,
            classifier,
            packaging,
        });
    }
    // Otherwise the version **directory**, which is where a repository walk stops.
    version_dir_coord(&segments)
}

/// `<group…>/<artifactId>/<version>` read as a coordinate, with no file to consult.
fn version_dir_coord(segments: &[String]) -> Option<Coord> {
    let version = segments.last()?.clone();
    let artifact_id = segments.get(segments.len().checked_sub(2)?)?.clone();
    let group_id = segments[..segments.len() - 2].join(".");
    // A version is not a guess to make: it has to start with a digit, which is what tells
    // `spring-web/5.3.27` from a group segment that happens to sit three deep.
    version.starts_with(|c: char| c.is_ascii_digit()).then_some(Coord {
        group_id,
        artifact_id,
        version,
        ..Coord::default()
    })
}

/// The `(artifactId, version, classifier, packaging)` a repository **file** path ends with.
///
/// The layout check lives here: the file is named after the two directories above it. That is what
/// tells a repository jar from any other jar that happens to be three levels deep, and it is also
/// how the classifier is found.
fn tail(segments: &[String]) -> Option<(String, String, String, String)> {
    if segments.len() < 4 {
        return None;
    }
    let file = segments.last()?;
    let version = &segments[segments.len() - 2];
    let artifact_id = &segments[segments.len() - 3];
    let (stem, extension) = file.rsplit_once('.').unwrap_or((file.as_str(), ""));
    let head = format!("{artifact_id}-{version}");
    let classifier = match stem.strip_prefix(&head) {
        Some("") => String::new(),
        Some(rest) => rest.trim_start_matches('-').to_string(),
        None => return None,
    };
    // The extension is the packaging, except for `jar` which is left empty because it is the
    // default and writing it out would make two equal coordinates compare unequal.
    //
    // Deliberately NOT inferring `test-jar` from a `tests` classifier: the two spellings rebuild the
    // same path either way, and a pom that writes `<classifier>tests</classifier>` has to match a
    // jar read back off disk — blanking the classifier in favour of a packaging is how that match
    // silently stops working.
    let packaging = match extension {
        "jar" | "" => String::new(),
        other => other.to_string(),
    };
    Some((artifact_id.clone(), version.clone(), classifier, packaging))
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

    fn coord(p: &str) -> Option<Coord> {
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
        assert_eq!(c.ga(), "org.springframework:spring-web");
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
        // And not turned into a packaging: a pom writing `<classifier>tests</classifier>` has to
        // match this, and blanking it in favour of `test-jar` is how that match stops working.
        assert!(c.packaging.is_empty());
    }

    #[test]
    fn a_pom_reads_its_packaging_off_its_extension() {
        let c = coord("/r/.m2/repository/com/acme/bom/1.0/bom-1.0.pom").unwrap();
        assert!(c.is_pom());
        assert_eq!(c.kind(), "pom");
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

    /// …and with the root known, the same path costs nothing at all. This is the half that used to
    /// be a third implementation, in a crate that happened to know where the root was.
    #[test]
    fn a_known_root_makes_the_group_exact_wherever_the_repository_lives() {
        let root = PathBuf::from("D:/build-cache");
        let c = coord_under(&root, &PathBuf::from("D:/build-cache/org/x/widget/3.1/widget-3.1.jar")).unwrap();
        assert_eq!(c.ga(), "org.x:widget");
        assert_eq!(c.version, "3.1");
    }

    /// The repository walk stops at the version directory, where there is no file name to read.
    #[test]
    fn a_version_directory_is_a_coordinate_too() {
        let root = PathBuf::from("/r");
        let c = coord_under(&root, &PathBuf::from("/r/org/springframework/spring-web/5.3.27")).unwrap();
        assert_eq!(c.ga(), "org.springframework:spring-web");
        assert_eq!(c.version, "5.3.27");
        // A directory that is not a version is not a coordinate: `spring-web` is an artifact, and
        // reading it as `org:springframework@spring-web` would invent one.
        assert!(coord_under(&root, &PathBuf::from("/r/org/springframework/spring-web")).is_none());
    }

    /// The property the whole module exists for: reading a path and building one are inverses. When
    /// they drift, a jar that is sitting in the repository reads as missing.
    #[test]
    fn reading_a_path_and_building_one_are_inverses() {
        let root = PathBuf::from("/r");
        for path in [
            "/r/org/springframework/spring-web/5.3.27/spring-web-5.3.27.jar",
            "/r/org/x/core/1.0/core-1.0-tests.jar",
            "/r/com/acme/bom/1.0/bom-1.0.pom",
            "/r/junit/junit/4.13.2/junit-4.13.2.jar",
        ] {
            let coord = coord_under(&root, &PathBuf::from(path)).expect(path);
            let rebuilt = coord.file_in(&root).to_string_lossy().replace('\\', "/");
            assert_eq!(rebuilt, path, "{coord:?}");
        }
    }

    #[test]
    fn the_types_that_lie_about_their_file_name_still_resolve() {
        let root = PathBuf::from("/r");
        // A `test-jar` is a `.jar` with a `tests` classifier…
        let test_jar = Coord { packaging: "test-jar".into(), ..Coord::new("g", "core", "1.0") };
        assert!(test_jar.file_in(&root).to_string_lossy().ends_with("core-1.0-tests.jar"));
        // …and a `bundle` is a plain jar. Both used to read as "not in your repository" while
        // sitting in it.
        let bundle = Coord { packaging: "bundle".into(), ..Coord::new("g", "core", "1.0") };
        assert!(bundle.file_in(&root).to_string_lossy().ends_with("core-1.0.jar"));
    }

    #[test]
    fn the_conflict_key_separates_an_artifact_from_its_own_variants() {
        let core = Coord::new("g", "core", "1.0");
        let newer = Coord::new("g", "core", "2.0");
        let tests = Coord { classifier: "tests".into(), ..core.clone() };
        assert_eq!(core.key(), newer.key(), "the version is not part of identity");
        assert_ne!(core.key(), tests.key(), "the classifier is");
        assert_eq!(core.gav(), "g:core:1.0");
        assert_eq!(tests.gav(), "g:core:tests:1.0");
    }
}
