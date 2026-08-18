//! The dependencies a manifest **declares**, as written.
//!
//! Its own module because four different things need the same reading and must not each do it:
//! validation (a feature referring to a dependency that is not there), the crate graph, the
//! dependency panel's report, and — for the workspace root — the versions members inherit.
//!
//! ## Both spellings, one shape
//!
//! Cargo lets a dependency be written three ways, and a reader that only understood the first
//! would silently miss half of a real manifest:
//!
//! ```toml
//! serde = "1"                                   # short
//! serde = { version = "1", optional = true }    # inline table
//! [dependencies.serde]                          # long form
//! version = "1"
//! ```
//!
//! All three land in one [`DeclaredDep`], with the span of the *name* — so a panel row and a
//! diagnostic both have somewhere to jump to.
//!
//! ## What is not here
//!
//! Resolution. Nothing below asks what version was actually chosen, whether the crate is in the
//! local registry, or what it drags in — those need `Cargo.lock` and the cargo home, and they live
//! with the consumer that wants them. This module reads the manifest and stops.

use crate::manifest::{Entry, Manifest};

/// Which dependency table a declaration came from.
///
/// Cargo's three kinds, which are three genuinely different times a crate is needed: to build the
/// library, to build its tests, to build its build script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

impl DepKind {
    /// The wire/display name — `normal` · `dev` · `build`. Cargo's own words, and the analogue of
    /// a Maven scope.
    pub fn as_str(self) -> &'static str {
        match self {
            DepKind::Normal => "normal",
            DepKind::Dev => "dev",
            DepKind::Build => "build",
        }
    }

    /// The kind a dependency-table segment names, or `None` when it is not one.
    fn from_segment(segment: &str) -> Option<DepKind> {
        match segment {
            "dependencies" => Some(DepKind::Normal),
            "dev-dependencies" => Some(DepKind::Dev),
            "build-dependencies" => Some(DepKind::Build),
            _ => None,
        }
    }
}

/// One dependency, exactly as the manifest declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredDep {
    /// The key as written. For a renamed dependency this is the **local** name — the one used in
    /// `use` statements and in feature references, which is the one every consumer matches on.
    pub name: String,
    /// The real crate name: `package = "…"` when the entry renames it, else the same as
    /// [`DeclaredDep::name`].
    pub package: String,
    pub kind: DepKind,
    /// The `cfg(…)` of a `[target.'…'.dependencies]` table, empty for an unconditional one.
    ///
    /// Worth carrying rather than flattening: a target-specific dependency is only on the graph
    /// for platforms that match, which is the Cargo analogue of a Maven profile.
    pub target: String,
    /// The version requirement as written (`1`, `^1.2`, `>=1, <3`), empty when the entry has none
    /// — a `path` / `git` / workspace-inherited dependency.
    pub req: String,
    /// `path = "…"` as written, relative to the manifest's directory.
    pub path: String,
    /// `git = "…"` as written.
    pub git: String,
    /// The `branch` / `tag` / `rev` pinning the git source, empty when there is none.
    pub git_ref: String,
    /// Features enabled on the dependency.
    pub features: Vec<String>,
    /// Whether the dependency's own default features are on. `true` unless the entry says
    /// otherwise, which is Cargo's default made explicit.
    pub default_features: bool,
    /// `optional = true` — only built when a feature turns it on, and (unless `dep:` syntax is
    /// used anywhere) it also defines an implicit feature of the same name.
    pub optional: bool,
    /// `workspace = true` — version and features come from `[workspace.dependencies]`.
    pub from_workspace: bool,
    /// `registry = "…"`, empty for crates.io.
    pub registry: String,
    /// Byte offset of the dependency's name in the manifest.
    pub offset: usize,
    /// 1-based line of the dependency's name.
    pub line: u32,
    /// The table it was declared in, as written.
    pub table: String,
    /// Whether the declaration is syntactically **finished**.
    ///
    /// It exists for the validator. `serde = ` and `serde = {` are both declarations of `serde`
    /// with no source, and both are states every dependency passes through while being typed — so
    /// the checks that ask "is this spec complete and wrong" have to be able to tell them from
    /// `serde = {}`, which is finished and wrong. A panel can ignore this; a squiggle cannot.
    pub complete: bool,
    /// Byte span of the version **value as written**, quotes included — `0..0` when the entry has
    /// no version.
    ///
    /// Carried so a caller can *rewrite* it: "there is a newer release of this crate" is only
    /// actionable if the thing to replace can be pointed at, and the span covers the quotes so the
    /// replacement is a complete TOML value rather than the inside of one.
    pub req_start: usize,
    pub req_end: usize,
}

