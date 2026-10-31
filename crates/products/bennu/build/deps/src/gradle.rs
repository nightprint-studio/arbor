//! The Gradle producer of the dependency [`Report`].
//!
//! Same four questions as the Maven one — *what is it*, *which version*, *who decided that*, *is it
//! there* — answered from the build scripts instead of the poms. The vocabulary lines up closely:
//!
//! | This field | Gradle |
//! |---|---|
//! | [`Dependency::scope`] | the **configuration**, folded into `compile` · `runtime` · `provided` · `test` · `processor` |
//! | [`Dependency::origin`] | declared · managed (a variable, the version catalog, a BOM) · inherited (`allprojects {}` / `subprojects {}`) |
//! | [`Dependency::kind`] | `platform` for a BOM, `test-fixtures`, else the artifact extension when not `jar` |
//! | [`Dependency::resolved`] | the jar in the Gradle cache (or `~/.m2` for `mavenLocal()`) |
//!
//! A `project(':x')` dependency is reported with the other project's **path** as its name, so the
//! module graph — which matches a dependency to a module by name — draws the edge.
//!
//! Nothing is run. What a build script *computes* (a dependency added in a loop, one a convention
//! plugin contributes) is invisible here; the panel's tree, which asks Gradle, is where that shows.

use std::path::{Path, PathBuf};

use bennu_gradle::prelude::{
    cache_coord_of, GradleModule, GradleProject, RawDep, VersionOrigin,
};

use crate::model::{Dependency, Module, Origin, Report, Site, Transitive};
use crate::repo::coord_of as m2_coord_of;

/// Read a Gradle build's dependencies.
///
/// `jars` is the already-resolved classpath (what the index service holds). Empty means none has
/// been resolved yet, and every dependency then comes back unresolved with
/// [`Report::resolved_known`] `false` — *unknown*, not *missing*.
pub fn read(root: &Path, jars: &[PathBuf]) -> Report {
    let project = bennu_gradle::prelude::read(root);
    let index = JarIndex::build(jars);
    let mut report = Report {
        ecosystem: "gradle".to_string(),
        resolved_known: !jars.is_empty(),
        ..Report::default()
    };

    for module in &project.modules {
        let inherited = project.inherited_dependencies(module);
        // A project that only groups others — no script, nothing inherited — has no dependencies
        // to show and would be an empty group in the panel. The root is always kept: it is where
        // `allprojects {}` lives.
        if module.build_file.is_none() && inherited.is_empty() && !module.is_root() {
            continue;
        }
        let mut dependencies = Vec::new();
        for raw in &module.info.dependencies {
            dependencies.push(convert(raw, module, module, None, &project, &index));
        }
        for (raw, owner) in inherited {
            dependencies.push(convert(raw, module, owner, Some(owner), &project, &index));
        }
        report.modules.push(Module {
            name: module.name.clone(),
            id: module_id(module, &project),
            manifest: manifest_of(module, &project),
            kind: module.packaging().to_string(),
            dependencies,
        });
    }

    report.transitive = index.unclaimed(&report);
    report
}

/// The identifier a project is known by in the report: its Gradle path, or the build's name for the
/// root. Unique within the build, unlike the last segment (`:a:util` and `:b:util`).
fn module_id(module: &GradleModule, project: &GradleProject) -> String {
    if module.is_root() {
        project.name.clone()
    } else {
        module.path.clone()
    }
}

/// The file a module's row opens: its build script, else the settings script, else nothing.
fn manifest_of(module: &GradleModule, project: &GradleProject) -> String {
    module
        .build_file
        .as_deref()
        .or(project.settings_file.as_deref())
        .map(wire_path)
        .unwrap_or_default()
}

fn wire_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// One declaration, with everything that can be answered filled in.
///
/// `owner` is the project whose script wrote it (the module itself, or an ancestor for an
/// inherited one); `inherited_from` is `Some` only for the latter.
fn convert(
    raw: &RawDep,
    module: &GradleModule,
    owner: &GradleModule,
    inherited_from: Option<&GradleModule>,
    project: &GradleProject,
    index: &JarIndex,
) -> Dependency {
    let site = Site {
        file: owner.build_file.as_deref().map(wire_path).unwrap_or_default(),
        offset: raw.offset,
        line: raw.line,
    };
    let scope = scope_of(&raw.configuration);

    if !raw.project.is_empty() {
        let target = project.find_project(&raw.project);
        let name = target.map(|t| module_id(t, project)).unwrap_or_else(|| raw.project.clone());
        return Dependency {
            name,
            scope,
            kind: "project".to_string(),
            origin: inherited_from.map(|o| Origin::Inherited { from: o.name.clone() }).unwrap_or(Origin::Declared),
            declared_in: site,
            ..Dependency::default()
        };
    }

    let origin = match inherited_from {
        Some(o) => Origin::Inherited { from: o.name.clone() },
        None => origin_of(raw, module, project),
    };
    let jar = index.find(&raw.group, &raw.name, &raw.version);
    // The classpath settles a version the scripts could not (a BOM's), and that is not a guess: it
    // is the jar the compiler is handed.
    let version = if raw.version.is_empty() {
        jar.map(|j| j.version.clone()).unwrap_or_default()
    } else {
        raw.version.clone()
    };
    let kind = if !raw.kind.is_empty() {
        raw.kind.clone()
    } else if !raw.extension.is_empty() && raw.extension != "jar" {
        raw.extension.clone()
    } else {
        String::new()
    };
    Dependency {
        group: raw.group.clone(),
        name: raw.name.clone(),
        version,
        scope,
        kind,
        variant: raw.classifier.clone(),
        origin,
        declared_in: site,
        resolved: jar.map(|j| j.path.clone()).unwrap_or_default(),
        ..Dependency::default()
    }
}

