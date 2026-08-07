//! A `Cargo.toml` read into tables and keys, every one of them carrying its byte span.
//!
//! ## Why this is hand-rolled
//!
//! Both consumers of this module need something a serde front-end structurally cannot give:
//!
//! - **validation** needs the span of every key and every array item, so a diagnostic lands under
//!   the offending text rather than on the file;
//! - **validation** also needs the keys nobody expected — the whole point is flagging a key that
//!   is *not* in the schema, which a typed deserialization discards before you can see it;
//! - **completion** needs to work on a file that does not parse, because a caret mid-key is by
//!   definition mid-error.
//!
//! `toml::Value` throws the spans away, and `toml::Spanned` requires a typed model of a manifest
//! whose optional-key surface is enormous and would still drop the unknown ones. So the reader is
//! a small scanner, in the same spirit as `bennu-project`'s pom and Cargo readers.
//!
//! ## What it does and does not handle
//!
//! Handled: comments (including a `#` inside a string), `[table]` and `[[array-of-tables]]`
//! headers, quoted and bare keys, dotted keys (kept verbatim — see [`Entry::key`]), values that
//! span lines because a bracket or brace is open, quoted items inside arrays, and the keys of an
//! inline table.
//!
//! Not handled: multi-line basic/literal strings (`"""` / `'''`) are skipped as opaque, and
//! nothing is *interpreted* — no integers, no dates, no escape processing. Every one of those is
//! a shape Cargo manifests do not use for the keys anyone reads, and the tolerance rule means an
//! unhandled shape yields *no entry* rather than a wrong one.
//!
//! The invariant across the whole module: **an unexpected shape produces less information, never
//! an error and never a wrong span.**

/// The implicit root table — the keys written before the first `[header]`.
///
/// A real Cargo.toml has none (everything is under `[package]` or another table), but a file being
/// created has plenty, and the completion path has to name the table it is in either way.
pub const ROOT_TABLE: &str = "";

/// One `[table]` / `[[array-of-tables]]` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSpan {
    /// The dotted path as written, with quotes stripped from each segment:
    /// `dependencies`, `workspace.package`, `target.cfg(unix).dependencies`.
    pub path: String,
    /// `true` for `[[bin]]` and friends.
    pub array: bool,
    /// Byte offset of the opening `[`.
    pub start: usize,
    /// Byte offset just past the closing `]`.
    pub end: usize,
    /// 1-based line of the header.
    pub line: u32,
}

/// A quoted string inside a value, with its own span — so a diagnostic about one member of
/// `members = [...]` underlines *that member*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The string's content, unquoted.
    pub text: String,
    /// Byte offset of the opening quote.
    pub start: usize,
    /// Byte offset just past the closing quote.
    pub end: usize,
}

/// One `key = value` pair of an inline table (`{ version = "1", optional = true }`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineKey {
    /// The key, unquoted.
    pub key: String,
    /// The raw value text, trimmed.
    pub value: String,
    /// Byte offset of the first character of the key.
    pub start: usize,
    /// Byte offset just past the key.
    pub end: usize,
    /// Byte offset of the first character of the value — a diagnostic about the value has to
    /// underline the value, not the key that introduced it.
    pub value_start: usize,
    /// Byte offset just past the value.
    pub value_end: usize,
}

/// One `key = value` assignment, and which table it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The dotted path of the enclosing table, [`ROOT_TABLE`] before the first header.
    pub table: String,
    /// The key as written, unquoted, dots kept.
    ///
    /// A dotted key is **not** split: `edition.workspace = true` is the key
    /// `edition.workspace`, because that is what has to be matched against the schema and what
    /// [`Entry::base_key`] then reduces for the callers that want `edition`. Splitting it into a
    /// synthetic `[package.edition]` table would invent a table the file does not have.
    pub key: String,
    /// The raw value text: comment stripped, trimmed, and — for a value that spanned lines —
    /// exactly the source between its first and last byte, newlines included.
    pub value: String,
    /// Byte offset of the first character of the key.
    pub key_start: usize,
    /// Byte offset just past the key (before any whitespace and the `=`).
    pub key_end: usize,
    /// Byte offset of the first character of the value. Equal to `value_end` for `key =` with
    /// nothing after it — which is a real state while typing.
    pub value_start: usize,
    /// Byte offset just past the value.
    pub value_end: usize,
    /// 1-based line the KEY is on.
    pub line: u32,
    /// The quoted strings in the value, with their spans. Empty when there are none.
    pub items: Vec<Item>,
}

