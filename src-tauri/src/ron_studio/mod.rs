//! RON Studio — in-memory RON document registry.
//!
//! Backs the `ron-studio` plugin. The Text view holds the source-of-truth
//! string, which the user edits directly. The Tree view is a derived, lazy,
//! read-only projection of the parsed value — recomputed when the text
//! changes (debounced on the frontend). Save writes the current text;
//! Format/RON↔JSON normalise it through the parser-serialiser round-trip
//! (and lose comments — caller is warned in the UI).
//!
//! Parsing uses a small custom AST (`ast::RonAst`) instead of `ron::Value`
//! because `ron::Value` documents itself as not supporting enums: it drops
//! variant tags, so `element: Dark` and `variant: Action(..)` would render
//! as `()` / `[..]` with no indication of which variant was used. The
//! custom AST preserves tags so the tree shows `Dark` / `Action`. Schema
//! (cross-file `.rs` walking) lives in `schema.rs`.

pub mod ast;
pub mod backend_impl;
pub mod schema;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_json_path::{JsonPath, PathElement};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::ron_studio::ast::RonAst;

#[derive(Default)]
pub struct RonStudioRegistry {
    docs: HashMap<String, Doc>,
}

struct Doc {
    /// Path the file was loaded from (None when opened via paste).
    source_path: Option<String>,
    /// Canonical encoding name (`UTF-8`, `windows-1252`, `UTF-16LE`,
    /// …) detected from the raw bytes at open time. Used at save to
    /// re-encode losslessly. UTF-8 default for FE-pushed text.
    encoding_label: String,
    /// `true` when the open buffer began with a UTF-8/UTF-16 BOM. Save
    /// re-prepends the BOM appropriate for `encoding_label`.
    had_bom:     bool,
    /// The text loaded from disk (or pasted) on open — used as the "before"
    /// side of the diff. Doesn't change after `parse`.
    original:    String,
    /// Current text: starts equal to `original`, updated by every
    /// `ron_studio_set_text` call as the user types. This is what
    /// `ron_studio_save` writes back.
    current:     String,
    /// Latest parse of `current`. `None` when the current text doesn't
    /// parse — the Tree view shows the parse error instead.
    parsed:      Option<RonAst>,
    /// Latest parse error, if any (matches `parsed = None`).
    parse_error: Option<String>,
    /// Indent unit used by the pretty-printer for tree edits, Format
    /// and RON↔JSON. Default `"  "` (two spaces) — overridable via
    /// `ron_studio_set_indent`. Anything goes: `"  "`, `"    "`, `"\t"`.
    indent:      String,

    // ── Undo / redo ───────────────────────────────────────────────────────
    //
    // Snapshot-based history. Each entry is a full document text — for
    // the file sizes we expect (a few KB to a few hundred KB) the
    // memory cost is negligible compared to the complexity of a
    // command-pattern. The first entry is the initial load text, the
    // current cursor sits at `history_pos`.
    //
    // Rapid `set_text` calls (the textarea push-back) coalesce into the
    // top entry if they land within `HISTORY_COALESCE_WINDOW` of the
    // previous push. Discrete tree edits never coalesce, even when fast.
    history:     Vec<String>,
    history_pos: usize,
    /// Whether the next `set_text` is eligible to coalesce. Tree edits
    /// flip this to false to break the run; subsequent typing starts
    /// a fresh coalesce window.
    history_coalesce_armed: bool,
    last_push:   Instant,
}

const HISTORY_MAX_ENTRIES: usize = 128;
const HISTORY_COALESCE_WINDOW: Duration = Duration::from_millis(500);

/// Push a snapshot of `new_text` onto the document's history.
///
/// `can_coalesce` indicates this edit *could* be merged with the
/// previous one if it's recent enough — true for typing in the
/// textarea (`set_text`), false for discrete tree mutations.
fn record_history(doc: &mut Doc, new_text: &str, can_coalesce: bool) {
    // Anything ahead of the cursor is reachable only via redo; drop it
    // the moment the user creates a new branch.
    doc.history.truncate(doc.history_pos + 1);

    let elapsed = doc.last_push.elapsed();
    let coalesce = can_coalesce
        && doc.history_coalesce_armed
        && elapsed < HISTORY_COALESCE_WINDOW
        && !doc.history.is_empty();

    if coalesce {
        // Overwrite the top entry so undo jumps back past the whole
        // typing burst, not just the last keystroke.
        if let Some(last) = doc.history.last_mut() {
            if last != new_text {
                *last = new_text.to_string();
            }
        }
    } else {
        // Skip the push if the text didn't actually change vs the top —
        // avoids polluting history when a no-op mutation is replayed.
        if doc.history.last().map(|s| s.as_str()) != Some(new_text) {
            doc.history.push(new_text.to_string());
            doc.history_pos = doc.history.len() - 1;
            if doc.history.len() > HISTORY_MAX_ENTRIES {
                let drop = doc.history.len() - HISTORY_MAX_ENTRIES;
                doc.history.drain(0..drop);
                doc.history_pos = doc.history_pos.saturating_sub(drop);
            }
        }
    }
    doc.history_coalesce_armed = can_coalesce;
    doc.last_push = Instant::now();
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Struct,           // (field: val, ...) — anonymous
    NamedStruct,      // Name(field: val, ...) — named struct or struct-variant of an enum
    Tuple,            // (val, val) — anonymous tuple / unit
    NamedTuple,       // Name(val, val) — named tuple or tuple-variant of an enum
    UnitVariant,      // bare identifier — unit-only enum variant (e.g. `Dark`)
    Map,              // { key: val, ... }
    List,             // [val, val, ...]
    String,
    Char,
    Number,
    Bool,
    Option,           // Some(_) / None
    Unit,             // ()
}

#[derive(Debug, Serialize)]
pub struct ParseResult {
    pub doc_id:      String,
    pub size_bytes:  usize,
    pub source_path: Option<String>,
    pub original:    String,
    pub parse_error: Option<String>,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    /// Schema hint extracted from a `//! ron-studio:` first-line
    /// directive inside the document, OR from a `.ron-studio.toml`
    /// found in the file's folder hierarchy. The frontend uses this
    /// to auto-load the schema without prompting the user.
    pub schema_hint: Option<SchemaHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaHint {
    pub rs_file:   String,
    pub root_type: String,
    /// Where the hint came from — purely informational, surfaced in
    /// the schema panel so the user knows whether the auto-load came
    /// from an inline directive or a side-car config.
    pub origin:    SchemaHintOrigin,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaHintOrigin {
    /// `//! ron-studio: schema=…, root=…` at the top of the file.
    Directive,
    /// `.ron-studio.toml` in this file's folder or an ancestor.
    Sidecar,
}

#[derive(Debug, Serialize)]
pub struct UpdateResult {
    pub parse_error: Option<String>,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    /// Surfaced to the UI so the undo / redo buttons (and their
    /// keyboard shortcuts) can light up immediately after each edit,
    /// without a separate round-trip to query history state.
    pub can_undo:    bool,
    pub can_redo:    bool,
}

/// Result of a structured tree-edit (primitive set, option toggle, …).
/// Carries the regenerated text back so the frontend can refresh its
/// textarea + highlight overlay in a single round-trip.
#[derive(Debug, Serialize)]
pub struct MutateResult {
    pub text:        String,
    pub parse_error: Option<String>,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    pub can_undo:    bool,
    pub can_redo:    bool,
}

/// Type-tagged primitive value coming from the frontend. The discriminant
/// chooses which `RonAst` variant to install — we *don't* try to be clever
/// and coerce numbers from strings, etc. The frontend is expected to send
/// the right variant for the node's current kind.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PrimitiveValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Char(char),
}

#[derive(Debug, Serialize, Clone)]
pub struct NodeView {
    /// Label shown in the row. For root: `"$"`; struct fields: the field
    /// name; map keys: stringified key; list/tuple: index as string.
    pub key:         String,
    /// Absolute path from root.
    pub path:        Vec<String>,
    pub kind:        NodeKind,
    pub preview:     String,
    pub child_count: usize,
    /// For NamedStruct / NamedTuple / UnitVariant: the variant or
    /// type tag preserved from source (e.g. "Action", "Dark"). `None`
    /// for everything else.
    pub variant_tag: Option<String>,
}

const PREVIEW_MAX_CHARS: usize = 64;

/// Cap on query results — mirrors JSON Studio. Recursive descent on a
/// real game-config doc can yield thousands of hits which would overwhelm
/// both the IPC channel and the hit-list UI; the user sees a `(capped)`
/// badge when this fires so it's clear results were truncated.
const QUERY_MAX_HITS: usize = 500;

/// One row in the query hit list. `path` is the AST-path (struct fields
/// by name, list/tuple by index-as-string, map by key-as-string, Option
/// via synthetic `"Some"`) — same encoding the tree-jump helper consumes.
/// `variant_tag` is populated for `named_struct` / `named_tuple` /
/// `unit_variant` so the UI can label hits as `Goblin(…)` etc.
#[derive(Debug, Serialize, Clone)]
pub struct RonQueryHit {
    pub path:        Vec<String>,
    pub kind:        NodeKind,
    pub preview:     String,
    pub variant_tag: Option<String>,
}

impl RonStudioRegistry {
    /// Full RFC 9535 JSONPath over the parsed RON AST, projected to JSON
    /// for `serde_json_path`. The projection preserves enum/struct tags
    /// as synthetic `$type` (named struct) / `$tag` (named tuple) fields
    /// and wraps `Option::Some(x)` as `{"Some": x}` so the resulting hit
    /// paths line up with the tree's `Some` synthetic-segment scheme.
    ///
    /// Synthetic projection-only segments (`$type`, `$tag`, `$items`) are
    /// transparently stripped from result paths — `[?@.$type == "Goblin"]`
    /// matches a struct value and the returned path points at the struct
    /// itself, not at its synthetic `$type` field. Hits whose final
    /// segment can't be resolved against the live AST (the only way
    /// that happens in practice is a query landing on a synthetic
    /// metadata field) are skipped.
    ///
    /// Examples (same shape as JSON Studio's query syntax):
    ///
    ///   `$`                                — root
    ///   `$.units[0].name`                  — field chain
    ///   `$..id`                            — every `id` anywhere
    ///   `$..units[?@.level > 10]`          — filter on numeric field
    ///   `$..[?@.$type == "Goblin"]`        — every struct/enum-variant named Goblin
    ///   `$..element[?@ == "Dark"]`         — every `element` equal to a unit variant
    ///   `$..*[?match(@.name, "^G.*")]`     — regex match on a field
    pub fn query(&self, doc_id: &str, expr: &str) -> Result<Vec<RonQueryHit>> {
        let doc = self.doc(doc_id)?;
        let root = match &doc.parsed {
            Some(v) => v,
            None    => return Ok(Vec::new()), // document doesn't parse — nothing to query
        };
        query_ast(root, expr)
    }
}

/// Free-standing JSONPath query — same semantics as
/// `RonStudioRegistry::query` but takes a parsed `RonAst` directly,
/// so the project-wide F13 bulk-edit flow can run the same query
/// over every file without paying the doc-id bookkeeping cost.
pub fn query_ast(root: &RonAst, expr: &str) -> Result<Vec<RonQueryHit>> {
    let normalised = normalise_query(expr);
    if normalised.is_empty() { return Ok(Vec::new()); }

    let projection = project_for_query(root);
    let path = JsonPath::parse(&normalised)
        .map_err(|e| AppError::Other(format!("Query parse error: {e}")))?;
    let located = path.query_located(&projection);

    let mut hits = Vec::<RonQueryHit>::with_capacity(QUERY_MAX_HITS.min(located.len()));
    let mut seen = std::collections::HashSet::<String>::new();
    for ln in located.iter() {
        if hits.len() >= QUERY_MAX_HITS { break; }
        let raw_path: Vec<String> = ln.location().iter().map(|el| match el {
            PathElement::Name(s)  => s.to_string(),
            PathElement::Index(i) => i.to_string(),
        }).collect();
        let ast_path = strip_synthetic_segments(&raw_path);
        let target = match resolve_path(root, &ast_path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let key = ast_path.join("\x00");
        if !seen.insert(key) { continue; }
        hits.push(RonQueryHit {
            path:        ast_path,
            kind:        node_kind(target),
            preview:     preview_for(target),
            variant_tag: variant_tag(target),
        });
    }
    Ok(hits)
}

/// Smooth common user inputs into valid JSONPath expressions before
/// handing them to `serde_json_path`. Same shorthands as JSON Studio:
///
///   `foo`              → `$..foo`     (recursive descent on a name)
///   `.foo` / `[0]`     → `$.foo` / `$[0]`
///   `users[?@...]`     → `$.users[?@...]`
///
/// We do NOT try to fix arbitrary invalid expressions — anything we
/// can't recognise is passed through so the engine produces an honest
/// error message.
fn normalise_query(expr: &str) -> String {
    let s = expr.trim();
    if s.is_empty() || s == "$" {
        return s.to_string();
    }
    if s.starts_with('$') {
        return s.to_string();
    }
    if s.starts_with('.') || s.starts_with('[') {
        return format!("${}", s);
    }
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return format!("$..{}", s);
    }
    if s.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_') {
        return format!("$.{}", s);
    }
    s.to_string()
}

