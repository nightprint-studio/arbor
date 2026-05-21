//! Properties Studio — editable Java/Spring `.properties` document
//! registry (Phase 6).
//!
//! Owned by `PropertiesBackend` (see `backend_impl.rs`) which exposes it
//! through the unified `StudioFormatBackend` trait.
//!
//! Doc model:
//!   - `original`     — text the file was opened with, snapshot-immutable.
//!   - `current`      — live edited buffer the FE sees through `raw_current`.
//!   - `lines`        — `Vec<RawLine>` — exact byte-preserving line view
//!                      (`Logical { key, value, leading_ws, separator,
//!                      after_value }`, `Comment(raw)`, `Blank(raw)`).
//!                      Continuation backslashes are joined into a single
//!                      logical line so a mutation only touches the
//!                      affected block; emit() walks the same vec.
//!   - `value`        — `serde_json::Value` projection of the key/value
//!                      pairs, built from dotted-key segments via the
//!                      properties_codec assembly logic. Used by the
//!                      tree pane + JSONPath query (FROZEN F6 — same
//!                      JSONPath syntax as every other studio format).
//!   - `history`      — text snapshots backing undo / redo. Same coalesce
//!                      window as the YAML / TOML backends.
//!   - encoding       — sniffed at parse time, round-tripped through save
//!                      (FROZEN F16 — windows-1252 / UTF-16 BOM survive
//!                      legacy Spring Boot configs on Windows).
//!
//! FROZEN F4: lossless edit is `true`. `.properties` is intrinsically
//! line-oriented + sequential, so a per-line view is the natural rowan
//! analog — every comment, blank line, trailing whitespace and Unicode
//! escape survives an edit cycle.

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json_path::{JsonPath, PathElement};
use similar::{ChangeTag, TextDiff};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::studio::format::types::{
    DiffHunk, DiffLine, DiffLineKind, DiffStatus, DiffTreeNode,
};

pub mod backend_impl;

// ── Doc registry ────────────────────────────────────────────────────────

#[derive(Default)]
pub struct PropertiesStudioRegistry {
    docs: HashMap<String, Doc>,
}

struct Doc {
    original:       String,
    current:        String,
    /// Parsed line view. `None` only when the buffer is unparseable —
    /// in practice `.properties` has no unparseable byte stream; we keep
    /// the `Option` for parity with other backends and to gate
    /// structural mutations behind a freshly-parsed view.
    lines:          Option<Vec<RawLine>>,
    /// JSON projection — `Object` at root, dotted segments split into
    /// nested objects, `[N]` brackets recognised as array indices
    /// (Spring-compatible). Empty file projects to `Object({})`.
    value:          Option<Value>,
    parse_error:    Option<String>,
    /// Indent string — `.properties` has no nested indent, but the FE
    /// still asks for `get_indent` to seed editor preferences. We
    /// surface "  " unconditionally; the field exists for parity.
    indent:         String,
    source_path:    Option<String>,
    encoding_label: String,
    had_bom:        bool,
    history:        Vec<String>,
    history_pos:    usize,
    coalesce_armed: bool,
    last_push:      Instant,
}

// ── Public types ────────────────────────────────────────────────────────

/// Kind tag for the FE tree pane. `.properties` is structurally a flat
/// `string` → `string` map; the JSON projection promotes dotted keys to
/// nested `Object`/`Array` containers. Inner leaves are always
/// rendered as `string` since `.properties` has no typing. We keep the
/// `null` variant so the bulk-edit modal's null-policy display works
/// uniformly with the other formats.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Object,
    Array,
    String,
    Null,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Object => "object",
            NodeKind::Array  => "array",
            NodeKind::String => "string",
            NodeKind::Null   => "null",
        }
    }
}

#[derive(Debug)]
pub struct ParseResult {
    pub doc_id:      String,
    pub size_bytes:  usize,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    pub source_path: Option<String>,
    pub parse_error: Option<String>,
}

#[derive(Debug)]
pub struct UpdateResult {
    pub parse_error: Option<String>,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    pub can_undo:    bool,
    pub can_redo:    bool,
}

#[derive(Debug)]
pub struct MutateResult {
    pub text:        String,
    pub parse_error: Option<String>,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    pub can_undo:    bool,
    pub can_redo:    bool,
}

#[derive(Debug)]
pub struct NodeView {
    pub key:         String,
    pub path:        Vec<String>,
    pub kind:        NodeKind,
    pub preview:     String,
    pub child_count: usize,
}

#[derive(Debug)]
pub struct QueryHit {
    pub path:    Vec<String>,
    pub kind:    NodeKind,
    pub preview: String,
}

const PREVIEW_MAX_CHARS:  usize = 64;
const QUERY_MAX_HITS:     usize = 500;
const HISTORY_CAP:        usize = 200;
const COALESCE_WINDOW_MS: u128  = 500;

// ── Line view ───────────────────────────────────────────────────────────

/// A single physical block in the source buffer. Continuation lines
/// (`\` at EOL) get joined into one `Logical { value_text }` block
/// where `value_text` keeps the joined string post-unescape — but on
/// emit we re-split using the original `continuation_marks` so the
/// line shape survives byte-for-byte.
#[derive(Debug, Clone)]
pub enum RawLine {
    /// `# foo` or `! foo` line, possibly trailing whitespace.
    Comment(String),
    /// Empty / whitespace-only line.
    Blank(String),
    /// A logical key=value entry. The struct fields preserve every byte
    /// of the original source for lossless round-trip.
    Logical {
        /// Whitespace before the key (typically empty, but `.properties`
        /// allows leading whitespace).
        leading_ws: String,
        /// Raw key text exactly as it appears in source, *with* escapes.
        key_raw:    String,
        /// Decoded key — escapes + `\uXXXX` resolved.
        key:        String,
        /// The separator between key and value: `=`, `:`, or whitespace.
        /// Includes any whitespace padding around it so `host = value`
        /// vs `host=value` both survive.
        separator:  String,
        /// Raw value text — INCLUDES the continuation backslashes and the
        /// trailing whitespace on every physical line. On emit we splice
        /// it back as-is.
        value_raw:  String,
        /// Decoded value — escapes + `\uXXXX` + continuation joins
        /// resolved. The FE shows this; mutations write back via
        /// `value_raw` rebuilt from the new decoded text.
        value:      String,
    },
}

// ── Parsing ─────────────────────────────────────────────────────────────

/// Parse a `.properties` text into `(lines, value, error)`.
///
/// `.properties` has no truly-failing byte stream so `error` is `None`
/// in practice; we surface a structured error when a dotted-key
/// assembly conflict is detected (e.g. `foo=string` AND `foo.bar=42`).
/// In that case `value` falls back to a flat `Object` mapping keys to
/// their string values so the FE can still render something useful.
fn parse_text(text: &str) -> (Option<Vec<RawLine>>, Option<Value>, Option<String>) {
    let lines = parse_lines(text);

    // Build the JSON projection from the decoded key/value pairs. We
    // accept duplicate keys (last wins) and try to assemble dotted-key
    // segments into nested Objects/Arrays. On conflict we degrade to a
    // flat Object<String, String> + emit a parse warning so the user
    // sees something instead of losing the tree pane entirely.
    let (value, parse_err) = build_projection(&lines);

    (Some(lines), Some(value), parse_err)
}

