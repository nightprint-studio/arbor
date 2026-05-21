//! studio — project-wide file index used by the built-in Studio sidebar.
//!
//! Walks the active repo for the data-file kinds we have first-class
//! viewers for (`.ron`, `.json`, `.toml`) and returns them as a flat list
//! that the frontend folds into a tree. Stays deliberately thin — schema
//! resolution, cross-ref indexing and binding persistence live elsewhere
//! (in their own modules / per-format crates), so this one is just a
//! filtered directory walk with sane vendor-folder exclusions.

use crate::error::{AppError, Result};
use crate::ron_studio::{self, ast::{self, RonAst}, SchemaHint, SchemaHintOrigin};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub mod config;
pub mod edit_expr;
pub mod format;
pub mod index;
use config::{
    is_excluded, load as load_config, matches_pattern, resolve_reference_fields,
};

/// File kinds the Studio sidebar can index. Serialised lowercase so the
/// frontend keeps thinking in `'ron' | 'json' | 'toml' | 'yaml' |
/// 'properties'` strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StudioFileKind {
    Ron,
    Json,
    Toml,
    /// Phase 5.a — YAML (`.yaml` and `.yml`). Read-only navigation
    /// in 5.a; lossless edit + cross-refs land in later sub-phases.
    Yaml,
    /// Phase 6 — `.properties`. Lossless line-based edit + cross-refs
    /// (every key = target, every value = ref) + F12 / F13 / JSON
    /// Schema sidecar.
    Properties,
}

