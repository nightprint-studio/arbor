//! YAML parsing, `serde_json::Value` projection, multi-document `---`
//! stream splitting / joining, and the indent sniffer.
//!
//! Two independent parser passes (lifted verbatim from the launcher's
//! `yaml_studio/mod.rs`, only the error type changed to plain `String`):
//!
//! * `yaml_edit` is the edit-side source of truth (rowan tree, preserves
//!   formatting). We re-parse it on demand in [`crate::mutate`] rather than
//!   caching it (its `Document` holds rowan `NonNull` which is `!Send`).
//! * `serde_yaml_ng` is the nav-side source of truth: it yields the JSON
//!   projection the tree pane and JSONPath query depend on. Multi-doc
//!   streams collapse to `Value::Array`.
//!
//! When the two passes disagree about doc-ability or doc count we treat it
//! as a parse error — the modal then shows the raw text + the error rather
//! than a misleading half-tree.

use serde_json::Value;
use yaml_edit::Document;

/// Canonical YAML stream separator. A line of exactly `---` (no
/// leading/trailing whitespace) at column 0 is the YAML 1.2
/// directive-end / document-start marker. We slice on it for per-document
/// `yaml_edit` parsing.
pub const DOC_SEPARATOR: &str = "---";

/// Result of the parse pair: the projected `serde_json::Value` (`None` on
/// parse error), the parse error string, and the multi-doc flag (cached so
/// the mutation path resolver doesn't re-derive it).
pub struct ParseOutcomeFull {
    pub value:     Option<Value>,
    pub error:     Option<String>,
    pub multi_doc: bool,
}

/// Parse `text` to the JSON projection + multi-doc flag, dropping the
/// `Vec<Document>` (the backend never caches the `!Send` AST). This is the
/// `SimpleFormat::parse` body.
pub fn parse_outcome(text: &str) -> ParseOutcomeFull {
    let (docs, value, error, _count, multi) = parse_text(text);
    drop(docs);
    ParseOutcomeFull { value, error, multi_doc: multi }
}

/// Parse `text` and project to `serde_json::Value`, dropping the AST.
/// `None` on parse error (best-effort, for the cross-ref scanner).
pub fn parse_to_value(text: &str) -> Option<Value> {
    parse_text(text).1
}

/// Parses `text` into:
///   - `Vec<yaml_edit::Document>` per stream item (None on parse error)
///   - `Value` projection (also None on parse error)
///   - parse error string (when set, both `docs` and `value` are None)
///   - doc_count (number of stream items)
///   - multi_doc flag (`doc_count > 1`)
pub fn parse_text(
    text: &str,
) -> (Option<Vec<Document>>, Option<Value>, Option<String>, usize, bool) {
    use std::str::FromStr;
    let chunks = split_yaml_stream(text);
    let mut docs: Vec<Document> = Vec::with_capacity(chunks.len());
    for (idx, chunk) in chunks.iter().enumerate() {
        match Document::from_str(chunk) {
            Ok(d) => docs.push(d),
            Err(e) => {
                return (
                    None,
                    None,
                    Some(format!("YAML parse error (doc {idx}): {e}")),
                    0,
                    false,
                );
            }
        }
    }

    let (value, value_err, val_doc_count) = parse_to_value_via_serde_yaml_ng(text);
    if let Some(e) = value_err {
        return (None, None, Some(e), 0, false);
    }
    let edit_doc_count = docs.len().max(1);
    if val_doc_count != 0 && val_doc_count != edit_doc_count {
        return (
            None,
            None,
            Some(format!(
                "YAML parse disagreement: editor saw {edit_doc_count} docs, parser saw {val_doc_count}",
            )),
            0,
            false,
        );
    }
    let doc_count = edit_doc_count.max(val_doc_count);
    let multi_doc = doc_count > 1;
    (Some(docs), value, None, doc_count, multi_doc)
}

/// Parse the text via `serde_yaml_ng::Deserializer` and project to
/// `serde_json::Value`. Returns `(value, error, doc_count)`. Multi-doc
/// streams collapse to `Value::Array`.
fn parse_to_value_via_serde_yaml_ng(text: &str) -> (Option<Value>, Option<String>, usize) {
    use serde::Deserialize;
    let mut docs: Vec<Value> = Vec::new();
    for de in serde_yaml_ng::Deserializer::from_str(text) {
        match Value::deserialize(de) {
            Ok(v) => docs.push(v),
            Err(e) => return (None, Some(format!("YAML parse error: {e}")), 0),
        }
    }
    if docs.is_empty() {
        return (Some(Value::Null), None, 0);
    }
    if docs.len() == 1 {
        let v = docs.into_iter().next().unwrap();
        (Some(v), None, 1)
    } else {
        let n = docs.len();
        (Some(Value::Array(docs)), None, n)
    }
}

/// Slice the buffer at lines containing exactly `---` (column 0, no
/// surrounding whitespace). Each slice is a single-document YAML body
/// passed to `yaml_edit::Document::from_str`.
///
/// Limitation: a literal `---` line inside a block scalar would be treated
/// as a separator here but as scalar content by the YAML parser. We catch
/// the divergence in [`parse_text`] by comparing edit-side and parser-side
/// doc counts and surfacing it as a parse error.
pub fn split_yaml_stream(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut first_chunk = true;
    for line in text.split_inclusive('\n') {
        let stripped = line.trim_end_matches(['\n', '\r']);
        if stripped == DOC_SEPARATOR {
            if first_chunk && cur.trim().is_empty() {
                first_chunk = false;
                cur.clear();
                continue;
            }
            chunks.push(std::mem::take(&mut cur));
            first_chunk = false;
            continue;
        }
        cur.push_str(line);
    }
    if !cur.is_empty() || chunks.is_empty() {
        chunks.push(cur);
    }
    chunks
}

/// Re-emit a list of `yaml_edit::Document` back to text. Single-doc
/// outputs the document's `Display` as-is; multi-doc joins with `---\n`
/// between chunks (no leading `---` — the first chunk owns the implicit
/// document-start marker).
pub fn join_documents(docs: &[Document], multi: bool) -> String {
    if docs.is_empty() {
        return String::new();
    }
    if !multi || docs.len() == 1 {
        return docs[0].to_string();
    }
    let mut out = String::new();
    for (i, d) in docs.iter().enumerate() {
        let body = d.to_string();
        if i > 0 {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("---\n");
        }
        out.push_str(&body);
    }
    out
}

/// Sniff the document's indent string for the FE indent pill.
pub fn detect_indent(text: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let n = line.len() - trimmed.len();
        if n == 0 {
            continue;
        }
        if line.starts_with('\t') {
            return "\t".into();
        }
        return " ".repeat(n);
    }
    "  ".into()
}