/// Split the source buffer into `RawLine`s. Backslash continuations
/// (when not escaped) extend a logical value across physical lines —
/// we join them into a single `Logical.value_raw` so the FE's get_value
/// returns the joined string, while emit() splits back identically.
fn parse_lines(text: &str) -> Vec<RawLine> {
    let mut out: Vec<RawLine> = Vec::new();
    let mut iter = text.split_inclusive('\n').peekable();
    while let Some(physical) = iter.next() {
        let trimmed_left = physical.trim_start_matches(['\t', ' ']);
        // Strip the trailing newline ourselves so we can re-attach it
        // identically on emit (preserve \r\n vs \n).
        let (body, eol) = strip_eol(physical);

        // Classify: blank, comment, or logical.
        let leading_ws: String = physical.chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();

        if trimmed_left.trim().is_empty() {
            // Pure blank or whitespace-only line.
            out.push(RawLine::Blank(format!("{body}{eol}")));
            continue;
        }
        if trimmed_left.starts_with('#') || trimmed_left.starts_with('!') {
            out.push(RawLine::Comment(format!("{body}{eol}")));
            continue;
        }

        // Logical line. Walk the body to find the key/separator split,
        // then look ahead for continuation lines.
        let body_after_ws = &body[leading_ws.len()..];
        let (key_raw, sep, value_first) = split_key_value(body_after_ws);

        // Accumulate continuation lines into `value_raw`.
        let mut value_raw = format!("{value_first}{eol}");
        // A line "continues" when it ends with an unescaped backslash
        // *before* the newline. We trim the EOL first then test.
        while is_continued(&value_raw) {
            let Some(next_physical) = iter.next() else { break; };
            let (next_body, next_eol) = strip_eol(next_physical);
            value_raw.push_str(&format!("{next_body}{next_eol}"));
        }

        let key      = decode_unicode(&unescape_key(&key_raw));
        let value    = decode_unicode(&unescape_value(&join_continuations(&value_raw)));

        out.push(RawLine::Logical {
            leading_ws,
            key_raw:   key_raw.to_string(),
            key,
            separator: sep.to_string(),
            value_raw,
            value,
        });
    }
    out
}

fn strip_eol(s: &str) -> (&str, &str) {
    if let Some(stripped) = s.strip_suffix("\r\n") {
        (stripped, "\r\n")
    } else if let Some(stripped) = s.strip_suffix('\n') {
        (stripped, "\n")
    } else if let Some(stripped) = s.strip_suffix('\r') {
        (stripped, "\r")
    } else {
        (s, "")
    }
}

/// Detect whether the (possibly multi-physical) value ends with an
/// unescaped trailing backslash before the EOL. Counts trailing `\` to
/// distinguish `foo\\` (escaped, NOT a continuation) from `foo\` (is).
fn is_continued(value_raw: &str) -> bool {
    // Look at the last physical line (split inclusive uses `\n`).
    let last = value_raw
        .rsplit_terminator('\n')
        .next()
        .unwrap_or("")
        .trim_end_matches('\r');
    let count = last.chars().rev().take_while(|c| *c == '\\').count();
    count % 2 == 1
}

fn join_continuations(value_raw: &str) -> String {
    let mut out = String::new();
    for line in value_raw.split_inclusive('\n') {
        let (body, _eol) = strip_eol(line);
        // Strip leading whitespace of physical continuation lines —
        // Java spec says continuation lines have their leading whitespace
        // dropped before being joined.
        let body = if out.is_empty() {
            body.to_string()
        } else {
            body.trim_start_matches(['\t', ' ']).to_string()
        };
        if is_continued_last(&body) {
            // Drop the trailing backslash, don't add newline.
            let trimmed = &body[..body.len() - 1];
            out.push_str(trimmed);
        } else {
            out.push_str(&body);
        }
    }
    out
}

fn is_continued_last(line: &str) -> bool {
    let count = line.chars().rev().take_while(|c| *c == '\\').count();
    count % 2 == 1
}

/// Find the first unescaped `=`, `:`, or run-of-whitespace separator
/// and split. Returns `(key, separator, value_starting_text)`.
fn split_key_value(body: &str) -> (&str, &str, &str) {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if c == b'=' || c == b':' || c == b' ' || c == b'\t' {
            break;
        }
        i += 1;
    }
    let key_end = i;
    // Consume whitespace + at most one `=`/`:`.
    let mut j = i;
    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') { j += 1; }
    if j < bytes.len() && (bytes[j] == b'=' || bytes[j] == b':') { j += 1; }
    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') { j += 1; }
    let sep_end = j;
    (&body[..key_end], &body[key_end..sep_end], &body[sep_end..])
}

fn unescape_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it  = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(&next) = it.peek() {
                match next {
                    '=' | ':' | '#' | '!' | ' ' | '\\' => { out.push(next); it.next(); }
                    'n' => { out.push('\n'); it.next(); }
                    'r' => { out.push('\r'); it.next(); }
                    't' => { out.push('\t'); it.next(); }
                    _   => out.push(c),
                }
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn unescape_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it  = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(&next) = it.peek() {
                match next {
                    'n'  => { out.push('\n'); it.next(); }
                    'r'  => { out.push('\r'); it.next(); }
                    't'  => { out.push('\t'); it.next(); }
                    '\\' => { out.push('\\'); it.next(); }
                    _    => { out.push(next); it.next(); }
                }
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Decode Java `\uXXXX` escapes in an already-unescaped string.
fn decode_unicode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 5 < bytes.len() && bytes[i + 1] == b'u' {
            let hex = &s[i + 2..i + 6];
            if let Ok(n) = u32::from_str_radix(hex, 16) {
                if let Some(c) = char::from_u32(n) {
                    out.push(c);
                    i += 6;
                    continue;
                }
            }
        }
        let ch_size = s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        out.push_str(&s[i..i + ch_size]);
        i += ch_size;
    }
    out
}

fn escape_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _    => out.push(c),
        }
    }
    out
}

fn escape_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '=' | ':' | '#' | '!' | ' ' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _    => out.push(c),
        }
    }
    out
}

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
/// spec: last wins. We don't surface a warning — the source still has
/// both lines and the user can find them via the Text view.
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

/// Reserved key used inside the projected JSON to carry the leaf value
/// of a key that is ALSO a prefix for sub-keys. Never written to the
/// `.properties` source — `path_to_flat_key` strips it.
const VALUE_SENTINEL: &str = "$value";

