//! Structured YAML mutations — the `yaml_edit` ops + `serde_yaml_ng`
//! round-trip fallback, lifted verbatim from the launcher's
//! `yaml_studio/mod.rs` (only the error type changed: `AppError::Other(..)`
//! → `StudioError::App(..)`).
//!
//! The public entry point is [`mutate`]: it lowers one [`SimpleMutation`]
//! against the parsed `Vec<Document>` (multi-doc aware), re-emits, and
//! re-parse-validates (rejecting a mutation that produced invalid YAML —
//! exactly the pre-extraction `mutate_with` contract).
//!
//! ## Lossless-ness (FROZEN F9)
//!
//! `SetPrimitive` on a scalar leaf routes through
//! `yaml_edit::path::YamlPath::set_path`, which mutates the rowan tree in
//! place — comments, anchors, quote style and surrounding whitespace
//! survive. Structural ops (replace / insert / duplicate / move, and a
//! `null` set) round-trip the affected sub-tree through `serde_yaml_ng`,
//! which DOES drop comments around the splice site — documented trade-off.

use arbor_studio_core::prelude::{SimpleMutation, StudioError, StudioResult};
use serde_json::Value;
use yaml_edit::path::YamlPath;
use yaml_edit::Document;

use crate::project;

fn err(message: impl Into<String>) -> StudioError {
    StudioError::App(message.into())
}

/// Apply one structured mutation to `text` and return the new text.
pub fn mutate(text: &str, mutation: SimpleMutation) -> StudioResult<String> {
    match mutation {
        SimpleMutation::SetPrimitive { path, value } => {
            mutate_with(text, |docs, multi| {
                let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
                let target = docs
                    .get_mut(doc_idx)
                    .ok_or_else(|| err(format!("Doc index out of range: {doc_idx}")))?;
                set_primitive_in_doc(target, sub_path, &value)
            })
        }
        SimpleMutation::ReplaceAt { path, text: snippet } => {
            mutate_with(text, |docs, multi| {
                let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
                let target = docs
                    .get_mut(doc_idx)
                    .ok_or_else(|| err(format!("Doc index out of range: {doc_idx}")))?;
                replace_in_doc(target, sub_path, &snippet)
            })
        }
        SimpleMutation::RemoveAt { path } => {
            if path.is_empty() {
                return Err(err("Cannot remove document root"));
            }
            mutate_with(text, |docs, multi| {
                let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
                if sub_path.is_empty() {
                    // Removing a whole doc in a multi-doc stream.
                    if !multi || docs.len() <= 1 {
                        return Err(err("Cannot remove the only document"));
                    }
                    docs.remove(doc_idx);
                    return Ok(());
                }
                let target = docs
                    .get_mut(doc_idx)
                    .ok_or_else(|| err(format!("Doc index out of range: {doc_idx}")))?;
                remove_in_doc(target, sub_path)
            })
        }
        SimpleMutation::InsertField { path, name, text: snippet } => {
            mutate_with(text, |docs, multi| {
                let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
                let target = docs
                    .get_mut(doc_idx)
                    .ok_or_else(|| err(format!("Doc index out of range: {doc_idx}")))?;
                insert_field_in_doc(target, sub_path, &name, &snippet)
            })
        }
        SimpleMutation::InsertItem { path, text: snippet } => {
            mutate_with(text, |docs, multi| {
                let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
                let target = docs
                    .get_mut(doc_idx)
                    .ok_or_else(|| err(format!("Doc index out of range: {doc_idx}")))?;
                insert_item_in_doc(target, sub_path, &snippet)
            })
        }
        // YAML treats mappings and "maps" interchangeably — delegate to
        // insert_field semantics.
        SimpleMutation::InsertMapEntry { path, key_text, val_text } => {
            mutate_with(text, |docs, multi| {
                let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
                let target = docs
                    .get_mut(doc_idx)
                    .ok_or_else(|| err(format!("Doc index out of range: {doc_idx}")))?;
                insert_field_in_doc(target, sub_path, &key_text, &val_text)
            })
        }
        SimpleMutation::DuplicateAt { path } => {
            if path.is_empty() {
                return Err(err("Cannot duplicate document root"));
            }
            mutate_with(text, |docs, multi| {
                let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
                let target = docs
                    .get_mut(doc_idx)
                    .ok_or_else(|| err(format!("Doc index out of range: {doc_idx}")))?;
                duplicate_in_doc(target, sub_path)
            })
        }
        SimpleMutation::MoveItem { path, delta } => {
            if path.is_empty() {
                return Err(err("Cannot move document root"));
            }
            mutate_with(text, |docs, multi| {
                let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
                let target = docs
                    .get_mut(doc_idx)
                    .ok_or_else(|| err(format!("Doc index out of range: {doc_idx}")))?;
                move_in_doc(target, sub_path, delta)
            })
        }
    }
}

