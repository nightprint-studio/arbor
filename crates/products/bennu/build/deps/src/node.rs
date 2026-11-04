//! What a **Node** project depends on — the same [`Report`] the Maven and Cargo sides produce.
//!
//! The three questions every producer answers, for `package.json`:
//!
//! 1. **which version am I really getting?** The manifest says `^4.17.0`; the lockfile (npm, pnpm
//!    or Yarn — see [`bennu_npm::lock`]) says `4.17.21`.
//! 2. **is it installed?** A `node_modules/<name>` folder, found the way Node finds it: next to the
//!    manifest, then in each folder above up to the project root.
//! 3. **what came in behind it?** Every package in the lockfile that no manifest declares.
//!
//! A **workspace** (npm/Yarn `workspaces`, pnpm's `pnpm-workspace.yaml`) is one report with one
//! module per member, over the one lockfile at the root.
//!
//! ## Nothing is executed
//!
//! No `npm ls`, no network. The lockfile is the resolved graph; [`tree`] reads "who requires this"
//! straight out of it. A project with no lockfile still lists what it declares — as the ranges
//! written, which is what is known.
//!
//! Dependency vocabulary, for the shared model: [`Dependency::scope`] is `runtime` · `dev` · `peer` ·
//! `optional` (the `package.json` section), [`Dependency::source`] is where the package comes from
//! (`npm`, `workspace`, `git`, `file`, `url`, `alias`, `catalog`), and [`Dependency::variant`] is the
//! real package behind an `npm:` alias.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use bennu_npm::prelude::{
    parse as parse_manifest, read_lock, Dependency as DeclaredDep, DependencyKind, Lock, PackageManifest,
};

use serde::Serialize;

use crate::model::{Dependency, Module, Origin, Report, Site, Transitive};
use crate::tree::{ModuleTree, TreeNode};

/// One `package.json` of the project.
struct Member {
    /// Relative to the root, forward slashes; empty for the root.
    dir: String,
    manifest: PathBuf,
    id: String,
    parsed: PackageManifest,
}

/// Everything read from disk, once, for both the report and the tree.
struct Project {
    root: PathBuf,
    members: Vec<Member>,
    lock: Option<Lock>,
    unreadable: Vec<String>,
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn load(root: &Path) -> Project {
    let (members, unreadable) = load_members(root);
    Project { root: root.to_path_buf(), members, lock: read_lock(root), unreadable }
}

/// The root `package.json` and every workspace member, without touching the lockfile.
fn load_members(root: &Path) -> (Vec<Member>, Vec<String>) {
    let mut dirs = vec![String::new()];
    dirs.extend(bennu_npm::prelude::workspace_members(root));

    let mut members = Vec::new();
    let mut unreadable = Vec::new();
    for dir in dirs {
        let manifest = if dir.is_empty() { root.join("package.json") } else { root.join(&dir).join("package.json") };
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            // The root's absence is the "not a Node project" case; a member's is worth saying.
            if !dir.is_empty() {
                unreadable.push(slashed(&manifest));
            }
            continue;
        };
        let parsed = parse_manifest(&text);
        let id = parsed.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| folder_name(root, &dir));
        members.push(Member { dir, manifest, id, parsed });
    }
    (members, unreadable)
}

/// A manifest with no `name` is still a package: the folder it lives in is what people call it.
fn folder_name(root: &Path, dir: &str) -> String {
    let path = if dir.is_empty() { root.to_path_buf() } else { root.join(dir) };
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "package".to_string())
}

