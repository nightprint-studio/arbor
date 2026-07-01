//! [`PropertiesRefactor`] — the SPECIAL, hand-written F12/F13 refactor
//! for `.properties`.
//!
//! Unlike TOML / YAML (which delegate F12/F13 to `core::refactor` via the
//! uniform `RefactorOps` seam), `.properties` diverges in two
//! behavior-load-bearing ways that the generic seam can't express, so
//! this stays hand-written (sanctioned by the blueprint §2.5 / Stage 2e):
//!
//! * **F12 is key-scoped, not string-leaf.** A rename renames the dotted
//!   *key* itself (with a per-site `Key` / `Value` scope + an `old_value`
//!   to match), not the `(paths, new)` string-leaf seam `core::refactor`
//!   assumes. See [`crate::line_model::apply_rename_in_text`].
//! * **F13 coerces every value to a string with an `(empty)` sentinel**
//!   and renders a divergent preview (`(empty)` for null, `"…"` for a
//!   string) — `core::refactor`'s `SetValue::preview()` renders `null`,
//!   not `(empty)`.
//!
//! The orchestration that *is* generic (index aggregation, repo scan,
//! dirty-blocker, collision, atomic multi-file flush + per-file encoding
//! round-trip) lives at the launcher call site (`studio/project_refactor.rs`),
//! reusing `core::persist` for the encoding-aware flush. This module owns
//! only the `.properties`-specific text transforms + the F13
//! coercion/preview/op-building, exactly as the pre-extraction
//! `properties_studio/backend_impl.rs` did.

use arbor_studio_core::prelude::edit_expr::{CompiledExpr, Value as ExprValue};
use arbor_studio_types::prelude::{
    BulkEditAction, BulkEditLiteral, BulkEditSite, BulkEditValueSource, RenameSiteScope,
};
use serde_json::Value;

use crate::line_model::{PropertiesBulkOp, PropertiesSetValue};
use crate::project;

/// Stateless `.properties` leaf operations for the launcher's
/// project-wide F12/F13 orchestration. All methods are pure functions
/// over text / projected values; the launcher owns the FS + index.
pub struct PropertiesRefactor;

impl PropertiesRefactor {
    /// Project a `.properties` text to the JSON value used for F13 site
    /// matching. `None` on parse failure (best-effort scan policy).
    pub fn parse_to_value(text: &str) -> Option<Value> {
        project::parse_to_value(text)
    }

    /// Kind string for a value node (FE site row).
    pub fn node_kind(v: &Value) -> String {
        project::node_kind(v)
    }

    /// Short preview of the current value (the "old →" half).
    pub fn preview_for(v: &Value) -> String {
        project::preview_for(v)
    }
}

// ── F12 — preview-line synthesis (key-scoped) ───────────────────────────

/// Synth a preview line for a rename site. For `.properties` we have the
/// exact line in the source — match by flat key. Lifted from the
/// pre-extraction `properties_studio/backend_impl.rs::synth_preview_line`.
///
/// `key_name` is the flat dotted key (the site's `id_value` for a Key
/// site, or the key whose value matched for a Reference site); `old_value`
/// is the rename target value the FE typed.
pub fn synth_preview_line(
    text:      &str,
    scope:     &RenameSiteScope,
    key_name:  &str,
    old_value: &str,
) -> String {
    if text.is_empty() { return String::new(); }
    let flat = key_name;
    let mut best: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') { continue; }
        let starts = trimmed.starts_with(flat);
        let contains_old = trimmed.contains(old_value);
        let matches = match scope {
            RenameSiteScope::Key        => starts,
            RenameSiteScope::Reference  => starts && contains_old,
            RenameSiteScope::Definition => starts,
        };
        if matches { best = Some(trimmed); break; }
    }
    let line = best.unwrap_or("").to_string();
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(79).collect::<String>())
    } else {
        line
    }
}

// ── F13 — value coercion + preview + op building (.properties string-only) ─