impl Entry {
    /// The key with any dotted suffix removed: `edition.workspace` → `edition`.
    ///
    /// The suffix is Cargo's workspace-inheritance marker, and every caller that asks "does this
    /// manifest set an edition" means the base key.
    pub fn base_key(&self) -> &str {
        self.key.split_once('.').map(|(head, _)| head).unwrap_or(&self.key)
    }

    /// The dotted suffix, when the key has one: `edition.workspace` → `Some("workspace")`.
    pub fn key_suffix(&self) -> Option<&str> {
        self.key.split_once('.').map(|(_, tail)| tail)
    }

    /// The value as a plain string, or `None` when it is not a quoted string.
    ///
    /// Deliberately strict: `edition.workspace = true` must not read as the edition `"true"`, and
    /// a bare word is not a TOML string.
    pub fn str_value(&self) -> Option<&str> {
        let v = self.value.as_str();
        for q in ['"', '\''] {
            if let Some(inner) = v.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
                if !inner.contains(q) {
                    return Some(inner);
                }
            }
        }
        None
    }

    /// The value as a bool, or `None`.
    pub fn bool_value(&self) -> Option<bool> {
        match self.value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    /// Whether the value is an inline table (`{ … }`).
    pub fn is_inline_table(&self) -> bool {
        self.value.starts_with('{')
    }

    /// Whether the value is an array (`[ … ]`).
    pub fn is_array(&self) -> bool {
        self.value.starts_with('[')
    }

    /// The keys of the value when it is an inline table, in source order.
    ///
    /// Only the table's OWN keys (depth 1): a nested `{ a = { b = 1 } }` yields `a`, not `b`.
    pub fn inline_keys(&self) -> Vec<InlineKey> {
        if !self.is_inline_table() {
            return Vec::new();
        }
        inline_keys_of(&self.value, self.value_start)
    }
}

/// A `Cargo.toml` read into its headers and assignments, in source order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    pub tables: Vec<TableSpan>,
    pub entries: Vec<Entry>,
}

impl Manifest {
    /// Read `text`. Never fails — see the module doc.
    pub fn parse(text: &str) -> Manifest {
        scan(text)
    }

    /// The header of `path`, when the file declares that table.
    pub fn table(&self, path: &str) -> Option<&TableSpan> {
        self.tables.iter().find(|t| t.path == path)
    }

    /// Whether the file declares `path` as a table.
    pub fn has_table(&self, path: &str) -> bool {
        self.table(path).is_some()
    }

    /// Every entry of `path`, in source order.
    ///
    /// An array-of-tables (`[[bin]]`) has one header per element and they all share a path, so
    /// this yields the keys of *all* of them — which is what "every `[[bin]]` name" wants. Use
    /// [`Manifest::array_elements`] when the elements have to stay apart.
    pub fn entries_in<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a Entry> + 'a {
        self.entries.iter().filter(move |e| e.table == path)
    }

    /// The entry `table.key`, when it is there.
    pub fn get(&self, table: &str, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.table == table && e.key == key)
    }

    /// The entry whose [`Entry::base_key`] is `key` — so `edition` finds
    /// `edition.workspace = true` too.
    pub fn get_base(&self, table: &str, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.table == table && e.base_key() == key)
    }

    /// `table.key` as a string, when it is one.
    pub fn str_of(&self, table: &str, key: &str) -> Option<&str> {
        self.get(table, key).and_then(Entry::str_value)
    }

    /// `table.key` as a bool, when it is one.
    pub fn bool_of(&self, table: &str, key: &str) -> Option<bool> {
        self.get(table, key).and_then(Entry::bool_value)
    }

    /// The quoted items of the array `table.key`, with their spans.
    pub fn items_of(&self, table: &str, key: &str) -> &[Item] {
        self.get(table, key).map(|e| e.items.as_slice()).unwrap_or(&[])
    }

    /// The elements of the array-of-tables `path`, each as the entries of one element.
    ///
    /// What `[[bin]]` needs: two elements each with a `name`, kept apart.
    pub fn array_elements(&self, path: &str) -> Vec<Vec<&Entry>> {
        let starts: Vec<usize> =
            self.tables.iter().filter(|t| t.array && t.path == path).map(|t| t.start).collect();
        starts
            .iter()
            .map(|&from| {
                // Bounded by the next header of ANY table, not just the next element: a
                // `[[bin]]` followed by `[dependencies]` must not swallow the dependencies.
                let until = self
                    .tables
                    .iter()
                    .map(|t| t.start)
                    .filter(|&s| s > from)
                    .min()
                    .unwrap_or(usize::MAX);
                self.entries
                    .iter()
                    .filter(|e| e.table == path && e.key_start > from && e.key_start < until)
                    .collect()
            })
            .collect()
    }

    /// The table the byte offset `at` is inside — the last header at or before it.
    ///
    /// [`ROOT_TABLE`] before the first header. A caret *inside* a header's brackets belongs to the
    /// table being typed, which is the answer completion wants there.
    pub fn table_at(&self, at: usize) -> &str {
        // Headers are pushed in source order, so the LAST one at or before the caret is the
        // enclosing table.
        self.tables
            .iter()
            .rev()
            .find(|t| t.start <= at)
            .map(|t| t.path.as_str())
            .unwrap_or(ROOT_TABLE)
    }

    /// The entry whose key or value contains the byte offset `at`.
    ///
    /// Spans the whole assignment, first byte of the key to last byte of the value, so a caret in
    /// the middle of `features = ["de|rive"]` finds it.
    pub fn entry_at(&self, at: usize) -> Option<&Entry> {
        self.entries.iter().find(|e| at >= e.key_start && at <= e.value_end)
    }
}

