//! An artifact's pom, with everything its parents and its BOMs contribute already folded in.
//!
//! ## Why a pom in the repository is not enough on its own
//!
//! `spring-boot-starter-web-3.2.0.pom` declares five dependencies and gives a version for none of
//! them. The versions are in `spring-boot-dependencies`, imported as a BOM by a parent two levels
//! up, and half of them are written as `${jackson.version}` even there. A resolver that reads the
//! pom in front of it and stops resolves nothing at all.
//!
//! So this does what Maven does before anything else happens: walk the parent chain, collect the
//! properties (child wins), collect `<dependencyManagement>` (child wins, imported BOMs behind the
//! pom that imports them), and expand `${…}` against the result.
//!
//! ## What it does not do
//!
//! Profiles are not activated — whether one is on depends on the JDK, the OS and a `-P` flag, none
//! of which is a fact about the file. Their dependencies are read (they are the reason half the
//! legacy poms in the world resolve at all) and their `<optional>` handling is the same as any
//! other, which errs toward listing an artifact the build might not use rather than missing one it
//! does. Version *ranges* (`[1.0,2.0)`) are left as written and treated as unresolvable, which is
//! honest: picking one would be inventing a build.

use std::collections::HashMap;

use bennu_deps::prelude::{parse_pom, ParentRef, Pom, RawDependency};

use crate::repo::{Coord, LocalRepo};

/// How deep a parent chain is followed before it is assumed to be a cycle. Real chains are three
/// or four; anything past this is a pom that names itself, directly or through a loop.
const MAX_PARENT_DEPTH: usize = 16;

/// A pom with its inheritance applied.
#[derive(Debug, Clone, Default)]
pub struct Effective {
    /// The coordinate this describes.
    pub coord: Coord,
    /// Every property in scope, `${…}` already expanded where one property names another.
    pub properties: HashMap<String, String>,
    /// `<dependencyManagement>`, keyed by [`Coord::key`] — what pins a version the dependency
    /// itself does not write.
    pub managed: HashMap<String, Managed>,
    /// The dependencies the artifact actually declares, parents' included, with `${…}` expanded and
    /// management applied.
    pub dependencies: Vec<Resolved>,
    /// The pom files of the inheritance chain, this one first. Every one of them is a file that can
    /// be opened, which is what makes "where does this come from" answerable with a jump rather
    /// than with a sentence.
    pub chain: Vec<String>,
    /// Property name → the pom that defines it (the nearest one, since a child's wins).
    pub property_sites: HashMap<String, String>,
}

/// A managed version (and scope), and the pom that supplied it — the second half is what makes
/// "where does this version come from" answerable.
#[derive(Debug, Clone, Default)]
pub struct Managed {
    pub version: String,
    pub scope: String,
    pub optional: Option<bool>,
    /// Excluded coordinates as `groupId:artifactId`, from the managed entry.
    pub exclusions: Vec<String>,
    /// The artifact whose `<dependencyManagement>` this came from.
    pub from: String,
    /// The pom file that declares it, when there is one to open — a parent on disk, or the
    /// artifact's own `.pom` in the local repository, which is a real file either way. What turns
    /// "the version comes from somewhere else" into a jump.
    pub from_path: String,
}

/// One dependency, after properties and management.
#[derive(Debug, Clone, Default)]
pub struct Resolved {
    pub coord: Coord,
    pub scope: String,
    pub optional: bool,
    /// `groupId:artifactId` pairs this dependency excludes from its own subtree.
    pub exclusions: Vec<String>,
    /// Empty when the dependency writes its own version; otherwise what supplied it.
    pub managed_by: String,
    /// The profile it was declared under, empty for the ordinary case.
    pub profile: String,
    /// Byte offset of the `<dependency>` in the pom that declares it, and its 1-based line.
    pub offset: usize,
    pub line: u32,
}

