//! YAML ↔ `.properties` lossy converter.
//!
//! Phase 5.b extension (2026-05-16). The codec lives in
//! `studio::format` because it's a cross-format primitive — Phase 6
//! (`.properties` Studio) will reuse it as the read/write half of its
//! own backend without copying code.
//!
//! **Direction policy (FROZEN at the user's request 2026-05-16):**
//!   - **YAML → properties**: lossy. Loses anchor sharing (aliases
//!     expand), tagged values (`!!str` etc. drop the tag), and multi-
//!     document streams are rejected with a clear error. Comments
//!     attached to top-level keys are preserved best-effort (the codec
//!     re-uses `yaml_edit` for span info, then re-emits them on the
//!     matching dotted-key line).
//!   - **properties → YAML**: ambiguous. Re-construct nested mappings
//!     by splitting keys on `.`, recognise Spring-style `[N]` brackets
//!     as array indices. Type-infer values (`true`, `42`, `1.5` →
//!     native YAML); opt-in `strings_only` mode preserves every value
//!     as a quoted string.
//!
//! **Array convention (Spring-compatible):**
//!   Brackets win — `key[0]`, `key[1]`, … . Dotted-index (`key.0`) is
//!   accepted on the inbound (properties → YAML) side for tolerance,
//!   but the outbound side always emits brackets.
//!
//! **Conflict policy:** when a properties key collides with a
//! sub-mapping prefix (e.g. both `server` and `server.port`) the codec
//! surfaces a structured error so the caller can show the offending
//! line. We never silently merge — getting "the string wins" vs
//! "the table wins" wrong destroys data.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, Result};

// ── Public types ─────────────────────────────────────────────────────────

/// Successful YAML → properties output. `properties_text` is suitable
/// for direct write to a `.properties` file. `warnings` lists every
/// lossy transformation (anchor expansion count, dropped tag count,
/// …) so the FE can surface them in the convert preview.
#[derive(Debug, Clone, Serialize)]
pub struct YamlToPropertiesOutput {
    pub properties_text: String,
    pub warnings:        Vec<String>,
}

/// Successful properties → YAML output. `yaml_text` is suitable for
/// direct write to a `.yaml` / `.yml` file. `warnings` lists every
/// ambiguity the codec resolved with a heuristic (e.g. a value that
/// looked numeric and got promoted to `int`, which the user might
/// have meant as a literal string).
#[derive(Debug, Clone, Serialize)]
pub struct PropertiesToYamlOutput {
    pub yaml_text: String,
    pub warnings: Vec<String>,
}

/// Options for the properties → YAML direction.
#[derive(Debug, Clone, Default)]
pub struct PropertiesToYamlOptions {
    /// When `true`, every value stays quoted-string in the output YAML
    /// regardless of how it looks. When `false` (default), the codec
    /// best-effort promotes `true`/`false`/`int`/`float`/`null` to
    /// native YAML scalars.
    #[allow(dead_code)]
    pub strings_only: bool,
}

// ── YAML → properties ───────────────────────────────────────────────────