/// Project a `RonAst` into a JSON value suitable for `serde_json_path`.
/// Crucially this differs from `ast::to_json` (which is the user-facing
/// RON→JSON conversion) in two ways:
///
///  1. **Tag preservation.** Named struct → `{"$type": name, ...fields}`,
///     named tuple → `{"$tag": name, "$items": [...]}`. Lets the user
///     filter by variant: `$..[?@.$type == "Goblin"]`. `ast::to_json`
///     already does this for structs (compatible) and tuples (we'd
///     have collided with its `$tag`/`$items` keys — same scheme).
///
///  2. **Option wrap.** `Option::Some(x)` → `{"Some": project(x)}`,
///     keeping the synthetic `Some` segment in result paths so they
///     line up with the tree's path scheme. `ast::to_json` unwraps
///     to the inner value, which would lose the `Some` step.
fn project_for_query(v: &RonAst) -> JsonValue {
    use JsonValue as J;
    match v {
        RonAst::Unit          => J::Null,
        RonAst::Bool(b)       => J::Bool(*b),
        RonAst::Char(c)       => J::String(c.to_string()),
        RonAst::Int(i)        => J::Number((*i).into()),
        RonAst::Float(f)      => serde_json::Number::from_f64(*f).map(J::Number).unwrap_or(J::Null),
        RonAst::String(s)     => J::String(s.clone()),
        RonAst::Option(None)  => J::Null,
        RonAst::Option(Some(inner)) => {
            let mut obj = serde_json::Map::new();
            obj.insert("Some".into(), project_for_query(inner.as_ref()));
            J::Object(obj)
        }
        RonAst::UnitVariant(name) => J::String(name.clone()),
        RonAst::List(items)   => J::Array(items.iter().map(project_for_query).collect()),
        RonAst::Map(pairs)    => {
            let mut obj = serde_json::Map::new();
            for (k, v) in pairs {
                obj.insert(key_to_string(k), project_for_query(v));
            }
            J::Object(obj)
        }
        RonAst::Struct { name, fields } => {
            let mut obj = serde_json::Map::new();
            if let Some(n) = name { obj.insert("$type".into(), J::String(n.clone())); }
            for (k, v) in fields { obj.insert(k.clone(), project_for_query(v)); }
            J::Object(obj)
        }
        RonAst::Tuple { name, items } => {
            if let Some(n) = name {
                let mut obj = serde_json::Map::new();
                obj.insert("$tag".into(),   J::String(n.clone()));
                obj.insert("$items".into(), J::Array(items.iter().map(project_for_query).collect()));
                J::Object(obj)
            } else {
                J::Array(items.iter().map(project_for_query).collect())
            }
        }
    }
}

/// Drop projection-only segments (`$items`) so the result path lines up
/// with the live AST. `$type` / `$tag` are not real path segments either
/// — a hit landing *on* one of them is discarded upstream in `query()`.
fn strip_synthetic_segments(p: &[String]) -> Vec<String> {
    p.iter().filter(|s| s.as_str() != "$items").cloned().collect()
}

impl RonStudioRegistry {
    pub fn parse(
        &mut self,
        text:           String,
        source_path:    Option<String>,
        encoding_label: String,
        had_bom:        bool,
    ) -> Result<ParseResult> {
        let size = text.len();
        let (parsed, parse_error) = try_parse(&text);
        let (root_kind, child_count) = match &parsed {
            Some(v) => (Some(node_kind(v)), container_len(v)),
            None    => (None, 0),
        };
        // Detect schema hint before we move the strings around. Inline
        // directive takes precedence; falls back to .ron-studio.toml.
        let schema_hint = detect_schema_hint(&text, source_path.as_deref());
        let id = Uuid::new_v4().to_string();
        self.docs.insert(id.clone(), Doc {
            source_path: source_path.clone(),
            encoding_label,
            had_bom,
            original:    text.clone(),
            current:     text.clone(),
            parsed,
            parse_error: parse_error.clone(),
            indent:      "  ".to_string(),
            history:     vec![text],
            history_pos: 0,
            history_coalesce_armed: false,
            last_push:   Instant::now(),
        });
        Ok(ParseResult {
            doc_id:      id,
            size_bytes:  size,
            source_path,
            original:    String::new(),
            parse_error,
            root_kind,
            child_count,
            schema_hint,
        })
    }

    pub fn close(&mut self, doc_id: &str) {
        self.docs.remove(doc_id);
    }