impl Resolved {
    /// Whether the version is a range (`[1.0,2.0)`) rather than a point. Nothing here picks one.
    pub fn is_range(&self) -> bool {
        let v = self.coord.version.trim();
        v.starts_with('[') || v.starts_with('(')
    }

    /// Whether a `${…}` survived expansion — a property nothing in scope defines, which is nearly
    /// always a real bug in the pom rather than something to hide.
    pub fn has_unresolved_property(&self) -> bool {
        self.coord.version.contains("${")
    }
}

/// Reads poms out of the local repository, once each.
///
/// A reactor of forty modules asks for the same twenty parents; a transitive walk asks for the same
/// BOM from every branch. Without the memo this is the whole cost of the resolver.
pub struct PomReader<'a> {
    repo: &'a LocalRepo,
    poms: HashMap<String, Option<Pom>>,
    effective: HashMap<String, Effective>,
    /// The artifacts whose effective pom is being built right now.
    ///
    /// A BOM that imports itself — directly, or through a second BOM that imports the first — is a
    /// published artifact somebody really has released, and without this it is an infinite
    /// recursion inside an editor keystroke rather than a broken dependency.
    building: std::collections::HashSet<String>,
}

impl<'a> PomReader<'a> {
    pub fn new(repo: &'a LocalRepo) -> Self {
        Self { repo, poms: HashMap::new(), effective: HashMap::new(), building: Default::default() }
    }

    pub fn repo(&self) -> &LocalRepo {
        self.repo
    }

    /// The raw pom of a repository artifact, or `None` when it is not installed.
    pub fn pom(&mut self, coord: &Coord) -> Option<&Pom> {
        let key = coord.gav();
        if !self.poms.contains_key(&key) {
            let path = self.repo.pom_file(coord);
            let parsed = std::fs::read(&path)
                .ok()
                .map(|bytes| parse_pom(&String::from_utf8_lossy(&bytes)));
            self.poms.insert(key.clone(), parsed);
        }
        self.poms.get(&key).and_then(|p| p.as_ref())
    }

    /// The effective pom of a repository artifact.
    pub fn effective(&mut self, coord: &Coord) -> Option<&Effective> {
        let key = coord.gav();
        if !self.effective.contains_key(&key) {
            if !self.building.insert(key.clone()) {
                return None; // an import cycle — see `building`
            }
            let pom = self.pom(coord).cloned();
            let built = pom.map(|pom| self.build(coord.clone(), &pom, None));
            self.building.remove(&key);
            self.effective.insert(key.clone(), built?);
        }
        self.effective.get(&key)
    }

    /// The effective pom of a pom **on disk** — a reactor module, whose parents may be on disk too
    /// (`<relativePath>`) or in the repository.
    ///
    /// The same machinery as [`Self::effective`], entered from a file rather than a coordinate,
    /// which is what lets a reactor module import a BOM and have its versions found.
    pub fn effective_of_file(&mut self, pom: &Pom, dir: &std::path::Path) -> Effective {
        let coord = Coord::new(pom.effective_group(), &pom.artifact_id, pom.effective_version());
        self.build(coord, pom, Some(dir))
    }