impl DeclaredDep {
    /// Where the crate comes from, in one word: `workspace` · `path` · `git` · a registry name ·
    /// `crates.io`.
    ///
    /// The order is the precedence Cargo itself applies, so the label is never a guess about
    /// which source wins.
    pub fn source(&self) -> &str {
        if self.from_workspace {
            "workspace"
        } else if !self.path.is_empty() {
            "path"
        } else if !self.git.is_empty() {
            "git"
        } else if !self.registry.is_empty() {
            &self.registry
        } else {
            "crates.io"
        }
    }

    /// Whether the entry names no source at all — which Cargo refuses to build.
    pub fn has_no_source(&self) -> bool {
        self.req.is_empty()
            && self.path.is_empty()
            && self.git.is_empty()
            && !self.from_workspace
    }

    /// Whether this entry renames the crate (`json = { package = "serde_json" }`).
    pub fn is_renamed(&self) -> bool {
        self.package != self.name
    }
}

/// Every dependency the manifest declares, in source order.
///
/// Order is by table then by position, which is the order they appear in the file — a panel that
/// reshuffled them between two reads of the same manifest would read as a change.
pub fn declared(m: &Manifest) -> Vec<DeclaredDep> {
    let mut out: Vec<DeclaredDep> = Vec::new();

    // The short and inline-table forms: one entry per dependency, inside a dependency table.
    for entry in &m.entries {
        let Some((kind, target)) = dependency_table(&entry.table) else { continue };
        let name = entry.base_key();
        // A dotted key is one spec key of the dependency its base names: `tracing.workspace =
        // true` is the dependency `tracing`, and a second dotted key beside it
        // (`tracing.features = […]`) is another key of the SAME dependency, not a second one.
        if let Some(existing) =
            out.iter_mut().find(|d| d.name == name && d.kind == kind && d.target == target)
        {
            if let Some(suffix) = entry.key_suffix() {
                apply_spec_key_at(existing, suffix, &entry.value, entry.value_start, entry.value_end);
            }
            continue;
        }
        out.push(from_entry(entry, kind, target));
    }

    // The long form: `[dependencies.serde]` is a TABLE whose keys are the spec.
    for table in &m.tables {
        let Some((parent, name)) = split_last(&table.path) else { continue };
        let Some((kind, target)) = dependency_table(parent) else { continue };
        if name.is_empty() {
            continue;
        }
        // A `[dependencies.serde]` table and a `serde = …` key in the same manifest is a
        // duplicate the validator reports; here the first wins, so the panel shows one row.
        if out.iter().any(|d| d.name == name && d.kind == kind && d.target == target) {
            continue;
        }
        out.push(from_long_form(m, &table.path, name, kind, target, table.start, table.line));
    }

    out.sort_by_key(|d| d.offset);
    out
}

/// The kind and `cfg(…)` of a dependency table, or `None` when the path is not one.
///
/// `[workspace.dependencies]` is deliberately included: the versions a workspace root declares
/// for its members are dependencies, and the inheritance check needs to read them the same way.
fn dependency_table(path: &str) -> Option<(DepKind, String)> {
    let segs: Vec<&str> = path.split('.').collect();
    match segs.as_slice() {
        [one] => DepKind::from_segment(one).map(|k| (k, String::new())),
        ["workspace", "dependencies"] => Some((DepKind::Normal, String::new())),
        // `target.cfg(unix).dependencies` — the manifest reader has already unquoted the middle
        // segment, and a `cfg` expression containing a dot is not a shape that exists.
        ["target", cfg, kind] => DepKind::from_segment(kind).map(|k| (k, (*cfg).to_string())),
        _ => None,
    }
}

/// Split `a.b.c` into `("a.b", "c")`.
fn split_last(path: &str) -> Option<(&str, &str)> {
    path.rfind('.').map(|i| (&path[..i], &path[i + 1..]))
}

/// The short (`serde = "1"`) and inline-table (`serde = { … }`) forms.
fn from_entry(entry: &Entry, kind: DepKind, target: String) -> DeclaredDep {
    let name = entry.base_key().to_string();
    let mut dep = DeclaredDep {
        package: name.clone(),
        name,
        kind,
        target,
        req: String::new(),
        path: String::new(),
        git: String::new(),
        git_ref: String::new(),
        features: Vec::new(),
        default_features: true,
        optional: false,
        from_workspace: false,
        registry: String::new(),
        offset: entry.key_start,
        line: entry.line,
        table: entry.table.clone(),
        complete: false,
        req_start: 0,
        req_end: 0,
    };

    // A dotted key carries exactly one spec key.
    if let Some(suffix) = entry.key_suffix() {
        dep.complete = !entry.value.trim().is_empty();
        apply_spec_key_at(&mut dep, suffix, &entry.value, entry.value_start, entry.value_end);
        return dep;
    }
    if let Some(req) = entry.str_value() {
        // The short form: the whole value is the requirement, so the value's own span is it.
        dep.req = req.to_string();
        dep.req_start = entry.value_start;
        dep.req_end = entry.value_end;
        dep.complete = true;
        return dep;
    }
    // An inline table is finished only once it is closed — `serde = {` is not `serde = {}`.
    dep.complete = entry.value.trim_end().ends_with('}');
    for k in entry.inline_keys() {
        apply_spec_key_at(&mut dep, &k.key, &k.value, k.value_start, k.value_end);
    }
    dep
}

