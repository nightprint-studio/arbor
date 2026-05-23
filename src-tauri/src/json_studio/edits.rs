//! Byte-splice mutations for JSON Studio.
//!
//! Each mutation:
//!   1. Resolves `path` against the current AST.
//!   2. Computes a `(byte_range, replacement)` pair.
//!   3. Re-applies the splice to the live text buffer.
//!   4. Re-parses to validate; rollback the buffer on failure.
//!
//! This is the *position-preserving* variant of RON's "edit AST, then
//! re-serialise the whole tree" pattern: editing a leaf at line 500
//! mutates the bytes at line 500 — every other line stays untouched on
//! disk. That's the "lossless edit" descriptor flag's contract.
//!
//! `apply_*` functions return the new buffer string. They never mutate
//! the registry directly — the caller (in `mod.rs`) commits the new
//! text + re-parses + records history.

use serde_json::Value;

use crate::error::{AppError, Result};

use super::ast::{self, JsonAst, JsonObject, JsonArray, Span};

// ── Primitive serialisation ─────────────────────────────────────────────────

/// Unwrap the FE's tagged `StudioPrimitiveValue` (`{type, value}`) into a
/// raw `serde_json::Value`. Accepts both wire formats — raw scalar
/// (`true`, `42`, `"foo"`) and the tagged form
/// (`{type: "string", value: "foo"}`). Mirror of yaml_studio's helper.
fn unwrap_primitive_wire(v: &Value) -> Value {
    if let Value::Object(map) = v {
        let is_tagged = map.len() == 2
            && map.contains_key("type")
            && map.contains_key("value");
        if is_tagged {
            if let Some(inner) = map.get("value") {
                return inner.clone();
            }
        }
    }
    v.clone()
}

/// Produce the literal text for a primitive value, suitable for
/// splicing into a JSON buffer. Mirrors `studio::format::types::
/// StudioMutation::SetPrimitive { value: serde_json::Value }` and the
/// `null_handling = Native` policy from the descriptor.
pub fn serialize_primitive(value: &Value) -> Result<String> {
    let value = unwrap_primitive_wire(value);
    match &value {
        Value::String(s) => Ok(serde_json::to_string(s)
            .map_err(|e| AppError::Other(format!("serialize string: {e}")))?),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b)   => Ok(b.to_string()),
        Value::Null      => Ok("null".to_string()),
        // Containers via set_primitive are nonsensical — the FE uses
        // `ReplaceAt` with a literal JSON snippet for those.
        _ => Err(AppError::Other("SetPrimitive only accepts scalars".into())),
    }
}

// ── Indent detection ────────────────────────────────────────────────────────

/// Heuristic indent prober. Reads the contents of `container_span`
/// (an object or array body) and infers two values:
///   - `child_indent`: whitespace used at the start of a fresh line
///     **before** each child (e.g. `"  "` for a 2-space pretty doc).
///   - `is_multiline`: whether children are on their own lines.
///
/// When the container is empty or single-line, falls back to defaults
/// (2 spaces for `child_indent`, single-line layout).
struct IndentInfo {
    child_indent: String,
    container_indent: String,
    multiline: bool,
}

fn probe_indent(text: &str, container_span: Span) -> IndentInfo {
    let body = container_span.slice(text);
    // Strip the open + close bracket (first and last char). Both ASCII
    // 1-byte tokens.
    let inner = if body.len() >= 2 {
        &body[1..body.len() - 1]
    } else {
        ""
    };
    let multiline = inner.contains('\n');
    if !multiline {
        return IndentInfo {
            child_indent:     "  ".into(),
            container_indent: String::new(),
            multiline:        false,
        };
    }
    // Find the indent before the first non-whitespace char after a
    // newline.
    let child_indent = inner
        .split('\n')
        .skip(1) // skip "{" line remnant
        .find_map(|line| {
            let ws: String = line
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            (!line.trim_start().is_empty()).then_some(ws)
        })
        .unwrap_or_else(|| "  ".into());
    // The container's outer indent is the same minus one "step" — try
    // to subtract the common pretty step (the run length of the
    // leading space char in child_indent). When detection fails, the
    // closing bracket sits on its own line at column 0.
    let step_char = child_indent.chars().next().unwrap_or(' ');
    let container_indent = if child_indent.is_empty() {
        String::new()
    } else {
        // Heuristic: the container's indent is what shows up before
        // the closing brace. Look at the line containing `container_span.end`.
        let close = container_span.end.saturating_sub(1);
        let line_start = text[..close].rfind('\n').map(|p| p + 1).unwrap_or(0);
        text[line_start..close]
            .chars()
            .take_while(|c| *c == step_char || *c == ' ' || *c == '\t')
            .collect::<String>()
    };
    IndentInfo { child_indent, container_indent, multiline }
}