    /// The shared body: walk up, collect, expand.
    fn build(&mut self, coord: Coord, pom: &Pom, dir: Option<&std::path::Path>) -> Effective {
        let own_path = match dir {
            Some(d) => d.join("pom.xml"),
            None => self.repo.pom_file(&coord),
        };
        let mut chain: Vec<Link> =
            vec![Link { pom: pom.clone(), dir: dir.map(|d| d.to_path_buf()), path: own_path }];
        let mut depth = 0;
        while depth < MAX_PARENT_DEPTH {
            let Some(last) = chain.last() else { break };
            let Some(parent_ref) = last.pom.parent.clone() else { break };
            let Some(parent) = self.read_parent(&parent_ref, last.dir.as_deref()) else { break };
            chain.push(parent);
            depth += 1;
        }

        // Properties: the child's win, so collect from the top of the chain downward.
        let mut properties: HashMap<String, String> = HashMap::new();
        let mut property_sites: HashMap<String, String> = HashMap::new();
        for link in chain.iter().rev() {
            for (k, v) in &link.pom.properties {
                properties.insert(k.clone(), v.clone());
                property_sites.insert(k.clone(), forward(&link.path));
            }
        }
        // The implicit ones, which a pom references as often as it does its own.
        let root = &chain[0].pom;
        for (k, v) in [
            ("project.version", root.effective_version()),
            ("project.groupId", root.effective_group()),
            ("project.artifactId", root.artifact_id.as_str()),
            ("pom.version", root.effective_version()),
            ("pom.groupId", root.effective_group()),
            ("version", root.effective_version()),
        ] {
            properties.entry(k.to_string()).or_insert_with(|| v.to_string());
        }
        if let Some(parent) = &root.parent {
            properties.entry("project.parent.version".into()).or_insert(parent.version.clone());
            properties.entry("project.parent.groupId".into()).or_insert(parent.group_id.clone());
        }
        resolve_property_references(&mut properties);

        // Management: the child's wins, and an imported BOM sits behind the pom that imports it.
        let mut managed: HashMap<String, Managed> = HashMap::new();
        for link in chain.iter().rev() {
            let pom = &link.pom;
            let owner = format!("{}:{}", pom.effective_group(), pom.artifact_id);
            let owner_path = forward(&link.path);
            let mut imports: Vec<Coord> = Vec::new();
            for raw in &pom.managed {
                let coord = coord_of(raw, &properties);
                if raw.scope == "import" && coord.is_pom() {
                    imports.push(coord);
                    continue;
                }
                managed.insert(
                    coord.key(),
                    Managed {
                        version: coord.version,
                        scope: expand(&raw.scope, &properties),
                        optional: raw.optional.then_some(true),
                        exclusions: Vec::new(),
                        from: owner.clone(),
                        from_path: owner_path.clone(),
                    },
                );
            }
            // Imports are read after the pom's own entries and must not overwrite them: an explicit
            // `<dependencyManagement>` entry beside an imported BOM is precisely how a project pins
            // one library away from what the BOM says.
            for bom in imports {
                let Some(imported) = self.effective(&bom).cloned() else { continue };
                for (key, entry) in imported.managed {
                    managed.entry(key).or_insert(entry);
                }
            }
        }

        // Dependencies: the artifact's own, plus every parent's (Maven inherits `<dependencies>`).
        let mut dependencies: Vec<Resolved> = Vec::new();
        for link in chain.iter() {
            for raw in &link.pom.dependencies {
                let mut coord = coord_of(raw, &properties);
                let mut scope = expand(&raw.scope, &properties);
                let mut optional = raw.optional;
                let mut managed_by = String::new();
                if let Some(pin) = managed.get(&coord.key()) {
                    if coord.version.is_empty() {
                        coord.version = pin.version.clone();
                        managed_by = pin.from.clone();
                    }
                    if scope.is_empty() && !pin.scope.is_empty() {
                        scope = pin.scope.clone();
                    }
                    if let Some(o) = pin.optional {
                        optional |= o;
                    }
                }
                if scope.is_empty() {
                    scope = "compile".to_string();
                }
                if dependencies.iter().any(|d| d.coord.key() == coord.key()) {
                    continue; // a child's declaration wins over the parent's
                }
                dependencies.push(Resolved {
                    coord,
                    scope,
                    optional,
                    exclusions: raw_exclusions(raw),
                    managed_by,
                    profile: raw.profile.clone(),
                    offset: raw.offset,
                    line: raw.line,
                });
            }
        }

        Effective {
            coord,
            properties,
            managed,
            dependencies,
            chain: chain.iter().map(|l| forward(&l.path)).collect(),
            property_sites,
        }
    }

