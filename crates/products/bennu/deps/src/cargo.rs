//! What a **Cargo** workspace depends on — the same [`Report`] the Maven side produces.
//!
//! One panel, two ecosystems: see [`crate::model`] for how the fields line up. What is here is the
//! Cargo-specific half of filling them in, and it comes down to three questions the manifests alone
//! cannot answer:
//!
//! 1. **which version am I really getting?** A manifest says `serde = "1"`. `Cargo.lock` says
//!    `1.0.219`. The lock is the answer, and it is what a dependency panel exists to show.
//! 2. **is it actually here?** A locked version has an unpacked source directory under
//!    `$CARGO_HOME/registry/src`, and its absence is exactly the shape of a project that will not
//!    build offline.
//! 3. **what came in behind it?** Everything in the lock that no crate of this workspace declares
//!    directly — the Cargo analogue of the jars nobody asked for.
//!
//! ## Nothing is executed
//!
//! No `cargo metadata`, no `cargo tree`, no network. Both of the first two would be seconds on a
//! cold workspace, and a panel that costs that much to open is one nobody opens twice. The
//! consequence is stated rather than hidden: the *shape* of the graph below is the manifests', and
//! the versions are the lockfile's. Which crate pulled in a given transitive dependency is a
//! question only a real resolve answers, so it is not claimed.
//!
//! ## Requirement matching is approximate, on purpose
//!
//! Choosing between two locked versions of one crate means evaluating a semver requirement, and a
//! semver parser is a dependency and a maintenance surface for the sake of one column. So
//! [`pick_locked`] takes the single locked version when there is one, and otherwise prefers the one
//! whose numbers start the way the requirement does. When it cannot tell, it says nothing — the
//! requirement stays on screen as written, which is honest, rather than a version that might be
//! wrong.

use std::path::Path;

use bennu_cargo::prelude::{
    declared, read_workspace, registry_dirs, CargoCrate, DeclaredDep, Manifest,
};

use crate::model::{Dependency, Module, Origin, Report, Site, Transitive};

/// Read a Cargo workspace's dependencies.
///
/// Never fails: a root with no manifest yields an empty report, which is what an editor opened on
/// the wrong directory should show.
pub fn read(root: &Path) -> Report {
    let ws = read_workspace(root);
    let lock = Lock::read(root);
    let sources = SourceIndex::discover();

    let mut report = Report {
        ecosystem: "cargo".to_string(),
        resolved_known: lock.present,
        unreadable: ws.unreadable.clone(),
        ..Report::default()
    };

    // The workspace root's `[workspace.dependencies]` — what a `workspace = true` inherits, and the
    // answer to "so which version". Read once for the whole report.
    let root_inherited = std::fs::read_to_string(root.join("Cargo.toml"))
        .map(|text| declared(&Manifest::parse(&text)))
        .unwrap_or_default();
    let root_name = ws.name.clone();

    for c in &ws.crates {
        let Ok(text) = std::fs::read_to_string(&c.manifest) else {
            report.unreadable.push(c.manifest.clone());
            continue;
        };
        let manifest = Manifest::parse(&text);
        let dependencies = declared(&manifest)
            .into_iter()
            // A `[workspace.dependencies]` entry in the ROOT manifest is not a dependency of the
            // root crate — it is the workspace's version table, and listing it as one would show
            // every shared version twice.
            .filter(|d| d.table != "workspace.dependencies")
            .map(|d| convert(d, c, &root_inherited, &root_name, &lock, &sources))
            .collect();
        report.modules.push(Module {
            name: c.name.clone(),
            id: c.name.clone(),
            manifest: c.manifest.clone(),
            kind: crate_kind(c),
            dependencies,
        });
    }

    report.transitive = lock.unclaimed(&report, &sources);
    report
}

