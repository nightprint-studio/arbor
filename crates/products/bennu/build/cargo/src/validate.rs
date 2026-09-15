//! Diagnostics for a `Cargo.toml`.
//!
//! ## What earns a diagnostic
//!
//! Only things that are **wrong**, and mostly things Cargo itself would tell you about — but at the
//! moment you type them rather than at the next build. The ones that pay for the whole module:
//!
//! - a **key typo** (`dependancies`, `feature = [...]`) — silently ignored by Cargo, so the
//!   symptom is a dependency that is not there or a feature that does nothing, and the manifest
//!   looks fine;
//! - a **feature referring to something that does not exist** — Cargo *does* refuse this, and it
//!   is the most common way a manifest breaks after a rename;
//! - a **`workspace = true` with nothing behind it** — the version was supposed to come from a
//!   `[workspace.dependencies]` entry that was never added;
//! - a **`path` or a member that is not on disk** — the shape of a half-finished move.
//!
//! ## Severity means something
//!
//! `error` is reserved for what Cargo genuinely **refuses to build**. Everything else is a
//! `warning`, including every unknown key: Cargo warns rather than failing on those, it gains keys
//! every few releases, and a Bennu one version behind must not paint red over a good manifest.
//!
//! ## False positives are the failure mode
//!
//! A manifest is edited character by character, so every check has to be silent about a shape it
//! does not recognise rather than suspicious of it. Concretely: an unknown *table* silences
//! everything inside it, a value whose kind cannot be told is not checked, a spec that is not
//! syntactically finished ([`DeclaredDep::complete`]) is not judged, and every filesystem check is
//! skipped when no directory was supplied. Being quiet costs a missed problem; being wrong costs
//! the user's trust in every squiggle in the editor.
//!
//! [`DeclaredDep::complete`]: crate::deps::DeclaredDep::complete

use std::path::{Path, PathBuf};

use bennu_proto::prelude::Diagnostic;

use crate::deps::{declared, DepKind};
use crate::manifest::{Entry, Manifest};
use crate::schema::{self, Openness, ValueKind};

/// What the validator knows besides the text — all optional, and each check that needs a field is
/// skipped when it is absent.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The directory holding this manifest. Enables the on-disk checks (`path` dependencies,
    /// `members`).
    pub dir: Option<PathBuf>,
    /// The workspace root's manifest, when this one is a member. Enables the inheritance checks —
    /// `x.workspace = true` naming something the root does not declare.
    pub workspace: Option<Manifest>,
}

/// How far up the tree a workspace root is looked for.
const MAX_ANCESTORS: usize = 12;

impl Context {
    /// A context for the manifest at `path`, resolving the workspace root by walking up.
    ///
    /// Reads at most a handful of small files: the walk stops at the first ancestor whose
    /// `Cargo.toml` declares a `[workspace]`, and gives up after [`MAX_ANCESTORS`] levels.
    pub fn for_file(path: &Path) -> Context {
        let dir = path.parent().map(Path::to_path_buf);
        let workspace = dir
            .as_deref()
            .and_then(find_workspace_root)
            // Not the manifest being validated: a root that validated itself would report each of
            // its own inheritance sources as missing from itself.
            .filter(|root| Some(root.as_path()) != dir.as_deref())
            .and_then(|root| std::fs::read_to_string(root.join("Cargo.toml")).ok())
            .map(|text| Manifest::parse(&text));
        Context { dir, workspace }
    }
}

/// The nearest ancestor (including `from`) whose `Cargo.toml` declares a `[workspace]`.
fn find_workspace_root(from: &Path) -> Option<PathBuf> {
    let mut dir = Some(from);
    for _ in 0..MAX_ANCESTORS {
        let d = dir?;
        if let Ok(text) = std::fs::read_to_string(d.join("Cargo.toml")) {
            if Manifest::parse(&text).has_table("workspace") {
                return Some(d.to_path_buf());
            }
        }
        dir = d.parent();
    }
    None
}

/// Validate the manifest `text`. Never fails; an empty vector is a clean manifest.
pub fn validate(text: &str, ctx: &Context) -> Vec<Diagnostic> {
    let m = Manifest::parse(text);
    let mut out = Vec::new();

    check_identity(&m, text, &mut out);
    check_duplicates(&m, &mut out);
    check_tables_and_keys(&m, &mut out);
    check_inheritance(&m, ctx, &mut out);
    check_dependencies(&m, ctx, &mut out);
    check_features(&m, &mut out);
    check_default_members(&m, &mut out);
    check_members(&m, ctx, &mut out);

    // By position, so the Problems panel reads down the file and the editor's first squiggle is
    // the first problem.
    out.sort_by_key(|d| (d.start, d.end));
    out
}

