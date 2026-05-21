//! Owned, byte-range-aware JSON AST. Built on top of `jsonc-parser`
//! (borrowing parser) but lifted into a `'static`-friendly tree the
//! `JsonStudioRegistry::Doc` can keep alongside the source buffer for
//! position-preserving splice mutations (FROZEN F17 lossless edit
//! requirement for JSON since Phase 3.b).
//!
//! Why our own AST instead of operating on `jsonc-parser::ast::Value<'a>`
//! directly: every `Value` variant carries a `'a` borrowed against the
//! source string, so storing it inside the long-lived `Doc` forces a
//! self-referential struct or a heap-pinned source. We pay one tree
//! re-build per parse (cheap — bounded by file size, no character-level
//! scans) and the rest of the registry stays straightforwardly owned.
//!
//! Each node records a `Span { start, end }` in **byte offsets** of the
//! source buffer. Splicing edits write at these offsets directly — no
//! re-serialisation of untouched bytes — which is what makes "lossless
//! edit" hold for any subtree the user doesn't touch.

use jsonc_parser::ast::{
    Array as JpArray, Object as JpObject, ObjectProp as JpObjectProp,
    ObjectPropName as JpObjectPropName, Value as JpValue,
};
use jsonc_parser::{parse_to_ast, CollectOptions, CommentCollectionStrategy, ParseOptions};

/// Byte range inside the source buffer. `end` is exclusive, matching
/// `&str[start..end]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end:   usize,
}

impl Span {
    #[allow(dead_code)]
    pub fn len(&self) -> usize { self.end.saturating_sub(self.start) }
    pub fn slice<'a>(&self, text: &'a str) -> &'a str {
        &text[self.start..self.end]
    }
}

/// Owned JSON AST node. Mirrors the six JSON value kinds (object,
/// array, string, number, bool, null) — JSONC-only constructs (comments,
/// trailing commas, single-quoted keys) are NOT modelled here: parse
/// rejects them in strict mode (Phase 3.b default for `.json`). Phase
/// 3.d will flip the parser into relaxed mode and either extend this
/// AST or carry the extra tokens out-of-band.
#[derive(Debug, Clone)]
pub enum JsonAst {
    Object(JsonObject),
    Array(JsonArray),
    String(JsonString),
    Number(JsonNumber),
    Bool(JsonBool),
    Null(Span),
}

impl JsonAst {
    pub fn span(&self) -> Span {
        match self {
            JsonAst::Object(o) => o.span,
            JsonAst::Array(a)  => a.span,
            JsonAst::String(s) => s.span,
            JsonAst::Number(n) => n.span,
            JsonAst::Bool(b)   => b.span,
            JsonAst::Null(s)   => *s,
        }
    }