/// The long form: `[dependencies.serde]` plus its keys.
fn from_long_form(
    m: &Manifest,
    table: &str,
    name: &str,
    kind: DepKind,
    target: String,
    offset: usize,
    line: u32,
) -> DeclaredDep {
    let mut dep = DeclaredDep {
        name: name.to_string(),
        package: name.to_string(),
        kind,
        target,
        req: String::new(),
        path: String::new(),
        git: String::new(),
        git_ref: String::new(),
        features: Vec::new(),
        default_features: true,
        optional: false,
        from_workspace: false,
        registry: String::new(),
        offset,
        line,
        table: table.to_string(),
        // A `[dependencies.serde]` header IS the whole declaration: its keys are separate
        // assignments, so there is no half-written spec to wait for.
        complete: true,
        req_start: 0,
        req_end: 0,
    };
    for entry in m.entries_in(table) {
        apply_spec_key_at(&mut dep, &entry.key, &entry.value, entry.value_start, entry.value_end);
    }
    dep
}

/// Apply one spec key to a dependency, and record **where its value is**.
///
/// Unknown keys are ignored — the validator reports them, and a reader that refused them would make
/// a typo cost the whole row.
///
/// The span is only kept for `version`, because that is the only spec key anything rewrites. Passing
/// `0, 0` means "the position is unknown", which reads as "not actionable" downstream.
fn apply_spec_key_at(
    dep: &mut DeclaredDep,
    key: &str,
    raw_value: &str,
    value_start: usize,
    value_end: usize,
) {
    let value = unquote(raw_value);
    match key {
        "version" => {
            dep.req = value.to_string();
            dep.req_start = value_start;
            dep.req_end = value_end;
        }
        "path" => dep.path = value.to_string(),
        "git" => dep.git = value.to_string(),
        "branch" | "tag" | "rev" => dep.git_ref = format!("{key} {value}"),
        "package" => dep.package = value.to_string(),
        "registry" => dep.registry = value.to_string(),
        "optional" => dep.optional = raw_value.trim() == "true",
        "workspace" => dep.from_workspace = raw_value.trim() == "true",
        "default-features" | "default_features" => dep.default_features = raw_value.trim() != "false",
        "features" => dep.features = quoted_items(raw_value),
        _ => {}
    }
}

/// Strip one pair of surrounding quotes.
fn unquote(s: &str) -> &str {
    let s = s.trim();
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            return inner;
        }
    }
    s
}