/// Validate the manifest at `path`, resolving its workspace root itself.
pub fn validate_file(path: &Path, text: &str) -> Vec<Diagnostic> {
    validate(text, &Context::for_file(path))
}

// ── the checks ─────────────────────────────────────────────────────────────────

/// A manifest is a package, a workspace, or both. Neither is the one thing Cargo cannot use.
fn check_identity(m: &Manifest, text: &str, out: &mut Vec<Diagnostic>) {
    if m.has_table("package") {
        // `name` is the only genuinely required key: `version` has defaulted to `0.0.0` for
        // several releases now, so demanding it would flag manifests that build.
        if m.get_base("package", "name").is_none() {
            let t = m.table("package").expect("just checked");
            out.push(err(
                "cargo-missing-name",
                "`[package]` has no `name` — Cargo cannot build this crate.",
                t.start,
                t.end,
            ));
        }
        return;
    }
    if m.has_table("workspace") {
        return;
    }
    // Nothing structural yet — a file being created, which must not be scolded.
    if m.tables.is_empty() && m.entries.is_empty() {
        return;
    }
    let end = text.find('\n').unwrap_or(text.len());
    out.push(err("cargo-no-package", "A manifest needs a `[package]` or a `[workspace]` table.", 0, end));
}

/// A duplicate key or table is a TOML parse error — Cargo will not read the file at all.
fn check_duplicates(m: &Manifest, out: &mut Vec<Diagnostic>) {
    // Every path declared with `[[…]]`. Its entries share one table path but belong to different
    // elements, so `name` twice across two `[[bin]]` blocks is not a duplicate — it is two
    // binaries. Checked per element below.
    let array_paths: Vec<&str> =
        m.tables.iter().filter(|t| t.array).map(|t| t.path.as_str()).collect();

    for (i, e) in m.entries.iter().enumerate() {
        if array_paths.contains(&e.table.as_str()) {
            continue;
        }
        if m.entries[..i].iter().any(|p| p.table == e.table && p.key == e.key) {
            out.push(err(
                "cargo-duplicate-key",
                &format!("`{}` is already set in this table.", e.key),
                e.key_start,
                e.key_end,
            ));
        }
    }
    for path in {
        let mut unique = array_paths.clone();
        unique.dedup();
        unique
    } {
        for element in m.array_elements(path) {
            for (i, e) in element.iter().enumerate() {
                if element[..i].iter().any(|p| p.key == e.key) {
                    out.push(err(
                        "cargo-duplicate-key",
                        &format!("`{}` is already set in this table.", e.key),
                        e.key_start,
                        e.key_end,
                    ));
                }
            }
        }
    }

    for (i, t) in m.tables.iter().enumerate() {
        // An array-of-tables repeats its header by design — that is what makes it an array.
        if t.array {
            continue;
        }
        if m.tables[..i].iter().any(|p| !p.array && p.path == t.path) {
            out.push(err(
                "cargo-duplicate-table",
                &format!("`[{}]` is declared twice.", t.path),
                t.start,
                t.end,
            ));
        }
    }
}

/// Unknown tables, unknown keys, and values of the wrong shape.
fn check_tables_and_keys(m: &Manifest, out: &mut Vec<Diagnostic>) {
    /// The first segments a manifest's tables may start with. A table starting with anything else
    /// is a typo worth naming — the check that catches `[dependancies]`.
    const ROOTS: &[&str] = &[
        "package", "workspace", "dependencies", "dev-dependencies", "build-dependencies",
        "target", "features", "lib", "bin", "example", "test", "bench", "profile", "patch",
        "replace", "lints", "badges",
    ];

    for t in &m.tables {
        let head = t.path.split('.').next().unwrap_or("");
        if !ROOTS.contains(&head) {
            out.push(warn(
                "cargo-unknown-table",
                &format!("Cargo does not know a `[{head}]` table — everything in it is ignored."),
                t.start,
                t.end,
            ));
        }
    }

    for e in &m.entries {
        // An unknown table is silent about its contents: see the module doc.
        let Some(def) = schema::table_def(&e.table) else { continue };
        match def.open {
            Openness::Free => continue,
            Openness::Dependencies => {
                check_dep_entry(e, out);
                continue;
            }
            Openness::Closed => {}
        }
        let base = e.base_key();
        let Some(key) = def.key(base) else {
            out.push(warn(
                "cargo-unknown-key",
                &format!("`{base}` is not a key of `[{}]` — Cargo ignores it.", e.table),
                e.key_start,
                e.key_end,
            ));
            continue;
        };
        // `x.workspace = true` — legal only for the keys Cargo lets a member inherit.
        if e.key_suffix() == Some("workspace") {
            if !key.inheritable {
                out.push(warn(
                    "cargo-not-inheritable",
                    &format!("`{base}` cannot be inherited from the workspace."),
                    e.key_start,
                    e.key_end,
                ));
            }
            continue;
        }
        if let Some(message) = value_problem(key.kind, &e.value) {
            out.push(warn("cargo-bad-value", &message, e.value_start, value_end(e)));
        }
    }
}

