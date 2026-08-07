//! The reactor, assembled: every module's effective dependency list, matched against the jars that
//! actually resolved.
//!
//! ## The four questions, and where each is answered
//!
//! 1. **Which modules are there** — the root pom's `<modules>`, followed recursively, because a
//!    module may itself be a reactor.
//! 2. **What does a module depend on** — its own `<dependencies>`, plus every ancestor's, because
//!    a parent pom's dependencies are inherited by every child whether or not anyone remembers
//!    that.
//! 3. **Which version** — `${…}` expanded against the property scope of the pom that wrote the
//!    string, then `<dependencyManagement>` consulted for the ones that name none. Both walk the
//!    parent chain, nearest first, which is Maven's own rule.
//! 4. **Did it resolve** — the coordinate is looked for in the classpath Maven already produced
//!    for the index. Nothing is run here: the panel reports, it does not build.
//!
//! ## What is deliberately not attempted
//!
//! Imported BOMs (`<scope>import</scope>`), version ranges, and the conflict mediation that
//! decides which of two transitive versions wins. Each needs the full repository, not the files on
//! disk. Where they are the answer, the version simply stays unknown — and the resolved classpath
//! usually settles it anyway, which is the one place a guess is not a guess.

use std::path::{Path, PathBuf};

use crate::model::{Dependency, Module, Origin, Report, Site, Transitive};
use crate::pom::{self, Pom, RawDependency};
use crate::repo::{coord_of, JarCoord};

/// How deep a reactor is followed, and how long a parent chain may be. Both are far past anything
/// real; they exist so a cycle (a module that lists itself, a pom that is its own parent) costs a
/// bounded walk instead of the editor.
const MAX_DEPTH: usize = 8;
/// A hard ceiling on how many poms one project can pull in.
const MAX_POMS: usize = 512;

/// Read a Maven project's dependencies.
///
/// `jars` is the already-resolved dependency classpath — pass an empty slice when none has been
/// resolved yet, and every dependency comes back with an empty [`Dependency::resolved`] and
/// [`Report::resolved_known`] `false`, which the UI shows as *unknown* rather than *missing*.
pub fn read(root: &Path, jars: &[PathBuf]) -> Report {
    let poms = Poms::collect(root);
    if poms.entries.is_empty() {
        return Report {
            ecosystem: "maven".to_string(),
            unreadable: poms.unreadable,
            ..Report::default()
        };
    }

    let index = JarIndex::build(jars);
    let mut report = Report {
        ecosystem: "maven".to_string(),
        resolved_known: !jars.is_empty(),
        unreadable: poms.unreadable.clone(),
        ..Report::default()
    };

    for (i, entry) in poms.entries.iter().enumerate() {
        if !entry.is_module {
            continue;
        }
        let chain = poms.chain(i);
        let mut dependencies = Vec::new();
        for (dep, from) in poms.effective(&chain) {
            dependencies.push(poms.resolve(dep, from, &chain, &index));
        }
        report.modules.push(Module {
            name: entry.pom.display_name().to_string(),
            id: entry.pom.artifact_id.clone(),
            manifest: wire_path(&entry.path),
            kind: entry.pom.packaging.clone(),
            dependencies,
        });
    }

    let transitive = index.unclaimed(&report, &poms);
    report.transitive = transitive;
    report
}

// ── The poms of one project ──────────────────────────────────────────────────

struct Entry {
    path: PathBuf,
    pom: Pom,
    /// Whether this pom is a module of the reactor (reached through `<modules>`) rather than only
    /// an ancestor pulled in to answer a version. A parent that lives outside the project is not a
    /// module and must not become a group in the panel.
    is_module: bool,
}

struct Poms {
    entries: Vec<Entry>,
    unreadable: Vec<String>,
}

impl Poms {
    /// Read the root pom, every module it reaches, and every parent those need.
    fn collect(root: &Path) -> Self {
        let mut poms = Poms { entries: Vec::new(), unreadable: Vec::new() };
        poms.load_module(&normalize(&root.join("pom.xml")), MAX_DEPTH);
        // Parents second, so a parent that is also a module keeps the module flag it already has.
        let mut cursor = 0usize;
        while cursor < poms.entries.len() {
            if let Some(path) = poms.parent_path(cursor) {
                poms.load_ancestor(&path);
            }
            cursor += 1;
        }
        poms
    }