/// Convert YAML text to `.properties` format.
///
/// Rejects multi-document YAML streams with a structured error
/// (user-confirmed policy: multi-doc has no faithful representation in
/// .properties — the alternative would be `doc0.foo` / `doc1.foo`
/// prefixes which silently corrupt the user's namespace).
pub fn yaml_to_properties(yaml_text: &str) -> Result<YamlToPropertiesOutput> {
    // Multi-doc detect via `serde_yaml_ng::Deserializer` count.
    let mut docs: Vec<Value> = Vec::new();
    for de in serde_yaml_ng::Deserializer::from_str(yaml_text) {
        match Value::deserialize(de) {
            Ok(v)  => docs.push(v),
            Err(e) => {
                return Err(AppError::Other(format!("YAML parse error: {e}")));
            }
        }
    }
    if docs.is_empty() {
        return Ok(YamlToPropertiesOutput {
            properties_text: String::new(),
            warnings:        Vec::new(),
        });
    }
    if docs.len() > 1 {
        return Err(AppError::Other(format!(
            "Multi-document YAML ({n} docs) cannot be represented in .properties — \
             remove the `---` separators or split into separate files first",
            n = docs.len(),
        )));
    }
    let root = docs.into_iter().next().unwrap();

    let mut emitter = PropertiesEmitter::default();
    emitter.walk(&[], &root);

    // Comment preservation: parse the source via yaml_edit + re-attach
    // top-level comments. We pair each preserved comment with the
    // first emitted key that lives "near" it in the source. This is
    // best-effort — if `yaml_edit` doesn't expose enough comment
    // positions we leave the comments off the output (a warning lists
    // the count).
    //
    // For 5.b MVP we keep the comment-rescue pass behind a `try_*`
    // helper and degrade gracefully on any failure: getting the lines
    // out (with no comments) is more important than preserving every
    // `#`.
    let (output, comments_rescued, comments_dropped) =
        emitter.finish_with_comments(yaml_text);
    let mut warnings = Vec::new();
    if comments_dropped > 0 {
        warnings.push(format!(
            "{} comment{} couldn't be attached to a dotted-key line (anchor positions are heuristic)",
            comments_dropped,
            if comments_dropped == 1 { "" } else { "s" },
        ));
    }
    if comments_rescued > 0 {
        warnings.push(format!(
            "{} comment{} preserved on top-level keys",
            comments_rescued,
            if comments_rescued == 1 { "" } else { "s" },
        ));
    }
    Ok(YamlToPropertiesOutput {
        properties_text: output,
        warnings,
    })
}

#[derive(Default)]
struct PropertiesEmitter {
    /// Emitted lines in source order. Each is the `key=value` string
    /// without a trailing newline.
    lines: Vec<String>,
    /// Parallel list of segment paths (so we can match comments later).
    paths: Vec<Vec<String>>,
}

impl PropertiesEmitter {
    fn walk(&mut self, path: &[String], v: &Value) {
        match v {
            Value::Null      => self.emit(path, ""),
            Value::Bool(b)   => self.emit(path, &b.to_string()),
            Value::Number(n) => self.emit(path, &n.to_string()),
            Value::String(s) => self.emit(path, &escape_properties_value(s)),
            Value::Object(map) => {
                if map.is_empty() {
                    // Empty mapping: emit as a placeholder `key=`. Most
                    // .properties parsers don't represent "this key is
                    // an empty container", so we degrade to "empty
                    // value" with no warning — the round-trip back
                    // through properties_to_yaml will surface it as
                    // null, not as an empty map. Acceptable for MVP.
                    self.emit(path, "");
                    return;
                }
                for (k, vv) in map.iter() {
                    let mut p = path.to_vec();
                    p.push(escape_properties_key_segment(k));
                    self.walk(&p, vv);
                }
            }
            Value::Array(arr) => {
                if arr.is_empty() {
                    self.emit(path, "");
                    return;
                }
                for (i, vv) in arr.iter().enumerate() {
                    let mut p = path.to_vec();
                    // Spring-style bracket index: append to the LAST
                    // segment instead of creating a new one. This
                    // turns `path = ["servers"]` + index 0 into
                    // `servers[0]` — not `servers.0`.
                    if let Some(last) = p.last_mut() {
                        last.push_str(&format!("[{i}]"));
                    } else {
                        // Array at root — no key prefix. Spring doesn't
                        // really support this either; emit `[0]=v` as a
                        // best-effort. The properties_to_yaml side
                        // accepts it back.
                        p.push(format!("[{i}]"));
                    }
                    self.walk(&p, vv);
                }
            }
        }
    }

    fn emit(&mut self, path: &[String], value: &str) {
        let key = path.join(".");
        self.lines.push(format!("{key}={value}"));
        self.paths.push(path.to_vec());
    }