/// Parse + re-emit harness: parse `text` into a `Vec<Document>`, run `op`,
/// join, then re-parse the result to reject a mutation that produced
/// invalid YAML. The post-op length decides single- vs multi-doc emission
/// (a whole-doc remove can collapse a stream back to single-doc).
fn mutate_with<F>(text: &str, op: F) -> StudioResult<String>
where
    F: FnOnce(&mut Vec<Document>, bool) -> StudioResult<()>,
{
    let (parsed, _value, parse_error, _doc_count, multi_doc) = project::parse_text(text);
    if let Some(e) = parse_error {
        return Err(err(format!("Document has parse errors — cannot edit tree: {e}")));
    }
    let mut working = parsed
        .ok_or_else(|| err("Document has parse errors — cannot edit tree"))?;
    op(&mut working, multi_doc)?;
    let new_multi = working.len() > 1;
    let new_text = project::join_documents(&working, new_multi);
    drop(working);
    // Re-parse so a mutation that produced invalid YAML never escapes.
    let (_re_docs, _re_value, re_error, _c, _m) = project::parse_text(&new_text);
    if let Some(e) = re_error {
        return Err(err(format!("Mutation produced invalid YAML: {e}")));
    }
    Ok(new_text)
}

// ── Path splitting (multi-doc aware) ────────────────────────────────────

/// Split a `Vec<String>` path into `(doc_idx, sub_path)` based on the
/// multi-doc flag. For single-doc files the whole path is the sub-path and
/// `doc_idx = 0`.
fn split_doc_path(path: &[String], multi_doc: bool) -> StudioResult<(usize, &[String])> {
    if !multi_doc {
        return Ok((0, path));
    }
    let first = path
        .first()
        .ok_or_else(|| err("Multi-document path needs at least one segment"))?;
    let idx: usize = first
        .parse()
        .map_err(|_| err(format!("Invalid document index segment: {first}")))?;
    Ok((idx, &path[1..]))
}

// ── yaml_edit mutation helpers (lifted from yaml_studio/mod.rs) ──────────

/// Unwrap the FE's tagged `{type, value}` form into a raw scalar.
fn unwrap_primitive_wire(v: &Value) -> Value {
    if let Value::Object(map) = v {
        let is_tagged = map.len() == 2 && map.contains_key("type") && map.contains_key("value");
        if is_tagged {
            if let Some(inner) = map.get("value") {
                return inner.clone();
            }
        }
    }
    v.clone()
}

/// SetPrimitive — lossless via `yaml_edit`'s `YamlPath::set_path` for the
/// scalar leaves it knows how to format (str/i64/f64/bool). `null` and
/// containers route through the round-trip writer.
fn set_primitive_in_doc(doc: &mut Document, path: &[String], value: &Value) -> StudioResult<()> {
    let raw = unwrap_primitive_wire(value);
    let yaml_path = path_to_yaml_edit_path(path);
    match &raw {
        Value::String(s) => {
            doc.set_path(&yaml_path, s.as_str());
        }
        Value::Bool(b) => {
            doc.set_path(&yaml_path, *b);
        }
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                doc.set_path(&yaml_path, i);
            } else if let Some(f) = n.as_f64() {
                doc.set_path(&yaml_path, f);
            } else {
                return Err(err("Unsupported number form"));
            }
        }
        // YAML `null` — `AsYaml` doesn't accept `()`, so route through the
        // serde_yaml_ng round-trip writer. Drops comments around the
        // splice site (documented trade-off).
        Value::Null => apply_value_replacement(doc, path, serde_yaml_ng::Value::Null)?,
        _ => {
            return Err(err(
                "Cannot set a primitive — value is a container; use replace_at",
            ))
        }
    }
    Ok(())
}