    /// Read a module pom and recurse into its own `<modules>`.
    ///
    /// A pom already loaded is not walked again — which is what makes a reactor that names the
    /// same module twice (or itself) terminate at the first visit rather than at the depth bound.
    /// Safe because ancestors are only pulled in *after* the whole module walk, so an early return
    /// here can never skip a pom that had been loaded as something else.
    fn load_module(&mut self, path: &Path, depth: usize) {
        if let Some(i) = self.entries.iter().position(|e| e.path == path) {
            self.entries[i].is_module = true;
            return;
        }
        let Some(i) = self.load(path, true) else { return };
        if depth == 0 {
            return;
        }
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for module in self.entries[i].pom.modules.clone() {
            // A `<module>` names a directory, but Maven also accepts the pom file itself.
            let target = dir.join(&module);
            let candidate =
                if target.extension().is_some() { target } else { target.join("pom.xml") };
            self.load_module(&normalize(&candidate), depth - 1);
        }
    }

    /// Read a pom that is only needed to answer questions — a parent outside the reactor — and
    /// then its parent, up the chain.
    fn load_ancestor(&mut self, path: &Path) {
        let mut next = Some(path.to_path_buf());
        for _ in 0..MAX_DEPTH {
            let Some(path) = next.take() else { return };
            let Some(i) = self.load(&path, false) else { return };
            next = self.parent_path(i);
        }
    }

    /// Parse `path` unless it is already loaded; returns its index. `is_module` is only ever
    /// turned **on** — a pom reached both ways is a module.
    fn load(&mut self, path: &Path, is_module: bool) -> Option<usize> {
        if let Some(i) = self.entries.iter().position(|e| e.path == path) {
            self.entries[i].is_module |= is_module;
            return Some(i);
        }
        if self.entries.len() >= MAX_POMS {
            return None;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            // Only worth reporting for a pom the project pointed at: a `<relativePath>` that
            // resolves to nothing is the normal way of saying "the parent is in the repository".
            if is_module {
                self.unreadable.push(wire_path(path));
            }
            return None;
        };
        self.entries.push(Entry { path: path.to_path_buf(), pom: pom::parse(&text), is_module });
        Some(self.entries.len() - 1)
    }

    /// Where entry `i`'s parent pom is on disk, if it says.
    fn parent_path(&self, i: usize) -> Option<PathBuf> {
        let entry = &self.entries[i];
        let parent = entry.pom.parent.as_ref()?;
        let relative = parent.relative_path.as_deref().unwrap_or("../pom.xml");
        if relative.is_empty() {
            return None; // explicitly "resolve from the repository, not from disk"
        }
        let dir = entry.path.parent()?;
        let target = dir.join(relative);
        let candidate = if target.extension().is_some() { target } else { target.join("pom.xml") };
        Some(normalize(&candidate))
    }

    /// Entry `i` and its ancestors, nearest first — the scope every lookup walks.
    ///
    /// The parent is found by path when the pom says where it is, and by **coordinate** otherwise:
    /// a module whose `<relativePath>` is missing or wrong is still parented by the reactor root
    /// nine times out of ten, and refusing to notice would lose every managed version in the
    /// project.
    fn chain(&self, i: usize) -> Vec<usize> {
        let mut out = vec![i];
        let mut current = i;
        for _ in 0..MAX_DEPTH {
            let Some(parent) = self.entries[current].pom.parent.as_ref() else { break };
            let by_path = self
                .parent_path(current)
                .and_then(|p| self.entries.iter().position(|e| e.path == p));
            let next = by_path.or_else(|| {
                self.entries.iter().position(|e| {
                    e.pom.artifact_id == parent.artifact_id
                        && (parent.group_id.is_empty()
                            || e.pom.effective_group() == parent.group_id)
                })
            });
            let Some(next) = next.filter(|n| !out.contains(n)) else { break };
            out.push(next);
            current = next;
        }
        out
    }

