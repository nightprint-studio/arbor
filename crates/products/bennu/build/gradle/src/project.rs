//! A Gradle build, assembled: settings → subprojects → each one's build script.
//!
//! The Gradle counterpart of a Maven reactor, and read with the same discipline — files only, no
//! Gradle run. A build is a root plus the projects `settings.gradle` includes; each project is a
//! directory (by default the one its path implies, `:web:client` → `web/client`) that may have a
//! build script of its own. A project with no script is legal and common — `:web` above
//! `:web:client` is just a container — and is kept as a module so the tree has the shape the
//! settings file declares.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::build_file::{BuildInfo, Env, JavaLevel, RawDep};
use crate::catalog::Catalog;
use crate::settings::Settings;

/// The settings and build scripts, in the order Gradle itself prefers them.
const SETTINGS_FILES: [&str; 2] = ["settings.gradle", "settings.gradle.kts"];
const BUILD_FILES: [&str; 2] = ["build.gradle", "build.gradle.kts"];

/// One project of the build.
#[derive(Debug, Clone)]
pub struct GradleModule {
    /// The Gradle path: `:` for the root, `:api`, `:web:client`.
    pub path: String,
    /// The last segment of the path; the build's name for the root.
    pub name: String,
    /// The project directory.
    pub dir: PathBuf,
    /// The directory relative to the build root, forward-slashed; empty for the root.
    pub rel_dir: String,
    /// The build script, when the project has one.
    pub build_file: Option<PathBuf>,
    pub info: BuildInfo,
}

impl GradleModule {
    pub fn is_root(&self) -> bool {
        self.path == ":"
    }

    /// What the project builds, in the words the dependency panel uses for Maven's packaging:
    /// `war`, `platform` (a BOM), `jar`, or empty for a project that applies no plugin.
    pub fn packaging(&self) -> &'static str {
        if self.info.applies("war") {
            "war"
        } else if self.info.applies("java-platform") {
            "platform"
        } else if self.is_jvm() {
            "jar"
        } else {
            ""
        }
    }

    /// Whether the project applies a plugin that compiles Java, Kotlin, Groovy or Scala.
    pub fn is_jvm(&self) -> bool {
        const JVM_PLUGINS: [&str; 12] = [
            "java", "java-library", "application", "war", "groovy", "scala", "org.jetbrains.kotlin",
            "org.springframework.boot", "io.quarkus", "kotlin", "com.android", "io.micronaut",
        ];
        JVM_PLUGINS.iter().any(|id| self.info.applies(id))
    }
}

/// A whole build.
#[derive(Debug, Clone)]
pub struct GradleProject {
    pub root: PathBuf,
    /// `rootProject.name`, else the directory's name.
    pub name: String,
    pub settings_file: Option<PathBuf>,
    pub settings: Settings,
    /// Root first, then the included projects in declaration order.
    pub modules: Vec<GradleModule>,
    pub catalogs: Vec<Catalog>,
    /// `gradle.properties`.
    pub properties: BTreeMap<String, String>,
    /// The build has `gradlew` at its root.
    pub has_wrapper: bool,
}

/// Whether `dir` is the root of a Gradle build: it has a settings script or a build script.
pub fn is_gradle_root(dir: &Path) -> bool {
    first_existing(dir, &SETTINGS_FILES).is_some() || first_existing(dir, &BUILD_FILES).is_some()
}

fn first_existing(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

/// `gradle.properties` as a map. `#` / `!` comment lines are skipped; `key=value`, `key:value` and
/// `key value` all separate.
pub fn read_properties(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(root.join("gradle.properties")) else { return out };
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with('!') {
            continue;
        }
        if let Some(at) = l.find(|c: char| c == '=' || c == ':' || c.is_whitespace()) {
            let key = l[..at].trim();
            let value = l[at + 1..].trim();
            if !key.is_empty() {
                out.insert(key.to_string(), value.to_string());
            }
        }
    }
    out
}

