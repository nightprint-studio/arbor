//! Orchestration of the **project-wide** Studio refactor ops that
//! `arbor_studio_core::DefaultBackend` deliberately leaves `Unsupported`
//! because they need the repo scanner / `StudioIndex` (which `core` and
//! the format crates must not name).
//!
//! `DefaultBackend` implements active-doc F13 + the FS-only `rename_apply`;
//! the operations here are the gap:
//!
//!   · [`list_files`]            — the repo scan for a format's file kind.
//!   · [`rename_preview`]        — F12 project-wide preview (index → sites).
//!   · [`bulk_edit_preview_pw`]  — F13 `ProjectWide` preview (scan → sites).
//!   · [`bulk_edit_apply_pw`]    — F13 `ProjectWide` apply (atomic flush).
//!
//! Each is generic over a `&dyn RefactorOps` + a `StudioFileKind`, so each
//! simple-format routes through the same helper instead of re-deriving the
//! orchestration. The F12/F13 behavior (multi-file atomic, dirty-blocker,
//! collision, 2-phase delete) is identical to the pre-extraction
//! `toml_studio/backend_impl.rs` flow — only the call site moved.

use std::collections::BTreeMap;

use arbor_studio_core::prelude::{self as core, refactor, DefScopeStyle, RefactorOps};
use arbor_studio_types::prelude::{
    BulkEditAction, BulkEditOpenDoc, BulkEditPreview, BulkEditResult, BulkEditScope, BulkEditSite,
    BulkEditValueSource, FileEntry, RenameFailure, RenameOpenDoc, RenamePreview, RenameResult,
    RenameSite, RenameSiteScope, StudioError, StudioResult,
};

use crate::scanner::{self, StudioFileKind};
use crate::{index, refactor_glue};

/// Scan the repo for one format's file kind (the `list_files` body the
/// extracted backends no longer carry).
pub fn list_files(folder: String, kind: StudioFileKind) -> StudioResult<Vec<FileEntry>> {
    let entries = scanner::scan_repo(&folder, &[kind])?;
    Ok(entries
        .into_iter()
        .map(|e| FileEntry {
            absolute_path: e.absolute_path,
            relative_path: e.relative_path,
            name:          e.name,
            size_bytes:    e.size_bytes,
        })
        .collect())
}

/// F12 project-wide rename preview: refresh the index slice for `kind`,
/// build the sites, fill per-site preview lines via `synth_preview`, detect
/// collisions + dirty blockers. Mirrors the pre-extraction flow exactly.
pub fn rename_preview(
    repo_root:      String,
    kind:           StudioFileKind,
    old_value:      String,
    new_value_hint: Option<String>,
    open_docs:      Vec<RenameOpenDoc>,
    synth_preview:  impl Fn(&str, &str, &str) -> String,
) -> StudioResult<RenamePreview> {
    if old_value.is_empty() {
        return Err(StudioError::App("Rename target value is empty".into()));
    }
    let idx = match index::refresh_for(&repo_root, &[kind], None) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(
                "rename_preview ({kind:?}): index refresh failed, falling back to fresh scan ({e})"
            );
            index::load(&repo_root)
        }
    };

    let kinds = [kind];
    let defs   = refactor_glue::collect_rename_defs(&idx, &kinds);
    let usages = refactor_glue::collect_rename_usages(&idx, &old_value, &kinds);

    let mut sites = refactor::build_rename_sites(
        &defs, &usages, &old_value, refactor::DefScopeStyle::Definition,
    );
    // Best-effort line-snippet preview, reading each file once.
    let mut file_text_cache: BTreeMap<String, String> = BTreeMap::new();
    for site in sites.iter_mut() {
        let text = file_text_cache
            .entry(site.absolute_path.clone())
            .or_insert_with(|| core::persist::read_to_string_lossy(&site.absolute_path));
        site.preview = synth_preview(text, &site.key_name, &old_value);
    }
    let collisions = refactor::collisions_for(&defs, new_value_hint.as_deref(), &old_value);
    let affected   = refactor::affected_path_set(sites.iter().map(|s| s.absolute_path.as_str()));
    let dirty_blockers = refactor::dirty_blockers_for(
        &refactor_glue::rename_open_doc_states(open_docs), &affected,
    );

    Ok(RenamePreview { sites, dirty_blockers, collisions })
}

