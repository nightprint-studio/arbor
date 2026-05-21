//! Persistent project-wide index of RON definitions + reference fields.
//!
//! Lives at `<repo_root>/.arbor/studio-index.json` and is read/written
//! exclusively by background jobs (never on the hot IPC path). Each
//! entry carries a cheap content-stamp (`size_mtime`) so an incremental
//! refresh only re-parses files whose stamp changed since the last
//! pass. Aggregating definitions / references becomes a hash-map scan
//! instead of a full disk walk — the original motivation for this
//! module (a few hundred `.ron` files starts to feel snappy enough to
//! deserve it).

use crate::error::{AppError, Result};
use crate::json_studio::ast::{self as json_ast, JsonAst};
use crate::ron_studio::ast::{self as ron_ast, RonAst};
use crate::studio::{
    config::{self as studio_config, matches_pattern, resolve_reference_fields, StudioConfig},
    scan_repo, BrokenRef, StudioFileKind, CrossRefDef, UsageMatch,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Bumped whenever the on-disk shape changes incompatibly. Older
/// indexes are discarded silently — a fresh refresh re-derives
/// everything in seconds.
///   v2 → `IndexedDef.path` added (recursive def scan).
///   v3 → `IndexedFile.kind` + JSON file support (Phase 3.c).
const INDEX_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StudioIndex {
    pub version: u32,
    /// Hash of the `.ron-studio.toml` content at the time of build.
    /// When the config changes (e.g. reference field patterns), every
    /// entry's `refs` may shift, so we invalidate the whole index.
    #[serde(default)]
    pub config_hash: u64,
    /// Repo-relative POSIX path → entry. `BTreeMap` keeps the JSON
    /// deterministic for diff-friendliness if someone version-controls
    /// the index by accident.
    #[serde(default)]
    pub files: BTreeMap<String, IndexedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile {
    /// Cheap stamp: `<size>_<mtime_millis>`. Plenty unique for cache
    /// invalidation; content hashing would cost ~as much as parsing.
    pub stamp:      String,
    pub size_bytes: u64,
    pub abs_path:   String,
    pub file_name:  String,
    /// File kind — drives parser selection and the aggregator's
    /// per-kind filter (RON and JSON have separate id namespaces).
    /// Defaults to `Ron` for legacy v2 entries that lack the field.
    #[serde(default = "default_kind")]
    pub kind:       StudioFileKind,
    #[serde(default)]
    pub excluded:   bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defs:       Vec<IndexedDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs:       Vec<IndexedRef>,
}

fn default_kind() -> StudioFileKind { StudioFileKind::Ron }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedDef {
    pub value: String,
    /// `"id"` or `"name"` — last segment of `path`, kept separately
    /// for label rendering.
    pub field: String,
    /// Full AST path of the def field. Empty == top-level (legacy
    /// indexes built before recursion shipped — bump `INDEX_VERSION`
    /// when changing this shape).
    #[serde(default)]
    pub path:  Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedRef {
    pub value: String,
    pub key:   String,
    pub path:  Vec<String>,
}

/// On-progress callback: (processed_files, total_files).
pub type ProgressFn = dyn FnMut(usize, usize) + Send;

pub fn index_path(repo_root: &str) -> PathBuf {
    Path::new(repo_root).join(".arbor").join("studio-index.json")
}

pub fn load(repo_root: &str) -> StudioIndex {
    let path = index_path(repo_root);
    let Ok(text) = std::fs::read_to_string(&path) else { return StudioIndex::default(); };
    let Ok(idx) = serde_json::from_str::<StudioIndex>(&text) else { return StudioIndex::default(); };
    if idx.version != INDEX_VERSION { return StudioIndex::default(); }
    idx
}

pub fn save(repo_root: &str, idx: &StudioIndex) -> Result<()> {
    let path = index_path(repo_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Other(format!("mkdir .arbor: {e}")))?;
    }
    let text = serde_json::to_string_pretty(idx)
        .map_err(|e| AppError::Other(format!("encode studio-index: {e}")))?;
    std::fs::write(&path, text)
        .map_err(|e| AppError::Other(format!("write studio-index: {e}")))?;
    Ok(())
}

/// Compute a stable hash of the studio config so we can invalidate
/// the whole index when the user changes excludes / overrides /
/// reference_fields. Order-independent for `excludes` so reordering
/// alone doesn't trigger a rebuild.
fn hash_config(cfg: &StudioConfig) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();

    // Excludes — sort first so reorder-only doesn't invalidate.
    let mut ex = cfg.excludes.clone();
    ex.sort();
    ex.hash(&mut h);

    // Default binding — only the fields we use for indexing.
    if let Some(def) = &cfg.default {
        def.rs_file.hash(&mut h);
        def.root_type.hash(&mut h);
        let mut fields = def.reference_fields.clone();
        fields.sort();
        fields.hash(&mut h);
    } else {
        0u8.hash(&mut h);
    }

    // Overrides — first-match-wins semantics make order significant.
    for ov in &cfg.overrides {
        ov.glob.hash(&mut h);
        ov.rs_file.hash(&mut h);
        ov.root_type.hash(&mut h);
        let mut fields = ov.reference_fields.clone();
        fields.sort();
        fields.hash(&mut h);
    }

    // Externals — order doesn't matter (the scanner walks each
    // independently), so sort by path before hashing so adding /
    // removing entries invalidates the index but pure reorder
    // doesn't. Without this, adding an external location would
    // leave the index stale until a manual rebuild.
    let mut ext: Vec<&crate::studio::config::ExternalEntry> = cfg.externals.iter().collect();
    ext.sort_by(|a, b| a.path.cmp(&b.path));
    for e in ext {
        e.path.hash(&mut h);
        e.label.hash(&mut h);
    }

    h.finish()
}

fn stamp_for(path: &Path) -> String {
    let Ok(meta) = std::fs::metadata(path) else { return String::new(); };
    let size  = meta.len();
    let mtime = meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{size}_{mtime}")
}

/// Walk the repo, refresh stale entries, drop deleted ones. Calls
/// `progress` on each file (cheap stamp-only checks count too — the
/// caller surfaces the ratio in the UI). Returns the up-to-date index;
/// also persisted to disk before returning.
///
/// `kinds` filters the walk to specific file kinds — empty defaults to
/// every supported kind (RON + JSON today). Entries for kinds excluded
/// by the filter are left untouched in the on-disk index so a later
/// refresh covering them won't re-walk needlessly.
pub fn refresh(
    repo_root:  &str,
    progress:   Option<&mut ProgressFn>,
) -> Result<StudioIndex> {
    refresh_for(repo_root, &[], progress)
}

pub fn refresh_for(
    repo_root:  &str,
    kinds:      &[StudioFileKind],
    progress:   Option<&mut ProgressFn>,
) -> Result<StudioIndex> {
    let cfg = studio_config::load(repo_root).unwrap_or_default();
    let new_cfg_hash = hash_config(&cfg);

    let mut idx = load(repo_root);
    // Config changed → throw out every entry's parsed data. Stamp
    // re-check is still cheap; the parse phase is what would happen
    // anyway.
    if idx.version != INDEX_VERSION || idx.config_hash != new_cfg_hash {
        idx = StudioIndex { version: INDEX_VERSION, config_hash: new_cfg_hash, files: BTreeMap::new() };
    }

    // Effective filter: empty means "every supported kind" — the index
    // is one file per repo, so the natural default is to keep it
    // complete across formats.
    let effective_kinds: &[StudioFileKind] = if kinds.is_empty() {
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

    // Pull the file list through the normal scanner so excludes
    // and walk-skipping stay in sync.
    let entries = scan_repo(repo_root, effective_kinds)?;
    let total = entries.len();
    let mut seen: HashSet<String> = HashSet::with_capacity(total);
    let mut progress_cb = progress;

    for (i, entry) in entries.into_iter().enumerate() {
        seen.insert(entry.relative_path.clone());
        let stamp = stamp_for(Path::new(&entry.absolute_path));
        let needs_reparse = idx.files.get(&entry.relative_path)
            .map(|f| f.stamp != stamp || f.excluded != entry.excluded || f.kind != entry.kind)
            .unwrap_or(true);

        if needs_reparse {
            let patterns = resolve_reference_fields(&cfg, &entry.relative_path);
            let (defs, refs) = if entry.excluded {
                // Keep the entry so the file stays known + can be
                // re-indexed cheaply when un-excluded, but don't pay
                // the parse cost.
                (Vec::new(), Vec::new())
            } else {
                parse_and_extract(&entry.absolute_path, entry.kind, patterns.as_deref())
                    .unwrap_or((Vec::new(), Vec::new()))
            };
            idx.files.insert(entry.relative_path.clone(), IndexedFile {
                stamp,
                size_bytes: entry.size_bytes,
                abs_path:   entry.absolute_path,
                file_name:  entry.name,
                kind:       entry.kind,
                excluded:   entry.excluded,
                defs,
                refs,
            });
        }

        if let Some(p) = progress_cb.as_deref_mut() { p(i + 1, total); }
    }

    // Sweep deleted files — but only within the kinds we just walked,
    // so a kind-scoped refresh doesn't clobber the other format's data.
    let kinds_set: HashSet<StudioFileKind> = effective_kinds.iter().copied().collect();
    idx.files.retain(|k, f| {
        if !kinds_set.contains(&f.kind) { return true; }
        seen.contains(k)
    });
    idx.config_hash = new_cfg_hash;
    save(repo_root, &idx)?;
    Ok(idx)
}

fn parse_and_extract(
    abs_path: &str,
    kind:     StudioFileKind,
    patterns: Option<&[String]>,
) -> Option<(Vec<IndexedDef>, Vec<IndexedRef>)> {
    let text = std::fs::read_to_string(abs_path).ok()?;
    let mut defs = Vec::new();
    let mut refs = Vec::new();
    match kind {
        StudioFileKind::Ron => {
            let value = ron_ast::parse(&text).ok()?;
            collect_defs_ron(&value, &[], &mut defs);
            collect_refs_ron(&value, &[], patterns, &mut refs);
        }
        StudioFileKind::Json => {
            // Phase 3.d: lenient parse so `.jsonc` index entries don't
            // skip on a comment / trailing comma.
            let value = json_ast::parse_with(&text, /* strict */ false).ok()?;
            collect_defs_json(&value, &[], &mut defs);
            collect_refs_json(&value, &[], patterns, &mut refs);
        }
        StudioFileKind::Toml => {
            // Phase 4.c.a: walk the JSON projection of the TOML
            // document — same shape JSON uses, so the recursion is
            // straightforward.
            let value = crate::toml_studio::parse_to_value(&text)?;
            collect_defs_toml(&value, &[], &mut defs);
            collect_refs_toml(&value, &[], patterns, &mut refs);
        }
        StudioFileKind::Yaml => {
            // Phase 5.c — YAML projects to the same JSON Value shape as
            // TOML (`yaml_studio::parse_to_value`), so we reuse the TOML
            // walkers verbatim. Multi-doc streams collapse to
            // `Value::Array`; recursion handles it.
            let value = crate::yaml_studio::parse_to_value(&text)?;
            collect_defs_toml(&value, &[], &mut defs);
            collect_refs_toml(&value, &[], patterns, &mut refs);
        }
        StudioFileKind::Properties => {
            // Phase 6 — special convention: every flat dotted key is a
            // def (value = the key itself), every non-empty leaf value
            // is a ref. Patterns are ignored — the format baked the
            // convention in by FROZEN F5.
            collect_defs_properties_index(&text, &mut defs);
            collect_refs_properties_index(&text, &mut refs);
        }
    }
    Some((defs, refs))
}

// ── `.properties` index walkers (Phase 6) ────────────────────────────

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

fn collect_defs_properties_index(text: &str, out: &mut Vec<IndexedDef>) {
    for (flat_key, _v) in crate::properties_studio::collect_kv_pairs(text) {
        let path = properties_flat_to_path(&flat_key);
        if path.is_empty() { continue; }
        let last = path.last().cloned().unwrap_or_default();
        out.push(IndexedDef {
            value: flat_key,
            field: last,
            path,
        });
    }
}

fn collect_refs_properties_index(text: &str, out: &mut Vec<IndexedRef>) {
    for (flat_key, value) in crate::properties_studio::collect_kv_pairs(text) {
        if value.is_empty() { continue; }
        let path = properties_flat_to_path(&flat_key);
        if path.is_empty() { continue; }
        let key_name = path.last().cloned().unwrap_or_default();
        out.push(IndexedRef {
            value,
            key:   key_name,
            path,
        });
    }
}

fn collect_defs_toml(value: &serde_json::Value, path: &[String], out: &mut Vec<IndexedDef>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let mut p = path.to_vec();
                p.push(k.clone());
                if k == "id" || k == "name" {
                    if let serde_json::Value::String(s) = v {
                        if !s.is_empty() {
                            out.push(IndexedDef {
                                value: s.clone(),
                                field: k.clone(),
                                path:  p.clone(),
                            });
                        }
                    }
                }
                collect_defs_toml(v, &p, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, child) in arr.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_defs_toml(child, &p, out);
            }
        }
        _ => {}
    }
}