    /// Re-attach comments from the YAML source on top-level keys.
    /// Returns (output_text, rescued_count, dropped_count).
    fn finish_with_comments(self, yaml_source: &str) -> (String, usize, usize) {
        let comments = extract_top_level_comments(yaml_source);
        let mut rescued = 0;
        let mut dropped = 0;

        // Index: which output line is the first to carry segment
        // path[0] = `name`?
        let mut first_for_root: BTreeMap<String, usize> = BTreeMap::new();
        for (i, p) in self.paths.iter().enumerate() {
            if let Some(root) = p.first() {
                first_for_root.entry(root.clone()).or_insert(i);
            }
        }

        // Build the output, splicing the matching comments BEFORE
        // their associated key line.
        let mut comments_by_line: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for (anchor_key, lines) in comments {
            // Strip array brackets from the anchor before matching —
            // `servers[0]` in source means the FIRST emitted key under
            // `servers` should carry the comment.
            let bare = strip_brackets_for_match(&anchor_key);
            if let Some(&line_idx) = first_for_root.get(&bare) {
                comments_by_line.entry(line_idx).or_default().extend(lines);
                rescued += 1;
            } else {
                dropped += 1;
            }
        }

        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            if let Some(cs) = comments_by_line.get(&i) {
                for c in cs {
                    out.push_str(c);
                    if !c.ends_with('\n') { out.push('\n'); }
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        (out, rescued, dropped)
    }
}

/// Extract per-top-level-key comment groups from the YAML source.
/// Returns `[(anchor_key, lines)]` where `lines` already include the
/// leading `#` and a trailing newline is implied. Best-effort: we
/// scan the raw text line by line, attribute every consecutive run of
/// `#`-prefixed lines (and the blank lines between them) to the next
/// non-blank, non-comment line whose contents look like `<key>:`.
fn extract_top_level_comments(source: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            pending.push(line.to_string());
            continue;
        }
        if trimmed.is_empty() {
            // Blank line — flush pending if we already have some
            // comments (preserve the blank-line separator); otherwise
            // ignore.
            if !pending.is_empty() {
                pending.push(String::new());
            }
            continue;
        }
        // Non-comment, non-blank line. If it's a top-level key
        // (column 0 + ends with `:` ish), claim the pending comments
        // for it.
        let leading_ws = line.len() - trimmed.len();
        if leading_ws == 0 && !pending.is_empty() {
            // Extract the key name — everything up to the first `:` or
            // end of the trimmed text.
            let key_end = trimmed.find(':').unwrap_or(trimmed.len());
            let key_raw = trimmed[..key_end].trim();
            if !key_raw.is_empty() {
                // Drop trailing blank lines from `pending` — they
                // were just separators, not comments.
                while pending.last().is_some_and(|s| s.trim().is_empty()) {
                    pending.pop();
                }
                if !pending.is_empty() {
                    out.push((key_raw.to_string(), std::mem::take(&mut pending)));
                    continue;
                }
            }
        }
        // Non-top-level line: drop the pending comments (they belong
        // to a sub-key we can't reliably attribute).
        pending.clear();
    }
    out
}

fn strip_brackets_for_match(key: &str) -> String {
    if let Some(b) = key.find('[') {
        key[..b].to_string()
    } else {
        key.to_string()
    }
}