    /// A parent pom: the one on disk when `<relativePath>` points at a real file, else the
    /// repository's.
    ///
    /// Disk first, and it matters: a reactor's own parent is usually **not installed**, so a
    /// resolver that only looked in the repository would lose every version a reactor parent pins —
    /// which on a multi-module project is most of them.
    fn read_parent(&mut self, parent: &ParentRef, child_dir: Option<&std::path::Path>) -> Option<Link> {
        if let Some(dir) = child_dir {
            // An explicitly empty `<relativePath/>` means "do not look on disk" — Maven's own way
            // of saying the parent is a released artifact, and following it anyway can read a
            // completely unrelated pom that happens to sit one directory up.
            let relative = parent.relative_path.as_deref().unwrap_or("../pom.xml");
            if !relative.trim().is_empty() {
                let candidate = dir.join(relative);
                let candidate =
                    if candidate.is_dir() { candidate.join("pom.xml") } else { candidate };
                if let Ok(bytes) = std::fs::read(&candidate) {
                    let pom = parse_pom(&String::from_utf8_lossy(&bytes));
                    // Only when it really is the parent it names: `../pom.xml` in a repository
                    // checkout can easily be somebody else's pom.
                    if pom.artifact_id == parent.artifact_id {
                        let dir = candidate.parent().map(|p| p.to_path_buf());
                        return Some(Link { pom, dir, path: candidate });
                    }
                }
            }
        }
        let coord = Coord {
            packaging: "pom".into(),
            ..Coord::new(&parent.group_id, &parent.artifact_id, &parent.version)
        };
        let path = self.repo.pom_file(&coord);
        self.pom(&coord).cloned().map(|pom| Link { pom, dir: None, path })
    }
}

/// The effective pom of a **buffer** — the text in the editor, whose parents are on disk.
///
/// The one entry point a caller with a file and its text needs: parsing the buffer, finding its
/// directory and walking its chain are three steps that always happen together, and the editor and
/// its tests doing them separately is how the two come to disagree about what a pom says.
pub fn effective_of_buffer(repo: &LocalRepo, path: &std::path::Path, source: &str) -> Effective {
    let pom = parse_pom(source);
    let dir = path.parent().unwrap_or(std::path::Path::new(""));
    PomReader::new(repo).effective_of_file(&pom, dir)
}

/// One pom of an inheritance chain, and where it lives.
struct Link {
    pom: Pom,
    /// The directory, for resolving the *next* `<relativePath>`. `None` for a pom read out of the
    /// repository, which has no reactor around it.
    dir: Option<std::path::PathBuf>,
    /// The pom file itself — always a real file, whether on disk or in `~/.m2`.
    path: std::path::PathBuf,
}