fn read_text(path: &Path) -> String {
    std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

fn forward(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Read the build rooted at `root`. Never fails: a folder with no scripts yields a project with
/// just a root module, which is what an editor opened on the wrong directory should show.
pub fn read(root: &Path) -> GradleProject {
    let settings_file = first_existing(root, &SETTINGS_FILES);
    let settings = settings_file.as_deref().map(|p| Settings::parse(&read_text(p))).unwrap_or_default();
    let dir_name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = if settings.root_name.is_empty() { dir_name } else { settings.root_name.clone() };
    let properties = read_properties(root);
    let catalogs = read_catalogs(root, &settings);

    let mut modules = Vec::new();
    let empty = BTreeMap::new();

    let root_file = first_existing(root, &BUILD_FILES);
    let root_info = parse_script(root_file.as_deref(), &properties, &empty, &catalogs);
    let inherited = root_info.variables.clone();
    modules.push(GradleModule {
        path: ":".to_string(),
        name: name.clone(),
        dir: root.to_path_buf(),
        rel_dir: String::new(),
        build_file: root_file,
        info: root_info,
    });

    for path in &settings.includes {
        let rel = settings
            .project_dirs
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.trim_start_matches(':').replace(':', "/"));
        let dir = root.join(&rel);
        let build_file = first_existing(&dir, &BUILD_FILES);
        let info = parse_script(build_file.as_deref(), &properties, &inherited, &catalogs);
        modules.push(GradleModule {
            path: path.clone(),
            name: path.rsplit(':').next().unwrap_or(path).to_string(),
            rel_dir: forward(Path::new(&rel)).trim_start_matches("./").to_string(),
            dir,
            build_file,
            info,
        });
    }

    GradleProject {
        has_wrapper: root.join("gradlew").is_file() || root.join("gradlew.bat").is_file(),
        root: root.to_path_buf(),
        name,
        settings_file,
        settings,
        modules,
        catalogs,
        properties,
    }
}

fn parse_script(
    file: Option<&Path>,
    properties: &BTreeMap<String, String>,
    inherited: &BTreeMap<String, String>,
    catalogs: &[Catalog],
) -> BuildInfo {
    match file {
        Some(p) => BuildInfo::parse(&read_text(p), &Env { properties, inherited, catalogs }),
        None => BuildInfo::default(),
    }
}

/// The default `libs` catalog (when `gradle/libs.versions.toml` exists) and every catalog the
/// settings declare.
fn read_catalogs(root: &Path, settings: &Settings) -> Vec<Catalog> {
    let mut out: Vec<Catalog> = Vec::new();
    for (name, rel) in &settings.catalogs {
        let path = root.join(rel);
        if path.is_file() {
            out.push(Catalog::parse(name, &forward(&path), &read_text(&path)));
        }
    }
    let default = root.join("gradle").join("libs.versions.toml");
    if default.is_file() && !out.iter().any(|c| c.name == "libs") {
        out.push(Catalog::parse("libs", &forward(&default), &read_text(&default)));
    }
    out
}

/// `a.b` ≥ `c.d` as Java levels: `1.8` is 8.
fn level_number(version: &str) -> u32 {
    let v = version.trim().trim_start_matches("1.");
    v.split('.').next().and_then(|n| n.parse().ok()).unwrap_or(0)
}

impl GradleProject {
    /// The module at a Gradle path.
    pub fn module(&self, path: &str) -> Option<&GradleModule> {
        self.modules.iter().find(|m| m.path == path)
    }

    /// The module a `project(':x')` dependency or typesafe accessor names. The match ignores case
    /// and `-` / `_` so `projects.myLib` (read as `:my-lib`) finds `:myLib` and `:my_lib` too.
    pub fn find_project(&self, path: &str) -> Option<&GradleModule> {
        if let Some(m) = self.module(path) {
            return Some(m);
        }
        let key = |s: &str| s.chars().filter(|c| *c != '-' && *c != '_').collect::<String>().to_lowercase();
        let wanted = key(path);
        self.modules.iter().find(|m| key(&m.path) == wanted)
    }

    /// The highest Java level any project declares — the same choice the Maven reader makes, and
    /// for the same reason: a level too low invents errors in the module that uses newer syntax,
    /// one too high can only stay silent about an older module.
    pub fn java_level(&self) -> Option<JavaLevel> {
        self.modules
            .iter()
            .filter_map(|m| m.info.java.as_ref())
            .max_by_key(|j| level_number(&j.version))
            .cloned()
    }

    /// The source encoding the build declares (`options.encoding`), root first.
    pub fn encoding(&self) -> Option<&str> {
        self.modules.iter().map(|m| m.info.encoding.as_str()).find(|e| !e.is_empty())
    }

    /// Dependencies a module gets from an ancestor's `allprojects {}` / `subprojects {}` block,
    /// each with the project that declares it.
    pub fn inherited_dependencies(&self, module: &GradleModule) -> Vec<(&RawDep, &GradleModule)> {
        let mut out = Vec::new();
        for other in &self.modules {
            let ancestor = other.is_root() || module.path.starts_with(&format!("{}:", other.path));
            let same = other.path == module.path;
            if !(ancestor || same) {
                continue;
            }
            out.extend(other.info.all_projects.iter().map(|d| (d, other)));
            if ancestor && !same {
                out.extend(other.info.sub_projects.iter().map(|d| (d, other)));
            }
        }
        out
    }

    /// Whether any project of the build is a JVM project.
    pub fn is_jvm(&self) -> bool {
        self.modules.iter().any(GradleModule::is_jvm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bennu-gradle-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (f, text) in files {
            let full = dir.join(f);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        dir
    }

    #[test]
    fn the_settings_decide_the_modules_and_their_directories() {
        let dir = build(
            "modules",
            &[
                ("settings.gradle", "rootProject.name = 'shop'\ninclude ':api', ':web:client'\n"),
                ("build.gradle", "plugins { id 'java' }\n"),
                ("api/build.gradle", "plugins { id 'java-library' }\ndependencies { implementation project(':web:client') }\n"),
                ("web/client/build.gradle.kts", "plugins { `java-library` }\n"),
            ],
        );
        let p = read(&dir);
        assert_eq!(p.name, "shop");
        let paths: Vec<&str> = p.modules.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(paths, [":", ":api", ":web", ":web:client"]);
        assert_eq!(p.module(":web:client").unwrap().rel_dir, "web/client");
        // `:web` is only a container.
        assert!(p.module(":web").unwrap().build_file.is_none());
        assert_eq!(p.module(":api").unwrap().packaging(), "jar");
        assert_eq!(p.module(":web").unwrap().packaging(), "");
    }

    #[test]
    fn a_build_with_no_settings_is_a_single_project() {
        let dir = build("single", &[("build.gradle.kts", "plugins { java }\n")]);
        let p = read(&dir);
        assert_eq!(p.modules.len(), 1);
        assert!(p.modules[0].is_root() && p.is_jvm());
    }

    #[test]
    fn root_ext_variables_reach_the_subprojects() {
        let dir = build(
            "vars",
            &[
                ("settings.gradle", "include 'a'\n"),
                ("build.gradle", "ext {\n  guavaVersion = '32.0'\n}\n"),
                ("a/build.gradle", "dependencies { implementation \"com.google.guava:guava:$guavaVersion\" }\n"),
            ],
        );
        let p = read(&dir);
        assert_eq!(p.module(":a").unwrap().info.dependencies[0].version, "32.0");
    }

    #[test]
    fn subprojects_blocks_reach_children_but_not_the_root() {
        let dir = build(
            "inherit",
            &[
                ("settings.gradle", "include 'a'\n"),
                ("build.gradle", "subprojects { dependencies { testImplementation 'junit:junit:4.13' } }\n"),
            ],
        );
        let p = read(&dir);
        assert_eq!(p.inherited_dependencies(p.module(":a").unwrap()).len(), 1);
        assert!(p.inherited_dependencies(p.module(":").unwrap()).is_empty());
    }

    #[test]
    fn the_highest_declared_java_level_wins() {
        let dir = build(
            "java",
            &[
                ("settings.gradle", "include 'old', 'new'\n"),
                ("old/build.gradle", "sourceCompatibility = '1.8'\n"),
                ("new/build.gradle", "sourceCompatibility = '17'\n"),
            ],
        );
        assert_eq!(read(&dir).java_level().unwrap().version, "17");
    }

    #[test]
    fn properties_read_all_three_separators() {
        let dir = build("props", &[("gradle.properties", "# c\na=1\nb: 2\nc 3\n")]);
        let props = read_properties(&dir);
        assert_eq!((props["a"].as_str(), props["b"].as_str(), props["c"].as_str()), ("1", "2", "3"));
    }
}