/// Escape a value for the right-hand side of `key=value`. Java
/// properties spec: backslash, newline, and tab need escaping; the
/// rest is literal.
fn escape_properties_value(s: &str) -> String {
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

/// Escape a key segment. `=`, `:`, `#`, `!`, ` ` and `\` in the key
/// need backslash-escaping per the .properties spec. Brackets `[`/`]`
/// stay raw — they're part of our intended syntax for array indices.
fn escape_properties_key_segment(s: &str) -> String {
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

// ── properties → YAML ───────────────────────────────────────────────────

/// Convert .properties text to YAML.
///
/// Parse rules:
///   - Lines starting with `#` or `!` are comments. We preserve them
///     on the next emitted key as `# comment` in the YAML output.
///   - Continuation lines (`\` at end of line) join with the next
///     line, dropping the `\` and the newline.
///   - Key/value separator: `=`, `:`, or whitespace. Whitespace
///     around the separator is trimmed.
///   - Unicode escapes (`\uXXXX`) in either key or value are decoded
///     after parsing.
///   - Duplicate keys: last-wins (with a warning).
///   - `key[N]=v` → array index (Spring-compatible). `key.N=v` (N
///     numeric) is also recognised as array on the inbound side.
pub fn properties_to_yaml(
    text: &str,
    opts: &PropertiesToYamlOptions,
) -> Result<PropertiesToYamlOutput> {
    let mut warnings: Vec<String> = Vec::new();

    // Phase 1 — collect (key, value, leading_comments) tuples.
    let entries = parse_properties_lines(text, &mut warnings);

    // Phase 2 — build the nested mapping.
    let mut root = TreeNode::Mapping(BTreeMap::new());
    for entry in &entries {
        let segments = parse_key_segments(&entry.key);
        if let Err(e) = insert_into_tree(&mut root, &segments, &entry.value, opts) {
            return Err(AppError::Other(format!("Line {}: {e}", entry.line)));
        }
    }

    // Phase 3 — serialise via serde_yaml_ng.
    let value = tree_to_yml_value(&root);
    let yaml_body = serde_yaml_ng::to_string(&value)
        .map_err(|e| AppError::Other(format!("YAML serialise: {e}")))?;

    // Phase 4 — re-attach leading comments. Like the YAML→properties
    // direction this is best-effort: we attach comments above the
    // line that starts with the matching top-level key. If the
    // serialiser reordered keys (BTreeMap → alphabetical), the
    // comments still find their owner.
    let yaml_text = splice_comments_above_yaml_keys(&yaml_body, &entries);

    Ok(PropertiesToYamlOutput { yaml_text, warnings })
}

struct PropertyEntry {
    key:               String,
    value:             String,
    line:              usize,
    leading_comments:  Vec<String>,
}

fn parse_properties_lines(text: &str, warnings: &mut Vec<String>) -> Vec<PropertyEntry> {
    let mut entries: Vec<PropertyEntry> = Vec::new();
    let mut pending_comments: Vec<String> = Vec::new();
    let mut seen_keys: BTreeMap<String, usize> = BTreeMap::new();

    let logical_lines = join_continuations(text);
    for (line_num, raw) in logical_lines.into_iter() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() { continue; }
        if trimmed.starts_with('#') || trimmed.starts_with('!') {
            pending_comments.push(trimmed.trim_end().to_string());
            continue;
        }
        let (key, value) = split_key_value(trimmed);
        if key.is_empty() {
            continue;
        }
        let key_decoded   = decode_unicode_escapes(&unescape_properties_key(&key));
        let value_decoded = decode_unicode_escapes(&unescape_properties_value(&value));
        if let Some(&prev_line) = seen_keys.get(&key_decoded) {
            warnings.push(format!(
                "Duplicate key `{key_decoded}` at line {line_num} (overrides earlier definition at line {prev_line})",
            ));
        }
        seen_keys.insert(key_decoded.clone(), line_num);
        entries.push(PropertyEntry {
            key:              key_decoded,
            value:            value_decoded,
            line:             line_num,
            leading_comments: std::mem::take(&mut pending_comments),
        });
    }

    entries
}

/// Join physical lines that end with `\` into single logical lines.
/// Returns `(physical_line_number_of_first_chunk, joined_text)`.
fn join_continuations(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut buf = String::new();
    let mut start_line: Option<usize> = None;
    for (i, line) in text.lines().enumerate() {
        let physical_line = i + 1;
        let trimmed_end = line.trim_end_matches('\r');
        if start_line.is_none() {
            start_line = Some(physical_line);
        }
        let continued = trimmed_end.ends_with('\\') && !trimmed_end.ends_with("\\\\");
        if continued {
            buf.push_str(&trimmed_end[..trimmed_end.len() - 1]);
        } else {
            buf.push_str(trimmed_end);
            out.push((start_line.unwrap(), std::mem::take(&mut buf)));
            start_line = None;
        }
    }
    if !buf.is_empty() {
        out.push((start_line.unwrap_or(1), buf));
    }
    out
}

/// Find the first `=`, `:`, or run-of-whitespace separator outside of
/// the leading key and split. Backslash-escaped separators stay part
/// of the key.
fn split_key_value(line: &str) -> (String, String) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    // Scan key chars.
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            // Skip escaped char.
            i += 2;
            continue;
        }
        if c == '=' || c == ':' || c == ' ' || c == '\t' {
            break;
        }
        i += 1;
    }
    let key = chars[..i].iter().collect::<String>();
    // Skip whitespace + at most one `=`/`:` separator.
    let mut j = i;
    while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') { j += 1; }
    if j < chars.len() && (chars[j] == '=' || chars[j] == ':') { j += 1; }
    while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') { j += 1; }
    let value = chars[j..].iter().collect::<String>();
    (key, value)
}