fn collect_refs_toml(
    value:    &serde_json::Value,
    path:     &[String],
    patterns: Option<&[String]>,
    out:      &mut Vec<IndexedRef>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let mut p = path.to_vec();
                p.push(k.clone());
                if matches_reference(k, patterns) {
                    match v {
                        serde_json::Value::String(s) if !s.is_empty() => {
                            out.push(IndexedRef {
                                value: s.clone(),
                                key:   k.clone(),
                                path:  p.clone(),
                            });
                        }
                        serde_json::Value::Array(arr) => {
                            for (i, child) in arr.iter().enumerate() {
                                let serde_json::Value::String(s) = child else { continue; };
                                if s.is_empty() { continue; }
                                let mut item_path = p.clone();
                                item_path.push(i.to_string());
                                out.push(IndexedRef {
                                    value: s.clone(),
                                    key:   k.clone(),
                                    path:  item_path,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                collect_refs_toml(v, &p, patterns, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, child) in arr.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_refs_toml(child, &p, patterns, out);
            }
        }
        _ => {}
    }
}

fn collect_defs_ron(value: &RonAst, path: &[String], out: &mut Vec<IndexedDef>) {
    match value {
        RonAst::Struct { fields, .. } => {
            for (k, v) in fields {
                let mut p = path.to_vec();
                p.push(k.clone());
                if k == "id" || k == "name" {
                    if let RonAst::String(s) = v {
                        if !s.is_empty() {
                            out.push(IndexedDef {
                                value: s.clone(),
                                field: k.clone(),
                                path:  p.clone(),
                            });
                        }
                    }
                }
                collect_defs_ron(v, &p, out);
            }
        }
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            for (i, child) in items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_defs_ron(child, &p, out);
            }
        }
        RonAst::Map(pairs) => {
            for (k, v) in pairs {
                let mut p = path.to_vec();
                p.push(key_for_path_ron(k));
                collect_defs_ron(v, &p, out);
            }
        }
        RonAst::Option(Some(inner)) => {
            let mut p = path.to_vec();
            p.push("Some".into());
            collect_defs_ron(inner.as_ref(), &p, out);
        }
        _ => {}
    }
}

fn matches_reference(key: &str, patterns: Option<&[String]>) -> bool {
    match patterns {
        Some(list) => list.iter().any(|p| matches_pattern(p, key)),
        None       => matches!(key, "target" | "source" | "parent" | "owner" | "prev" | "next")
                      || key.ends_with("_id")  || key.ends_with("_ref")
                      || key.ends_with("Id")   || key.ends_with("Ref"),
    }
}

fn collect_refs_ron(
    value:    &RonAst,
    path:     &[String],
    patterns: Option<&[String]>,
    out:      &mut Vec<IndexedRef>,
) {
    match value {
        RonAst::Struct { fields, .. } => {
            for (k, v) in fields {
                let mut p = path.to_vec();
                p.push(k.clone());
                if matches_reference(k, patterns) {
                    // Reference field may carry either a single string
                    // or a list of strings — index both shapes so the
                    // user can mark a `Vec<String>` (e.g. `Action = [
                    // "gladius_strike", "fury_of_mars", … ]`) and have
                    // every entry surface as a usage hit.
                    match v {
                        RonAst::String(s) if !s.is_empty() => {
                            out.push(IndexedRef { value: s.clone(), key: k.clone(), path: p.clone() });
                        }
                        RonAst::List(items) | RonAst::Tuple { items, .. } => {
                            for (i, child) in items.iter().enumerate() {
                                let RonAst::String(s) = child else { continue; };
                                if s.is_empty() { continue; }
                                let mut item_path = p.clone();
                                item_path.push(i.to_string());
                                out.push(IndexedRef {
                                    value: s.clone(),
                                    key:   k.clone(),
                                    path:  item_path,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                collect_refs_ron(v, &p, patterns, out);
            }
        }
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            for (i, child) in items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_refs_ron(child, &p, patterns, out);
            }
        }
        RonAst::Map(pairs) => {
            for (k, child) in pairs {
                let mut p = path.to_vec();
                p.push(key_for_path_ron(k));
                collect_refs_ron(child, &p, patterns, out);
            }
        }
        RonAst::Option(Some(inner)) => {
            let mut p = path.to_vec();
            p.push("Some".into());
            collect_refs_ron(inner.as_ref(), &p, patterns, out);
        }
        _ => {}
    }
}

fn key_for_path_ron(k: &RonAst) -> String {
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

// ── JSON walkers ────────────────────────────────────────────────────

fn collect_defs_json(value: &JsonAst, path: &[String], out: &mut Vec<IndexedDef>) {
    match value {
        JsonAst::Object(o) => {
            for prop in &o.props {
                let mut p = path.to_vec();
                p.push(prop.name.clone());
                if prop.name == "id" || prop.name == "name" {
                    if let JsonAst::String(s) = &prop.value {
                        if !s.value.is_empty() {
                            out.push(IndexedDef {
                                value: s.value.clone(),
                                field: prop.name.clone(),
                                path:  p.clone(),
                            });
                        }
                    }
                }
                collect_defs_json(&prop.value, &p, out);
            }
        }
        JsonAst::Array(a) => {
            for (i, child) in a.items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_defs_json(child, &p, out);
            }
        }
        _ => {}
    }
}

fn collect_refs_json(
    value:    &JsonAst,
    path:     &[String],
    patterns: Option<&[String]>,
    out:      &mut Vec<IndexedRef>,
) {
    match value {
        JsonAst::Object(o) => {
            for prop in &o.props {
                let mut p = path.to_vec();
                p.push(prop.name.clone());
                if matches_reference(&prop.name, patterns) {
                    match &prop.value {
                        JsonAst::String(s) if !s.value.is_empty() => {
                            out.push(IndexedRef {
                                value: s.value.clone(),
                                key:   prop.name.clone(),
                                path:  p.clone(),
                            });
                        }
                        JsonAst::Array(arr) => {
                            for (i, child) in arr.items.iter().enumerate() {
                                let JsonAst::String(s) = child else { continue; };
                                if s.value.is_empty() { continue; }
                                let mut item_path = p.clone();
                                item_path.push(i.to_string());
                                out.push(IndexedRef {
                                    value: s.value.clone(),
                                    key:   prop.name.clone(),
                                    path:  item_path,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                collect_refs_json(&prop.value, &p, patterns, out);
            }
        }
        JsonAst::Array(a) => {
            for (i, child) in a.items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(i.to_string());
                collect_refs_json(child, &p, patterns, out);
            }
        }
        _ => {}
    }
}

// ── Aggregators consumed by scan_cross_refs / find_usages ─────────────────

fn kind_passes(kinds: &[StudioFileKind], k: StudioFileKind) -> bool {
    kinds.is_empty() || kinds.contains(&k)
}

pub fn aggregate_cross_refs(idx: &StudioIndex) -> Vec<CrossRefDef> {
    aggregate_cross_refs_for(idx, &[])
}

pub fn aggregate_cross_refs_for(idx: &StudioIndex, kinds: &[StudioFileKind]) -> Vec<CrossRefDef> {
    let mut out = Vec::new();
    for (rel, f) in &idx.files {
        if f.excluded { continue; }
        if !kind_passes(kinds, f.kind) { continue; }
        for d in &f.defs {
            // Legacy v1 entries had no `path`; fall back to `[field]`
            // so cross-ref navigation still works against top-level
            // defs while the index gradually rebuilds.
            let def_path = if d.path.is_empty() {
                vec![d.field.clone()]
            } else {
                d.path.clone()
            };
            out.push(CrossRefDef {
                id_value:      d.value.clone(),
                absolute_path: f.abs_path.clone(),
                relative_path: rel.clone(),
                file_name:     f.file_name.clone(),
                kind:          f.kind,
                def_path,
                def_field:     d.field.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.id_value.cmp(&b.id_value)
        .then_with(|| a.relative_path.cmp(&b.relative_path)));
    out
}

pub fn aggregate_usages(idx: &StudioIndex, target: &str) -> Vec<UsageMatch> {
    aggregate_usages_for(idx, target, &[])
}

pub fn aggregate_usages_for(idx: &StudioIndex, target: &str, kinds: &[StudioFileKind]) -> Vec<UsageMatch> {
    let mut out = Vec::new();
    for (rel, f) in &idx.files {
        if f.excluded { continue; }
        if !kind_passes(kinds, f.kind) { continue; }
        for r in &f.refs {
            if r.value != target { continue; }
            out.push(UsageMatch {
                absolute_path: f.abs_path.clone(),
                relative_path: rel.clone(),
                file_name:     f.file_name.clone(),
                kind:          f.kind,
                field_path:    r.path.clone(),
                key_name:      r.key.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path)
        .then_with(|| a.field_path.cmp(&b.field_path)));
    out
}

#[allow(dead_code)]
pub fn aggregate_broken_refs(idx: &StudioIndex) -> Vec<BrokenRef> {
    aggregate_broken_refs_for(idx, &[])
}

/// Project-wide broken-reference walk over the cached index. Builds
/// a `HashSet<String>` of every known `id`/`name` value once, then
/// emits any `IndexedRef` whose value isn't in that set. Result is
/// shaped like `UsageMatch` plus the orphaned `value` so the
/// frontend can render an actionable label and jump to the offender.
///
/// Kinds filter the WHOLE walk: a broken ref in a JSON file only
/// counts against the JSON def namespace — a RON `id: "goblin"` does
/// not satisfy a JSON ref to `"goblin"`. The two formats have
/// independent id namespaces.
pub fn aggregate_broken_refs_for(idx: &StudioIndex, kinds: &[StudioFileKind]) -> Vec<BrokenRef> {
    use std::collections::HashSet;
    // Build the def-namespace per kind. Excluded files don't
    // contribute — same rule as cross-refs / usages — so refs that
    // happen to point at an excluded file's id still count as broken
    // (the user opted that file out for a reason).
    let mut defs_by_kind: std::collections::HashMap<StudioFileKind, HashSet<String>> =
        std::collections::HashMap::new();
    for f in idx.files.values() {
        if f.excluded { continue; }
        let entry = defs_by_kind.entry(f.kind).or_default();
        for d in &f.defs {
            if !d.value.is_empty() { entry.insert(d.value.clone()); }
        }
    }
    let empty: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for (rel, f) in &idx.files {
        if f.excluded { continue; }
        if !kind_passes(kinds, f.kind) { continue; }
        let defs = defs_by_kind.get(&f.kind).unwrap_or(&empty);
        for r in &f.refs {
            if r.value.is_empty() || defs.contains(&r.value) { continue; }
            out.push(BrokenRef {
                absolute_path: f.abs_path.clone(),
                relative_path: rel.clone(),
                file_name:     f.file_name.clone(),
                kind:          f.kind,
                field_path:    r.path.clone(),
                key_name:      r.key.clone(),
                value:         r.value.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.value.cmp(&b.value)
        .then_with(|| a.relative_path.cmp(&b.relative_path))
        .then_with(|| a.field_path.cmp(&b.field_path)));
    out
}
