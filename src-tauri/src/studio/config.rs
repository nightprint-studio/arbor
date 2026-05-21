//! Read/write the repo-root studio config from the Studio sidebar.
//!
//! The same file format that `ron_studio::detect_schema_hint` already
//! consumes for *passive* schema bindings (default + overrides), plus a
//! new top-level `excludes = [...]` array driven by the sidebar's
//! "Exclude file" / "Exclude folder" actions.
//!
//! Location: `<repo_root>/.arbor/studio.toml`. The file used to live at
//! `<repo_root>/.ron-studio.toml` (back when the studio was RON-only);
//! we keep reading the legacy path as a fallback so existing projects
//! don't break, but every write goes to the new location. The legacy
//! file is left in place — the user can delete it manually after
//! verifying the migration.
//!
//! Nested-folder bindings (a `.arbor/studio.toml` or the legacy
//! `.ron-studio.toml` deeper in the tree) still work for reads via
//! `ron_studio::parse_sidecar_config`; the host only ever creates / writes
//! the single file at the repo root.

use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// New location — under `.arbor/` alongside the studio-index. Single
/// file shared by every studio format (RON, JSON, TOML, …).
const SIDECAR_PATH: &str = ".arbor/studio.toml";
/// Legacy location — at the repo root, RON-studio-only naming. Read
/// fallback only; never written.
const SIDECAR_LEGACY: &str = ".ron-studio.toml";

/// Mirror of the on-disk config — fields not in the file default to
/// empty, fields we don't yet manage from the UI are passed through
/// unchanged on write so the user's manual edits survive.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StudioConfig {
    /// Glob patterns (relative to the repo root) — files matching any
    /// of these are hidden by default in the Studio sidebar and skipped
    /// by the cross-ref + find-usages scanners.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excludes: Vec<String>,

    /// The fallback schema binding when no override matches a given file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<SchemaBinding>,

    /// Per-glob bindings, first-match-wins (same precedence the reader uses).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<SchemaOverride>,

    /// File/folder paths outside the repo that the user wants
    /// indexed as if they lived inside it — save games stored in
    /// `%APPDATA%`, configs under `~/.config/`, content folders on
    /// network shares, etc. Each entry is rendered in the sidebar
    /// under a synthetic `external/<label>/…` prefix so existing
    /// bindings + cross-refs work transparently.
    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "external")]
    pub externals: Vec<ExternalEntry>,
}

/// One user-registered external location. `path` is absolute (or
/// resolved on save); `label` is a short human name used for the
/// synthetic relative-path prefix and the sidebar group title.
/// When `label` is empty we fall back to the basename of `path`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaBinding {
    pub rs_file:   String,
    pub root_type: String,
    /// Custom list of reference-field patterns (Ctrl+click sources).
    /// Empty/missing → fall back to the built-in convention. Patterns
    /// support `*suffix`, `prefix*`, or exact match. Replaces the
    /// convention entirely when set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaOverride {
    pub glob:      String,
    pub rs_file:   String,
    pub root_type: String,
    /// Same semantics as `SchemaBinding::reference_fields`, scoped to
    /// the files matched by `glob`. A non-empty list here takes
    /// precedence over the default-level list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_fields: Vec<String>,
}

/// `<repo_root>/.arbor/studio.toml` — the canonical location.
pub fn config_path(repo_root: &str) -> PathBuf {
    Path::new(repo_root).join(SIDECAR_PATH)
}

/// `<repo_root>/.ron-studio.toml` — legacy read-only fallback.
pub fn legacy_config_path(repo_root: &str) -> PathBuf {
    Path::new(repo_root).join(SIDECAR_LEGACY)
}

pub fn load(repo_root: &str) -> Result<StudioConfig> {
    // Prefer the new path; fall back to the legacy file when only the
    // old one exists. We don't merge — the new path wins outright once
    // it's been written.
    let new_path    = config_path(repo_root);
    let legacy_path = legacy_config_path(repo_root);
    let (path, label) = if new_path.is_file() {
        (new_path, SIDECAR_PATH)
    } else if legacy_path.is_file() {
        (legacy_path, SIDECAR_LEGACY)
    } else {
        return Ok(StudioConfig::default());
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| AppError::Other(format!("read {label}: {e}")))?;
    toml::from_str::<StudioConfig>(&text)
        .map_err(|e| AppError::Other(format!("parse {label}: {e}")))
}

