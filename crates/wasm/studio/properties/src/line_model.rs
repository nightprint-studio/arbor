//! Line-oriented `.properties` model — the byte-preserving `RawLine`
//! view, the parser (continuation `\` joins + key/value escapes +
//! `\uXXXX`), the emitter, and the path-keyed mutation primitives.
//!
//! Lifted verbatim from the launcher's `properties_studio/mod.rs` (only
//! the error type changed: `crate::error::{AppError,Result}` →
//! `arbor_studio_core::prelude::{StudioError, StudioResult}`).
//!
//! FROZEN F4: lossless edit is `true`. `.properties` is intrinsically
//! line-oriented + sequential, so a per-line view is the natural rowan
//! analog — every comment, blank line, trailing whitespace and Unicode
//! escape survives an edit cycle.

use arbor_studio_core::prelude::{StudioError, StudioResult};

use crate::project::VALUE_SENTINEL;

/// A single physical block in the source buffer. Continuation lines
/// (`\` at EOL) get joined into one `Logical { value }` block where
/// `value` keeps the joined string post-unescape — but on emit we
/// re-splice the original `value_raw` so the line shape survives
/// byte-for-byte.
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

/// Split the source buffer into `RawLine`s. Backslash continuations
/// (when not escaped) extend a logical value across physical lines —
/// we join them into a single `Logical.value_raw` so the FE's get_value
/// returns the joined string, while emit() splits back identically.
pub fn parse_lines(text: &str) -> Vec<RawLine> {
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

        let key      = decode_unicode(&unescape_key(key_raw));
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

pub fn strip_eol(s: &str) -> (&str, &str) {
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

pub fn escape_key(s: &str) -> String {
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

// ── Emit ────────────────────────────────────────────────────────────────

pub fn emit_lines(lines: &[RawLine]) -> String {
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

// ── Public helpers consumed by the scanner ──────────────────────────────

/// Walk every logical key in the document and return `(key, value)`
/// pairs. Used by the project-wide broken-ref / usage scanners.
/// FROZEN F5: every key in `.properties` is a potential cross-ref target.
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

// ── Path mutation primitives ────────────────────────────────────────────

/// Convert a path `Vec<String>` into the flat `.properties` key. Segments
/// that parse as `usize` become Spring brackets on the previous segment.
/// The `$value` sentinel (used in the JSON projection to carry a leaf
/// value at a prefix that also has sub-keys) is stripped — when the FE
/// edits or removes `["foo", "$value"]`, the actual flat key in the
/// source is just `foo`.
pub fn path_to_flat_key(path: &[String]) -> String {
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

/// Convert a (possibly FE-tagged `{type,value}`) `serde_json::Value` to
/// the flat-string form `.properties` stores. `.properties` has no native
/// typing — every value collapses to its string representation.
pub fn primitive_to_string(v: &serde_json::Value) -> String {
    use serde_json::Value;
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
pub fn set_value_at_path(lines: &mut Vec<RawLine>, path: &[String], new_value: &str) -> StudioResult<()> {
    if path.is_empty() {
        return Err(StudioError::App("Cannot set value at the document root".into()));
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

pub fn remove_at_path(lines: &mut Vec<RawLine>, path: &[String]) -> StudioResult<()> {
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
        return Err(StudioError::App(format!("Key not found: `{target_key}`")));
    }
    Ok(())
}

pub fn insert_or_set(lines: &mut Vec<RawLine>, path: &[String], snippet: &str) -> StudioResult<()> {
    // If the key already exists, set; otherwise insert.
    set_value_at_path(lines, path, snippet)
}

pub fn next_array_index_under(lines: &[RawLine], path: &[String]) -> usize {
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

pub fn duplicate_at_path(lines: &mut Vec<RawLine>, path: &[String]) -> StudioResult<()> {
    let target_key = path_to_flat_key(path);
    let idx = lines.iter().position(|l| matches!(l, RawLine::Logical { key, .. } if key == &target_key))
        .ok_or_else(|| StudioError::App(format!("Key not found: `{target_key}`")))?;
    let mut cloned = lines[idx].clone();
    let new_key = format!("{target_key}_copy");
    if let RawLine::Logical { ref mut key, ref mut key_raw, .. } = cloned {
        *key     = new_key.clone();
        *key_raw = escape_key(&new_key);
    }
    lines.insert(idx + 1, cloned);
    Ok(())
}

pub fn move_at_path(lines: &mut [RawLine], path: &[String], delta: i32) -> StudioResult<()> {
    let target_key = path_to_flat_key(path);
    let idx = lines.iter().position(|l| matches!(l, RawLine::Logical { key, .. } if key == &target_key))
        .ok_or_else(|| StudioError::App(format!("Key not found: `{target_key}`")))?;
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

// ── F12 / F13 line-level helpers (key-scoped) ───────────────────────────

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
) -> StudioResult<String> {
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
            return Err(StudioError::App(format!(
                "Rename Key site not found: `{k}`",
            )));
        }
        if k != old_value {
            return Err(StudioError::App(format!(
                "Rename Key site `{k}` doesn't match old value `{old_value}`",
            )));
        }
    }
    for k in &val_flats {
        let ok = lines.iter().any(|l| matches!(l, RawLine::Logical { key, value, .. }
            if key == k && value == old_value));
        if !ok {
            return Err(StudioError::App(format!(
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

impl PropertiesSetValue {
    pub fn to_string_value(&self) -> String {
        match self {
            PropertiesSetValue::String(s) => s.clone(),
            PropertiesSetValue::Empty     => String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PropertiesBulkOp {
    Set(PropertiesSetValue),
    Delete,
}

pub fn apply_bulk_edits_in_place(
    lines: &mut Vec<RawLine>,
    ops:   &[(Vec<String>, PropertiesBulkOp)],
) -> StudioResult<()> {
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
            return Err(StudioError::App("Cannot delete the document root".into()));
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
) -> StudioResult<String> {
    let mut lines = parse_lines(input);
    apply_bulk_edits_in_place(&mut lines, ops)?;
    Ok(emit_lines(&lines))
}