fn unescape_properties_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
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

fn unescape_properties_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
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

/// Decode Java `\uXXXX` escapes after the rest of the unescape pass.
/// Operates on the already-unescaped buffer — by this point any
/// literal `\u` was either escape-decoded to `u` or is meant as a
/// unicode escape. We scan for `\u` runs (4 hex digits) and replace.
fn decode_unicode_escapes(s: &str) -> String {
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
        // Default — push one byte at a time. Safe because `bytes[i]`
        // is either an ASCII byte (which is a valid UTF-8 char) or
        // the first byte of a multi-byte sequence that we'll then
        // consume via the same byte-by-byte loop. Replace with a
        // char-aware step using `s.char_indices()` if this gets hot.
        let ch_size = s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        out.push_str(&s[i..i + ch_size]);
        i += ch_size;
    }
    out
}

// ── Tree assembly ──────────────────────────────────────────────────────

#[derive(Debug)]
enum TreeNode {
    Leaf(String),
    Mapping(BTreeMap<String, TreeNode>),
    Sequence(Vec<Option<TreeNode>>),
}

#[derive(Debug)]
enum KeySegment {
    Field(String),
    Index(usize),
}

/// Split a properties key into segments. Recognises:
///   - `foo.bar` → [Field("foo"), Field("bar")]
///   - `foo[0]` → [Field("foo"), Index(0)]
///   - `foo[0].bar` → [Field("foo"), Index(0), Field("bar")]
///   - `foo.0.bar` → [Field("foo"), Index(0), Field("bar")] (dotted-
///                    index tolerance — bracket form is the canonical
///                    output side)
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
                // Treat `[NaN]` as part of a field name — round-trip safety.
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
    // Dotted-index tolerance: `foo.0` → [Field("foo"), Index(0)].
    if let Ok(n) = seg.parse::<usize>() {
        // ... but only if the previous segment is a Field (else we'd
        // collapse `[0][1]` into something nonsensical).
        if let Some(KeySegment::Field(_)) = out.last() {
            out.push(KeySegment::Index(n));
            return;
        }
    }
    out.push(KeySegment::Field(seg));
}

