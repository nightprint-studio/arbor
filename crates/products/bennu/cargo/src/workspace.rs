//! The crate graph — what the Rust tool window shows.
//!
//! ## Read, not asked
//!
//! `cargo metadata` would answer all of this authoritatively, and it is the wrong tool for a panel:
//! on a cold workspace it takes seconds, it wants the network for a manifest it has not resolved
//! before, and it fails outright on a manifest that does not parse — which is exactly when you most
//! want to see the workspace. So the graph is read from the manifests and from the filesystem, and
//! the panel opens instantly on a project that has never been built.
//!
//! The trade is stated rather than hidden: nothing here is *resolved*. Feature unification across
//! the workspace, which dependency versions Cargo actually picked, and target-specific graphs are
//! Cargo's answers. What this gives is what is written down, plus what is on disk.
//!
//! ## Targets are half declared and half discovered
//!
//! A crate's binaries are usually not in its manifest at all — `src/main.rs` and `src/bin/*.rs` are
//! auto-discovered by Cargo, and a panel that only listed `[[bin]]` blocks would show nothing for
//! most crates. So [`read`] does both: the declared targets, then the conventional paths, honouring
//! the `autobins` / `autoexamples` / `autotests` / `autobenches` switches that turn discovery off.
//!
//! ## Orphans
//!
//! [`CargoWorkspace::orphans`] is a crate directory under the root that no `members` pattern
//! covers. It earns its place because the failure is silent: the crate compiles when you build it
//! directly, is invisible to `cargo build --workspace`, and the mistake survives for weeks.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::deps::{declared, DepKind};
use crate::manifest::Manifest;

/// Directories never walked when looking for crates.
const SKIP_DIRS: &[&str] = &["target", ".git", "node_modules", ".idea", ".arbor", ".vscode"];

/// How deep the orphan walk goes below the workspace root.
const MAX_DEPTH: usize = 6;

/// A cap on the crates a workspace is read as, against a pathological tree.
const MAX_CRATES: usize = 500;

/// One thing a crate builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoTarget {
    /// The target's name — what `--bin <name>` takes.
    pub name: String,
    /// `lib` · `bin` · `example` · `test` · `bench`.
    pub kind: String,
    /// Source file, relative to the crate's directory and forward-slashed.
    pub path: String,
    /// Whether the manifest declares it, as opposed to Cargo discovering it by convention.
    ///
    /// Worth showing: a declared target has settings you can go and edit, and a discovered one has
    /// no manifest entry to open.
    pub declared: bool,
    /// A `[lib]` that is a procedural macro — it is compiled for the host, not the target, which
    /// is the kind of thing worth seeing without opening the manifest.
    pub proc_macro: bool,
    /// `required-features` — the target does not build unless these are on.
    pub required_features: Vec<String>,
}

/// One feature of a crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoFeature {
    pub name: String,
    /// What turning it on turns on, verbatim (`dep:serde`, `serde/derive`, another feature).
    pub enables: Vec<String>,
    /// Whether `default` enables it — directly or through another default feature.
    pub default: bool,
}

/// One crate of the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoCrate {
    pub name: String,
    /// `[package] version`, or `inherited` when it comes from the workspace.
    pub version: String,
    /// Path relative to the workspace root, forward-slashed. Empty for the root crate itself.
    pub rel_path: String,
    /// Absolute path of the crate's `Cargo.toml`, forward-slashed.
    pub manifest: String,
    pub edition: String,
    pub description: String,
    /// Whether this is the root manifest's own `[package]`.
    pub is_root: bool,
    /// `false` when the manifest says `publish = false`.
    pub publish: bool,
    pub targets: Vec<CargoTarget>,
    pub features: Vec<CargoFeature>,
    /// Dependency counts by kind, so a panel can badge a crate without carrying every row.
    pub deps: usize,
    pub dev_deps: usize,
    pub build_deps: usize,
}

