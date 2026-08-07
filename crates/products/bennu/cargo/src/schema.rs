//! What a `Cargo.toml` may contain — the table [`validate`](crate::validate) and
//! [`complete`](crate::complete) both read.
//!
//! ## Why one table and not two
//!
//! These two features are the same knowledge asked in opposite directions: *"is `edtion` a real
//! key"* and *"what keys can I type here"*. Written twice they drift, and the failure is
//! particularly bad — a key that completes and then underlines itself as unknown is worse than
//! having neither feature, because it makes the editor look wrong about something the user can
//! see. So there is one list, and both directions are derived from it.
//!
//! ## What "unknown key" is allowed to mean
//!
//! Cargo itself warns about unrecognised keys rather than failing, and it gains keys every few
//! releases. A hard error here would mean a Bennu that is a version behind flags a perfectly good
//! manifest. So an unknown key in a *closed* table is a **warning**, and there are three whole
//! categories of table where it is nothing at all:
//!
//! - [`Openness::Dependencies`] — every key is a crate name;
//! - [`Openness::Free`] — `[features]`, `[package.metadata]`, `[lints.*]`: the key set is the
//!   user's, or a third-party tool's, and we have no standing to have an opinion;
//! - a table this schema has never heard of — see [`table_def`], which answers `None` and makes
//!   the validator say nothing about anything inside it.

/// What a value may be. Used to check one and to offer one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// A quoted string.
    Str,
    /// `true` / `false`.
    Bool,
    /// An integer. Also accepts a string, because `opt-level = "s"` is legal.
    Int,
    /// A string from a closed set — the values completion offers and validation checks.
    Enum(&'static [&'static str]),
    /// An array of strings.
    StrArray,
    /// A string **or** an array of strings (`include`/`exclude` are arrays, `readme` may be
    /// either a path or `false`).
    StrOrArray,
    /// A table or an inline table.
    Table,
    /// Known to exist, shape deliberately unchecked.
    Any,
}

/// One key of one table.
#[derive(Debug, Clone, Copy)]
pub struct KeyDef {
    pub name: &'static str,
    pub kind: ValueKind,
    /// One line, shown as the completion's detail and in a hover.
    pub doc: &'static str,
    /// Whether `name.workspace = true` is legal here — Cargo's workspace inheritance.
    ///
    /// It has to be per-key: `edition.workspace = true` is right and `name.workspace = true` is
    /// not, and a validator that did not know the difference would either flag the correct one or
    /// wave through the wrong one.
    pub inheritable: bool,
}

/// How a table's key set behaves — see the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Openness {
    /// Only the declared keys are valid. An unknown one is a warning.
    Closed,
    /// Every key is a dependency name; the value is a version string or a dependency spec.
    Dependencies,
    /// Any key, any value. Nothing is checked.
    Free,
}

/// One table of the manifest.
#[derive(Debug, Clone, Copy)]
pub struct TableDef {
    /// The canonical path, with `*` standing for a free-form segment — see [`canonical_path`].
    pub path: &'static str,
    /// One line, shown when completing the header.
    pub doc: &'static str,
    pub keys: &'static [KeyDef],
    pub open: Openness,
}

impl TableDef {
    /// The definition of `key` in this table.
    pub fn key(&self, key: &str) -> Option<&'static KeyDef> {
        self.keys.iter().find(|k| k.name == key)
    }
}

const EDITIONS: &[&str] = &["2015", "2018", "2021", "2024"];
const RESOLVERS: &[&str] = &["1", "2", "3"];
const CRATE_TYPES: &[&str] = &["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro", "bin"];
const LTO: &[&str] = &["false", "true", "thin", "fat", "off"];
const PANIC: &[&str] = &["unwind", "abort"];
const STRIP: &[&str] = &["none", "debuginfo", "symbols", "true", "false"];
const SPLIT_DEBUGINFO: &[&str] = &["off", "packed", "unpacked"];
const DEBUG_LEVELS: &[&str] = &["none", "limited", "full", "line-tables-only", "line-directives-only"];
const LINT_LEVELS: &[&str] = &["allow", "warn", "deny", "forbid"];