fn insert_into_tree(
    root: &mut TreeNode,
    segments: &[KeySegment],
    value: &str,
) {
    if segments.is_empty() {
        // Leaf write at the current position. If a container already
        // exists here, stash the leaf as `$value` so both the prefix's
        // own value AND its sub-keys remain visible.
        match root {
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
                let mut seq: Vec<Option<TreeNode>> = Vec::new();
                seq.push(Some(TreeNode::Leaf(leaf_val)));
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

// ── Emit ────────────────────────────────────────────────────────────────

fn emit_lines(lines: &[RawLine]) -> String {
    let mut out = String::new();
    for line in lines {
        match line {
            RawLine::Comment(s) | RawLine::Blank(s) => out.push_str(s),
            RawLine::Logical { leading_ws, key_raw, separator, value_raw, .. } => {
                out.push_str(leading_ws);
                out.push_str(key_raw);
                out.push_str(separator);
                out.push_str(value_raw);
            }
        }
    }
    out
}

/// Build the full key string for a `Logical` line including `leading_ws`.
#[allow(dead_code)]
fn logical_full_key(line: &RawLine) -> Option<&str> {
    if let RawLine::Logical { key, .. } = line { Some(key) } else { None }
}

// ── Registry impl ───────────────────────────────────────────────────────

impl PropertiesStudioRegistry {
    pub fn parse(
        &mut self,
        text:           String,
        source_path:    Option<String>,
        encoding_label: String,
        had_bom:        bool,
    ) -> ParseResult {
        let size = text.len();
        let (lines, value, parse_error) = parse_text(&text);
        let kind        = value.as_ref().map(value_kind);
        let child_count = value.as_ref().map(value_child_count).unwrap_or(0);
        let id          = Uuid::new_v4().to_string();
        self.docs.insert(id.clone(), Doc {
            original:       text.clone(),
            current:        text.clone(),
            lines,
            value,
            parse_error:    parse_error.clone(),
            indent:         "  ".to_string(),
            source_path:    source_path.clone(),
            encoding_label,
            had_bom,
            history:        vec![text],
            history_pos:    0,
            coalesce_armed: false,
            last_push:      Instant::now(),
        });
        ParseResult {
            doc_id:      id,
            size_bytes:  size,
            root_kind:   kind,
            child_count,
            source_path,
            parse_error,
        }
    }

    pub fn close(&mut self, doc_id: &str) {
        self.docs.remove(doc_id);
    }

    fn doc(&self, doc_id: &str) -> Result<&Doc> {
        self.docs.get(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown Properties Studio doc: {doc_id}")))
    }
    fn doc_mut(&mut self, doc_id: &str) -> Result<&mut Doc> {
        self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown Properties Studio doc: {doc_id}")))
    }

    pub fn raw_original(&self, doc_id: &str) -> Result<String> {
        Ok(self.doc(doc_id)?.original.clone())
    }
    pub fn raw_current(&self, doc_id: &str) -> Result<String> {
        Ok(self.doc(doc_id)?.current.clone())
    }
    pub fn source_path(&self, doc_id: &str) -> Result<Option<String>> {
        Ok(self.doc(doc_id)?.source_path.clone())
    }
    pub fn encoding_info(&self, doc_id: &str) -> Result<(String, bool)> {
        let d = self.doc(doc_id)?;
        Ok((d.encoding_label.clone(), d.had_bom))
    }
    pub fn parse_error(&self, doc_id: &str) -> Result<Option<String>> {
        Ok(self.doc(doc_id)?.parse_error.clone())
    }
    pub fn root_kind(&self, doc_id: &str) -> Result<Option<NodeKind>> {
        Ok(self.doc(doc_id)?.value.as_ref().map(value_kind))
    }
    pub fn root_child_count(&self, doc_id: &str) -> Result<usize> {
        Ok(self.doc(doc_id)?.value.as_ref()
            .map(value_child_count).unwrap_or(0))
    }
    pub fn get_indent(&self, doc_id: &str) -> Result<String> {
        Ok(self.doc(doc_id)?.indent.clone())
    }
    pub fn set_indent(&mut self, doc_id: &str, indent: String) -> Result<()> {
        self.doc_mut(doc_id)?.indent = indent;
        Ok(())
    }
    pub fn history_state(&self, doc_id: &str) -> Result<(bool, bool)> {
        let d = self.doc(doc_id)?;
        Ok((d.history_pos > 0, d.history_pos + 1 < d.history.len()))
    }

    pub fn get_root(&self, doc_id: &str) -> Result<NodeView> {
        let doc = self.doc(doc_id)?;
        let v = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot navigate".into()))?;
        Ok(node_view_for_value("$", &[], v))
    }

    pub fn get_children(&self, doc_id: &str, path: &[String]) -> Result<Vec<NodeView>> {
        let doc = self.doc(doc_id)?;
        let v = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot navigate".into()))?;
        let cur = resolve_value(v, path)
            .ok_or_else(|| AppError::Other(format!("Missing path: {path:?}")))?;
        Ok(children_of_value(path, cur))
    }

    pub fn get_value_pretty(&self, doc_id: &str, path: &[String]) -> Result<String> {
        let doc = self.doc(doc_id)?;
        let v = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot read value".into()))?;
        let cur = resolve_value(v, path)
            .ok_or_else(|| AppError::Other(format!("Missing path: {path:?}")))?;
        serde_json::to_string_pretty(cur).map_err(|e| AppError::Other(e.to_string()))
    }

    pub fn pretty(&self, doc_id: &str) -> Result<String> {
        // `.properties` has no canonical pretty form — we already preserve
        // every byte. Returning the current buffer keeps `Ctrl+Shift+I`
        // a no-op but lets the FE call `format_doc` indiscriminately.
        Ok(self.doc(doc_id)?.current.clone())
    }

    pub fn set_text(&mut self, doc_id: &str, text: String) -> Result<UpdateResult> {
        let (lines, value, parse_error) = parse_text(&text);
        let root_kind   = value.as_ref().map(value_kind);
        let child_count = value.as_ref().map(value_child_count).unwrap_or(0);
        let doc = self.doc_mut(doc_id)?;
        record_history(doc, &text, /* can_coalesce */ true);
        doc.current     = text;
        doc.lines       = lines;
        doc.value       = value;
        doc.parse_error = parse_error.clone();
        let can_undo = doc.history_pos > 0;
        let can_redo = doc.history_pos + 1 < doc.history.len();
        Ok(UpdateResult { parse_error, root_kind, child_count, can_undo, can_redo })
    }

    // ── Structural mutations ───────────────────────────────────────

    fn mutate_with<F>(&mut self, doc_id: &str, op: F) -> Result<MutateResult>
    where
        F: FnOnce(&mut Vec<RawLine>) -> Result<()>,
    {
        let doc = self.doc_mut(doc_id)?;
        let mut working = doc.lines.clone()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot edit".into()))?;
        op(&mut working)?;
        let new_text = emit_lines(&working);
        let (lines, value, parse_error) = parse_text(&new_text);
        if let Some(err) = &parse_error {
            // Conflict warning is non-fatal for parse — let it surface as
            // a banner. We only reject when the projection truly failed,
            // which can't happen for `.properties`.
            let _ = err;
        }
        record_history(doc, &new_text, /* can_coalesce */ false);
        let kind        = value.as_ref().map(value_kind);
        let child_count = value.as_ref().map(value_child_count).unwrap_or(0);
        doc.current     = new_text.clone();
        doc.lines       = lines;
        doc.value       = value;
        doc.parse_error = parse_error;
        let can_undo = doc.history_pos > 0;
        let can_redo = doc.history_pos + 1 < doc.history.len();
        Ok(MutateResult {
            text:        new_text,
            parse_error: doc.parse_error.clone(),
            root_kind:   kind,
            child_count,
            can_undo,
            can_redo,
        })
    }

    pub fn mutate_primitive(
        &mut self,
        doc_id: &str,
        path:   &[String],
        value:  Value,
    ) -> Result<MutateResult> {
        let path = path.to_vec();
        self.mutate_with(doc_id, move |lines| {
            let new_str = primitive_to_string(&value);
            set_value_at_path(lines, &path, &new_str)
        })
    }

    pub fn replace_at(
        &mut self,
        doc_id:  &str,
        path:    &[String],
        snippet: String,
    ) -> Result<MutateResult> {
        // `.properties` has no nested snippet syntax; replace_at is
        // identical to set_primitive with the snippet as raw string.
        let path = path.to_vec();
        self.mutate_with(doc_id, move |lines| {
            set_value_at_path(lines, &path, &snippet)
        })
    }

    pub fn remove_at(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot remove document root".into()));
        }
        let path = path.to_vec();
        self.mutate_with(doc_id, move |lines| {
            remove_at_path(lines, &path)
        })
    }

    pub fn insert_field(
        &mut self,
        doc_id:  &str,
        path:    &[String],
        name:    String,
        snippet: String,
    ) -> Result<MutateResult> {
        let path = path.to_vec();
        self.mutate_with(doc_id, move |lines| {
            let mut full = path.clone();
            full.push(name.clone());
            insert_or_set(lines, &full, &snippet)
        })
    }

    pub fn insert_item(
        &mut self,
        doc_id:  &str,
        path:    &[String],
        snippet: String,
    ) -> Result<MutateResult> {
        let path = path.to_vec();
        self.mutate_with(doc_id, move |lines| {
            // Append at the next index under `path`. Use Spring `[N]`
            // bracket notation so the assembled tree treats it as an
            // array index.
            let mut next = path.clone();
            let n = next_array_index_under(lines, &path);
            // Encode array index into the LAST existing segment when
            // present (Spring style: `path = ["servers"]` + idx 0
            // → key `servers[0]`); otherwise create a synthetic
            // bracketed segment at root level.
            if let Some(last) = next.last_mut() {
                *last = format!("{last}[{n}]");
            } else {
                next.push(format!("[{n}]"));
            }
            insert_or_set(lines, &next, &snippet)
        })
    }

    pub fn insert_map_entry(
        &mut self,
        doc_id:   &str,
        path:     &[String],
        key_text: String,
        val_text: String,
    ) -> Result<MutateResult> {
        // Maps and "fields" are interchangeable in `.properties`.
        self.insert_field(doc_id, path, key_text, val_text)
    }

    pub fn duplicate_at(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot duplicate document root".into()));
        }
        let path = path.to_vec();
        self.mutate_with(doc_id, move |lines| {
            duplicate_at_path(lines, &path)
        })
    }

    pub fn move_item(&mut self, doc_id: &str, path: &[String], delta: i32) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot move document root".into()));
        }
        let path = path.to_vec();
        self.mutate_with(doc_id, move |lines| {
            move_at_path(lines, &path, delta)
        })
    }

    // ── Undo / redo ───────────────────────────────────────────────

    pub fn undo(&mut self, doc_id: &str) -> Result<MutateResult> {
        let doc = self.doc_mut(doc_id)?;
        if doc.history_pos == 0 {
            return Err(AppError::Other("Nothing to undo".into()));
        }
        doc.history_pos -= 1;
        Self::apply_history_cursor(doc)
    }

    pub fn redo(&mut self, doc_id: &str) -> Result<MutateResult> {
        let doc = self.doc_mut(doc_id)?;
        if doc.history_pos + 1 >= doc.history.len() {
            return Err(AppError::Other("Nothing to redo".into()));
        }
        doc.history_pos += 1;
        Self::apply_history_cursor(doc)
    }

    fn apply_history_cursor(doc: &mut Doc) -> Result<MutateResult> {
        let text = doc.history[doc.history_pos].clone();
        let (lines, value, parse_error) = parse_text(&text);
        let kind        = value.as_ref().map(value_kind);
        let child_count = value.as_ref().map(value_child_count).unwrap_or(0);
        doc.current        = text.clone();
        doc.lines          = lines;
        doc.value          = value;
        doc.parse_error    = parse_error.clone();
        doc.coalesce_armed = false;
        doc.last_push      = Instant::now();
        let can_undo = doc.history_pos > 0;
        let can_redo = doc.history_pos + 1 < doc.history.len();
        Ok(MutateResult { text, parse_error, root_kind: kind, child_count, can_undo, can_redo })
    }

    // ── Diff ──────────────────────────────────────────────────────

    pub fn diff(&self, doc_id: &str) -> Result<Vec<DiffHunk>> {
        let doc = self.doc(doc_id)?;
        Ok(unified_diff(&doc.original, &doc.current))
    }

    pub fn tree_diff(&self, doc_id: &str) -> Result<DiffTreeNode> {
        let doc = self.doc(doc_id)?;
        let orig_val = parse_text(&doc.original).1;
        let curr_val = doc.value.clone();
        Ok(build_tree_diff(orig_val.as_ref(), curr_val.as_ref()))
    }

    // ── Save ──────────────────────────────────────────────────────

    pub fn mark_saved(&mut self, doc_id: &str) -> Result<()> {
        let doc = self.doc_mut(doc_id)?;
        doc.original = doc.current.clone();
        Ok(())
    }
    pub fn rebind_source(&mut self, doc_id: &str, path: String) -> Result<()> {
        let doc = self.doc_mut(doc_id)?;
        doc.source_path = Some(path);
        Ok(())
    }

    // ── Query ─────────────────────────────────────────────────────

    pub fn query(&self, doc_id: &str, expr: &str) -> Result<Vec<QueryHit>> {
        let doc = self.doc(doc_id)?;
        let v = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot query".into()))?;
        let normalised = normalise_query(expr);
        if normalised.is_empty() { return Ok(Vec::new()); }
        let path = JsonPath::parse(&normalised)
            .map_err(|e| AppError::Other(format!("Invalid JSONPath: {e}")))?;
        let result = path.query_located(v);
        let mut out = Vec::new();
        for located in result.all() {
            if out.len() >= QUERY_MAX_HITS { break; }
            let mut segments: Vec<String> = Vec::new();
            for el in located.location().iter() {
                match el {
                    PathElement::Name(n)  => segments.push(n.to_string()),
                    PathElement::Index(i) => segments.push(i.to_string()),
                }
            }
            let node = located.node();
            out.push(QueryHit {
                path:    segments,
                kind:    value_kind(node),
                preview: preview_for_value(node),
            });
        }
        Ok(out)
    }

    pub fn query_value_pairs(
        &self,
        doc_id: &str,
        expr:   &str,
    ) -> Result<Vec<(Vec<String>, Value)>> {
        let doc = self.doc(doc_id)?;
        let root = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot query".into()))?;
        query_value_pairs_against(root, expr)
    }

    pub fn apply_bulk_edits_doc(
        &mut self,
        doc_id: &str,
        ops:    &[(Vec<String>, PropertiesBulkOp)],
    ) -> Result<MutateResult> {
        let ops = ops.to_vec();
        self.mutate_with(doc_id, move |lines| {
            apply_bulk_edits_in_place(lines, &ops)
        })
    }
}