impl CargoCrate {
    /// The targets of one kind.
    pub fn targets_of(&self, kind: &str) -> Vec<&CargoTarget> {
        self.targets.iter().filter(|t| t.kind == kind).collect()
    }

    /// The binary a bare `cargo run -p <this>` would launch, when there is exactly one.
    ///
    /// `None` for a library, and `None` for a crate with several binaries — which is a real
    /// ambiguity Cargo itself refuses to resolve without `--bin`, so guessing would produce a run
    /// button that launches the wrong program.
    pub fn sole_binary(&self) -> Option<&CargoTarget> {
        let bins = self.targets_of("bin");
        (bins.len() == 1).then(|| bins[0])
    }

    /// Whether the crate builds anything runnable.
    pub fn is_runnable(&self) -> bool {
        self.targets.iter().any(|t| t.kind == "bin")
    }
}

/// A Cargo workspace, as its manifests describe it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoWorkspace {
    /// Absolute workspace root, forward-slashed.
    pub root: String,
    /// Display name: the root `[package] name`, else the root directory's name.
    pub name: String,
    /// `true` when the root manifest declares a `[workspace]` and no `[package]` — so the root
    /// itself compiles nothing and `--workspace` is the only thing a build at the root can mean.
    pub virtual_manifest: bool,
    /// Whether the root declares a `[workspace]` at all. A single-crate project is a workspace of
    /// one, which is worth saying rather than showing an empty panel.
    pub is_workspace: bool,
    /// `[workspace.package] edition`, or the root package's.
    pub edition: String,
    /// `[workspace] resolver`, empty when unset.
    pub resolver: String,
    /// The crates, root first then members in declaration order.
    pub crates: Vec<CargoCrate>,
    /// Manifests that were found but could not be read. Said out loud, because a crate missing
    /// from the list is otherwise indistinguishable from one that does not exist.
    pub unreadable: Vec<String>,
    /// Whether a `Cargo.lock` is next to the root manifest.
    pub locked: bool,
    /// Crate directories under the root that no `members` pattern covers, relative and
    /// forward-slashed. See the module doc for why these are worth surfacing.
    pub orphans: Vec<String>,
}

impl CargoWorkspace {
    /// The crate whose directory is `rel_path` (empty = the root crate).
    pub fn crate_at(&self, rel_path: &str) -> Option<&CargoCrate> {
        self.crates.iter().find(|c| c.rel_path == rel_path)
    }

    /// The crate called `name`.
    pub fn crate_named(&self, name: &str) -> Option<&CargoCrate> {
        self.crates.iter().find(|c| c.name == name)
    }

    /// Every binary in the workspace, as `(crate, target)`. What a run configuration picks from.
    pub fn binaries(&self) -> Vec<(&CargoCrate, &CargoTarget)> {
        self.crates
            .iter()
            .flat_map(|c| c.targets_of("bin").into_iter().map(move |t| (c, t)))
            .collect()
    }
}

/// Read the workspace rooted at `root`.
///
/// Never fails: a root with no manifest yields an empty workspace named after the directory, which
/// is what an editor opened on the wrong folder should show.
pub fn read(root: &Path) -> CargoWorkspace {
    let mut ws = CargoWorkspace {
        root: slash(root),
        name: dir_name(root),
        locked: root.join("Cargo.lock").is_file(),
        ..CargoWorkspace::default()
    };

    let Ok(root_text) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        return ws;
    };
    let root_manifest = Manifest::parse(&root_text);

    ws.is_workspace = root_manifest.has_table("workspace");
    ws.virtual_manifest = ws.is_workspace && !root_manifest.has_table("package");
    ws.resolver = root_manifest
        .str_of("workspace", "resolver")
        .or_else(|| root_manifest.str_of("package", "resolver"))
        .unwrap_or_default()
        .to_string();
    ws.edition = root_manifest
        .str_of("workspace.package", "edition")
        .or_else(|| root_manifest.str_of("package", "edition"))
        .unwrap_or_default()
        .to_string();

    // The root's own package, when it has one.
    if root_manifest.has_table("package") {
        if let Some(c) = crate_from(root, "", &root_manifest, true) {
            ws.name = c.name.clone();
            ws.crates.push(c);
        }
    }

    // The members, in declaration order with globs expanded.
    let members = expand_members(root, &root_manifest);
    for rel in &members {
        if ws.crates.len() >= MAX_CRATES {
            break;
        }
        let dir = root.join(rel);
        match std::fs::read_to_string(dir.join("Cargo.toml")) {
            Ok(text) => {
                let m = Manifest::parse(&text);
                if let Some(c) = crate_from(&dir, rel, &m, false) {
                    ws.crates.push(c);
                }
            }
            Err(_) => ws.unreadable.push(slash(&dir.join("Cargo.toml"))),
        }
    }

    if ws.is_workspace {
        ws.orphans = find_orphans(root, &root_manifest, &members);
    }
    ws
}