    pub fn set_text(&mut self, doc_id: &str, text: String) -> Result<UpdateResult> {
        let doc = self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown RON Studio doc: {doc_id}")))?;
        // Record before mutating so coalescing reads the previous
        // `last_push` timestamp; `record_history` updates it.
        record_history(doc, &text, /* can_coalesce */ true);
        doc.current = text;
        let (parsed, parse_error) = try_parse(&doc.current);
        let (root_kind, child_count) = match &parsed {
            Some(v) => (Some(node_kind(v)), container_len(v)),
            None    => (None, 0),
        };
        doc.parsed = parsed;
        doc.parse_error = parse_error.clone();
        let can_undo = doc.history_pos > 0;
        let can_redo = doc.history_pos + 1 < doc.history.len();
        Ok(UpdateResult { parse_error, root_kind, child_count, can_undo, can_redo })
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

    /// `(encoding_label, had_bom)` for `doc_id` — used at save time to
    /// round-trip legacy encodings (windows-1252, UTF-16 BOM, …).
    pub fn encoding_info(&self, doc_id: &str) -> Result<(String, bool)> {
        let d = self.doc(doc_id)?;
        Ok((d.encoding_label.clone(), d.had_bom))
    }

    /// Replace a primitive value (Bool/Int/Float/String/Char) at `path`.
    /// The frontend sends a type-tagged value matching the node's current
    /// kind. The document text is regenerated by pretty-printing the AST —
    /// this loses comments and re-flows whitespace (which the UI warns
    /// about via a one-shot banner before the first tree edit).
    pub fn mutate_primitive(
        &mut self,
        doc_id: &str,
        path:   &[String],
        value:  PrimitiveValue,
    ) -> Result<MutateResult> {
        self.mutate(doc_id, |root| {
            let target = resolve_path_mut(root, path)?;
            *target = match value {
                PrimitiveValue::Bool(b)   => RonAst::Bool(b),
                PrimitiveValue::Int(i)    => RonAst::Int(i),
                PrimitiveValue::Float(f)  => RonAst::Float(f),
                PrimitiveValue::String(s) => RonAst::String(s),
                PrimitiveValue::Char(c)   => RonAst::Char(c),
            };
            Ok(())
        })
    }

    /// Toggle an Option node between `None` and `Some(<default>)`. The
    /// default for the re-materialised `Some` is `Unit` — the frontend
    /// can immediately follow up with a primitive set when the user
    /// picks a concrete value. (When schema is available we'll fill in
    /// a smarter default; that's a separate slice.)
    pub fn toggle_option(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        self.mutate(doc_id, |root| {
            let target = resolve_path_mut(root, path)?;
            match target {
                RonAst::Option(None) => {
                    *target = RonAst::Option(Some(Box::new(RonAst::Unit)));
                }
                RonAst::Option(Some(_)) => {
                    *target = RonAst::Option(None);
                }
                _ => return Err(AppError::Other("Node is not an Option".into())),
            }
            Ok(())
        })
    }

    /// Replace the subtree at `path` with a freshly-parsed RON snippet.
    /// The snippet is parsed in isolation; failure to parse aborts the
    /// edit (no partial state). Used by the frontend to install new enum
    /// variants whose default payload it builds from the schema.
    pub fn replace_at(
        &mut self,
        doc_id:   &str,
        path:     &[String],
        ron_text: String,
    ) -> Result<MutateResult> {
        let snippet = ast::parse(&ron_text)
            .map_err(|e| AppError::Other(format!("Invalid RON snippet: {e}")))?;
        self.mutate(doc_id, |root| {
            if path.is_empty() {
                *root = snippet;
            } else {
                let target = resolve_path_mut(root, path)?;
                *target = snippet;
            }
            Ok(())
        })
    }

    /// Add a field `name = <ron_text>` to the struct at `path`. Refuses
    /// the edit when the path doesn't resolve to a struct, when the
    /// field already exists, or when the snippet doesn't parse. New
    /// fields are appended at the end — the frontend orders them in the
    /// detail pane "missing fields" list via the schema.
    pub fn insert_field(
        &mut self,
        doc_id:   &str,
        path:     &[String],
        name:     String,
        ron_text: String,
    ) -> Result<MutateResult> {
        let snippet = ast::parse(&ron_text)
            .map_err(|e| AppError::Other(format!("Invalid RON snippet: {e}")))?;
        self.mutate(doc_id, |root| {
            let target = resolve_path_mut(root, path)?;
            match target {
                RonAst::Struct { fields, .. } => {
                    if fields.iter().any(|(k, _)| k == &name) {
                        return Err(AppError::Other(format!("Field `{name}` already exists")));
                    }
                    fields.push((name, snippet));
                    Ok(())
                }
                _ => Err(AppError::Other("Can only add fields on a struct".into())),
            }
        })
    }

    /// Append an item to the list/tuple at `path`. Index of the new
    /// item is `len()` (the caller can `move_item` it afterwards).
    pub fn insert_item(
        &mut self,
        doc_id:   &str,
        path:     &[String],
        ron_text: String,
    ) -> Result<MutateResult> {
        let snippet = ast::parse(&ron_text)
            .map_err(|e| AppError::Other(format!("Invalid RON snippet: {e}")))?;
        self.mutate(doc_id, |root| {
            let target = resolve_path_mut(root, path)?;
            match target {
                RonAst::List(items) | RonAst::Tuple { items, .. } => {
                    items.push(snippet);
                    Ok(())
                }
                _ => Err(AppError::Other("Can only append items on a list or tuple".into())),
            }
        })
    }

    /// Insert a `key: value` pair on the map at `path`. Both sides are
    /// parsed as RON snippets so the key may be any valid map-key shape
    /// (string, int, char, …) — matches how the user would type it.
    pub fn insert_map_entry(
        &mut self,
        doc_id:    &str,
        path:      &[String],
        key_text:  String,
        val_text:  String,
    ) -> Result<MutateResult> {
        let key = ast::parse(&key_text)
            .map_err(|e| AppError::Other(format!("Invalid RON key snippet: {e}")))?;
        let val = ast::parse(&val_text)
            .map_err(|e| AppError::Other(format!("Invalid RON value snippet: {e}")))?;
        self.mutate(doc_id, |root| {
            let target = resolve_path_mut(root, path)?;
            match target {
                RonAst::Map(pairs) => {
                    let key_repr = key_to_string(&key);
                    if pairs.iter().any(|(k, _)| key_to_string(k) == key_repr) {
                        return Err(AppError::Other(format!("Map key `{key_repr}` already exists")));
                    }
                    pairs.push((key, val));
                    Ok(())
                }
                _ => Err(AppError::Other("Can only insert entries on a map".into())),
            }
        })
    }

    /// Duplicate the field / list-item / map-entry at `path`, inserting
    /// the clone immediately after the original. Field clones suffix
    /// `_copy` to the name to avoid the duplicate-key check; map clones
    /// suffix `"_copy"` to string keys (other key kinds are rejected
    /// since we can't synthesise a guaranteed-unique value).
    pub fn duplicate_at(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot duplicate the document root".into()));
        }
        let (parent_path, last) = path.split_at(path.len() - 1);
        let key = last[0].clone();
        self.mutate(doc_id, |root| {
            let parent = resolve_path_mut(root, parent_path)?;
            duplicate_in_parent(parent, &key)
        })
    }

    /// Swap a list/tuple item with its neighbour. `delta = -1` moves it
    /// up, `delta = 1` moves it down. Out-of-range moves are a no-op
    /// (return `Ok` without touching the document).
    pub fn move_item(&mut self, doc_id: &str, path: &[String], delta: i32) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot move the document root".into()));
        }
        let (parent_path, last) = path.split_at(path.len() - 1);
        let key = last[0].clone();
        self.mutate(doc_id, |root| {
            let parent = resolve_path_mut(root, parent_path)?;
            move_item_in_parent(parent, &key, delta)
        })
    }

    /// Remove the field / list-item / tuple-item / map-entry referenced
    /// by the *last* path segment, leaving the rest of the document
    /// untouched. Removing from an `Option` is rejected — the caller
    /// should `toggle_option` instead. Removing from a tuple changes its
    /// arity; the caller is responsible for not breaking the schema.
    pub fn remove_at(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot remove the document root".into()));
        }
        let (parent_path, last) = path.split_at(path.len() - 1);
        let key = last[0].clone();
        self.mutate(doc_id, |root| {
            let parent = resolve_path_mut(root, parent_path)?;
            remove_from_parent(parent, &key)
        })
    }

    /// Shared mutate-and-regenerate path. The closure mutates the AST
    /// in place; we then pretty-print the new tree, re-parse to populate
    /// the cache fields, and return both the new text and the parse
    /// status. (Pretty-print is total — re-parse can't actually fail —
    /// but we surface the result shape symmetrically with `set_text`.)
    /// F13 — apply a batch of `BulkEditOp`s to an open doc. Recorded
    /// as a single discrete history entry (matching the user's mental
    /// model: one bulk edit = one undo). Reuses the `mutate(...)`
    /// pipeline so the regenerated text + history + parse state stay
    /// in lock-step.
    pub fn apply_bulk_edits_doc(
        &mut self,
        doc_id: &str,
        ops:    &[(Vec<String>, BulkEditOp)],
    ) -> Result<MutateResult> {
        self.mutate(doc_id, |root| apply_bulk_edits_inplace(root, ops))
    }

    fn mutate<F>(&mut self, doc_id: &str, op: F) -> Result<MutateResult>
    where
        F: FnOnce(&mut RonAst) -> Result<()>,
    {
        let doc = self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown RON Studio doc: {doc_id}")))?;
        let indent = doc.indent.clone();
        let root = doc.parsed.as_mut()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot edit tree".into()))?;
        op(root)?;
        let new_text = ast::to_pretty_string_with(root, &indent);
        // Tree mutations are discrete user actions — never coalesce
        // with each other or with surrounding typing. Recording before
        // we overwrite `current` so the snapshot is correctly the new
        // state.
        record_history(doc, &new_text, /* can_coalesce */ false);
        doc.current = new_text.clone();
        let (parsed, parse_error) = try_parse(&doc.current);
        let (root_kind, child_count) = match &parsed {
            Some(v) => (Some(node_kind(v)), container_len(v)),
            None    => (None, 0),
        };
        doc.parsed = parsed;
        doc.parse_error = parse_error.clone();
        let can_undo = doc.history_pos > 0;
        let can_redo = doc.history_pos + 1 < doc.history.len();
        Ok(MutateResult { text: new_text, parse_error, root_kind, child_count, can_undo, can_redo })
    }

    /// Move backward one step in the history stack. Returns the
    /// reverted state shaped exactly like a tree edit, so the frontend
    /// can reuse the existing post-mutation refresh path. Errors when
    /// already at the oldest snapshot — callers should gate the
    /// keyboard shortcut and the button on `can_undo` to avoid that.
    pub fn undo(&mut self, doc_id: &str) -> Result<MutateResult> {
        let doc = self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown RON Studio doc: {doc_id}")))?;
        if doc.history_pos == 0 {
            return Err(AppError::Other("Nothing to undo".into()));
        }
        doc.history_pos -= 1;
        Self::apply_history_cursor(doc)
    }

    pub fn redo(&mut self, doc_id: &str) -> Result<MutateResult> {
        let doc = self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown RON Studio doc: {doc_id}")))?;
        if doc.history_pos + 1 >= doc.history.len() {
            return Err(AppError::Other("Nothing to redo".into()));
        }
        doc.history_pos += 1;
        Self::apply_history_cursor(doc)
    }

    /// Internal — push the snapshot at `history_pos` into `current` and
    /// rebuild the parse cache. Used by both undo and redo so the
    /// frontend gets the same MutateResult shape either way.
    fn apply_history_cursor(doc: &mut Doc) -> Result<MutateResult> {
        let text = doc.history[doc.history_pos].clone();
        doc.current = text.clone();
        let (parsed, parse_error) = try_parse(&doc.current);
        let (root_kind, child_count) = match &parsed {
            Some(v) => (Some(node_kind(v)), container_len(v)),
            None    => (None, 0),
        };
        doc.parsed = parsed;
        doc.parse_error = parse_error.clone();
        // Break the coalesce run so a redo immediately followed by
        // typing creates a fresh history entry (instead of clobbering
        // the redo target).
        doc.history_coalesce_armed = false;
        doc.last_push = Instant::now();
        let can_undo = doc.history_pos > 0;
        let can_redo = doc.history_pos + 1 < doc.history.len();
        Ok(MutateResult { text, parse_error, root_kind, child_count, can_undo, can_redo })
    }

    pub fn history_state(&self, doc_id: &str) -> Result<(bool, bool)> {
        let doc = self.doc(doc_id)?;
        Ok((doc.history_pos > 0, doc.history_pos + 1 < doc.history.len()))
    }

    /// Full state snapshot for a doc, used by the workspace tab bar
    /// when switching to an already-open doc — one round-trip vs the
    /// several separate getters it would otherwise need.
    pub fn snapshot(&self, doc_id: &str) -> Result<DocSnapshot> {
        let doc = self.doc(doc_id)?;
        let (root_kind, child_count) = match &doc.parsed {
            Some(v) => (Some(node_kind(v)), container_len(v)),
            None    => (None, 0),
        };
        Ok(DocSnapshot {
            doc_id:      doc_id.to_string(),
            source_path: doc.source_path.clone(),
            size_bytes:  doc.current.len(),
            original:    doc.original.clone(),
            current:     doc.current.clone(),
            parse_error: doc.parse_error.clone(),
            root_kind,
            child_count,
            can_undo:    doc.history_pos > 0,
            can_redo:    doc.history_pos + 1 < doc.history.len(),
            indent:      doc.indent.clone(),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct DocSnapshot {
    pub doc_id:      String,
    pub source_path: Option<String>,
    pub size_bytes:  usize,
    pub original:    String,
    pub current:     String,
    pub parse_error: Option<String>,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    pub can_undo:    bool,
    pub can_redo:    bool,
    pub indent:      String,
}

/// Recursively list `.ron` files under `folder`, skipping common
/// vendor/build directories. Returns paths relative to `folder` plus
/// full absolute paths for opening.
pub fn list_ron_files(folder: &str) -> Result<Vec<RonFileEntry>> {
    let root = std::path::Path::new(folder);
    if !root.is_dir() {
        return Err(AppError::Other(format!("Not a directory: {folder}")));
    }
    let mut out = Vec::new();
    walk_ron_files(root, root, &mut out, 0);
    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct RonFileEntry {
    pub absolute_path: String,
    pub relative_path: String,
    pub name:          String,
    pub size_bytes:    u64,
}

fn walk_ron_files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<RonFileEntry>, depth: usize) {
    // Hard cap on recursion to avoid runaway walks (symlinks, weird FS).
    if depth > 16 { return; }
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // Skip hidden + common build/vendor folders so the tree stays
        // clean inside Cargo projects.
        if name.starts_with('.') { continue; }
        if matches!(name, "target" | "node_modules" | ".git" | "dist" | "build") { continue; }
        if path.is_dir() {
            walk_ron_files(root, &path, out, depth + 1);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ron") {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(RonFileEntry {
                absolute_path: path.to_string_lossy().into_owned(),
                relative_path: rel.to_string_lossy().into_owned(),
                name:          name.to_string(),
                size_bytes:    size,
            });
        }
    }
}

