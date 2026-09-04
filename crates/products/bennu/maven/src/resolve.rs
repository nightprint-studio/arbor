//! The project's classpath, resolved from the poms and the local repository — no Maven, no network.
//!
//! ## Why this exists next to `mvn dependency:build-classpath`
//!
//! Running Maven is the ground truth and stays the first answer where it works. But it fails in
//! ways that are ordinary rather than exotic, and each of them used to cost the whole dependency
//! tier — which means every library type in the project reading as *cannot resolve*, thousands of
//! errors on a tree that compiles:
//!
//! - **Maven is not installed**, or is installed somewhere a desktop app launched from the Dock
//!   cannot see;
//! - the reactor **does not build** — a broken plugin configuration three modules away has nothing
//!   to do with the classpath, and yet `build-classpath` never gets to write one;
//! - a **handful of artifacts are missing** and the offline goal reports failure for the whole run;
//! - it is **slow**: a JVM start plus a reactor walk is seconds, every time a pom is touched.
//!
//! Reading the poms and looking in `~/.m2` costs milliseconds and answers in all four cases. What it
//! cannot do is *download* anything — so a coordinate that was never fetched stays missing, and
//! [`Resolution::missing`] names it. That list is the thing the user actually needs: not "0 jars
//! resolved", but `com.acme:legacy-core:2.4.0 is not in your local repository`.
//!
//! ## What it implements of Maven's resolution
//!
//! Nearest-wins conflict resolution, `<dependencyManagement>` (including imported BOMs), the parent
//! chain on disk and in the repository, `<exclusions>` down a subtree, optional dependencies stopping
//! at the artifact that declares them, and Maven's scope table. What it deliberately does not do is
//! activate profiles or pick a version out of a range — both are decisions about a *build*, and
//! inventing one here would produce a classpath no build ever has.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use bennu_deps::prelude::{parse_pom, Pom};

use crate::effective::{Effective, PomReader, Resolved};
use crate::repo::{Coord, LocalRepo};

/// How deep the transitive walk goes. Real graphs bottom out around ten; the cap is a guard against
/// a repository holding a pom that (directly or through a chain) depends on itself.
const MAX_DEPTH: usize = 24;

/// What the offline resolve produced.
#[derive(Debug, Clone, Default)]
pub struct Resolution {
    /// Artifact files that exist, in the order the walk found them: a module's own declarations
    /// first, then what they drag in.
    pub jars: Vec<PathBuf>,
    /// Coordinates the graph needs and the repository does not have. **The** answer to "why is
    /// nothing resolving".
    pub missing: Vec<Coord>,
    /// Declared dependencies whose version nothing on disk answers — an undefined `${property}`, a
    /// BOM that is itself missing, a version range. Distinct from [`Self::missing`] because the fix
    /// is different: no download will help.
    pub unversioned: Vec<Coord>,
    /// Reactor modules, by `groupId:artifactId` — resolved from source, never looked for in the
    /// repository.
    pub reactor: Vec<String>,
}

impl Resolution {
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.unversioned.is_empty()
    }

    /// The jar paths as the strings every cache and wire type uses.
    pub fn jar_strings(&self) -> Vec<String> {
        self.jars.iter().map(|p| p.display().to_string()).collect()
    }

    /// Where each missing artifact was looked for.
    ///
    /// The freshness key of the classpath cache: a pom does not change when the jar it names
    /// finally lands in `~/.m2`, so an mtime alone pinned a project to a half-resolved classpath
    /// until somebody edited a pom. These paths answer "has it arrived yet" with one `stat` each.
    pub fn missing_paths(&self, repo: &LocalRepo) -> Vec<String> {
        self.missing
            .iter()
            .map(|c| {
                let file = if c.is_pom() { repo.pom_file(c) } else { repo.artifact_file(c) };
                file.display().to_string()
            })
            .collect()
    }

    /// One line for the user, when something is missing. `None` when everything resolved.
    pub fn shortfall(&self) -> Option<String> {
        if self.is_complete() {
            return None;
        }
        let mut parts = Vec::new();
        if !self.missing.is_empty() {
            parts.push(format!(
                "{} not in the local repository ({})",
                self.missing.len(),
                sample(self.missing.iter().map(|c| c.gav()))
            ));
        }
        if !self.unversioned.is_empty() {
            parts.push(format!(
                "{} with no resolvable version ({})",
                self.unversioned.len(),
                sample(self.unversioned.iter().map(|c| c.gav()))
            ));
        }
        Some(parts.join("; "))
    }
}