// ── the scanner ────────────────────────────────────────────────────────────────

fn scan(text: &str) -> Manifest {
    let mut out = Manifest::default();
    let bytes = text.as_bytes();
    let mut table = ROOT_TABLE.to_string();
    let mut pos = 0usize;
    let mut line = 1u32;

    while pos < bytes.len() {
        let eol = line_end(bytes, pos);
        let raw = &text[pos..eol];
        let content = strip_comment(raw);
        let trimmed = content.trim_start();
        let indent = content.len() - trimmed.len();

        if trimmed.trim().is_empty() {
            pos = next_line(bytes, eol);
            line += 1;
            continue;
        }

        // A multi-line string opener anywhere on the line makes the rest opaque: nothing we read
        // lives in one, and mis-scanning its contents as keys is exactly how a false diagnostic
        // gets invented.
        if let Some(fence) = opening_fence(trimmed) {
            let (skipped_to, skipped_lines) = skip_fenced(text, pos + indent, fence);
            pos = next_line(bytes, skipped_to);
            line += skipped_lines;
            continue;
        }

        if trimmed.starts_with('[') {
            if let Some(t) = parse_header(trimmed, pos + indent, line) {
                table = t.path.clone();
                out.tables.push(t);
            }
            pos = next_line(bytes, eol);
            line += 1;
            continue;
        }

        match parse_assignment(text, pos + indent, eol, &table, line) {
            // The value may have run past this line (an open bracket or brace).
            Some((entry, consumed_to, consumed_lines)) => {
                out.entries.push(entry);
                pos = next_line(bytes, consumed_to.max(eol));
                line += consumed_lines + 1;
            }
            None => {
                pos = next_line(bytes, eol);
                line += 1;
            }
        }
    }
    out
}

/// Offset of the end of the line starting at `from` (the `\n`, or the end of input).
fn line_end(bytes: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    // A `\r\n` terminator: the `\r` is not content.
    if i > from && bytes[i - 1] == b'\r' { i - 1 } else { i }
}

/// Offset of the start of the line after the one ending at `eol`.
fn next_line(bytes: &[u8], eol: usize) -> usize {
    let mut i = eol;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    if i < bytes.len() { i + 1 } else { i }
}

/// Drop a trailing `#` comment, respecting quoted strings (a `#` inside `version = "1 # 2"`
/// is content).
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' | b'\'' => match quote {
                Some(q) if q == b => quote = None,
                Some(_) => {}
                None => quote = Some(b),
            },
            b'#' if quote.is_none() => return &line[..i],
            _ => {}
        }
    }
    line
}

/// The multi-line string fence a line OPENS and does not close, if any.
fn opening_fence(trimmed: &str) -> Option<&'static str> {
    for fence in ["\"\"\"", "'''"] {
        if let Some(after) = trimmed.find(fence) {
            let rest = &trimmed[after + fence.len()..];
            if !rest.contains(fence) {
                return Some(fence);
            }
        }
    }
    None
}

