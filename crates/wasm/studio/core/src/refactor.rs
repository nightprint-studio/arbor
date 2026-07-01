//! `core::refactor` — F12 (cross-ref rename) + F13 (query-driven bulk
//! edit) orchestration, lifted from the ~300 LOC of identical glue that
//! was copy-pasted across the 5 `*_studio/backend_impl.rs`.
//!
//! ## What lives here (format-agnostic, unit-testable)
//!
//! - **Site building** for the rename preview from already-aggregated
//!   def/usage data ([`build_rename_sites`], [`collisions_for`]).
//! - **Dirty-blocker** detection ([`dirty_blockers_for`]) + the
//!   [`canonicalise_path_key`] / [`any_affected_dirty`] helpers.
//! - **Bulk-edit site building** with the exact skip-reason taxonomy
//!   (container-on-`set`, eval error, root-delete) — [`build_bulk_site`].
//! - **Op building** from accepted sites via [`RefactorOps::coerce_set_value`]
//!   ([`build_bulk_ops`]), counting applied vs skipped.
//! - The **atomic multi-file flush** for both rename ([`rename_apply_files`])
//!   and project-wide bulk ([`bulk_apply_files`]): group-by-file →
//!   parse+rewrite every file in memory (abort the batch before ANY disk
//!   write on the first failure, FROZEN F12) → flush sequentially through
//!   [`crate::persist`], preserving each file's original encoding + BOM
//!   (FROZEN F16). No rollback of already-written files mid-batch.
//!
//! ## What stays in the format crate (the leaf ops, [`RefactorOps`])
//!
//! Parsing the format to a [`serde_json::Value`], the lossless
//! string-rename re-emit, applying a batch of [`BulkOp`] to text, and the
//! per-format value coercion (Int-vs-Float, Option-wrap, null policy:
//! TOML routes `null` → delete, YAML keeps it native). The 2-phase
//! set-then-delete-reverse-index ordering is the leaf `apply_bulk_ops`'s
//! job (it owns the format's mutation engine), not the orchestrator's.
//!
//! ## What is NOT lifted (deliberately special — see the survey)
//!
//! - **Index aggregation + repo scan** (`crate::studio::index` /
//!   `scan_repo` in the launcher today, `arbor-studio-api` after Stage 4)
//!   stay at the call site: `core` must not depend on the launcher's
//!   `StudioIndex` / `StudioFileKind`. The backend feeds this module the
//!   already-collected def/usage slices, which is exactly the
//!   "synthetic index" the §6 tests drive.
//! - **The RON backend keeps its F12/F13 hand-written.** RON aggregates
//!   the index unfiltered, its `apply_string_rename` takes an indent
//!   arg, its bulk flow splits parse / build-ops / pretty-print with a
//!   `Result`-returning op builder, and its active-doc path goes through
//!   `raw_current`. Forcing it onto the uniform `BulkOp` seam would
//!   change behavior. It still calls `core::persist` + the pure helpers
//!   here. (Same precedent as RON's special diff/query in earlier
//!   stages.)
//! - **The active-doc bulk branch** stays in each backend: it touches
//!   the per-format doc registry / history pipeline (`self.lock()`),
//!   which `core` has no handle on. Only the project-wide flush is lifted.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use arbor_studio_types::prelude::{
    BulkEditAction, BulkEditFailure, BulkEditLiteral, BulkEditResult, BulkEditScope, BulkEditSite,
    BulkEditValueSource, RenameCollision, RenameDirtyBlocker, RenameFailure, RenameResult,
    RenameSite, RenameSiteScope, StudioError, StudioResult,
};

use crate::edit_expr::{self, CompiledExpr};

// ─── RefactorOps — the per-format leaf operations ─────────────────────

/// One resolved bulk-edit operation at a single site, ready for the
/// format's mutation engine. The orchestrator builds these from accepted
/// sites; the format's [`RefactorOps::apply_bulk_ops`] splices them.
#[derive(Debug, Clone, PartialEq)]
pub enum BulkOp {
    /// Install a primitive value at `path`.
    Set { path: Vec<String>, value: SetValue },
    /// Remove the node at `path` from its parent.
    Delete { path: Vec<String> },
}

impl BulkOp {
    pub fn path(&self) -> &[String] {
        match self {
            BulkOp::Set { path, .. } => path,
            BulkOp::Delete { path }  => path,
        }
    }
}

