//! F13 — Query-driven bulk edit helpers for JSON Studio.
//!
//! Mirrors `ron_studio::{BulkSetValue, BulkEditOp, apply_bulk_edits_*}`
//! but operates on the *byte-splice* model (Phase 3.b lossless edits)
//! instead of "mutate AST → re-pretty-print". Each op produces a
//! `(span, replacement)` pair against the live text buffer; the loop
//! re-parses between splices so paths resolve against the current
//! state. That keeps the apply step simple and avoids hand-rolling a
//! span-shifting allocator for overlapping cut ranges.
//!
//! Sets run before deletes (same phase ordering as RON). Within the
//! delete phase, ops are grouped by parent and sorted in numeric-
//! aware descending child-index order so removing list/array entries
//! never shifts the indices of later removals.

use std::collections::BTreeMap;

use crate::error::{AppError, Result};

use super::ast::{self, JsonAst, Span};
use super::edits;

/// Concrete value to install at a `set` site. Maps roughly to
/// `studio::edit_expr::Value` but pre-shaped for the splice writer:
/// the encoder produces the exact JSON literal text that lands in the
/// buffer.
#[derive(Debug, Clone)]
pub enum JsonSetValue {
    String(String),
    Number(f64),
    Bool(bool),
    /// First-class JSON null (descriptor `null_handling = Native`).
    Null,
}

/// One edit op applied to the JSON buffer.
#[derive(Debug, Clone)]
pub enum JsonBulkOp {
    Set(JsonSetValue),
    Delete,
}

/// Apply a batch of `JsonBulkOp`s to a JSON document text and return
/// the regenerated source. Used by both the active-doc and project-
/// wide flows of F13.
///
/// Position-preserving: every byte outside the spliced ranges survives
/// a round-trip unchanged. Each iteration re-parses the current text so
/// the next op's path resolves against the latest state — that lets
/// adjacent sibling deletes coexist without overlap-detection logic.
///
/// Returns the new text on success. On any per-op failure (path not
/// found, set targeting a container, document-root delete) the
/// function aborts BEFORE the caller flushes — matches the F12/F13
/// "atomic pre-flush" rule (no partial edits).
pub fn apply_bulk_edits_text(
    input: &str,
    ops:   &[(Vec<String>, JsonBulkOp)],
) -> Result<String> {
    let mut text = input.to_string();

    // Phase A — sets. Order among sets doesn't matter (each operates on
    // a scalar leaf; leaf spans don't overlap with each other).
    for (path, op) in ops {
        let JsonBulkOp::Set(val) = op else { continue; };
        let ast = ast::parse_with(&text, /* strict */ false)
            .map_err(|e| AppError::Other(format!("parse: {e}")))?;
        let target = ast::resolve(&ast, path).ok_or_else(|| AppError::Other(format!(
            "Set site path not found: {}",
            path.join("/"),
        )))?;
        let span = match target {
            JsonAst::Object(_) | JsonAst::Array(_) => {
                return Err(AppError::Other(format!(
                    "Set cannot target a container at {}", path.join("/"),
                )));
            }
            other => other.span(),
        };
        let lit = encode_set_value(val);
        text = splice(&text, span, &lit);
    }

    // Phase B — deletes. Group by parent path; sort each parent's keys
    // numeric-aware descending so list/array index removals never shift
    // earlier indices we still need to find. Iterate parents in
    // *descending* path order so deeper parents go first (deleting an
    // ancestor wipes out the descendant references we'd otherwise
    // try to resolve).
    let mut by_parent: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
    for (path, op) in ops {
        if !matches!(op, JsonBulkOp::Delete) { continue; }
        if path.is_empty() {
            return Err(AppError::Other(
                "Cannot delete the document root".into(),
            ));
        }
        let (key, parent) = path.split_last().unwrap();
        by_parent.entry(parent.to_vec()).or_default().push(key.clone());
    }

    for (parent_path, mut keys) in by_parent.into_iter().rev() {
        keys.sort_by(|a, b| match (a.parse::<i64>().ok(), b.parse::<i64>().ok()) {
            (Some(ai), Some(bi)) => bi.cmp(&ai),
            _ => b.cmp(a),
        });
        keys.dedup();
        for k in &keys {
            let ast = ast::parse_with(&text, /* strict */ false)
                .map_err(|e| AppError::Other(format!("parse: {e}")))?;
            let mut full = parent_path.clone();
            full.push(k.clone());
            text = edits::remove_at(&text, &ast, &full)?;
        }
    }

    Ok(text)
}

/// Render a `JsonSetValue` as a JSON literal suitable for byte-splice
/// into the buffer. Strings round-trip through `serde_json::to_string`
/// so escapes / quotes are correct; numbers prefer integer form when
/// integral (matches the user's intuition for `old + 1` on an int
/// field).
pub fn encode_set_value(v: &JsonSetValue) -> String {
    match v {
        JsonSetValue::String(s) => serde_json::to_string(s).unwrap_or_else(|_| {
            // Fallback: hand-roll a minimal escape if serde dies (it
            // shouldn't on any valid utf-8 string).
            let escaped: String = s.chars().flat_map(|c| match c {
                '"' => vec!['\\', '"'],
                '\\' => vec!['\\', '\\'],
                '\n' => vec!['\\', 'n'],
                '\r' => vec!['\\', 'r'],
                '\t' => vec!['\\', 't'],
                _ => vec![c],
            }).collect();
            format!("\"{escaped}\"")
        }),
        JsonSetValue::Number(n) => format_number_literal(*n),
        JsonSetValue::Bool(b)   => b.to_string(),
        JsonSetValue::Null      => "null".to_string(),
    }
}

fn format_number_literal(n: f64) -> String {
    if n.is_nan() || n.is_infinite() {
        // JSON has no NaN/Infinity literals; surface a stable string so
        // the user sees the value rather than getting a silent corrupt
        // file. The eventual parse will reject it — caller layer turns
        // that into a per-site skip.
        return n.to_string();
    }
    if n.fract() == 0.0 && n.abs() < 1e16 {
        return format!("{}", n as i64);
    }
    format!("{n}")
}

fn splice(text: &str, span: Span, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..span.start]);
    out.push_str(replacement);
    out.push_str(&text[span.end..]);
    out
}