/// What the crate builds, as the Cargo answer to Maven's `<packaging>`.
fn crate_kind(c: &CargoCrate) -> String {
    let has_lib = c.targets.iter().any(|t| t.kind == "lib");
    let has_bin = c.targets.iter().any(|t| t.kind == "bin");
    let proc_macro = c.targets.iter().any(|t| t.proc_macro);
    match (proc_macro, has_lib, has_bin) {
        (true, _, _) => "proc-macro".to_string(),
        (_, true, true) => "lib+bin".to_string(),
        (_, true, false) => "lib".to_string(),
        (_, false, true) => "bin".to_string(),
        // No targets at all — a crate whose sources are not there yet, or one that only holds
        // tests. Saying nothing beats claiming it is a library.
        _ => String::new(),
    }
}

/// One declared dependency, with everything resolvable resolved.
fn convert(
    d: DeclaredDep,
    owner: &CargoCrate,
    root_inherited: &[DeclaredDep],
    root_name: &str,
    lock: &Lock,
    sources: &SourceIndex,
) -> Dependency {
    // A `workspace = true` dependency has no version of its own; the root's table has it. Reported
    // as `Managed` for the same reason a Maven `<dependencyManagement>` entry is: the module asked
    // for the dependency and something further up chose the version, and "which" is the question.
    let (mut req, origin) = if d.from_workspace {
        let from_root = root_inherited.iter().find(|r| r.name == d.name);
        (
            from_root.map(|r| r.req.clone()).unwrap_or_default(),
            Origin::Managed { from: root_name.to_string() },
        )
    } else {
        (d.req.clone(), Origin::Declared)
    };

    // The lockfile's answer beats the requirement — `serde = "1"` is not what you are compiling
    // against, `1.0.219` is.
    let locked = lock.pick(&d.package, &req);
    if let Some(v) = &locked {
        req = v.clone();
    }

    // Where it actually is. A path dependency resolves against the crate's own directory; a
    // registry one against the unpacked source. A git dependency's checkout is keyed by a hash of
    // the URL that we would have to reproduce, so it is left unresolved rather than guessed at.
    let resolved = if !d.path.is_empty() {
        let dir = Path::new(&owner.manifest).parent().map(|p| p.join(&d.path));
        dir.filter(|p| p.join("Cargo.toml").is_file())
            .map(|p| slash(&p))
            .unwrap_or_default()
    } else if d.git.is_empty() {
        locked.as_deref().and_then(|v| sources.find(&d.package, v)).unwrap_or_default()
    } else {
        String::new()
    };

    Dependency {
        // Cargo has no groupId; where the crate comes from is provenance, and lives in `source`.
        group: String::new(),
        source: d.source().to_string(),
        name: d.name.clone(),
        version: req,
        scope: d.kind.as_str().to_string(),
        kind: String::new(),
        // A renamed dependency: the row is titled by the local name, and this is what it really is.
        variant: if d.is_renamed() { d.package.clone() } else { String::new() },
        optional: d.optional,
        origin,
        condition: d.target.clone(),
        features: d.features.clone(),
        declared_in: Site { file: owner.manifest.clone(), offset: d.offset, line: d.line },
        resolved,
    }
}

// ── Cargo.lock ─────────────────────────────────────────────────────────────────

/// The `[[package]]` entries of a workspace's `Cargo.lock`.
struct Lock {
    /// `(name, version)`, in lockfile order.
    packages: Vec<(String, String)>,
    /// Whether the file was there at all — the difference between "not resolved" and "unknown".
    present: bool,
}

impl Lock {
    fn read(root: &Path) -> Lock {
        let Ok(text) = std::fs::read_to_string(root.join("Cargo.lock")) else {
            return Lock { packages: Vec::new(), present: false };
        };
        let m = Manifest::parse(&text);
        let mut packages = Vec::new();
        for element in m.array_elements("package") {
            let name = element.iter().find(|e| e.key == "name").and_then(|e| e.str_value());
            let version = element.iter().find(|e| e.key == "version").and_then(|e| e.str_value());
            if let (Some(n), Some(v)) = (name, version) {
                packages.push((n.to_string(), v.to_string()));
            }
        }
        Lock { packages, present: true }
    }

    /// The locked version of `name` that best answers the requirement `req`.
    fn pick(&self, name: &str, req: &str) -> Option<String> {
        let found: Vec<&str> =
            self.packages.iter().filter(|(n, _)| n == name).map(|(_, v)| v.as_str()).collect();
        pick_locked(&found, req).map(str::to_string)
    }