impl StudioFileKind {
    fn from_ext(ext: &str) -> Option<Self> {
        match ext {
            "ron"          => Some(Self::Ron),
            // Phase 3.d: `.jsonc` shares the JSON backend (single
            // plugin, hybrid AST/stream parser per FROZEN F14).
            "json" | "jsonc" => Some(Self::Json),
            "toml"         => Some(Self::Toml),
            // Phase 5.a — both YAML extensions map to the same backend.
            "yaml" | "yml" => Some(Self::Yaml),
            // Phase 6 — `.properties`.
            "properties"   => Some(Self::Properties),
            _              => None,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct StudioFileEntry {
    pub absolute_path: String,
    pub relative_path: String,
    pub name:          String,
    pub kind:          StudioFileKind,
    pub size_bytes:    u64,
    /// `true` when the file matches one of the `excludes` globs in the
    /// repo's `.ron-studio.toml`. The sidebar hides excluded files by
    /// default and the cross-ref / find-usages scanners skip them.
    #[serde(default)]
    pub excluded:      bool,
    /// Resolved schema binding (directive in file or sidecar match).
    /// `null` when no binding exists — surfaced as the empty-circle
    /// badge in the sidebar. Only computed for `.ron` files for now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema:        Option<EntrySchema>,
    /// `true` when the entry came from a registered external location
    /// rather than the repo's own tree. The sidebar groups these
    /// under a virtual `external/<label>/…` root and decorates them
    /// with a link-style icon so the user reads them as "outside
    /// the project, but tracked".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external:      bool,
}

/// Slim mirror of `SchemaHint` for the file index — keeps the entry row
/// in the sidebar small while still telling us which `.rs` the binding
/// resolves to and how it was configured.
#[derive(Debug, Serialize, Clone)]
pub struct EntrySchema {
    pub rs_file:   String,
    pub root_type: String,
    pub origin:    SchemaHintOriginExt,
}

impl From<SchemaHint> for EntrySchema {
    fn from(h: SchemaHint) -> Self {
        Self {
            rs_file:   h.rs_file,
            root_type: h.root_type,
            origin:    match h.origin {
                SchemaHintOrigin::Directive => SchemaHintOriginExt::Directive,
                SchemaHintOrigin::Sidecar   => SchemaHintOriginExt::Sidecar,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaHintOriginExt {
    Directive,
    Sidecar,
}

/// Recursive vendor-skipping walk under `folder` matching any of `kinds`.
/// Returns paths sorted by `relative_path` (case-insensitive — the
/// frontend tree groups by folder, sort just stabilises insertion).
///
/// Annotates each `.ron` entry with its resolved schema binding + the
/// `excluded` flag from the repo's `.ron-studio.toml`. Excluded files
/// are NOT filtered out here — the sidebar wants to optionally show
/// them; downstream scanners (`scan_cross_refs`, `find_usages`) skip
/// them explicitly via the same flag.
pub fn scan_repo(folder: &str, kinds: &[StudioFileKind]) -> Result<Vec<StudioFileEntry>> {
    let root = Path::new(folder);
    if !root.is_dir() {
        return Err(AppError::Other(format!("Not a directory: {folder}")));
    }
    let cfg = load_config(folder).unwrap_or_default();
    let mut out = Vec::new();
    walk(root, root, kinds, &mut out, 0);

    // External locations registered in `.ron-studio.toml`: walk each
    // alongside the repo. Files land in the same flat list with a
    // synthetic `relative_path = "external/<label>/<sub>"` prefix so
    // bindings + cross-refs + the sidebar tree treat them as if they
    // lived under the repo root (the user's mental model). The
    // absolute path stays real so opening the file actually goes to
    // disk. Each entry's `external` flag flips on so the sidebar can
    // render the group with a distinct icon.
    for ext in &cfg.externals {
        walk_external(ext, kinds, &mut out);
    }

    for entry in &mut out {
        entry.excluded = is_excluded(&cfg, &entry.relative_path);
        // RON: prefer the inline directive in the file body (a `//!
        // ron-studio:` comment) or the walk-up sidecar lookup. JSON /
        // TOML don't carry inline directives so they skip step 1 and
        // jump straight to the cfg-keyed resolver. Either way, the
        // `cfg.resolve_binding` second pass covers external files and
        // any path the walk-up can't reach.
        if entry.kind == StudioFileKind::Ron {
            let text = std::fs::read_to_string(&entry.absolute_path).ok();
            let hint = text.as_deref().and_then(|t|
                ron_studio::detect_schema_hint(t, Some(&entry.absolute_path)));
            if let Some(h) = hint {
                entry.schema = Some(h.into());
                continue;
            }
        }
        if let Some((rs_file, root_type)) =
            config::resolve_binding(&cfg, folder, &entry.relative_path)
        {
            entry.schema = Some(EntrySchema {
                rs_file,
                root_type,
                origin: SchemaHintOriginExt::Sidecar,
            });
        }
    }
    out.sort_by(|a, b| a.relative_path.to_ascii_lowercase()
        .cmp(&b.relative_path.to_ascii_lowercase()));
    Ok(out)
}

/// Walk an external file or directory registered via the sidebar's
/// "Add external" action. Files outside the repo's own tree don't
/// honour the recursion-cap rules but we still apply the same skip
/// list (vendor / hidden) — a folder pointing at a user's whole home
/// directory shouldn't suck in `.cache` and friends.
fn walk_external(
    ext:   &crate::studio::config::ExternalEntry,
    kinds: &[StudioFileKind],
    out:   &mut Vec<StudioFileEntry>,
) {
    let abs = Path::new(&ext.path);
    if !abs.exists() { return; }
    let label = if ext.label.trim().is_empty() {
        // Fall back to the path's basename when the user didn't
        // provide an explicit label.
        abs.file_name().and_then(|n| n.to_str()).unwrap_or("external").to_string()
    } else {
        ext.label.clone()
    };
    let virt_root = format!("external/{label}");

    if abs.is_file() {
        // Single-file registration sits directly under `external/` —
        // no synthetic label-folder around it. Two single files with
        // colliding names disambiguate via the user-typed label (or
        // a parent-dir suffix the user can rename later); for now we
        // keep the layout flat and accept the rare collision case.
        let name = abs.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let Some(ext_s) = abs.extension().and_then(|e| e.to_str()) else { return; };
        let Some(kind) = StudioFileKind::from_ext(&ext_s.to_ascii_lowercase()) else { return; };
        if !kinds.is_empty() && !kinds.contains(&kind) { return; }
        let size = std::fs::metadata(abs).map(|m| m.len()).unwrap_or(0);
        // Use the explicit label as a folder when given (so the user
        // can group their loose files under a meaningful name);
        // otherwise drop the file straight into `external/`.
        let rel = if ext.label.trim().is_empty() {
            format!("external/{name}")
        } else {
            format!("{virt_root}/{name}")
        };
        out.push(StudioFileEntry {
            absolute_path: abs.to_string_lossy().into_owned(),
            relative_path: rel,
            name,
            kind,
            size_bytes: size,
            excluded:   false,
            schema:     None,
            external:   true,
        });
        return;
    }

    if abs.is_dir() {
        let mut local = Vec::<StudioFileEntry>::new();
        walk(abs, abs, kinds, &mut local, 0);
        for mut e in local {
            // Re-root each entry under the virtual `external/<label>/`
            // prefix. The walker already produced repo-relative paths;
            // we just prepend.
            e.relative_path = format!("{virt_root}/{}", e.relative_path);
            e.external = true;
            out.push(e);
        }
    }
}

fn walk(
    root:  &Path,
    dir:   &Path,
    kinds: &[StudioFileKind],
    out:   &mut Vec<StudioFileEntry>,
    depth: usize,
) {
    // Hard cap on recursion to avoid runaway walks (symlinks, weird FS).
    if depth > 16 { return; }
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.is_empty() { continue; }
        // Hidden + common build/vendor folders. `target`/`node_modules`/`dist`
        // can contain large amounts of generated json (lockfiles, source
        // maps) that would drown the real config files.
        if name.starts_with('.') { continue; }
        if matches!(
            name,
            "target" | "node_modules" | ".git" | "dist" | "build"
                | "out"  | ".next" | ".svelte-kit" | ".cargo" | "venv"
                | "__pycache__" | ".idea" | ".vscode"
        ) { continue; }
        if path.is_dir() {
            walk(root, &path, kinds, out, depth + 1);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue; };
        let Some(kind) = StudioFileKind::from_ext(&ext.to_ascii_lowercase()) else { continue; };
        if !kinds.is_empty() && !kinds.contains(&kind) { continue; }
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(StudioFileEntry {
            absolute_path: path.to_string_lossy().into_owned(),
            relative_path: rel.to_string_lossy().replace('\\', "/"),
            name:          name.to_string(),
            kind,
            size_bytes:    size,
            excluded:      false,
            schema:        None,
            external:      false,
        });
    }
}

// ── Cross-reference scanner ───────────────────────────────────────────────────

/// A single top-level definition: a struct field whose key is `id` or
/// `name` and whose value is a string literal. Bevy moddable games keep
/// most of their references this way ("`enemy_id: "goblin"` points at a
/// file whose top-level has `id: "goblin"`"), so a project-wide index of
/// these pairs is enough to power Ctrl+click cross-navigation.
#[derive(Debug, Serialize)]
pub struct CrossRefDef {
    /// The literal string the def field carries (e.g. `"goblin"`).
    pub id_value:      String,
    /// Absolute path of the .ron file that hosts the definition.
    pub absolute_path: String,
    /// Path relative to the repo root, POSIX separators — matches what
    /// `scan_repo` returns so the frontend can correlate.
    pub relative_path: String,
    /// Plain file name (e.g. `goblin.ron`) for the picker / breadcrumb.
    pub file_name:     String,
    /// File kind the def came from. Lets the FE filter cross-refs by
    /// format (a RON `id: "goblin"` and a JSON `id: "goblin"` are
    /// independent namespaces — Phase 3.c).
    pub kind:          StudioFileKind,
    /// Full AST path of the def field (e.g. `["abilities", "2", "id"]`
    /// for a nested definition). The frontend uses this to expand +
    /// select the right node after jumping into the file.
    pub def_path:      Vec<String>,
    /// `"id"` or `"name"`. Kept separately for label rendering — the
    /// last element of `def_path` is the same value.
    pub def_field:     String,
}

#[allow(dead_code)]
pub fn scan_cross_refs(folder: &str) -> Result<Vec<CrossRefDef>> {
    scan_cross_refs_for(folder, &[StudioFileKind::Ron])
}

/// Walk every file under `folder` whose kind appears in `kinds` and
/// collect top-level / nested `id`/`name` string definitions. Files
/// that fail to parse are skipped silently — the goal is a best-effort
/// project-wide index, not a strict validator. Returns defs sorted by
/// (id_value, path) so later picker UIs render deterministically.
pub fn scan_cross_refs_for(folder: &str, kinds: &[StudioFileKind]) -> Result<Vec<CrossRefDef>> {
    let root = Path::new(folder);
    if !root.is_dir() {
        return Err(AppError::Other(format!("Not a directory: {folder}")));
    }
    let effective: &[StudioFileKind] = if kinds.is_empty() {
        &[
            StudioFileKind::Ron,
            StudioFileKind::Json,
            StudioFileKind::Toml,
            StudioFileKind::Yaml,
            StudioFileKind::Properties,
        ]
    } else {
        kinds
    };
    let files = scan_repo(folder, effective)?;
    let mut out = Vec::new();
    for f in files {
        if f.excluded { continue; }
        let Ok(text) = std::fs::read_to_string(&f.absolute_path) else { continue; };
        match f.kind {
            StudioFileKind::Ron => {
                let Ok(value) = ast::parse(&text) else { continue; };
                collect_defs_ron_into(&value, &[], &f, &mut out);
            }
            StudioFileKind::Json => {
                // Phase 3.d: lenient so `.jsonc` files (with comments
                // / trailing commas) are walked alongside `.json`.
                let Ok(value) = crate::json_studio::ast::parse_with(&text, false) else { continue; };
                collect_defs_json_into(&value, &[], &f, &mut out);
            }
            StudioFileKind::Toml => {
                // Phase 4.c.a: project to JSON Value (already the
                // toml_studio convention) and walk the same shape.
                let Some(value) = crate::toml_studio::parse_to_value(&text) else { continue; };
                collect_defs_toml_into(&value, &[], &f, &mut out);
            }
            StudioFileKind::Yaml => {
                // Phase 5.c — YAML projects to the same `serde_json::Value`
                // shape as TOML, so we reuse the TOML walker verbatim.
                // Multi-doc streams collapse to `Value::Array` at the
                // root — the recursion handles it naturally.
                let Some(value) = crate::yaml_studio::parse_to_value(&text) else { continue; };
                collect_defs_toml_into(&value, &[], &f, &mut out);
            }
            StudioFileKind::Properties => {
                // Phase 6 — `.properties` defs = every flat dotted key
                // (FROZEN F5). We walk the line view directly because
                // the JSON projection would lose the bracketed array
                // notation we need for the flat-key string.
                collect_defs_properties_into(&text, &f, &mut out);
            }
        }
    }
    out.sort_by(|a, b| a.id_value.cmp(&b.id_value)
        .then_with(|| a.relative_path.cmp(&b.relative_path)));
    Ok(out)
}

/// Walk the whole AST looking for `id`/`name` string fields at any
/// depth. Records the full path so the frontend can expand + select
/// the exact node after the cross-ref jump — works for both flat
/// per-file definitions (`(id: "goblin", …)`) and nested catalogues
/// (`(abilities: [(id: "gladius_strike", …), …])`).
fn collect_defs_ron_into(
    value: &RonAst,
    path:  &[String],
    entry: &StudioFileEntry,
    out:   &mut Vec<CrossRefDef>,
) {
    match value {
        RonAst::Struct { fields, .. } => {
            for (key, val) in fields {
                let mut p = path.to_vec();
                p.push(key.clone());
                if key == "id" || key == "name" {
                    if let RonAst::String(s) = val {
                        if !s.is_empty() {
                            out.push(CrossRefDef {
                                id_value:      s.clone(),
                                absolute_path: entry.absolute_path.clone(),
                                relative_path: entry.relative_path.clone(),
                                file_name:     entry.name.clone(),
                                kind:          entry.kind,
                                def_path:      p.clone(),
                                def_field:     key.clone(),
                            });
                        }
                    }
                }
                collect_defs_ron_into(val, &p, entry, out);
            }
        }
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            for (i, child) in items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_defs_ron_into(child, &p, entry, out);
            }
        }
        RonAst::Map(pairs) => {
            for (k, child) in pairs {
                let mut p = path.to_vec();
                p.push(key_for_path(k));
                collect_defs_ron_into(child, &p, entry, out);
            }
        }
        RonAst::Option(Some(inner)) => {
            let mut p = path.to_vec();
            p.push("Some".into());
            collect_defs_ron_into(inner.as_ref(), &p, entry, out);
        }
        _ => {}
    }
}

/// TOML twin of `collect_defs_ron_into`. TOML projects to
/// `serde_json::Value` (see `toml_studio::parse_to_value`), so we walk
/// the same shape JSON does. Arrays-of-tables show as Value::Array
/// of Value::Object, plain TOML tables as Value::Object — the
/// recursion handles both naturally.
fn collect_defs_toml_into(
    value: &serde_json::Value,
    path:  &[String],
    entry: &StudioFileEntry,
    out:   &mut Vec<CrossRefDef>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let mut p = path.to_vec();
                p.push(k.clone());
                if k == "id" || k == "name" {
                    if let serde_json::Value::String(s) = v {
                        if !s.is_empty() {
                            out.push(CrossRefDef {
                                id_value:      s.clone(),
                                absolute_path: entry.absolute_path.clone(),
                                relative_path: entry.relative_path.clone(),
                                file_name:     entry.name.clone(),
                                kind:          entry.kind,
                                def_path:      p.clone(),
                                def_field:     k.clone(),
                            });
                        }
                    }
                }
                collect_defs_toml_into(v, &p, entry, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, child) in arr.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_defs_toml_into(child, &p, entry, out);
            }
        }
        _ => {}
    }
}

/// JSON twin of `collect_defs_ron_into`.
fn collect_defs_json_into(
    value: &crate::json_studio::ast::JsonAst,
    path:  &[String],
    entry: &StudioFileEntry,
    out:   &mut Vec<CrossRefDef>,
) {
    use crate::json_studio::ast::JsonAst;
    match value {
        JsonAst::Object(o) => {
            for prop in &o.props {
                let mut p = path.to_vec();
                p.push(prop.name.clone());
                if prop.name == "id" || prop.name == "name" {
                    if let JsonAst::String(s) = &prop.value {
                        if !s.value.is_empty() {
                            out.push(CrossRefDef {
                                id_value:      s.value.clone(),
                                absolute_path: entry.absolute_path.clone(),
                                relative_path: entry.relative_path.clone(),
                                file_name:     entry.name.clone(),
                                kind:          entry.kind,
                                def_path:      p.clone(),
                                def_field:     prop.name.clone(),
                            });
                        }
                    }
                }
                collect_defs_json_into(&prop.value, &p, entry, out);
            }
        }
        JsonAst::Array(a) => {
            for (i, child) in a.items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_defs_json_into(child, &p, entry, out);
            }
        }
        _ => {}
    }
}