/// `[package]`. The inheritable flags are Cargo's actual list, not a guess: `name` and `build`
/// are not inheritable, and `version` / `edition` / `license` are the ones every workspace sets
/// once.
const PACKAGE_KEYS: &[KeyDef] = &[
    KeyDef { name: "name", kind: ValueKind::Str, doc: "The crate name. Required.", inheritable: false },
    KeyDef { name: "version", kind: ValueKind::Str, doc: "The crate version (semver).", inheritable: true },
    KeyDef { name: "edition", kind: ValueKind::Enum(EDITIONS), doc: "The Rust edition the crate is compiled as.", inheritable: true },
    KeyDef { name: "rust-version", kind: ValueKind::Str, doc: "Minimum supported Rust version (MSRV).", inheritable: true },
    KeyDef { name: "authors", kind: ValueKind::StrArray, doc: "The crate's authors.", inheritable: true },
    KeyDef { name: "description", kind: ValueKind::Str, doc: "One-paragraph description. Required to publish.", inheritable: true },
    KeyDef { name: "documentation", kind: ValueKind::Str, doc: "URL of the crate's documentation.", inheritable: true },
    KeyDef { name: "readme", kind: ValueKind::StrOrArray, doc: "Path to the README, or `false` for none.", inheritable: true },
    KeyDef { name: "homepage", kind: ValueKind::Str, doc: "URL of the crate's home page.", inheritable: true },
    KeyDef { name: "repository", kind: ValueKind::Str, doc: "URL of the source repository.", inheritable: true },
    KeyDef { name: "license", kind: ValueKind::Str, doc: "SPDX licence expression (`MIT OR Apache-2.0`).", inheritable: true },
    KeyDef { name: "license-file", kind: ValueKind::Str, doc: "Path to a licence file, when `license` cannot express it.", inheritable: true },
    KeyDef { name: "keywords", kind: ValueKind::StrArray, doc: "Up to five keywords, for crates.io search.", inheritable: true },
    KeyDef { name: "categories", kind: ValueKind::StrArray, doc: "crates.io category slugs.", inheritable: true },
    KeyDef { name: "workspace", kind: ValueKind::Str, doc: "Path to the workspace root this crate belongs to.", inheritable: false },
    KeyDef { name: "build", kind: ValueKind::StrOrArray, doc: "Build-script path, or `false` to disable it.", inheritable: false },
    KeyDef { name: "links", kind: ValueKind::Str, doc: "Name of the native library this crate links.", inheritable: false },
    KeyDef { name: "exclude", kind: ValueKind::StrArray, doc: "Paths left out of the published package.", inheritable: true },
    KeyDef { name: "include", kind: ValueKind::StrArray, doc: "Paths included in the published package (overrides `exclude`).", inheritable: true },
    KeyDef { name: "publish", kind: ValueKind::Any, doc: "`false` to forbid publishing, or the registries allowed.", inheritable: true },
    KeyDef { name: "metadata", kind: ValueKind::Table, doc: "Free-form table for external tools. Cargo ignores it.", inheritable: false },
    KeyDef { name: "default-run", kind: ValueKind::Str, doc: "Which `[[bin]]` plain `cargo run` launches.", inheritable: false },
    KeyDef { name: "resolver", kind: ValueKind::Enum(RESOLVERS), doc: "Feature-resolver version. Ignored in a workspace member.", inheritable: false },
    KeyDef { name: "autolib", kind: ValueKind::Bool, doc: "Whether to auto-discover `src/lib.rs`.", inheritable: false },
    KeyDef { name: "autobins", kind: ValueKind::Bool, doc: "Whether to auto-discover `src/bin/*`.", inheritable: false },
    KeyDef { name: "autoexamples", kind: ValueKind::Bool, doc: "Whether to auto-discover `examples/`.", inheritable: false },
    KeyDef { name: "autotests", kind: ValueKind::Bool, doc: "Whether to auto-discover `tests/`.", inheritable: false },
    KeyDef { name: "autobenches", kind: ValueKind::Bool, doc: "Whether to auto-discover `benches/`.", inheritable: false },
];