/// One crate, from its manifest and its directory.
fn crate_from(dir: &Path, rel: &str, m: &Manifest, is_root: bool) -> Option<CargoCrate> {
    if !m.has_table("package") {
        return None;
    }
    let name = m
        .str_of("package", "name")
        .map(str::to_string)
        // A manifest mid-edit may have no name yet; the directory is a better label than nothing.
        .unwrap_or_else(|| dir_name(dir));

    let deps = declared(m);
    Some(CargoCrate {
        name: name.clone(),
        version: inherited_or(m, "version"),
        rel_path: rel.replace('\\', "/"),
        manifest: slash(&dir.join("Cargo.toml")),
        edition: inherited_or(m, "edition"),
        description: m.str_of("package", "description").unwrap_or_default().to_string(),
        is_root,
        // `publish = false` is the only shape that means "not publishable"; a list of registries
        // still is.
        publish: m.get_base("package", "publish").and_then(|e| e.bool_value()) != Some(false),
        targets: targets_of(dir, m, &name),
        features: features_of(m),
        deps: deps.iter().filter(|d| d.kind == DepKind::Normal).count(),
        dev_deps: deps.iter().filter(|d| d.kind == DepKind::Dev).count(),
        build_deps: deps.iter().filter(|d| d.kind == DepKind::Build).count(),
    })
}

/// A `[package]` string key, or the word `inherited` when it comes from the workspace.
///
/// Better than an empty cell: "this crate has no version" and "this crate's version is the
/// workspace's" are different facts, and the second is the common one.
fn inherited_or(m: &Manifest, key: &str) -> String {
    match m.get_base("package", key) {
        Some(e) if e.key_suffix() == Some("workspace") => "inherited".to_string(),
        Some(e) => e.str_value().unwrap_or_default().to_string(),
        None => String::new(),
    }
}

// ── targets ────────────────────────────────────────────────────────────────────