/// Skip from `from` (which opens `fence`) to the offset just past the closing fence.
/// Returns that offset and how many newlines were crossed.
fn skip_fenced(text: &str, from: usize, fence: &str) -> (usize, u32) {
    let after_open = match text[from..].find(fence) {
        Some(i) => from + i + fence.len(),
        None => return (text.len(), count_newlines(&text[from..])),
    };
    match text[after_open..].find(fence) {
        Some(i) => {
            let end = after_open + i + fence.len();
            (end, count_newlines(&text[from..end]))
        }
        // Unterminated — the rest of the file is inside the string. A file being typed.
        None => (text.len(), count_newlines(&text[from..])),
    }
}

fn count_newlines(s: &str) -> u32 {
    s.bytes().filter(|&b| b == b'\n').count() as u32
}

/// `[a.b]` / `[[a]]` → a header. `None` for a line that only looks like one.
fn parse_header(trimmed: &str, start: usize, line: u32) -> Option<TableSpan> {
    let array = trimmed.starts_with("[[");
    let open = if array { 2 } else { 1 };
    let close = if array { "]]" } else { "]" };
    let end_rel = trimmed.find(close)?;
    if end_rel < open {
        return None;
    }
    let inner = &trimmed[open..end_rel];
    let path = normalize_path(inner);
    if path.is_empty() {
        return None;
    }
    Some(TableSpan {
        path,
        array,
        start,
        end: start + end_rel + close.len(),
        line,
    })
}

/// A dotted table path with each segment unquoted and trimmed:
/// `target.'cfg(unix)'.dependencies` → `target.cfg(unix).dependencies`.
///
/// Split on the dots that are OUTSIDE quotes — a quoted segment may contain them
/// (`target.'cfg(target_os = "linux")'.dependencies` does not, but `"a.b"` as a key does).
fn normalize_path(inner: &str) -> String {
    split_dotted(inner).join(".")
}

/// Split a dotted path on unquoted dots, unquoting and trimming each segment.
fn split_dotted(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut quote: Option<char> = None;
    for ch in s.chars() {
        match ch {
            '"' | '\'' => match quote {
                Some(q) if q == ch => quote = None,
                Some(_) => buf.push(ch),
                None => quote = Some(ch),
            },
            '.' if quote.is_none() => out.push(std::mem::take(&mut buf).trim().to_string()),
            _ => buf.push(ch),
        }
    }
    out.push(buf.trim().to_string());
    out.retain(|s| !s.is_empty());
    out
}

/// Read `key = value` starting at `start`. Returns the entry, the offset the value ended at, and
/// how many extra lines it spanned.
fn parse_assignment(
    text: &str,
    start: usize,
    eol: usize,
    table: &str,
    line: u32,
) -> Option<(Entry, usize, u32)> {
    let head = &text[start..eol];
    let eq_rel = find_unquoted_eq(head)?;
    let key_raw = head[..eq_rel].trim_end();
    let key = split_dotted(key_raw).join(".");
    if key.is_empty() {
        return None;
    }
    let key_end = start + key_raw.len();

    // The value starts at the first non-space after the `=`, which may be the end of the line
    // (`features =` with the array on the next one, or a key still being typed).
    let after_eq = start + eq_rel + 1;
    let value_start = after_eq + leading_space(&text[after_eq..]);
    let (value, value_end, extra_lines) = scan_value(text, value_start.min(text.len()));

    Some((
        Entry {
            table: table.to_string(),
            key,
            items: items_of(&value, value_start),
            value,
            key_start: start,
            key_end,
            value_start,
            value_end,
            line,
        },
        value_end,
        extra_lines,
    ))
}

/// How many bytes of spaces/tabs `s` starts with, stopping at a newline.
fn leading_space(s: &str) -> usize {
    s.bytes().take_while(|&b| b == b' ' || b == b'\t').count()
}

/// The offset of the `=` that separates key from value, ignoring one inside quotes.
fn find_unquoted_eq(line: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (i, &b) in line.as_bytes().iter().enumerate() {
        match b {
            b'"' | b'\'' => match quote {
                Some(q) if q == b => quote = None,
                Some(_) => {}
                None => quote = Some(b),
            },
            b'=' if quote.is_none() => return Some(i),
            _ => {}
        }
    }
    None
}