/// Resolve a value-source against `target` and produce a `PropertiesSetValue`.
/// `.properties` has no native typing — every set ends up as a string
/// (or as the empty sentinel for the null case). Lifted from the
/// pre-extraction `compute_new_value`.
pub fn compute_new_value(
    target:       &Value,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> Result<PropertiesSetValue, String> {
    let raw_value: ExprValue = match value_source {
        Some(BulkEditValueSource::Literal { literal }) => match literal {
            BulkEditLiteral::String(s) => ExprValue::String(s.clone()),
            BulkEditLiteral::Number(n) => ExprValue::Number(*n),
            BulkEditLiteral::Bool(b)   => ExprValue::Bool(*b),
            BulkEditLiteral::Null      => ExprValue::Null,
        },
        Some(BulkEditValueSource::Expression { .. }) => {
            let compiled = compiled.ok_or_else(|| "internal: compiled expression missing".to_string())?;
            let old = json_to_eval_value(target)
                .ok_or_else(|| "container node — cannot bind `old`".to_string())?;
            match compiled.eval(&old) {
                Ok(v) => v,
                Err(e) => return Err(e.0),
            }
        }
        None => return Err("Value source missing for `set` action".into()),
    };

    Ok(match raw_value {
        ExprValue::Null      => PropertiesSetValue::Empty,
        ExprValue::Bool(b)   => PropertiesSetValue::String(b.to_string()),
        ExprValue::Number(n) => PropertiesSetValue::String(if n.fract() == 0.0 {
            (n as i64).to_string()
        } else {
            n.to_string()
        }),
        ExprValue::String(s) => PropertiesSetValue::String(s),
    })
}

fn json_to_eval_value(v: &Value) -> Option<ExprValue> {
    match v {
        Value::Null      => Some(ExprValue::Null),
        Value::Bool(b)   => Some(ExprValue::Bool(*b)),
        Value::Number(n) => n.as_f64().map(ExprValue::Number),
        Value::String(s) => Some(ExprValue::String(s.clone())),
        _ => None,
    }
}

/// Divergent F13 preview: `(empty)` for the null/empty case, `"…"` for a
/// string. Lifted from the pre-extraction `render_set_preview`.
pub fn render_set_preview(v: &PropertiesSetValue) -> String {
    match v {
        PropertiesSetValue::String(s) => format!("\"{s}\""),
        PropertiesSetValue::Empty     => "(empty)".into(),
    }
}

/// Build one F13 preview site for a hit at `field_path`/`target`. Encodes
/// the same skip-reason taxonomy as the structured formats, but with the
/// `.properties` string-only coercion + `(empty)` preview. Lifted from
/// `build_site_for_preview`.
#[allow(clippy::too_many_arguments)]
pub fn build_site_for_preview(
    abs_path:     &str,
    rel_path:     &str,
    file_name:    &str,
    field_path:   &[String],
    target:       &Value,
    action:       BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> BulkEditSite {
    let kind        = project::node_kind(target);
    let old_preview = project::preview_for(target);
    let mut will_skip   = false;
    let mut skip_reason = String::new();
    let mut new_preview = String::new();

    let is_container = matches!(target, Value::Object(_) | Value::Array(_));

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
            if is_container {
                will_skip = true;
                skip_reason = "`set` cannot target a container node — descend deeper into the query".into();
            } else {
                match compute_new_value(target, value_source, compiled) {
                    Ok(v)       => new_preview = render_set_preview(&v),
                    Err(reason) => {
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

/// Build the resolved op list from accepted F13 sites against a parsed
/// root, returning `(ops, applied, skipped)`. Lifted from
/// `build_ops_from_sites`.
pub fn build_ops_from_sites(
    root_value:   &Value,
    sites:        &[BulkEditSite],
    action:       BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> (Vec<(Vec<String>, PropertiesBulkOp)>, usize, usize) {
    let mut ops:     Vec<(Vec<String>, PropertiesBulkOp)> = Vec::with_capacity(sites.len());
    let mut applied: usize = 0;
    let mut skipped: usize = 0;
    for site in sites {
        if site.will_skip { skipped += 1; continue; }
        let Some(target) = resolve_value_path(root_value, &site.field_path) else {
            skipped += 1;
            continue;
        };
        match action {
            BulkEditAction::Delete => {
                if site.field_path.is_empty() { skipped += 1; continue; }
                ops.push((site.field_path.clone(), PropertiesBulkOp::Delete));
                applied += 1;
            }
            BulkEditAction::Set => {
                match compute_new_value(target, value_source, compiled) {
                    Ok(v) => {
                        ops.push((site.field_path.clone(), PropertiesBulkOp::Set(v)));
                        applied += 1;
                    }
                    Err(_) => skipped += 1,
                }
            }
        }
    }
    (ops, applied, skipped)
}

fn resolve_value_path<'a>(root: &'a Value, path: &[String]) -> Option<&'a Value> {
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

/// Synthesise the `(absolute, relative, file_name)` triple for an
/// active-doc F13 site from the doc's optional source path. Lifted from
/// `synth_active_doc_paths`.
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

#[cfg(test)]
mod tests;
