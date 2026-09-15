//! The Maven reactor as a build graph — which modules a module is built FROM.
//!
//! A launch compiles and runs one module, and three questions follow from that which the pom
//! `<modules>` list alone cannot answer:
//!
//! - **what to put on the classpath** besides the module: its upstream siblings' `target/classes`,
//!   and not every module in the project — a sibling nobody depends on brings its own
//!   `application.yml`, its own `spring.factories` and its own `@Component`s into a JVM that never
//!   asked for them;
//! - **what a change invalidates**: an edit in `core` makes `web`'s classes stale, an edit in `batch`
//!   does not;
//! - **which jars are really siblings**: a module resolved outside its reactor names a sibling as a
//!   jar in `~/.m2`, which is whatever the last `mvn install` left there.
//!
//! Read from the poms by artifactId, which is how Maven matches a dependency to a reactor project
//! (the groupId is almost always inherited from the parent and so absent from the module's own pom).
//! Everything here except [`load`] is pure.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One module of the reactor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactorModule {
    /// Relative to the reactor root, `/`-separated. Empty = the root project itself.
    pub(crate) rel: String,
    /// The module's own `<artifactId>`, lowercased.
    pub(crate) artifact_id: String,
    /// The artifactIds (lowercased) of every `<dependency>` the pom declares.
    pub(crate) dependencies: Vec<String>,
}

/// Normalise a module path as callers send it (`services\core/`, ` web `) to [`ReactorModule::rel`].
pub(crate) fn normalize_rel(module: &str) -> String {
    module.trim().replace('\\', "/").trim_matches('/').to_string()
}

/// Read the reactor rooted at `root`: the root project plus every module [`crate::build::module_dirs`]
/// finds. One small pom read per module.
pub(crate) fn load(root: &Path) -> Vec<ReactorModule> {
    let mut dirs = vec![root.to_path_buf()];
    dirs.extend(crate::build::module_dirs(root));
    dirs.iter()
        .filter_map(|dir| {
            let xml = std::fs::read_to_string(dir.join("pom.xml")).ok()?;
            let pom = bennu_project::prelude::parse_pom(&xml);
            let rel = dir
                .strip_prefix(root)
                .map(|p| normalize_rel(&p.to_string_lossy()))
                .unwrap_or_default();
            Some(ReactorModule {
                rel,
                artifact_id: pom.artifact_id.to_ascii_lowercase(),
                dependencies: pom
                    .dependencies
                    .iter()
                    .filter_map(|d| d.rsplit(':').next())
                    .map(str::to_string)
                    .collect(),
            })
        })
        .collect()
}

/// The modules `target` is built from, transitively, **dependencies before dependents** — the
/// order `-am` builds them in and the order their classes belong on a classpath. `target` itself is
/// not included. Unknown `target` → empty.
pub(crate) fn upstream_of(modules: &[ReactorModule], target: &str) -> Vec<String> {
    let by_artifact: HashMap<&str, &ReactorModule> = modules
        .iter()
        .filter(|m| !m.artifact_id.is_empty())
        .map(|m| (m.artifact_id.as_str(), m))
        .collect();
    let Some(start) = modules.iter().find(|m| m.rel == target) else { return Vec::new() };

    // Post-order DFS: a module is emitted after everything it depends on. `visiting` breaks a cycle
    // (Maven refuses one, but a half-edited pom can hold one and this must still terminate).
    fn visit<'a>(
        m: &'a ReactorModule,
        by_artifact: &HashMap<&str, &'a ReactorModule>,
        visiting: &mut HashSet<&'a str>,
        out: &mut Vec<String>,
    ) {
        if !visiting.insert(m.rel.as_str()) {
            return;
        }
        for dep in &m.dependencies {
            if let Some(up) = by_artifact.get(dep.as_str()) {
                if up.rel != m.rel {
                    visit(up, by_artifact, visiting, out);
                }
            }
        }
        if !out.contains(&m.rel) {
            out.push(m.rel.clone());
        }
    }
    let mut out = Vec::new();
    let mut visiting = HashSet::new();
    visit(start, &by_artifact, &mut visiting, &mut out);
    out.retain(|r| r != target);
    out
}

/// The aggregator directories between the root and `rel`, root first, `rel` excluded — the poms a
/// module inherits from in the usual layout, whose edits (a managed version, a compiler flag) change
/// how the module builds.
pub(crate) fn ancestors_of(rel: &str) -> Vec<String> {
    if rel.is_empty() {
        return Vec::new();
    }
    let parts: Vec<&str> = rel.split('/').collect();
    (0..parts.len()).map(|n| parts[..n].join("/")).collect()
}

/// Replace every classpath entry that is a **reactor module's own artifact** with that module's
/// `target/classes`, keeping the position.
///
/// A module resolved outside its reactor names a sibling as `~/.m2/…/core/1.0/core-1.0.jar` or
/// `core/target/core-1.0.jar` — a copy frozen at the last `install` or `package`, holding classes
/// whose sources have been edited, renamed or deleted since. `classes_dir` answers the directory for
/// a module rel; an entry is replaced only when that directory exists, so a sibling that has never
/// been compiled keeps its jar rather than vanishing.
pub(crate) fn substitute_sibling_artifacts(
    entries: Vec<PathBuf>,
    modules: &[ReactorModule],
    classes_dir: impl Fn(&str) -> Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::with_capacity(entries.len());
    for entry in entries {
        let replacement = sibling_of(&entry, modules).and_then(|m| classes_dir(m.rel.as_str()));
        let next = replacement.unwrap_or(entry);
        if !out.contains(&next) {
            out.push(next);
        }
    }
    out
}