/// TOML-aware preview line for F12: TOML keys live on their own line as
/// `key = "value"`. Look for a line that mentions both the key and the
/// quoted value; fall back to a value-only match (handy when the key was
/// promoted to a `[section]` header). Lifted from the pre-extraction
/// `toml_studio/backend_impl.rs::synth_preview_line`.
pub fn toml_synth_preview_line(text: &str, key: &str, value: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let needle_val = format!("\"{value}\"");
    let mut best: Option<&str> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() { continue; }
        let has_key = l.starts_with(key) || l.contains(&format!("\"{key}\""));
        if has_key && l.contains(&needle_val) {
            best = Some(l);
            break;
        }
        if best.is_none() && l.contains(&needle_val) {
            best = Some(l);
        }
    }
    let line = best.unwrap_or("").to_string();
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(79).collect::<String>())
    } else {
        line
    }
}

/// YAML-aware preview line for F12: match on `key:` + the value text on
/// the same line. YAML allows both `key: "value"` (quoted) and
/// `key: value` (unquoted), so we look for either shape; fall back to a
/// value-only match when key+value sit on separate lines (block-scalar
/// style). Lifted from the pre-extraction
/// `yaml_studio/backend_impl.rs::synth_preview_line`.
pub fn yaml_synth_preview_line(text: &str, key: &str, value: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let needle_quoted = format!("\"{value}\"");
    let needle_singleq = format!("'{value}'");
    let mut best: Option<&str> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() { continue; }
        let has_key = l.starts_with(key)
            || l.starts_with(&format!("- {key}"))
            || l.contains(&format!("{key}:"));
        let has_val = l.contains(&needle_quoted)
            || l.contains(&needle_singleq)
            || l.contains(value);
        if has_key && has_val {
            best = Some(l);
            break;
        }
        if best.is_none() && has_val {
            best = Some(l);
        }
    }
    let line = best.unwrap_or("").to_string();
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(79).collect::<String>())
    } else {
        line
    }
}

/// F12 apply when the backend (DefaultBackend) leaves project-wide preview
/// to us but still owns the FS-only apply: this is a thin pass-through to
/// `core::refactor::rename_apply_files` with the supplied `RefactorOps`,
/// used only if a caller wants apply orchestrated here too. The default
/// path keeps using `backend.rename_apply` (DefaultBackend implements it).
#[allow(dead_code)]
pub fn rename_apply<O: RefactorOps>(
    ops:       &O,
    old_value: &str,
    new_value: &str,
    sites:     &[RenameSite],
) -> StudioResult<RenameResult> {
    refactor::rename_apply_files(ops, old_value, new_value, sites)
}

/// F13 `ProjectWide` preview: scan the repo, parse + query each file, build
/// the bulk sites + dirty blockers. Mirrors the pre-extraction flow.
#[allow(clippy::too_many_arguments)]
pub fn bulk_edit_preview_pw<O: RefactorOps>(
    ops:          &O,
    repo_root:    String,
    kind:         StudioFileKind,
    query:        String,
    action:       BulkEditAction,
    value_source: Option<BulkEditValueSource>,
    open_docs:    Vec<BulkEditOpenDoc>,
) -> StudioResult<BulkEditPreview> {
    let compiled = match refactor::compile_expression(action, &value_source) {
        Ok(c) => c,
        Err(e) => {
            return Ok(BulkEditPreview {
                sites:            Vec::new(),
                dirty_blockers:   Vec::new(),
                expression_error: Some(e),
            })
        }
    };

    let mut sites: Vec<BulkEditSite> = Vec::new();
    let files = scanner::scan_repo(&repo_root, &[kind])?;
    for f in &files {
        if f.excluded { continue; }
        let text = core::persist::read_to_string_lossy(&f.absolute_path);
        let Some(root) = ops.parse_to_value(&text) else { continue; };
        let pairs = match core::query::run(&root, &query, 500) {
            Ok(locs) => locs.into_iter().map(|l| (l.path, l.value)).collect::<Vec<_>>(),
            Err(_)   => continue,
        };
        for (path, pair_value) in pairs {
            sites.push(refactor::build_bulk_site(
                ops,
                &f.absolute_path,
                &f.relative_path,
                &f.name,
                &path, &pair_value,
                action, &value_source, compiled.as_ref(),
            ));
        }
    }
    sites.sort_by(|a, b| {
        a.relative_path
            .cmp(&b.relative_path)
            .then_with(|| a.field_path.cmp(&b.field_path))
    });

    let affected = refactor::affected_path_set(sites.iter().map(|s| s.absolute_path.as_str()));
    let dirty_blockers = refactor::dirty_blockers_for(
        &refactor_glue::bulk_open_doc_states(open_docs), &affected,
    );

    Ok(BulkEditPreview { sites, dirty_blockers, expression_error: None })
}