pub fn save(repo_root: &str, cfg: &StudioConfig) -> Result<()> {
    let path = config_path(repo_root);
    let is_empty = cfg.excludes.is_empty()
        && cfg.default.is_none()
        && cfg.overrides.is_empty()
        && cfg.externals.is_empty();
    if is_empty {
        // Empty config — clean up by deleting the new file. The legacy
        // file (if any) is also nuked so we don't get false-positive
        // reads on the next load.
        if path.is_file() {
            let _ = std::fs::remove_file(&path);
        }
        let legacy = legacy_config_path(repo_root);
        if legacy.is_file() {
            let _ = std::fs::remove_file(&legacy);
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Other(format!("mkdir .arbor: {e}")))?;
    }
    let text = toml::to_string_pretty(cfg)
        .map_err(|e| AppError::Other(format!("encode {SIDECAR_PATH}: {e}")))?;
    std::fs::write(&path, text)
        .map_err(|e| AppError::Other(format!("write {SIDECAR_PATH}: {e}")))?;
    Ok(())
}

/// Returns `true` when the relative path (POSIX-normalised) matches any
/// of the exclude globs in `cfg.excludes`. Empty config → never.
pub fn is_excluded(cfg: &StudioConfig, rel_path: &str) -> bool {
    if cfg.excludes.is_empty() { return false; }
    let norm = rel_path.replace('\\', "/");
    cfg.excludes.iter().any(|g| glob_match(g, &norm))
}

/// Toggle an exclude entry for `rel_path` (an exact match, no globbing
/// — the call site normalises). Returns the new state.
pub fn toggle_exclude(cfg: &mut StudioConfig, rel_path: &str) -> bool {
    let norm = rel_path.replace('\\', "/");
    if let Some(i) = cfg.excludes.iter().position(|s| s == &norm) {
        cfg.excludes.remove(i);
        false
    } else {
        cfg.excludes.push(norm);
        true
    }
}

/// Replace (or insert) a per-file/per-folder binding. Uses a `glob`
/// that matches just this target so the existing first-match-wins
/// reader picks it up identically to a hand-written entry.
///
/// When `reference_fields` is `Some`, it replaces the stored list; when
/// `None`, the existing list (if any) is preserved — so re-binding via
/// the UI doesn't silently drop a hand-curated list of reference field
/// patterns.
pub fn set_binding(
    cfg:              &mut StudioConfig,
    rel_path:         &str,
    rs_file:          &str,
    root_type:        &str,
    reference_fields: Option<Vec<String>>,
) {
    let norm = rel_path.replace('\\', "/");
    if let Some(slot) = cfg.overrides.iter_mut().find(|o| o.glob == norm) {
        slot.rs_file   = rs_file.to_string();
        slot.root_type = root_type.to_string();
        if let Some(fields) = reference_fields {
            slot.reference_fields = fields;
        }
    } else {
        cfg.overrides.push(SchemaOverride {
            glob:             norm,
            rs_file:          rs_file.to_string(),
            root_type:        root_type.to_string(),
            reference_fields: reference_fields.unwrap_or_default(),
        });
    }
}

