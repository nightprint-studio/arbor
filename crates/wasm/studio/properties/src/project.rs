//! `.properties` → `serde_json::Value` projection (dotted keys → nested
//! Objects/Arrays, `$value` prefix-collision sentinel), plus node
//! kind/preview helpers.
//!
//! Lifted verbatim from the launcher's `properties_studio/mod.rs`.

use serde_json::Value;

use crate::line_model::{parse_lines, RawLine};

const PREVIEW_MAX_CHARS: usize = 64;

/// Reserved key used inside the projected JSON to carry the leaf value
/// of a key that is ALSO a prefix for sub-keys. Never written to the
/// `.properties` source — `path_to_flat_key` strips it.
pub const VALUE_SENTINEL: &str = "$value";

// ── Projection (dotted keys → nested JSON) ──────────────────────────────

#[derive(Debug)]
enum TreeNode {
    Leaf(String),
    Mapping(std::collections::BTreeMap<String, TreeNode>),
    Sequence(Vec<Option<TreeNode>>),
}

#[derive(Debug)]
enum KeySegment {
    Field(String),
    Index(usize),
}

fn parse_key_segments(key: &str) -> Vec<KeySegment> {
    let mut out: Vec<KeySegment> = Vec::new();
    let mut cur = String::new();
    let mut chars = key.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '.' {
            if !cur.is_empty() {
                push_segment(&mut out, std::mem::take(&mut cur));
            }
            continue;
        }
        if c == '[' {
            if !cur.is_empty() {
                push_segment(&mut out, std::mem::take(&mut cur));
            }
            let mut idx_buf = String::new();
            for ic in chars.by_ref() {
                if ic == ']' { break; }
                idx_buf.push(ic);
            }
            if let Ok(n) = idx_buf.parse::<usize>() {
                out.push(KeySegment::Index(n));
            } else {
                cur.push('[');
                cur.push_str(&idx_buf);
                cur.push(']');
            }
            continue;
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        push_segment(&mut out, cur);
    }
    out
}

fn push_segment(out: &mut Vec<KeySegment>, seg: String) {
    out.push(KeySegment::Field(seg));
}

/// Build the JSON projection from the line view.
///
/// `.properties` is fundamentally `Map<String, String>` — Java `Properties`
/// has no nested namespaces. The dotted-key tree is a UI convenience.
/// **Prefix collisions are legal and common** (Spring / Struts routinely
/// use `service.url=...` alongside `service.url.timeout=...`), so we
/// resolve them via a `$value` sentinel child: when a key X exists as
/// both a leaf and a prefix for sub-keys, the tree shows X as a
/// container with one child named `$value` holding the leaf and the
/// regular children holding the sub-tree. The FE renders `$value` rows
/// with a "self" label and mutations skip the `$value` segment when
/// translating back to the flat dotted key.
///
/// Duplicate flat keys (same exact string twice) follow the .properties
/// spec: last wins.
fn build_projection(lines: &[RawLine]) -> (Value, Option<String>) {
    let mut root = TreeNode::Mapping(std::collections::BTreeMap::new());
    for line in lines {
        if let RawLine::Logical { key, value, .. } = line {
            let segments = parse_key_segments(key);
            if segments.is_empty() { continue; }
            insert_into_tree(&mut root, &segments, value);
        }
    }
    (tree_to_value(&root), None)
}