/// Kind-dispatched F13 `ProjectWide` preview for the `DefaultBackend`-riding
/// simple formats (TOML / YAML). Picks the per-format `RefactorOps` impl so
/// the launcher's IPC seam doesn't have to name the format crates.
pub fn bulk_edit_preview_pw_for(
    repo_root:    String,
    kind:         StudioFileKind,
    query:        String,
    action:       BulkEditAction,
    value_source: Option<BulkEditValueSource>,
    open_docs:    Vec<BulkEditOpenDoc>,
) -> StudioResult<BulkEditPreview> {
    match kind {
        StudioFileKind::Yaml => bulk_edit_preview_pw(
            &arbor_studio_yaml::prelude::YamlRefactor,
            repo_root, kind, query, action, value_source, open_docs,
        ),
        _ => bulk_edit_preview_pw(
            &arbor_studio_toml::prelude::TomlRefactor,
            repo_root, kind, query, action, value_source, open_docs,
        ),
    }
}

/// F13 `ProjectWide` apply: dirty re-check then the atomic multi-file flush.
///
/// Unused today — `DefaultBackend::bulk_edit_apply` already implements the
/// `ProjectWide` apply (it needs only the FE-supplied sites, not the index),
/// so the apply handler stays on the backend. Kept as the documented seam in
/// case a future format needs apply orchestrated here too.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn bulk_edit_apply_pw<O: RefactorOps>(
    ops:             &O,
    kind:            StudioFileKind,
    action:          BulkEditAction,
    value_source:    Option<BulkEditValueSource>,
    sites:           Vec<BulkEditSite>,
    open_docs:       Vec<BulkEditOpenDoc>,
) -> StudioResult<BulkEditResult> {
    let compiled = refactor::compile_expression(action, &value_source)
        .map_err(|e| StudioError::App(format!("Expression compile error: {e}")))?;

    let affected = refactor::affected_path_set(sites.iter().map(|s| s.absolute_path.as_str()));
    if refactor::any_affected_dirty(&refactor_glue::bulk_open_doc_states(open_docs), &affected) {
        return Err(StudioError::App(
            "Some affected files have unsaved changes. Save or discard first.".into(),
        ));
    }

    let label = format!("{kind:?}").to_ascii_lowercase();
    refactor::bulk_apply_files(
        ops, sites, action, &value_source, compiled.as_ref(),
        move |p| format!("parse {p}: invalid {label}"),
    )
}

// ── .properties — the SPECIAL hand-written F12/F13 orchestration ──────────
//
// `.properties` cannot ride the generic helpers above:
//   · F12 is key-scoped (renames the dotted KEY itself with a per-site
//     Key/Value scope + an `old_value`), not the string-leaf `(paths, new)`
//     seam `core::refactor::rename_apply_files` assumes.
//   · F13 coerces every value to a string with an `(empty)` sentinel and
//     renders a divergent preview, which `core::refactor`'s `SetValue` can't
//     express.
//
// So these mirror the pre-extraction `properties_studio/backend_impl.rs`
// project flows exactly, delegating the `.properties`-specific transforms to
// `arbor_studio_properties` and reusing only the index/scan + `core::persist`
// flush. (The ACTIVE-doc F13 apply stays on `DefaultBackend` — its generic
// string-coercion mutate seam produces byte-identical files as one history
// step; only the preview is synthesised here so the `(empty)` rendering is
// exact.)

