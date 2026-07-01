//! `core::diff` — the format-agnostic diff engines lifted from the 5
//! per-format backends (RON/JSON are the canonical references; the
//! TOML/YAML/.properties twins carried incidental drift that this
//! extraction collapses — see the blueprint §2.2).
//!
//! Two engines:
//!
//! * [`unified`] — line-level unified diff (similar crate, 3-line
//!   context, grouped hunks). Identical across every format.
//! * [`tree`] — recursive structural diff over `serde_json::Value`.
//!   Walks two Values producing
//!   `Added/Removed/Modified/Partial/Unchanged` nodes with child-change
//!   rollup; `Unchanged` subtrees are pruned (only `Partial` containers
//!   keep children). `change_count` is the leaf-change tally.
//!
//! Format-specific shape-matching (RON's struct/tuple name match +
//! synthetic `Some` for `Option`) lives in the format crate: it projects
//! its AST to a `$type`/`$tag`/`Some`-wrapped `Value` *before* calling
//! [`tree`] (the projection already exists as `project_for_query`). The
//! generic engine only ever sees `serde_json::Value`.

use arbor_studio_types::prelude::{
    DiffHunk, DiffLine, DiffLineKind, DiffStatus, DiffTreeNode,
};
use serde_json::Value;
use similar::{ChangeTag, TextDiff};

/// Max preview length, in chars, for a string leaf (mirrors the
/// per-format `PREVIEW_MAX_CHARS`).
const PREVIEW_MAX_CHARS: usize = 64;

// ── Unified (line-level) diff ────────────────────────────────────────

/// Line-level unified diff: 3-line context, grouped hunks. Returns an
/// empty vec when the two inputs are byte-identical.
pub fn unified(original: &str, current: &str) -> Vec<DiffHunk> {
    if original == current {
        return Vec::new();
    }
    let diff = TextDiff::from_lines(original, current);
    let mut hunks = Vec::new();
    for group in diff.grouped_ops(3) {
        if group.is_empty() {
            continue;
        }
        let mut lines = Vec::new();
        let first = group.first().unwrap();
        let last = group.last().unwrap();
        let old_start = first.old_range().start as u32 + 1;
        let new_start = first.new_range().start as u32 + 1;
        let old_count = (last.old_range().end - first.old_range().start) as u32;
        let new_count = (last.new_range().end - first.new_range().start) as u32;
        for op in group {
            for change in diff.iter_inline_changes(&op) {
                let (kind, old_line, new_line) = match change.tag() {
                    ChangeTag::Equal => (
                        DiffLineKind::Context,
                        change.old_index().map(|i| (i + 1) as u32),
                        change.new_index().map(|i| (i + 1) as u32),
                    ),
                    ChangeTag::Delete => (
                        DiffLineKind::Del,
                        change.old_index().map(|i| (i + 1) as u32),
                        None,
                    ),
                    ChangeTag::Insert => (
                        DiffLineKind::Add,
                        None,
                        change.new_index().map(|i| (i + 1) as u32),
                    ),
                };
                let mut text = String::new();
                for (_, slice) in change.iter_strings_lossy() {
                    text.push_str(&slice);
                }
                // Strip trailing newline — the DiffLine renderer adds
                // line breaks at row boundaries itself.
                while text.ends_with('\n') || text.ends_with('\r') {
                    text.pop();
                }
                lines.push(DiffLine { kind, old_line, new_line, text });
            }
        }
        hunks.push(DiffHunk { old_start, old_count, new_start, new_count, lines });
    }
    hunks
}

// ── Tree (structural) diff ───────────────────────────────────────────

/// Recursive structural diff over two `serde_json::Value`s. The root
/// node is keyed `"$"` with an empty path; descendants carry their full
/// path segment chain.
pub fn tree(before: &Value, after: &Value) -> DiffTreeNode {
    walk("$".into(), Vec::new(), Some(before), Some(after))
}

/// `Option`-edge convenience over [`tree`]. Backends project both sides
/// to `Value` but one side may be absent when the original/current
/// failed to parse — then the root reads as a single Added / Removed /
/// Unchanged node. The common Some/Some case delegates to [`tree`].
pub fn tree_opt(before: Option<&Value>, after: Option<&Value>) -> DiffTreeNode {
    walk("$".into(), Vec::new(), before, after)
}

