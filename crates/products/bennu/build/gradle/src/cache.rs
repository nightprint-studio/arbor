//! The Gradle cache, read directly.
//!
//! Gradle keeps every artifact it has downloaded under
//! `$GRADLE_USER_HOME/caches/modules-2/files-2.1/<group>/<artifact>/<version>/<hash>/<file>`. The
//! hash directory is Gradle's, opaque and different per file, so a path *names* its coordinate
//! (group, artifact, version are the first three segments) but cannot be built from one without
//! listing a directory. Both directions are here: [`jars_of`] to find a declared dependency's jar,
//! [`coord_of`] to read a classpath entry back as a coordinate.
//!
//! It is the cheap half of resolving a classpath. It cannot follow transitive dependencies — that
//! takes Gradle's resolution rules — so what it answers is "which of the dependencies this build
//! *declares* are on this machine", and the caller asks Gradle itself when the answer is short.

use std::path::{Path, PathBuf};

use crate::project::GradleProject;

/// `$GRADLE_USER_HOME`, or `~/.gradle`.
///
/// A windowed app inherits very little environment, so the variable being absent is the ordinary
/// case and the home directory the usual answer.
pub fn gradle_user_home() -> Option<PathBuf> {
    match std::env::var_os("GRADLE_USER_HOME") {
        Some(h) if !h.is_empty() => Some(PathBuf::from(h)),
        _ => arbor_core::prelude::user_home().map(|h| h.join(".gradle")),
    }
}

/// The directory that holds the downloaded modules.
pub fn modules_dir() -> Option<PathBuf> {
    let dir = gradle_user_home()?.join("caches").join("modules-2").join("files-2.1");
    dir.is_dir().then_some(dir)
}

/// A jar that is not the sources or the javadoc of another.
fn is_binary_jar(name: &str) -> bool {
    name.ends_with(".jar")
        && !name.ends_with("-sources.jar")
        && !name.ends_with("-javadoc.jar")
        && !name.ends_with("-tests.jar")
}

/// The jars of `group:name:version` that the cache holds. Empty when it holds none — including the
/// ordinary case of an artifact that is only a pom (a BOM).
pub fn jars_of(group: &str, name: &str, version: &str) -> Vec<PathBuf> {
    let Some(base) = modules_dir() else { return Vec::new() };
    jars_in(&base, group, name, version)
}