/// Who decided the version: the build script itself, or something further away.
fn origin_of(raw: &RawDep, module: &GradleModule, project: &GradleProject) -> Origin {
    match &raw.origin {
        VersionOrigin::Literal => Origin::Declared,
        VersionOrigin::Property(name) => Origin::Managed { from: format!("{name} (variable)") },
        VersionOrigin::Catalog { catalog, alias } => {
            let file = project
                .catalogs
                .iter()
                .find(|c| &c.name == catalog)
                .and_then(|c| Path::new(&c.file).file_name().map(|f| f.to_string_lossy().into_owned()))
                .unwrap_or_else(|| format!("{catalog}.versions.toml"));
            Origin::Managed { from: format!("{file} · {alias}") }
        }
        VersionOrigin::Unversioned => match unversioned_source(module, project) {
            Some(from) => Origin::Managed { from },
            None => Origin::Declared,
        },
    }
}

/// What can be supplying the version of a dependency that names none: a BOM, or a plugin whose job
/// that is.
fn unversioned_source(module: &GradleModule, project: &GradleProject) -> Option<String> {
    let root = project.modules.first();
    let applies = |id: &str| {
        module.info.applies(id) || root.map(|r| r.info.plugins.iter().any(|p| p.id == id)).unwrap_or(false)
    };
    if applies("org.springframework.boot") {
        return Some("Spring Boot's dependency management".to_string());
    }
    if applies("io.spring.dependency-management") {
        return Some("the dependency-management plugin".to_string());
    }
    if let Some(bom) = module.info.boms.first().or_else(|| root.and_then(|r| r.info.boms.first())) {
        return Some(format!("BOM {bom}"));
    }
    let has_platform = |m: &GradleModule| m.info.dependencies.iter().any(|d| d.kind == "platform");
    if has_platform(module) || root.map(has_platform).unwrap_or(false) {
        return Some("a platform (BOM)".to_string());
    }
    None
}

/// A Gradle configuration, in the words the panel's `scope` column already uses for Maven.
fn scope_of(configuration: &str) -> String {
    let lower = configuration.to_ascii_lowercase();
    let scope = if lower.contains("test") {
        "test"
    } else if lower == "compileonly" || lower == "compileonlyapi" {
        "provided"
    } else if lower.starts_with("runtime") {
        "runtime"
    } else if lower == "annotationprocessor" || lower == "kapt" || lower == "ksp" {
        "processor"
    } else if matches!(lower.as_str(), "implementation" | "api" | "compile") {
        "compile"
    } else {
        return configuration.to_string();
    };
    scope.to_string()
}

// ── the resolved classpath, indexed ───────────────────────────────────────────

struct Jar {
    group: String,
    name: String,
    version: String,
    path: String,
}

struct JarIndex {
    jars: Vec<Jar>,
}

impl JarIndex {
    /// Each classpath entry read back as a coordinate: from the Gradle cache layout first, then the
    /// Maven one (`mavenLocal()`). An entry that is neither — a project's own `build/libs` jar, a
    /// `files(…)` dependency — is not a coordinate and is left out.
    fn build(paths: &[PathBuf]) -> Self {
        let jars = paths
            .iter()
            .filter_map(|p| {
                let path = wire_path(p);
                if let Some((group, name, version)) = cache_coord_of(p) {
                    return Some(Jar { group, name, version, path });
                }
                m2_coord_of(p).map(|c| Jar { group: c.group_id, name: c.artifact_id, version: c.version, path })
            })
            .collect();
        JarIndex { jars }
    }