/// Read a Node project's dependencies. Never fails: a root with no manifest yields an empty report.
pub fn read(root: &Path) -> Report {
    let project = load(root);
    let installed_anywhere = root.join("node_modules").is_dir();
    let mut report = Report {
        ecosystem: "node".to_string(),
        resolved_known: installed_anywhere || project.lock.is_some(),
        unreadable: project.unreadable.clone(),
        ..Report::default()
    };

    let is_workspace = project.members.len() > 1;
    let mut direct: HashSet<usize> = HashSet::new();
    for member in &project.members {
        // A workspace root that only lists its members declares nothing worth a group of its own.
        if member.dir.is_empty() && is_workspace && member.parsed.dependencies.is_empty() {
            continue;
        }
        let dependencies = member
            .parsed
            .dependencies
            .iter()
            .map(|d| dependency_of(&project, member, d, installed_anywhere, &mut direct))
            .collect();
        report.modules.push(Module {
            name: member.id.clone(),
            id: member.id.clone(),
            manifest: slashed(&member.manifest),
            kind: if member.dir.is_empty() && is_workspace { "workspace".into() } else { "package".into() },
            dependencies,
        });
    }
    report.transitive = project.lock.as_ref().map(|lock| transitive_of(lock, &direct)).unwrap_or_default();
    report
}

fn dependency_of(
    project: &Project,
    member: &Member,
    d: &DeclaredDep,
    installed_anywhere: bool,
    direct: &mut HashSet<usize>,
) -> Dependency {
    let source = source_of(&d.range);
    let locked = project.lock.as_ref().and_then(|lock| {
        let idx = lock.direct_index(&member.dir, &d.name, &d.range)?;
        Some((idx, &lock.packages[idx]))
    });
    if let Some((idx, _)) = locked {
        direct.insert(idx);
    }
    let version = match locked {
        Some((_, pkg)) if !pkg.local => pkg.version.clone(),
        // A sibling package: its own version is the answer, wherever the lockfile keeps it.
        _ if source == "workspace" || locked.is_some_and(|(_, p)| p.local) => sibling_version(project, &d.name).unwrap_or_else(|| d.range.clone()),
        _ => d.range.clone(),
    };
    Dependency {
        name: d.name.clone(),
        version,
        scope: scope_of(d.kind).to_string(),
        source: source.to_string(),
        variant: alias_target(&d.range).unwrap_or_default(),
        optional: d.kind == DependencyKind::Optional,
        origin: match source {
            "workspace" => Origin::Managed { from: "workspace".into() },
            "catalog" => Origin::Managed { from: "catalog".into() },
            _ => Origin::Declared,
        },
        declared_in: Site { file: slashed(&member.manifest), offset: d.offset, line: d.line as u32 },
        resolved: if installed_anywhere { installed_dir(&project.root, &member.dir, &d.name) } else { String::new() },
        ..Dependency::default()
    }
}

fn scope_of(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Runtime => "runtime",
        DependencyKind::Dev => "dev",
        DependencyKind::Peer => "peer",
        DependencyKind::Optional => "optional",
    }
}

fn sibling_version(project: &Project, name: &str) -> Option<String> {
    project.members.iter().find(|m| m.id == name).and_then(|m| m.parsed.version.clone())
}

/// Where a declared range says the package comes from. The registry is the default and the only
/// one that needs no word.
pub fn source_of(range: &str) -> &'static str {
    let r = range.trim();
    if r.starts_with("workspace:") {
        "workspace"
    } else if r.starts_with("catalog:") {
        "catalog"
    } else if r.starts_with("npm:") {
        "alias"
    } else if r.starts_with("file:") || r.starts_with("link:") || r.starts_with("portal:") {
        "file"
    } else if r.starts_with("http://") || r.starts_with("https://") {
        "url"
    } else if r.starts_with("git") || r.starts_with("github:") || r.starts_with("gitlab:") || r.starts_with("bitbucket:") {
        "git"
    } else {
        "npm"
    }
}

/// `npm:string-width@^4` → `string-width`.
fn alias_target(range: &str) -> Option<String> {
    let spec = range.trim().strip_prefix("npm:")?;
    let at = spec.rfind('@').filter(|&i| i > 0);
    Some(at.map_or(spec, |i| &spec[..i]).to_string())
}

/// `node_modules/<name>` as Node finds it from `dir`: there, then in every folder up to the root.
fn installed_dir(root: &Path, dir: &str, name: &str) -> String {
    let mut at = if dir.is_empty() { root.to_path_buf() } else { root.join(dir) };
    loop {
        let candidate = at.join("node_modules").join(name);
        if candidate.is_dir() {
            return slashed(&candidate);
        }
        if at == root || !at.pop() {
            return String::new();
        }
    }
}