/// Format-agnostic typed primitive for a `set`. Each format maps this to
/// its own value type in [`RefactorOps::apply_bulk_ops`].
#[derive(Debug, Clone, PartialEq)]
pub enum SetValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl SetValue {
    /// Short preview rendering for the "→ new" half of a site row.
    /// Matches the per-format `render_set_preview` shape.
    pub fn preview(&self) -> String {
        match self {
            SetValue::String(s) => format!("\"{s}\""),
            SetValue::Int(i)    => i.to_string(),
            SetValue::Float(f)  => f.to_string(),
            SetValue::Bool(b)   => b.to_string(),
            SetValue::Null      => "null".into(),
        }
    }
}

/// Why a site was rejected during coercion. The orchestrator turns this
/// into a `will_skip` site (preview) or a skipped count (apply); the
/// `DeleteInstead` outcome lets a format (TOML) route a `null` payload
/// to a delete op rather than a set.
#[derive(Debug, Clone, PartialEq)]
pub enum CoerceOutcome {
    /// Install this value.
    Set(SetValue),
    /// `null_handling = AsDelete` — turn the `set null` into a delete.
    DeleteInstead,
}

/// A coercion rejection carrying the user-visible skip reason.
#[derive(Debug, Clone, PartialEq)]
pub struct CoerceSkip(pub String);

/// Per-format hooks the refactor orchestrator needs. Implemented once
/// per format crate (RON opts out of the bulk seam — see module docs).
pub trait RefactorOps: Send + Sync {
    /// Project a parsed doc text to the `Value` used for site matching.
    /// `None` on a parse failure (the file is skipped, not fatal, in the
    /// preview scan; fatal in apply — the orchestrator decides).
    fn parse_to_value(&self, text: &str) -> Option<Value>;

    /// Apply a lossless string rename to every `path` in this file,
    /// re-emitting the text. `paths` are the per-site `field_path`s that
    /// landed in this file.
    fn apply_string_rename(
        &self,
        text:  &str,
        paths: &[Vec<String>],
        new:   &str,
    ) -> StudioResult<String>;

    /// Apply a batch of resolved ops to `text` and re-emit. The 2-phase
    /// (sets, then deletes in reverse-index order) ordering is this
    /// impl's responsibility — it owns the mutation engine.
    fn apply_bulk_ops(&self, text: &str, ops: &[BulkOp]) -> StudioResult<String>;

    /// Format-specific kind string for a value node (FE site row).
    fn node_kind(&self, v: &Value) -> String;
    /// Short preview of the current value (the "old →" half).
    fn preview_for(&self, v: &Value) -> String;

    /// Coerce a resolved raw value to the format's typed set-value,
    /// honoring the node kind + null policy. Returns the skip reason on
    /// a type mismatch.
    fn coerce_set_value(
        &self,
        target_kind: &str,
        raw:         &edit_expr::Value,
    ) -> Result<CoerceOutcome, CoerceSkip>;
}

// ─── Shared path-key helper (identical across all 5 backends) ─────────

/// Normalise a filesystem path for set-membership comparison: forward
/// slashes + lowercase, so case-only / separator-only differences in the
/// FE-supplied source path don't slip past the dirty-blocker.
pub fn canonicalise_path_key(p: &str) -> String {
    p.replace('\\', "/").to_ascii_lowercase()
}

// ─── F12 — rename preview building (FS-free, synthetic-index-testable) ─

/// One definition site fed in from the project index.
#[derive(Debug, Clone)]
pub struct RenameDefInput {
    pub id_value:      String,
    pub absolute_path: String,
    pub relative_path: String,
    pub file_name:     String,
    pub def_path:      Vec<String>,
    pub def_field:     String,
}

/// One reference/usage site fed in from the project index.
#[derive(Debug, Clone)]
pub struct RenameUsageInput {
    pub absolute_path: String,
    pub relative_path: String,
    pub file_name:     String,
    pub field_path:    Vec<String>,
    pub key_name:      String,
}

/// How the definition site presents in the preview. Properties uses
/// `Key` (the dotted key is itself the identifier) with `key_name =
/// id_value`; the structured formats use `Definition` with `key_name =
/// def_field`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefScopeStyle {
    /// `RenameSiteScope::Definition`, `key_name = def_field`.
    Definition,
    /// `RenameSiteScope::Key`, `key_name = id_value`.
    Key,
}