/// `[workspace]`.
const WORKSPACE_KEYS: &[KeyDef] = &[
    KeyDef { name: "members", kind: ValueKind::StrArray, doc: "The crates in the workspace. Globs allowed (`crates/*`).", inheritable: false },
    KeyDef { name: "exclude", kind: ValueKind::StrArray, doc: "Paths under the root that are NOT members.", inheritable: false },
    KeyDef { name: "default-members", kind: ValueKind::StrArray, doc: "What a bare `cargo build` at the root builds.", inheritable: false },
    KeyDef { name: "resolver", kind: ValueKind::Enum(RESOLVERS), doc: "Feature-resolver version for the whole workspace.", inheritable: false },
    KeyDef { name: "package", kind: ValueKind::Table, doc: "Package fields members inherit with `x.workspace = true`.", inheritable: false },
    KeyDef { name: "dependencies", kind: ValueKind::Table, doc: "Dependency versions members inherit with `x.workspace = true`.", inheritable: false },
    KeyDef { name: "lints", kind: ValueKind::Table, doc: "Lint levels members inherit with `[lints] workspace = true`.", inheritable: false },
    KeyDef { name: "metadata", kind: ValueKind::Table, doc: "Free-form table for external tools. Cargo ignores it.", inheritable: false },
];

/// `[lib]` / `[[bin]]` / `[[example]]` / `[[test]]` / `[[bench]]`.
const TARGET_KEYS: &[KeyDef] = &[
    KeyDef { name: "name", kind: ValueKind::Str, doc: "The target's name — the binary or library name.", inheritable: false },
    KeyDef { name: "path", kind: ValueKind::Str, doc: "Source file, when it is not the conventional one.", inheritable: false },
    KeyDef { name: "test", kind: ValueKind::Bool, doc: "Whether `cargo test` builds this target.", inheritable: false },
    KeyDef { name: "doctest", kind: ValueKind::Bool, doc: "Whether doc examples in this target run.", inheritable: false },
    KeyDef { name: "bench", kind: ValueKind::Bool, doc: "Whether `cargo bench` builds this target.", inheritable: false },
    KeyDef { name: "doc", kind: ValueKind::Bool, doc: "Whether `cargo doc` documents this target.", inheritable: false },
    KeyDef { name: "proc-macro", kind: ValueKind::Bool, doc: "This library is a procedural macro.", inheritable: false },
    KeyDef { name: "harness", kind: ValueKind::Bool, doc: "`false` to supply your own `main` instead of libtest's.", inheritable: false },
    KeyDef { name: "edition", kind: ValueKind::Enum(EDITIONS), doc: "Edition for this target only.", inheritable: false },
    KeyDef { name: "crate-type", kind: ValueKind::StrArray, doc: "What to emit: `lib`, `cdylib`, `staticlib`, …", inheritable: false },
    KeyDef { name: "required-features", kind: ValueKind::StrArray, doc: "Features that must be on for this target to build.", inheritable: false },
    KeyDef { name: "doc-scrape-examples", kind: ValueKind::Bool, doc: "Whether rustdoc scrapes this target for examples.", inheritable: false },
];

/// `[profile.*]`, `[profile.*.package.*]` and `[profile.*.build-override]`.
const PROFILE_KEYS: &[KeyDef] = &[
    KeyDef { name: "opt-level", kind: ValueKind::Int, doc: "Optimisation level: 0–3, `\"s\"` or `\"z\"` for size.", inheritable: false },
    KeyDef { name: "debug", kind: ValueKind::Enum(DEBUG_LEVELS), doc: "Debug info: a bool, 0–2, or a named level.", inheritable: false },
    KeyDef { name: "split-debuginfo", kind: ValueKind::Enum(SPLIT_DEBUGINFO), doc: "Whether debug info is split out of the binary.", inheritable: false },
    KeyDef { name: "strip", kind: ValueKind::Enum(STRIP), doc: "What to strip from the binary.", inheritable: false },
    KeyDef { name: "debug-assertions", kind: ValueKind::Bool, doc: "Whether `debug_assert!` is compiled in.", inheritable: false },
    KeyDef { name: "overflow-checks", kind: ValueKind::Bool, doc: "Whether arithmetic overflow panics.", inheritable: false },
    KeyDef { name: "lto", kind: ValueKind::Enum(LTO), doc: "Link-time optimisation.", inheritable: false },
    KeyDef { name: "panic", kind: ValueKind::Enum(PANIC), doc: "Panic strategy. `abort` drops unwinding.", inheritable: false },
    KeyDef { name: "incremental", kind: ValueKind::Bool, doc: "Incremental compilation for this profile.", inheritable: false },
    KeyDef { name: "codegen-units", kind: ValueKind::Int, doc: "Parallel codegen units. Fewer is slower to build, faster to run.", inheritable: false },
    KeyDef { name: "rpath", kind: ValueKind::Bool, doc: "Pass `-C rpath` to rustc.", inheritable: false },
    KeyDef { name: "inherits", kind: ValueKind::Str, doc: "The profile this one starts from (`release`, `dev`).", inheritable: false },
];