    /// The jar of a declared dependency: group and name match, then the version when the script
    /// wrote one. Several versions with none asked for match nothing rather than the first — a
    /// version the project may not be using is worse than none.
    fn find(&self, group: &str, name: &str, version: &str) -> Option<&Jar> {
        let mut hits = self.jars.iter().filter(|j| {
            j.name == name && (group.is_empty() || j.group.is_empty() || j.group == group)
        });
        if version.is_empty() || !is_plain(version) {
            let first = hits.next()?;
            return hits.next().is_none().then_some(first);
        }
        hits.find(|j| j.version == version)
    }

    /// The jars no declaration accounted for: what the declared dependencies dragged in.
    fn unclaimed(&self, report: &Report) -> Vec<Transitive> {
        let claimed: Vec<&str> = report
            .modules
            .iter()
            .flat_map(|m| m.dependencies.iter())
            .filter(|d| !d.resolved.is_empty())
            .map(|d| d.resolved.as_str())
            .collect();
        let mut out: Vec<Transitive> = self
            .jars
            .iter()
            .filter(|j| !claimed.contains(&j.path.as_str()))
            .map(|j| Transitive {
                group: j.group.clone(),
                name: j.name.clone(),
                version: j.version.clone(),
                resolved: j.path.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.coord().cmp(&b.coord()).then_with(|| a.version.cmp(&b.version)));
        out
    }
}

/// A version that names one directory — not a range, a dynamic selector or an unexpanded variable.
fn is_plain(version: &str) -> bool {
    !version.contains(&['+', '[', ']', '(', ')', ',', '$'][..]) && !version.starts_with("latest")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bennu-deps-gradle-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (f, text) in files {
            let full = dir.join(f);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        dir
    }

    fn cache_jar(group: &str, name: &str, version: &str) -> PathBuf {
        PathBuf::from(format!(
            "/home/u/.gradle/caches/modules-2/files-2.1/{group}/{name}/{version}/hash/{name}-{version}.jar"
        ))
    }

    #[test]
    fn configurations_fold_into_the_scopes_the_panel_knows() {
        for (configuration, scope) in [
            ("implementation", "compile"),
            ("api", "compile"),
            ("compileOnly", "provided"),
            ("runtimeOnly", "runtime"),
            ("testImplementation", "test"),
            ("integrationTestRuntimeOnly", "test"),
            ("annotationProcessor", "processor"),
            ("customThing", "customThing"),
        ] {
            assert_eq!(scope_of(configuration), scope, "{configuration}");
        }
    }

    #[test]
    fn a_report_says_what_is_declared_where_and_whether_it_resolved() {
        let dir = build(
            "report",
            &[
                ("settings.gradle", "rootProject.name = 'shop'\ninclude 'api', 'core'\n"),
                ("build.gradle", "subprojects { dependencies { testImplementation 'junit:junit:4.13' } }\n"),
                (
                    "api/build.gradle",
                    "plugins { id 'java' }\ndependencies {\n  implementation project(':core')\n  implementation 'org.x:lib:1.0'\n  implementation 'org.y:gone:2.0'\n}\n",
                ),
                ("core/build.gradle", "plugins { id 'java' }\n"),
            ],
        );
        let report = read(&dir, &[cache_jar("org.x", "lib", "1.0"), cache_jar("org.z", "dragged", "3.0")]);
        assert_eq!(report.ecosystem, "gradle");
        assert!(report.resolved_known);

        let api = report.modules.iter().find(|m| m.id == ":api").expect("api module");
        let by_name = |n: &str| api.dependencies.iter().find(|d| d.name == n).unwrap();
        assert_eq!(by_name(":core").kind, "project");
        assert!(by_name("lib").resolved.ends_with("lib-1.0.jar"));
        assert!(by_name("gone").resolved.is_empty());
        // `subprojects {}` reaches the child and says who sent it.
        assert!(matches!(by_name("junit").origin, Origin::Inherited { .. }));
        assert_eq!(by_name("junit").scope, "test");

        assert_eq!(report.transitive.len(), 1);
        assert_eq!(report.transitive[0].name, "dragged");
    }

    #[test]
    fn a_version_the_scripts_cannot_name_is_read_off_the_classpath() {
        let dir = build(
            "bom",
            &[("build.gradle", "plugins { id 'java'; id 'org.springframework.boot' version '3.1.0' }\ndependencies { implementation 'org.springframework:spring-web' }\n")],
        );
        let report = read(&dir, &[cache_jar("org.springframework", "spring-web", "6.0.9")]);
        let dep = &report.modules[0].dependencies[0];
        assert_eq!(dep.version, "6.0.9");
        assert!(matches!(&dep.origin, Origin::Managed { from } if from.contains("Spring Boot")));
    }

    #[test]
    fn nothing_resolved_yet_is_unknown_not_missing() {
        let dir = build("cold", &[("build.gradle", "dependencies { implementation 'a:b:1' }\n")]);
        let report = read(&dir, &[]);
        assert!(!report.resolved_known);
        assert_eq!(report.unresolved_count(), 0);
    }
}