    /// Every dependency that applies to the module at the head of `chain`, paired with the index
    /// of the pom that wrote it. Its own first, then each ancestor's, skipping any artifact
    /// already present — the nearer declaration wins, which is Maven's rule and also the useful
    /// one to show.
    fn effective(&self, chain: &[usize]) -> Vec<(&RawDependency, usize)> {
        let mut out: Vec<(&RawDependency, usize)> = Vec::new();
        for &i in chain {
            for dep in &self.entries[i].pom.dependencies {
                let seen = out.iter().any(|(d, _)| {
                    d.group_id == dep.group_id
                        && d.artifact_id == dep.artifact_id
                        && d.classifier == dep.classifier
                });
                if !seen {
                    out.push((dep, i));
                }
            }
        }
        out
    }

    /// Turn one raw declaration into the answer: version expanded, management applied, jar found.
    fn resolve(
        &self,
        dep: &RawDependency,
        from: usize,
        chain: &[usize],
        jars: &JarIndex,
    ) -> Dependency {
        let own_chain = self.chain(from);
        let group = self.expand(&dep.group_id, &own_chain);
        let artifact = self.expand(&dep.artifact_id, &own_chain);
        let classifier = self.expand(&dep.classifier, &own_chain);

        // Origin first: whether the module wrote this at all is the more informative fact, so a
        // dependency inherited from a parent keeps that label even when a third pom pins its
        // version.
        let inherited = from != chain[0];
        let mut origin = if inherited {
            Origin::Inherited { from: self.entries[from].pom.artifact_id.clone() }
        } else {
            Origin::Declared
        };

        let mut version = self.expand(&dep.version, &own_chain);
        let mut scope = dep.scope.clone();
        if version.is_empty() || scope.is_empty() {
            if let Some((managed, at)) = self.managed_for(&group, &artifact, &classifier, chain) {
                let managing = self.chain(at);
                if version.is_empty() {
                    version = self.expand(&managed.version, &managing);
                    if !inherited && !version.is_empty() {
                        origin =
                            Origin::Managed { from: self.entries[at].pom.artifact_id.clone() };
                    }
                }
                if scope.is_empty() {
                    scope = managed.scope.clone();
                }
            }
        }

        // The classpath is the last word on a version the poms could not settle — an imported BOM,
        // a parent that only exists in the repository. It is not a guess: it is the jar the
        // compiler is being handed.
        let hit = jars.find(&group, &artifact, &version, &classifier);
        if version.is_empty() {
            if let Some(h) = &hit {
                version = h.coord.version.clone();
            }
        }

        Dependency {
            group,
            // Provenance is Cargo's question; for Maven the repository the jar came from answers it.
            source: String::new(),
            name: artifact,
            version,
            scope: if scope.is_empty() { "compile".to_string() } else { scope },
            kind: if dep.packaging == "jar" { String::new() } else { dep.packaging.clone() },
            variant: classifier,
            optional: dep.optional,
            origin,
            condition: dep.profile.clone(),
            // Maven has no features.
            features: Vec::new(),
            declared_in: Site {
                file: wire_path(&self.entries[from].path),
                offset: dep.offset,
                line: dep.line,
            },
            resolved: hit.map(|h| h.path.clone()).unwrap_or_default(),
        }
    }

    /// The `<dependencyManagement>` entry for a coordinate, nearest pom in `chain` first.
    fn managed_for(
        &self,
        group: &str,
        artifact: &str,
        classifier: &str,
        chain: &[usize],
    ) -> Option<(&RawDependency, usize)> {
        for &i in chain {
            let own = self.chain(i);
            let hit = self.entries[i].pom.managed.iter().find(|m| {
                self.expand(&m.artifact_id, &own) == artifact
                    && self.expand(&m.classifier, &own) == classifier
                    && (group.is_empty() || self.expand(&m.group_id, &own) == group)
            });
            if let Some(m) = hit {
                return Some((m, i));
            }
        }
        None
    }