impl RonStudioRegistry {
    pub fn rebind_source(&mut self, doc_id: &str, new_path: String) -> Result<()> {
        let doc = self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown RON Studio doc: {doc_id}")))?;
        doc.source_path = Some(new_path);
        Ok(())
    }

    pub fn mark_saved(&mut self, doc_id: &str) -> Result<()> {
        let doc = self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown RON Studio doc: {doc_id}")))?;
        doc.original = doc.current.clone();
        Ok(())
    }

    fn doc(&self, doc_id: &str) -> Result<&Doc> {
        self.docs.get(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown RON Studio doc: {doc_id}")))
    }

    pub fn get_root(&self, doc_id: &str) -> Result<Option<NodeView>> {
        let doc = self.doc(doc_id)?;
        Ok(doc.parsed.as_ref().map(|v| view_for("$".to_string(), Vec::new(), v)))
    }

    pub fn get_children(&self, doc_id: &str, path: &[String]) -> Result<Vec<NodeView>> {
        let doc = self.doc(doc_id)?;
        let v = match &doc.parsed { Some(v) => v, None => return Ok(Vec::new()) };
        let target = resolve_path(v, path)?;
        Ok(children_of(path, target))
    }

    pub fn get_value_pretty(&self, doc_id: &str, path: &[String]) -> Result<String> {
        let doc = self.doc(doc_id)?;
        let v = doc.parsed.as_ref().ok_or_else(|| AppError::Other("Document has parse errors".into()))?;
        let target = resolve_path(v, path)?;
        Ok(ast::to_pretty_string(target))
    }

    pub fn format(&self, doc_id: &str) -> Result<String> {
        let doc = self.doc(doc_id)?;
        let v = doc.parsed.as_ref().ok_or_else(|| AppError::Other("Document has parse errors — cannot format".into()))?;
        Ok(ast::to_pretty_string_with(v, &doc.indent))
    }

    pub fn get_indent(&self, doc_id: &str) -> Result<String> {
        Ok(self.doc(doc_id)?.indent.clone())
    }

    pub fn set_indent(&mut self, doc_id: &str, indent: String) -> Result<()> {
        let doc = self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown RON Studio doc: {doc_id}")))?;
        // Accept any non-empty string — `"  "`, `"    "`, `"\t"`, even
        // weird mixes are valid as far as the parser cares (it ignores
        // whitespace between tokens). Rejected when empty to avoid
        // ambiguous round-trips.
        if indent.is_empty() {
            return Err(AppError::Other("Indent cannot be empty".into()));
        }
        doc.indent = indent;
        Ok(())
    }

    pub fn to_json(&self, doc_id: &str) -> Result<String> {
        let doc = self.doc(doc_id)?;
        let v = doc.parsed.as_ref().ok_or_else(|| AppError::Other("Document has parse errors — cannot convert".into()))?;
        serde_json::to_string_pretty(&ast::to_json(v))
            .map_err(|e| AppError::Other(format!("JSON serialize: {e}")))
    }

    pub fn from_json(&self, doc_id: &str, json_text: &str) -> Result<String> {
        let indent = self.doc(doc_id).map(|d| d.indent.clone()).unwrap_or_else(|_| "  ".to_string());
        let j: serde_json::Value = serde_json::from_str(json_text)
            .map_err(|e| AppError::Other(format!("JSON parse: {e}")))?;
        let v = json_to_ron(&j);
        Ok(ast::to_pretty_string_with(&v, &indent))
    }
}

// ── Diff (original vs current) ─────────────────────────────────────────────

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Serialize)]
pub struct DiffLine {
    pub kind:     DiffLineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text:     String,
}

#[derive(Debug, Serialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines:     Vec<DiffLine>,
}

impl RonStudioRegistry {
    pub fn diff(&self, doc_id: &str) -> Result<Vec<DiffHunk>> {
        let doc = self.doc(doc_id)?;
        Ok(compute_diff_hunks(&doc.original, &doc.current))
    }
}

fn compute_diff_hunks(old: &str, new: &str) -> Vec<DiffHunk> {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(old, new);
    let mut hunks = Vec::<DiffHunk>::new();
    for group in diff.grouped_ops(3) {
        let mut lines = Vec::<DiffLine>::new();
        let mut old_start: u32 = u32::MAX;
        let mut new_start: u32 = u32::MAX;
        let mut old_end:   u32 = 0;
        let mut new_end:   u32 = 0;
        for op in &group {
            for change in diff.iter_changes(op) {
                let kind = match change.tag() {
                    ChangeTag::Equal  => DiffLineKind::Context,
                    ChangeTag::Insert => DiffLineKind::Add,
                    ChangeTag::Delete => DiffLineKind::Del,
                };
                let old_line = change.old_index().map(|i| (i as u32) + 1);
                let new_line = change.new_index().map(|i| (i as u32) + 1);
                if let Some(n) = old_line {
                    if n < old_start { old_start = n; }
                    if n + 1 > old_end { old_end = n + 1; }
                }
                if let Some(n) = new_line {
                    if n < new_start { new_start = n; }
                    if n + 1 > new_end { new_end = n + 1; }
                }
                lines.push(DiffLine {
                    kind,
                    old_line,
                    new_line,
                    text: strip_trailing_newline(change.value()).to_string(),
                });
            }
        }
        if old_start == u32::MAX { old_start = 0; }
        if new_start == u32::MAX { new_start = 0; }
        hunks.push(DiffHunk {
            old_start,
            old_count: old_end.saturating_sub(old_start),
            new_start,
            new_count: new_end.saturating_sub(new_start),
            lines,
        });
    }
    hunks
}

fn strip_trailing_newline(s: &str) -> &str {
    let s = s.strip_suffix('\n').unwrap_or(s);
    s.strip_suffix('\r').unwrap_or(s)
}