// ── Usage scanner (reverse navigation: def → references) ─────────────────────

/// A single occurrence of a reference field pointing at the target id.
/// `field_path` mirrors what `Doc::get_children`/`get_value` produce, so
/// the frontend can drive the same expand-and-select flow used by
/// Ctrl+click navigation.
#[derive(Debug, Serialize)]
pub struct UsageMatch {
    pub absolute_path: String,
    pub relative_path: String,
    pub file_name:     String,
    pub kind:          StudioFileKind,
    /// AST path of the matching string node (the *reference* field).
    pub field_path:    Vec<String>,
    /// The reference field's key (`enemy_id`, `target`, …) for picker labels.
    pub key_name:      String,
}

/// A reference field whose value doesn't correspond to any known
/// `id`/`name` definition in the project — i.e. a dangling pointer.
/// Mirrors `UsageMatch`'s shape so the frontend can jump to it using
/// the same expand-and-select flow, plus the orphaned `value` so we
/// can show what the broken ref was trying to point to.
#[derive(Debug, Serialize)]
pub struct BrokenRef {
    pub absolute_path: String,
    pub relative_path: String,
    pub file_name:     String,
    pub kind:          StudioFileKind,
    pub field_path:    Vec<String>,
    pub key_name:      String,
    /// The string value the field carried; no matching `id`/`name`
    /// exists anywhere in the project for it.
    pub value:         String,
}