/// The keys of a dependency spec — the inline table (or `[dependencies.foo]` table) form.
///
/// Public because completion inside `serde = { … }` needs it and it is the same list a
/// `[dependencies.serde]` table is checked against.
pub const DEP_KEYS: &[KeyDef] = &[
    KeyDef { name: "version", kind: ValueKind::Str, doc: "Semver requirement (`1`, `^1.2`, `=1.2.3`).", inheritable: false },
    KeyDef { name: "path", kind: ValueKind::Str, doc: "A crate on disk, relative to this manifest.", inheritable: false },
    KeyDef { name: "git", kind: ValueKind::Str, doc: "Repository URL to fetch the crate from.", inheritable: false },
    KeyDef { name: "branch", kind: ValueKind::Str, doc: "Branch to use with `git`.", inheritable: false },
    KeyDef { name: "tag", kind: ValueKind::Str, doc: "Tag to use with `git`.", inheritable: false },
    KeyDef { name: "rev", kind: ValueKind::Str, doc: "Exact revision to use with `git`.", inheritable: false },
    KeyDef { name: "features", kind: ValueKind::StrArray, doc: "Features to enable on the dependency.", inheritable: false },
    KeyDef { name: "default-features", kind: ValueKind::Bool, doc: "`false` to drop the dependency's default features.", inheritable: false },
    KeyDef { name: "optional", kind: ValueKind::Bool, doc: "Only built when a feature turns it on.", inheritable: false },
    KeyDef { name: "package", kind: ValueKind::Str, doc: "The real crate name, when this entry renames it.", inheritable: false },
    KeyDef { name: "registry", kind: ValueKind::Str, doc: "A registry other than crates.io.", inheritable: false },
    KeyDef { name: "workspace", kind: ValueKind::Bool, doc: "Inherit this dependency from `[workspace.dependencies]`.", inheritable: false },
    KeyDef { name: "public", kind: ValueKind::Bool, doc: "Part of this crate's public API (unstable).", inheritable: false },
];

/// A lint table: every key is a lint name, the value a level or `{ level, priority }`.
const LINT_KEYS: &[KeyDef] = &[
    KeyDef { name: "level", kind: ValueKind::Enum(LINT_LEVELS), doc: "`allow` · `warn` · `deny` · `forbid`.", inheritable: false },
    KeyDef { name: "priority", kind: ValueKind::Int, doc: "Order against other lints. Lower is applied first.", inheritable: false },
];