    /// Discriminating kind string used by the unified
    /// `studio::format::types::NodeView.kind` field.
    pub fn kind_str(&self) -> &'static str {
        match self {
            JsonAst::Object(_) => "object",
            JsonAst::Array(_)  => "array",
            JsonAst::String(_) => "string",
            JsonAst::Number(_) => "number",
            JsonAst::Bool(_)   => "bool",
            JsonAst::Null(_)   => "null",
        }
    }

    pub fn child_count(&self) -> usize {
        match self {
            JsonAst::Object(o) => o.props.len(),
            JsonAst::Array(a)  => a.items.len(),
            _ => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct JsonObject {
    pub span:  Span,
    pub props: Vec<JsonProp>,
}

#[derive(Debug, Clone)]
pub struct JsonProp {
    /// Whole `"key": value` range, used by `RemoveAt` to delete an
    /// entire property without leaving an orphaned key behind.
    pub span:      Span,
    /// Decoded property name (unescaped). Matches the segment-string
    /// in the `studio_*` path API.
    pub name:      String,
    /// Range of the key token (with surrounding quotes). Used by
    /// rename / key-edit flows (not in 3.b — reserved for 3.c F12).
    #[allow(dead_code)]
    pub name_span: Span,
    pub value:     JsonAst,
}

#[derive(Debug, Clone)]
pub struct JsonArray {
    pub span:  Span,
    pub items: Vec<JsonAst>,
}

#[derive(Debug, Clone)]
pub struct JsonString {
    pub span:  Span,
    /// Decoded string content (no surrounding quotes, escapes resolved).
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct JsonNumber {
    pub span: Span,
    /// Source text of the literal — preserves the user's chosen format
    /// (`1.0` stays `1.0`, `1e3` stays `1e3`). Round-tripped verbatim
    /// when the node isn't mutated.
    pub raw:  String,
}

#[derive(Debug, Clone)]
pub struct JsonBool {
    pub span:  Span,
    pub value: bool,
}

// ── Parse ────────────────────────────────────────────────────────────────────

/// Parse `text` as strict JSON (RFC 8259) — comments / trailing commas
/// / single-quoted keys are rejected. Phase 3.d flips an extra `strict`
/// flag here for `.jsonc` files.
#[allow(dead_code)]
pub fn parse(text: &str) -> Result<JsonAst, String> {
    parse_with(text, /* strict */ true)
}

pub fn parse_with(text: &str, strict: bool) -> Result<JsonAst, String> {
    let options = if strict {
        ParseOptions {
            allow_comments:                    false,
            allow_loose_object_property_names: false,
            allow_trailing_commas:             false,
            allow_missing_commas:              false,
            allow_single_quoted_strings:       false,
            allow_hexadecimal_numbers:         false,
            allow_unary_plus_numbers:          false,
        }
    } else {
        ParseOptions {
            allow_comments:                    true,
            allow_loose_object_property_names: true,
            allow_trailing_commas:             true,
            allow_missing_commas:              true,
            allow_single_quoted_strings:       true,
            allow_hexadecimal_numbers:         true,
            allow_unary_plus_numbers:          true,
        }
    };
    // We don't need tokens or comments at the moment (Phase 3.d will).
    let collect = CollectOptions {
        comments: CommentCollectionStrategy::Off,
        tokens:   false,
    };
    let parsed = parse_to_ast(text, &collect, &options)
        .map_err(|e| format!("JSON parse error: {e}"))?;
    let value = parsed
        .value
        .ok_or_else(|| "JSON parse error: empty input".to_string())?;
    Ok(from_jp_value(&value))
}

// ── Conversion from jsonc-parser borrowed AST → our owned AST ────────────────

fn from_jp_value(v: &JpValue<'_>) -> JsonAst {
    match v {
        JpValue::StringLit(s) => JsonAst::String(JsonString {
            span:  Span { start: s.range.start, end: s.range.end },
            value: s.value.to_string(),
        }),
        JpValue::NumberLit(n) => JsonAst::Number(JsonNumber {
            span: Span { start: n.range.start, end: n.range.end },
            raw:  n.value.to_string(),
        }),
        JpValue::BooleanLit(b) => JsonAst::Bool(JsonBool {
            span:  Span { start: b.range.start, end: b.range.end },
            value: b.value,
        }),
        JpValue::NullKeyword(n) => JsonAst::Null(Span { start: n.range.start, end: n.range.end }),
        JpValue::Object(o)      => JsonAst::Object(from_jp_object(o)),
        JpValue::Array(a)       => JsonAst::Array(from_jp_array(a)),
    }
}

fn from_jp_object(o: &JpObject<'_>) -> JsonObject {
    JsonObject {
        span:  Span { start: o.range.start, end: o.range.end },
        props: o.properties.iter().map(from_jp_prop).collect(),
    }
}

fn from_jp_prop(p: &JpObjectProp<'_>) -> JsonProp {
    let (name, name_span) = match &p.name {
        JpObjectPropName::String(s) => (
            s.value.to_string(),
            Span { start: s.range.start, end: s.range.end },
        ),
        JpObjectPropName::Word(w) => (
            w.value.to_string(),
            Span { start: w.range.start, end: w.range.end },
        ),
    };
    JsonProp {
        span:  Span { start: p.range.start, end: p.range.end },
        name,
        name_span,
        value: from_jp_value(&p.value),
    }
}

fn from_jp_array(a: &JpArray<'_>) -> JsonArray {
    JsonArray {
        span:  Span { start: a.range.start, end: a.range.end },
        items: a.elements.iter().map(from_jp_value).collect(),
    }
}

// ── Path resolution ──────────────────────────────────────────────────────────

/// Walk the AST following `path` (object keys verbatim, array indices
/// as decimal strings). `None` when any segment fails to resolve.
pub fn resolve<'a>(root: &'a JsonAst, path: &[String]) -> Option<&'a JsonAst> {
    let mut cur = root;
    for seg in path {
        cur = match cur {
            JsonAst::Object(o) => {
                let p = o.props.iter().find(|p| p.name == *seg)?;
                &p.value
            }
            JsonAst::Array(a) => {
                let i: usize = seg.parse().ok()?;
                a.items.get(i)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

/// Resolve the parent + child index pair of the node at `path`.
/// Returns the parent (`Object` or `Array`) plus the index of the child
/// inside its parent's `props` / `items` Vec. Useful for splicing edits
/// that need to know what surrounds the target (commas, sibling spans).
/// `None` when path is empty (root has no parent) or resolution fails.
pub fn resolve_parent<'a>(
    root: &'a JsonAst,
    path: &[String],
) -> Option<(&'a JsonAst, usize, &'a JsonAst)> {
    if path.is_empty() { return None; }
    let parent_path = &path[..path.len() - 1];
    let last = &path[path.len() - 1];
    let parent = resolve(root, parent_path)?;
    let (idx, target) = match parent {
        JsonAst::Object(o) => {
            let i = o.props.iter().position(|p| p.name == *last)?;
            (i, &o.props[i].value)
        }
        JsonAst::Array(a) => {
            let i: usize = last.parse().ok()?;
            (i, a.items.get(i)?)
        }
        _ => return None,
    };
    Some((parent, idx, target))
}

// ── JSONC feature detection + AST→Value bridge ──────────────────────────────

/// Scan `text` for JSONC-only constructs: line comments (`//`), block
/// comments (`/* ... */`), and trailing commas (a `,` followed only by
/// whitespace/newlines before the closing `]` / `}`). Strings are
/// honoured so that `"a // b"` does NOT match. Used by Phase 3.d to
/// flag `.json` files that contain features a strict parser would
/// reject — surfaces the "rename to .jsonc / strip" banner.
pub fn detect_jsonc_features(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if b == b'"' { in_string = false; }
            i += 1;
            continue;
        }
        match b {
            b'"' => { in_string = true; i += 1; }
            b'/' if i + 1 < bytes.len() => {
                let nxt = bytes[i + 1];
                if nxt == b'/' || nxt == b'*' { return true; }
                i += 1;
            }
            b',' => {
                // Look ahead, skipping whitespace; if we hit `]` or `}`
                // before any other non-whitespace byte → trailing comma.
                let mut j = i + 1;
                while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r') {
                    j += 1;
                }
                if j < bytes.len() && (bytes[j] == b']' || bytes[j] == b'}') {
                    return true;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    false
}

/// Build a `serde_json::Value` mirror of `ast`. Loss-free for the value
/// tree (decoded strings, numbers parsed). Used in lenient (JSONC) parse
/// paths where `simd_json` strict parsing of the source buffer would
/// fail on comments / trailing commas — the AST holds the canonical
/// representation, the Value is for navigation + JSONPath queries.
pub fn ast_to_value(ast: &JsonAst) -> serde_json::Value {
    match ast {
        JsonAst::Null(_)   => serde_json::Value::Null,
        JsonAst::Bool(b)   => serde_json::Value::Bool(b.value),
        JsonAst::Number(n) => serde_json::from_str(&n.raw)
            .unwrap_or(serde_json::Value::Null),
        JsonAst::String(s) => serde_json::Value::String(s.value.clone()),
        JsonAst::Array(a)  => serde_json::Value::Array(
            a.items.iter().map(ast_to_value).collect()
        ),
        JsonAst::Object(o) => {
            let mut map = serde_json::Map::with_capacity(o.props.len());
            for p in &o.props {
                map.insert(p.name.clone(), ast_to_value(&p.value));
            }
            serde_json::Value::Object(map)
        }
    }
}

// ── Pretty preview helpers (shared with the tree pane) ───────────────────────

const PREVIEW_MAX_CHARS: usize = 64;

pub fn preview(v: &JsonAst) -> String {
    match v {
        JsonAst::Object(o) => format!("{{{} keys}}", o.props.len()),
        JsonAst::Array(a)  => format!("[{} items]", a.items.len()),
        JsonAst::String(s) => {
            let mut out = String::with_capacity(s.value.len().min(PREVIEW_MAX_CHARS) + 2);
            out.push('"');
            for (i, ch) in s.value.chars().enumerate() {
                if i >= PREVIEW_MAX_CHARS { out.push('…'); break; }
                out.push(ch);
            }
            out.push('"');
            out
        }
        JsonAst::Number(n) => n.raw.clone(),
        JsonAst::Bool(b)   => b.value.to_string(),
        JsonAst::Null(_)   => "null".to_string(),
    }
}