/// F12 project-wide rename preview for `.properties`. Builds Key-scope
/// definition sites (every dotted key is the identifier) + Reference-scope
/// usage sites, fills the line-snippet preview via the scope-aware
/// `.properties` synth, then collisions + dirty blockers.
pub fn properties_rename_preview(
    repo_root:      String,
    old_value:      String,
    new_value_hint: Option<String>,
    open_docs:      Vec<RenameOpenDoc>,
) -> StudioResult<RenamePreview> {
    if old_value.is_empty() {
        return Err(StudioError::App("Rename target value is empty".into()));
    }
    let kind = StudioFileKind::Properties;
    let idx = match index::refresh_for(&repo_root, &[kind], None) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(
                "rename_preview (properties): index refresh failed, falling back to fresh scan ({e})"
            );
            index::load(&repo_root)
        }
    };

    let kinds = [kind];
    let defs   = refactor_glue::collect_rename_defs(&idx, &kinds);
    let usages = refactor_glue::collect_rename_usages(&idx, &old_value, &kinds);

    // `.properties` def sites are Key-scope (the dotted key IS the id_value).
    let mut sites = refactor::build_rename_sites(&defs, &usages, &old_value, DefScopeStyle::Key);

    let mut file_text_cache: BTreeMap<String, String> = BTreeMap::new();
    for site in sites.iter_mut() {
        let text = file_text_cache
            .entry(site.absolute_path.clone())
            .or_insert_with(|| core::persist::read_to_string_lossy(&site.absolute_path));
        site.preview = arbor_studio_properties::prelude::synth_preview_line(
            text, &site.scope, &site.key_name, &old_value,
        );
    }
    let collisions = refactor::collisions_for(&defs, new_value_hint.as_deref(), &old_value);
    let affected   = refactor::affected_path_set(sites.iter().map(|s| s.absolute_path.as_str()));
    let dirty_blockers = refactor::dirty_blockers_for(
        &refactor_glue::rename_open_doc_states(open_docs), &affected,
    );

    Ok(RenamePreview { sites, dirty_blockers, collisions })
}

/// F12 apply for `.properties` (key-scoped). Group sites by file, map each
/// site's `RenameSiteScope` → `.properties` Key/Value scope, rename in
/// memory (abort before any write on the first failure, FROZEN F12), then
/// flush sequentially preserving each file's encoding + BOM (FROZEN F16).
pub fn properties_rename_apply(
    old_value: String,
    new_value: String,
    sites:     Vec<RenameSite>,
    open_docs: Vec<RenameOpenDoc>,
) -> StudioResult<RenameResult> {
    use arbor_studio_properties::prelude::{
        apply_rename_in_text, PropertiesRenameScope, PropertiesRenameSite,
    };

    if new_value.is_empty() {
        return Err(StudioError::App("New value is empty".into()));
    }
    if new_value == old_value {
        return Err(StudioError::App(
            "New value equals old value — nothing to rename".into(),
        ));
    }
    if sites.is_empty() {
        return Err(StudioError::App("No sites selected for rename".into()));
    }

    let affected = refactor::affected_path_set(sites.iter().map(|s| s.absolute_path.as_str()));
    if refactor::any_affected_dirty(&refactor_glue::rename_open_doc_states(open_docs), &affected) {
        return Err(StudioError::App(
            "Some affected files have unsaved changes. Save or discard first.".into(),
        ));
    }

    let mut by_file: BTreeMap<String, Vec<RenameSite>> = BTreeMap::new();
    for s in sites {
        by_file.entry(s.absolute_path.clone()).or_default().push(s);
    }

    struct Pending {
        abs_path:       String,
        new_text:       String,
        encoding_label: String,
        had_bom:        bool,
    }
    let mut pending: Vec<Pending> = Vec::with_capacity(by_file.len());
    for (abs_path, sites_for_file) in by_file {
        let f = core::persist::read_decoded(&abs_path)?;
        let props_sites: Vec<PropertiesRenameSite> = sites_for_file
            .into_iter()
            .map(|s| PropertiesRenameSite {
                field_path: s.field_path,
                scope: match s.scope {
                    RenameSiteScope::Key       => PropertiesRenameScope::Key,
                    RenameSiteScope::Reference => PropertiesRenameScope::Value,
                    // Definition shouldn't occur for properties; map to Key
                    // defensively — every key is a def.
                    RenameSiteScope::Definition => PropertiesRenameScope::Key,
                },
            })
            .collect();
        let new_text = apply_rename_in_text(&f.text, &props_sites, &old_value, &new_value)
            .map_err(|e| StudioError::App(format!(
                "Rename in-memory pass failed for {abs_path}: {e}"
            )))?;
        pending.push(Pending {
            abs_path,
            new_text,
            encoding_label: f.encoding_label,
            had_bom:        f.had_bom,
        });
    }

    let mut written: Vec<String>        = Vec::new();
    let mut failed:  Vec<RenameFailure> = Vec::new();
    for w in pending {
        match core::persist::write_encoded(&w.abs_path, &w.new_text, &w.encoding_label, w.had_bom) {
            Ok(())  => written.push(w.abs_path),
            Err(e)  => failed.push(RenameFailure {
                absolute_path: w.abs_path,
                message:       e.to_string(),
            }),
        }
    }
    Ok(RenameResult { written_files: written, failed_files: failed })
}