/// The first few of a list, then a count — a message, not a dump.
fn sample(items: impl Iterator<Item = String>) -> String {
    const SHOW: usize = 3;
    let all: Vec<String> = items.collect();
    let head = all.iter().take(SHOW).cloned().collect::<Vec<_>>().join(", ");
    if all.len() > SHOW {
        format!("{head}, +{} more", all.len() - SHOW)
    } else {
        head
    }
}

/// Resolve the whole reactor rooted at `root` against `repo`.
pub fn resolve(root: &Path, repo: &LocalRepo) -> Resolution {
    let modules = reactor(root);
    let mut reader = PomReader::new(repo);
    let effectives: Vec<Effective> = modules
        .iter()
        .map(|(dir, pom)| reader.effective_of_file(pom, dir))
        .collect();

    // The reactor's own artifacts are resolved from source. Looking for them in the repository is
    // how a multi-module project reports half of itself as a missing dependency — and installing
    // them would not make it right either, because the jar in `~/.m2` is last week's build.
    let reactor: HashSet<String> = effectives.iter().map(|e| e.coord.ga()).collect();

    let mut out = Resolution { reactor: reactor.iter().cloned().collect(), ..Resolution::default() };
    out.reactor.sort();

    let mut chosen: HashMap<String, usize> = HashMap::new();
    let mut seen_missing: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<Node> = VecDeque::new();

    for eff in &effectives {
        for dep in &eff.dependencies {
            if reactor.contains(&dep.coord.ga()) {
                continue;
            }
            // A `system`-scoped dependency is a jar at an explicit `<systemPath>` — outside the
            // repository by definition. Looking for it there and reporting it missing would be a
            // warning about the one dependency that was never supposed to be downloaded.
            if dep.scope == "system" {
                continue;
            }
            if !usable_version(dep) {
                if dep.profile.is_empty() {
                    push_once(&mut out.unversioned, &mut seen_missing, dep.coord.clone(), "v");
                }
                continue;
            }
            queue.push_back(Node {
                coord: dep.coord.clone(),
                scope: dep.scope.clone(),
                depth: 0,
                excluded: dep.exclusions.iter().cloned().collect(),
                from_profile: !dep.profile.is_empty(),
            });
        }
    }

    while let Some(node) = queue.pop_front() {
        let key = node.coord.key();
        // Nearest wins: the first time a key is reached is by the shortest path, because the queue
        // is walked breadth-first. A later, deeper sighting of the same artifact is Maven's
        // "omitted for conflict" and contributes nothing.
        if let Some(&at) = chosen.get(&key) {
            if at <= node.depth {
                continue;
            }
        }
        chosen.insert(key, node.depth);

        match repo.resolve(&node.coord) {
            Some(file) => {
                // A BOM resolves to a `.pom`, which is not a classpath entry — it is management,
                // and putting it on the classpath would hand the compiler a file it cannot read.
                if !node.coord.is_pom() && !out.jars.contains(&file) {
                    out.jars.push(file);
                }
            }
            None => {
                // A profile's dependency is only fetched by a build that runs that profile, so its
                // absence is a fact about this machine rather than a broken project. Its jar is used
                // when it happens to be there, and its absence is not reported — otherwise a legacy
                // pom with a `was` and a `weblogic` profile reports a dozen missing artifacts on a
                // tree that builds perfectly.
                if !node.from_profile {
                    push_once(&mut out.missing, &mut seen_missing, node.coord.clone(), "m");
                }
                // Its pom is missing too, so there is nothing under it to walk. Recording the
                // parent is the useful answer; inventing its children is not.
                continue;
            }
        }

        if node.depth >= MAX_DEPTH {
            continue;
        }
        let Some(eff) = reader.effective(&node.coord) else { continue };
        let children: Vec<Resolved> = eff.dependencies.clone();
        for child in children {
            if !transitively_relevant(&node.scope, &child) {
                continue;
            }
            if node.excluded.contains(&child.coord.ga()) || excluded_by_wildcard(&node.excluded, &child.coord) {
                continue;
            }
            if reactor.contains(&child.coord.ga()) {
                continue;
            }
            if !usable_version(&child) {
                // A transitive with no resolvable version is not the user's pom's fault and there
                // is nothing to act on, so it is not reported — only the declared ones are.
                continue;
            }
            let mut excluded = node.excluded.clone();
            excluded.extend(child.exclusions.iter().cloned());
            queue.push_back(Node {
                coord: child.coord.clone(),
                scope: effective_scope(&node.scope, &child.scope),
                depth: node.depth + 1,
                excluded,
                from_profile: node.from_profile,
            });
        }
    }

    out.missing.sort();
    out.missing.dedup();
    out.unversioned.sort();
    out.unversioned.dedup();
    out
}