/// Every table this schema knows, by canonical path.
pub const TABLES: &[TableDef] = &[
    TableDef { path: "package", doc: "Who this crate is: name, version, edition, metadata.", keys: PACKAGE_KEYS, open: Openness::Closed },
    TableDef { path: "package.metadata", doc: "Free-form, for external tools. Cargo ignores it.", keys: &[], open: Openness::Free },
    TableDef { path: "workspace", doc: "The workspace: its members and what they inherit.", keys: WORKSPACE_KEYS, open: Openness::Closed },
    TableDef { path: "workspace.package", doc: "Package fields members inherit with `x.workspace = true`.", keys: PACKAGE_KEYS, open: Openness::Closed },
    TableDef { path: "workspace.metadata", doc: "Free-form, for external tools. Cargo ignores it.", keys: &[], open: Openness::Free },
    TableDef { path: "workspace.dependencies", doc: "Versions members inherit with `dep.workspace = true`.", keys: &[], open: Openness::Dependencies },
    TableDef { path: "workspace.lints", doc: "Lint levels members inherit with `[lints] workspace = true`.", keys: &[], open: Openness::Free },
    TableDef { path: "dependencies", doc: "What the crate needs to build.", keys: &[], open: Openness::Dependencies },
    TableDef { path: "dev-dependencies", doc: "Needed by tests, examples and benchmarks only.", keys: &[], open: Openness::Dependencies },
    TableDef { path: "build-dependencies", doc: "Needed by the build script only.", keys: &[], open: Openness::Dependencies },
    TableDef { path: "target.*.dependencies", doc: "Dependencies only for platforms matching the `cfg`.", keys: &[], open: Openness::Dependencies },
    TableDef { path: "target.*.dev-dependencies", doc: "Dev-dependencies only for platforms matching the `cfg`.", keys: &[], open: Openness::Dependencies },
    TableDef { path: "target.*.build-dependencies", doc: "Build-dependencies only for platforms matching the `cfg`.", keys: &[], open: Openness::Dependencies },
    TableDef { path: "features", doc: "The crate's optional features and what each turns on.", keys: &[], open: Openness::Free },
    TableDef { path: "lib", doc: "The library target, when it is not the conventional one.", keys: TARGET_KEYS, open: Openness::Closed },
    TableDef { path: "bin", doc: "One binary target.", keys: TARGET_KEYS, open: Openness::Closed },
    TableDef { path: "example", doc: "One example target.", keys: TARGET_KEYS, open: Openness::Closed },
    TableDef { path: "test", doc: "One integration-test target.", keys: TARGET_KEYS, open: Openness::Closed },
    TableDef { path: "bench", doc: "One benchmark target.", keys: TARGET_KEYS, open: Openness::Closed },
    TableDef { path: "profile.*", doc: "Compiler settings for one build profile.", keys: PROFILE_KEYS, open: Openness::Closed },
    TableDef { path: "profile.*.package.*", doc: "Profile settings for one dependency only.", keys: PROFILE_KEYS, open: Openness::Closed },
    TableDef { path: "profile.*.build-override", doc: "Profile settings for build scripts and proc macros.", keys: PROFILE_KEYS, open: Openness::Closed },
    TableDef { path: "patch.*", doc: "Replace a dependency everywhere it appears in the graph.", keys: &[], open: Openness::Dependencies },
    TableDef { path: "lints.*", doc: "Lint levels for one tool (`rust`, `clippy`, `rustdoc`).", keys: &[], open: Openness::Free },
    TableDef { path: "lints", doc: "Lint levels. `workspace = true` inherits the workspace's.", keys: &[], open: Openness::Free },
    TableDef { path: "badges", doc: "Deprecated. crates.io no longer renders these.", keys: &[], open: Openness::Free },
];

/// The tables offered when completing a `[header]`, in the order they are offered.
///
/// Not simply [`TABLES`]: the pattern paths cannot be typed as-is (`profile.*` is not a table
/// name), so each is offered with a plausible name filled in, and the array-of-tables ones are
/// offered in their `[[…]]` form. Ordered by how often a manifest gains one.
pub const HEADER_SUGGESTIONS: &[(&str, &str)] = &[
    ("dependencies", "What the crate needs to build."),
    ("dev-dependencies", "Needed by tests, examples and benchmarks only."),
    ("build-dependencies", "Needed by the build script only."),
    ("features", "The crate's optional features and what each turns on."),
    ("package", "Who this crate is: name, version, edition, metadata."),
    ("lib", "The library target, when it is not the conventional one."),
    ("[bin]", "One binary target (an array of tables)."),
    ("[example]", "One example target (an array of tables)."),
    ("[test]", "One integration-test target (an array of tables)."),
    ("[bench]", "One benchmark target (an array of tables)."),
    ("profile.release", "Compiler settings for release builds."),
    ("profile.dev", "Compiler settings for debug builds."),
    ("workspace", "The workspace: its members and what they inherit."),
    ("workspace.package", "Package fields members inherit with `x.workspace = true`."),
    ("workspace.dependencies", "Versions members inherit with `dep.workspace = true`."),
    ("lints.rust", "Lint levels for rustc."),
    ("lints.clippy", "Lint levels for clippy."),
    ("target.'cfg(unix)'.dependencies", "Dependencies only for platforms matching the `cfg`."),
    ("patch.crates-io", "Replace a dependency everywhere it appears in the graph."),
];