    /// Every locked package no module of `report` declares — what the declared dependencies dragged
    /// in.
    ///
    /// The workspace's own crates are excluded: a member appearing in its own dependency panel as
    /// something that came in transitively would be nonsense.
    fn unclaimed(&self, report: &Report, sources: &SourceIndex) -> Vec<Transitive> {
        let declared_names: Vec<&str> = report
            .modules
            .iter()
            .flat_map(|m| &m.dependencies)
            // The real crate name, which is what the lock knows it by — a renamed dependency is
            // locked under `serde_json`, not under `json`.
            .map(|d| if d.variant.is_empty() { d.name.as_str() } else { d.variant.as_str() })
            .collect();
        let members: Vec<&str> = report.modules.iter().map(|m| m.id.as_str()).collect();

        let mut out: Vec<Transitive> = self
            .packages
            .iter()
            .filter(|(name, _)| {
                !declared_names.contains(&name.as_str()) && !members.contains(&name.as_str())
            })
            .map(|(name, version)| Transitive {
                group: String::new(),
                name: name.clone(),
                version: version.clone(),
                resolved: sources.find(name, version).unwrap_or_default(),
            })
            .collect();
        out.sort_by(|a, b| (a.name.as_str(), a.version.as_str()).cmp(&(&b.name, &b.version)));
        out.dedup();
        out
    }
}

/// Choose among the locked versions of one crate.
///
/// - one version → that one, whatever the requirement says (it is what is being compiled);
/// - several → the one whose numbers start the way the requirement's do;
/// - none, or no way to tell → `None`, and the requirement stays on screen as written.
///
/// Deliberately not a semver implementation: see the module doc.
fn pick_locked<'a>(versions: &[&'a str], req: &str) -> Option<&'a str> {
    match versions {
        [] => None,
        [only] => Some(only),
        many => {
            // The numeric head of the requirement: `^1.2` → `1.2`, `>=1, <3` → `1`, `*` → nothing.
            let head: String = req
                .trim()
                .trim_start_matches(['^', '~', '=', '>', '<', ' '])
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            let head = head.trim_end_matches('.');
            if head.is_empty() {
                return None;
            }
            many.iter()
                .copied()
                .filter(|v| *v == head || v.starts_with(&format!("{head}.")))
                // The highest match, so a `^1` against 1.2 and 1.9 picks 1.9.
                .max_by_key(|v| numeric_key(v))
        }
    }
}

/// A version as comparable numbers, for picking the highest match.
fn numeric_key(v: &str) -> (u64, u64, u64) {
    let core = v.split(['-', '+']).next().unwrap_or(v);
    let mut parts = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

// ── the local registry ─────────────────────────────────────────────────────────

/// The unpacked crate sources under `$CARGO_HOME/registry/src/*`.
///
/// One directory listing per registry, done once per report rather than a `stat` per dependency:
/// the directory holds thousands of entries on a developer machine, and this is a panel that opens
/// on a click.
struct SourceIndex {
    /// `<name>-<version>` → absolute path.
    entries: Vec<(String, String)>,
}

impl SourceIndex {
    fn discover() -> SourceIndex {
        let mut entries = Vec::new();
        for dir in registry_dirs("src") {
            let Ok(read) = std::fs::read_dir(&dir) else { continue };
            for entry in read.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    entries.push((name.to_string(), slash(&entry.path())));
                }
            }
        }
        SourceIndex { entries }
    }

    /// The unpacked source of `name` at `version`, when it is there.
    fn find(&self, name: &str, version: &str) -> Option<String> {
        let key = format!("{name}-{version}");
        self.entries.iter().find(|(dir, _)| *dir == key).map(|(_, path)| path.clone())
    }
}