// ── Tree diff (structural) ─────────────────────────────────────────────────
//
// Path-aware diff between the document's *original* (load/save snapshot)
// and the *current* AST. Output is a recursive tree mirroring the
// document's shape, but pruned to the changed branches and annotated
// with per-node status + change-count totals. Containers with no
// changed descendants are reported as Unchanged (and pruned from their
// parent) so the resulting tree is small even for big documents that
// only changed a couple of leaves.
//
// Lives alongside the text diff (`compute_diff_hunks`) — the frontend
// toggles between the two depending on what the user wants to see.

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    /// Both sides equal — pruned from the parent before being sent.
    Unchanged,
    /// New leaf or subtree in `current` that didn't exist in `original`.
    Added,
    /// Leaf or subtree in `original` that's gone in `current`.
    Removed,
    /// Different leaf value at the same path; or two containers of
    /// fundamentally different shape that we don't recurse into.
    Modified,
    /// Container whose own value is "unchanged in shape" but at least one
    /// descendant differs. UI uses this to render the container with a
    /// change-count badge and recurse into the trimmed `children`.
    Partial,
}

#[derive(Debug, Serialize)]
pub struct DiffTreeNode {
    pub key:             String,
    pub path:            Vec<String>,
    pub status:          DiffStatus,
    pub kind_before:     Option<NodeKind>,
    pub kind_after:      Option<NodeKind>,
    pub preview_before:  Option<String>,
    pub preview_after:   Option<String>,
    /// Variant / struct-type tag preserved from source — same role as
    /// `NodeView.variant_tag` so the UI can render `Action(…)` etc. on
    /// each side independently.
    pub tag_before:      Option<String>,
    pub tag_after:       Option<String>,
    /// Only populated for `Partial` containers (and for `Added` /
    /// `Removed` we keep empty — the preview alone is enough). Unchanged
    /// siblings are filtered out at construction time so the tree shows
    /// only what changed.
    pub children:        Vec<DiffTreeNode>,
    /// Number of leaf-level changes inside this subtree (1 for any
    /// Added/Removed/Modified leaf, sum of children otherwise). Powers
    /// the header summary and aggregate badges on collapsed containers.
    pub change_count:    u32,
}

impl RonStudioRegistry {
    pub fn tree_diff(&self, doc_id: &str) -> Result<DiffTreeNode> {
        let doc = self.doc(doc_id)?;
        // The original text is parsed on demand — diff is a comparatively
        // rare call (only when the user opens the Diff view) and we
        // don't want to add overhead to the hot edit path.
        let before = ast::parse(&doc.original).ok();
        // For `after` we trust the live parse cache; if the document
        // currently doesn't parse, treat it as "no after" — every leaf
        // becomes Removed. Matches what the user sees in Errors view.
        let after = doc.parsed.clone();
        Ok(diff_value(before.as_ref(), after.as_ref(), "$".to_string(), Vec::new()))
    }
}

fn diff_value(
    before: Option<&RonAst>,
    after:  Option<&RonAst>,
    key:    String,
    path:   Vec<String>,
) -> DiffTreeNode {
    match (before, after) {
        (Some(b), Some(a)) => {
            if b == a {
                return diff_leaf(key, path, Some(b), Some(a), DiffStatus::Unchanged, 0);
            }
            if let Some(children) = recurse_children(b, a, &path) {
                let total: u32 = children.iter().map(|c| c.change_count).sum();
                if total == 0 {
                    return diff_leaf(key, path, Some(b), Some(a), DiffStatus::Unchanged, 0);
                }
                let pruned: Vec<DiffTreeNode> = children
                    .into_iter()
                    .filter(|c| c.status != DiffStatus::Unchanged)
                    .collect();
                return DiffTreeNode {
                    key,
                    path,
                    status:         DiffStatus::Partial,
                    kind_before:    Some(node_kind(b)),
                    kind_after:     Some(node_kind(a)),
                    preview_before: None,
                    preview_after:  None,
                    tag_before:     variant_tag(b),
                    tag_after:      variant_tag(a),
                    children:       pruned,
                    change_count:   total,
                };
            }
            // Fundamentally different shape — render as a leaf with
            // before/after previews side by side.
            diff_leaf(key, path, Some(b), Some(a), DiffStatus::Modified, 1)
        }
        (Some(b), None) => diff_leaf(key, path, Some(b), None, DiffStatus::Removed,  1),
        (None, Some(a)) => diff_leaf(key, path, None,    Some(a), DiffStatus::Added, 1),
        (None, None)    => diff_leaf(key, path, None,    None,    DiffStatus::Unchanged, 0),
    }
}

fn diff_leaf(
    key:     String,
    path:    Vec<String>,
    before:  Option<&RonAst>,
    after:   Option<&RonAst>,
    status:  DiffStatus,
    cc:      u32,
) -> DiffTreeNode {
    DiffTreeNode {
        key, path, status,
        kind_before:    before.map(node_kind),
        kind_after:     after.map(node_kind),
        preview_before: before.map(preview_for),
        preview_after:  after.map(preview_for),
        tag_before:     before.and_then(variant_tag),
        tag_after:      after.and_then(variant_tag),
        children:       Vec::new(),
        change_count:   cc,
    }
}

/// Returns Some(children-diff) if the two nodes are the same container
/// *shape* (so we can recurse). None means "different shape" — caller
/// treats the pair as a Modified leaf.
///
/// For Struct/Tuple, name-matching is part of the shape (so changing a
/// variant from `Dark` to `Light` is one Modified leaf, not a recurse
/// into the variant's contents).
fn recurse_children(
    b:    &RonAst,
    a:    &RonAst,
    path: &[String],
) -> Option<Vec<DiffTreeNode>> {
    use RonAst::*;
    match (b, a) {
        (Struct { name: bn, fields: bf }, Struct { name: an, fields: af }) if bn == an => {
            Some(diff_named_pairs(bf, af, path))
        }
        (Tuple { name: bn, items: bi }, Tuple { name: an, items: ai }) if bn == an => {
            Some(diff_indexed_items(bi, ai, path))
        }
        (List(bi), List(ai)) => Some(diff_indexed_items(bi, ai, path)),
        (Map(bp), Map(ap))   => Some(diff_map_pairs(bp, ap, path)),
        (Option(bb), Option(aa)) => {
            // Match by the synthetic "Some" key — mirrors how `children_of`
            // exposes the inner so the UI can step into it via the path.
            let mut child_path = path.to_vec();
            child_path.push("Some".into());
            match (bb, aa) {
                (Some(bv), Some(av)) => Some(vec![diff_value(Some(bv.as_ref()), Some(av.as_ref()), "Some".into(), child_path)]),
                (Some(bv), None)     => Some(vec![diff_value(Some(bv.as_ref()), None,              "Some".into(), child_path)]),
                (None, Some(av))     => Some(vec![diff_value(None,              Some(av.as_ref()), "Some".into(), child_path)]),
                (None, None)         => Some(Vec::new()),
            }
        }
        _ => None,
    }
}

fn diff_named_pairs(b: &[(String, RonAst)], a: &[(String, RonAst)], path: &[String]) -> Vec<DiffTreeNode> {
    let mut out  = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();
    // Preserve `after` order so the rendered diff reads like the
    // current document, with `Removed` fields appended at the end.
    for (k, av) in a {
        seen.insert(k.clone());
        let bv = b.iter().find(|(bk, _)| bk == k).map(|(_, v)| v);
        let mut p = path.to_vec(); p.push(k.clone());
        out.push(diff_value(bv, Some(av), k.clone(), p));
    }
    for (k, bv) in b {
        if seen.contains(k) { continue; }
        let mut p = path.to_vec(); p.push(k.clone());
        out.push(diff_value(Some(bv), None, k.clone(), p));
    }
    out
}

fn diff_indexed_items(b: &[RonAst], a: &[RonAst], path: &[String]) -> Vec<DiffTreeNode> {
    let n = b.len().max(a.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let key = i.to_string();
        let mut p = path.to_vec(); p.push(key.clone());
        out.push(diff_value(b.get(i), a.get(i), key, p));
    }
    out
}

fn diff_map_pairs(b: &[(RonAst, RonAst)], a: &[(RonAst, RonAst)], path: &[String]) -> Vec<DiffTreeNode> {
    let mut out  = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();
    for (k, av) in a {
        let ks = key_to_string(k);
        seen.insert(ks.clone());
        let bv = b.iter().find(|(bk, _)| key_to_string(bk) == ks).map(|(_, v)| v);
        let mut p = path.to_vec(); p.push(ks.clone());
        out.push(diff_value(bv, Some(av), ks, p));
    }
    for (k, bv) in b {
        let ks = key_to_string(k);
        if seen.contains(&ks) { continue; }
        let mut p = path.to_vec(); p.push(ks.clone());
        out.push(diff_value(Some(bv), None, ks, p));
    }
    out
}

// ── Schema-hint detection ──────────────────────────────────────────────────
//
// Two ways to wire a RON file to its schema without prompting:
//
//   1. **Inline directive** at the top of the .ron file (first ~20 lines,
//      first non-blank one wins):
//      ```
//      //! ron-studio: schema = "src/lib.rs", root = "crate::Config"
//      ```
//      Lives with the data, survives file moves. Best for one-off
//      schema bindings.
//
//   2. **Side-car config**: `.ron-studio.toml` in the file's folder or
//      any ancestor (closest wins). Supports a default + glob overrides:
//      ```toml
//      [default]
//      rs_file = "../src/lib.rs"
//      root_type = "crate::AssetData"
//
//      [[overrides]]
//      glob = "abilities/*.ron"
//      rs_file = "../src/ability/mod.rs"
//      root_type = "crate::ability::Ability"
//      ```
//      Paths are resolved relative to the `.ron-studio.toml`'s folder
//      and returned absolute. Best for whole-folder workspaces.