/// The crate's targets: what the manifest declares, then what Cargo would discover.
fn targets_of(dir: &Path, m: &Manifest, package: &str) -> Vec<CargoTarget> {
    let mut out: Vec<CargoTarget> = Vec::new();

    // `[lib]`.
    if m.has_table("lib") {
        out.push(CargoTarget {
            name: m.str_of("lib", "name").unwrap_or(&package.replace('-', "_")).to_string(),
            kind: "lib".to_string(),
            path: m.str_of("lib", "path").unwrap_or("src/lib.rs").to_string(),
            declared: true,
            proc_macro: m.bool_of("lib", "proc-macro").unwrap_or(false),
            required_features: string_items(m, "lib", "required-features"),
        });
    }
    // `[[bin]]` / `[[example]]` / `[[test]]` / `[[bench]]`.
    for kind in ["bin", "example", "test", "bench"] {
        for element in m.array_elements(kind) {
            let name = element.iter().find(|e| e.key == "name").and_then(|e| e.str_value());
            let path = element.iter().find(|e| e.key == "path").and_then(|e| e.str_value());
            let Some(name) = name.or(path) else { continue };
            let required = element
                .iter()
                .find(|e| e.key == "required-features")
                .map(|e| e.items.iter().map(|i| i.text.clone()).collect())
                .unwrap_or_default();
            out.push(CargoTarget {
                name: name.to_string(),
                kind: kind.to_string(),
                path: path.unwrap_or_default().to_string(),
                declared: true,
                proc_macro: false,
                required_features: required,
            });
        }
    }

    // Auto-discovery. Each switch defaults to on, which is Cargo's own behaviour.
    let auto = |key: &str| m.bool_of("package", key).unwrap_or(true);

    if auto("autolib") && !out.iter().any(|t| t.kind == "lib") && dir.join("src/lib.rs").is_file() {
        out.push(discovered(&package.replace('-', "_"), "lib", "src/lib.rs"));
    }
    if auto("autobins") {
        if dir.join("src/main.rs").is_file() {
            push_unique(&mut out, discovered(package, "bin", "src/main.rs"));
        }
        for (name, path) in rust_files(dir, "src/bin") {
            push_unique(&mut out, discovered(&name, "bin", &path));
        }
    }
    if auto("autoexamples") {
        for (name, path) in rust_files(dir, "examples") {
            push_unique(&mut out, discovered(&name, "example", &path));
        }
    }
    if auto("autotests") {
        for (name, path) in rust_files(dir, "tests") {
            push_unique(&mut out, discovered(&name, "test", &path));
        }
    }
    if auto("autobenches") {
        for (name, path) in rust_files(dir, "benches") {
            push_unique(&mut out, discovered(&name, "bench", &path));
        }
    }
    out
}

fn discovered(name: &str, kind: &str, path: &str) -> CargoTarget {
    CargoTarget {
        name: name.to_string(),
        kind: kind.to_string(),
        path: path.to_string(),
        declared: false,
        proc_macro: false,
        required_features: Vec::new(),
    }
}

/// Add unless a target of the same kind and name is already there — a declared `[[bin]]` wins over
/// the file that would have been discovered as the same one.
fn push_unique(out: &mut Vec<CargoTarget>, t: CargoTarget) {
    if !out.iter().any(|e| e.kind == t.kind && e.name == t.name) {
        out.push(t);
    }
}

/// The Rust source targets directly under `dir/sub`, as `(name, relative path)`.
///
/// Both conventions: `sub/foo.rs` and `sub/foo/main.rs`. Sorted, so the panel does not reshuffle
/// between two reads of the same crate.
fn rust_files(dir: &Path, sub: &str) -> Vec<(String, String)> {
    let base = dir.join(sub);
    let Ok(entries) = std::fs::read_dir(&base) else { return Vec::new() };
    let mut out: Vec<(String, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file) = entry.file_name().to_str().map(str::to_string) else { continue };
        if path.is_dir() {
            if path.join("main.rs").is_file() {
                out.push((file.clone(), format!("{sub}/{file}/main.rs")));
            }
            continue;
        }
        if let Some(stem) = file.strip_suffix(".rs") {
            // `mod.rs` under `src/bin` is not a target.
            if stem != "mod" {
                out.push((stem.to_string(), format!("{sub}/{file}")));
            }
        }
    }
    out.sort();
    out
}

// ── features ───────────────────────────────────────────────────────────────────