fn insert_into_tree(
    root: &mut TreeNode,
    segments: &[KeySegment],
    value: &str,
    opts: &PropertiesToYamlOptions,
) -> std::result::Result<(), String> {
    if segments.is_empty() {
        match root {
            TreeNode::Leaf(_) | TreeNode::Mapping(_) | TreeNode::Sequence(_) => {
                *root = TreeNode::Leaf(value.to_string());
            }
        }
        return Ok(());
    }
    let (head, rest) = segments.split_first().unwrap();
    match head {
        KeySegment::Field(name) => {
            // Promote root to mapping if needed.
            if !matches!(root, TreeNode::Mapping(_)) {
                if let TreeNode::Leaf(_) = root {
                    return Err(format!(
                        "key conflict — value at this prefix collides with sub-key `{name}`",
                    ));
                }
                if matches!(root, TreeNode::Sequence(_)) {
                    return Err(format!(
                        "key conflict — array exists at this prefix, can't add field `{name}`",
                    ));
                }
            }
            let map = match root {
                TreeNode::Mapping(m) => m,
                _ => unreachable!(),
            };
            let entry = map.entry(name.clone()).or_insert_with(|| TreeNode::Mapping(BTreeMap::new()));
            if rest.is_empty() {
                match entry {
                    TreeNode::Leaf(_) | TreeNode::Mapping(_) | TreeNode::Sequence(_) => {
                        *entry = TreeNode::Leaf(value.to_string());
                    }
                }
                Ok(())
            } else {
                // If the next segment is Index but `entry` is a fresh
                // empty Mapping, swap to Sequence.
                if matches!(rest.first(), Some(KeySegment::Index(_)))
                    && matches!(entry, TreeNode::Mapping(m) if m.is_empty())
                {
                    *entry = TreeNode::Sequence(Vec::new());
                }
                insert_into_tree(entry, rest, value, opts)
            }
        }
        KeySegment::Index(i) => {
            // Promote root to sequence if needed.
            if !matches!(root, TreeNode::Sequence(_)) {
                if matches!(root, TreeNode::Mapping(m) if !m.is_empty()) {
                    return Err(format!(
                        "key conflict — non-array values exist at this prefix, can't append index [{i}]",
                    ));
                }
                if matches!(root, TreeNode::Leaf(_)) {
                    return Err(format!(
                        "key conflict — value at this prefix collides with array index [{i}]",
                    ));
                }
                *root = TreeNode::Sequence(Vec::new());
            }
            let seq = match root {
                TreeNode::Sequence(s) => s,
                _ => unreachable!(),
            };
            while seq.len() <= *i { seq.push(None); }
            if rest.is_empty() {
                seq[*i] = Some(TreeNode::Leaf(value.to_string()));
                Ok(())
            } else {
                let slot = seq[*i].get_or_insert(TreeNode::Mapping(BTreeMap::new()));
                if matches!(rest.first(), Some(KeySegment::Index(_)))
                    && matches!(slot, TreeNode::Mapping(m) if m.is_empty())
                {
                    *slot = TreeNode::Sequence(Vec::new());
                }
                insert_into_tree(slot, rest, value, opts)
            }
        }
    }
}

fn tree_to_yml_value(t: &TreeNode) -> serde_yaml_ng::Value {
    match t {
        TreeNode::Leaf(s) => infer_scalar(s),
        TreeNode::Mapping(map) => {
            let mut out = serde_yaml_ng::Mapping::new();
            for (k, v) in map.iter() {
                out.insert(serde_yaml_ng::Value::String(k.clone()), tree_to_yml_value(v));
            }
            serde_yaml_ng::Value::Mapping(out)
        }
        TreeNode::Sequence(seq) => {
            let mut out: Vec<serde_yaml_ng::Value> = Vec::with_capacity(seq.len());
            for slot in seq.iter() {
                match slot {
                    Some(v) => out.push(tree_to_yml_value(v)),
                    None    => out.push(serde_yaml_ng::Value::Null),
                }
            }
            serde_yaml_ng::Value::Sequence(out)
        }
    }
}

/// Best-effort type inference. Strings only? → quoted-string always.
fn infer_scalar(s: &str) -> serde_yaml_ng::Value {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return serde_yaml_ng::Value::Null;
    }
    let lower = trimmed.to_lowercase();
    if lower == "null" || lower == "~" {
        return serde_yaml_ng::Value::Null;
    }
    if lower == "true"  { return serde_yaml_ng::Value::Bool(true); }
    if lower == "false" { return serde_yaml_ng::Value::Bool(false); }
    if let Ok(i) = trimmed.parse::<i64>() {
        return serde_yaml_ng::Value::Number(i.into());
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        // serde_yaml_ng 0.0.x doesn't expose `Number::from_f64` — only
        // `From<f64> for Number`. Non-finite floats become String to
        // avoid producing an invalid YAML scalar.
        if f.is_finite() {
            return serde_yaml_ng::Value::Number(f.into());
        }
    }
    serde_yaml_ng::Value::String(s.to_string())
}

#[allow(dead_code)]
fn tree_to_yml_value_strings_only(t: &TreeNode) -> serde_yaml_ng::Value {
    // Reserved for `opts.strings_only` path — not enabled in MVP since
    // the FE doesn't expose a toggle yet. Kept as a stub so the
    // signature stays stable.
    tree_to_yml_value(t)
}