/// The reactor module whose artifact `entry` is, if any: a jar named `<artifactId>-<version>….jar`
/// sitting either in a `~/.m2`-shaped `<artifactId>/<version>/` directory or in a `target/` directory.
fn sibling_of<'a>(entry: &Path, modules: &'a [ReactorModule]) -> Option<&'a ReactorModule> {
    let file = entry.file_name()?.to_str()?.to_ascii_lowercase();
    if !file.ends_with(".jar") {
        return None;
    }
    let parent = entry.parent()?;
    let parent_name = parent.file_name()?.to_str()?.to_ascii_lowercase();
    let grandparent_name = parent
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    modules.iter().filter(|m| !m.artifact_id.is_empty()).find(|m| {
        let prefix = format!("{}-", m.artifact_id);
        file.starts_with(&prefix)
            && (grandparent_name == m.artifact_id || parent_name == "target")
            // `core-api-1.0.jar` must not be taken for module `core`: what follows the prefix is
            // the version, which starts with a digit (or is a property-less SNAPSHOT build).
            && file[prefix.len()..].chars().next().is_some_and(|c| c.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(rel: &str, artifact: &str, deps: &[&str]) -> ReactorModule {
        ReactorModule {
            rel: rel.into(),
            artifact_id: artifact.into(),
            dependencies: deps.iter().map(|d| d.to_string()).collect(),
        }
    }

    /// root ← model ← core ← web; batch ← core; tools depends on nothing in the reactor.
    fn reactor() -> Vec<ReactorModule> {
        vec![
            module("", "parent", &["model", "core", "web"]),
            module("model", "model", &["lombok"]),
            module("core", "core", &["model", "spring-context"]),
            module("apps/web", "web", &["core", "spring-webmvc"]),
            module("apps/batch", "batch", &["core"]),
            module("tools", "tools", &[]),
        ]
    }

    #[test]
    fn upstream_is_transitive_and_dependencies_come_first() {
        assert_eq!(upstream_of(&reactor(), "apps/web"), vec!["model", "core"]);
        assert_eq!(upstream_of(&reactor(), "core"), vec!["model"]);
        assert!(upstream_of(&reactor(), "tools").is_empty());
    }

    /// A sibling nobody depends on is not upstream — its resources and beans stay out of the JVM.
    #[test]
    fn a_module_that_is_not_a_dependency_is_not_upstream() {
        let up = upstream_of(&reactor(), "apps/web");
        assert!(!up.contains(&"apps/batch".to_string()));
        assert!(!up.contains(&"tools".to_string()));
    }

    #[test]
    fn a_cycle_terminates() {
        let modules = vec![module("a", "a", &["b"]), module("b", "b", &["a"])];
        assert_eq!(upstream_of(&modules, "a"), vec!["b"]);
    }

    #[test]
    fn unknown_target_has_no_upstream() {
        assert!(upstream_of(&reactor(), "nope").is_empty());
    }

    #[test]
    fn ancestors_run_from_the_root_down() {
        assert_eq!(ancestors_of("apps/web"), vec!["".to_string(), "apps".to_string()]);
        assert_eq!(ancestors_of("core"), vec!["".to_string()]);
        assert!(ancestors_of("").is_empty());
    }

    #[test]
    fn normalize_rel_accepts_windows_and_trailing_separators() {
        assert_eq!(normalize_rel(r" apps\web\ "), "apps/web");
        assert_eq!(normalize_rel("/core/"), "core");
    }

    /// The stale-install case: `core` resolved from `~/.m2` (or its packaged jar) is replaced by its
    /// `target/classes`, in place, and third-party jars are untouched.
    #[test]
    fn sibling_jars_become_their_classes_directory() {
        let m2 = PathBuf::from("/home/u/.m2/repository/it/acme/core/1.0-SNAPSHOT/core-1.0-SNAPSHOT.jar");
        let packaged = PathBuf::from("/p/model/target/model-1.0-SNAPSHOT.jar");
        let spring = PathBuf::from("/home/u/.m2/repository/org/springframework/spring-core/6.1.0/spring-core-6.1.0.jar");
        let out = substitute_sibling_artifacts(
            vec![m2, spring.clone(), packaged],
            &reactor(),
            |rel| Some(PathBuf::from(format!("/p/{rel}/target/classes"))),
        );
        assert_eq!(
            out,
            vec![
                PathBuf::from("/p/core/target/classes"),
                spring,
                PathBuf::from("/p/model/target/classes"),
            ]
        );
    }

    /// `core-api-1.0.jar` shares a prefix with module `core` and is not it.
    #[test]
    fn a_prefix_match_is_not_a_sibling() {
        let jar = PathBuf::from("/home/u/.m2/repository/it/acme/core-api/1.0/core-api-1.0.jar");
        let out = substitute_sibling_artifacts(vec![jar.clone()], &reactor(), |rel| {
            Some(PathBuf::from(format!("/p/{rel}/target/classes")))
        });
        assert_eq!(out, vec![jar]);
    }

    /// A sibling never compiled keeps its jar: a stale class beats no class.
    #[test]
    fn a_sibling_without_classes_keeps_its_jar() {
        let jar = PathBuf::from("/home/u/.m2/repository/it/acme/core/1.0/core-1.0.jar");
        let out = substitute_sibling_artifacts(vec![jar.clone()], &reactor(), |_| None);
        assert_eq!(out, vec![jar]);
    }
}