/// Build the deduped, sorted rename-site list from aggregated defs +
/// usages. Defs whose `id_value == old_value` become definition/key
/// sites; usages become reference sites. Dedup key is
/// `(absolute_path, field_path)` — defs win ties (inserted first), the
/// exact ordering the backends relied on.
///
/// Previews are left empty; the caller fills them via
/// [`synth_preview_line`] after reading each file once (the only FS
/// touch, kept at the call site so this stays pure).
pub fn build_rename_sites(
    defs:       &[RenameDefInput],
    usages:     &[RenameUsageInput],
    old_value:  &str,
    def_scope:  DefScopeStyle,
) -> Vec<RenameSite> {
    let mut sites:    Vec<RenameSite> = Vec::new();
    let mut seen_key: HashSet<(String, Vec<String>)> = HashSet::new();

    for d in defs {
        if d.id_value != old_value { continue; }
        let key = (d.absolute_path.clone(), d.def_path.clone());
        if !seen_key.insert(key) { continue; }
        let (scope, key_name) = match def_scope {
            DefScopeStyle::Definition => (RenameSiteScope::Definition, d.def_field.clone()),
            DefScopeStyle::Key        => (RenameSiteScope::Key,        d.id_value.clone()),
        };
        sites.push(RenameSite {
            absolute_path: d.absolute_path.clone(),
            relative_path: d.relative_path.clone(),
            file_name:     d.file_name.clone(),
            field_path:    d.def_path.clone(),
            key_name,
            scope,
            preview:       String::new(),
        });
    }
    for u in usages {
        let key = (u.absolute_path.clone(), u.field_path.clone());
        if !seen_key.insert(key) { continue; }
        sites.push(RenameSite {
            absolute_path: u.absolute_path.clone(),
            relative_path: u.relative_path.clone(),
            file_name:     u.file_name.clone(),
            field_path:    u.field_path.clone(),
            key_name:      u.key_name.clone(),
            scope:         RenameSiteScope::Reference,
            preview:       String::new(),
        });
    }
    sites.sort_by(|a, b| {
        a.relative_path
            .cmp(&b.relative_path)
            .then_with(|| a.field_path.cmp(&b.field_path))
    });
    sites
}