/// Every package reachable from the declared ones that no manifest declares itself.
fn transitive_of(lock: &Lock, direct: &HashSet<usize>) -> Vec<Transitive> {
    let mut seen: HashSet<usize> = HashSet::new();
    let mut stack: Vec<usize> = direct.iter().copied().collect();
    while let Some(i) = stack.pop() {
        if !seen.insert(i) || lock.packages[i].local {
            continue;
        }
        stack.extend(lock.packages[i].deps.iter().map(|e| e.to));
    }
    let mut out: BTreeSet<(String, String)> = BTreeSet::new();
    for i in seen {
        let p = &lock.packages[i];
        if !direct.contains(&i) && !p.local {
            out.insert((p.name.clone(), p.version.clone()));
        }
    }
    out.into_iter().map(|(name, version)| Transitive { name, version, ..Transitive::default() }).collect()
}

// ── The packages themselves, for the tool window ─────────────────────────────

/// One `scripts` entry.
#[derive(Debug, Clone, Serialize)]
pub struct NodeScript {
    pub name: String,
    pub command: String,
    pub line: u32,
}

/// One `package.json` of the project: the root, then each workspace member.
#[derive(Debug, Clone, Serialize)]
pub struct NodePackage {
    /// The package name (the folder name when the manifest has none).
    pub id: String,
    /// Relative to the root, forward slashes; empty for the root.
    pub dir: String,
    /// Absolute path of its `package.json`, forward-slashed.
    pub manifest: String,
    pub version: String,
    pub scripts: Vec<NodeScript>,
}

/// The project's packages and their scripts. Reads manifests only — not the lockfile.
pub fn packages(root: &Path) -> Vec<NodePackage> {
    load_members(root)
        .0
        .into_iter()
        .map(|m| NodePackage {
            id: m.id,
            dir: m.dir,
            manifest: slashed(&m.manifest),
            version: m.parsed.version.unwrap_or_default(),
            scripts: m
                .parsed
                .scripts
                .into_iter()
                .map(|s| NodeScript { name: s.name, command: s.command, line: s.line as u32 })
                .collect(),
        })
        .collect()
}

// ── The tree, from the lockfile ──────────────────────────────────────────────

/// The resolved tree of each module, read out of the lockfile: who requires what, and every version
/// of a package the project ends up with. `Err` says what is missing, because an empty tree would
/// read as "no dependencies".
pub fn tree(root: &Path) -> Result<Vec<ModuleTree>, String> {
    let project = load(root);
    if project.members.is_empty() {
        return Err("There is no package.json at the project root.".into());
    }
    let Some(lock) = &project.lock else {
        return Err("There is no lockfile (package-lock.json, pnpm-lock.yaml or yarn.lock) yet, so there is no resolved \
                    graph to read. Install the dependencies once — `npm install`, `pnpm install` or `yarn` — and resolve again."
            .into());
    };
    let is_workspace = project.members.len() > 1;
    let mut out = Vec::new();
    for member in &project.members {
        if member.dir.is_empty() && is_workspace && member.parsed.dependencies.is_empty() {
            continue;
        }
        let mut walk = Walk { lock, nodes: Vec::new(), path: Vec::new(), expanded: HashSet::new() };
        for d in &member.parsed.dependencies {
            let Some(idx) = lock.direct_index(&member.dir, &d.name, &d.range) else { continue };
            walk.visit(idx, 1, None, scope_of(d.kind), d.kind == DependencyKind::Optional);
        }
        out.push(ModuleTree { module: member.id.clone(), version: member.parsed.version.clone().unwrap_or_default(), nodes: walk.nodes });
    }
    Ok(out)
}

struct Walk<'a> {
    lock: &'a Lock,
    nodes: Vec<TreeNode>,
    /// The packages from the module down to the one being visited, for cycle detection.
    path: Vec<usize>,
    /// Packages whose dependencies were already listed under this module.
    expanded: HashSet<usize>,
}

