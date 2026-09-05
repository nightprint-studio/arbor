//! The pom editor's answers, against a repository built for the test.
//!
//! The unit tests in the crate cover the pieces — the layout, the version order, the inheritance,
//! the transitive walk. This covers the thing a user sees: a pom, a repository that has some of
//! what it asks for, and the three answers that follow.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bennu_maven::prelude::{
    effective_of_buffer, pom_completions, pom_diagnostics, Catalog, LocalRepo, PomDoc, PomEnv,
};

/// A temporary local repository plus a project directory.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("bennu-maven-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("proj")).unwrap();
        Self { dir }
    }

    fn install(&self, group: &str, artifact: &str, version: &str) {
        let d = self.dir.join("m2").join(group.replace('.', "/")).join(artifact).join(version);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(format!("{artifact}-{version}.jar")), b"x").unwrap();
        std::fs::write(
            d.join(format!("{artifact}-{version}.pom")),
            format!(
                "<project><groupId>{group}</groupId><artifactId>{artifact}</artifactId>\
                 <version>{version}</version></project>"
            ),
        )
        .unwrap();
    }

    /// A dependency whose **pom** is in the repository and whose jar is not — the state Maven
    /// leaves behind when it walks a dependency graph without ever compiling against it. The
    /// commonest shape of "unresolved" on a project that has never been built, and the one that
    /// used to be reported as though the version were wrong.
    fn install_pom_only(&self, group: &str, artifact: &str, version: &str) {
        let d = self.dir.join("m2").join(group.replace('.', "/")).join(artifact).join(version);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join(format!("{artifact}-{version}.pom")),
            format!(
                "<project><groupId>{group}</groupId><artifactId>{artifact}</artifactId>\
                 <version>{version}</version></project>"
            ),
        )
        .unwrap();
    }

    fn repo(&self) -> LocalRepo {
        LocalRepo::at(self.dir.join("m2"))
    }

    fn pom_path(&self) -> PathBuf {
        self.dir.join("proj").join("pom.xml")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Run `f` with a `PomEnv` over `source`, as the extension builds one.
fn with_env<T>(f: &Fixture, source: &str, body: impl FnOnce(&PomEnv<'_>, &PomDoc<'_>) -> T) -> T {
    let repo = f.repo();
    let catalog = Catalog::scan(&repo);
    let effective = effective_of_buffer(&repo, &f.pom_path(), source);
    let reactor: HashMap<String, String> = HashMap::from([(
        "com.acme:app".to_string(),
        f.pom_path().to_string_lossy().replace('\\', "/"),
    )]);
    let path = f.pom_path().to_string_lossy().replace('\\', "/");
    let env = PomEnv {
        repo: &repo,
        catalog: &catalog,
        reactor: &reactor,
        effective: &effective,
        path: &path,
    };
    let doc = PomDoc::new(source);
    body(&env, &doc)
}

fn dependency(group: &str, artifact: &str, version: &str) -> String {
    format!(
        "<dependency><groupId>{group}</groupId><artifactId>{artifact}</artifactId>\
         <version>{version}</version></dependency>"
    )
}

fn project(body: &str) -> String {
    format!(
        "<project><modelVersion>4.0.0</modelVersion><groupId>com.acme</groupId>\
         <artifactId>app</artifactId><version>1.0</version>{body}</project>"
    )
}

/// The report that makes the feature worth having: the coordinate is underlined where it is
/// written, and the message names it.
#[test]
fn a_dependency_that_is_not_installed_is_marked_at_its_artifact_id() {
    let f = Fixture::new("missing");
    f.install("org.slf4j", "slf4j-api", "1.7.36");
    let source = project(&format!(
        "<dependencies>{}{}</dependencies>",
        dependency("org.slf4j", "slf4j-api", "1.7.36"),
        dependency("com.acme", "legacy-core", "2.4.0")
    ));
    let (message, marked) = with_env(&f, &source, |env, doc| {
        let diags = pom_diagnostics(env, doc);
        let hit = diags
            .iter()
            .find(|d| d.code == "maven-unresolved-dependency")
            .expect("the missing one is reported");
        (hit.message.clone(), source[hit.start..hit.end].to_string())
    });
    assert!(message.contains("com.acme:legacy-core:2.4.0"), "{message}");
    assert_eq!(marked, "legacy-core", "underlined where it is written");
}

/// The third state, and the one a reader is most likely to meet: the version IS in the repository,
/// as a pom, and its jar never arrived.
///
/// It used to be reported with the wrong-version message, which listed the very version it called
/// missing — *"version `2.21.1` … is not in the local repository. Installed: …, 2.21.1, …"*. That
/// contradiction sent a reader to check a folder that was right there and to conclude the tool was
/// broken. `versions` and `resolve` ask two different questions — does this version exist, and is
/// its jar on disk — and the message has to say which one it asked.
#[test]
fn a_version_present_as_a_pom_without_its_jar_says_so() {
    let f = Fixture::new("pom-only");
    f.install_pom_only("com.fasterxml.jackson.core", "jackson-databind", "2.21.1");
    let source = project(&format!(
        "<dependencies>{}</dependencies>",
        dependency("com.fasterxml.jackson.core", "jackson-databind", "2.21.1")
    ));
    let message = with_env(&f, &source, |env, doc| {
        pom_diagnostics(env, doc)
            .iter()
            .find(|d| d.code == "maven-unresolved-dependency")
            .expect("still reported — the jar really is missing")
            .message
            .clone()
    });
    assert!(message.contains("as a pom"), "names the state it is in: {message}");
    assert!(message.contains("jar was never downloaded"), "{message}");
    // The contradiction that made the old message useless.
    assert!(
        !message.contains("Installed:"),
        "must not list the version it is reporting as missing: {message}"
    );
}

/// The one that resolves must not be marked, and neither must the project's own module — the two
/// false positives that would make the whole feature unusable.
#[test]
fn what_resolves_and_what_is_built_from_source_are_both_left_alone() {
    let f = Fixture::new("clean");
    f.install("org.slf4j", "slf4j-api", "1.7.36");
    let source = project(&format!(
        "<dependencies>{}{}</dependencies>",
        dependency("org.slf4j", "slf4j-api", "1.7.36"),
        // A sibling module of this same project: built from source, never in a repository.
        dependency("com.acme", "app", "1.0")
    ));
    let codes = with_env(&f, &source, |env, doc| {
        pom_diagnostics(env, doc).into_iter().map(|d| d.code).collect::<Vec<_>>()
    });
    assert!(!codes.contains(&"maven-unresolved-dependency".to_string()), "{codes:?}");
}

/// A wrong version and a wrong artifactId are the same symptom and different mistakes, so they get
/// different messages — and the versions you do have are listed.
#[test]
fn a_version_that_is_not_installed_says_which_ones_are() {
    let f = Fixture::new("version");
    f.install("org.slf4j", "slf4j-api", "1.7.36");
    f.install("org.slf4j", "slf4j-api", "2.0.9");
    let source = project(&format!(
        "<dependencies>{}</dependencies>",
        dependency("org.slf4j", "slf4j-api", "1.7.30")
    ));
    let message = with_env(&f, &source, |env, doc| {
        pom_diagnostics(env, doc)
            .into_iter()
            .find(|d| d.code == "maven-unresolved-dependency")
            .map(|d| d.message)
            .unwrap_or_default()
    });
    assert!(message.contains("2.0.9") && message.contains("1.7.36"), "{message}");
}

/// A `${…}` nothing defines resolves to the literal text and then fails to find an artifact by that
/// name — reported as the property it is, not as a missing dependency.
#[test]
fn an_undefined_property_is_reported_as_itself() {
    let f = Fixture::new("property");
    let source = project(
        "<dependencies><dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId>\
         <version>${slf4j.version}</version></dependency></dependencies>",
    );
    let codes = with_env(&f, &source, |env, doc| {
        pom_diagnostics(env, doc).into_iter().map(|d| d.code).collect::<Vec<_>>()
    });
    assert!(codes.contains(&"maven-undefined-property".to_string()), "{codes:?}");
    // And NOT as an unresolved artifact: the coordinate could not be judged at all.
    assert!(!codes.contains(&"maven-unresolved-dependency".to_string()), "{codes:?}");
}

/// The completion that exists because a pom's values are content rather than names.
#[test]
fn a_version_completes_from_the_versions_you_have_newest_first() {
    let f = Fixture::new("complete-version");
    f.install("org.slf4j", "slf4j-api", "1.7.36");
    f.install("org.slf4j", "slf4j-api", "2.0.9");
    let source = project(
        "<dependencies><dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId>\
         <version></version></dependency></dependencies>",
    );
    let at = source.find("<version></version>").unwrap() + "<version>".len();
    let labels = with_env(&f, &source, |env, doc| {
        pom_completions(env, doc, at).into_iter().map(|c| c.label).collect::<Vec<_>>()
    });
    assert_eq!(labels, ["2.0.9", "1.7.36"]);
}

/// Completing an artifactId with the groupId above it still empty writes both — the range spans the
/// two elements and keeps the markup between them exactly as it is.
#[test]
fn an_artifact_id_completion_fills_the_group_id_too() {
    let f = Fixture::new("complete-artifact");
    f.install("org.slf4j", "slf4j-api", "2.0.9");
    let source = project(
        "<dependencies><dependency><groupId></groupId><artifactId>slf4j</artifactId>\
         </dependency></dependencies>",
    );
    let at = source.find("slf4j</artifactId>").unwrap() + "slf4j".len();
    let item = with_env(&f, &source, |env, doc| {
        pom_completions(env, doc, at)
            .into_iter()
            .find(|c| c.label == "slf4j-api")
            .expect("the artifact is offered")
    });
    let (start, end) = (item.replace_start.unwrap(), item.replace_end.unwrap());
    let mut written = source.clone();
    written.replace_range(start..end, item.insert_text.as_deref().unwrap_or(&item.label));
    assert!(written.contains("<groupId>org.slf4j</groupId>"), "{written}");
    assert!(written.contains("<artifactId>slf4j-api</artifactId>"), "{written}");
}

/// No repository, no claims. A machine whose `~/.m2` has never been populated must not have every
/// dependency of every project underlined.
#[test]
fn an_empty_repository_produces_no_resolution_reports_at_all() {
    let f = Fixture::new("empty-repo");
    let source = project(&format!(
        "<dependencies>{}</dependencies>",
        dependency("com.acme", "legacy-core", "2.4.0")
    ));
    let codes = with_env(&f, &source, |env, doc| {
        pom_diagnostics(env, doc).into_iter().map(|d| d.code).collect::<Vec<_>>()
    });
    assert!(!codes.contains(&"maven-unresolved-dependency".to_string()), "{codes:?}");
}

/// A `<module>` with no pom silently drops out of the reactor, and every type in it then stops
/// resolving everywhere else — with nothing anywhere saying so.
#[test]
fn a_module_that_is_not_there_is_reported() {
    let f = Fixture::new("module");
    std::fs::create_dir_all(f.dir.join("proj").join("core")).unwrap();
    std::fs::write(f.dir.join("proj").join("core").join("pom.xml"), "<project/>").unwrap();
    let source = project("<modules><module>core</module><module>gone</module></modules>");
    let messages = with_env(&f, &source, |env, doc| {
        pom_diagnostics(env, doc)
            .into_iter()
            .filter(|d| d.code == "maven-missing-module")
            .map(|d| d.message)
            .collect::<Vec<_>>()
    });
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("gone"), "{messages:?}");
}

/// The path is not assumed: a repository named by `settings.xml` is the one consulted.
#[test]
fn the_repository_layout_is_the_coordinate() {
    let f = Fixture::new("layout");
    f.install("org.springframework", "spring-web", "5.3.27");
    let repo = f.repo();
    let coord = bennu_maven::prelude::Coord::new("org.springframework", "spring-web", "5.3.27");
    let resolved = repo.resolve(&coord).expect("installed");
    assert!(resolved.ends_with(Path::new("spring-web-5.3.27.jar")));
    assert_eq!(repo.latest("org.springframework", "spring-web").as_deref(), Some("5.3.27"));
}