fn walk(key: String, path: Vec<String>, a: Option<&Value>, b: Option<&Value>) -> DiffTreeNode {
    match (a, b) {
        (Some(a), Some(b)) => {
            if a == b {
                return unchanged(key, path);
            }
            match (a, b) {
                (Value::Object(am), Value::Object(bm)) => {
                    let mut children = Vec::new();
                    // Union the key sets, preserving the order of `b`
                    // (current) — that's what the user sees.
                    let mut seen = std::collections::HashSet::<String>::new();
                    for (k, bv) in bm.iter() {
                        let av = am.get(k);
                        let mut p = path.clone();
                        p.push(k.clone());
                        let node = walk(k.clone(), p, av, Some(bv));
                        if node.status != DiffStatus::Unchanged {
                            children.push(node);
                        }
                        seen.insert(k.clone());
                    }
                    // Removed keys (present in a, missing in b).
                    for (k, av) in am.iter() {
                        if seen.contains(k) {
                            continue;
                        }
                        let mut p = path.clone();
                        p.push(k.clone());
                        children.push(walk(k.clone(), p, Some(av), None));
                    }
                    let change_count: u32 = children.iter().map(|c| c.change_count).sum();
                    let status = if change_count == 0 {
                        DiffStatus::Unchanged
                    } else {
                        DiffStatus::Partial
                    };
                    DiffTreeNode {
                        key,
                        path,
                        status,
                        kind_before: Some("object".into()),
                        kind_after: Some("object".into()),
                        preview_before: Some(format!("{{{} keys}}", am.len())),
                        preview_after: Some(format!("{{{} keys}}", bm.len())),
                        tag_before: None,
                        tag_after: None,
                        children,
                        change_count,
                    }
                }
                (Value::Array(aa), Value::Array(ba)) => {
                    let mut children = Vec::new();
                    let max = aa.len().max(ba.len());
                    for i in 0..max {
                        let mut p = path.clone();
                        p.push(i.to_string());
                        let node = walk(i.to_string(), p, aa.get(i), ba.get(i));
                        if node.status != DiffStatus::Unchanged {
                            children.push(node);
                        }
                    }
                    let change_count: u32 = children.iter().map(|c| c.change_count).sum();
                    let status = if change_count == 0 {
                        DiffStatus::Unchanged
                    } else {
                        DiffStatus::Partial
                    };
                    DiffTreeNode {
                        key,
                        path,
                        status,
                        kind_before: Some("array".into()),
                        kind_after: Some("array".into()),
                        preview_before: Some(format!("[{} items]", aa.len())),
                        preview_after: Some(format!("[{} items]", ba.len())),
                        tag_before: None,
                        tag_after: None,
                        children,
                        change_count,
                    }
                }
                // Leaf change or shape mismatch (object↔array, leaf↔container, …).
                _ => DiffTreeNode {
                    key,
                    path,
                    status: DiffStatus::Modified,
                    kind_before: Some(kind_str(a).into()),
                    kind_after: Some(kind_str(b).into()),
                    preview_before: Some(preview_for(a)),
                    preview_after: Some(preview_for(b)),
                    tag_before: None,
                    tag_after: None,
                    children: Vec::new(),
                    change_count: 1,
                },
            }
        }
        (Some(a), None) => DiffTreeNode {
            key,
            path,
            status: DiffStatus::Removed,
            kind_before: Some(kind_str(a).into()),
            kind_after: None,
            preview_before: Some(preview_for(a)),
            preview_after: None,
            tag_before: None,
            tag_after: None,
            children: Vec::new(),
            change_count: 1,
        },
        (None, Some(b)) => DiffTreeNode {
            key,
            path,
            status: DiffStatus::Added,
            kind_before: None,
            kind_after: Some(kind_str(b).into()),
            preview_before: None,
            preview_after: Some(preview_for(b)),
            tag_before: None,
            tag_after: None,
            children: Vec::new(),
            change_count: 1,
        },
        (None, None) => unchanged(key, path),
    }
}

fn unchanged(key: String, path: Vec<String>) -> DiffTreeNode {
    DiffTreeNode {
        key,
        path,
        status: DiffStatus::Unchanged,
        kind_before: None,
        kind_after: None,
        preview_before: None,
        preview_after: None,
        tag_before: None,
        tag_after: None,
        children: Vec::new(),
        change_count: 0,
    }
}

/// JSON-shaped kind label for a `Value`. The FE doesn't render container
/// kinds in the diff pane; leaf kinds are advisory. Format crates that
/// need their own kind taxonomy keep it on the query/NodeView path.
fn kind_str(v: &Value) -> &'static str {
    match v {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "bool",
        Value::Null => "null",
    }
}