// ── On-disk write ───────────────────────────────────────────────────────

pub fn write_to_disk(
    path:           &str,
    contents:       &str,
    encoding_label: &str,
    had_bom:        bool,
) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Other(format!("mkdir {parent:?}: {e}")))?;
        }
    }
    let bytes = crate::git::encoding::encode_for_disk_with_bom(
        contents,
        Some(encoding_label),
        had_bom,
    );
    std::fs::write(path, &bytes).map_err(|e| AppError::Other(format!("write {path}: {e}")))
}

// ── Public helpers consumed by `studio::*` walkers ──────────────────────

/// Parse a `.properties` text to the projected JSON value (no doc
/// state). Mirrors `yaml_studio::parse_to_value` / `toml_studio::parse_to_value`.
pub fn parse_to_value(text: &str) -> Option<Value> {
    parse_text(text).1
}

/// Walk every logical key in the document and return them as flat keys
/// (`server.port`, `servers[0]`, …). The studio cross-ref walker uses
/// this to emit `CrossRefDef` entries — FROZEN F5: every key in
/// `.properties` is a potential cross-ref target.
#[allow(dead_code)]
pub fn collect_all_keys(text: &str) -> Vec<String> {
    let lines = parse_lines(text);
    let mut out = Vec::with_capacity(lines.len());
    for line in &lines {
        if let RawLine::Logical { key, .. } = line {
            if !key.is_empty() { out.push(key.clone()); }
        }
    }
    out
}