/// A path as the bennu wire writes one: absolute, forward slashes.
fn forward(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// A raw dependency's coordinate, with `${…}` expanded.
fn coord_of(raw: &RawDependency, properties: &HashMap<String, String>) -> Coord {
    Coord {
        group_id: expand(&raw.group_id, properties),
        artifact_id: expand(&raw.artifact_id, properties),
        version: expand(&raw.version, properties),
        classifier: expand(&raw.classifier, properties),
        packaging: expand(&raw.packaging, properties),
    }
}

/// A dependency's `<exclusions>`, as `groupId:artifactId`.
///
/// The pom reader does not model them, so they are read off the raw text of the dependency block —
/// see [`crate::exclusions`].
fn raw_exclusions(raw: &RawDependency) -> Vec<String> {
    raw.exclusions.iter().map(|(g, a)| format!("{g}:{a}")).collect()
}

/// Expand `${…}` against a property map. An unknown property is left **as written**: that is nearly
/// always a bug in the pom, and silently blanking it would hide the one thing worth reporting.
pub fn expand(text: &str, properties: &HashMap<String, String>) -> String {
    if !text.contains("${") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &rest[start + 2..start + end];
        match properties.get(name) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[start..start + end + 1]),
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Properties that name other properties (`<spring.version>${platform.version}</spring.version>`),
/// expanded until they stop changing. Bounded, because a pom can define two that name each other.
fn resolve_property_references(properties: &mut HashMap<String, String>) {
    for _ in 0..8 {
        let mut changed = false;
        let snapshot = properties.clone();
        for (_, value) in properties.iter_mut() {
            if !value.contains("${") {
                continue;
            }
            let expanded = expand(value, &snapshot);
            if &expanded != value {
                *value = expanded;
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        dir: std::path::PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("bennu-mvn-eff-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        fn install(&self, group: &str, artifact: &str, version: &str, pom: &str) {
            let d = self.dir.join(group.replace('.', "/")).join(artifact).join(version);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{artifact}-{version}.pom")), pom).unwrap();
        }

        fn repo(&self) -> LocalRepo {
            LocalRepo::at(&self.dir)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// The case the whole module exists for: a starter that gives no version for anything, whose
    /// parent imports a BOM that does.
    #[test]
    fn a_version_from_an_imported_bom_two_poms_up_is_found() {
        let f = Fixture::new("bom");
        f.install(
            "com.acme",
            "platform-bom",
            "1.0",
            r#"<project><groupId>com.acme</groupId><artifactId>platform-bom</artifactId><version>1.0</version>
               <packaging>pom</packaging>
               <properties><jackson.version>2.15.2</jackson.version></properties>
               <dependencyManagement><dependencies>
                 <dependency><groupId>com.fasterxml.jackson.core</groupId><artifactId>jackson-databind</artifactId>
                   <version>${jackson.version}</version></dependency>
               </dependencies></dependencyManagement></project>"#,
        );
        f.install(
            "com.acme",
            "parent",
            "1.0",
            r#"<project><groupId>com.acme</groupId><artifactId>parent</artifactId><version>1.0</version>
               <packaging>pom</packaging>
               <dependencyManagement><dependencies>
                 <dependency><groupId>com.acme</groupId><artifactId>platform-bom</artifactId>
                   <version>1.0</version><type>pom</type><scope>import</scope></dependency>
               </dependencies></dependencyManagement></project>"#,
        );
        f.install(
            "com.acme",
            "starter",
            "1.0",
            r#"<project><parent><groupId>com.acme</groupId><artifactId>parent</artifactId><version>1.0</version></parent>
               <artifactId>starter</artifactId>
               <dependencies>
                 <dependency><groupId>com.fasterxml.jackson.core</groupId><artifactId>jackson-databind</artifactId></dependency>
               </dependencies></project>"#,
        );

        let repo = f.repo();
        let mut reader = PomReader::new(&repo);
        let eff = reader.effective(&Coord::new("com.acme", "starter", "1.0")).unwrap();
        let dep = &eff.dependencies[0];
        assert_eq!(dep.coord.version, "2.15.2");
        assert_eq!(dep.managed_by, "com.acme:parent");
        assert_eq!(dep.scope, "compile", "Maven's default, applied");
    }

    /// A property that names another property is what every Spring-era pom is written with.
    #[test]
    fn a_property_that_names_a_property_still_expands() {
        let mut props = HashMap::from([
            ("platform.version".to_string(), "5.3.27".to_string()),
            ("spring.version".to_string(), "${platform.version}".to_string()),
        ]);
        resolve_property_references(&mut props);
        assert_eq!(props["spring.version"], "5.3.27");
    }

    /// An unknown property survives as written, because that is the bug worth reporting.
    #[test]
    fn an_undefined_property_is_left_alone_rather_than_blanked() {
        let props = HashMap::new();
        assert_eq!(expand("${nope}", &props), "${nope}");
        assert_eq!(expand("1.0-${nope}", &props), "1.0-${nope}");
    }

    /// A cycle between two properties must terminate rather than hang the editor.
    #[test]
    fn two_properties_that_name_each_other_terminate() {
        let mut props = HashMap::from([
            ("a".to_string(), "${b}".to_string()),
            ("b".to_string(), "${a}".to_string()),
        ]);
        resolve_property_references(&mut props);
        assert!(props["a"].contains("${"), "left unresolved rather than looping");
    }
}