// ── set_primitive / replace_at ──────────────────────────────────────────────

/// Splice the literal text for `value` over the node at `path`.
pub fn set_primitive(text: &str, root: &JsonAst, path: &[String], value: &Value) -> Result<String> {
    let target = ast::resolve(root, path)
        .ok_or_else(|| AppError::Other(format!("Path not found: {}", path.join("/"))))?;
    // SetPrimitive must target a scalar — containers go through
    // `ReplaceAt` with a literal JSON snippet.
    match target {
        JsonAst::Object(_) | JsonAst::Array(_) => {
            return Err(AppError::Other(
                "SetPrimitive cannot target a container — use ReplaceAt".into(),
            ));
        }
        _ => {}
    }
    let lit = serialize_primitive(value)?;
    Ok(splice(text, target.span(), &lit))
}

/// Splice arbitrary raw text over the node at `path`. Validity of the
/// resulting buffer is the caller's concern (`mod.rs` re-parses after).
pub fn replace_at(text: &str, root: &JsonAst, path: &[String], snippet: &str) -> Result<String> {
    let target_span = if path.is_empty() {
        // Replace whole doc.
        Span { start: 0, end: text.len() }
    } else {
        ast::resolve(root, path)
            .ok_or_else(|| AppError::Other(format!("Path not found: {}", path.join("/"))))?
            .span()
    };
    Ok(splice(text, target_span, snippet))
}

// ── insert_field / insert_item / insert_map_entry ───────────────────────────

/// Append a new `"name": value` property to the object at `path`.
pub fn insert_field(
    text: &str,
    root: &JsonAst,
    path: &[String],
    name: &str,
    value_snippet: &str,
) -> Result<String> {
    let target = ast::resolve(root, path)
        .ok_or_else(|| AppError::Other(format!("Path not found: {}", path.join("/"))))?;
    let obj = match target {
        JsonAst::Object(o) => o,
        _ => return Err(AppError::Other("InsertField target is not an object".into())),
    };
    if obj.props.iter().any(|p| p.name == name) {
        return Err(AppError::Other(format!("Field `{name}` already exists")));
    }
    let key_lit = serde_json::to_string(name)
        .map_err(|e| AppError::Other(format!("serialize key: {e}")))?;
    let info = probe_indent(text, obj.span);
    let (insert_at, new_text) = build_object_insertion(text, obj, &info, &key_lit, value_snippet);
    Ok(splice(text, Span { start: insert_at, end: insert_at }, &new_text))
}

/// Append a new element to the array at `path`.
pub fn insert_item(
    text: &str,
    root: &JsonAst,
    path: &[String],
    snippet: &str,
) -> Result<String> {
    let target = ast::resolve(root, path)
        .ok_or_else(|| AppError::Other(format!("Path not found: {}", path.join("/"))))?;
    let arr = match target {
        JsonAst::Array(a) => a,
        _ => return Err(AppError::Other("InsertItem target is not an array".into())),
    };
    let info = probe_indent(text, arr.span);
    let (insert_at, new_text) = build_array_insertion(text, arr, &info, snippet);
    Ok(splice(text, Span { start: insert_at, end: insert_at }, &new_text))
}

/// Insert `(key, value)` into an object. Same shape as `InsertField`;
/// JSON has no distinct map type so the dispatch collapses here.
pub fn insert_map_entry(
    text: &str,
    root: &JsonAst,
    path: &[String],
    key_text: &str,
    val_text: &str,
) -> Result<String> {
    // `key_text` may be either a JSON-quoted string ("\"foo\"") or a
    // bare identifier; normalise to the decoded form first.
    let key = parse_key_text(key_text)?;
    insert_field(text, root, path, &key, val_text)
}

fn parse_key_text(s: &str) -> Result<String> {
    let trimmed = s.trim();
    if trimmed.starts_with('"') {
        // Parse as JSON string literal.
        let v: Value = serde_json::from_str(trimmed)
            .map_err(|e| AppError::Other(format!("Invalid quoted key `{trimmed}`: {e}")))?;
        match v {
            Value::String(s) => Ok(s),
            _ => Err(AppError::Other("Key must be a string".into())),
        }
    } else {
        Ok(trimmed.to_string())
    }
}