/// One entry of a dependency table: the key is a crate name, the value a requirement or a spec.
fn check_dep_entry(e: &Entry, out: &mut Vec<Diagnostic>) {
    // `serde.workspace = true` / `serde.version = "1"` — one spec key written dotted.
    if let Some(suffix) = e.key_suffix() {
        match schema::DEP_KEYS.iter().find(|k| k.name == suffix) {
            None => out.push(warn(
                "cargo-unknown-key",
                &format!("`{suffix}` is not a key of a dependency."),
                e.key_start,
                e.key_end,
            )),
            Some(def) => {
                if let Some(message) = value_problem(def.kind, &e.value) {
                    out.push(warn("cargo-bad-value", &message, e.value_start, value_end(e)));
                }
            }
        }
        return;
    }
    if e.is_inline_table() {
        for k in e.inline_keys() {
            match schema::DEP_KEYS.iter().find(|d| d.name == k.key) {
                None => out.push(warn(
                    "cargo-unknown-key",
                    &format!("`{}` is not a key of a dependency.", k.key),
                    k.start,
                    k.end,
                )),
                Some(def) => {
                    if let Some(message) = value_problem(def.kind, &k.value) {
                        let end = k.value_end.max(k.value_start + 1);
                        out.push(warn("cargo-bad-value", &message, k.value_start, end));
                    }
                }
            }
        }
        return;
    }
    // Anything else must be the version requirement, as a string. An unfinished value is a value
    // being typed.
    if e.value.is_empty() || unfinished(&e.value) {
        return;
    }
    if e.str_value().is_none() {
        out.push(warn(
            "cargo-bad-value",
            "A dependency is a version string or a table of options.",
            e.value_start,
            value_end(e),
        ));
    }
}

/// A value span that is never empty, so a squiggle is visible even on a one-character value.
fn value_end(e: &Entry) -> usize {
    e.value_end.max(e.value_start + 1)
}

/// Whether a raw value is mid-keystroke: an unterminated string, array or inline table.
fn unfinished(raw: &str) -> bool {
    let v = raw.trim();
    match v.as_bytes().first() {
        Some(b'"') => !(v.len() >= 2 && v.ends_with('"')),
        Some(b'\'') => !(v.len() >= 2 && v.ends_with('\'')),
        Some(b'[') => !v.ends_with(']'),
        Some(b'{') => !v.ends_with('}'),
        _ => false,
    }
}

/// `raw` as a plain string, when it is one.
fn as_str(raw: &str) -> Option<&str> {
    let v = raw.trim();
    for q in ['"', '\''] {
        if let Some(inner) = v.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            if !inner.contains(q) {
                return Some(inner);
            }
        }
    }
    None
}

/// Whether the value contradicts the schema's kind, and how to say so.
///
/// Conservative on purpose: `None` for anything it cannot be sure about — an empty value, an
/// unfinished one, or a bare word (which is not valid TOML at all, and reporting it as the wrong
/// *kind* would be a confusing way to say "this is not TOML").
fn value_problem(kind: ValueKind, raw: &str) -> Option<String> {
    let v = raw.trim();
    if v.is_empty() || unfinished(v) {
        return None;
    }
    let is_str = as_str(v).is_some();
    let is_bool = v == "true" || v == "false";
    let is_array = v.starts_with('[');
    let is_table = v.starts_with('{');
    let is_number = v.parse::<i64>().is_ok();

    match kind {
        ValueKind::Str if is_array || is_bool || is_table => Some("Expected a string.".to_string()),
        ValueKind::Bool if !is_bool && (is_str || is_array || is_number) => {
            Some("Expected `true` or `false`.".to_string())
        }
        // A number, or a string like `opt-level = "s"`.
        ValueKind::Int if is_array || is_bool || is_table => Some("Expected a number.".to_string()),
        ValueKind::StrArray if is_str || is_bool || is_table => {
            Some("Expected an array of strings.".to_string())
        }
        ValueKind::Enum(allowed) => {
            // Only a *string* is checked against the set. `debug = 2` and `strip = true` are legal
            // spellings of enum-ish keys, and flagging them would be wrong.
            let value = as_str(v)?;
            (!allowed.contains(&value))
                .then(|| format!("`{value}` is not one of: {}.", allowed.join(", ")))
        }
        _ => None,
    }
}