/// The crate's features, with what `default` reaches marked.
fn features_of(m: &Manifest) -> Vec<CargoFeature> {
    let mut out: Vec<CargoFeature> = m
        .entries_in("features")
        .map(|e| CargoFeature {
            name: e.key.clone(),
            enables: e.items.iter().map(|i| i.text.clone()).collect(),
            default: false,
        })
        .collect();

    // An optional dependency defines an implicit feature of its own name — unless something in the
    // manifest already refers to it as `dep:x`, which is Cargo's rule and the reason this is not
    // simply "every optional dependency".
    let deps = declared(m);
    let referenced_as_dep: Vec<String> = out
        .iter()
        .flat_map(|f| f.enables.iter())
        .filter_map(|e| e.strip_prefix("dep:").map(str::to_string))
        .collect();
    for d in deps.iter().filter(|d| d.optional) {
        if referenced_as_dep.contains(&d.name) || out.iter().any(|f| f.name == d.name) {
            continue;
        }
        out.push(CargoFeature { name: d.name.clone(), enables: Vec::new(), default: false });
    }

    // What `default` reaches, transitively. A fixed number of passes rather than recursion: a
    // manifest can have a cycle (Cargo rejects it, we must not hang on it), and the depth of a
    // real feature tree is small.
    let mut enabled: Vec<String> = out
        .iter()
        .find(|f| f.name == "default")
        .map(|f| f.enables.clone())
        .unwrap_or_default();
    for _ in 0..8 {
        let mut grew = false;
        for name in enabled.clone() {
            let bare = name.split('/').next().unwrap_or(&name).trim_end_matches('?');
            let bare = bare.strip_prefix("dep:").unwrap_or(bare);
            if let Some(f) = out.iter().find(|f| f.name == bare) {
                for next in &f.enables {
                    if !enabled.contains(next) {
                        enabled.push(next.clone());
                        grew = true;
                    }
                }
            }
        }
        if !grew {
            break;
        }
    }
    for f in &mut out {
        f.default = f.name == "default"
            || enabled.iter().any(|e| {
                let bare = e.split('/').next().unwrap_or(e).trim_end_matches('?');
                bare.strip_prefix("dep:").unwrap_or(bare) == f.name
            });
    }
    out
}

// ── members and orphans ────────────────────────────────────────────────────────

/// The manifest's `members`, expanded against the filesystem into paths relative to `root`.
///
/// A trailing-`*` glob lists the directories under that prefix that hold a `Cargo.toml`; a plain
/// path is kept when it holds one. Anything resolving to nothing is dropped — a member that is not
/// there is Cargo's error to raise (and the validator's to report), and listing a crate that does
/// not exist would be worse than a shorter list.
pub fn expand_members(root: &Path, m: &Manifest) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in m.items_of("workspace", "members") {
        let pattern = item.text.trim().trim_end_matches('/');
        if pattern.is_empty() {
            continue;
        }
        match pattern.strip_suffix("/*").or_else(|| pattern.strip_suffix('*')) {
            Some(prefix) if !prefix.contains('*') => {
                let dir = root.join(prefix.trim_end_matches('/'));
                let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                let mut found: Vec<String> = entries
                    .flatten()
                    .filter(|e| e.path().join("Cargo.toml").is_file())
                    .filter_map(|e| e.file_name().to_str().map(str::to_string))
                    .map(|name| join_rel(prefix.trim_end_matches('/'), &name))
                    .collect();
                // `read_dir` order is the filesystem's; a panel that reshuffled between two opens
                // of the same workspace would read as a change.
                found.sort();
                for f in found {
                    push_rel(&mut out, f);
                }
            }
            // An interior glob is not expanded — vanishingly rare, and half-expanding one would
            // put a literal `*` in the panel.
            Some(_) => {}
            None => {
                if root.join(pattern).join("Cargo.toml").is_file() {
                    push_rel(&mut out, pattern.replace('\\', "/"));
                }
            }
        }
    }
    out
}

fn push_rel(out: &mut Vec<String>, rel: String) {
    if !out.contains(&rel) {
        out.push(rel);
    }
}

fn join_rel(prefix: &str, name: &str) -> String {
    if prefix.is_empty() { name.to_string() } else { format!("{prefix}/{name}") }
}

