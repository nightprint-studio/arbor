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
//!
//! Profiles are read all the same, and the rule is where they were written. In the **project's own**
//! poms they are half the reason a legacy tree resolves at all, so their jars are used when the
//! repository happens to hold them — and their absence is never reported, because whether one is on
//! is a fact about a build. In a **library's** pom the same absence is not even a maybe: no
//! downstream build can switch that profile on, so Maven never fetches what it names and neither
//! does anything here.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

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
    /// How the graph reached each unresolved coordinate, keyed by [`Coord::gav`]. Covers
    /// [`Self::missing`] and [`Self::unversioned`].
    ///
    /// A reactor of a dozen modules reports one coordinate and the person reading it has to guess
    /// which module wants it — and for a transitive, which of its own dependencies dragged it in.
    /// Both answers are known at the moment the walk gives up on the artifact; nothing but this
    /// carried them out.
    pub origins: HashMap<String, Origin>,
}

/// Where an unresolved coordinate came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    /// The reactor module whose dependency tree reached it — its directory relative to the
    /// project root, or its `artifactId` for the root module itself.
    pub module: String,
    /// The artifact that declares it, as `groupId:artifactId:version`. Empty when the module
    /// declares it directly, which is the case where there is nothing in between to name.
    pub via: String,
}

impl Origin {
    /// The parenthetical the user reads after the coordinate: which module, and through what.
    pub fn describe(&self) -> String {
        match (self.module.as_str(), self.via.as_str()) {
            ("", "") => String::new(),
            (module, "") => format!("in {module}"),
            ("", via) => format!("via {via}"),
            (module, via) => format!("in {module}, via {via}"),
        }
    }
}