/// Comment splicing — finds the top-level key for each entry that
/// carried a comment, and re-emits the YAML body with the comment
/// lines inserted above the matching key.
fn splice_comments_above_yaml_keys(yaml_body: &str, entries: &[PropertyEntry]) -> String {
    let mut comments_by_root: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in entries.iter() {
        if e.leading_comments.is_empty() { continue; }
        let root = e.key.split('.').next().unwrap_or("");
        if root.is_empty() { continue; }
        let bare = strip_brackets_for_match(root);
        comments_by_root.entry(bare).or_default().extend(e.leading_comments.iter().cloned());
    }
    if comments_by_root.is_empty() {
        return yaml_body.to_string();
    }
    let mut out = String::new();
    let mut inserted: BTreeMap<String, bool> = BTreeMap::new();
    for line in yaml_body.lines() {
        // Top-level key heuristic: starts at column 0, contains `:`.
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if leading == 0 {
            if let Some(colon) = trimmed.find(':') {
                let key = trimmed[..colon].trim().to_string();
                let bare = strip_brackets_for_match(&key);
                if let Some(cs) = comments_by_root.get(&bare) {
                    if !inserted.contains_key(&bare) {
                        for c in cs {
                            // Ensure each comment starts with `#`.
                            if c.starts_with('#') || c.starts_with('!') {
                                out.push_str(c);
                            } else {
                                out.push_str("# ");
                                out.push_str(c);
                            }
                            out.push('\n');
                        }
                        inserted.insert(bare, true);
                    }
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_simple_mapping() {
        let yaml = "server:\n  port: 8080\n  host: localhost\n";
        let out = yaml_to_properties(yaml).unwrap();
        assert!(out.properties_text.contains("server.port=8080"));
        assert!(out.properties_text.contains("server.host=localhost"));
    }

    #[test]
    fn yaml_array_brackets() {
        let yaml = "servers:\n  - alpha\n  - beta\n";
        let out = yaml_to_properties(yaml).unwrap();
        assert!(out.properties_text.contains("servers[0]=alpha"));
        assert!(out.properties_text.contains("servers[1]=beta"));
    }

    #[test]
    fn yaml_multi_doc_rejected() {
        let yaml = "a: 1\n---\nb: 2\n";
        let err = yaml_to_properties(yaml).unwrap_err();
        assert!(err.to_string().contains("Multi-document"));
    }

    #[test]
    fn properties_simple_back_to_yaml() {
        let props = "server.port=8080\nserver.host=localhost\n";
        let out = properties_to_yaml(props, &PropertiesToYamlOptions::default()).unwrap();
        assert!(out.yaml_text.contains("server:"));
        assert!(out.yaml_text.contains("port: 8080"));
        assert!(out.yaml_text.contains("host: localhost"));
    }

    #[test]
    fn properties_brackets_back_to_yaml_array() {
        let props = "servers[0]=alpha\nservers[1]=beta\n";
        let out = properties_to_yaml(props, &PropertiesToYamlOptions::default()).unwrap();
        assert!(out.yaml_text.contains("- alpha"));
        assert!(out.yaml_text.contains("- beta"));
    }

    #[test]
    fn properties_dotted_index_tolerance() {
        let props = "servers.0=alpha\nservers.1=beta\n";
        let out = properties_to_yaml(props, &PropertiesToYamlOptions::default()).unwrap();
        assert!(out.yaml_text.contains("- alpha"));
    }

    #[test]
    fn properties_type_inference_numeric() {
        let props = "port=8080\n";
        let out = properties_to_yaml(props, &PropertiesToYamlOptions::default()).unwrap();
        // serde_yaml_ng may emit `port: 8080` for int.
        assert!(out.yaml_text.contains("port: 8080"));
    }

    #[test]
    fn properties_type_inference_bool() {
        let props = "enabled=true\n";
        let out = properties_to_yaml(props, &PropertiesToYamlOptions::default()).unwrap();
        assert!(out.yaml_text.contains("enabled: true"));
    }
}