/// Replace the AST node at `path` with the YAML parsed from `snippet`.
fn replace_in_doc(doc: &mut Document, path: &[String], snippet: &str) -> StudioResult<()> {
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(snippet)
        .map_err(|e| err(format!("Invalid YAML snippet: {e}")))?;
    apply_value_replacement(doc, path, parsed)
}

fn remove_in_doc(doc: &mut Document, path: &[String]) -> StudioResult<()> {
    if path.is_empty() {
        return Err(err("Cannot remove document root"));
    }
    let yaml_path = path_to_yaml_edit_path(path);
    if !doc.remove_path(&yaml_path) {
        return Err(err(format!("remove_path: path not found at {path:?}")));
    }
    Ok(())
}

fn insert_field_in_doc(
    doc: &mut Document,
    path: &[String],
    name: &str,
    snippet: &str,
) -> StudioResult<()> {
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(snippet)
        .map_err(|e| err(format!("Invalid YAML snippet: {e}")))?;
    let mut child_path = path.to_vec();
    child_path.push(name.to_string());
    apply_value_replacement(doc, &child_path, parsed)
}

fn insert_item_in_doc(doc: &mut Document, path: &[String], snippet: &str) -> StudioResult<()> {
    let parent_value = read_subtree(doc, path)?;
    let mut seq = match parent_value {
        serde_yaml_ng::Value::Sequence(s) => s,
        _ => return Err(err("Cannot append item — parent is not a sequence")),
    };
    let new_item: serde_yaml_ng::Value = serde_yaml_ng::from_str(snippet)
        .map_err(|e| err(format!("Invalid YAML snippet: {e}")))?;
    seq.push(new_item);
    apply_value_replacement(doc, path, serde_yaml_ng::Value::Sequence(seq))
}

fn duplicate_in_doc(doc: &mut Document, path: &[String]) -> StudioResult<()> {
    if path.is_empty() {
        return Err(err("Cannot duplicate document root"));
    }
    let (parent_path, last) = path.split_at(path.len() - 1);
    let last_seg = &last[0];
    let parent_val = read_subtree(doc, parent_path)?;
    match parent_val {
        serde_yaml_ng::Value::Mapping(mut m) => {
            let src = m
                .get(last_seg.as_str())
                .ok_or_else(|| err(format!("Key not found: {last_seg}")))?
                .clone();
            let mut next_key = format!("{last_seg}_copy");
            let mut n = 2;
            while m.contains_key(next_key.as_str()) {
                next_key = format!("{last_seg}_copy{n}");
                n += 1;
            }
            m.insert(serde_yaml_ng::Value::String(next_key), src);
            apply_value_replacement(doc, parent_path, serde_yaml_ng::Value::Mapping(m))
        }
        serde_yaml_ng::Value::Sequence(mut seq) => {
            let i: usize = last_seg
                .parse()
                .map_err(|_| err(format!("Invalid array index: {last_seg}")))?;
            if i >= seq.len() {
                return Err(err(format!("Array index out of bounds: {i}")));
            }
            let copy = seq[i].clone();
            seq.insert(i + 1, copy);
            apply_value_replacement(doc, parent_path, serde_yaml_ng::Value::Sequence(seq))
        }
        _ => Err(err("Parent is not a container")),
    }
}

fn move_in_doc(doc: &mut Document, path: &[String], delta: i32) -> StudioResult<()> {
    if path.is_empty() {
        return Err(err("Cannot move document root"));
    }
    let (parent_path, last) = path.split_at(path.len() - 1);
    let last_seg = &last[0];
    let parent_val = read_subtree(doc, parent_path)?;
    let serde_yaml_ng::Value::Sequence(mut seq) = parent_val else {
        return Err(err("Cannot move — parent is not an ordered sequence"));
    };
    let i: usize = last_seg
        .parse()
        .map_err(|_| err(format!("Invalid array index: {last_seg}")))?;
    if i >= seq.len() {
        return Err(err(format!("Array index out of bounds: {i}")));
    }
    let new_i = (i as i32 + delta).max(0) as usize;
    let new_i = new_i.min(seq.len() - 1);
    if new_i == i {
        return Ok(());
    }
    let item = seq.remove(i);
    seq.insert(new_i, item);
    apply_value_replacement(doc, parent_path, serde_yaml_ng::Value::Sequence(seq))
}