/// Crate directories under `root` that no `members` pattern covers.
///
/// `exclude` is honoured, and a crate *inside* another crate's directory is not an orphan: that is
/// how a fixture crate or a test-only workspace-within-a-workspace is written, and flagging it
/// would be a false positive on a deliberate layout.
fn find_orphans(root: &Path, m: &Manifest, members: &[String]) -> Vec<String> {
    let excluded: Vec<String> =
        m.items_of("workspace", "exclude").iter().map(|i| i.text.trim_end_matches('/').to_string()).collect();
    let mut out: Vec<String> = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let mut visited = 0usize;

    while let Some((dir, depth)) = stack.pop() {
        if depth >= MAX_DEPTH || visited >= MAX_CRATES * 4 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            visited += 1;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            let rel = relative(root, &path);
            if excluded.iter().any(|e| rel == *e || rel.starts_with(&format!("{e}/"))) {
                continue;
            }
            // Inside a member's own tree — not an orphan, and not worth walking further for one.
            if members.iter().any(|mem| rel.starts_with(&format!("{mem}/"))) {
                continue;
            }
            if path.join("Cargo.toml").is_file() {
                if !members.contains(&rel) {
                    out.push(rel);
                }
                // Whatever is below a crate belongs to that crate.
                continue;
            }
            stack.push((path, depth + 1));
        }
    }
    out.sort();
    out
}

// ── path helpers ───────────────────────────────────────────────────────────────

/// `path` relative to `root`, forward-slashed.
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