/// One artifact on the walk, with the context that decides what it drags in.
struct Node {
    coord: Coord,
    scope: String,
    depth: usize,
    /// `groupId:artifactId` excluded anywhere along the path that reached this node.
    excluded: HashSet<String>,
    /// Whether the declaration that started this branch lives under a `<profile>` — see the
    /// reporting rule where an artifact fails to resolve.
    from_profile: bool,
}

fn push_once(out: &mut Vec<Coord>, seen: &mut HashSet<String>, coord: Coord, tag: &str) {
    if seen.insert(format!("{tag}{}", coord.gav())) {
        out.push(coord);
    }
}

/// Whether a version is one this can look for. A range or a surviving `${…}` is not — see the
/// module docs on why neither is guessed at.
fn usable_version(dep: &Resolved) -> bool {
    !dep.coord.version.is_empty() && !dep.is_range() && !dep.has_unresolved_property()
}

/// Whether a dependency of a dependency reaches the classpath at all.
///
/// Maven's rule, and the two halves both matter: `optional` stops at the artifact that declares it
/// (that is what optional *means*), and `test` / `provided` are not inherited — a library's test
/// dependencies are not yours, and a `provided` one is the container's job.
fn transitively_relevant(parent_scope: &str, child: &Resolved) -> bool {
    if child.optional {
        return false;
    }
    match child.scope.as_str() {
        "test" | "provided" | "system" | "import" => false,
        _ => !parent_scope.is_empty(),
    }
}

/// Maven's scope table: what a `child` scope becomes when reached through a `parent` one.
fn effective_scope(parent: &str, child: &str) -> String {
    match (parent, child) {
        ("compile", "compile") => "compile",
        ("compile", "runtime") => "runtime",
        ("provided", _) => "provided",
        ("test", _) => "test",
        ("runtime", _) => "runtime",
        (_, c) => c,
    }
    .to_string()
}

/// Maven's wildcard exclusion — `<groupId>*</groupId><artifactId>*</artifactId>` means "nothing
/// under this dependency", and it is how a project cuts an entire transitive subtree.
fn excluded_by_wildcard(excluded: &HashSet<String>, coord: &Coord) -> bool {
    excluded.iter().any(|e| {
        let Some((g, a)) = e.split_once(':') else { return false };
        (g == "*" || g == coord.group_id) && (a == "*" || a == coord.artifact_id)
    })
}

/// Every pom of the reactor rooted at `root`: the root's own, then each `<modules>` entry,
/// recursively.
///
/// Follows the declaration rather than walking the tree, which is the difference between reading a
/// project and reading whatever happens to be checked out beside it — a `samples/` directory or a
/// vendored dependency with its own pom is not part of the reactor and its dependencies are not the
/// project's.
pub fn reactor(root: &Path) -> Vec<(PathBuf, Pom)> {
    let mut out = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    collect_modules(root, &mut out, &mut seen, 0);
    out
}