/// Read the subtree at `path` as a `serde_yaml_ng::Value`.
fn read_subtree(doc: &Document, path: &[String]) -> StudioResult<serde_yaml_ng::Value> {
    let full = doc.to_string();
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&full)
        .map_err(|e| err(format!("Re-parse for subtree: {e}")))?;
    walk_serde_yaml_ng_path(&parsed, path)
        .cloned()
        .ok_or_else(|| err(format!("Path not found: {path:?}")))
}

fn walk_serde_yaml_ng_path<'a>(
    root: &'a serde_yaml_ng::Value,
    path: &[String],
) -> Option<&'a serde_yaml_ng::Value> {
    let mut cur = root;
    for seg in path {
        cur = match cur {
            serde_yaml_ng::Value::Mapping(m) => m.get(seg.as_str())?,
            serde_yaml_ng::Value::Sequence(s) => {
                let i: usize = seg.parse().ok()?;
                s.get(i)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

/// Replace the value at `path` via the round-trip rewriter: parse the full
/// doc as serde_yaml_ng, splice, re-emit, re-parse as a yaml_edit Document.
/// Structural-edit fallback — drops comments from the surrounding doc.
fn apply_value_replacement(
    doc: &mut Document,
    path: &[String],
    new_value: serde_yaml_ng::Value,
) -> StudioResult<()> {
    use std::str::FromStr;
    let full = doc.to_string();
    let mut root: serde_yaml_ng::Value = serde_yaml_ng::from_str(&full)
        .map_err(|e| err(format!("Re-parse for replace: {e}")))?;
    if !splice_serde_yaml_ng(&mut root, path, new_value) {
        return Err(err(format!("Path not found: {path:?}")));
    }
    let serialised = serde_yaml_ng::to_string(&root)
        .map_err(|e| err(format!("Re-serialise YAML: {e}")))?;
    let new_doc =
        Document::from_str(&serialised).map_err(|e| err(format!("Re-parse mutated YAML: {e}")))?;
    *doc = new_doc;
    Ok(())
}

/// Splice `new_value` into `root` at `path`. Returns `false` when the path
/// doesn't resolve.
fn splice_serde_yaml_ng(
    root: &mut serde_yaml_ng::Value,
    path: &[String],
    new_value: serde_yaml_ng::Value,
) -> bool {
    if path.is_empty() {
        *root = new_value;
        return true;
    }
    let (head, rest) = path.split_first().unwrap();
    match root {
        serde_yaml_ng::Value::Mapping(m) => {
            let key = serde_yaml_ng::Value::String(head.clone());
            if rest.is_empty() {
                m.insert(key, new_value);
                return true;
            }
            if let Some(child) = m.get_mut(&key) {
                return splice_serde_yaml_ng(child, rest, new_value);
            }
            false
        }
        serde_yaml_ng::Value::Sequence(s) => {
            let i: usize = match head.parse() {
                Ok(v) => v,
                Err(_) => return false,
            };
            if i >= s.len() {
                return false;
            }
            if rest.is_empty() {
                s[i] = new_value;
                return true;
            }
            splice_serde_yaml_ng(&mut s[i], rest, new_value)
        }
        _ => false,
    }
}

/// Convert a `Vec<String>` path to the dotted-with-bracket path format
/// `yaml_edit::path::YamlPath` consumes. Numeric segments become `[N]`,
/// string segments are dot-prefixed.
///
/// Caveat: keys containing `.`/`[`/`]` literally collide with the path
/// syntax; the structural fallback (`apply_value_replacement`) often saves
/// such cases anyway.
fn path_to_yaml_edit_path(segments: &[String]) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        if let Ok(idx) = seg.parse::<usize>() {
            out.push('[');
            out.push_str(&idx.to_string());
            out.push(']');
            continue;
        }
        if i > 0 {
            out.push('.');
        }
        out.push_str(seg);
    }
    out
}