/// Built-in reference-field convention. Used when nothing is
/// configured for a given file via `[default]` / `[[overrides]]`
/// in `.ron-studio.toml`. Mirrors the frontend's `isReferenceFieldName`
/// so forward (ref → def) and backward (def → refs) navigation cover
/// the exact same set of keys.
fn is_builtin_reference_field(key: &str) -> bool {
    matches!(key, "target" | "source" | "parent" | "owner" | "prev" | "next")
        || key.ends_with("_id")  || key.ends_with("_ref")
        || key.ends_with("Id")   || key.ends_with("Ref")
}

/// Check `key` against either a configured pattern list (when set) or
/// the built-in convention. Configured patterns REPLACE the convention.
fn matches_reference(key: &str, patterns: Option<&[String]>) -> bool {
    match patterns {
        Some(list) => list.iter().any(|p| matches_pattern(p, key)),
        None       => is_builtin_reference_field(key),
    }
}

#[allow(dead_code)]
pub fn find_usages(folder: &str, target: &str) -> Result<Vec<UsageMatch>> {
    find_usages_for(folder, target, &[StudioFileKind::Ron])
}

/// Walk every file in the repo whose kind appears in `kinds` and emit
/// a `UsageMatch` for each reference field whose string value equals
/// `target`. Empty input returns an empty vec without doing IO.
pub fn find_usages_for(folder: &str, target: &str, kinds: &[StudioFileKind]) -> Result<Vec<UsageMatch>> {
    if target.is_empty() {
        return Ok(Vec::new());
    }
    let root = Path::new(folder);
    if !root.is_dir() {
        return Err(AppError::Other(format!("Not a directory: {folder}")));
    }
    let effective: &[StudioFileKind] = if kinds.is_empty() {
        &[
            StudioFileKind::Ron,
            StudioFileKind::Json,
            StudioFileKind::Toml,
            StudioFileKind::Yaml,
            StudioFileKind::Properties,
        ]
    } else {
        kinds
    };
    let files = scan_repo(folder, effective)?;
    let cfg   = load_config(folder).unwrap_or_default();
    let mut out = Vec::new();
    for f in files {
        if f.excluded { continue; }
        let Ok(text) = std::fs::read_to_string(&f.absolute_path) else { continue; };
        // Resolve reference-field patterns once per file — the walker
        // re-uses them for every nested struct it visits.
        let patterns = resolve_reference_fields(&cfg, &f.relative_path);
        match f.kind {
            StudioFileKind::Ron => {
                let Ok(value) = ast::parse(&text) else { continue; };
                collect_usages_ron(&value, &[], target, &f, patterns.as_deref(), &mut out);
            }
            StudioFileKind::Json => {
                // Phase 3.d: lenient so `.jsonc` files (with comments
                // / trailing commas) are walked alongside `.json`.
                let Ok(value) = crate::json_studio::ast::parse_with(&text, false) else { continue; };
                collect_usages_json(&value, &[], target, &f, patterns.as_deref(), &mut out);
            }
            StudioFileKind::Toml => {
                let Some(value) = crate::toml_studio::parse_to_value(&text) else { continue; };
                collect_usages_toml(&value, &[], target, &f, patterns.as_deref(), &mut out);
            }
            StudioFileKind::Yaml => {
                // Phase 5.c — reuse the TOML walker (both formats project
                // to `serde_json::Value`).
                let Some(value) = crate::yaml_studio::parse_to_value(&text) else { continue; };
                collect_usages_toml(&value, &[], target, &f, patterns.as_deref(), &mut out);
            }
            StudioFileKind::Properties => {
                // Phase 6 — `.properties` refs = every leaf value
                // (FROZEN F5: every value is a potential reference,
                // independent of `*_id`/`*_ref` patterns). Walks the
                // line view directly so bracketed array indices land
                // in `field_path` correctly.
                collect_usages_properties(&text, target, &f, &mut out);
            }
        }
    }
    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path)
        .then_with(|| a.field_path.cmp(&b.field_path)));
    Ok(out)
}