/// F13 preview for `.properties` (active-doc OR project-wide). Uses the
/// `.properties` string-coercion + `(empty)` divergent preview. `active`
/// carries the active doc's `(source_path, text)` when scope is ActiveDoc;
/// `None` for ProjectWide (which scans the repo).
pub fn properties_bulk_preview(
    repo_root:    String,
    scope:        BulkEditScope,
    active:       Option<(Option<String>, String)>,
    query:        String,
    action:       BulkEditAction,
    value_source: Option<BulkEditValueSource>,
    open_docs:    Vec<BulkEditOpenDoc>,
) -> StudioResult<BulkEditPreview> {
    use arbor_studio_properties::prelude::{build_site_for_preview, synth_active_doc_paths};

    let compiled = match refactor::compile_expression(action, &value_source) {
        Ok(c) => c,
        Err(e) => {
            return Ok(BulkEditPreview {
                sites:            Vec::new(),
                dirty_blockers:   Vec::new(),
                expression_error: Some(e),
            })
        }
    };

    match scope {
        BulkEditScope::ActiveDoc => {
            let (source_path, text) = active.ok_or_else(|| {
                StudioError::App("active-doc bulk preview needs the active doc text".into())
            })?;
            let (abs, rel, name) = synth_active_doc_paths(&source_path);
            let root = arbor_studio_properties::parse_to_value(&text).unwrap_or(serde_json::Value::Null);
            let pairs = match core::query::run(&root, &query, 500) {
                Ok(locs) => locs.into_iter().map(|l| (l.path, l.value)).collect::<Vec<_>>(),
                Err(e)   => return Err(StudioError::App(e.0)),
            };
            let sites = pairs
                .into_iter()
                .map(|(path, value)| {
                    build_site_for_preview(
                        &abs, &rel, &name, &path, &value, action, &value_source, compiled.as_ref(),
                    )
                })
                .collect();
            Ok(BulkEditPreview { sites, dirty_blockers: Vec::new(), expression_error: None })
        }
        BulkEditScope::ProjectWide => {
            let mut sites: Vec<BulkEditSite> = Vec::new();
            let files = scanner::scan_repo(&repo_root, &[StudioFileKind::Properties])?;
            for f in &files {
                if f.excluded { continue; }
                let text = core::persist::read_to_string_lossy(&f.absolute_path);
                let Some(root) = arbor_studio_properties::parse_to_value(&text) else { continue; };
                let pairs = match core::query::run(&root, &query, 500) {
                    Ok(locs) => locs.into_iter().map(|l| (l.path, l.value)).collect::<Vec<_>>(),
                    Err(_)   => continue,
                };
                for (path, pair_value) in pairs {
                    sites.push(build_site_for_preview(
                        &f.absolute_path, &f.relative_path, &f.name,
                        &path, &pair_value, action, &value_source, compiled.as_ref(),
                    ));
                }
            }
            sites.sort_by(|a, b| {
                a.relative_path
                    .cmp(&b.relative_path)
                    .then_with(|| a.field_path.cmp(&b.field_path))
            });
            let affected = refactor::affected_path_set(sites.iter().map(|s| s.absolute_path.as_str()));
            let dirty_blockers = refactor::dirty_blockers_for(
                &refactor_glue::bulk_open_doc_states(open_docs), &affected,
            );
            Ok(BulkEditPreview { sites, dirty_blockers, expression_error: None })
        }
    }
}