fn insert_into_tree(
    root: &mut TreeNode,
    segments: &[KeySegment],
    value: &str,
) {
    if segments.is_empty() {
        // Leaf write at the current position. A *non-empty* container here is a
        // real prefix collision (`foo.sub=…` already created children): stash the
        // leaf as `$value` so both the prefix's own value AND its sub-keys stay
        // visible. An *empty* mapping is just the placeholder the parent created
        // for this exact key — it's a plain leaf, not a collision.
        match root {
            TreeNode::Mapping(m) if m.is_empty() => {
                *root = TreeNode::Leaf(value.to_string());
            }
            TreeNode::Mapping(m) => {
                m.insert(VALUE_SENTINEL.to_string(), TreeNode::Leaf(value.to_string()));
            }
            TreeNode::Sequence(_) => {
                // Sequence-at-prefix collision is much rarer; we keep
                // the sequence and silently drop the leaf write. The
                // line still exists in the source (lossless).
            }
            TreeNode::Leaf(_) => {
                // Duplicate exact key — last wins.
                *root = TreeNode::Leaf(value.to_string());
            }
        }
        return;
    }
    let (head, rest) = segments.split_first().unwrap();
    match head {
        KeySegment::Field(name) => {
            // Upgrade Leaf → Mapping preserving the existing leaf as
            // `$value` (this is the common collision case: `foo=v` then
            // `foo.bar=w`).
            if let TreeNode::Leaf(existing) = root {
                let leaf_val = std::mem::take(existing);
                let mut map = std::collections::BTreeMap::new();
                map.insert(VALUE_SENTINEL.to_string(), TreeNode::Leaf(leaf_val));
                *root = TreeNode::Mapping(map);
            }
            // Field-into-Sequence is structurally weird (`foo[0]=v`
            // then `foo.bar=w`); we keep the sequence and skip the
            // field — the source still has both lines.
            if matches!(root, TreeNode::Sequence(_)) {
                return;
            }
            let map = match root {
                TreeNode::Mapping(m) => m,
                _ => unreachable!(),
            };
            let entry = map
                .entry(name.clone())
                .or_insert_with(|| TreeNode::Mapping(std::collections::BTreeMap::new()));
            if matches!(rest.first(), Some(KeySegment::Index(_)))
                && matches!(entry, TreeNode::Mapping(m) if m.is_empty())
            {
                *entry = TreeNode::Sequence(Vec::new());
            }
            insert_into_tree(entry, rest, value);
        }
        KeySegment::Index(i) => {
            // Upgrade Leaf → Sequence with the existing leaf as element 0
            // and the new entry written at the requested index. Same
            // spirit as the Field case but for `foo=v` + `foo[1]=w`.
            if let TreeNode::Leaf(existing) = root {
                let leaf_val = std::mem::take(existing);
                let seq: Vec<Option<TreeNode>> = vec![Some(TreeNode::Leaf(leaf_val))];
                *root = TreeNode::Sequence(seq);
            }
            // Index-into-Mapping (`foo.bar=v` then `foo[0]=w`) keeps
            // the mapping and skips the index — the source still has
            // both lines.
            if matches!(root, TreeNode::Mapping(m) if !m.is_empty()) {
                return;
            }
            if matches!(root, TreeNode::Mapping(_)) {
                *root = TreeNode::Sequence(Vec::new());
            }
            let seq = match root {
                TreeNode::Sequence(s) => s,
                _ => unreachable!(),
            };
            while seq.len() <= *i { seq.push(None); }
            if rest.is_empty() {
                seq[*i] = Some(TreeNode::Leaf(value.to_string()));
            } else {
                let slot = seq[*i].get_or_insert(TreeNode::Mapping(std::collections::BTreeMap::new()));
                if matches!(rest.first(), Some(KeySegment::Index(_)))
                    && matches!(slot, TreeNode::Mapping(m) if m.is_empty())
                {
                    *slot = TreeNode::Sequence(Vec::new());
                }
                insert_into_tree(slot, rest, value);
            }
        }
    }
}

fn tree_to_value(t: &TreeNode) -> Value {
    match t {
        TreeNode::Leaf(s) => Value::String(s.clone()),
        TreeNode::Mapping(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map.iter() {
                out.insert(k.clone(), tree_to_value(v));
            }
            Value::Object(out)
        }
        TreeNode::Sequence(seq) => {
            let mut out = Vec::with_capacity(seq.len());
            for slot in seq.iter() {
                match slot {
                    Some(v) => out.push(tree_to_value(v)),
                    None    => out.push(Value::Null),
                }
            }
            Value::Array(out)
        }
    }
}

/// Parse a `.properties` text into `(value, error)`. `.properties` has
/// no truly-failing byte stream, so `error` is `None` in practice.
pub fn parse_outcome(text: &str) -> (Option<Value>, Option<String>) {
    let lines = parse_lines(text);
    let (value, parse_err) = build_projection(&lines);
    (Some(value), parse_err)
}

/// Parse a `.properties` text to the projected JSON value (no doc
/// state). Mirrors `yaml_studio::parse_to_value` / `toml_studio::parse_to_value`.
/// Used by the cross-ref scanner.
pub fn parse_to_value(text: &str) -> Option<Value> {
    parse_outcome(text).0
}

/// Indent string — `.properties` has no nested indent, but the FE still
/// asks for `get_indent` to seed editor preferences. We surface "  "
/// unconditionally for parity with the other backends.
pub fn detect_indent(_text: &str) -> String {
    "  ".to_string()
}

// ── Node kind / preview ─────────────────────────────────────────────────

/// Kind string for a value node. `.properties` is structurally a flat
/// `string` → `string` map; the JSON projection promotes dotted keys to
/// nested `object`/`array` containers. Inner leaves are always rendered
/// as `string` since `.properties` has no typing. We keep the `null`
/// variant so the bulk-edit modal's null-policy display works uniformly.
pub fn node_kind(v: &Value) -> String {
    match v {
        Value::Null      => "null",
        Value::Object(_) => "object",
        Value::Array(_)  => "array",
        Value::String(_) | Value::Bool(_) | Value::Number(_) => "string",
    }
    .to_string()
}

pub fn preview_for(v: &Value) -> String {
    match v {
        Value::Null      => String::new(),
        Value::Object(_) => String::new(),
        Value::Array(_)  => String::new(),
        Value::String(s) => clip_preview(s),
        Value::Bool(b)   => b.to_string(),
        Value::Number(n) => n.to_string(),
    }
}

fn clip_preview(s: &str) -> String {
    if s.chars().count() <= PREVIEW_MAX_CHARS {
        s.to_string()
    } else {
        let head: String = s.chars().take(PREVIEW_MAX_CHARS).collect();
        format!("{head}…")
    }
}