fn collect_modules(dir: &Path, out: &mut Vec<(PathBuf, Pom)>, seen: &mut HashSet<PathBuf>, depth: usize) {
    /// Deeper than any reactor anybody maintains, and a hard stop on a `<module>..</module>` loop.
    const MAX_REACTOR_DEPTH: usize = 12;
    if depth > MAX_REACTOR_DEPTH || !seen.insert(dir.to_path_buf()) {
        return;
    }
    let Ok(bytes) = std::fs::read(dir.join("pom.xml")) else { return };
    let pom = parse_pom(&String::from_utf8_lossy(&bytes));
    let modules = pom.modules.clone();
    out.push((dir.to_path_buf(), pom));
    for module in modules {
        // A `<module>` names a directory, but naming the pom inside it (`sub/pom.xml`) is legal and
        // some generators write it that way.
        let mut child = dir.join(module.trim().trim_end_matches('/'));
        if child.is_file() {
            let up = child.parent().map(Path::to_path_buf);
            if let Some(up) = up {
                child = up;
            }
        }
        collect_modules(&child, out, seen, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("bennu-mvn-res-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        fn repo_root(&self) -> PathBuf {
            self.dir.join("m2")
        }

        fn project(&self) -> PathBuf {
            self.dir.join("proj")
        }

        /// Install an artifact (pom + jar) in the fake repository.
        fn install(&self, group: &str, artifact: &str, version: &str, pom: &str) {
            let d = self.repo_root().join(group.replace('.', "/")).join(artifact).join(version);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{artifact}-{version}.pom")), pom).unwrap();
            std::fs::write(d.join(format!("{artifact}-{version}.jar")), b"x").unwrap();
        }

        fn write_pom(&self, relative: &str, xml: &str) {
            let path = self.project().join(relative).join("pom.xml");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, xml).unwrap();
        }

        fn resolve(&self) -> Resolution {
            resolve(&self.project(), &LocalRepo::at(self.repo_root()))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn lib(group: &str, artifact: &str, version: &str, deps: &str) -> String {
        format!(
            "<project><groupId>{group}</groupId><artifactId>{artifact}</artifactId>\
             <version>{version}</version><dependencies>{deps}</dependencies></project>"
        )
    }

    fn dep(group: &str, artifact: &str, version: &str) -> String {
        format!("<dependency><groupId>{group}</groupId><artifactId>{artifact}</artifactId><version>{version}</version></dependency>")
    }

    /// The whole point: a classpath, transitives included, with no Maven anywhere.
    #[test]
    fn a_declared_dependency_brings_its_own_dependencies() {
        let f = Fixture::new("transitive");
        f.install("org.slf4j", "slf4j-api", "1.7.36", &lib("org.slf4j", "slf4j-api", "1.7.36", ""));
        f.install(
            "com.acme",
            "core",
            "1.0",
            &lib("com.acme", "core", "1.0", &dep("org.slf4j", "slf4j-api", "1.7.36")),
        );
        f.write_pom(
            "",
            &format!(
                "<project><groupId>p</groupId><artifactId>app</artifactId><version>1</version>\
                 <dependencies>{}</dependencies></project>",
                dep("com.acme", "core", "1.0")
            ),
        );
        let r = f.resolve();
        assert!(r.is_complete(), "{:?}", r);
        assert_eq!(r.jars.len(), 2);
        assert!(r.jars[0].ends_with("core-1.0.jar"), "declared before transitive");
        assert!(r.jars[1].ends_with("slf4j-api-1.7.36.jar"));
    }

    /// The answer that used to be "0 jars resolved": which coordinate, by name.
    #[test]
    fn a_dependency_that_was_never_downloaded_is_named() {
        let f = Fixture::new("missing");
        f.write_pom(
            "",
            &format!(
                "<project><groupId>p</groupId><artifactId>app</artifactId><version>1</version>\
                 <dependencies>{}</dependencies></project>",
                dep("com.acme", "legacy-core", "2.4.0")
            ),
        );
        let r = f.resolve();
        assert_eq!(r.missing.len(), 1);
        assert_eq!(r.missing[0].gav(), "com.acme:legacy-core:2.4.0");
        assert!(r.shortfall().unwrap().contains("com.acme:legacy-core:2.4.0"));
    }

    /// A reactor module is built from source. Reporting it as a missing dependency is how a
    /// multi-module project used to describe half of itself.
    #[test]
    fn a_sibling_module_is_never_looked_for_in_the_repository() {
        let f = Fixture::new("reactor");
        f.write_pom(
            "",
            "<project><groupId>p</groupId><artifactId>root</artifactId><version>1</version>\
             <packaging>pom</packaging><modules><module>core</module><module>web</module></modules></project>",
        );
        f.write_pom("core", "<project><parent><groupId>p</groupId><artifactId>root</artifactId><version>1</version></parent><artifactId>core</artifactId></project>");
        f.write_pom(
            "web",
            &format!(
                "<project><parent><groupId>p</groupId><artifactId>root</artifactId><version>1</version></parent>\
                 <artifactId>web</artifactId><dependencies>{}</dependencies></project>",
                dep("p", "core", "1")
            ),
        );
        let r = f.resolve();
        assert!(r.missing.is_empty(), "{:?}", r.missing);
        assert_eq!(r.reactor, ["p:core", "p:root", "p:web"]);
    }

    /// An exclusion is the difference between the classpath the build produces and the one a naive
    /// walk would — the excluded jar must not come back.
    #[test]
    fn an_exclusion_holds_down_the_whole_subtree() {
        let f = Fixture::new("exclusion");
        f.install("commons-logging", "commons-logging", "1.2", &lib("commons-logging", "commons-logging", "1.2", ""));
        f.install(
            "com.acme",
            "core",
            "1.0",
            &lib("com.acme", "core", "1.0", &dep("commons-logging", "commons-logging", "1.2")),
        );
        f.write_pom(
            "",
            "<project><groupId>p</groupId><artifactId>app</artifactId><version>1</version><dependencies>
               <dependency><groupId>com.acme</groupId><artifactId>core</artifactId><version>1.0</version>
                 <exclusions><exclusion><groupId>commons-logging</groupId><artifactId>commons-logging</artifactId></exclusion></exclusions>
               </dependency>
             </dependencies></project>",
        );
        let r = f.resolve();
        assert_eq!(r.jars.len(), 1, "{:?}", r.jars);
        assert!(r.jars[0].ends_with("core-1.0.jar"));
    }

    /// A library's own test dependencies are not yours. Dragging them in is how an offline resolve
    /// reports missing artifacts nobody ever needed.
    #[test]
    fn a_transitive_test_dependency_is_not_inherited() {
        let f = Fixture::new("scopes");
        f.install(
            "com.acme",
            "core",
            "1.0",
            "<project><groupId>com.acme</groupId><artifactId>core</artifactId><version>1.0</version>
             <dependencies>
               <dependency><groupId>junit</groupId><artifactId>junit</artifactId><version>4.13.2</version><scope>test</scope></dependency>
               <dependency><groupId>com.acme</groupId><artifactId>optional-bits</artifactId><version>1.0</version><optional>true</optional></dependency>
             </dependencies></project>",
        );
        f.write_pom(
            "",
            &format!(
                "<project><groupId>p</groupId><artifactId>app</artifactId><version>1</version>\
                 <dependencies>{}</dependencies></project>",
                dep("com.acme", "core", "1.0")
            ),
        );
        let r = f.resolve();
        assert!(r.missing.is_empty(), "neither junit nor the optional is ours: {:?}", r.missing);
        assert_eq!(r.jars.len(), 1);
    }

    /// Nearest-wins: the version the project declares beats the one a dependency asks for.
    #[test]
    fn the_nearest_declaration_decides_the_version() {
        let f = Fixture::new("nearest");
        f.install("org.slf4j", "slf4j-api", "1.7.36", &lib("org.slf4j", "slf4j-api", "1.7.36", ""));
        f.install("org.slf4j", "slf4j-api", "2.0.9", &lib("org.slf4j", "slf4j-api", "2.0.9", ""));
        f.install(
            "com.acme",
            "core",
            "1.0",
            &lib("com.acme", "core", "1.0", &dep("org.slf4j", "slf4j-api", "1.7.36")),
        );
        f.write_pom(
            "",
            &format!(
                "<project><groupId>p</groupId><artifactId>app</artifactId><version>1</version>\
                 <dependencies>{}{}</dependencies></project>",
                dep("com.acme", "core", "1.0"),
                dep("org.slf4j", "slf4j-api", "2.0.9")
            ),
        );
        let r = f.resolve();
        assert!(r.jars.iter().any(|j| j.ends_with("slf4j-api-2.0.9.jar")));
        assert!(!r.jars.iter().any(|j| j.ends_with("slf4j-api-1.7.36.jar")));
    }

    /// A `${property}` nothing defines cannot be looked for — and saying so is different from
    /// saying the artifact is not downloaded.
    #[test]
    fn an_undefined_property_is_reported_apart_from_a_missing_download() {
        let f = Fixture::new("unversioned");
        f.write_pom(
            "",
            "<project><groupId>p</groupId><artifactId>app</artifactId><version>1</version><dependencies>
               <dependency><groupId>com.acme</groupId><artifactId>core</artifactId><version>${core.version}</version></dependency>
             </dependencies></project>",
        );
        let r = f.resolve();
        assert!(r.missing.is_empty());
        assert_eq!(r.unversioned.len(), 1);
        assert!(r.shortfall().unwrap().contains("no resolvable version"));
    }
}