pub fn detect_schema_hint(text: &str, source_path: Option<&str>) -> Option<SchemaHint> {
    if let Some(h) = parse_inline_directive(text, source_path) {
        return Some(h);
    }
    if let Some(p) = source_path {
        return parse_sidecar_config(p);
    }
    None
}

fn parse_inline_directive(text: &str, source_path: Option<&str>) -> Option<SchemaHint> {
    for (i, line) in text.lines().enumerate() {
        if i >= 20 { break; }
        let t = line.trim_start();
        if t.is_empty() { continue; }
        // Accept both `//!` (Rust style) and `//` to match RON parsers'
        // line comments — the user might have either.
        let body = t.strip_prefix("//!").or_else(|| t.strip_prefix("//"))?;
        let body = body.trim();
        let body = body.strip_prefix("ron-studio:").map(|s| s.trim())?;
        // Two known keys: schema=... and root=... (comma-separated, =
        // optional whitespace, values may be quoted).
        let mut rs_file:   Option<String> = None;
        let mut root_type: Option<String> = None;
        for part in body.split(',') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix("schema") {
                rs_file = Some(parse_kv_value(rest));
            } else if let Some(rest) = part.strip_prefix("root") {
                root_type = Some(parse_kv_value(rest));
            }
        }
        let rs_file   = rs_file?;
        let root_type = root_type?;
        // Resolve rs_file relative to the source file when it's relative.
        let resolved = resolve_relative(&rs_file, source_path);
        return Some(SchemaHint {
            rs_file: resolved,
            root_type,
            origin: SchemaHintOrigin::Directive,
        });
    }
    None
}

fn parse_kv_value(after_key: &str) -> String {
    let v = after_key.trim_start_matches(|c: char| c == '=' || c.is_whitespace());
    v.trim().trim_matches(|c| c == '"' || c == '\'').to_string()
}

fn resolve_relative(p: &str, source_path: Option<&str>) -> String {
    let path = std::path::Path::new(p);
    if path.is_absolute() { return p.to_string(); }
    let Some(src) = source_path else { return p.to_string(); };
    let Some(parent) = std::path::Path::new(src).parent() else { return p.to_string(); };
    parent.join(path).to_string_lossy().into_owned()
}

fn parse_sidecar_config(source_path: &str) -> Option<SchemaHint> {
    let src = std::path::Path::new(source_path);
    let start = src.parent()?;
    // Walk upward up to ~12 levels — plenty for any sane project depth.
    // At each level try the new location (`.arbor/studio.toml`) FIRST
    // before falling back to the legacy `.ron-studio.toml`, so a
    // repo with both files in flight (e.g. mid-migration) reads the
    // canonical one.
    let mut cur: Option<&std::path::Path> = Some(start);
    for _ in 0..12 {
        let dir = cur?;
        let new_cfg = dir.join(".arbor").join("studio.toml");
        if new_cfg.is_file() {
            return parse_sidecar_file(&new_cfg, source_path);
        }
        let legacy_cfg = dir.join(".ron-studio.toml");
        if legacy_cfg.is_file() {
            return parse_sidecar_file(&legacy_cfg, source_path);
        }
        cur = dir.parent();
    }
    None
}

fn parse_sidecar_file(cfg_path: &std::path::Path, source_path: &str) -> Option<SchemaHint> {
    let text = std::fs::read_to_string(cfg_path).ok()?;
    let v: toml::Value = toml::from_str(&text).ok()?;
    let cfg_dir = cfg_path.parent()?;
    // Glob overrides win over the default — first match in source order.
    if let Some(overrides) = v.get("overrides").and_then(|o| o.as_array()) {
        for entry in overrides {
            let glob_str = entry.get("glob").and_then(|x| x.as_str()).unwrap_or("");
            if glob_str.is_empty() { continue; }
            if glob_match(glob_str, source_path, cfg_dir) {
                let rs    = entry.get("rs_file").and_then(|x| x.as_str())?;
                let root  = entry.get("root_type").and_then(|x| x.as_str())?;
                return Some(SchemaHint {
                    rs_file:   cfg_dir.join(rs).to_string_lossy().into_owned(),
                    root_type: root.to_string(),
                    origin:    SchemaHintOrigin::Sidecar,
                });
            }
        }
    }
    if let Some(def) = v.get("default") {
        let rs    = def.get("rs_file").and_then(|x| x.as_str())?;
        let root  = def.get("root_type").and_then(|x| x.as_str())?;
        return Some(SchemaHint {
            rs_file:   cfg_dir.join(rs).to_string_lossy().into_owned(),
            root_type: root.to_string(),
            origin:    SchemaHintOrigin::Sidecar,
        });
    }
    None
}

/// Lightweight glob match — supports `*` (any non-/ chars) and `**`
/// (any chars including /). Anchored to the start; the source path is
/// normalised relative to the cfg dir when possible.
fn glob_match(pattern: &str, source: &str, cfg_dir: &std::path::Path) -> bool {
    let src_path = std::path::Path::new(source);
    let rel = src_path.strip_prefix(cfg_dir).unwrap_or(src_path);
    let rel = rel.to_string_lossy().replace('\\', "/");
    let pat = pattern.replace('\\', "/");
    glob_match_segments(&pat, &rel)
}

fn glob_match_segments(pat: &str, s: &str) -> bool {
    // Convert glob to a tiny regex-free matcher. `**` → match anything,
    // `*` → match one segment, literals must match exactly. Recursive.
    let pb = pat.as_bytes();
    let sb = s.as_bytes();
    fn go(p: &[u8], s: &[u8]) -> bool {
        let mut pi = 0;
        let mut si = 0;
        while pi < p.len() {
            match p[pi] {
                b'*' if pi + 1 < p.len() && p[pi + 1] == b'*' => {
                    // `**` — try every tail of s
                    let rest = &p[pi + 2..];
                    let rest = if rest.starts_with(b"/") { &rest[1..] } else { rest };
                    for end in si..=s.len() {
                        if go(rest, &s[end..]) { return true; }
                    }
                    return false;
                }
                b'*' => {
                    let rest = &p[pi + 1..];
                    let mut end = si;
                    while end < s.len() && s[end] != b'/' {
                        end += 1;
                    }
                    for cut in si..=end {
                        if go(rest, &s[cut..]) { return true; }
                    }
                    return false;
                }
                c if si < s.len() && s[si] == c => {
                    pi += 1; si += 1;
                }
                _ => return false,
            }
        }
        si == s.len()
    }
    go(pb, sb)
}

// ── F12 — Rename refactor helpers ───────────────────────────────────────────

/// Apply a "set leaf string to `new_value`" at every AST path in
/// `paths`. Used by the project-wide rename refactor (FROZEN F12) on
/// files that are NOT necessarily open as user docs — keeps the
/// rename pipeline free of `RonStudioRegistry` doc-id bookkeeping.
///
/// Reformats the whole file via the same pretty-printer the regular
/// tree-edit path uses; comments/whitespace are lost (RON Studio
/// already advertises this trade-off via `supports_lossless_edit =
/// false`). `indent` defaults to `"  "` everywhere we don't have a
/// per-doc override (rename-target files that aren't open as docs).
///
/// Errors:
/// - Source `text` doesn't parse.
/// - One of the paths doesn't resolve OR doesn't point at a `String`
///   leaf (a defensive guard — the preview step shouldn't have
///   surfaced such a site, but better surfaced here than silently
///   skipping).
pub fn apply_string_rename(
    text:      &str,
    paths:     &[Vec<String>],
    new_value: &str,
    indent:    &str,
) -> Result<String> {
    let mut root = ast::parse(text)
        .map_err(|e| AppError::Other(format!("parse: {e}")))?;
    for path in paths {
        let target = resolve_path_mut(&mut root, path)?;
        match target {
            RonAst::String(_) => *target = RonAst::String(new_value.to_string()),
            other => {
                return Err(AppError::Other(format!(
                    "Rename site at {path:?} is not a string leaf (kind = {:?})",
                    node_kind(other),
                )));
            }
        }
    }
    Ok(ast::to_pretty_string_with(&root, indent))
}

// ── F13 — Query-driven bulk edit helpers ────────────────────────────

/// Concrete value to install at a `set` site. Maps roughly to
/// `studio::edit_expr::Value` but pre-validated against the site's
/// target kind (the bulk-edit preview already filters out skip-able
/// sites — this enum is just the typed payload the apply pass writes).
#[derive(Debug, Clone)]
pub enum BulkSetValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    /// For RON-option targets: produces `None`. The caller decides
    /// whether a non-option target accepts `Null` (it does NOT — the
    /// preview step rejects it).
    Null,
}

/// One edit op applied to the AST.
#[derive(Debug, Clone)]
pub enum BulkEditOp {
    /// Replace the value at `path`. Option semantics:
    /// `BulkSetValue::Null` on Option → `None`; any other value on
    /// Option → `Some(value)`. On non-option, `Null` is rejected.
    Set(BulkSetValue),
    /// Remove the node from its parent (struct field, list/tuple item,
    /// map entry). For Option, becomes `None` to preserve the field
    /// shape — "remove the option field from its parent struct" is
    /// a separate operation not exposed in v1.
    Delete,
}