/// Reduce a path as written to the one [`TABLES`] uses, replacing free-form segments with `*`.
///
/// `target.'cfg(windows)'.dependencies` → `target.*.dependencies`;
/// `profile.release.package.serde` → `profile.*.package.*`;
/// anything under `package.metadata` → `package.metadata`, because everything below it is the
/// user's and must not be walked into.
pub fn canonical_path(path: &str) -> String {
    let segs: Vec<&str> = path.split('.').collect();
    match segs.as_slice() {
        // The metadata subtrees swallow everything below them.
        ["package", "metadata", ..] => "package.metadata".to_string(),
        ["workspace", "metadata", ..] => "workspace.metadata".to_string(),
        ["workspace", "lints", ..] => "workspace.lints".to_string(),
        // `[dependencies.serde]` — the long form of one dependency spec. Reported as the spec
        // table so its keys are checked against DEP_KEYS.
        ["dependencies", _] => "dependencies.*".to_string(),
        ["dev-dependencies", _] => "dev-dependencies.*".to_string(),
        ["build-dependencies", _] => "build-dependencies.*".to_string(),
        ["workspace", "dependencies", _] => "workspace.dependencies.*".to_string(),
        ["target", _, kind] => format!("target.*.{kind}"),
        ["target", _, kind, _] => format!("target.*.{kind}.*"),
        ["profile", _] => "profile.*".to_string(),
        ["profile", _, "build-override"] => "profile.*.build-override".to_string(),
        ["profile", _, "package", _] => "profile.*.package.*".to_string(),
        ["patch", _] => "patch.*".to_string(),
        ["patch", _, _] => "patch.*.*".to_string(),
        ["lints", _] => "lints.*".to_string(),
        // A feature's value is an array, never a table, so `[features.foo]` is not a shape —
        // but `[lints]` with `workspace = true` is, and both fall through to the literal lookup.
        _ => path.to_string(),
    }
}

/// The definition of the table at `path`, or `None` for one this schema does not know.
///
/// `None` is what silences the validator for everything inside — see the module doc. It is
/// deliberately the answer for a table under an unknown one too, since `canonical_path` leaves it
/// alone and no entry matches.
pub fn table_def(path: &str) -> Option<&'static TableDef> {
    let canon = canonical_path(path);
    // The long dependency-spec forms all share DEP_KEYS and are not in TABLES (they would be
    // five identical rows), so they are answered here.
    if let Some(def) = dep_spec_table(&canon) {
        return Some(def);
    }
    TABLES.iter().find(|t| t.path == canon)
}

/// The `[dependencies.<name>]` long form, as a closed table over [`DEP_KEYS`].
fn dep_spec_table(canon: &str) -> Option<&'static TableDef> {
    /// One shared definition — the path is a lie in the sense that no single string covers all
    /// five, but nothing reads `TableDef::path` after resolution.
    static DEP_SPEC: TableDef = TableDef {
        path: "dependencies.*",
        doc: "One dependency, spelled out.",
        keys: DEP_KEYS,
        open: Openness::Closed,
    };
    matches!(
        canon,
        "dependencies.*"
            | "dev-dependencies.*"
            | "build-dependencies.*"
            | "workspace.dependencies.*"
            | "target.*.dependencies.*"
            | "target.*.dev-dependencies.*"
            | "target.*.build-dependencies.*"
            | "patch.*.*"
    )
    .then_some(&DEP_SPEC)
}