/// One-line preview of a leaf/container value (mirrors the per-format
/// `preview_for_value`). Strings are quoted and truncated; containers
/// summarise their length.
fn preview_for(v: &Value) -> String {
    match v {
        Value::Object(m) => format!("{{{} keys}}", m.len()),
        Value::Array(a) => format!("[{} items]", a.len()),
        Value::String(s) => {
            let mut out = String::with_capacity(s.len().min(PREVIEW_MAX_CHARS) + 2);
            out.push('"');
            for (i, ch) in s.chars().enumerate() {
                if i >= PREVIEW_MAX_CHARS {
                    out.push('…');
                    break;
                }
                out.push(ch);
            }
            out.push('"');
            out
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
    }
}

// ── Tests (blueprint §6 diff) ────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unified_identical_no_hunks() {
        assert!(unified("a\nb\nc\n", "a\nb\nc\n").is_empty());
    }

    #[test]
    fn unified_single_line_change() {
        let hunks = unified("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(hunks.len(), 1);
        let h = &hunks[0];
        let del = h.lines.iter().find(|l| matches!(l.kind, DiffLineKind::Del)).unwrap();
        let add = h.lines.iter().find(|l| matches!(l.kind, DiffLineKind::Add)).unwrap();
        assert_eq!(del.old_line, Some(2));
        assert_eq!(add.new_line, Some(2));
        assert!(del.text.contains('b'));
        assert!(add.text.contains('B'));
    }

    #[test]
    fn unified_multi_hunk_grouping() {
        // Two distant single-line changes (>6 lines apart) → two hunks.
        let orig = (1..=20).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        let mut lines: Vec<String> = orig.split('\n').map(|s| s.to_string()).collect();
        lines[1] = "CHANGED2".into();
        lines[17] = "CHANGED18".into();
        let curr = lines.join("\n");
        let hunks = unified(&orig, &curr);
        assert_eq!(hunks.len(), 2, "distant changes group into separate hunks");
    }

    #[test]
    fn tree_equal_is_unchanged_pruned() {
        let v = json!({ "a": 1, "b": [1, 2] });
        let node = tree(&v, &v);
        assert_eq!(node.status, DiffStatus::Unchanged);
        assert_eq!(node.change_count, 0);
        assert!(node.children.is_empty());
    }

    #[test]
    fn tree_added_key() {
        let before = json!({ "a": 1 });
        let after = json!({ "a": 1, "b": 2 });
        let node = tree(&before, &after);
        assert_eq!(node.status, DiffStatus::Partial);
        assert_eq!(node.change_count, 1);
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].key, "b");
        assert_eq!(node.children[0].status, DiffStatus::Added);
    }

    #[test]
    fn tree_removed_key() {
        let before = json!({ "a": 1, "b": 2 });
        let after = json!({ "a": 1 });
        let node = tree(&before, &after);
        assert_eq!(node.status, DiffStatus::Partial);
        assert_eq!(node.change_count, 1);
        let removed = node.children.iter().find(|c| c.key == "b").unwrap();
        assert_eq!(removed.status, DiffStatus::Removed);
    }

    #[test]
    fn tree_modified_leaf() {
        let before = json!({ "a": 1 });
        let after = json!({ "a": 2 });
        let node = tree(&before, &after);
        assert_eq!(node.status, DiffStatus::Partial);
        let a = &node.children[0];
        assert_eq!(a.status, DiffStatus::Modified);
        assert_eq!(a.change_count, 1);
        assert_eq!(a.preview_before.as_deref(), Some("1"));
        assert_eq!(a.preview_after.as_deref(), Some("2"));
    }

    #[test]
    fn tree_nested_partial_rollup() {
        let before = json!({ "outer": { "x": 1, "y": 2 } });
        let after = json!({ "outer": { "x": 9, "y": 2 } });
        let node = tree(&before, &after);
        assert_eq!(node.status, DiffStatus::Partial);
        assert_eq!(node.change_count, 1);
        let outer = &node.children[0];
        assert_eq!(outer.key, "outer");
        assert_eq!(outer.status, DiffStatus::Partial);
        assert_eq!(outer.change_count, 1);
        // Only the changed child (x) survives pruning.
        assert_eq!(outer.children.len(), 1);
        assert_eq!(outer.children[0].key, "x");
    }

    #[test]
    fn tree_sibling_union_added_and_removed() {
        let before = json!({ "keep": 1, "gone": 2 });
        let after = json!({ "keep": 1, "fresh": 3 });
        let node = tree(&before, &after);
        // fresh (added, b-order) + gone (removed) = 2 changes.
        assert_eq!(node.change_count, 2);
        // b-order union puts `fresh` before the removed `gone`.
        assert_eq!(node.children[0].key, "fresh");
        assert_eq!(node.children[1].key, "gone");
    }

    #[test]
    fn tree_shape_mismatch_is_modified() {
        let before = json!({ "a": [1, 2] });
        let after = json!({ "a": { "k": 1 } });
        let node = tree(&before, &after);
        let a = &node.children[0];
        assert_eq!(a.status, DiffStatus::Modified);
        assert_eq!(a.change_count, 1);
    }
}