/// Walk every logical key in the document and return `(key, value)`
/// pairs. Used by the project-wide broken-ref / usage scanners.
pub fn collect_kv_pairs(text: &str) -> Vec<(String, String)> {
    let lines = parse_lines(text);
    let mut out = Vec::with_capacity(lines.len());
    for line in &lines {
        if let RawLine::Logical { key, value, .. } = line {
            if !key.is_empty() { out.push((key.clone(), value.clone())); }
        }
    }
    out
}

// ── F12 / F13 helpers ───────────────────────────────────────────────────

/// Concrete write target for a `.properties` site. Always coerces to a
/// string because `.properties` has no native typing — the bulk-edit
/// modal's typed input (number/bool/null) collapses to the string
/// representation here.
#[derive(Debug, Clone)]
pub enum PropertiesSetValue {
    String(String),
    /// `key=` — empty value, key preserved. FROZEN F4 `null_handling = AskUser`
    /// default for null literal in F13. The "remove key entirely"
    /// alternative is the explicit `Delete` action.
    Empty,
}

#[derive(Debug, Clone)]
pub enum PropertiesBulkOp {
    Set(PropertiesSetValue),
    Delete,
}

impl PropertiesSetValue {
    fn to_string_value(&self) -> String {
        match self {
            PropertiesSetValue::String(s) => s.clone(),
            PropertiesSetValue::Empty     => String::new(),
        }
    }
}

fn apply_bulk_edits_in_place(
    lines: &mut Vec<RawLine>,
    ops:   &[(Vec<String>, PropertiesBulkOp)],
) -> Result<()> {
    // Phase A — sets.
    for (path, op) in ops {
        let PropertiesBulkOp::Set(val) = op else { continue; };
        set_value_at_path(lines, path, &val.to_string_value())?;
    }
    // Phase B — deletes, sorted reverse by flat key index so line removals
    // don't shift earlier indices for grouped deletes.
    let mut delete_paths: Vec<Vec<String>> = ops.iter()
        .filter_map(|(p, op)| match op {
            PropertiesBulkOp::Delete => Some(p.clone()),
            _ => None,
        })
        .collect();
    delete_paths.sort_by(|a, b| b.cmp(a));
    delete_paths.dedup();
    for p in delete_paths {
        if p.is_empty() {
            return Err(AppError::Other("Cannot delete the document root".into()));
        }
        let _ = remove_at_path(lines, &p);
    }
    Ok(())
}

/// Project-wide bulk-edit entry — parse, apply, re-emit. Pre-flush
/// route: caller writes to disk only when this returns Ok.
pub fn apply_bulk_edits_text(
    input: &str,
    ops:   &[(Vec<String>, PropertiesBulkOp)],
) -> Result<String> {
    let mut lines = parse_lines(input);
    apply_bulk_edits_in_place(&mut lines, ops)?;
    Ok(emit_lines(&lines))
}

/// Run a JSON-Path expression against `root` (the projected JSON Value)
/// and return owned `(path, value)` pairs.
pub fn query_value_pairs_against(
    root: &Value,
    expr: &str,
) -> Result<Vec<(Vec<String>, Value)>> {
    let normalised = normalise_query(expr);
    if normalised.is_empty() { return Ok(Vec::new()); }
    let path = JsonPath::parse(&normalised)
        .map_err(|e| AppError::Other(format!("Query parse error: {e}")))?;
    let located = path.query_located(root);
    let mut out = Vec::with_capacity(QUERY_MAX_HITS.min(located.len()));
    for ln in located.iter() {
        if out.len() >= QUERY_MAX_HITS { break; }
        let p: Vec<String> = ln.location().iter().map(|el| match el {
            PathElement::Name(s)  => s.to_string(),
            PathElement::Index(i) => i.to_string(),
        }).collect();
        out.push((p, ln.node().clone()));
    }
    Ok(out)
}

pub fn props_kind_str(v: &Value) -> &'static str { value_kind(v).as_str() }
pub fn props_preview_for(v: &Value) -> String { preview_for_value(v) }

/// Per-site descriptor for `apply_rename_in_text`. Mirrors the FE's
/// `RenameSite.scope` enum: a `.properties` rename can touch the LHS
/// (key declaration) or RHS (value reference) of a `key=value` line.
#[derive(Debug, Clone)]
pub enum PropertiesRenameScope {
    /// Site is the key (LHS) of a `key=value` line. The line's key is
    /// `field_path` joined into flat-key form, and matches `old_value`.
    Key,
    /// Site is the value (RHS) of a `key=value` line whose key is at
    /// `field_path` (joined to flat form). The value matches `old_value`.
    Value,
}

#[derive(Debug, Clone)]
pub struct PropertiesRenameSite {
    pub field_path: Vec<String>,
    pub scope:      PropertiesRenameScope,
}

/// Rename every selected site in `text`. Pre-flush: validates every
/// site exists + matches the expected `old_value` before touching the
/// buffer. Returns the rewritten text — caller is responsible for the
/// disk flush (FROZEN F12 sequential rollback policy).
pub fn apply_rename_in_text(
    text:      &str,
    sites:     &[PropertiesRenameSite],
    old_value: &str,
    new_value: &str,
) -> Result<String> {
    let mut lines = parse_lines(text);

    use std::collections::HashSet;
    let mut key_flats: HashSet<String> = HashSet::new();
    let mut val_flats: HashSet<String> = HashSet::new();
    for s in sites {
        let flat = path_to_flat_key(&s.field_path);
        match s.scope {
            PropertiesRenameScope::Key   => { key_flats.insert(flat); }
            PropertiesRenameScope::Value => { val_flats.insert(flat); }
        }
    }

    // Validate.
    for k in &key_flats {
        let ok = lines.iter().any(|l| matches!(l, RawLine::Logical { key, .. } if key == k));
        if !ok {
            return Err(AppError::Other(format!(
                "Rename Key site not found: `{k}`",
            )));
        }
        if k != old_value {
            return Err(AppError::Other(format!(
                "Rename Key site `{k}` doesn't match old value `{old_value}`",
            )));
        }
    }
    for k in &val_flats {
        let ok = lines.iter().any(|l| matches!(l, RawLine::Logical { key, value, .. }
            if key == k && value == old_value));
        if !ok {
            return Err(AppError::Other(format!(
                "Rename Value site not found / mismatched: `{k}` (expected old `{old_value}`)",
            )));
        }
    }

    // Apply.
    for line in lines.iter_mut() {
        if let RawLine::Logical { key, key_raw, value, value_raw, .. } = line {
            if !key_flats.is_empty() && key_flats.contains(key) {
                *key     = new_value.to_string();
                *key_raw = escape_key(new_value);
                continue;
            }
            if !val_flats.is_empty() && val_flats.contains(key) && value == old_value {
                let (_, eol) = strip_eol(value_raw);
                let eol = if eol.is_empty() { "\n" } else { eol };
                *value     = new_value.to_string();
                *value_raw = format!("{}{eol}", escape_value(new_value));
            }
        }
    }

    Ok(emit_lines(&lines))
}