/// `x.workspace = true` with nothing behind it in the workspace root.
///
/// Only runs when the root manifest was found — see [`Context`]. It is one of the two checks worth
/// the filesystem access: the build error it prevents names the *member*, not the root that is
/// missing the entry, so it is a genuinely hard one to track down by hand.
fn check_inheritance(m: &Manifest, ctx: &Context, out: &mut Vec<Diagnostic>) {
    let Some(root) = &ctx.workspace else { return };

    for e in &m.entries {
        // Both spellings of inheriting one thing: `edition.workspace = true` and
        // `serde = { workspace = true }`.
        let inherits = (e.key_suffix() == Some("workspace") && e.bool_value() == Some(true))
            || (schema::is_dependency_table(&e.table)
                && e.inline_keys().iter().any(|k| k.key == "workspace" && k.value.trim() == "true"));
        if !inherits {
            continue;
        }
        let base = e.base_key();
        let (source, present) = if schema::is_dependency_table(&e.table) {
            ("[workspace.dependencies]", root.get_base("workspace.dependencies", base).is_some())
        } else if e.table == "package" {
            ("[workspace.package]", root.get_base("workspace.package", base).is_some())
        } else {
            continue;
        };
        if !present {
            out.push(err(
                "cargo-workspace-missing",
                &format!("The workspace root has no `{base}` in `{source}`."),
                e.key_start,
                e.key_end,
            ));
        }
    }

    // `[lints] workspace = true` — the same inheritance, spelled as a key of its own table.
    if let Some(e) = m.get("lints", "workspace") {
        if e.bool_value() == Some(true) && !root.has_table("workspace.lints") {
            out.push(err(
                "cargo-workspace-missing",
                "The workspace root has no `[workspace.lints]` to inherit.",
                e.key_start,
                e.key_end,
            ));
        }
    }
}

/// The dependency specs themselves: conflicting sources, orphaned git refs, paths that are not
/// there.
fn check_dependencies(m: &Manifest, ctx: &Context, out: &mut Vec<Diagnostic>) {
    for d in declared(m) {
        let (from, to) = (d.offset, d.offset + d.name.len());

        if !d.git.is_empty() && !d.path.is_empty() {
            out.push(err(
                "cargo-conflicting-source",
                &format!("`{}` gives both a `git` and a `path` — Cargo needs one.", d.name),
                from,
                to,
            ));
        }
        if !d.git_ref.is_empty() && d.git.is_empty() {
            out.push(warn(
                "cargo-orphan-git-ref",
                &format!("`{}` pins a git {} but has no `git` source.", d.name, d.git_ref),
                from,
                to,
            ));
        }
        // Only judged once the spec is finished — see the module doc.
        if d.complete && d.has_no_source() {
            out.push(err(
                "cargo-dep-no-source",
                &format!(
                    "`{}` names no source — give it a `version`, a `path`, a `git`, or `workspace = true`.",
                    d.name
                ),
                from,
                to,
            ));
        }
        if d.optional && d.kind == DepKind::Dev {
            out.push(err("cargo-dev-dep-optional", "A dev-dependency cannot be optional.", from, to));
        }
        if !d.req.is_empty() && !plausible_version_req(&d.req) {
            out.push(warn(
                "cargo-bad-version-req",
                &format!("`{}` is not a version requirement Cargo can read.", d.req),
                from,
                to,
            ));
        }
        // The on-disk check, when we know where the manifest lives.
        if let (Some(dir), false) = (ctx.dir.as_deref(), d.path.is_empty()) {
            if !dir.join(&d.path).join("Cargo.toml").is_file() {
                out.push(warn(
                    "cargo-missing-path",
                    &format!("No crate at `{}` — there is no Cargo.toml there.", d.path),
                    from,
                    to,
                ));
            }
        }
    }
}

/// Whether a version requirement is one Cargo could read.
///
/// Deliberately shallow: `*`, and otherwise anything with a digit in it. Implementing the semver
/// requirement grammar here would be a parser to maintain for the sake of a warning, and the
/// mistakes worth catching are an empty string and a word (`"latest"`).
fn plausible_version_req(req: &str) -> bool {
    let r = req.trim();
    if r.is_empty() {
        return false;
    }
    r == "*" || r.bytes().any(|b| b.is_ascii_digit())
}

/// A feature referring to something that does not exist.
///
/// The most valuable check here, because Cargo *refuses* the manifest over it and the usual cause
/// is a rename: `[features] json = ["dep:serde_json"]` after `serde_json` was renamed.
///
/// The four shapes a feature value takes, all of which must be understood or the check becomes a
/// false-positive machine:
///
/// | Written | Means |
/// |---|---|
/// | `other-feature` | another feature of this crate, or an optional dependency's implicit one |
/// | `dep:serde` | the optional dependency `serde`, without an implicit feature |
/// | `serde/derive` | enable `derive` on the dependency `serde` |
/// | `serde?/derive` | …but only if something else already enabled `serde` |
fn check_features(m: &Manifest, out: &mut Vec<Diagnostic>) {
    if !m.has_table("features") {
        return;
    }
    let deps = declared(m);
    let dep_names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
    let optional: Vec<&str> = deps.iter().filter(|d| d.optional).map(|d| d.name.as_str()).collect();
    let features: Vec<&str> = m.entries_in("features").map(|e| e.key.as_str()).collect();

    for e in m.entries_in("features") {
        for item in &e.items {
            if let Some(message) = feature_ref_problem(&item.text, &features, &dep_names, &optional)
            {
                out.push(err("cargo-unknown-feature-ref", &message, item.start, item.end));
            }
        }
    }
}