/// Whether `path` is a table whose keys are dependency names.
pub fn is_dependency_table(path: &str) -> bool {
    table_def(path).is_some_and(|d| d.open == Openness::Dependencies)
}

/// The editions Cargo accepts — also the list the validator checks `edition` against.
pub fn editions() -> &'static [&'static str] {
    EDITIONS
}

/// The crate types a `crate-type` array may hold.
pub fn crate_types() -> &'static [&'static str] {
    CRATE_TYPES
}

/// The lint levels a `[lints.*]` value may be.
pub fn lint_levels() -> &'static [&'static str] {
    LINT_LEVELS
}

/// The keys of a lint's long form (`{ level = "warn", priority = -1 }`).
pub fn lint_keys() -> &'static [KeyDef] {
    LINT_KEYS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_common_tables_resolve() {
        assert_eq!(table_def("package").map(|t| t.open), Some(Openness::Closed));
        assert_eq!(table_def("dependencies").map(|t| t.open), Some(Openness::Dependencies));
        assert_eq!(table_def("features").map(|t| t.open), Some(Openness::Free));
    }

    #[test]
    fn free_form_segments_collapse_to_the_pattern() {
        assert_eq!(canonical_path("target.cfg(unix).dependencies"), "target.*.dependencies");
        assert_eq!(canonical_path("profile.release"), "profile.*");
        assert_eq!(canonical_path("profile.release.package.serde"), "profile.*.package.*");
        assert_eq!(canonical_path("profile.dev.build-override"), "profile.*.build-override");
        assert_eq!(canonical_path("patch.crates-io"), "patch.*");
        assert_eq!(canonical_path("lints.clippy"), "lints.*");
        // A custom profile name is still a profile.
        assert!(table_def("profile.release-lto").is_some_and(|t| t.key("lto").is_some()));
    }

    #[test]
    fn a_metadata_subtree_is_free_all_the_way_down() {
        // Everything under it must be Free — a tool's own nested tables are not our business,
        // and walking into them would flag every key of every external tool's config.
        for path in ["package.metadata", "package.metadata.docs.rs", "workspace.metadata.release"] {
            assert_eq!(table_def(path).map(|t| t.open), Some(Openness::Free), "{path}");
        }
    }

    #[test]
    fn the_long_dependency_form_is_checked_against_the_spec_keys() {
        let def = table_def("dependencies.serde").expect("the long form resolves");
        assert!(def.key("version").is_some());
        assert!(def.key("features").is_some());
        // And a typo in it is catchable, which is the whole point.
        assert!(def.key("feature").is_none());
        assert_eq!(def.open, Openness::Closed);
        // Same for the target-specific and workspace variants.
        assert!(table_def("target.cfg(unix).dependencies.libc").is_some());
        assert!(table_def("workspace.dependencies.serde").is_some());
    }

    #[test]
    fn an_unknown_table_resolves_to_nothing_so_nothing_inside_is_flagged() {
        assert!(table_def("nonsense").is_none());
        assert!(table_def("nonsense.deeper").is_none());
    }

    /// `edition` is inheritable and `name` is not, and a validator that confused the two would
    /// either flag `edition.workspace = true` or wave through `name.workspace = true`.
    #[test]
    fn inheritance_is_per_key() {
        let pkg = table_def("package").unwrap();
        assert!(pkg.key("edition").unwrap().inheritable);
        assert!(pkg.key("version").unwrap().inheritable);
        assert!(!pkg.key("name").unwrap().inheritable);
        assert!(!pkg.key("build").unwrap().inheritable);
    }

    /// Every header suggestion has to resolve, or completion offers a table validation then
    /// flags — the exact contradiction this module exists to prevent.
    #[test]
    fn every_header_suggestion_is_a_table_the_schema_knows() {
        for (name, _) in HEADER_SUGGESTIONS {
            // The `[[array]]` form is offered with its extra brackets; strip them to resolve.
            let path = name.trim_start_matches('[').trim_end_matches(']');
            // A quoted `cfg(...)` segment is normalised by the manifest reader before lookup.
            let path = path.replace('\'', "");
            assert!(table_def(&path).is_some(), "{name} is offered but unknown to the schema");
        }
    }
}