/// Consume a value from `from`: to the end of its line when it is balanced there, otherwise on
/// until the brackets and braces it opened are closed. Returns the raw text, the end offset, and
/// how many newlines were crossed.
///
/// Balanced-across-lines rather than "keep going while the line ends in a comma", because that is
/// what TOML actually allows and what real manifests do:
///
/// ```toml
/// members = [
///   "crates/*",
/// ]
/// ```
fn scan_value(text: &str, from: usize) -> (String, usize, u32) {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = from;
    let mut lines = 0u32;
    let mut last_content = from;

    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'"' | b'\'' if quote.is_none() => quote = Some(b),
            b'"' | b'\'' if quote == Some(b) => quote = None,
            b'[' | b'{' if quote.is_none() => depth += 1,
            b']' | b'}' if quote.is_none() => {
                depth -= 1;
                if depth <= 0 {
                    // Past the closer that balanced us.
                    return (text[from..i + 1].trim_end().to_string(), i + 1, lines);
                }
            }
            b'#' if quote.is_none() && depth == 0 => break,
            b'\n' => {
                if depth <= 0 {
                    break;
                }
                quote = None; // an unterminated string does not continue across a line
                lines += 1;
            }
            _ => {}
        }
        if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
            last_content = i + 1;
        }
        i += 1;
    }
    let end = last_content.min(bytes.len()).max(from);
    (text[from..end].to_string(), end, lines)
}

/// Every quoted string in `raw`, with spans absolute to `base` (the offset `raw` starts at).
///
/// Used for array items. An inline table's *values* are quoted too, which is why this is not
/// applied blindly by callers: [`Entry::items`] is only meaningful for an array, and the schema
/// says which keys are arrays.
fn items_of(raw: &str, base: usize) -> Vec<Item> {
    let mut out = Vec::new();
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            let open = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b {
                // A `\"` inside a basic string. Literal strings ('…') have no escapes, but
                // treating one uniformly costs nothing here: a `\` before the closing quote of a
                // literal string is not a shape any manifest has.
                if bytes[i] == b'\\' && b == b'"' {
                    i += 1;
                }
                i += 1;
            }
            if i < bytes.len() {
                out.push(Item {
                    text: raw[open + 1..i].to_string(),
                    start: base + open,
                    end: base + i + 1,
                });
                i += 1;
                continue;
            }
            // Unterminated — a string being typed. Not an item.
            break;
        }
        i += 1;
    }
    out
}

/// The depth-1 `key = value` pairs of the inline table `raw` (which starts with `{`).
fn inline_keys_of(raw: &str, base: usize) -> Vec<InlineKey> {
    let mut out = Vec::new();
    let bytes = raw.as_bytes();
    // Everything between the outer braces, split on depth-1 commas.
    let inner_from = 1usize;
    let inner_to = raw.rfind('}').unwrap_or(raw.len());
    if inner_to <= inner_from {
        return out;
    }
    // `inner_to` is the outer `}` itself, so the loop stops BEFORE it: letting the closing brace
    // through would take the depth negative and the last segment would never be emitted — which
    // silently dropped every one-key spec (`{ workspace = true }`) and the last key of every
    // other.
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut seg_start = inner_from;
    let mut i = inner_from;
    while i < inner_to {
        let b = bytes[i];
        match b {
            b'"' | b'\'' if quote.is_none() => quote = Some(b),
            b'"' | b'\'' if quote == Some(b) => quote = None,
            b'[' | b'{' if quote.is_none() => depth += 1,
            b']' | b'}' if quote.is_none() => depth -= 1,
            _ => {}
        }
        if quote.is_none() && depth == 0 && b == b',' {
            if let Some(k) = inline_pair(&raw[seg_start..i], base + seg_start) {
                out.push(k);
            }
            seg_start = i + 1;
        }
        i += 1;
    }
    // The segment the closing brace ends. A trailing comma leaves it empty, which `inline_pair`
    // rejects on its own.
    if seg_start < inner_to {
        if let Some(k) = inline_pair(&raw[seg_start..inner_to], base + seg_start) {
            out.push(k);
        }
    }
    out
}