/// Apply a batch of `BulkEditOp`s to a RON document text and return
/// the regenerated source. Used by both the active-doc and project-
/// wide flows of F13. Sets run before deletes; within a parent,
/// numeric (list/tuple) deletes go reverse-index-first so earlier
/// removals don't shift later indices.
///
/// Reformats the whole file via the pretty-printer — same lossy
/// trade-off as `apply_string_rename` (FROZEN F11: RON Studio doesn't
/// advertise lossless edit).
#[allow(dead_code)]
pub fn apply_bulk_edits(
    text:   &str,
    ops:    &[(Vec<String>, BulkEditOp)],
    indent: &str,
) -> Result<String> {
    let mut root = ast::parse(text)
        .map_err(|e| AppError::Other(format!("parse: {e}")))?;
    apply_bulk_edits_inplace(&mut root, ops)?;
    Ok(ast::to_pretty_string_with(&root, indent))
}

/// In-place variant — mutates `root` directly. Used by the active-doc
/// flow which already holds a parsed AST inside the registry.
pub fn apply_bulk_edits_inplace(
    root: &mut RonAst,
    ops:  &[(Vec<String>, BulkEditOp)],
) -> Result<()> {
    // Phase A — sets. Order among sets doesn't matter: each operates
    // on a leaf, no sibling-index shifts.
    for (path, op) in ops {
        let BulkEditOp::Set(val) = op else { continue; };
        let target = resolve_path_mut(root, path)?;
        apply_set_to_target(target, val)
            .map_err(|e| AppError::Other(format!("set at {path:?}: {e}")))?;
    }

    // Phase B — deletes. Group by parent path so we can sort each
    // parent's keys in numeric-aware reverse order (largest-index-
    // first, so list/tuple removals don't invalidate later indices).
    use std::collections::BTreeMap;
    let mut by_parent: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
    for (path, op) in ops {
        if !matches!(op, BulkEditOp::Delete) { continue; }
        if path.is_empty() {
            return Err(AppError::Other("Cannot delete the document root".into()));
        }
        let (key, parent) = path.split_last().unwrap();
        by_parent.entry(parent.to_vec()).or_default().push(key.clone());
    }
    // Iterate parents in DESCENDING path order so we touch deeper
    // parents first — deleting a deeper key never invalidates an
    // ancestor's path. (Same parent path can hold multiple deletes;
    // we sort those internally.)
    for (parent_path, mut keys) in by_parent.into_iter().rev() {
        keys.sort_by(|a, b| {
            match (a.parse::<i64>().ok(), b.parse::<i64>().ok()) {
                (Some(ai), Some(bi)) => bi.cmp(&ai),
                _ => b.cmp(a),
            }
        });
        keys.dedup();
        let parent = resolve_path_mut(root, &parent_path)?;
        for k in &keys {
            apply_delete_in_parent(parent, k)
                .map_err(|e| AppError::Other(format!(
                    "delete at {parent_path:?}/{k}: {e}",
                )))?;
        }
    }

    Ok(())
}

fn apply_set_to_target(target: &mut RonAst, v: &BulkSetValue) -> Result<()> {
    // Option targets have their own wrap/unwrap semantics (FROZEN F13).
    if matches!(target, RonAst::Option(_)) {
        let inner: Option<RonAst> = match v {
            BulkSetValue::Null      => None,
            BulkSetValue::String(s) => Some(RonAst::String(s.clone())),
            BulkSetValue::Int(i)    => Some(RonAst::Int(*i)),
            BulkSetValue::Float(f)  => Some(RonAst::Float(*f)),
            BulkSetValue::Bool(b)   => Some(RonAst::Bool(*b)),
        };
        *target = RonAst::Option(inner.map(Box::new));
        return Ok(());
    }
    // Non-option: null is rejected (RON has no null).
    match (v, &target) {
        (BulkSetValue::Null, _) => Err(AppError::Other(
            "RON has no null. Use Option or delete the field instead.".into(),
        )),
        (BulkSetValue::String(s), RonAst::String(_)) => {
            *target = RonAst::String(s.clone());
            Ok(())
        }
        (BulkSetValue::Bool(b), RonAst::Bool(_)) => {
            *target = RonAst::Bool(*b);
            Ok(())
        }
        // Number → Int when the current node is Int and the value is
        // integral; otherwise Float. Lets `old + 1` on an integer field
        // stay integer; `old * 1.5` on the same field becomes float.
        (BulkSetValue::Int(i), RonAst::Int(_)) => {
            *target = RonAst::Int(*i);
            Ok(())
        }
        (BulkSetValue::Int(i), RonAst::Float(_)) => {
            *target = RonAst::Float(*i as f64);
            Ok(())
        }
        (BulkSetValue::Float(f), RonAst::Int(_)) if f.fract() == 0.0 && f.is_finite() => {
            *target = RonAst::Int(*f as i64);
            Ok(())
        }
        (BulkSetValue::Float(f), RonAst::Int(_)) => {
            *target = RonAst::Float(*f);
            Ok(())
        }
        (BulkSetValue::Float(f), RonAst::Float(_)) => {
            *target = RonAst::Float(*f);
            Ok(())
        }
        // Cross-kind coercions are rejected here — the preview step
        // should have surfaced these as "type mismatch" skips. The
        // defensive guard keeps a stale FE payload from corrupting
        // the doc.
        (v, t) => Err(AppError::Other(format!(
            "Type mismatch: cannot install {v:?} on {:?}", node_kind(t),
        ))),
    }
}

fn apply_delete_in_parent(parent: &mut RonAst, key: &str) -> Result<()> {
    // Option targets become None (preserving field shape).
    if let RonAst::Option(_) = parent {
        if key == "Some" {
            *parent = RonAst::Option(None);
            return Ok(());
        }
    }
    remove_from_parent(parent, key)
}

// ── Save ────────────────────────────────────────────────────────────────────

/// FROZEN F16: encoding-aware disk write. Re-encodes `contents` from
/// the in-memory `String` (always UTF-8 internally) back to the doc's
/// original byte representation, prepending the BOM when the original
/// file had one. Callers without a known encoding pass UTF-8 / no BOM
/// (== `encode_for_disk` behaviour).
pub fn write_to_disk(
    path:           &str,
    contents:       &str,
    encoding_label: &str,
    had_bom:        bool,
) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Other(format!("mkdir {parent:?}: {e}")))?;
        }
    }
    let bytes = crate::git::encoding::encode_for_disk_with_bom(
        contents,
        Some(encoding_label),
        had_bom,
    );
    std::fs::write(path, &bytes).map_err(|e| AppError::Other(format!("write {path}: {e}")))
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn try_parse(text: &str) -> (Option<RonAst>, Option<String>) {
    match ast::parse(text) {
        Ok(v)  => (Some(v), None),
        Err(e) => (None, Some(e.to_string())),
    }
}

pub fn node_kind(v: &RonAst) -> NodeKind {
    match v {
        RonAst::Unit          => NodeKind::Unit,
        RonAst::Bool(_)       => NodeKind::Bool,
        RonAst::Char(_)       => NodeKind::Char,
        RonAst::Int(_)        => NodeKind::Number,
        RonAst::Float(_)      => NodeKind::Number,
        RonAst::String(_)     => NodeKind::String,
        RonAst::Option(_)     => NodeKind::Option,
        RonAst::List(_)       => NodeKind::List,
        RonAst::Map(_)        => NodeKind::Map,
        RonAst::Struct { name: None, .. }    => NodeKind::Struct,
        RonAst::Struct { name: Some(_), .. } => NodeKind::NamedStruct,
        RonAst::Tuple  { name: None, .. }    => NodeKind::Tuple,
        RonAst::Tuple  { name: Some(_), .. } => NodeKind::NamedTuple,
        RonAst::UnitVariant(_) => NodeKind::UnitVariant,
    }
}

pub fn variant_tag(v: &RonAst) -> Option<String> {
    match v {
        RonAst::Struct { name: Some(n), .. } => Some(n.clone()),
        RonAst::Tuple  { name: Some(n), .. } => Some(n.clone()),
        RonAst::UnitVariant(n) => Some(n.clone()),
        _ => None,
    }
}

fn container_len(v: &RonAst) -> usize {
    match v {
        RonAst::List(items)           => items.len(),
        RonAst::Map(pairs)            => pairs.len(),
        RonAst::Struct { fields, .. } => fields.len(),
        RonAst::Tuple  { items, .. }  => items.len(),
        // `Some(_)` always exposes exactly one synthetic child keyed
        // `"Some"` (see `children_of`). Returning the *inner's* count
        // here would leave the Tree thinking primitives-in-Option aren't
        // expandable — but they need to be so the user can drill into,
        // select, and edit the inner value. None has zero children.
        RonAst::Option(Some(_))       => 1,
        _ => 0,
    }
}

pub fn preview_for(v: &RonAst) -> String {
    // The variant/struct tag (for named_* and unit_variant kinds) is
    // returned separately on the NodeView so the frontend can render it
    // as its own chip. Here we only emit the "shape" half of the preview.
    match v {
        RonAst::Unit          => "()".to_string(),
        RonAst::Bool(b)       => b.to_string(),
        RonAst::Char(c)       => format!("'{}'", c.escape_default()),
        RonAst::Int(i)        => i.to_string(),
        RonAst::Float(f)      => format!("{f}"),
        RonAst::String(s)     => preview_string(s),
        RonAst::Option(None)  => "None".into(),
        RonAst::Option(Some(inner)) => format!("Some({})", preview_for(inner.as_ref())),
        RonAst::List(items)   => format!("[{} items]", items.len()),
        RonAst::Map(pairs)    => format!("{{{} keys}}", pairs.len()),
        RonAst::Struct { fields, .. } => format!("({} fields)", fields.len()),
        RonAst::Tuple  { items, .. } if items.is_empty() => "()".to_string(),
        RonAst::Tuple  { items, .. } if items.len() == 1 => format!("({})", preview_for(&items[0])),
        RonAst::Tuple  { items, .. } => format!("({} items)", items.len()),
        RonAst::UnitVariant(_) => String::new(),
    }
}