    /// Expand `${…}` against the property scope of `chain`.
    ///
    /// Bounded rather than recursive-until-stable: property chains three deep are normal
    /// (`${spring.version}` → `${framework.version}`), a cycle is a broken pom, and an editor must
    /// answer either way. What cannot be expanded is left **as written**, because a `${…}` nothing
    /// defines is usually the bug you are looking for.
    fn expand(&self, value: &str, chain: &[usize]) -> String {
        let mut out = value.to_string();
        for _ in 0..8 {
            if !out.contains("${") {
                break;
            }
            let next = self.expand_once(&out, chain);
            if next == out {
                break;
            }
            out = next;
        }
        out
    }

    fn expand_once(&self, value: &str, chain: &[usize]) -> String {
        let mut out = String::with_capacity(value.len());
        let mut rest = value;
        while let Some(start) = rest.find("${") {
            let Some(len) = rest[start..].find('}') else { break };
            let key = &rest[start + 2..start + len];
            out.push_str(&rest[..start]);
            match self.property(key, chain) {
                Some(v) => out.push_str(&v),
                None => out.push_str(&rest[start..start + len + 1]),
            }
            rest = &rest[start + len + 1..];
        }
        out.push_str(rest);
        out
    }

    /// A property value, from the nearest pom in `chain` that defines it — or from the built-ins
    /// every pom has.
    fn property(&self, key: &str, chain: &[usize]) -> Option<String> {
        for &i in chain {
            if let Some(v) = self.entries[i].pom.property(key) {
                return Some(v.to_string());
            }
        }
        let head = &self.entries[*chain.first()?].pom;
        // `pom.` is the Maven 1 spelling and the bare forms are the Maven 2 legacy ones; all three
        // are still written, and all three mean the project's own coordinate.
        let name = key.strip_prefix("project.").or_else(|| key.strip_prefix("pom.")).unwrap_or(key);
        match name {
            "version" => Some(head.effective_version().to_string()),
            "groupId" => Some(head.effective_group().to_string()),
            "artifactId" => Some(head.artifact_id.clone()),
            "parent.version" => head.parent.as_ref().map(|p| p.version.clone()),
            "parent.groupId" => head.parent.as_ref().map(|p| p.group_id.clone()),
            "parent.artifactId" => head.parent.as_ref().map(|p| p.artifact_id.clone()),
            _ => None,
        }
        .filter(|v| !v.is_empty())
    }

    /// Whether any pom in the project is this artifact — used to keep a sibling module's own jar
    /// out of the "something dragged this in" list.
    fn is_reactor_artifact(&self, artifact: &str) -> bool {
        self.entries.iter().any(|e| e.pom.artifact_id == artifact)
    }
}

// ── The resolved classpath ───────────────────────────────────────────────────

struct Jar {
    coord: JarCoord,
    path: String,
}

struct JarIndex {
    jars: Vec<Jar>,
}

impl JarIndex {
    fn build(paths: &[PathBuf]) -> Self {
        let jars = paths
            .iter()
            .filter_map(|p| coord_of(p).map(|coord| Jar { coord, path: wire_path(p) }))
            .collect();
        JarIndex { jars }
    }

    /// The jar for a coordinate.
    ///
    /// Matched on artifactId and classifier, then narrowed by whatever else is known: the group
    /// when both sides have one (a repository path outside `~/.m2` yields none), the version when
    /// the poms produced one. An artifact present at two versions with the version unknown matches
    /// nothing rather than the first — showing a version the project may not be using is worse
    /// than showing none.
    fn find(&self, group: &str, artifact: &str, version: &str, classifier: &str) -> Option<&Jar> {
        let mut hits = self.jars.iter().filter(|j| {
            j.coord.artifact_id == artifact
                && j.coord.classifier == classifier
                && (group.is_empty() || j.coord.group_id.is_empty() || j.coord.group_id == group)
        });
        if version.is_empty() {
            let first = hits.next()?;
            return hits.next().is_none().then_some(first);
        }
        hits.find(|j| j.coord.version == version)
    }