/// F13 project-wide apply for `.properties` (the `(empty)`-sentinel,
/// string-coercion bulk). Mirrors the pre-extraction project flow: group
/// by file, parse + build ops + apply in memory (abort before any write on
/// the first failure), then flush preserving encoding + BOM. (ActiveDoc
/// apply stays on `DefaultBackend`.)
pub fn properties_bulk_apply_pw(
    action:       BulkEditAction,
    value_source: Option<BulkEditValueSource>,
    sites:        Vec<BulkEditSite>,
    open_docs:    Vec<BulkEditOpenDoc>,
) -> StudioResult<BulkEditResult> {
    use arbor_studio_properties::prelude::{
        apply_bulk_edits_text, build_ops_from_sites,
    };
    use arbor_studio_types::prelude::BulkEditFailure;

    let compiled = refactor::compile_expression(action, &value_source)
        .map_err(|e| StudioError::App(format!("Expression compile error: {e}")))?;

    let affected = refactor::affected_path_set(sites.iter().map(|s| s.absolute_path.as_str()));
    if refactor::any_affected_dirty(&refactor_glue::bulk_open_doc_states(open_docs), &affected) {
        return Err(StudioError::App(
            "Some affected files have unsaved changes. Save or discard first.".into(),
        ));
    }

    let mut by_file: BTreeMap<String, Vec<BulkEditSite>> = BTreeMap::new();
    for s in sites {
        by_file.entry(s.absolute_path.clone()).or_default().push(s);
    }

    struct Pending {
        abs_path:       String,
        new_text:       String,
        encoding_label: String,
        had_bom:        bool,
    }
    let mut pending:   Vec<Pending> = Vec::with_capacity(by_file.len());
    let mut applied_n: usize        = 0;
    let mut skipped_n: usize        = 0;
    for (abs_path, sites_for_file) in by_file {
        let f = core::persist::read_decoded(&abs_path)?;
        let root = arbor_studio_properties::parse_to_value(&f.text)
            .ok_or_else(|| StudioError::App(format!("parse {abs_path}: invalid .properties")))?;
        let (ops, a, s) =
            build_ops_from_sites(&root, &sites_for_file, action, &value_source, compiled.as_ref());
        applied_n += a;
        skipped_n += s;
        if ops.is_empty() { continue; }
        let new_text = apply_bulk_edits_text(&f.text, &ops)
            .map_err(|e| StudioError::App(format!("Apply edits to {abs_path}: {e}")))?;
        pending.push(Pending {
            abs_path,
            new_text,
            encoding_label: f.encoding_label,
            had_bom:        f.had_bom,
        });
    }

    let mut written: Vec<String>          = Vec::new();
    let mut failed:  Vec<BulkEditFailure> = Vec::new();
    for w in pending {
        match core::persist::write_encoded(&w.abs_path, &w.new_text, &w.encoding_label, w.had_bom) {
            Ok(())  => written.push(w.abs_path),
            Err(e)  => failed.push(BulkEditFailure {
                absolute_path: w.abs_path,
                message:       e.to_string(),
            }),
        }
    }
    Ok(BulkEditResult {
        written_files:    written,
        failed_files:     failed,
        applied_sites:    applied_n,
        skipped_sites:    skipped_n,
        active_doc_state: None,
    })
}