/// Toggle a single field name in the reference_fields list of the
/// override matching `rel_path`. Creates a bare override (empty
/// rs_file/root_type) when no per-file/per-folder binding exists yet —
/// the reader is happy with overrides that only carry patterns.
///
/// Returns `(new_state, scope)` where `new_state` is `true` when the
/// field is now part of the list and `scope` describes which binding
/// was touched ("file", "folder", "default", "fallback").
pub fn toggle_reference_field(
    cfg:      &mut StudioConfig,
    rel_path: &str,
    field:    &str,
) -> (bool, &'static str) {
    let norm = rel_path.replace('\\', "/");

    // Prefer the most-specific existing match (first override hit) so
    // toggling on a deeply-nested file doesn't bubble up into the
    // default unintentionally.
    if let Some(o) = cfg.overrides.iter_mut().find(|o| glob_match(&o.glob, &norm)) {
        let now = toggle_str(&mut o.reference_fields, field);
        let scope = if o.glob == norm { "file" } else { "folder" };
        return (now, scope);
    }

    if let Some(def) = cfg.default.as_mut() {
        let now = toggle_str(&mut def.reference_fields, field);
        return (now, "default");
    }

    // No binding at all — synthesise a per-file override that carries
    // ONLY the reference_fields list. The reader treats the empty
    // rs_file/root_type as "no schema bound for this glob"; the
    // resolver still picks up the patterns.
    cfg.overrides.push(SchemaOverride {
        glob:             norm,
        rs_file:          String::new(),
        root_type:        String::new(),
        reference_fields: vec![field.to_string()],
    });
    (true, "fallback")
}

fn toggle_str(list: &mut Vec<String>, value: &str) -> bool {
    if let Some(i) = list.iter().position(|s| s == value) {
        list.remove(i);
        false
    } else {
        list.push(value.to_string());
        true
    }
}

/// Resolve the schema binding (rs_file + root_type) that applies to
/// `rel_path` using the in-memory config — same first-match-wins
/// precedence the on-disk reader uses, but driven by the synthetic
/// relative path rather than walking up from the absolute path.
///
/// This is the path that makes external bindings work: an entry like
/// `<APPDATA>/saves/foo.ron` lives nowhere near the repo, so the
/// walk-up sidecar lookup in `ron_studio::parse_sidecar_config`
/// always misses it; here we match the override glob
/// (`external/<label>/foo.ron`) directly against the synthetic
/// `relative_path` the scanner produces.
///
/// `rs_file` is returned as an absolute path resolved against
/// `repo_root` (where `.ron-studio.toml` lives), mirroring what
/// `parse_sidecar_file` does so downstream code (schema-probe) sees
/// the same shape regardless of which resolution path got us here.
pub fn resolve_binding(
    cfg:       &StudioConfig,
    repo_root: &str,
    rel_path:  &str,
) -> Option<(String, String)> {
    let norm = rel_path.replace('\\', "/");
    let cfg_dir = Path::new(repo_root);
    for o in &cfg.overrides {
        if glob_match(&o.glob, &norm) {
            if o.rs_file.is_empty() || o.root_type.is_empty() {
                // First-match-wins for binding lookup — if this
                // override exists only to carry reference-field
                // patterns, fall through to the default rather than
                // checking subsequent overrides.
                break;
            }
            let rs_abs = cfg_dir.join(&o.rs_file).to_string_lossy().into_owned();
            return Some((rs_abs, o.root_type.clone()));
        }
    }
    if let Some(def) = &cfg.default {
        if !def.rs_file.is_empty() && !def.root_type.is_empty() {
            let rs_abs = cfg_dir.join(&def.rs_file).to_string_lossy().into_owned();
            return Some((rs_abs, def.root_type.clone()));
        }
    }
    None
}

/// Resolve which reference-field patterns apply to `rel_path`. Returns
/// `None` when nothing is configured at either the matched override or
/// the default level — the caller falls back to the built-in
/// convention in that case.
pub fn resolve_reference_fields(cfg: &StudioConfig, rel_path: &str) -> Option<Vec<String>> {
    let norm = rel_path.replace('\\', "/");
    for o in &cfg.overrides {
        if glob_match(&o.glob, &norm) {
            if !o.reference_fields.is_empty() {
                return Some(o.reference_fields.clone());
            }
            // First match wins for binding lookup too — if this override
            // didn't define fields, fall through to default rather than
            // checking subsequent overrides.
            break;
        }
    }
    if let Some(def) = &cfg.default {
        if !def.reference_fields.is_empty() {
            return Some(def.reference_fields.clone());
        }
    }
    None
}