/// What is wrong with one feature reference, if anything.
fn feature_ref_problem(
    reference: &str,
    features: &[&str],
    deps: &[&str],
    optional: &[&str],
) -> Option<String> {
    let r = reference.trim();
    if r.is_empty() {
        return None;
    }
    if let Some(dep) = r.strip_prefix("dep:") {
        return (!deps.contains(&dep))
            .then(|| format!("`{dep}` is not a dependency of this crate."));
    }
    if let Some((head, _feature)) = r.split_once('/') {
        // A trailing `?` means "only if already enabled" — it still names a dependency. What
        // features that dependency has is *its* manifest's business, not ours.
        let dep = head.trim_end_matches('?');
        return (!deps.contains(&dep))
            .then(|| format!("`{dep}` is not a dependency of this crate."));
    }
    // A bare name: another feature, or an optional dependency's implicit feature.
    if features.contains(&r) || optional.contains(&r) {
        return None;
    }
    // A non-optional dependency named bare earns its own wording: the fix is `optional = true` or
    // `dep:`, not inventing a feature.
    if deps.contains(&r) {
        return Some(format!(
            "`{r}` is a dependency but not optional, so there is no feature by that name."
        ));
    }
    Some(format!("`{r}` is neither a feature nor an optional dependency of this crate."))
}

/// `default-members` entries that are not in `members`.
///
/// Cargo errors on this, and the cause is always the same: a crate left `members` and the default
/// list still points at it.
fn check_default_members(m: &Manifest, out: &mut Vec<Diagnostic>) {
    let members = m.items_of("workspace", "members");
    let defaults = m.items_of("workspace", "default-members");
    if members.is_empty() || defaults.is_empty() {
        return;
    }
    // A member written as a glob covers paths we would have to match against it, and doing that
    // properly is more than a warning is worth — so a workspace using globs is not checked.
    if members.iter().any(|mem| mem.text.contains('*')) {
        return;
    }
    for item in defaults {
        let wanted = item.text.trim_end_matches('/');
        if !members.iter().any(|mem| mem.text.trim_end_matches('/') == wanted) {
            out.push(err(
                "cargo-default-member-not-member",
                &format!("`{}` is not in `members`.", item.text),
                item.start,
                item.end,
            ));
        }
    }
}

/// A `[workspace] members` entry that matches nothing on disk.
fn check_members(m: &Manifest, ctx: &Context, out: &mut Vec<Diagnostic>) {
    let Some(dir) = ctx.dir.as_deref() else { return };
    if !m.has_table("workspace") {
        return;
    }
    for item in m.items_of("workspace", "members") {
        if !member_exists(dir, &item.text) {
            out.push(warn(
                "cargo-missing-member",
                &format!("`{}` matches no crate under this workspace.", item.text),
                item.start,
                item.end,
            ));
        }
    }
    // `exclude` deliberately not checked: it names paths that need not be crates, or exist.
}

/// Whether a `members` pattern resolves to at least one crate under `dir`.
///
/// Only the trailing-`*` glob is expanded (`crates/*`), which is the shape every real workspace
/// uses. A pattern with an interior `*` is treated as existing: half-understanding a glob and then
/// reporting it missing would be worse than not checking it.
fn member_exists(dir: &Path, pattern: &str) -> bool {
    let pattern = pattern.trim().trim_end_matches('/');
    if pattern.is_empty() {
        return true;
    }
    match pattern.strip_suffix("/*").or_else(|| pattern.strip_suffix('*')) {
        Some(prefix) if !prefix.contains('*') => {
            let base = dir.join(prefix.trim_end_matches('/'));
            std::fs::read_dir(&base)
                .map(|rd| rd.flatten().any(|e| e.path().join("Cargo.toml").is_file()))
                .unwrap_or(false)
        }
        // An interior glob — not checked.
        Some(_) => true,
        None => dir.join(pattern).join("Cargo.toml").is_file(),
    }
}

// ── constructors ───────────────────────────────────────────────────────────────

fn err(code: &str, message: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        message: message.to_string(),
        severity: "error".to_string(),
        code: code.to_string(),
        start,
        end,
    }
}