/// Compute (insert_offset, new_text_to_splice_in) for appending a
/// property to `obj`. Honours the existing indentation style.
fn build_object_insertion(
    _text:   &str,
    obj:     &JsonObject,
    info:    &IndentInfo,
    key_lit: &str,
    value:   &str,
) -> (usize, String) {
    let close = obj.span.end.saturating_sub(1); // position of '}'
    if obj.props.is_empty() {
        if info.multiline {
            // `{}` empty multiline (rare) — drop the new prop on its own
            // line with proper indents.
            let s = format!(
                "\n{ci}{key}: {val}\n{co}",
                ci = info.child_indent,
                co = info.container_indent,
                key = key_lit,
                val = value,
            );
            return (close, s);
        }
        return (close, format!("{key_lit}: {value}"));
    }
    // Non-empty: append after the last prop. We splice **between** the
    // last prop and the closing brace, including the leading comma.
    let last_prop_end = obj.props.last().unwrap().span.end;
    if info.multiline {
        // Insert pattern: `,\n<child_indent>"key": value`
        let s = format!(
            ",\n{ci}{key}: {val}",
            ci = info.child_indent,
            key = key_lit,
            val = value,
        );
        return (last_prop_end, s);
    }
    // Single-line: `, "key": value`
    (last_prop_end, format!(", {key_lit}: {value}"))
}

fn build_array_insertion(
    _text: &str,
    arr:   &JsonArray,
    info:  &IndentInfo,
    value: &str,
) -> (usize, String) {
    let close = arr.span.end.saturating_sub(1); // ']'
    if arr.items.is_empty() {
        if info.multiline {
            let s = format!(
                "\n{ci}{val}\n{co}",
                ci = info.child_indent,
                co = info.container_indent,
                val = value,
            );
            return (close, s);
        }
        return (close, value.to_string());
    }
    let last_item_end = arr.items.last().unwrap().span().end;
    if info.multiline {
        let s = format!(
            ",\n{ci}{val}",
            ci = info.child_indent,
            val = value,
        );
        return (last_item_end, s);
    }
    (last_item_end, format!(", {value}"))
}

// ── remove_at ───────────────────────────────────────────────────────────────

/// Delete the node at `path` from its parent.
pub fn remove_at(text: &str, root: &JsonAst, path: &[String]) -> Result<String> {
    if path.is_empty() {
        return Err(AppError::Other("Cannot remove the document root".into()));
    }
    let (parent, idx, _target) = ast::resolve_parent(root, path)
        .ok_or_else(|| AppError::Other(format!("Path not found: {}", path.join("/"))))?;
    let cut = match parent {
        JsonAst::Object(o) => object_remove_range(text, o, idx),
        JsonAst::Array(a)  => array_remove_range(text, a, idx),
        _ => return Err(AppError::Other("Parent is not a container".into())),
    };
    Ok(splice(text, cut, ""))
}

/// Compute the byte range to cut for removing `obj.props[idx]`.
/// Includes the surrounding comma + leading whitespace, so the buffer
/// stays well-formed JSON after splicing.
fn object_remove_range(_text: &str, obj: &JsonObject, idx: usize) -> Span {
    let prop = &obj.props[idx];
    // Try cutting forward (this prop + trailing `,` + whitespace) if a
    // following prop exists; otherwise cut backward (preceding `,` +
    // whitespace + this prop) if a previous prop exists; otherwise just
    // the prop's own span (only prop in the object).
    let after_target = obj.props.get(idx + 1);
    if let Some(next) = after_target {
        // Cut from prop.span.start to next.span.start to swallow the
        // trailing comma + whitespace.
        return Span { start: prop.span.start, end: next.span.start };
    }
    if idx > 0 {
        // Cut from previous prop's end to current prop's end (swallow
        // the leading comma + whitespace).
        let prev = &obj.props[idx - 1];
        return Span { start: prev.span.end, end: prop.span.end };
    }
    // Sole prop — also strip the surrounding whitespace between `{` and
    // `}` so an empty multi-line object collapses cleanly.
    let open  = obj.span.start + 1;
    let close = obj.span.end.saturating_sub(1);
    Span { start: open, end: close }
}

fn array_remove_range(_text: &str, arr: &JsonArray, idx: usize) -> Span {
    let item = &arr.items[idx];
    let after = arr.items.get(idx + 1);
    if let Some(next) = after {
        return Span { start: item.span().start, end: next.span().start };
    }
    if idx > 0 {
        let prev = &arr.items[idx - 1];
        return Span { start: prev.span().end, end: item.span().end };
    }
    let open  = arr.span.start + 1;
    let close = arr.span.end.saturating_sub(1);
    Span { start: open, end: close }
}