/// Recursive AST walker mirroring `children_of` in `ron_studio::mod`:
/// struct fields keep their key, tuple/list items use the index as
/// string, map keys are stringified, Option wraps in a `"Some"` step.
/// Keeps the produced paths usable verbatim by the frontend tree.
fn collect_usages_ron(
    value:    &RonAst,
    path:     &[String],
    target:   &str,
    entry:    &StudioFileEntry,
    patterns: Option<&[String]>,
    out:      &mut Vec<UsageMatch>,
) {
    match value {
        RonAst::Struct { fields, .. } => {
            for (k, v) in fields {
                let mut p = path.to_vec();
                p.push(k.clone());
                if matches_reference(k, patterns) {
                    match v {
                        RonAst::String(s) => {
                            if s == target {
                                out.push(UsageMatch {
                                    absolute_path: entry.absolute_path.clone(),
                                    relative_path: entry.relative_path.clone(),
                                    file_name:     entry.name.clone(),
                                    kind:          entry.kind,
                                    field_path:    p.clone(),
                                    key_name:      k.clone(),
                                });
                            }
                        }
                        RonAst::List(items) | RonAst::Tuple { items, .. } => {
                            for (i, child) in items.iter().enumerate() {
                                let RonAst::String(s) = child else { continue; };
                                if s != target { continue; }
                                let mut item_path = p.clone();
                                item_path.push(i.to_string());
                                out.push(UsageMatch {
                                    absolute_path: entry.absolute_path.clone(),
                                    relative_path: entry.relative_path.clone(),
                                    file_name:     entry.name.clone(),
                                    kind:          entry.kind,
                                    field_path:    item_path,
                                    key_name:      k.clone(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                collect_usages_ron(v, &p, target, entry, patterns, out);
            }
        }
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            for (i, child) in items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_usages_ron(child, &p, target, entry, patterns, out);
            }
        }
        RonAst::Map(pairs) => {
            for (k, child) in pairs {
                let mut p = path.to_vec();
                p.push(key_for_path(k));
                collect_usages_ron(child, &p, target, entry, patterns, out);
            }
        }
        RonAst::Option(Some(inner)) => {
            let mut p = path.to_vec();
            p.push("Some".into());
            collect_usages_ron(inner.as_ref(), &p, target, entry, patterns, out);
        }
        _ => {}
    }
}

/// TOML twin of `collect_usages_ron`. Walks the JSON projection
/// emitted by `toml_studio::parse_to_value` so the convention
/// (`*_id`/`*_ref`/`target`/…) matches the JSON / RON behaviour
/// exactly.
fn collect_usages_toml(
    value:    &serde_json::Value,
    path:     &[String],
    target:   &str,
    entry:    &StudioFileEntry,
    patterns: Option<&[String]>,
    out:      &mut Vec<UsageMatch>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let mut p = path.to_vec();
                p.push(k.clone());
                if matches_reference(k, patterns) {
                    match v {
                        serde_json::Value::String(s) if s == target => {
                            out.push(UsageMatch {
                                absolute_path: entry.absolute_path.clone(),
                                relative_path: entry.relative_path.clone(),
                                file_name:     entry.name.clone(),
                                kind:          entry.kind,
                                field_path:    p.clone(),
                                key_name:      k.clone(),
                            });
                        }
                        serde_json::Value::Array(arr) => {
                            for (i, child) in arr.iter().enumerate() {
                                let serde_json::Value::String(s) = child else { continue; };
                                if s != target { continue; }
                                let mut item_path = p.clone();
                                item_path.push(i.to_string());
                                out.push(UsageMatch {
                                    absolute_path: entry.absolute_path.clone(),
                                    relative_path: entry.relative_path.clone(),
                                    file_name:     entry.name.clone(),
                                    kind:          entry.kind,
                                    field_path:    item_path,
                                    key_name:      k.clone(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                collect_usages_toml(v, &p, target, entry, patterns, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, child) in arr.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_usages_toml(child, &p, target, entry, patterns, out);
            }
        }
        _ => {}
    }
}

/// JSON twin of `collect_usages_ron`.
fn collect_usages_json(
    value:    &crate::json_studio::ast::JsonAst,
    path:     &[String],
    target:   &str,
    entry:    &StudioFileEntry,
    patterns: Option<&[String]>,
    out:      &mut Vec<UsageMatch>,
) {
    use crate::json_studio::ast::JsonAst;
    match value {
        JsonAst::Object(o) => {
            for prop in &o.props {
                let mut p = path.to_vec();
                p.push(prop.name.clone());
                if matches_reference(&prop.name, patterns) {
                    match &prop.value {
                        JsonAst::String(s) => {
                            if s.value == target {
                                out.push(UsageMatch {
                                    absolute_path: entry.absolute_path.clone(),
                                    relative_path: entry.relative_path.clone(),
                                    file_name:     entry.name.clone(),
                                    kind:          entry.kind,
                                    field_path:    p.clone(),
                                    key_name:      prop.name.clone(),
                                });
                            }
                        }
                        JsonAst::Array(arr) => {
                            for (i, child) in arr.items.iter().enumerate() {
                                let JsonAst::String(s) = child else { continue; };
                                if s.value != target { continue; }
                                let mut item_path = p.clone();
                                item_path.push(i.to_string());
                                out.push(UsageMatch {
                                    absolute_path: entry.absolute_path.clone(),
                                    relative_path: entry.relative_path.clone(),
                                    file_name:     entry.name.clone(),
                                    kind:          entry.kind,
                                    field_path:    item_path,
                                    key_name:      prop.name.clone(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                collect_usages_json(&prop.value, &p, target, entry, patterns, out);
            }
        }
        JsonAst::Array(a) => {
            for (i, child) in a.items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_usages_json(child, &p, target, entry, patterns, out);
            }
        }
        _ => {}
    }
}

/// Project-wide reference validation: for every `.ron` file under
/// `folder`, walk every reference field (built-in convention OR
/// configured patterns) and collect the ones whose string value
/// doesn't match any `id`/`name` definition seen elsewhere in the
/// project. Empty result is the happy path. Same exclude-folder
/// rules as `scan_cross_refs` / `find_usages`.
///
/// Two-pass shape because we can't know if a value is broken until
/// we've gathered every def in the project. Pass 1 collects defs +
/// candidate refs per file (one parse each); pass 2 filters refs
/// against the def set. The intermediate is bounded by document
/// content size so it's safe to hold in memory.
#[allow(dead_code)]
pub fn scan_broken_refs(folder: &str) -> Result<Vec<BrokenRef>> {
    scan_broken_refs_for(folder, &[StudioFileKind::Ron])
}

pub fn scan_broken_refs_for(folder: &str, kinds: &[StudioFileKind]) -> Result<Vec<BrokenRef>> {
    let root = Path::new(folder);
    if !root.is_dir() {
        return Err(AppError::Other(format!("Not a directory: {folder}")));
    }
    let effective: &[StudioFileKind] = if kinds.is_empty() {
        &[
            StudioFileKind::Ron,
            StudioFileKind::Json,
            StudioFileKind::Toml,
            StudioFileKind::Yaml,
            StudioFileKind::Properties,
        ]
    } else {
        kinds
    };
    let files = scan_repo(folder, effective)?;
    let cfg   = load_config(folder).unwrap_or_default();

    // Pass 1 — parse each file once, gather defs (per-kind, to keep
    // namespaces separate) + candidate refs.
    let mut defs_by_kind: std::collections::HashMap<StudioFileKind, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    let mut all_refs: Vec<PendingRef> = Vec::new();
    for f in files {
        if f.excluded { continue; }
        let Ok(text)  = std::fs::read_to_string(&f.absolute_path) else { continue; };
        let mut defs_here: Vec<CrossRefDef> = Vec::new();
        let patterns = resolve_reference_fields(&cfg, &f.relative_path);
        match f.kind {
            StudioFileKind::Ron => {
                let Ok(value) = ast::parse(&text) else { continue; };
                collect_defs_ron_into(&value, &[], &f, &mut defs_here);
                collect_all_refs_ron(&value, &[], &f, patterns.as_deref(), &mut all_refs);
            }
            StudioFileKind::Json => {
                // Phase 3.d: lenient so `.jsonc` files (with comments
                // / trailing commas) are walked alongside `.json`.
                let Ok(value) = crate::json_studio::ast::parse_with(&text, false) else { continue; };
                collect_defs_json_into(&value, &[], &f, &mut defs_here);
                collect_all_refs_json(&value, &[], &f, patterns.as_deref(), &mut all_refs);
            }
            StudioFileKind::Toml => {
                let Some(value) = crate::toml_studio::parse_to_value(&text) else { continue; };
                collect_defs_toml_into(&value, &[], &f, &mut defs_here);
                collect_all_refs_toml(&value, &[], &f, patterns.as_deref(), &mut all_refs);
            }
            StudioFileKind::Yaml => {
                // Phase 5.c — reuse the TOML walker (both formats project
                // to `serde_json::Value`).
                let Some(value) = crate::yaml_studio::parse_to_value(&text) else { continue; };
                collect_defs_toml_into(&value, &[], &f, &mut defs_here);
                collect_all_refs_toml(&value, &[], &f, patterns.as_deref(), &mut all_refs);
            }
            StudioFileKind::Properties => {
                // Phase 6 — every key is a def, every value is a ref
                // (FROZEN F5). Patterns are ignored: the convention is
                // baked into the format.
                collect_defs_properties_into(&text, &f, &mut defs_here);
                collect_all_refs_properties(&text, &f, &mut all_refs);
            }
        }
        let entry = defs_by_kind.entry(f.kind).or_default();
        for d in defs_here {
            if !d.id_value.is_empty() { entry.insert(d.id_value); }
        }
    }

    // Pass 2 — filter to genuinely-broken refs and reshape. The match
    // happens *within the same kind*: a RON id never satisfies a JSON
    // ref and vice versa.
    let empty: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<BrokenRef> = all_refs.into_iter()
        .filter(|r| {
            let defs = defs_by_kind.get(&r.entry.kind).unwrap_or(&empty);
            !defs.contains(&r.value)
        })
        .map(|r| BrokenRef {
            absolute_path: r.entry.absolute_path,
            relative_path: r.entry.relative_path,
            file_name:     r.entry.name,
            kind:          r.entry.kind,
            field_path:    r.field_path,
            key_name:      r.key,
            value:         r.value,
        })
        .collect();
    out.sort_by(|a, b| a.value.cmp(&b.value)
        .then_with(|| a.relative_path.cmp(&b.relative_path))
        .then_with(|| a.field_path.cmp(&b.field_path)));
    Ok(out)
}

/// Same walking semantics as `collect_usages_ron` but emits *every*
/// reference site without filtering on a target value. The caller
/// then matches against a def-set to keep only the broken ones.
fn collect_all_refs_ron(
    value:    &RonAst,
    path:     &[String],
    entry:    &StudioFileEntry,
    patterns: Option<&[String]>,
    out:      &mut Vec<PendingRef>,
) {
    match value {
        RonAst::Struct { fields, .. } => {
            for (k, v) in fields {
                let mut p = path.to_vec();
                p.push(k.clone());
                if matches_reference(k, patterns) {
                    match v {
                        RonAst::String(s) if !s.is_empty() => {
                            out.push(PendingRef {
                                entry:      entry.clone(),
                                field_path: p.clone(),
                                key:        k.clone(),
                                value:      s.clone(),
                            });
                        }
                        RonAst::List(items) | RonAst::Tuple { items, .. } => {
                            for (i, child) in items.iter().enumerate() {
                                let RonAst::String(s) = child else { continue; };
                                if s.is_empty() { continue; }
                                let mut item_path = p.clone();
                                item_path.push(i.to_string());
                                out.push(PendingRef {
                                    entry:      entry.clone(),
                                    field_path: item_path,
                                    key:        k.clone(),
                                    value:      s.clone(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                collect_all_refs_ron(v, &p, entry, patterns, out);
            }
        }
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            for (i, child) in items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_all_refs_ron(child, &p, entry, patterns, out);
            }
        }
        RonAst::Map(pairs) => {
            for (k, child) in pairs {
                let mut p = path.to_vec();
                p.push(key_for_path(k));
                collect_all_refs_ron(child, &p, entry, patterns, out);
            }
        }
        RonAst::Option(Some(inner)) => {
            let mut p = path.to_vec();
            p.push("Some".into());
            collect_all_refs_ron(inner.as_ref(), &p, entry, patterns, out);
        }
        _ => {}
    }
}

/// TOML twin of `collect_all_refs_ron` — walks the JSON projection
/// and emits every reference-site value (without filtering on a
/// specific target). Pairs with `collect_defs_toml_into` for the
/// broken-refs pass.
fn collect_all_refs_toml(
    value:    &serde_json::Value,
    path:     &[String],
    entry:    &StudioFileEntry,
    patterns: Option<&[String]>,
    out:      &mut Vec<PendingRef>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let mut p = path.to_vec();
                p.push(k.clone());
                if matches_reference(k, patterns) {
                    match v {
                        serde_json::Value::String(s) if !s.is_empty() => {
                            out.push(PendingRef {
                                entry:      entry.clone(),
                                field_path: p.clone(),
                                key:        k.clone(),
                                value:      s.clone(),
                            });
                        }
                        serde_json::Value::Array(arr) => {
                            for (i, child) in arr.iter().enumerate() {
                                let serde_json::Value::String(s) = child else { continue; };
                                if s.is_empty() { continue; }
                                let mut item_path = p.clone();
                                item_path.push(i.to_string());
                                out.push(PendingRef {
                                    entry:      entry.clone(),
                                    field_path: item_path,
                                    key:        k.clone(),
                                    value:      s.clone(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                collect_all_refs_toml(v, &p, entry, patterns, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, child) in arr.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_all_refs_toml(child, &p, entry, patterns, out);
            }
        }
        _ => {}
    }
}

fn collect_all_refs_json(
    value:    &crate::json_studio::ast::JsonAst,
    path:     &[String],
    entry:    &StudioFileEntry,
    patterns: Option<&[String]>,
    out:      &mut Vec<PendingRef>,
) {
    use crate::json_studio::ast::JsonAst;
    match value {
        JsonAst::Object(o) => {
            for prop in &o.props {
                let mut p = path.to_vec();
                p.push(prop.name.clone());
                if matches_reference(&prop.name, patterns) {
                    match &prop.value {
                        JsonAst::String(s) if !s.value.is_empty() => {
                            out.push(PendingRef {
                                entry:      entry.clone(),
                                field_path: p.clone(),
                                key:        prop.name.clone(),
                                value:      s.value.clone(),
                            });
                        }
                        JsonAst::Array(arr) => {
                            for (i, child) in arr.items.iter().enumerate() {
                                let JsonAst::String(s) = child else { continue; };
                                if s.value.is_empty() { continue; }
                                let mut item_path = p.clone();
                                item_path.push(i.to_string());
                                out.push(PendingRef {
                                    entry:      entry.clone(),
                                    field_path: item_path,
                                    key:        prop.name.clone(),
                                    value:      s.value.clone(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                collect_all_refs_json(&prop.value, &p, entry, patterns, out);
            }
        }
        JsonAst::Array(a) => {
            for (i, child) in a.items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_all_refs_json(child, &p, entry, patterns, out);
            }
        }
        _ => {}
    }
}

/// Staging struct produced by `collect_all_refs` and consumed by
/// `scan_broken_refs`. Kept local because it isn't part of any
/// stable IPC payload — `BrokenRef` is what crosses the wire.
struct PendingRef {
    entry:      StudioFileEntry,
    field_path: Vec<String>,
    key:        String,
    value:      String,
}

// ── `.properties` walkers (Phase 6) ───────────────────────────────────────
//
// Special convention (FROZEN F5): every flat dotted key is a cross-ref
// target, every leaf value is a potential reference. Reference-field
// patterns from `.studio.toml` are ignored — they don't make sense for
// a format where every leaf is automatically eligible.

/// Split a flat key into AST path segments. Numeric bracket indices
/// become numeric path segments so the standard tree-navigation works
/// against the projected JSON.
fn properties_flat_to_path(flat: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = flat.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '.' {
            if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
            continue;
        }
        if c == '[' {
            if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
            let mut idx = String::new();
            for ic in chars.by_ref() {
                if ic == ']' { break; }
                idx.push(ic);
            }
            if !idx.is_empty() { out.push(idx); }
            continue;
        }
        cur.push(c);
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

fn collect_defs_properties_into(
    text:  &str,
    entry: &StudioFileEntry,
    out:   &mut Vec<CrossRefDef>,
) {
    for (flat_key, _value) in crate::properties_studio::collect_kv_pairs(text) {
        let path = properties_flat_to_path(&flat_key);
        if path.is_empty() { continue; }
        let last = path.last().cloned().unwrap_or_default();
        out.push(CrossRefDef {
            id_value:      flat_key.clone(),
            absolute_path: entry.absolute_path.clone(),
            relative_path: entry.relative_path.clone(),
            file_name:     entry.name.clone(),
            kind:          entry.kind,
            def_path:      path,
            def_field:     last,
        });
    }
}

fn collect_usages_properties(
    text:   &str,
    target: &str,
    entry:  &StudioFileEntry,
    out:    &mut Vec<UsageMatch>,
) {
    for (flat_key, value) in crate::properties_studio::collect_kv_pairs(text) {
        if value != target { continue; }
        let path = properties_flat_to_path(&flat_key);
        if path.is_empty() { continue; }
        let key_name = path.last().cloned().unwrap_or_default();
        out.push(UsageMatch {
            absolute_path: entry.absolute_path.clone(),
            relative_path: entry.relative_path.clone(),
            file_name:     entry.name.clone(),
            kind:          entry.kind,
            field_path:    path,
            key_name,
        });
    }
}

fn collect_all_refs_properties(
    text:  &str,
    entry: &StudioFileEntry,
    out:   &mut Vec<PendingRef>,
) {
    for (flat_key, value) in crate::properties_studio::collect_kv_pairs(text) {
        if value.is_empty() { continue; }
        let path = properties_flat_to_path(&flat_key);
        if path.is_empty() { continue; }
        let key_name = path.last().cloned().unwrap_or_default();
        out.push(PendingRef {
            entry:      entry.clone(),
            field_path: path,
            key:        key_name,
            value,
        });
    }
}

/// Stringify a map key for path use. Same shape as the frontend's
/// tree-jump expects — matches `ron_studio::mod::key_to_string` but
/// kept local so the studio module stays free of internal-only deps.
fn key_for_path(k: &RonAst) -> String {
    match k {
        RonAst::String(s)      => s.clone(),
        RonAst::Char(c)        => c.to_string(),
        RonAst::Bool(b)        => b.to_string(),
        RonAst::Int(i)         => i.to_string(),
        RonAst::Float(f)       => f.to_string(),
        RonAst::UnitVariant(n) => n.clone(),
        _ => String::new(),
    }
}