fn warn(code: &str, message: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        message: message.to_string(),
        severity: "warning".to_string(),
        code: code.to_string(),
        start,
        end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diags(text: &str) -> Vec<Diagnostic> {
        validate(text, &Context::default())
    }

    fn codes(text: &str) -> Vec<String> {
        diags(text).into_iter().map(|d| d.code).collect()
    }

    /// A real manifest has to come back clean, or the feature is a nuisance rather than a tool.
    /// The strongest single test here.
    #[test]
    fn a_real_manifest_is_clean() {
        let text = r#"
[package]
name         = "bennu-cargo"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
description  = "Reads Cargo manifests."

[dependencies]
serde = { workspace = true }
bennu-proto = { path = "../proto" }

[dev-dependencies]
tempfile = "3"

[features]
default = ["std"]
std = []
extra = ["dep:serde", "serde/derive"]
"#;
        // No workspace root and no directory in the context, so the inheritance and on-disk
        // checks are skipped — exactly as they must be when the answer is unknowable.
        assert_eq!(codes(text), Vec::<String>::new(), "{:?}", diags(text));
    }

    #[test]
    fn a_typo_in_a_table_name_is_named_and_silences_its_contents() {
        let d = diags("[package]\nname = \"x\"\n[dependancies]\nserde = \"1\"\n");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-unknown-table");
        assert!(d[0].message.contains("dependancies"));
    }

    #[test]
    fn a_typo_in_a_package_key_is_a_warning_not_an_error() {
        let d = diags("[package]\nname = \"x\"\nedtion = \"2021\"\n");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-unknown-key");
        assert_eq!(d[0].severity, "warning", "Cargo warns rather than failing");
    }

    #[test]
    fn a_typo_in_a_dependency_spec_key_is_caught_in_every_spelling() {
        // Inline table.
        let d = diags("[package]\nname = \"x\"\n[dependencies]\nserde = { version = \"1\", feature = [\"derive\"] }\n");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-unknown-key");
        assert!(d[0].message.contains("feature"));

        // Dotted key. Two diagnostics here, and both are true: the key is not one Cargo knows,
        // and because it is the ONLY thing written about `serde` the dependency really does name
        // no source — which is the error Cargo itself would give.
        let d = diags("[package]\nname = \"x\"\n[dependencies]\nserde.versoin = \"1\"\n");
        let found: Vec<&str> = d.iter().map(|x| x.code.as_str()).collect();
        assert!(found.contains(&"cargo-unknown-key"), "{d:?}");
        assert!(found.contains(&"cargo-dep-no-source"), "{d:?}");

        // Long form.
        let d = diags("[package]\nname = \"x\"\n[dependencies.serde]\nversion = \"1\"\nfeature = []\n");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-unknown-key");
    }

    #[test]
    fn a_bad_edition_lists_the_real_ones() {
        let text = "[package]\nname = \"x\"\nedition = \"2020\"\n";
        let d = diags(text);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-bad-value");
        assert!(d[0].message.contains("2021"), "{}", d[0].message);
        // Underlined at the value, not the key.
        assert_eq!(&text[d[0].start..d[0].end], "\"2020\"");
    }

    #[test]
    fn a_spec_value_of_the_wrong_shape_is_reported_at_the_value() {
        let text = "[package]\nname = \"x\"\n[dependencies]\nserde = { version = \"1\", optional = \"yes\" }\n";
        let d = diags(text);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-bad-value");
        assert_eq!(&text[d[0].start..d[0].end], "\"yes\"");
    }

    #[test]
    fn workspace_inheritance_on_a_key_that_cannot_be_inherited() {
        let d = diags("[package]\nname.workspace = true\n");
        let found: Vec<&str> = d.iter().map(|x| x.code.as_str()).collect();
        assert!(found.contains(&"cargo-not-inheritable"), "{found:?}");
        // …and it does not ALSO complain the name is missing: the key is there, it just cannot be
        // inherited.
        assert!(!found.contains(&"cargo-missing-name"));
    }

    #[test]
    fn a_duplicate_key_is_an_error_and_a_repeated_array_header_is_not() {
        let d = diags("[package]\nname = \"x\"\nname = \"y\"\n");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-duplicate-key");
        assert_eq!(d[0].severity, "error");

        let two_bins = "[package]\nname = \"x\"\n[[bin]]\nname = \"a\"\n[[bin]]\nname = \"b\"\n";
        assert_eq!(codes(two_bins), Vec::<String>::new(), "{:?}", diags(two_bins));
    }

    #[test]
    fn conflicting_and_missing_dependency_sources() {
        let d = diags(
            "[package]\nname = \"x\"\n[dependencies]\na = { git = \"u\", path = \"p\" }\nb = { branch = \"main\" }\nc = { optional = true }\n",
        );
        let found: Vec<&str> = d.iter().map(|x| x.code.as_str()).collect();
        assert!(found.contains(&"cargo-conflicting-source"), "{found:?}");
        assert!(found.contains(&"cargo-orphan-git-ref"), "{found:?}");
        assert!(found.contains(&"cargo-dep-no-source"), "{found:?}");
    }

    #[test]
    fn an_optional_dev_dependency_is_refused_the_way_cargo_refuses_it() {
        let d = diags("[package]\nname = \"x\"\n[dev-dependencies]\na = { version = \"1\", optional = true }\n");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-dev-dep-optional");
        assert_eq!(d[0].severity, "error");
    }

    /// The check that pays for the module: every shape of feature reference, right and wrong.
    #[test]
    fn feature_references_are_resolved_against_features_and_optional_dependencies() {
        let text = "\
[package]
name = \"x\"

[dependencies]
serde = { version = \"1\", optional = true }
anyhow = \"1\"

[features]
default = [\"pretty\"]
pretty = [\"serde\", \"dep:serde\", \"serde/derive\", \"serde?/alloc\", \"anyhow/std\"]
broken = [\"nope\", \"dep:absent\", \"absent/feat\", \"anyhow\"]
";
        let d = diags(text);
        let bad: Vec<(&str, &str)> =
            d.iter().map(|x| (x.code.as_str(), x.message.as_str())).collect();
        // The four wrong ones, and nothing else: an invented name, a `dep:` on nothing, a `/` on
        // nothing, and a non-optional dependency named bare.
        assert_eq!(bad.len(), 4, "{bad:?}");
        assert!(bad.iter().all(|(c, _)| *c == "cargo-unknown-feature-ref"));
        assert!(bad.iter().any(|(_, m)| m.contains("`nope`")), "{bad:?}");
        assert!(bad.iter().any(|(_, m)| m.contains("`absent`")), "{bad:?}");
        assert!(
            bad.iter().any(|(_, m)| m.contains("`anyhow`") && m.contains("not optional")),
            "the wording has to point at the real fix: {bad:?}"
        );
    }

    #[test]
    fn a_feature_diagnostic_underlines_the_item_not_the_line() {
        let text = "[package]\nname = \"x\"\n[features]\na = [\"one\", \"two\"]\n";
        let d = diags(text);
        assert_eq!(d.len(), 2, "{d:?}");
        assert_eq!(&text[d[0].start..d[0].end], "\"one\"");
        assert_eq!(&text[d[1].start..d[1].end], "\"two\"");
    }

    #[test]
    fn a_manifest_that_is_neither_a_package_nor_a_workspace() {
        let d = diags("[dependencies]\nserde = \"1\"\n");
        assert!(d.iter().any(|x| x.code == "cargo-no-package"), "{d:?}");
        // But an EMPTY file is not scolded — it is a file being created.
        assert!(diags("").is_empty());
        assert!(diags("\n\n# nothing yet\n").is_empty());
    }

    #[test]
    fn package_without_a_name() {
        let d = diags("[package]\nversion = \"0.1.0\"\n");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-missing-name");
        assert_eq!(d[0].severity, "error");
    }

    #[test]
    fn a_version_requirement_with_no_number_in_it() {
        let d = diags("[package]\nname = \"x\"\n[dependencies]\na = \"latest\"\n");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-bad-version-req");
        // `*` and a comma-separated range are real requirements and must not be flagged.
        assert!(diags("[package]\nname = \"x\"\n[dependencies]\na = \"*\"\n").is_empty());
        assert!(diags("[package]\nname = \"x\"\n[dependencies]\na = \">=1, <3\"\n").is_empty());
    }

    #[test]
    fn free_tables_are_never_checked() {
        let text = "\
[package]
name = \"x\"

[package.metadata.docs.rs]
all-features = true
rustdoc-args = [\"--cfg\", \"docsrs\"]

[lints.clippy]
needless_borrow = \"warn\"

[features]
whatever = []
";
        assert_eq!(codes(text), Vec::<String>::new(), "{:?}", diags(text));
    }

    #[test]
    fn default_members_must_be_members() {
        let text = "[workspace]\nmembers = [\"a\", \"b\"]\ndefault-members = [\"a\", \"c\"]\n";
        let d = validate(text, &Context::default());
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-default-member-not-member");
        assert_eq!(&text[d[0].start..d[0].end], "\"c\"");

        // With globs in `members` the check stands down rather than guessing.
        let globbed = "[workspace]\nmembers = [\"crates/*\"]\ndefault-members = [\"crates/a\"]\n";
        assert!(validate(globbed, &Context::default()).is_empty());
    }

    #[test]
    fn diagnostics_come_back_in_document_order() {
        let text = "[package]\nedtion = \"2021\"\nname = \"x\"\nbogus = 1\n";
        let d = diags(text);
        assert!(d.len() >= 2, "{d:?}");
        let starts: Vec<usize> = d.iter().map(|x| x.start).collect();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }

    /// Every intermediate state of typing a manifest has to be quiet about the part being typed.
    /// The single most important property here — a validator that squiggles while you type is one
    /// people turn off.
    #[test]
    fn a_manifest_being_typed_is_not_scolded() {
        for text in [
            "[pack",
            "[package]\n",
            "[package]\nname",
            "[package]\nname = ",
            "[package]\nname = \"",
            "[package]\nname = \"x\"\n[dep",
            "[package]\nname = \"x\"\n[dependencies]\n",
            "[package]\nname = \"x\"\n[dependencies]\nser",
            "[package]\nname = \"x\"\n[dependencies]\nserde = ",
            "[package]\nname = \"x\"\n[dependencies]\nserde = \"",
            "[package]\nname = \"x\"\n[dependencies]\nserde = \"1",
            "[package]\nname = \"x\"\n[dependencies]\nserde = {",
            "[package]\nname = \"x\"\n[dependencies]\nserde = { version = \"1\"",
            "[package]\nname = \"x\"\n[features]\nf = [",
            "[package]\nname = \"x\"\n[features]\nf = [\"",
        ] {
            let d = diags(text);
            // The only thing tolerated is the identity complaint on a `[package]` with no name
            // yet, and the unknown-table one on a header that is not finished being typed.
            assert!(
                d.iter().all(|x| x.code == "cargo-missing-name" || x.code == "cargo-unknown-table"),
                "typing {text:?} produced {d:?}"
            );
        }
    }

    // ── the filesystem-backed checks ──────────────────────────────────────────

    /// A scratch directory, unique per process and thread.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bennu-cargo-validate-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_member_that_is_not_on_disk_is_named() {
        let dir = scratch("members");
        std::fs::create_dir_all(dir.join("crates/real")).unwrap();
        std::fs::write(dir.join("crates/real/Cargo.toml"), "[package]\nname=\"real\"\n").unwrap();

        let text = "[workspace]\nmembers = [\"crates/*\", \"crates/real\", \"gone\"]\n";
        let ctx = Context { dir: Some(dir.clone()), workspace: None };
        let d = validate(text, &ctx);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-missing-member");
        assert_eq!(&text[d[0].start..d[0].end], "\"gone\"");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_dependency_pointing_nowhere_is_named() {
        let dir = scratch("path-dep");
        let text = "[package]\nname = \"x\"\n[dependencies]\nlocal = { path = \"../nope\" }\n";
        let ctx = Context { dir: Some(dir.clone()), workspace: None };
        let d = validate(text, &ctx);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, "cargo-missing-path");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inheriting_something_the_workspace_root_does_not_declare() {
        let root = Manifest::parse(
            "[workspace]\nmembers = [\"a\"]\n[workspace.package]\nedition = \"2021\"\n[workspace.dependencies]\nserde = \"1\"\n",
        );
        let ctx = Context { dir: None, workspace: Some(root) };
        let text = "\
[package]
name = \"a\"
edition.workspace = true
version.workspace = true

[dependencies]
serde = { workspace = true }
tokio = { workspace = true }
";
        let d = validate(text, &ctx);
        let messages: Vec<&str> = d.iter().map(|x| x.message.as_str()).collect();
        assert_eq!(d.len(), 2, "{messages:?}");
        assert!(d.iter().all(|x| x.code == "cargo-workspace-missing"));
        // `version` is not in `[workspace.package]`; `tokio` is not in `[workspace.dependencies]`.
        assert!(messages.iter().any(|m| m.contains("`version`")), "{messages:?}");
        assert!(messages.iter().any(|m| m.contains("`tokio`")), "{messages:?}");
        // …and the two that ARE declared produced nothing.
        assert!(!messages.iter().any(|m| m.contains("`edition`")), "{messages:?}");
        assert!(!messages.iter().any(|m| m.contains("`serde`")), "{messages:?}");
    }

    /// A member with no workspace root reachable must not have its inheritance flagged: the answer
    /// is unknown, and guessing it would squiggle every crate opened on its own.
    #[test]
    fn inheritance_is_not_checked_without_a_root() {
        let text = "[package]\nname = \"a\"\nedition.workspace = true\n[dependencies]\nserde = { workspace = true }\n";
        assert!(validate(text, &Context::default()).is_empty());
    }

    /// The root of a workspace inherits from itself, and must not be told its own
    /// `[workspace.package]` is missing.
    #[test]
    fn a_workspace_root_is_not_its_own_member() {
        let dir = scratch("root-self");
        let text = "\
[workspace]
members = []

[workspace.package]
edition = \"2021\"

[package]
name = \"root\"
edition.workspace = true
";
        std::fs::write(dir.join("Cargo.toml"), text).unwrap();
        let d = validate_file(&dir.join("Cargo.toml"), text);
        assert!(d.iter().all(|x| x.code != "cargo-workspace-missing"), "{d:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