// ── Path mutation primitives ────────────────────────────────────────────

/// Convert a path `Vec<String>` into the flat `.properties` key. Segments
/// that parse as `usize` become Spring brackets on the previous segment.
/// The `$value` sentinel (used in the JSON projection to carry a leaf
/// value at a prefix that also has sub-keys) is stripped — when the FE
/// edits or removes `["foo", "$value"]`, the actual flat key in the
/// source is just `foo`.
fn path_to_flat_key(path: &[String]) -> String {
    let mut out = String::new();
    for seg in path.iter() {
        if seg == VALUE_SENTINEL { continue; }
        if let Ok(n) = seg.parse::<usize>() {
            out.push_str(&format!("[{n}]"));
        } else {
            if !out.is_empty() { out.push('.'); }
            out.push_str(seg);
        }
    }
    out
}

fn primitive_to_string(v: &Value) -> String {
    // The wire format may be tagged ({type,value}) or raw — accept either.
    let unwrapped = match v {
        Value::Object(map) if map.len() == 2
            && map.contains_key("type")
            && map.contains_key("value") => map.get("value").cloned().unwrap_or(Value::Null),
        other => other.clone(),
    };
    match unwrapped {
        Value::String(s) => s,
        Value::Bool(b)   => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null      => String::new(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(&unwrapped).unwrap_or_default(),
    }
}

/// Set the value at `path`. Creates a new logical line at the end if
/// the key is missing. Preserves separator + leading_ws of an existing
/// entry; uses `=` + no whitespace + `\n` for fresh keys.
fn set_value_at_path(lines: &mut Vec<RawLine>, path: &[String], new_value: &str) -> Result<()> {
    if path.is_empty() {
        return Err(AppError::Other("Cannot set value at the document root".into()));
    }
    let key = path_to_flat_key(path);
    for line in lines.iter_mut() {
        if let RawLine::Logical { key: k, value, value_raw, .. } = line {
            if k == &key {
                let (_, eol) = strip_eol(value_raw);
                let eol = if eol.is_empty() { "\n" } else { eol };
                *value     = new_value.to_string();
                *value_raw = format!("{}{eol}", escape_value(new_value));
                return Ok(());
            }
        }
    }
    // Key missing — append a fresh logical line, with a leading newline
    // when the buffer doesn't already end with one.
    let need_lead_nl = match lines.last() {
        Some(RawLine::Logical { value_raw, .. }) => !value_raw.ends_with('\n'),
        Some(RawLine::Comment(s)) | Some(RawLine::Blank(s)) => !s.ends_with('\n'),
        None => false,
    };
    if need_lead_nl {
        // Promote the last line's trailing newline.
        if let Some(last) = lines.last_mut() {
            match last {
                RawLine::Logical { value_raw, .. } => value_raw.push('\n'),
                RawLine::Comment(s) | RawLine::Blank(s) => s.push('\n'),
            }
        }
    }
    lines.push(RawLine::Logical {
        leading_ws: String::new(),
        key_raw:    escape_key(&key),
        key,
        separator:  "=".to_string(),
        value_raw:  format!("{}\n", escape_value(new_value)),
        value:      new_value.to_string(),
    });
    Ok(())
}

fn remove_at_path(lines: &mut Vec<RawLine>, path: &[String]) -> Result<()> {
    let target_key = path_to_flat_key(path);

    // Two semantics depending on whether the path targets the `$value`
    // sentinel (leaf at a prefix) or a regular node:
    //   - `$value` removal → wipe ONLY the exact-key line, leave the
    //                         sub-tree intact (the user clicked the
    //                         "self" row of a prefix that's also a
    //                         container).
    //   - regular removal  → wipe the exact-key line + every
    //                         descendant under that prefix.
    let leaf_only = path.last().map(|s| s == VALUE_SENTINEL).unwrap_or(false);

    let original_len = lines.len();
    if leaf_only {
        lines.retain(|line| match line {
            RawLine::Logical { key, .. } => key != &target_key,
            _ => true,
        });
    } else {
        let mut prefix = target_key.clone();
        prefix.push('.');
        let prefix_b = format!("{target_key}[");
        lines.retain(|line| match line {
            RawLine::Logical { key, .. } => {
                key != &target_key && !key.starts_with(&prefix) && !key.starts_with(&prefix_b)
            }
            _ => true,
        });
    }
    if lines.len() == original_len {
        return Err(AppError::Other(format!("Key not found: `{target_key}`")));
    }
    Ok(())
}

fn insert_or_set(lines: &mut Vec<RawLine>, path: &[String], snippet: &str) -> Result<()> {
    // If the key already exists, set; otherwise insert.
    set_value_at_path(lines, path, snippet)
}

fn next_array_index_under(lines: &[RawLine], path: &[String]) -> usize {
    let prefix = path_to_flat_key(path);
    let prefix_b = format!("{prefix}[");
    let mut max_seen: Option<usize> = None;
    for line in lines {
        if let RawLine::Logical { key, .. } = line {
            if let Some(rest) = key.strip_prefix(&prefix_b) {
                if let Some(end) = rest.find(']') {
                    if let Ok(n) = rest[..end].parse::<usize>() {
                        max_seen = Some(max_seen.map_or(n, |m| m.max(n)));
                    }
                }
            }
        }
    }
    max_seen.map_or(0, |m| m + 1)
}

fn duplicate_at_path(lines: &mut Vec<RawLine>, path: &[String]) -> Result<()> {
    let target_key = path_to_flat_key(path);
    let idx = lines.iter().position(|l| matches!(l, RawLine::Logical { key, .. } if key == &target_key))
        .ok_or_else(|| AppError::Other(format!("Key not found: `{target_key}`")))?;
    let mut cloned = lines[idx].clone();
    let new_key = format!("{target_key}_copy");
    if let RawLine::Logical { ref mut key, ref mut key_raw, .. } = cloned {
        *key     = new_key.clone();
        *key_raw = escape_key(&new_key);
    }
    lines.insert(idx + 1, cloned);
    Ok(())
}

fn move_at_path(lines: &mut Vec<RawLine>, path: &[String], delta: i32) -> Result<()> {
    let target_key = path_to_flat_key(path);
    let idx = lines.iter().position(|l| matches!(l, RawLine::Logical { key, .. } if key == &target_key))
        .ok_or_else(|| AppError::Other(format!("Key not found: `{target_key}`")))?;
    if delta == 0 { return Ok(()); }
    // Move only within the logical lines — skip blanks/comments when computing
    // the swap partner so the visible order changes by exactly `delta` rows.
    let direction = delta.signum();
    let mut steps = delta.unsigned_abs();
    let mut cur = idx;
    while steps > 0 {
        let next = if direction > 0 {
            // forward — find next Logical
            (cur + 1..lines.len()).find(|i| matches!(lines[*i], RawLine::Logical { .. }))
        } else {
            // backward
            (0..cur).rev().find(|i| matches!(lines[*i], RawLine::Logical { .. }))
        };
        let Some(next) = next else { break; };
        lines.swap(cur, next);
        cur = next;
        steps -= 1;
    }
    Ok(())
}

// ── Value-tree helpers ──────────────────────────────────────────────────

fn value_kind(v: &Value) -> NodeKind {
    match v {
        Value::Null      => NodeKind::Null,
        Value::Object(_) => NodeKind::Object,
        Value::Array(_)  => NodeKind::Array,
        Value::String(_) | Value::Bool(_) | Value::Number(_) => NodeKind::String,
    }
}

fn value_child_count(v: &Value) -> usize {
    match v {
        Value::Object(m) => m.len(),
        Value::Array(a)  => a.len(),
        _ => 0,
    }
}

fn resolve_value<'a>(root: &'a Value, path: &[String]) -> Option<&'a Value> {
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