fn jars_in(base: &Path, group: &str, name: &str, version: &str) -> Vec<PathBuf> {
    let version_dir = base.join(group).join(name).join(version);
    let mut out = Vec::new();
    let Ok(hashes) = std::fs::read_dir(&version_dir) else { return out };
    for hash in hashes.flatten() {
        let Ok(files) = std::fs::read_dir(hash.path()) else { continue };
        for file in files.flatten() {
            let path = file.path();
            let is_jar = path.file_name().and_then(|n| n.to_str()).map(is_binary_jar).unwrap_or(false);
            if is_jar {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The versions of `group:name` the cache holds, oldest name first.
pub fn versions_of(group: &str, name: &str) -> Vec<String> {
    let Some(base) = modules_dir() else { return Vec::new() };
    let Ok(read) = std::fs::read_dir(base.join(group).join(name)) else { return Vec::new() };
    let mut out: Vec<String> = read
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

/// The `(group, name, version)` a path inside the cache encodes, or `None` for a path that is not
/// laid out like one (a project's `build/libs` jar, a file under `~/.m2`).
pub fn coord_of(path: &Path) -> Option<(String, String, String)> {
    let segments: Vec<String> = path.iter().map(|s| s.to_string_lossy().into_owned()).collect();
    let at = segments.iter().position(|s| s == "files-2.1")?;
    let rest = &segments[at + 1..];
    // group / name / version / hash / file
    if rest.len() < 5 {
        return None;
    }
    Some((rest[0].clone(), rest[1].clone(), rest[2].clone()))
}

/// Whether a version string is a fixed one: dynamic selectors (`1.+`, `[1.0,2.0)`, `latest.release`)
/// and unexpanded variables name no single directory.
pub fn is_fixed_version(version: &str) -> bool {
    !version.is_empty()
        && !version.contains(&['+', '[', ']', '(', ')', ',', '$', '{', '}'][..])
        && !version.starts_with("latest")
}

/// What reading the declared dependencies against the cache found.
#[derive(Debug, Clone, Default)]
pub struct Resolution {
    pub jars: Vec<PathBuf>,
    /// `group:name:version` declared with a fixed version and absent from the cache.
    pub missing: Vec<String>,
    /// `group:name` declared with no usable version — a BOM, a dynamic selector or an unexpanded
    /// variable decides it, and a download would not help.
    pub unversioned: Vec<String>,
}

impl Resolution {
    /// Nothing that could be located is absent.
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty()
    }
}

/// Resolve every artifact dependency the build declares against the cache and `~/.m2`.
///
/// Only *declared* ones — see the module doc. `platform` dependencies are BOMs, with no jar to
/// contribute; `project(…)` dependencies are the build's own output.
pub fn resolve_declared(project: &GradleProject) -> Resolution {
    let mut out = Resolution::default();
    let mut seen: Vec<String> = Vec::new();
    for module in &project.modules {
        let own = module.info.dependencies.iter();
        let inherited = project.inherited_dependencies(module).into_iter().map(|(d, _)| d);
        for dep in own.chain(inherited) {
            if !dep.project.is_empty() || dep.kind == "platform" || dep.name.is_empty() {
                continue;
            }
            let key = format!("{}:{}:{}", dep.group, dep.name, dep.version);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key.clone());
            if !is_fixed_version(&dep.version) {
                out.unversioned.push(format!("{}:{}", dep.group, dep.name));
                continue;
            }
            let mut found = jars_of(&dep.group, &dep.name, &dep.version);
            if found.is_empty() {
                found = maven_local_jar(&dep.group, &dep.name, &dep.version).into_iter().collect();
            }
            if found.is_empty() {
                out.missing.push(key);
            } else {
                for jar in found {
                    if !out.jars.contains(&jar) {
                        out.jars.push(jar);
                    }
                }
            }
        }
    }
    out
}

/// The jar under `~/.m2/repository`, for a build that resolves from `mavenLocal()`.
fn maven_local_jar(group: &str, name: &str, version: &str) -> Option<PathBuf> {
    let repo = arbor_core::prelude::user_home()?.join(".m2").join("repository");
    let mut dir = repo;
    for part in group.split('.') {
        dir.push(part);
    }
    let jar = dir.join(name).join(version).join(format!("{name}-{version}.jar"));
    jar.is_file().then_some(jar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cache_path_names_its_coordinate() {
        let p = Path::new("/home/u/.gradle/caches/modules-2/files-2.1/org.springframework/spring-web/5.3.10/abc123/spring-web-5.3.10.jar");
        assert_eq!(
            coord_of(p),
            Some(("org.springframework".to_string(), "spring-web".to_string(), "5.3.10".to_string()))
        );
        assert_eq!(coord_of(Path::new("/p/build/libs/app.jar")), None);
    }

    #[test]
    fn only_fixed_versions_name_a_directory() {
        assert!(is_fixed_version("5.3.10"));
        assert!(!is_fixed_version("1.+"));
        assert!(!is_fixed_version("[1.0,2.0)"));
        assert!(!is_fixed_version("$springVersion"));
        assert!(!is_fixed_version("latest.release"));
        assert!(!is_fixed_version(""));
    }

    #[test]
    fn sources_and_javadoc_jars_are_not_the_binary() {
        assert!(is_binary_jar("a-1.0.jar"));
        assert!(!is_binary_jar("a-1.0-sources.jar"));
        assert!(!is_binary_jar("a-1.0-javadoc.jar"));
        assert!(!is_binary_jar("a-1.0.pom"));
    }

    #[test]
    fn jars_are_found_under_the_hash_directory() {
        let base = std::env::temp_dir().join(format!("bennu-gradle-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("org.x").join("lib").join("1.0").join("deadbeef");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("lib-1.0.jar"), "").unwrap();
        std::fs::write(dir.join("lib-1.0-sources.jar"), "").unwrap();
        let jars = jars_in(&base, "org.x", "lib", "1.0");
        assert_eq!(jars.len(), 1);
        assert!(jars[0].ends_with("lib-1.0.jar"));
    }
}