/// One `k = v` segment of an inline table.
fn inline_pair(seg: &str, base: usize) -> Option<InlineKey> {
    let eq = find_unquoted_eq(seg)?;
    let key_raw = seg[..eq].trim();
    if key_raw.is_empty() {
        return None;
    }
    let lead = seg[..eq].len() - seg[..eq].trim_start().len();
    let key = split_dotted(key_raw).join(".");
    if key.is_empty() {
        return None;
    }
    let after_eq = eq + 1;
    let value_lead = seg[after_eq..].len() - seg[after_eq..].trim_start().len();
    let value = seg[after_eq..].trim().to_string();
    let value_start = base + after_eq + value_lead;
    Some(InlineKey {
        value_end: value_start + value.len(),
        value,
        key,
        start: base + lead,
        end: base + lead + key_raw.len(),
        value_start,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKSPACE: &str = r#"[workspace]
resolver = "2"
members = [
  "crates/foundation/*",   # a glob
  "crates/products/bennu/be",
]

[workspace.package]
edition = "2021"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
toml  = "0.8"
"#;

    #[test]
    fn reads_tables_and_keys_with_their_tables() {
        let m = Manifest::parse(WORKSPACE);
        assert!(m.has_table("workspace"));
        assert!(m.has_table("workspace.package"));
        assert!(m.has_table("workspace.dependencies"));
        assert_eq!(m.str_of("workspace", "resolver"), Some("2"));
        assert_eq!(m.str_of("workspace.package", "edition"), Some("2021"));
        assert_eq!(m.str_of("workspace.dependencies", "toml"), Some("0.8"));
    }

    #[test]
    fn a_multi_line_array_keeps_every_item_with_its_own_span() {
        let m = Manifest::parse(WORKSPACE);
        let items = m.items_of("workspace", "members");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "crates/foundation/*");
        assert_eq!(items[1].text, "crates/products/bennu/be");
        // The span must be the item, not the array: a diagnostic about a missing member has to
        // underline that member.
        assert_eq!(&WORKSPACE[items[1].start..items[1].end], "\"crates/products/bennu/be\"");
        // …and the comment inside the array is not an item.
        assert!(items.iter().all(|i| !i.text.contains("glob")));
    }

    #[test]
    fn an_inline_table_yields_its_own_keys_only() {
        let m = Manifest::parse(WORKSPACE);
        let serde = m.get("workspace.dependencies", "serde").expect("serde is there");
        assert!(serde.is_inline_table());
        let keys: Vec<String> = serde.inline_keys().into_iter().map(|k| k.key).collect();
        assert_eq!(keys, vec!["version", "features"]);
        // The span points at the key inside the inline table.
        let vk = &serde.inline_keys()[0];
        assert_eq!(&WORKSPACE[vk.start..vk.end], "version");
    }

    #[test]
    fn a_dotted_key_is_one_key_and_reduces_to_its_base() {
        let m = Manifest::parse("[package]\nname = \"x\"\nedition.workspace = true\n");
        let e = m.get("package", "edition.workspace").expect("the dotted key is one key");
        assert_eq!(e.base_key(), "edition");
        assert_eq!(e.key_suffix(), Some("workspace"));
        assert_eq!(e.bool_value(), Some(true));
        // Which is NOT an edition: reading it as one would put "true" in the project header.
        assert_eq!(e.str_value(), None);
        // And `get_base` finds it, which is how "does this manifest set an edition" is asked.
        assert!(m.get_base("package", "edition").is_some());
    }

    #[test]
    fn a_hash_inside_a_string_is_not_a_comment() {
        let m = Manifest::parse("[package]\nname = \"a#b\" # real comment\n");
        assert_eq!(m.str_of("package", "name"), Some("a#b"));
    }

    #[test]
    fn array_of_tables_elements_stay_apart() {
        let text = "\
[[bin]]
name = \"one\"
path = \"src/one.rs\"

[[bin]]
name = \"two\"

[dependencies]
serde = \"1\"
";
        let m = Manifest::parse(text);
        let els = m.array_elements("bin");
        assert_eq!(els.len(), 2);
        assert_eq!(els[0].iter().find(|e| e.key == "name").unwrap().str_value(), Some("one"));
        assert_eq!(els[1].iter().find(|e| e.key == "name").unwrap().str_value(), Some("two"));
        // The `[dependencies]` that follows must not have been swallowed into the last element.
        assert_eq!(els[1].len(), 1, "only `name` belongs to the second [[bin]]");
        assert_eq!(m.str_of("dependencies", "serde"), Some("1"));
    }

    #[test]
    fn a_quoted_table_segment_is_unquoted_in_the_path() {
        let m = Manifest::parse("[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n");
        assert!(m.has_table("target.cfg(unix).dependencies"));
        assert_eq!(m.str_of("target.cfg(unix).dependencies", "libc"), Some("0.2"));
    }

    #[test]
    fn table_at_names_the_enclosing_table() {
        let m = Manifest::parse(WORKSPACE);
        let at_toml = WORKSPACE.find("toml").expect("in the text");
        assert_eq!(m.table_at(at_toml), "workspace.dependencies");
        let at_resolver = WORKSPACE.find("resolver").expect("in the text");
        assert_eq!(m.table_at(at_resolver), "workspace");
        assert_eq!(m.table_at(0), "workspace", "the caret is inside the first header");
    }

    #[test]
    fn a_key_before_the_first_header_belongs_to_the_root_table() {
        let m = Manifest::parse("stray = 1\n[package]\nname = \"x\"\n");
        assert_eq!(m.get(ROOT_TABLE, "stray").map(|e| e.value.as_str()), Some("1"));
        assert_eq!(m.table_at(0), ROOT_TABLE);
    }

    /// A multi-line string is opaque: what is inside it must not be read as keys, or a doc
    /// comment full of `key = value` examples invents diagnostics out of prose.
    #[test]
    fn a_multi_line_string_is_skipped_whole() {
        let text = "\
[package]
name = \"x\"
description = \"\"\"
not_a_key = \"and not a value\"
[not-a-table]
\"\"\"
edition = \"2021\"
";
        let m = Manifest::parse(text);
        assert_eq!(m.str_of("package", "edition"), Some("2021"));
        assert!(m.get("package", "not_a_key").is_none());
        assert!(!m.has_table("not-a-table"));
    }

    /// The state every file is in while it is being typed. It must yield what it has.
    #[test]
    fn a_half_typed_file_still_reads() {
        let m = Manifest::parse("[dependencies]\nser");
        assert!(m.has_table("dependencies"));
        assert!(m.entries.is_empty(), "`ser` is not an assignment yet");

        let m = Manifest::parse("[dependencies]\nserde = ");
        let e = m.get("dependencies", "serde").expect("the key exists the moment there is an `=`");
        assert_eq!(e.value, "");
        assert_eq!(e.value_start, e.value_end);

        let m = Manifest::parse("[dependencies]\nserde = \"1");
        let e = m.get("dependencies", "serde").unwrap();
        assert_eq!(e.value, "\"1", "an unterminated string is the value so far");
        assert!(e.items.is_empty(), "and not a complete item");
    }

    #[test]
    fn crlf_line_endings_do_not_leak_into_values() {
        let m = Manifest::parse("[package]\r\nname = \"x\"\r\nedition = \"2021\"\r\n");
        assert_eq!(m.str_of("package", "name"), Some("x"));
        assert_eq!(m.str_of("package", "edition"), Some("2021"));
    }

    #[test]
    fn entry_at_finds_the_assignment_a_caret_is_in() {
        let text = "[dependencies]\nserde = { version = \"1\" }\n";
        let m = Manifest::parse(text);
        let inside_value = text.find("version").unwrap() + 3;
        assert_eq!(m.entry_at(inside_value).map(|e| e.key.as_str()), Some("serde"));
        let on_header = 2;
        assert!(m.entry_at(on_header).is_none());
    }

    #[test]
    fn line_numbers_are_one_based_and_survive_a_multi_line_value() {
        let m = Manifest::parse(WORKSPACE);
        assert_eq!(m.table("workspace").unwrap().line, 1);
        assert_eq!(m.get("workspace", "resolver").unwrap().line, 2);
        assert_eq!(m.get("workspace", "members").unwrap().line, 3);
        // `members` spans lines 3..6, so the header after it is line 8.
        assert_eq!(m.table("workspace.package").unwrap().line, 8);
        assert_eq!(m.get("workspace.package", "edition").unwrap().line, 9);
        assert_eq!(m.table("workspace.dependencies").unwrap().line, 11);
        assert_eq!(m.get("workspace.dependencies", "toml").unwrap().line, 13);
    }
}