fn children_of_value(parent: &[String], v: &Value) -> Vec<NodeView> {
    match v {
        Value::Object(map) => map.iter().map(|(k, child)| {
            let mut p = parent.to_vec();
            p.push(k.clone());
            node_view_for_value(k, &p, child)
        }).collect(),
        Value::Array(arr) => arr.iter().enumerate().map(|(i, child)| {
            let mut p = parent.to_vec();
            p.push(i.to_string());
            node_view_for_value(&i.to_string(), &p, child)
        }).collect(),
        _ => Vec::new(),
    }
}

fn node_view_for_value(key: &str, path: &[String], v: &Value) -> NodeView {
    NodeView {
        key:         key.to_string(),
        path:        path.to_vec(),
        kind:        value_kind(v),
        preview:     preview_for_value(v),
        child_count: value_child_count(v),
    }
}

fn preview_for_value(v: &Value) -> String {
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

fn normalise_query(expr: &str) -> String {
    let t = expr.trim();
    if t.is_empty() { return String::new(); }
    if t.starts_with('$') { return t.to_string(); }
    if t.starts_with("[") || t.starts_with('.') {
        return format!("${t}");
    }
    format!("$.{t}")
}

// ── Diff helpers ────────────────────────────────────────────────────────

fn unified_diff(a: &str, b: &str) -> Vec<DiffHunk> {
    let diff = TextDiff::from_lines(a, b);
    let mut hunks = Vec::new();
    let mut hunk = DiffHunk {
        old_start: 0, new_start: 0,
        old_count: 0, new_count: 0,
        lines:     Vec::new(),
    };
    let mut old_line = 1u32;
    let mut new_line = 1u32;
    let mut in_hunk = false;
    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            ChangeTag::Delete => DiffLineKind::Del,
            ChangeTag::Insert => DiffLineKind::Add,
            ChangeTag::Equal  => DiffLineKind::Context,
        };
        if !in_hunk && matches!(kind, DiffLineKind::Add | DiffLineKind::Del) {
            hunk = DiffHunk {
                old_start: old_line, new_start: new_line,
                old_count: 0, new_count: 0,
                lines:     Vec::new(),
            };
            in_hunk = true;
        }
        if in_hunk {
            let line = DiffLine {
                kind,
                old_line: matches!(kind, DiffLineKind::Context | DiffLineKind::Del).then_some(old_line),
                new_line: matches!(kind, DiffLineKind::Context | DiffLineKind::Add).then_some(new_line),
                text:     change.value().to_string(),
            };
            hunk.lines.push(line);
            match kind {
                DiffLineKind::Del     => hunk.old_count += 1,
                DiffLineKind::Add     => hunk.new_count += 1,
                DiffLineKind::Context => { hunk.old_count += 1; hunk.new_count += 1; }
            }
        }
        match kind {
            DiffLineKind::Del | DiffLineKind::Context => old_line += 1,
            _ => {}
        }
        match kind {
            DiffLineKind::Add | DiffLineKind::Context => new_line += 1,
            _ => {}
        }
    }
    if in_hunk && !hunk.lines.is_empty() {
        hunks.push(hunk);
    }
    hunks
}

fn build_tree_diff(orig: Option<&Value>, curr: Option<&Value>) -> DiffTreeNode {
    let status = diff_status(orig, curr);
    let mut node = make_diff_node("$".to_string(), Vec::new(), status, orig, curr);
    fill_tree_diff_children(&mut node, orig, curr);
    node.change_count = node.children.iter().map(|c| c.change_count).sum::<u32>()
        + if matches!(status, DiffStatus::Added | DiffStatus::Removed | DiffStatus::Modified) { 1 } else { 0 };
    node
}

fn make_diff_node(
    key:    String,
    path:   Vec<String>,
    status: DiffStatus,
    orig:   Option<&Value>,
    curr:   Option<&Value>,
) -> DiffTreeNode {
    DiffTreeNode {
        key,
        path,
        status,
        kind_before:    orig.map(value_kind_str),
        kind_after:     curr.map(value_kind_str),
        preview_before: orig.and_then(|v| {
            let p = preview_for_value(v);
            if p.is_empty() { None } else { Some(p) }
        }),
        preview_after:  curr.and_then(|v| {
            let p = preview_for_value(v);
            if p.is_empty() { None } else { Some(p) }
        }),
        tag_before:     None,
        tag_after:      None,
        children:       Vec::new(),
        change_count:   0,
    }
}

fn fill_tree_diff_children(parent: &mut DiffTreeNode, a: Option<&Value>, b: Option<&Value>) {
    use std::collections::BTreeSet;
    match (a, b) {
        (Some(Value::Object(ma)), Some(Value::Object(mb))) => {
            let mut keys: BTreeSet<&String> = BTreeSet::new();
            keys.extend(ma.keys()); keys.extend(mb.keys());
            for k in keys {
                let ca = ma.get(k);
                let cb = mb.get(k);
                let st = diff_status(ca, cb);
                let mut path = parent.path.clone(); path.push(k.clone());
                let mut child = make_diff_node(k.clone(), path, st, ca, cb);
                fill_tree_diff_children(&mut child, ca, cb);
                child.change_count = child.children.iter().map(|c| c.change_count).sum::<u32>()
                    + if matches!(st, DiffStatus::Added | DiffStatus::Removed | DiffStatus::Modified) { 1 } else { 0 };
                if !matches!(child.status, DiffStatus::Unchanged) || child.change_count > 0 {
                    parent.children.push(child);
                }
            }
        }
        (Some(Value::Array(la)), Some(Value::Array(lb))) => {
            let n = la.len().max(lb.len());
            for i in 0..n {
                let ca = la.get(i); let cb = lb.get(i);
                let st = diff_status(ca, cb);
                let mut path = parent.path.clone(); path.push(i.to_string());
                let mut child = make_diff_node(i.to_string(), path, st, ca, cb);
                fill_tree_diff_children(&mut child, ca, cb);
                child.change_count = child.children.iter().map(|c| c.change_count).sum::<u32>()
                    + if matches!(st, DiffStatus::Added | DiffStatus::Removed | DiffStatus::Modified) { 1 } else { 0 };
                if !matches!(child.status, DiffStatus::Unchanged) || child.change_count > 0 {
                    parent.children.push(child);
                }
            }
        }
        _ => {}
    }
}