/// Match a single key against one of the configured patterns. Supports
/// exact match, `*suffix` and `prefix*` wildcards. A pattern that's
/// just `"*"` matches everything (useful as an opt-in "all fields can
/// be references" mode).
pub fn matches_pattern(pattern: &str, key: &str) -> bool {
    if pattern == "*"          { return true; }
    if let Some(suf) = pattern.strip_prefix('*') {
        return key.ends_with(suf);
    }
    if let Some(pre) = pattern.strip_suffix('*') {
        return key.starts_with(pre);
    }
    pattern == key
}

/// Remove the binding that targets exactly `rel_path`. Returns true if
/// something was removed.
pub fn clear_binding(cfg: &mut StudioConfig, rel_path: &str) -> bool {
    let norm = rel_path.replace('\\', "/");
    let before = cfg.overrides.len();
    cfg.overrides.retain(|o| o.glob != norm);
    cfg.overrides.len() != before
}

// ── Glob ────────────────────────────────────────────────────────────────────
// Same `*` / `**` semantics as `ron_studio::glob_match`, simplified for
// "anchored to repo root" use. Duplicated rather than imported to keep
// the studio module free of internal ron_studio dependencies.

fn glob_match(pattern: &str, s: &str) -> bool {
    let pat: Vec<char> = pattern.replace('\\', "/").chars().collect();
    let txt: Vec<char> = s.chars().collect();
    match_helper(&pat, 0, &txt, 0)
}

fn match_helper(pat: &[char], pi: usize, txt: &[char], ti: usize) -> bool {
    let mut pi = pi;
    let mut ti = ti;
    loop {
        if pi == pat.len() { return ti == txt.len(); }
        let c = pat[pi];
        if c == '*' {
            // `**` matches any chars including `/`; `*` matches non-`/`.
            let dbl = pi + 1 < pat.len() && pat[pi + 1] == '*';
            let rest_pi = if dbl { pi + 2 } else { pi + 1 };
            let mut k = ti;
            loop {
                if match_helper(pat, rest_pi, txt, k) { return true; }
                if k >= txt.len() { return false; }
                if !dbl && txt[k] == '/' { return false; }
                k += 1;
            }
        }
        if ti == txt.len() { return false; }
        if c != txt[ti] { return false; }
        pi += 1;
        ti += 1;
    }
}

/// Add (or update) an external location for the repo. Idempotent on
/// the absolute path — re-adding the same one just refreshes the
/// label. Path is canonicalised when it exists so the on-disk file
/// stores something stable regardless of how the user typed it
/// (trailing slashes, forward/backslashes, …).
pub fn add_external(cfg: &mut StudioConfig, path: &str, label: Option<&str>) {
    let norm = normalise_external_path(path);
    let label = label
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_external_label(&norm));
    if let Some(existing) = cfg.externals.iter_mut().find(|e| e.path == norm) {
        existing.label = label;
    } else {
        cfg.externals.push(ExternalEntry { path: norm, label });
    }
}

/// Drop the external whose `path` matches (post-normalisation).
/// No-op when the entry isn't there. Returns `true` when an entry
/// was actually removed so callers can decide whether to skip the
/// downstream rescan.
pub fn remove_external(cfg: &mut StudioConfig, path: &str) -> bool {
    let norm = normalise_external_path(path);
    let len_before = cfg.externals.len();
    cfg.externals.retain(|e| e.path != norm);
    cfg.externals.len() != len_before
}

fn normalise_external_path(p: &str) -> String {
    let trimmed = p.trim();
    let path    = Path::new(trimmed);
    // Try canonicalise — succeeds when the path exists on disk,
    // falls back to a lossy normalisation (lossy on Windows: keeps
    // the original casing of intermediate segments). Either way
    // the result is a stable string we can compare against.
    if let Ok(canon) = path.canonicalize() {
        return canon.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    }
    trimmed.replace('\\', "/")
}

fn default_external_label(path: &str) -> String {
    let p = path.replace('\\', "/");
    let trimmed = p.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
}