impl Resolution {
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.unversioned.is_empty()
    }

    /// Where the graph reached `coord` from, when it is one of the unresolved ones.
    pub fn origin_of(&self, coord: &Coord) -> Option<&Origin> {
        self.origins.get(&coord.gav())
    }

    /// A coordinate as the user should read it: the `gav`, plus where it came from when that is
    /// known. The one string every message about a missing artifact should be built from.
    pub fn describe(&self, coord: &Coord) -> String {
        match self.origin_of(coord).map(Origin::describe).filter(|d| !d.is_empty()) {
            Some(origin) => format!("{} ({origin})", coord.gav()),
            None => coord.gav(),
        }
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
            // Not parenthesised: each entry carries its own `(in module, via …)`, and a list in
            // brackets whose items are themselves bracketed is a sentence nobody can parse.
            parts.push(format!(
                "{} not in the local repository — {}",
                self.missing.len(),
                sample(self.missing.iter().map(|c| self.describe(c)))
            ));
        }
        if !self.unversioned.is_empty() {
            parts.push(format!(
                "{} with no resolvable version — {}",
                self.unversioned.len(),
                sample(self.unversioned.iter().map(|c| self.describe(c)))
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
    let pinned = project_management(&effectives);

    let mut out = Resolution { reactor: reactor.iter().cloned().collect(), ..Resolution::default() };
    out.reactor.sort();

    let mut chosen: HashMap<String, usize> = HashMap::new();
    let mut seen_missing: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<Node> = VecDeque::new();

    for (eff, (dir, _)) in effectives.iter().zip(modules.iter()) {
        let module: Rc<str> = Rc::from(module_label(root, dir, eff).as_str());
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
                    record_origin(&mut out, &dep.coord, &module, None);
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
                module: Rc::clone(&module),
                via: None,
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
                // No jar — but that is not the same as "not downloaded". The artifact's own pom
                // may be sitting right there saying it never had one.
                match no_jar_reason(&mut reader, &node.coord) {
                    // Renamed. The artifact keeps publishing at its old coordinates as a jarless
                    // pom whose only content is where to go instead, and Maven follows it. Anything
                    // that does not reports as missing a jar that is on disk under its new name —
                    // measured on `org.hibernate.orm:hibernate-jpamodelgen`, which ORM 7 renamed to
                    // `hibernate-processor`.
                    Some(NoJar::MovedTo(coord)) => {
                        queue.push_back(Node { coord, ..node });
                        continue;
                    }
                    // `<packaging>pom</packaging>` on the artifact ITSELF, whatever the dependency
                    // that named it wrote as its `<type>`. A pom has no jar and never will, so
                    // looking for one and reporting its absence is a false positive by
                    // construction.
                    Some(NoJar::ByPackaging) => continue,
                    None => {}
                }
                // A profile's dependency is only fetched by a build that runs that profile, so its
                // absence is a fact about this machine rather than a broken project. Its jar is used
                // when it happens to be there, and its absence is not reported — otherwise a legacy
                // pom with a `was` and a `weblogic` profile reports a dozen missing artifacts on a
                // tree that builds perfectly.
                if !node.from_profile {
                    record_origin(&mut out, &node.coord, &node.module, node.via.as_deref());
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
        // Built once and shared by every child rather than formatted per edge — a real graph is
        // thousands of edges and this is only ever read for the handful that fail to resolve.
        let via: Rc<str> = Rc::from(node.coord.gav().as_str());
        for mut child in children {
            if !transitively_relevant(&node.scope, &child) {
                continue;
            }
            // The project's own `<dependencyManagement>` decides, whatever this pom wrote. See
            // [`project_management`] — this single line is the difference between the classpath the
            // build has and one nobody has ever compiled against.
            if let Some(version) = pinned.get(&child.coord.key()) {
                child.coord.version = version.clone();
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
                // A `<profile>` in a *library's* pom is the one case where "might not be used" is
                // certain: a downstream build has no way to switch it on, and Maven never fetches
                // it. Jersey's parent declares MOXy under `<profile id="moxy">`, every jersey
                // artifact inherits that pom, and reading the flag off the branch's root instead of
                // off the declaration reported `org.eclipse.persistence.moxy:5.0.0-B09` as a
                // missing dependency of a project that has never heard of it — a coordinate no
                // amount of downloading would ever make right, since nothing wants it.
                from_profile: node.from_profile || !child.profile.is_empty(),
                module: Rc::clone(&node.module),
                via: Some(Rc::clone(&via)),
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
    /// Whether anything on the path that reached this node was declared under a `<profile>` — see
    /// the reporting rule where an artifact fails to resolve.
    from_profile: bool,
    /// The reactor module this branch started from. See [`Origin::module`].
    module: Rc<str>,
    /// The artifact that declares this one, `None` for a module's own declaration.
    via: Option<Rc<str>>,
}

/// Why an artifact with no jar on disk is not, in fact, missing.
enum NoJar {
    /// It was renamed; this is where it went. Every part the relocation leaves out means "the same
    /// as before", which is how Maven reads an artifact that only changed its groupId.
    MovedTo(Coord),
    /// Its own pom says `<packaging>pom</packaging>` — a BOM, a parent, an aggregator. There is no
    /// jar to look for.
    ByPackaging,
}

/// Read the artifact's OWN pom to tell "never had a jar" from "not downloaded".
///
/// `None` when the pom is not there either, which is the genuine miss: nothing on disk claims to
/// know anything about this coordinate.
fn no_jar_reason(reader: &mut PomReader<'_>, coord: &Coord) -> Option<NoJar> {
    let pom = reader.pom(coord)?;
    if let Some(reloc) = pom.relocation.clone() {
        let moved = Coord {
            group_id: pick(&reloc.group_id, &coord.group_id),
            artifact_id: pick(&reloc.artifact_id, &coord.artifact_id),
            version: pick(&reloc.version, &coord.version),
            classifier: coord.classifier.clone(),
            // The relocation target is a real artifact, whatever `<type>` the dependency that
            // reached here wrote — and the old coordinate's own `pom` packaging is about the
            // redirect, not about what it redirects to.
            packaging: String::new(),
        };
        // A relocation that changes nothing would be an infinite loop, and one that points at
        // itself is a broken pom rather than a redirect.
        if moved.key() != coord.key() || moved.version != coord.version {
            return Some(NoJar::MovedTo(moved));
        }
    }
    (pom.packaging == "pom").then_some(NoJar::ByPackaging)
}

/// The relocation's value when it names one, else the coordinate's own — Maven's reading of an
/// omitted part.
fn pick(moved: &str, current: &str) -> String {
    if moved.is_empty() { current.to_string() } else { moved.to_string() }
}

fn push_once(out: &mut Vec<Coord>, seen: &mut HashSet<String>, coord: Coord, tag: &str) {
    if seen.insert(format!("{tag}{}", coord.gav())) {
        out.push(coord);
    }
}

/// Remember where an unresolved coordinate came from. First sighting wins, which is the shortest
/// path to it: the queue is breadth-first, so the first module to want it is the nearest answer.
fn record_origin(out: &mut Resolution, coord: &Coord, module: &str, via: Option<&str>) {
    out.origins.entry(coord.gav()).or_insert_with(|| Origin {
        module: module.to_string(),
        via: via.unwrap_or_default().to_string(),
    });
}

/// What to call a reactor module in a message: its directory relative to the project root, which is
/// the name in `<modules>` and the one the user can act on. The root module has no relative path,
/// so it is named by its `artifactId`.
fn module_label(root: &Path, dir: &Path, eff: &Effective) -> String {
    match dir.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/")) {
        Some(rel) if !rel.is_empty() => rel,
        _ => eff.coord.artifact_id.clone(),
    }
}

/// The versions the **project** pins, by [`Coord::key`] — applied to the whole graph, not only to
/// what the project declares.
///
/// Maven's `<dependencyManagement>` overrides the version a *transitive* dependency writes in its
/// own pom, and that is the entire point of importing a BOM: `spring-boot-dependencies` decides
/// which Jackson the graph gets, whoever asked for it and whichever version they asked for. Reading
/// each transitive at the version its own pom happens to declare produces a classpath no build has.
///
/// It is not a near miss either. On a Spring Boot 4.1 project it reported **nineteen artifacts
/// missing, and not one of them is on the classpath Maven builds** — jackson 2.21.1 where the BOM
/// says 2.21.4, `spring-boot-*` 4.0.5 and 4.0.7 where the project is on 4.1.0, junit-platform 6.1.0
/// where it is 6.0.3. Every one of them is a real artifact at a version nothing needs, so nothing
/// ever downloads it: the warning names artifacts that will still be absent tomorrow, and the
/// classpath quietly lacks the versions that *are* used.
///
/// Only the reactor's management is collected, and the first module to pin a key wins. This builds
/// **one** classpath for the whole project — the index serves a project, not a module at a time —
/// so there is one answer to give, and where two modules disagree the root's is the one the person
/// reading the editor is looking at.
///
/// A version that is a range or still carries a `${…}` is skipped rather than pinned: overriding a
/// version we can resolve with one we cannot would turn a working entry into a missing one.
fn project_management(effectives: &[Effective]) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    for eff in effectives {
        for (key, managed) in &eff.managed {
            if managed.version.is_empty()
                || managed.version.contains("${")
                || managed.version.starts_with('[')
                || managed.version.starts_with('(')
            {
                continue;
            }
            out.entry(key.clone()).or_insert_with(|| managed.version.clone());
        }
    }
    out
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

    impl Fixture {
        /// Install a **pom only** — a BOM, a parent, or a relocation. There is no jar, by design.
        fn install_pom_only(&self, group: &str, artifact: &str, version: &str, pom: &str) {
            let d = self.repo_root().join(group.replace('.', "/")).join(artifact).join(version);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{artifact}-{version}.pom")), pom).unwrap();
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

    /// The project's `<dependencyManagement>` decides the version of a **transitive**, not just of
    /// what the project declares. Nearest-wins would leave the dependency's own choice standing.
    #[test]
    fn the_projects_management_overrides_a_transitives_own_version() {
        let f = Fixture::new("managed-transitive");
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
                "<project><groupId>p</groupId><artifactId>app</artifactId><version>1</version>
                 <dependencyManagement><dependencies>{}</dependencies></dependencyManagement>
                 <dependencies>{}</dependencies></project>",
                dep("org.slf4j", "slf4j-api", "2.0.9"),
                dep("com.acme", "core", "1.0")
            ),
        );
        let r = f.resolve();
        assert!(r.is_complete(), "1.7.36 is not needed and must not be reported: {:?}", r.missing);
        assert!(r.jars.iter().any(|j| j.ends_with("slf4j-api-2.0.9.jar")), "{:?}", r.jars);
    }

    /// The same thing through an **imported BOM**, which is how every Spring Boot project pins its
    /// graph — and the shape that reported nineteen phantom artifacts on a project that builds.
    #[test]
    fn an_imported_bom_pins_a_transitive_that_asks_for_another_version() {
        let f = Fixture::new("bom-transitive");
        f.install("org.slf4j", "slf4j-api", "2.0.9", &lib("org.slf4j", "slf4j-api", "2.0.9", ""));
        f.install(
            "com.acme",
            "platform-bom",
            "1.0",
            &format!(
                "<project><groupId>com.acme</groupId><artifactId>platform-bom</artifactId>
                 <version>1.0</version><packaging>pom</packaging>
                 <dependencyManagement><dependencies>{}</dependencies></dependencyManagement></project>",
                dep("org.slf4j", "slf4j-api", "2.0.9")
            ),
        );
        f.install(
            "com.acme",
            "core",
            "1.0",
            &lib("com.acme", "core", "1.0", &dep("org.slf4j", "slf4j-api", "1.7.36")),
        );
        f.write_pom(
            "",
            &format!(
                "<project><groupId>p</groupId><artifactId>app</artifactId><version>1</version>
                 <dependencyManagement><dependencies>
                   <dependency><groupId>com.acme</groupId><artifactId>platform-bom</artifactId>
                     <version>1.0</version><type>pom</type><scope>import</scope></dependency>
                 </dependencies></dependencyManagement>
                 <dependencies>{}</dependencies></project>",
                dep("com.acme", "core", "1.0")
            ),
        );
        let r = f.resolve();
        assert!(
            r.missing.is_empty(),
            "the BOM's version is the one the build uses; 1.7.36 is a phantom: {:?}",
            r.missing
        );
        assert!(r.jars.iter().any(|j| j.ends_with("slf4j-api-2.0.9.jar")), "{:?}", r.jars);
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

    /// A `<profile>` inside a **library's** pom is dead code as far as this project is concerned:
    /// nothing downstream can switch it on, so Maven never fetches what it names. Reporting it is
    /// how a project that has never heard of MOXy is told one of *its* dependencies is missing —
    /// Jersey's parent declares it under `<profile id="moxy">` and every jersey artifact inherits
    /// that pom.
    #[test]
    fn a_transitives_profile_dependency_is_never_reported_missing() {
        let f = Fixture::new("transitive-profile");
        f.install(
            "org.glassfish.jersey",
            "project",
            "4.0.2",
            "<project><groupId>org.glassfish.jersey</groupId><artifactId>project</artifactId><version>4.0.2</version>
             <profiles><profile><id>moxy</id><dependencies>
               <dependency><groupId>org.eclipse.persistence</groupId>
                 <artifactId>org.eclipse.persistence.moxy</artifactId><version>5.0.0-B09</version></dependency>
             </dependencies></profile></profiles></project>",
        );
        f.install(
            "org.glassfish.jersey.core",
            "jersey-common",
            "4.0.2",
            "<project><parent><groupId>org.glassfish.jersey</groupId><artifactId>project</artifactId>
               <version>4.0.2</version></parent>
             <groupId>org.glassfish.jersey.core</groupId><artifactId>jersey-common</artifactId>
             <version>4.0.2</version></project>",
        );
        f.write_pom(
            "",
            &format!(
                "<project><groupId>p</groupId><artifactId>app</artifactId><version>1</version>\
                 <dependencies>{}</dependencies></project>",
                dep("org.glassfish.jersey.core", "jersey-common", "4.0.2")
            ),
        );
        let r = f.resolve();
        assert!(r.missing.is_empty(), "MOXy is nobody's dependency here: {:?}", r.missing);
        assert!(r.is_complete());
    }

    /// The coordinate on its own does not answer the only question a reactor of a dozen modules
    /// raises — *which* module wants it, and what dragged it in.
    #[test]
    fn a_missing_artifact_names_the_module_and_what_pulled_it_in() {
        let f = Fixture::new("origin");
        f.install(
            "com.acme",
            "core",
            "1.0",
            &lib("com.acme", "core", "1.0", &dep("com.acme", "absent", "2.4.0")),
        );
        f.write_pom(
            "",
            "<project><groupId>p</groupId><artifactId>root</artifactId><version>1</version>
             <packaging>pom</packaging><modules><module>service</module></modules></project>",
        );
        f.write_pom(
            "service",
            &format!(
                "<project><parent><groupId>p</groupId><artifactId>root</artifactId><version>1</version></parent>\
                 <artifactId>service</artifactId><dependencies>{}</dependencies></project>",
                dep("com.acme", "core", "1.0")
            ),
        );
        let r = f.resolve();
        assert_eq!(r.missing.len(), 1, "{:?}", r.missing);
        let origin = r.origin_of(&r.missing[0]).expect("an origin was recorded");
        assert_eq!(origin.module, "service");
        assert_eq!(origin.via, "com.acme:core:1.0");
        assert_eq!(
            r.describe(&r.missing[0]),
            "com.acme:absent:2.4.0 (in service, via com.acme:core:1.0)"
        );
        assert!(r.shortfall().unwrap().contains("in service, via com.acme:core:1.0"));
    }

    /// What a module declares itself has nothing in between to name, and saying `via` anyway would
    /// be noise on the commonest case.
    #[test]
    fn a_directly_declared_missing_artifact_names_only_its_module() {
        let f = Fixture::new("origin-direct");
        f.write_pom(
            "",
            &format!(
                "<project><groupId>p</groupId><artifactId>app</artifactId><version>1</version>\
                 <dependencies>{}</dependencies></project>",
                dep("com.acme", "absent", "2.4.0")
            ),
        );
        let r = f.resolve();
        assert_eq!(r.missing.len(), 1);
        assert_eq!(r.describe(&r.missing[0]), "com.acme:absent:2.4.0 (in app)");
    }

    /// The reported case, in miniature. `org.hibernate.orm:hibernate-jpamodelgen:7.4.5.Final` is a
    /// **relocation**: ORM 7 renamed it `hibernate-processor`, and the old coordinates keep
    /// publishing a jarless pom whose only content is where to go instead. Maven follows it. Not
    /// following it reported as missing a jar that is on disk under its new name — and the report
    /// named an artifact that will never exist, so no amount of downloading could have fixed it.
    #[test]
    fn a_relocated_artifact_resolves_to_where_it_moved() {
        let f = Fixture::new("reloc");
        f.install_pom_only(
            "org.hibernate.orm",
            "hibernate-jpamodelgen",
            "7.4.5.Final",
            "<project><groupId>org.hibernate.orm</groupId>\
             <artifactId>hibernate-jpamodelgen</artifactId><version>7.4.5.Final</version>\
             <packaging>pom</packaging><distributionManagement><relocation>\
             <groupId>org.hibernate.orm</groupId><artifactId>hibernate-processor</artifactId>\
             <version>7.4.5.Final</version></relocation></distributionManagement></project>",
        );
        f.install(
            "org.hibernate.orm",
            "hibernate-processor",
            "7.4.5.Final",
            &lib("org.hibernate.orm", "hibernate-processor", "7.4.5.Final", ""),
        );
        f.write_pom(
            "",
            &lib(
                "it.acme",
                "service",
                "1.0",
                &dep("org.hibernate.orm", "hibernate-jpamodelgen", "7.4.5.Final"),
            ),
        );
        let out = f.resolve();
        assert!(out.missing.is_empty(), "nothing is missing: {:?}", out.missing);
        assert!(
            out.jars.iter().any(|j| j.to_string_lossy().contains("hibernate-processor-7.4.5.Final.jar")),
            "the relocated jar must be on the classpath: {:?}",
            out.jars
        );
    }

    /// A relocation that only changes the groupId leaves the other parts out, and Maven reads an
    /// omitted part as "the same as before".
    #[test]
    fn a_relocation_that_omits_a_part_keeps_the_old_one() {
        let f = Fixture::new("reloc-partial");
        f.install_pom_only(
            "old.group",
            "widget",
            "2.0",
            "<project><groupId>old.group</groupId><artifactId>widget</artifactId>\
             <version>2.0</version><packaging>pom</packaging><distributionManagement>\
             <relocation><groupId>new.group</groupId></relocation>\
             </distributionManagement></project>",
        );
        f.install("new.group", "widget", "2.0", &lib("new.group", "widget", "2.0", ""));
        f.write_pom("", &lib("it.acme", "service", "1.0", &dep("old.group", "widget", "2.0")));
        let out = f.resolve();
        assert!(out.missing.is_empty(), "{:?}", out.missing);
        // The group's dots are directories in a repository path, so this is `new/group`.
        assert!(
            out.jars.iter().any(|j| j.to_string_lossy().contains("new/group")),
            "{:?}",
            out.jars
        );
    }

    /// An artifact whose OWN pom is `<packaging>pom</packaging>` has no jar, whatever `<type>` the
    /// dependency that named it wrote. Looking for one and reporting its absence is a false
    /// positive by construction.
    #[test]
    fn a_pom_packaged_artifact_named_without_a_type_is_not_missing() {
        let f = Fixture::new("pom-packaging");
        f.install_pom_only(
            "it.acme",
            "platform",
            "1.0",
            "<project><groupId>it.acme</groupId><artifactId>platform</artifactId>\
             <version>1.0</version><packaging>pom</packaging></project>",
        );
        // Declared with no `<type>`, so the dependency reads as a jar.
        f.write_pom("", &lib("it.acme", "service", "1.0", &dep("it.acme", "platform", "1.0")));
        let out = f.resolve();
        assert!(out.missing.is_empty(), "a pom-packaged artifact has no jar: {:?}", out.missing);
        assert!(
            !out.jars.iter().any(|j| j.to_string_lossy().ends_with(".pom")),
            "and its pom is not a classpath entry: {:?}",
            out.jars
        );
    }

    /// The genuine miss still reports. Nothing on disk claims to know the coordinate at all.
    #[test]
    fn an_artifact_with_no_pom_either_is_still_missing() {
        let f = Fixture::new("really-missing");
        f.write_pom("", &lib("it.acme", "service", "1.0", &dep("nowhere", "ghost", "9.9")));
        let out = f.resolve();
        assert!(out.missing.iter().any(|c| c.artifact_id == "ghost"), "{:?}", out.missing);
    }

}