fn diff_status(a: Option<&Value>, b: Option<&Value>) -> DiffStatus {
    match (a, b) {
        (Some(va), Some(vb)) if va == vb => DiffStatus::Unchanged,
        (Some(va), Some(vb)) => {
            // Container with same shape but inner changes → Partial; leaves
            // or shape changes → Modified.
            let same_container_shape = matches!(
                (va, vb),
                (Value::Object(_), Value::Object(_)) | (Value::Array(_), Value::Array(_)),
            );
            if same_container_shape { DiffStatus::Partial } else { DiffStatus::Modified }
        }
        (Some(_),  None)                  => DiffStatus::Removed,
        (None,     Some(_))               => DiffStatus::Added,
        (None,     None)                  => DiffStatus::Unchanged,
    }
}

fn value_kind_str(v: &Value) -> String { value_kind(v).as_str().to_string() }

// ── History helpers ─────────────────────────────────────────────────────

fn record_history(doc: &mut Doc, new_text: &str, can_coalesce: bool) {
    if doc.history.get(doc.history_pos).map(|s| s == new_text).unwrap_or(false) {
        return;
    }
    let now = Instant::now();
    let elapsed_ms = now.duration_since(doc.last_push).as_millis();
    let coalesce = can_coalesce
        && doc.coalesce_armed
        && elapsed_ms < COALESCE_WINDOW_MS
        && doc.history_pos + 1 == doc.history.len();
    if coalesce {
        doc.history[doc.history_pos] = new_text.to_string();
    } else {
        if doc.history_pos + 1 < doc.history.len() {
            doc.history.truncate(doc.history_pos + 1);
        }
        doc.history.push(new_text.to_string());
        if doc.history.len() > HISTORY_CAP {
            doc.history.remove(0);
        }
        doc.history_pos = doc.history.len() - 1;
    }
    doc.coalesce_armed = can_coalesce;
    doc.last_push      = now;
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let txt = "server.port=8080\nserver.host=localhost\n";
        let (lines, value, err) = parse_text(txt);
        assert!(err.is_none());
        assert_eq!(lines.as_ref().unwrap().len(), 2);
        let v = value.unwrap();
        assert_eq!(v.pointer("/server/port"), Some(&Value::String("8080".into())));
        assert_eq!(v.pointer("/server/host"), Some(&Value::String("localhost".into())));
    }

    #[test]
    fn lossless_roundtrip() {
        let txt = "# leading comment\n  server.port = 8080\n!bang comment\n\nserver.host=localhost\n";
        let (lines, _v, _e) = parse_text(txt);
        let emit = emit_lines(lines.as_ref().unwrap());
        assert_eq!(emit, txt);
    }

    #[test]
    fn set_existing_preserves_separator() {
        let txt = "server.port = 8080\nserver.host=localhost\n";
        let mut lines = parse_lines(txt);
        set_value_at_path(&mut lines, &["server".into(), "port".into()], "9090").unwrap();
        let out = emit_lines(&lines);
        assert!(out.contains("server.port = 9090\n"));
        assert!(out.contains("server.host=localhost\n"));
    }

    #[test]
    fn set_missing_appends() {
        let txt = "server.port=8080\n";
        let mut lines = parse_lines(txt);
        set_value_at_path(&mut lines, &["new".into(), "key".into()], "v").unwrap();
        let out = emit_lines(&lines);
        assert!(out.ends_with("new.key=v\n"));
    }

    #[test]
    fn remove_container_drops_subkeys() {
        let txt = "server.port=8080\nserver.host=localhost\nother=v\n";
        let mut lines = parse_lines(txt);
        remove_at_path(&mut lines, &["server".into()]).unwrap();
        let out = emit_lines(&lines);
        assert!(!out.contains("server."));
        assert!(out.contains("other=v"));
    }

    #[test]
    fn array_index_brackets() {
        let txt = "servers[0]=alpha\nservers[1]=beta\n";
        let (_lines, value, _err) = parse_text(txt);
        let v = value.unwrap();
        let arr = v.pointer("/servers").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], Value::String("alpha".into()));
    }

    #[test]
    fn continuation_lines_joined() {
        let txt = "long.value=abc\\\n  def\nother=x\n";
        let (lines, _v, _e) = parse_text(txt);
        let lines = lines.unwrap();
        // Three logical-ish lines: long.value (with continuation), other.
        let logical_count = lines.iter().filter(|l| matches!(l, RawLine::Logical { .. })).count();
        assert_eq!(logical_count, 2);
        if let RawLine::Logical { value, .. } = &lines[0] {
            assert_eq!(value, "abcdef");
        }
        // Re-emit preserves the continuation byte-for-byte.
        let emit = emit_lines(&lines);
        assert_eq!(emit, txt);
    }

    #[test]
    fn comments_preserved_on_rename() {
        let txt = "# database\ndb.url=postgres://localhost\n";
        let sites = vec![PropertiesRenameSite {
            field_path: vec!["db".into(), "url".into()],
            scope:      PropertiesRenameScope::Key,
        }];
        let out = apply_rename_in_text(txt, &sites, "db.url", "db.uri").unwrap();
        assert!(out.contains("# database\n"));
        assert!(out.contains("db.uri=postgres://localhost\n"));
    }

    #[test]
    fn rename_value_scope() {
        let txt = "alias=db.url\nother=v\n";
        let sites = vec![PropertiesRenameSite {
            field_path: vec!["alias".into()],
            scope:      PropertiesRenameScope::Value,
        }];
        let out = apply_rename_in_text(txt, &sites, "db.url", "db.uri").unwrap();
        assert!(out.contains("alias=db.uri\n"));
        assert!(out.contains("other=v\n"));
    }

    #[test]
    fn duplicate_keys_last_wins() {
        let txt = "k=a\nk=b\n";
        let (_lines, value, _err) = parse_text(txt);
        let v = value.unwrap();
        assert_eq!(v.pointer("/k"), Some(&Value::String("b".into())));
    }

    #[test]
    fn prefix_collision_via_value_sentinel() {
        // `foo=bar` + `foo.sub=baz` — same prefix is both a leaf and a
        // container. Projection keeps both: `foo` becomes a container
        // with a `$value` sentinel child for the leaf, plus the
        // regular `sub` child for the nested key.
        let txt = "foo=bar\nfoo.sub=baz\n";
        let (_lines, value, err) = parse_text(txt);
        assert!(err.is_none(), "collisions are legal .properties — no warning");
        let v = value.unwrap();
        assert_eq!(v.pointer("/foo/$value"), Some(&Value::String("bar".into())));
        assert_eq!(v.pointer("/foo/sub"),    Some(&Value::String("baz".into())));
    }

    #[test]
    fn flat_key_strips_value_sentinel() {
        // Editing `["foo", "$value"]` must write back to flat key `foo`,
        // not `foo.$value`.
        assert_eq!(path_to_flat_key(&["foo".into(), "$value".into()]), "foo");
        assert_eq!(path_to_flat_key(&["foo".into(), "$value".into(), "bar".into()]), "foo.bar");
    }
}