/// The quoted strings inside `raw` (an array literal), in order.
fn quoted_items(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            let from = i + 1;
            i += 1;
            while i < bytes.len() && bytes[i] != b {
                i += 1;
            }
            if i < bytes.len() {
                out.push(raw[from..i].to_string());
                i += 1;
                continue;
            }
            break;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
[package]
name = "demo"

[dependencies]
serde = { version = "1", features = ["derive"], optional = true }
anyhow = "1.0"
local = { path = "../local" }
tracing.workspace = true
json = { package = "serde_json", version = "1" }

[dependencies.reqwest]
version = "0.12"
default-features = false
features = ["json", "rustls-tls"]

[dev-dependencies]
tempfile = "3"

[build-dependencies]
cc = "1"

[target.'cfg(unix)'.dependencies]
libc = "0.2"
"#;

    fn dep<'a>(deps: &'a [DeclaredDep], name: &str) -> &'a DeclaredDep {
        deps.iter().find(|d| d.name == name).unwrap_or_else(|| panic!("no dependency {name}"))
    }

    #[test]
    fn the_version_span_points_at_the_quoted_value_in_every_spelling() {
        // What makes "a newer release exists" actionable: the span has to be the whole TOML value,
        // quotes included, so replacing it leaves valid TOML — in all three spellings, since a
        // manifest mixes them freely.
        let deps = declared(&Manifest::parse(MANIFEST));
        let span_text = |name: &str| {
            let d = dep(&deps, name);
            &MANIFEST[d.req_start..d.req_end]
        };
        assert_eq!(span_text("anyhow"), "\"1.0\"", "short form");
        assert_eq!(span_text("serde"), "\"1\"", "inline table");
        assert_eq!(span_text("reqwest"), "\"0.12\"", "long form");
        assert_eq!(span_text("libc"), "\"0.2\"", "target-specific");

        // A dependency with no version has no span, which reads downstream as "not actionable" —
        // there is nothing to rewrite on a path or an inherited dependency.
        assert_eq!(dep(&deps, "local").req_start, 0);
        assert_eq!(dep(&deps, "tracing").req_end, 0);
    }

    #[test]
    fn all_three_spellings_land_in_one_shape() {
        let deps = declared(&Manifest::parse(MANIFEST));

        // Short form.
        assert_eq!(dep(&deps, "anyhow").req, "1.0");
        assert_eq!(dep(&deps, "anyhow").source(), "crates.io");

        // Inline table.
        let serde = dep(&deps, "serde");
        assert_eq!(serde.req, "1");
        assert_eq!(serde.features, vec!["derive"]);
        assert!(serde.optional);
        assert!(serde.default_features, "not saying is Cargo's `true`");

        // Long form.
        let reqwest = dep(&deps, "reqwest");
        assert_eq!(reqwest.req, "0.12");
        assert!(!reqwest.default_features);
        assert_eq!(reqwest.features, vec!["json", "rustls-tls"]);
    }

    #[test]
    fn the_kind_comes_from_the_table_and_a_cfg_is_kept() {
        let deps = declared(&Manifest::parse(MANIFEST));
        assert_eq!(dep(&deps, "serde").kind, DepKind::Normal);
        assert_eq!(dep(&deps, "tempfile").kind, DepKind::Dev);
        assert_eq!(dep(&deps, "cc").kind, DepKind::Build);
        let libc = dep(&deps, "libc");
        assert_eq!(libc.kind, DepKind::Normal);
        assert_eq!(libc.target, "cfg(unix)");
    }

    #[test]
    fn the_source_is_named_by_precedence() {
        let deps = declared(&Manifest::parse(MANIFEST));
        assert_eq!(dep(&deps, "local").source(), "path");
        assert_eq!(dep(&deps, "tracing").source(), "workspace");
        assert!(dep(&deps, "tracing").from_workspace);
    }

    /// A renamed dependency keeps BOTH names: the local one is what a feature reference and a
    /// `use` see, the real one is what is fetched, and a panel showing only one of them is
    /// answering the wrong question.
    #[test]
    fn a_renamed_dependency_keeps_both_names() {
        let deps = declared(&Manifest::parse(MANIFEST));
        let json = dep(&deps, "json");
        assert_eq!(json.package, "serde_json");
        assert!(json.is_renamed());
        assert!(!dep(&deps, "serde").is_renamed());
    }

    #[test]
    fn a_dotted_workspace_key_is_a_dependency_not_a_table() {
        let deps = declared(&Manifest::parse(MANIFEST));
        // `tracing.workspace = true` is the key `tracing.workspace`, and the dependency is
        // `tracing` — reading it as a dependency called `tracing.workspace` would be a row
        // pointing at a crate that does not exist.
        assert!(deps.iter().all(|d| !d.name.contains('.')), "no dependency has a dotted name");
        assert!(dep(&deps, "tracing").from_workspace);
    }

    #[test]
    fn a_spec_with_no_source_is_recognised() {
        let deps = declared(&Manifest::parse("[dependencies]\nserde = { optional = true }\n"));
        assert!(deps[0].has_no_source());
        assert!(deps[0].complete);
        assert!(!declared(&Manifest::parse("[dependencies]\nserde = \"1\"\n"))[0].has_no_source());
    }

    /// The states a dependency passes through while it is typed. Each is a declaration with no
    /// source, and none of them is finished — which is what stops the validator flagging every
    /// keystroke.
    #[test]
    fn a_half_written_spec_is_not_complete() {
        for text in ["[dependencies]\nserde = ", "[dependencies]\nserde = {", "[dependencies]\nserde = { version = \"1\""] {
            let deps = declared(&Manifest::parse(text));
            assert_eq!(deps.len(), 1, "{text:?}");
            assert!(!deps[0].complete, "{text:?} should not read as finished");
        }
        // …and `serde = "1` — an unterminated string — is not a requirement either.
        let deps = declared(&Manifest::parse("[dependencies]\nserde = \"1"));
        assert!(deps[0].req.is_empty());
        assert!(!deps[0].complete);
    }

    #[test]
    fn workspace_dependencies_are_read_as_dependencies() {
        let m = Manifest::parse("[workspace.dependencies]\nserde = { version = \"1\" }\n");
        let deps = declared(&m);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "serde");
        assert_eq!(deps[0].req, "1");
    }

    #[test]
    fn declarations_come_back_in_source_order() {
        let deps = declared(&Manifest::parse(MANIFEST));
        let offsets: Vec<usize> = deps.iter().map(|d| d.offset).collect();
        let mut sorted = offsets.clone();
        sorted.sort_unstable();
        assert_eq!(offsets, sorted);
    }
}