fn preview_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len().min(PREVIEW_MAX_CHARS) + 2);
    out.push('"');
    for (i, ch) in s.chars().enumerate() {
        if i >= PREVIEW_MAX_CHARS { out.push('…'); break; }
        out.push(ch);
    }
    out.push('"');
    out
}

fn view_for(key: String, path: Vec<String>, v: &RonAst) -> NodeView {
    NodeView {
        key,
        path,
        kind: node_kind(v),
        preview: preview_for(v),
        child_count: container_len(v),
        variant_tag: variant_tag(v),
    }
}

pub fn resolve_path<'a>(root: &'a RonAst, path: &[String]) -> Result<&'a RonAst> {
    let mut cur = root;
    for seg in path {
        cur = step_into(cur, seg)?;
    }
    Ok(cur)
}

fn step_into<'a>(v: &'a RonAst, seg: &str) -> Result<&'a RonAst> {
    match v {
        RonAst::Struct { fields, .. } => {
            for (k, val) in fields {
                if k == seg { return Ok(val); }
            }
            Err(AppError::Other(format!("Missing field: {seg}")))
        }
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            let idx: usize = seg.parse().map_err(|_| AppError::Other(format!("Invalid index: {seg}")))?;
            items.get(idx).ok_or_else(|| AppError::Other(format!("Index out of bounds: {idx}")))
        }
        RonAst::Map(pairs) => {
            for (k, val) in pairs {
                if key_to_string(k) == seg { return Ok(val); }
            }
            Err(AppError::Other(format!("Missing key: {seg}")))
        }
        RonAst::Option(Some(inner)) => {
            if seg == "Some" { Ok(inner.as_ref()) } else { step_into(inner.as_ref(), seg) }
        }
        _ => Err(AppError::Other(format!("Cannot descend at: {seg}"))),
    }
}

fn duplicate_in_parent(parent: &mut RonAst, key: &str) -> Result<()> {
    match parent {
        RonAst::Struct { fields, .. } => {
            let i = fields.iter().position(|(k, _)| k == key)
                .ok_or_else(|| AppError::Other(format!("Missing field: {key}")))?;
            // Build a fresh non-colliding name. `_copy`, `_copy_2`, …
            let base = format!("{}_copy", fields[i].0);
            let mut candidate = base.clone();
            let mut n = 2;
            while fields.iter().any(|(k, _)| k == &candidate) {
                candidate = format!("{base}_{n}");
                n += 1;
            }
            let cloned = fields[i].1.clone();
            fields.insert(i + 1, (candidate, cloned));
            Ok(())
        }
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            let idx: usize = key.parse()
                .map_err(|_| AppError::Other(format!("Invalid index: {key}")))?;
            if idx >= items.len() {
                return Err(AppError::Other(format!("Index out of bounds: {idx}")));
            }
            let cloned = items[idx].clone();
            items.insert(idx + 1, cloned);
            Ok(())
        }
        RonAst::Map(pairs) => {
            let i = pairs.iter().position(|(k, _)| key_to_string(k) == key)
                .ok_or_else(|| AppError::Other(format!("Missing key: {key}")))?;
            let (orig_key, orig_val) = pairs[i].clone();
            // Synthesise a unique key. Strings: append `_copy`. Other
            // key kinds aren't safely cloneable — the user can edit the
            // duplicate's key afterwards if we let it land, so we still
            // try by appending `_copy` to the pretty-printed key.
            let new_key = match &orig_key {
                RonAst::String(s) => RonAst::String(format!("{s}_copy")),
                _                 => RonAst::String(format!("{}_copy", key_to_string(&orig_key))),
            };
            let new_repr = key_to_string(&new_key);
            if pairs.iter().any(|(k, _)| key_to_string(k) == new_repr) {
                return Err(AppError::Other(format!(
                    "Duplicate would collide with existing key `{new_repr}`"
                )));
            }
            pairs.insert(i + 1, (new_key, orig_val));
            Ok(())
        }
        RonAst::Option(_) => Err(AppError::Other(
            "Cannot duplicate inside Option — wrap in a list/tuple first".into(),
        )),
        _ => Err(AppError::Other("Cannot duplicate from this node".into())),
    }
}

fn move_item_in_parent(parent: &mut RonAst, key: &str, delta: i32) -> Result<()> {
    match parent {
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            let idx: usize = key.parse()
                .map_err(|_| AppError::Other(format!("Invalid index: {key}")))?;
            if idx >= items.len() {
                return Err(AppError::Other(format!("Index out of bounds: {idx}")));
            }
            let target = idx as i32 + delta;
            if target < 0 || target as usize >= items.len() {
                // Out-of-range is a no-op so the user holding a "move up"
                // button on the first item doesn't see scary errors.
                return Ok(());
            }
            items.swap(idx, target as usize);
            Ok(())
        }
        RonAst::Struct { .. } | RonAst::Map(_) =>
            Err(AppError::Other("Reorder isn't supported on structs / maps yet".into())),
        _ => Err(AppError::Other("Cannot reorder inside this node".into())),
    }
}

fn remove_from_parent(parent: &mut RonAst, key: &str) -> Result<()> {
    match parent {
        RonAst::Struct { fields, .. } => {
            let i = fields.iter().position(|(k, _)| k == key)
                .ok_or_else(|| AppError::Other(format!("Missing field: {key}")))?;
            fields.remove(i);
            Ok(())
        }
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            let idx: usize = key.parse()
                .map_err(|_| AppError::Other(format!("Invalid index: {key}")))?;
            if idx >= items.len() {
                return Err(AppError::Other(format!("Index out of bounds: {idx}")));
            }
            items.remove(idx);
            Ok(())
        }
        RonAst::Map(pairs) => {
            let i = pairs.iter().position(|(k, _)| key_to_string(k) == key)
                .ok_or_else(|| AppError::Other(format!("Missing key: {key}")))?;
            pairs.remove(i);
            Ok(())
        }
        RonAst::Option(_) => Err(AppError::Other(
            "Cannot remove from Option — toggle to None instead".into(),
        )),
        _ => Err(AppError::Other("Cannot remove from this node".into())),
    }
}

fn resolve_path_mut<'a>(root: &'a mut RonAst, path: &[String]) -> Result<&'a mut RonAst> {
    let mut cur = root;
    for seg in path {
        cur = step_into_mut(cur, seg)?;
    }
    Ok(cur)
}

fn step_into_mut<'a>(v: &'a mut RonAst, seg: &str) -> Result<&'a mut RonAst> {
    match v {
        RonAst::Struct { fields, .. } => {
            for (k, val) in fields.iter_mut() {
                if k == seg { return Ok(val); }
            }
            Err(AppError::Other(format!("Missing field: {seg}")))
        }
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            let idx: usize = seg.parse().map_err(|_| AppError::Other(format!("Invalid index: {seg}")))?;
            items.get_mut(idx).ok_or_else(|| AppError::Other(format!("Index out of bounds: {idx}")))
        }
        RonAst::Map(pairs) => {
            for (k, val) in pairs.iter_mut() {
                if key_to_string(k) == seg { return Ok(val); }
            }
            Err(AppError::Other(format!("Missing key: {seg}")))
        }
        RonAst::Option(Some(inner)) => {
            if seg == "Some" { Ok(inner.as_mut()) } else { step_into_mut(inner.as_mut(), seg) }
        }
        _ => Err(AppError::Other(format!("Cannot descend at: {seg}"))),
    }
}

fn children_of(parent_path: &[String], v: &RonAst) -> Vec<NodeView> {
    match v {
        RonAst::Struct { fields, .. } => fields.iter().map(|(k, child)| {
            let mut p = parent_path.to_vec();
            p.push(k.clone());
            view_for(k.clone(), p, child)
        }).collect(),
        RonAst::Tuple { items, .. } | RonAst::List(items) => {
            items.iter().enumerate().map(|(i, child)| {
                let key = i.to_string();
                let mut p = parent_path.to_vec();
                p.push(key.clone());
                view_for(key, p, child)
            }).collect()
        }
        RonAst::Map(pairs) => pairs.iter().map(|(k, child)| {
            let key = key_to_string(k);
            let mut p = parent_path.to_vec();
            p.push(key.clone());
            view_for(key, p, child)
        }).collect(),
        RonAst::Option(Some(inner)) => {
            let mut p = parent_path.to_vec();
            p.push("Some".into());
            vec![view_for("Some".into(), p, inner.as_ref())]
        }
        _ => Vec::new(),
    }
}

fn key_to_string(v: &RonAst) -> String {
    match v {
        RonAst::String(s) => s.clone(),
        RonAst::Char(c)   => c.to_string(),
        RonAst::Bool(b)   => b.to_string(),
        RonAst::Int(i)    => i.to_string(),
        RonAst::Float(f)  => f.to_string(),
        RonAst::UnitVariant(n) => n.clone(),
        _ => ast::to_pretty_string(v),
    }
}

fn json_to_ron(j: &serde_json::Value) -> RonAst {
    use serde_json::Value as J;
    match j {
        J::Null      => RonAst::Option(None),
        J::Bool(b)   => RonAst::Bool(*b),
        J::String(s) => RonAst::String(s.clone()),
        J::Number(n) => {
            if let Some(i) = n.as_i64() { RonAst::Int(i) }
            else if let Some(f) = n.as_f64() { RonAst::Float(f) }
            else { RonAst::Unit }
        }
        J::Array(items) => RonAst::List(items.iter().map(json_to_ron).collect()),
        J::Object(map) => {
            let fields: Vec<(String, RonAst)> = map.iter()
                .map(|(k, v)| (k.clone(), json_to_ron(v)))
                .collect();
            RonAst::Struct { name: None, fields }
        }
    }
}