    /// The jars no module accounted for: what the declared dependencies dragged in.
    fn unclaimed(&self, report: &Report, poms: &Poms) -> Vec<Transitive> {
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
            .filter(|j| !poms.is_reactor_artifact(&j.coord.artifact_id))
            .map(|j| Transitive {
                group: j.coord.group_id.clone(),
                name: j.coord.artifact_id.clone(),
                version: j.coord.version.clone(),
                resolved: j.path.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.coord().cmp(&b.coord()).then(a.version.cmp(&b.version)));
        out
    }
}

// ── Paths ────────────────────────────────────────────────────────────────────

/// Lexically resolve `.` and `..`, so `<root>/web/../pom.xml` and `<root>/pom.xml` are recognised
/// as the same file.
///
/// Lexical rather than [`std::fs::canonicalize`] on purpose: the paths are shown to the user and
/// go into go-to targets, and canonicalizing on Windows yields the `\\?\C:\…` verbatim form, which
/// is correct, unopenable by half the world, and unrecognisable next to the path the user typed.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// A path as the bennu wire writes them: absolute, forward slashes.
fn wire_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A reactor on disk: a parent with management + one inherited dependency, and two modules.
    fn fixture(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("bennu-deps-{tag}"));
        let _ = fs::remove_dir_all(&root);
        write(
            &root.join("pom.xml"),
            r#"<project>
                 <groupId>com.acme</groupId>
                 <artifactId>portale-parent</artifactId>
                 <version>2.4.0</version>
                 <packaging>pom</packaging>
                 <modules><module>web</module><module>core</module></modules>
                 <properties><spring.version>5.3.27</spring.version></properties>
                 <dependencyManagement><dependencies>
                   <dependency>
                     <groupId>org.springframework</groupId>
                     <artifactId>spring-web</artifactId>
                     <version>${spring.version}</version>
                   </dependency>
                   <dependency>
                     <groupId>javax.servlet</groupId>
                     <artifactId>javax.servlet-api</artifactId>
                     <version>4.0.1</version>
                     <scope>provided</scope>
                   </dependency>
                 </dependencies></dependencyManagement>
                 <dependencies>
                   <dependency>
                     <groupId>org.projectlombok</groupId>
                     <artifactId>lombok</artifactId>
                     <version>1.18.30</version>
                     <scope>provided</scope>
                   </dependency>
                 </dependencies>
               </project>"#,
        );
        write(
            &root.join("web/pom.xml"),
            r#"<project>
                 <parent>
                   <groupId>com.acme</groupId>
                   <artifactId>portale-parent</artifactId>
                   <version>2.4.0</version>
                 </parent>
                 <artifactId>portale-web</artifactId>
                 <packaging>war</packaging>
                 <dependencies>
                   <dependency>
                     <groupId>org.springframework</groupId>
                     <artifactId>spring-web</artifactId>
                   </dependency>
                   <dependency>
                     <groupId>javax.servlet</groupId>
                     <artifactId>javax.servlet-api</artifactId>
                   </dependency>
                   <dependency>
                     <groupId>com.acme</groupId>
                     <artifactId>portale-core</artifactId>
                     <version>${project.version}</version>
                   </dependency>
                 </dependencies>
               </project>"#,
        );
        write(
            &root.join("core/pom.xml"),
            r#"<project>
                 <parent><artifactId>portale-parent</artifactId><version>2.4.0</version></parent>
                 <artifactId>portale-core</artifactId>
               </project>"#,
        );
        root
    }

    fn write(path: &Path, text: &str) {
        let _ = fs::create_dir_all(path.parent().unwrap());
        fs::write(path, text).unwrap();
    }

    fn module<'a>(report: &'a Report, artifact: &str) -> &'a Module {
        report.modules.iter().find(|m| m.id == artifact).expect("module")
    }

    fn dep<'a>(m: &'a Module, artifact: &str) -> &'a Dependency {
        m.dependencies.iter().find(|d| d.name == artifact).expect("dependency")
    }

    #[test]
    fn every_module_of_the_reactor_becomes_a_group() {
        let root = fixture("reactor");
        let report = read(&root, &[]);
        let names: Vec<&str> = report.modules.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(names, ["portale-parent", "portale-web", "portale-core"]);
        assert_eq!(module(&report, "portale-parent").kind, "pom");
        assert!(report.unreadable.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    /// The three origins, which is the question this panel exists to answer.
    #[test]
    fn a_version_says_which_pom_decided_it() {
        let root = fixture("origins");
        let report = read(&root, &[]);
        let web = module(&report, "portale-web");

        // Declared here, version and all.
        assert!(matches!(dep(web, "portale-core").origin, Origin::Declared));
        // Declared here without a version — the parent's management supplies it, THROUGH a
        // property declared in that same parent.
        let spring = dep(web, "spring-web");
        assert_eq!(spring.version, "5.3.27");
        assert_eq!(spring.origin, Origin::Managed { from: "portale-parent".into() });
        // Not written in this module at all — the parent's own `<dependencies>` are inherited.
        let lombok = dep(web, "lombok");
        assert_eq!(lombok.origin, Origin::Inherited { from: "portale-parent".into() });
        assert_eq!(lombok.version, "1.18.30");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_managed_scope_is_applied_and_the_default_is_never_blank() {
        let root = fixture("scopes");
        let report = read(&root, &[]);
        let web = module(&report, "portale-web");
        assert_eq!(dep(web, "javax.servlet-api").scope, "provided", "from the management entry");
        assert_eq!(dep(web, "portale-core").scope, "compile", "Maven's default, made explicit");
        let _ = fs::remove_dir_all(&root);
    }

    /// `${project.version}` in a module that declares no `<version>` of its own has to expand to
    /// the parent's — the single most common interpolation in a multi-module build.
    #[test]
    fn project_version_expands_through_the_parent() {
        let root = fixture("props");
        let report = read(&root, &[]);
        assert_eq!(dep(module(&report, "portale-web"), "portale-core").version, "2.4.0");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_resolved_jar_is_matched_to_the_dependency_that_asked_for_it() {
        let root = fixture("jars");
        let jars = vec![
            PathBuf::from("/r/.m2/repository/org/springframework/spring-web/5.3.27/spring-web-5.3.27.jar"),
            PathBuf::from("/r/.m2/repository/org/slf4j/slf4j-api/1.7.36/slf4j-api-1.7.36.jar"),
        ];
        let report = read(&root, &jars);
        assert!(report.resolved_known);
        let web = module(&report, "portale-web");
        assert!(dep(web, "spring-web").resolved.ends_with("spring-web-5.3.27.jar"));
        assert!(dep(web, "javax.servlet-api").resolved.is_empty(), "declared, never resolved");
        // Nobody declared slf4j — something dragged it in, and that is its own list.
        assert_eq!(report.transitive.len(), 1);
        assert_eq!(report.transitive[0].coord(), "org.slf4j:slf4j-api");
        let _ = fs::remove_dir_all(&root);
    }

    /// A sibling module's own artifact appears on the classpath of the module that depends on it,
    /// and calling that "transitive" would be nonsense.
    #[test]
    fn a_reactor_modules_own_jar_is_not_a_transitive_dependency() {
        let root = fixture("sibling");
        let jars =
            vec![PathBuf::from("/r/.m2/repository/com/acme/portale-core/2.4.0/portale-core-2.4.0.jar")];
        let report = read(&root, &jars);
        assert!(report.transitive.is_empty());
        assert!(dep(module(&report, "portale-web"), "portale-core").resolved.ends_with("portale-core-2.4.0.jar"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_project_that_is_not_maven_reports_nothing_rather_than_failing() {
        let dir = std::env::temp_dir().join("bennu-deps-empty");
        let _ = fs::create_dir_all(&dir);
        let report = read(&dir, &[]);
        assert!(report.modules.is_empty());
        assert!(!report.resolved_known);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_module_that_lists_itself_terminates() {
        let root = std::env::temp_dir().join("bennu-deps-cycle");
        let _ = fs::remove_dir_all(&root);
        write(
            &root.join("pom.xml"),
            "<project><artifactId>loop</artifactId><modules><module>.</module></modules></project>",
        );
        assert_eq!(read(&root, &[]).modules.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn paths_are_normalized_before_they_are_compared() {
        assert_eq!(normalize(Path::new("/a/b/../pom.xml")), PathBuf::from("/a/pom.xml"));
        assert_eq!(normalize(Path::new("/a/./b/pom.xml")), PathBuf::from("/a/b/pom.xml"));
    }
}