/// An absolute path, forward-slashed — the convention on the bennu wire.
fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn dir_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// The quoted items of `table.key`.
fn string_items(m: &Manifest, table: &str, key: &str) -> Vec<String> {
    m.items_of(table, key).iter().map(|i| i.text.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch workspace on disk, unique per process and thread.
    struct Fixture(PathBuf);

    impl Fixture {
        fn new(tag: &str) -> Fixture {
            let dir = std::env::temp_dir().join(format!(
                "bennu-cargo-ws-{tag}-{}-{:?}",
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

        fn dir(&self, rel: &str) -> &Fixture {
            std::fs::create_dir_all(self.0.join(rel)).unwrap();
            self
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_virtual_workspace_lists_its_members_in_declaration_order() {
        let f = Fixture::new("virtual");
        f.write(
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\", \"tools/cli\"]\n[workspace.package]\nedition = \"2021\"\n",
        );
        f.write("crates/beta/Cargo.toml", "[package]\nname = \"beta\"\nversion = \"0.2.0\"\n");
        f.write("crates/alpha/Cargo.toml", "[package]\nname = \"alpha\"\nedition.workspace = true\n");
        f.write("tools/cli/Cargo.toml", "[package]\nname = \"cli\"\nversion = \"1.0.0\"\n");

        let ws = read(&f.0);
        assert!(ws.virtual_manifest, "no [package] at the root");
        assert!(ws.is_workspace);
        assert_eq!(ws.resolver, "2");
        assert_eq!(ws.edition, "2021");
        // Globs expand sorted; a plain member keeps its place in the declaration order.
        let names: Vec<&str> = ws.crates.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta", "cli"]);
        // A version from the workspace is said to be inherited rather than left blank.
        assert_eq!(ws.crate_named("alpha").unwrap().edition, "inherited");
        assert_eq!(ws.crate_named("beta").unwrap().version, "0.2.0");
    }

    #[test]
    fn a_single_crate_project_is_a_workspace_of_one() {
        let f = Fixture::new("single");
        f.write("Cargo.toml", "[package]\nname = \"solo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
        f.write("src/main.rs", "fn main() {}\n");

        let ws = read(&f.0);
        assert!(!ws.is_workspace);
        assert!(!ws.virtual_manifest);
        assert_eq!(ws.name, "solo");
        assert_eq!(ws.crates.len(), 1);
        assert!(ws.crates[0].is_root);
        assert_eq!(ws.crates[0].rel_path, "");
    }

    #[test]
    fn targets_are_discovered_as_well_as_declared() {
        let f = Fixture::new("targets");
        f.write("Cargo.toml", "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n[[bin]]\nname = \"special\"\npath = \"src/other.rs\"\nrequired-features = [\"extra\"]\n");
        f.write("src/lib.rs", "");
        f.write("src/main.rs", "fn main() {}");
        f.write("src/bin/helper.rs", "fn main() {}");
        f.write("src/bin/nested/main.rs", "fn main() {}");
        f.write("examples/demo.rs", "fn main() {}");
        f.write("tests/it.rs", "");
        f.write("benches/bench.rs", "");

        let ws = read(&f.0);
        let c = &ws.crates[0];
        let names = |kind: &str| {
            let mut n: Vec<String> = c.targets_of(kind).iter().map(|t| t.name.clone()).collect();
            n.sort();
            n
        };
        // The lib's default name is the package name with hyphens turned into underscores — the
        // crate name a `use` statement writes.
        assert_eq!(names("lib"), vec!["my_app"]);
        assert_eq!(names("bin"), vec!["helper", "my-app", "nested", "special"]);
        assert_eq!(names("example"), vec!["demo"]);
        assert_eq!(names("test"), vec!["it"]);
        assert_eq!(names("bench"), vec!["bench"]);

        let special = c.targets_of("bin").into_iter().find(|t| t.name == "special").unwrap();
        assert!(special.declared);
        assert_eq!(special.path, "src/other.rs");
        assert_eq!(special.required_features, vec!["extra"]);
        assert!(!c.targets_of("bin").iter().find(|t| t.name == "helper").unwrap().declared);
    }

    #[test]
    fn auto_discovery_switches_are_honoured() {
        let f = Fixture::new("auto");
        f.write("Cargo.toml", "[package]\nname = \"x\"\nautobins = false\nautotests = false\n");
        f.write("src/main.rs", "fn main() {}");
        f.write("src/bin/extra.rs", "fn main() {}");
        f.write("tests/it.rs", "");
        f.write("examples/e.rs", "fn main() {}");

        let ws = read(&f.0);
        let c = &ws.crates[0];
        assert!(c.targets_of("bin").is_empty(), "autobins = false");
        assert!(c.targets_of("test").is_empty(), "autotests = false");
        assert_eq!(c.targets_of("example").len(), 1, "examples are still discovered");
        assert!(!c.is_runnable());
    }

    /// The question a run button has to answer, and the one case where guessing would launch the
    /// wrong program.
    #[test]
    fn a_sole_binary_is_identified_and_an_ambiguous_one_is_not() {
        let f = Fixture::new("sole");
        f.write("Cargo.toml", "[package]\nname = \"one\"\n");
        f.write("src/main.rs", "fn main() {}");
        assert_eq!(read(&f.0).crates[0].sole_binary().map(|t| t.name.clone()), Some("one".into()));

        f.write("src/bin/two.rs", "fn main() {}");
        assert!(read(&f.0).crates[0].sole_binary().is_none(), "two binaries is a real ambiguity");
    }

    #[test]
    fn features_include_implicit_ones_and_mark_what_default_reaches() {
        let f = Fixture::new("features");
        f.write(
            "Cargo.toml",
            "\
[package]
name = \"x\"

[dependencies]
serde = { version = \"1\", optional = true }
tracing = { version = \"1\", optional = true }
anyhow = \"1\"

[features]
default = [\"pretty\"]
pretty = [\"colour\"]
colour = []
loud = [\"dep:tracing\"]
",
        );
        let ws = read(&f.0);
        let feats = &ws.crates[0].features;
        let named = |n: &str| feats.iter().find(|f| f.name == n);

        // `default` reaches `pretty` reaches `colour` — transitively, which is the only useful
        // reading of "on by default".
        assert!(named("default").unwrap().default);
        assert!(named("pretty").unwrap().default);
        assert!(named("colour").unwrap().default);
        assert!(!named("loud").unwrap().default);

        // An optional dependency defines an implicit feature…
        assert!(named("serde").is_some(), "serde's implicit feature");
        // …unless something already refers to it as `dep:`, which is Cargo's rule.
        assert!(named("tracing").is_none(), "`dep:tracing` suppresses the implicit feature");
        // A non-optional dependency defines nothing.
        assert!(named("anyhow").is_none());
    }

    /// A feature cycle is a manifest Cargo rejects. It must not make this hang.
    #[test]
    fn a_feature_cycle_terminates() {
        let f = Fixture::new("cycle");
        f.write(
            "Cargo.toml",
            "[package]\nname = \"x\"\n[features]\ndefault = [\"a\"]\na = [\"b\"]\nb = [\"a\"]\n",
        );
        let ws = read(&f.0);
        assert!(ws.crates[0].features.iter().find(|f| f.name == "a").unwrap().default);
    }

    #[test]
    fn a_crate_the_workspace_forgot_is_reported_as_an_orphan() {
        let f = Fixture::new("orphans");
        f.write("Cargo.toml", "[workspace]\nmembers = [\"crates/listed\"]\nexclude = [\"vendor\"]\n");
        f.write("crates/listed/Cargo.toml", "[package]\nname = \"listed\"\n");
        f.write("crates/forgotten/Cargo.toml", "[package]\nname = \"forgotten\"\n");
        f.write("vendor/thirdparty/Cargo.toml", "[package]\nname = \"vendored\"\n");
        // Inside a member — a fixture crate, which is a deliberate layout and not an orphan.
        f.write("crates/listed/fixtures/inner/Cargo.toml", "[package]\nname = \"inner\"\n");
        f.dir("target/debug");

        let ws = read(&f.0);
        assert_eq!(ws.orphans, vec!["crates/forgotten"]);
    }

    #[test]
    fn an_unreadable_member_is_named_rather_than_silently_missing() {
        let f = Fixture::new("unreadable");
        f.write("Cargo.toml", "[workspace]\nmembers = [\"a\"]\n");
        // The member is declared but there is no manifest at all — which `expand_members` drops,
        // so nothing is claimed about it and the list is simply shorter.
        f.dir("a");
        let ws = read(&f.0);
        assert!(ws.crates.is_empty());
        assert!(ws.unreadable.is_empty(), "a member that does not exist is the validator's report");
    }

    #[test]
    fn a_root_with_no_manifest_yields_an_empty_workspace_named_after_the_directory() {
        let f = Fixture::new("bare");
        let ws = read(&f.0);
        assert!(ws.crates.is_empty());
        assert!(!ws.is_workspace);
        assert!(ws.name.starts_with("bennu-cargo-ws-bare"));
    }

    #[test]
    fn binaries_lists_every_runnable_target_across_the_workspace() {
        let f = Fixture::new("bins");
        f.write("Cargo.toml", "[workspace]\nmembers = [\"a\", \"b\"]\n");
        f.write("a/Cargo.toml", "[package]\nname = \"a\"\n");
        f.write("a/src/main.rs", "fn main() {}");
        f.write("b/Cargo.toml", "[package]\nname = \"b\"\n");
        f.write("b/src/lib.rs", "");
        f.write("b/src/bin/tool.rs", "fn main() {}");

        let ws = read(&f.0);
        let found: Vec<String> =
            ws.binaries().into_iter().map(|(c, t)| format!("{}:{}", c.name, t.name)).collect();
        assert_eq!(found, vec!["a:a", "b:tool"]);
    }

    #[test]
    fn dependency_counts_are_split_by_kind() {
        let f = Fixture::new("counts");
        f.write(
            "Cargo.toml",
            "[package]\nname = \"x\"\n[dependencies]\na = \"1\"\nb = \"1\"\n[dev-dependencies]\nc = \"1\"\n[build-dependencies]\nd = \"1\"\n",
        );
        let c = &read(&f.0).crates[0];
        assert_eq!((c.deps, c.dev_deps, c.build_deps), (2, 1, 1));
    }
}