/// An absolute path, forward-slashed — the convention on the bennu wire.
fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A scratch workspace on disk, removed on drop.
    struct Fixture(PathBuf);

    impl Fixture {
        fn new(tag: &str) -> Fixture {
            let dir = std::env::temp_dir().join(format!(
                "bennu-deps-cargo-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Fixture(dir)
        }

        fn write(&self, rel: &str, text: &str) -> &Fixture {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
            self
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn dep<'a>(report: &'a Report, module: &str, name: &str) -> &'a Dependency {
        report
            .modules
            .iter()
            .find(|m| m.id == module)
            .unwrap_or_else(|| panic!("no module {module}"))
            .dependencies
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("no dependency {name} in {module}"))
    }

    fn workspace(tag: &str) -> Fixture {
        let f = Fixture::new(tag);
        f.write(
            "Cargo.toml",
            "\
[workspace]
members = [\"app\", \"lib\"]

[workspace.dependencies]
serde = \"1\"
",
        );
        f.write(
            "app/Cargo.toml",
            "\
[package]
name = \"app\"
version = \"0.1.0\"

[dependencies]
serde = { workspace = true, features = [\"derive\"] }
anyhow = \"1.0\"
lib = { path = \"../lib\" }
json = { package = \"serde_json\", version = \"1\" }
gone = { path = \"../gone\" }
remote = { git = \"https://example.invalid/x\" }

[dev-dependencies]
tempfile = \"3\"

[target.'cfg(unix)'.dependencies]
libc = \"0.2\"
",
        );
        f.write("app/src/main.rs", "fn main() {}");
        f.write("lib/Cargo.toml", "[package]\nname = \"lib\"\nversion = \"0.1.0\"\n");
        f.write("lib/src/lib.rs", "");
        f
    }

    #[test]
    fn every_crate_becomes_a_module_with_what_it_builds() {
        let f = workspace("modules");
        let report = read(&f.0);
        assert_eq!(report.ecosystem, "cargo");
        let ids: Vec<&str> = report.modules.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["app", "lib"]);
        // The Cargo answer to `<packaging>`.
        assert_eq!(report.modules[0].kind, "bin");
        assert_eq!(report.modules[1].kind, "lib");
    }

    #[test]
    fn the_kind_of_a_dependency_is_the_table_it_came_from() {
        let f = workspace("scopes");
        let report = read(&f.0);
        assert_eq!(dep(&report, "app", "anyhow").scope, "normal");
        assert_eq!(dep(&report, "app", "tempfile").scope, "dev");
        // A target table's `cfg` is a condition, the Cargo analogue of a Maven profile.
        assert_eq!(dep(&report, "app", "libc").condition, "cfg(unix)");
        assert_eq!(dep(&report, "app", "anyhow").condition, "");
    }

    /// The question the panel exists for: `serde = { workspace = true }` shows no version, and the
    /// answer is in the root manifest.
    #[test]
    fn a_workspace_inherited_version_is_resolved_and_attributed() {
        let f = workspace("inherit");
        let report = read(&f.0);
        let serde = dep(&report, "app", "serde");
        assert_eq!(serde.version, "1", "the root's [workspace.dependencies] answers it");
        assert!(matches!(serde.origin, Origin::Managed { .. }), "{:?}", serde.origin);
        assert_eq!(serde.source, "workspace");
        assert_eq!(serde.features, vec!["derive"]);
    }

    #[test]
    fn the_source_is_named_and_a_path_dependency_is_resolved_against_disk() {
        let f = workspace("sources");
        let report = read(&f.0);
        let lib = dep(&report, "app", "lib");
        assert_eq!(lib.source, "path");
        assert!(lib.resolved.ends_with("/lib"), "{}", lib.resolved);
        // A path pointing nowhere is left unresolved rather than reported as present.
        assert!(dep(&report, "app", "gone").resolved.is_empty());
        // A git checkout is keyed by a hash of the URL — not guessed at.
        assert_eq!(dep(&report, "app", "remote").source, "git");
        assert!(dep(&report, "app", "remote").resolved.is_empty());
    }

    #[test]
    fn a_renamed_dependency_shows_both_names() {
        let f = workspace("rename");
        let report = read(&f.0);
        let json = dep(&report, "app", "json");
        assert_eq!(json.name, "json", "the row is titled by the local name");
        assert_eq!(json.variant, "serde_json", "and says what it really is");
    }

    /// The root's `[workspace.dependencies]` is a version table, not the root crate's dependency
    /// list — listing it as one would show every shared version twice.
    #[test]
    fn the_workspace_version_table_is_not_a_dependency_list() {
        let f = Fixture::new("wsdeps");
        f.write(
            "Cargo.toml",
            "[package]\nname = \"root\"\nversion = \"0.1.0\"\n[workspace]\nmembers = []\n[workspace.dependencies]\nserde = \"1\"\n[dependencies]\nanyhow = \"1\"\n",
        );
        let report = read(&f.0);
        let names: Vec<&str> =
            report.modules[0].dependencies.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["anyhow"]);
    }

    #[test]
    fn without_a_lockfile_nothing_is_claimed_about_resolution() {
        let f = workspace("nolock");
        let report = read(&f.0);
        assert!(!report.resolved_known);
        assert_eq!(report.unresolved_count(), 0, "unknown is not missing");
        assert!(report.transitive.is_empty());
    }

    #[test]
    fn the_lockfile_supplies_the_version_actually_compiled_and_the_transitive_tail() {
        let f = workspace("lock");
        f.write(
            "Cargo.lock",
            "\
version = 3

[[package]]
name = \"app\"
version = \"0.1.0\"

[[package]]
name = \"lib\"
version = \"0.1.0\"

[[package]]
name = \"serde\"
version = \"1.0.219\"

[[package]]
name = \"serde_derive\"
version = \"1.0.219\"

[[package]]
name = \"anyhow\"
version = \"1.0.95\"

[[package]]
name = \"serde_json\"
version = \"1.0.140\"
",
        );
        let report = read(&f.0);
        assert!(report.resolved_known);
        // `serde = "1"` is not what is compiled; `1.0.219` is.
        assert_eq!(dep(&report, "app", "serde").version, "1.0.219");
        assert_eq!(dep(&report, "app", "anyhow").version, "1.0.95");
        // A renamed dependency is locked under its REAL name.
        assert_eq!(dep(&report, "app", "json").version, "1.0.140");

        // The tail: in the lock, declared by nobody. The workspace's own crates are not in it, and
        // neither is anything declared directly.
        let tail: Vec<&str> = report.transitive.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(tail, vec!["serde_derive"]);
    }

    /// A dependency the lock has no entry for keeps its requirement on screen rather than showing
    /// a blank — the manifest still said something.
    #[test]
    fn an_unlocked_dependency_keeps_its_requirement() {
        let f = workspace("partial");
        f.write("Cargo.lock", "version = 3\n\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\n");
        let report = read(&f.0);
        assert_eq!(dep(&report, "app", "anyhow").version, "1.0");
    }

    #[test]
    fn choosing_among_several_locked_versions() {
        // One version wins whatever the requirement says: it is what is being compiled.
        assert_eq!(pick_locked(&["1.0.5"], "^2"), Some("1.0.5"));
        // Several: the highest that matches the requirement's numeric head.
        assert_eq!(pick_locked(&["1.2.0", "1.9.3", "2.0.0"], "^1"), Some("1.9.3"));
        assert_eq!(pick_locked(&["1.2.0", "1.9.3", "2.0.0"], "2"), Some("2.0.0"));
        assert_eq!(pick_locked(&["1.2.0", "1.9.3"], "=1.2.0"), Some("1.2.0"));
        // Nothing to go on — the requirement stays as written rather than a version that might be
        // wrong.
        assert_eq!(pick_locked(&["1.0.0", "2.0.0"], "*"), None);
        assert_eq!(pick_locked(&["1.0.0", "2.0.0"], "3"), None);
        assert_eq!(pick_locked(&[], "1"), None);
    }

    #[test]
    fn a_root_with_no_manifest_yields_an_empty_report() {
        let f = Fixture::new("bare");
        let report = read(&f.0);
        assert_eq!(report.ecosystem, "cargo");
        assert!(report.modules.is_empty());
        assert!(!report.resolved_known);
    }
}