/// Surface every existing definition whose value equals the user's
/// (preview-time) new value — the "target already exists" sticky warning
/// (FROZEN F12, not a hard block). Empty when no usable hint was given.
pub fn collisions_for(defs: &[RenameDefInput], new_value_hint: Option<&str>, old_value: &str) -> Vec<RenameCollision> {
    match new_value_hint {
        Some(hint) if !hint.is_empty() && hint != old_value => defs
            .iter()
            .filter(|d| d.id_value == hint)
            .map(|d| RenameCollision {
                absolute_path: d.absolute_path.clone(),
                relative_path: d.relative_path.clone(),
                field_path:    d.def_path.clone(),
                key_name:      d.def_field.clone(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// An open doc's dirty state, fed in by the FE (mirrors `RenameOpenDoc`
/// / `BulkEditOpenDoc`, which differ only in name on the wire).
#[derive(Debug, Clone)]
pub struct OpenDocState {
    pub doc_id:      String,
    pub source_path: Option<String>,
    pub dirty:       bool,
}

/// The set of canonicalised file paths a refactor will touch.
pub fn affected_path_set<'a, I>(abs_paths: I) -> HashSet<String>
where
    I: IntoIterator<Item = &'a str>,
{
    abs_paths.into_iter().map(canonicalise_path_key).collect()
}

/// Build the dirty-blocker list: open docs that are dirty AND whose
/// source path matches an affected file. Sorted by `doc_id`.
pub fn dirty_blockers_for(open_docs: &[OpenDocState], affected: &HashSet<String>) -> Vec<RenameDirtyBlocker> {
    let mut out: Vec<RenameDirtyBlocker> = open_docs
        .iter()
        .filter(|d| d.dirty)
        .filter(|d| match &d.source_path {
            Some(p) => affected.contains(&canonicalise_path_key(p)),
            None    => false,
        })
        .map(|d| RenameDirtyBlocker {
            doc_id:      d.doc_id.clone(),
            source_path: d.source_path.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.doc_id.cmp(&b.doc_id));
    out
}

/// Defensive apply-time dirty re-check: returns `true` if any open doc
/// is dirty AND affected. The backends abort the apply on `true`.
pub fn any_affected_dirty(open_docs: &[OpenDocState], affected: &HashSet<String>) -> bool {
    open_docs.iter().any(|d| {
        d.dirty
            && d.source_path
                .as_ref()
                .map(|p| affected.contains(&canonicalise_path_key(p)))
                .unwrap_or(false)
    })
}

// ─── F12 — atomic multi-file rename flush ─────────────────────────────

/// Validate inputs, group sites by file, rename every file in memory
/// (abort before any write on the first failure), then flush
/// sequentially preserving each file's encoding + BOM. Mirrors the
/// per-format `rename_apply` project flow exactly.
///
/// `read_decoded` is injected so the unit tests can drive it without a
/// real FS; production passes [`crate::persist::read_decoded`].
pub fn rename_apply_files<O: RefactorOps>(
    ops:       &O,
    old_value: &str,
    new_value: &str,
    sites:     &[RenameSite],
) -> StudioResult<RenameResult> {
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

    // Group sites by absolute_path so each file is parsed once.
    let mut by_file: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
    for s in sites {
        by_file
            .entry(s.absolute_path.clone())
            .or_default()
            .push(s.field_path.clone());
    }

    // Phase A — read + rename in memory. Any failure aborts BEFORE any
    // disk write (FROZEN F12). Each file remembers its own encoding so
    // the flush re-encodes per-file (FROZEN F16).
    struct Pending {
        abs_path:       String,
        new_text:       String,
        encoding_label: String,
        had_bom:        bool,
    }
    let mut pending: Vec<Pending> = Vec::with_capacity(by_file.len());
    for (abs_path, paths) in by_file {
        let f = crate::persist::read_decoded(&abs_path)?;
        let new_text = ops
            .apply_string_rename(&f.text, &paths, new_value)
            .map_err(|e| {
                StudioError::App(format!("Rename in-memory pass failed for {abs_path}: {e}"))
            })?;
        pending.push(Pending {
            abs_path,
            new_text,
            encoding_label: f.encoding_label,
            had_bom:        f.had_bom,
        });
    }

    // Phase B — flush sequentially; per-file IO failures recorded, no
    // rollback of files already written (FROZEN F12).
    let mut written: Vec<String>        = Vec::new();
    let mut failed:  Vec<RenameFailure> = Vec::new();
    for w in pending {
        match crate::persist::write_encoded(&w.abs_path, &w.new_text, &w.encoding_label, w.had_bom) {
            Ok(())  => written.push(w.abs_path),
            Err(e)  => failed.push(RenameFailure {
                absolute_path: w.abs_path,
                message:       e.to_string(),
            }),
        }
    }
    Ok(RenameResult { written_files: written, failed_files: failed })
}

// ─── F13 — bulk-edit site + op building (FS-free, stub-testable) ──────

/// Resolve a value node by `field_path` from a projected root.
pub fn resolve_value_path<'a>(root: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path {
        cur = match cur {
            Value::Object(m) => m.get(seg)?,
            Value::Array(a)  => {
                let i: usize = seg.parse().ok()?;
                a.get(i)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

fn is_container(v: &Value) -> bool {
    matches!(v, Value::Object(_) | Value::Array(_))
}

/// Resolve the raw value-source for a `set` at `target` into an
/// `edit_expr::Value`, then coerce it via the format. Shared by both the
/// preview ([`build_bulk_site`]) and apply ([`build_bulk_ops`]) paths so
/// they can never disagree on skip semantics.
fn coerce_for_site<O: RefactorOps>(
    ops:          &O,
    target:       &Value,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> Result<CoerceOutcome, CoerceSkip> {
    let raw: edit_expr::Value = match value_source {
        Some(BulkEditValueSource::Literal { literal }) => match literal {
            BulkEditLiteral::String(s) => edit_expr::Value::String(s.clone()),
            BulkEditLiteral::Number(n) => edit_expr::Value::Number(*n),
            BulkEditLiteral::Bool(b)   => edit_expr::Value::Bool(*b),
            BulkEditLiteral::Null      => edit_expr::Value::Null,
        },
        Some(BulkEditValueSource::Expression { .. }) => {
            let compiled = compiled
                .ok_or_else(|| CoerceSkip("internal: compiled expression missing".to_string()))?;
            let old = json_to_eval_value(target)
                .ok_or_else(|| CoerceSkip("container node — cannot bind `old`".to_string()))?;
            match compiled.eval(&old) {
                Ok(v)  => v,
                Err(e) => return Err(CoerceSkip(e.0)),
            }
        }
        None => return Err(CoerceSkip("Value source missing for `set` action".into())),
    };
    let kind = ops.node_kind(target);
    ops.coerce_set_value(&kind, &raw)
}

fn json_to_eval_value(v: &Value) -> Option<edit_expr::Value> {
    match v {
        Value::Null      => Some(edit_expr::Value::Null),
        Value::Bool(b)   => Some(edit_expr::Value::Bool(*b)),
        Value::Number(n) => n.as_f64().map(edit_expr::Value::Number),
        Value::String(s) => Some(edit_expr::Value::String(s.clone())),
        _ => None,
    }
}

/// Build one preview site for a hit at `field_path`/`target`. Encodes
/// the exact skip-reason taxonomy the backends shared:
/// - `delete` on the document root → skip ("Cannot delete the document root");
/// - `set` on a container → skip ("descend deeper into the query");
/// - `set` whose coercion fails → skip (the coercion message);
/// - `set null` under `AsDelete` (TOML) → "(removed via null)".
#[allow(clippy::too_many_arguments)]
pub fn build_bulk_site<O: RefactorOps>(
    ops:          &O,
    abs_path:     &str,
    rel_path:     &str,
    file_name:    &str,
    field_path:   &[String],
    target:       &Value,
    action:       BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> BulkEditSite {
    let kind        = ops.node_kind(target);
    let old_preview = ops.preview_for(target);
    let mut will_skip   = false;
    let mut skip_reason = String::new();
    let mut new_preview = String::new();

    match action {
        BulkEditAction::Delete => {
            if field_path.is_empty() {
                will_skip = true;
                skip_reason = "Cannot delete the document root".into();
            } else {
                new_preview = "(removed)".into();
            }
        }
        BulkEditAction::Set => {
            if is_container(target) {
                will_skip = true;
                skip_reason =
                    "`set` cannot target a container node — descend deeper into the query".into();
            } else {
                match coerce_for_site(ops, target, value_source, compiled) {
                    Ok(CoerceOutcome::Set(v)) => new_preview = v.preview(),
                    Ok(CoerceOutcome::DeleteInstead) => {
                        if field_path.is_empty() {
                            will_skip = true;
                            skip_reason = "Cannot delete the document root".into();
                        } else {
                            new_preview = "(removed via null)".into();
                        }
                    }
                    Err(CoerceSkip(reason)) => {
                        will_skip = true;
                        skip_reason = reason;
                    }
                }
            }
        }
    }

    BulkEditSite {
        absolute_path: abs_path.to_string(),
        relative_path: rel_path.to_string(),
        file_name:     file_name.to_string(),
        field_path:    field_path.to_vec(),
        kind,
        old_preview,
        new_preview,
        will_skip,
        skip_reason,
    }
}

/// Build the resolved op list from accepted sites against a parsed root,
/// returning `(ops, applied, skipped)`. Re-resolves each site's value
/// from the (possibly newer) root, re-applying the same skip taxonomy as
/// the preview — defensive against stale FE previews.
pub fn build_bulk_ops<O: RefactorOps>(
    ops:          &O,
    root:         &Value,
    sites:        &[BulkEditSite],
    action:       BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> (Vec<BulkOp>, usize, usize) {
    let mut out:     Vec<BulkOp> = Vec::with_capacity(sites.len());
    let mut applied: usize = 0;
    let mut skipped: usize = 0;
    for site in sites {
        if site.will_skip {
            skipped += 1;
            continue;
        }
        let Some(target) = resolve_value_path(root, &site.field_path) else {
            skipped += 1;
            continue;
        };
        match action {
            BulkEditAction::Delete => {
                if site.field_path.is_empty() {
                    skipped += 1;
                    continue;
                }
                out.push(BulkOp::Delete { path: site.field_path.clone() });
                applied += 1;
            }
            BulkEditAction::Set => {
                match coerce_for_site(ops, target, value_source, compiled) {
                    Ok(CoerceOutcome::Set(v)) => {
                        out.push(BulkOp::Set { path: site.field_path.clone(), value: v });
                        applied += 1;
                    }
                    Ok(CoerceOutcome::DeleteInstead) => {
                        if site.field_path.is_empty() {
                            skipped += 1;
                            continue;
                        }
                        out.push(BulkOp::Delete { path: site.field_path.clone() });
                        applied += 1;
                    }
                    Err(_) => skipped += 1,
                }
            }
        }
    }
    (out, applied, skipped)
}

/// Compile a `Set`-action expression source once. Returns `Ok(None)`
/// when there is nothing to compile (delete, or literal source), and
/// `Err(message)` on a compile error so the preview can surface
/// `expression_error` without building any sites.
pub fn compile_expression(
    action:       BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
) -> Result<Option<CompiledExpr>, String> {
    match (action, value_source) {
        (BulkEditAction::Set, Some(BulkEditValueSource::Expression { source })) => {
            edit_expr::compile(source).map(Some).map_err(|e| e.0)
        }
        _ => Ok(None),
    }
}

// ─── F13 — atomic multi-file bulk flush ───────────────────────────────

/// Project-wide bulk apply: group sites by file, parse + build ops +
/// apply each file in memory (abort before any write on the first parse
/// or apply failure, FROZEN F12), then flush preserving encoding + BOM.
/// Files whose accepted-op set is empty are not written. Returns the
/// roll-up `BulkEditResult` (active-doc state always `None` for the
/// project scope).
///
/// `parse_err_label` builds the per-file parse-failure message (the
/// backends used `"parse {path}: invalid TOML"` etc.).
pub fn bulk_apply_files<O: RefactorOps>(
    ops:             &O,
    sites:           Vec<BulkEditSite>,
    action:          BulkEditAction,
    value_source:    &Option<BulkEditValueSource>,
    compiled:        Option<&CompiledExpr>,
    parse_err_label: impl Fn(&str) -> String,
) -> StudioResult<BulkEditResult> {
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
        let f = crate::persist::read_decoded(&abs_path)?;
        let root = ops
            .parse_to_value(&f.text)
            .ok_or_else(|| StudioError::App(parse_err_label(&abs_path)))?;
        let (file_ops, a, s) =
            build_bulk_ops(ops, &root, &sites_for_file, action, value_source, compiled);
        applied_n += a;
        skipped_n += s;
        if file_ops.is_empty() {
            continue;
        }
        let new_text = ops
            .apply_bulk_ops(&f.text, &file_ops)
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
        match crate::persist::write_encoded(&w.abs_path, &w.new_text, &w.encoding_label, w.had_bom) {
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

/// Default integral-vs-float coercion shared by TOML/YAML/JSON: integer
/// when finite, integral, and inside `i64`; float otherwise. Formats
/// call this from `coerce_set_value` for the numeric case.
pub fn coerce_number_default(n: f64) -> SetValue {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < (i64::MAX as f64) {
        SetValue::Int(n as i64)
    } else {
        SetValue::Float(n)
    }
}

// ─── Bulk-edit preview synthesis of active-doc / project paths ─────────

/// Synthesise the `(absolute, relative, file_name)` triple for an
/// active-doc site from the doc's optional source path. Mirrors the
/// per-format `synth_active_doc_paths`.
pub fn synth_active_doc_paths(source_path: &Option<String>) -> (String, String, String) {
    match source_path {
        Some(p) => {
            let norm = p.replace('\\', "/");
            let name = norm.rsplit('/').next().unwrap_or(&norm).to_string();
            (p.clone(), norm, name)
        }
        None => (
            "(active doc)".to_string(),
            "(active doc)".to_string(),
            "(active doc)".to_string(),
        ),
    }
}

/// Convenience: build the active-doc preview sites from `(path, value)`
/// query pairs (the active-doc branch the backends share, minus the
/// `self.lock()` value fetch).
#[allow(clippy::too_many_arguments)]
pub fn build_active_doc_sites<O: RefactorOps>(
    ops:          &O,
    source_path:  &Option<String>,
    pairs:        Vec<(Vec<String>, Value)>,
    action:       BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> Vec<BulkEditSite> {
    let (abs, rel, name) = synth_active_doc_paths(source_path);
    pairs
        .into_iter()
        .map(|(path, value)| {
            build_bulk_site(
                ops, &abs, &rel, &name, &path, &value, action, value_source, compiled,
            )
        })
        .collect()
}

/// Whether a scope is project-wide (small helper to keep call sites
/// reading declaratively).
pub fn is_project_wide(scope: BulkEditScope) -> bool {
    matches!(scope, BulkEditScope::ProjectWide)
}

#[cfg(test)]
mod tests;