impl Walk<'_> {
    fn visit(&mut self, idx: usize, depth: usize, parent: Option<usize>, scope: &str, optional: bool) {
        let pkg = &self.lock.packages[idx];
        let mut node = TreeNode {
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            scope: scope.to_string(),
            depth,
            parent,
            optional,
            ..TreeNode::default()
        };
        // Repeats of a package that has dependencies are named but not re-expanded (`duplicate`); a
        // leaf is cheap, and listing it under every requirer is what "who requires this" needs.
        let repeat = !pkg.deps.is_empty() && !self.expanded.insert(idx);
        if self.path.contains(&idx) {
            node.omitted = "cycle".into();
        } else if repeat {
            node.omitted = "duplicate".into();
        }
        let here = self.nodes.len();
        let descend = node.omitted.is_empty() && !pkg.local;
        self.nodes.push(node);
        if !descend {
            return;
        }
        self.path.push(idx);
        let edges: Vec<_> = self.lock.packages[idx].deps.clone();
        for e in edges {
            self.visit(e.to, depth + 1, Some(here), scope, e.optional);
        }
        self.path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bennu-deps-node-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (rel, body) in files {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        dir
    }

    const MANIFEST: &str = r#"{
  "name": "app",
  "version": "1.0.0",
  "dependencies": { "a": "^1.0.0", "shared": "workspace:*" },
  "devDependencies": { "d": "~2.0.0" },
  "optionalDependencies": { "alias": "npm:real@^3.0.0" }
}"#;

    const LOCK: &str = r#"{
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "app", "dependencies": { "a": "^1.0.0" }, "devDependencies": { "d": "~2.0.0" } },
    "node_modules/a": { "version": "1.4.0", "dependencies": { "b": "^2.0.0", "c": "^1.0.0" } },
    "node_modules/b": { "version": "2.1.0", "dependencies": { "c": "^2.0.0" } },
    "node_modules/c": { "version": "2.5.0" },
    "node_modules/a/node_modules/c": { "version": "1.0.0" },
    "node_modules/d": { "version": "2.0.5" }
  }
}"#;

    #[test]
    fn the_report_lists_what_is_declared_with_what_was_locked() {
        let dir = fixture("report", &[("package.json", MANIFEST), ("package-lock.json", LOCK), ("node_modules/a/package.json", "{}")]);
        let report = read(&dir);
        assert_eq!(report.ecosystem, "node");
        assert!(report.resolved_known);

        let deps = &report.modules[0].dependencies;
        let by_name = |n: &str| deps.iter().find(|d| d.name == n).unwrap();
        assert_eq!(by_name("a").version, "1.4.0");
        assert_eq!(by_name("a").scope, "runtime");
        assert_eq!(by_name("d").version, "2.0.5");
        assert_eq!(by_name("d").scope, "dev");
        assert!(!by_name("a").resolved.is_empty(), "node_modules/a exists");
        assert!(by_name("d").resolved.is_empty(), "node_modules/d does not");
        // Nothing in the lockfile answers these, so the range stays as written.
        assert_eq!(by_name("alias").version, "npm:real@^3.0.0");
        assert_eq!(by_name("alias").variant, "real");
        assert!(by_name("alias").optional);
        assert_eq!(by_name("shared").source, "workspace");
        assert!(matches!(by_name("shared").origin, Origin::Managed { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn transitive_is_what_no_manifest_declares() {
        let dir = fixture("transitive", &[("package.json", MANIFEST), ("package-lock.json", LOCK)]);
        let report = read(&dir);
        let names: Vec<(&str, &str)> = report.transitive.iter().map(|t| (t.name.as_str(), t.version.as_str())).collect();
        assert_eq!(names, [("b", "2.1.0"), ("c", "1.0.0"), ("c", "2.5.0")], "both copies of c, and not a or d");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tree_shows_who_requires_each_version() {
        let dir = fixture("tree", &[("package.json", MANIFEST), ("package-lock.json", LOCK)]);
        let trees = tree(&dir).unwrap();
        let nodes = &trees[0].nodes;
        let c_versions: Vec<&str> = nodes.iter().filter(|n| n.name == "c").map(|n| n.version.as_str()).collect();
        // Depth-first: c@2 under b comes before a's own c@1.
        assert_eq!(c_versions, ["2.5.0", "1.0.0"]);
        let c2 = nodes.iter().find(|n| n.name == "c" && n.version == "2.5.0").unwrap();
        assert_eq!(nodes[c2.parent.unwrap()].name, "b", "c@2 is there because of b");
        assert_eq!(c2.depth, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_repeated_package_with_dependencies_is_not_expanded_twice() {
        let lock = r#"{ "lockfileVersion": 3, "packages": {
          "": { "dependencies": { "x": "1", "y": "1" } },
          "node_modules/x": { "version": "1.0.0", "dependencies": { "shared": "1" } },
          "node_modules/y": { "version": "1.0.0", "dependencies": { "shared": "1" } },
          "node_modules/shared": { "version": "1.0.0", "dependencies": { "leaf": "1" } },
          "node_modules/leaf": { "version": "1.0.0" }
        } }"#;
        let dir = fixture("dup", &[("package.json", r#"{ "name": "p", "dependencies": { "x": "1", "y": "1" } }"#), ("package-lock.json", lock)]);
        let nodes = &tree(&dir).unwrap()[0].nodes;
        let shared: Vec<&str> = nodes.iter().filter(|n| n.name == "shared").map(|n| n.omitted.as_str()).collect();
        assert_eq!(shared, ["", "duplicate"]);
        assert_eq!(nodes.iter().filter(|n| n.name == "leaf").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_lockfile_the_report_has_ranges_and_the_tree_says_why_it_cannot() {
        let dir = fixture("nolock", &[("package.json", MANIFEST)]);
        let report = read(&dir);
        assert!(!report.resolved_known);
        assert_eq!(report.modules[0].dependencies[0].version, "^1.0.0");
        assert!(tree(&dir).unwrap_err().contains("lockfile"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_workspace_is_one_module_per_member_over_one_lockfile() {
        let root = r#"{ "name": "mono", "private": true, "workspaces": ["packages/*"] }"#;
        let member = r#"{ "name": "@mono/ui", "version": "0.3.0", "dependencies": { "lib": "^3.0.0", "@mono/core": "*" } }"#;
        let core = r#"{ "name": "@mono/core", "version": "0.2.0" }"#;
        let lock = r#"{ "lockfileVersion": 3, "packages": {
          "": { "name": "mono", "workspaces": ["packages/*"] },
          "packages/ui": { "name": "@mono/ui", "version": "0.3.0", "dependencies": { "lib": "^3.0.0", "@mono/core": "*" } },
          "packages/core": { "name": "@mono/core", "version": "0.2.0" },
          "node_modules/lib": { "version": "3.4.5" },
          "node_modules/@mono/ui": { "resolved": "packages/ui", "link": true },
          "node_modules/@mono/core": { "resolved": "packages/core", "link": true }
        } }"#;
        let dir = fixture("ws", &[
            ("package.json", root),
            ("packages/ui/package.json", member),
            ("packages/core/package.json", core),
            ("package-lock.json", lock),
        ]);
        let report = read(&dir);
        let names: Vec<&str> = report.modules.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(names, ["@mono/core", "@mono/ui"], "the empty root is left out; members are sorted by folder");
        let ui = report.modules.iter().find(|m| m.id == "@mono/ui").unwrap();
        assert_eq!(ui.dependencies.iter().find(|d| d.name == "lib").unwrap().version, "3.4.5");
        assert_eq!(ui.dependencies.iter().find(|d| d.name == "@mono/core").unwrap().version, "0.2.0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sources_are_told_apart_by_the_range() {
        for (range, source) in [
            ("^1.2.3", "npm"), ("workspace:*", "workspace"), ("catalog:", "catalog"), ("npm:x@1", "alias"),
            ("file:../x", "file"), ("link:../x", "file"), ("https://x/y.tgz", "url"),
            ("git+ssh://git@h/r.git", "git"), ("github:o/r", "git"),
        ] {
            assert_eq!(source_of(range), source, "{range}");
        }
    }
}