// ── duplicate_at ────────────────────────────────────────────────────────────

/// Duplicate the node at `path`, inserting the copy right after the
/// original. For object properties the duplicate keeps the same key —
/// JSON allows duplicate keys but most consumers warn; the FE marks
/// the new prop selectable so the user can rename immediately.
pub fn duplicate_at(text: &str, root: &JsonAst, path: &[String]) -> Result<String> {
    if path.is_empty() {
        return Err(AppError::Other("Cannot duplicate the document root".into()));
    }
    let (parent, idx, _target) = ast::resolve_parent(root, path)
        .ok_or_else(|| AppError::Other(format!("Path not found: {}", path.join("/"))))?;
    match parent {
        JsonAst::Object(o) => duplicate_object_prop(text, o, idx),
        JsonAst::Array(a)  => duplicate_array_item(text, a, idx),
        _ => Err(AppError::Other("Parent is not a container".into())),
    }
}

fn duplicate_object_prop(text: &str, obj: &JsonObject, idx: usize) -> Result<String> {
    let prop = &obj.props[idx];
    let prop_text = prop.span.slice(text).to_string();
    let info = probe_indent(text, obj.span);
    let insertion = if info.multiline {
        format!(",\n{ci}{p}", ci = info.child_indent, p = prop_text)
    } else {
        format!(", {p}", p = prop_text)
    };
    Ok(splice(
        text,
        Span { start: prop.span.end, end: prop.span.end },
        &insertion,
    ))
}

fn duplicate_array_item(text: &str, arr: &JsonArray, idx: usize) -> Result<String> {
    let item = &arr.items[idx];
    let item_text = item.span().slice(text).to_string();
    let info = probe_indent(text, arr.span);
    let insertion = if info.multiline {
        format!(",\n{ci}{v}", ci = info.child_indent, v = item_text)
    } else {
        format!(", {v}", v = item_text)
    };
    Ok(splice(
        text,
        Span { start: item.span().end, end: item.span().end },
        &insertion,
    ))
}

// ── move_item ───────────────────────────────────────────────────────────────

/// Swap the node at `path` with its sibling at `idx + delta`. Operates
/// on the **text span of the value only** — surrounding commas /
/// whitespace stay put so the diff stays minimal.
pub fn move_item(text: &str, root: &JsonAst, path: &[String], delta: i32) -> Result<String> {
    if path.is_empty() {
        return Err(AppError::Other("Cannot move the document root".into()));
    }
    if delta == 0 { return Ok(text.to_string()); }
    let (parent, idx, _target) = ast::resolve_parent(root, path)
        .ok_or_else(|| AppError::Other(format!("Path not found: {}", path.join("/"))))?;
    let new_idx = idx as i64 + delta as i64;
    if new_idx < 0 {
        return Err(AppError::Other("Cannot move before first sibling".into()));
    }
    let new_idx = new_idx as usize;
    let (a_span, b_span) = match parent {
        JsonAst::Object(o) => {
            if new_idx >= o.props.len() {
                return Err(AppError::Other("Cannot move after last sibling".into()));
            }
            (o.props[idx].span, o.props[new_idx].span)
        }
        JsonAst::Array(a) => {
            if new_idx >= a.items.len() {
                return Err(AppError::Other("Cannot move after last sibling".into()));
            }
            (a.items[idx].span(), a.items[new_idx].span())
        }
        _ => return Err(AppError::Other("Parent is not a container".into())),
    };
    Ok(swap_spans(text, a_span, b_span))
}

// ── splice helpers ──────────────────────────────────────────────────────────

fn splice(text: &str, span: Span, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..span.start]);
    out.push_str(replacement);
    out.push_str(&text[span.end..]);
    out
}

/// Swap the bytes at `a` with the bytes at `b`. Spans must not overlap.
fn swap_spans(text: &str, a: Span, b: Span) -> String {
    let (lo, hi) = if a.start <= b.start { (a, b) } else { (b, a) };
    debug_assert!(lo.end <= hi.start, "swap_spans: ranges overlap");
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..lo.start]);
    out.push_str(hi.slice(text));
    out.push_str(&text[lo.end..hi.start]);
    out.push_str(lo.slice(text));
    out.push_str(&text[hi.end..]);
    out
}
