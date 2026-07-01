//! `core::simple` — the `SimpleFormat` trait (blueprint §2.6).
//!
//! `SimpleFormat` captures the *only* format-specific primitives the
//! three "simple" formats (TOML / YAML / .properties) need; everything
//! else — the doc registry, `History<String>`, encoding round-trip,
//! original/current snapshots, diff/query/refactor/persist delegation —
//! is owned once by [`crate::backend::DefaultBackend`].
//!
//! ## Why the mutation seam is text → text
//!
//! The blueprint sketches an associated `type Doc`. In practice the
//! three formats can NOT share a single owned doc model living on the
//! registry:
//!
//! * TOML's `toml_edit::DocumentMut` clones cheaply and re-parses.
//! * YAML's `yaml_edit::Document` contains rowan `NonNull` which is
//!   `!Send`; the YAML backend deliberately re-parses on every mutation
//!   path rather than caching the tree (it would break the `Send` bound
//!   on the async `StudioFormatBackend` trait).
//! * `.properties` keeps a `Vec<RawLine>` that it clones + re-parses.
//!
//! What all three genuinely agree on is **"take the current text, apply
//! one structured mutation, emit the new text"**. So each mutation here
//! is `(&self, text, …) -> StudioResult<String>`: the impl parses its
//! own (possibly `!Send`) doc, mutates, and re-emits — exactly what each
//! `mutate_with` does today, minus the registry bookkeeping. `type Doc`
//! survives as a parsed-projection handle returned by [`SimpleFormat::parse`]
//! so the backend can pull `root_kind` / `child_count` / `value` without
//! re-parsing, but it never has to be `Send` because the backend only
//! holds the projected `serde_json::Value`, never the `Doc` itself.

use serde_json::Value;

use arbor_studio_types::prelude::{EncodingInfo, FormatDescriptor, StudioResult};

/// Outcome of parsing a buffer: the projected `serde_json::Value`
/// (`None` on parse failure) plus the human-readable parse error.
///
/// The `value` projection is the nav/query source-of-truth (same trick
/// every backend uses: project the format-native AST to JSON). The
/// backend caches only this — never the format's own (possibly `!Send`)
/// AST.
#[derive(Debug, Clone, Default)]
pub struct ParseOutcome {
    /// JSON projection of the document. `None` when the buffer failed to
    /// parse (then `error` is `Some`).
    pub value: Option<Value>,
    /// Parse error message, `None` when the buffer parsed cleanly.
    pub error: Option<String>,
}

impl ParseOutcome {
    pub fn ok(value: Value) -> Self {
        Self { value: Some(value), error: None }
    }
    pub fn err(message: impl Into<String>) -> Self {
        Self { value: None, error: Some(message.into()) }
    }
}

/// A structured tree mutation, dispatched once by the backend.
///
/// This mirrors [`arbor_studio_types::prelude::StudioMutation`] minus the
/// `ToggleOption` variant (no simple format has Option/None — TOML/YAML/
/// .properties all reject it at the descriptor level). The backend
/// destructures `StudioMutation` and hands the leaf op here; the impl
/// applies it to `text` and returns the new text.
#[derive(Debug, Clone)]
pub enum SimpleMutation {
    /// Set a scalar primitive at `path`. `value` is the raw (possibly
    /// FE-tagged `{type,value}`) `serde_json::Value`; the impl unwraps +
    /// coerces it to its own scalar type.
    SetPrimitive   { path: Vec<String>, value: Value },
    /// Replace the whole node at `path` with the parsed `text` snippet.
    ReplaceAt      { path: Vec<String>, text: String },
    /// Remove the node at `path`.
    RemoveAt       { path: Vec<String> },
    /// Insert a `name = <text>` field into the container at `path`.
    InsertField    { path: Vec<String>, name: String, text: String },
    /// Append the parsed `text` item to the array at `path`.
    InsertItem     { path: Vec<String>, text: String },
    /// Insert a `key_text = val_text` entry into the map at `path`.
    InsertMapEntry { path: Vec<String>, key_text: String, val_text: String },
    /// Duplicate the node at `path` next to itself.
    DuplicateAt    { path: Vec<String> },
    /// Move the node at `path` by `delta` positions inside its parent.
    MoveItem       { path: Vec<String>, delta: i32 },
}

/// The minimal per-format surface a "simple" format must expose for
/// [`crate::backend::DefaultBackend`] to provide a full
/// `StudioFormatBackend`. Everything not here is generic.
///
/// All methods are text-in / text-out (see module docs) so the impl can
/// own a `!Send` editor internally without leaking it across the async
/// trait boundary.
pub trait SimpleFormat: Send + Sync + 'static {
    /// The capability matrix for this format (hard-coded per crate).
    fn descriptor(&self) -> &FormatDescriptor;

    // ── Doc lifecycle ────────────────────────────────────────────────

    /// Parse `text` into the projected `serde_json::Value` (+ parse
    /// error). `encoding` is informational — most formats ignore it at
    /// parse time and only consult it on save, but it is passed for
    /// parity with the trait.
    fn parse(&self, text: &str, encoding: &EncodingInfo) -> ParseOutcome;

    /// Sniff the document's indent string for the FE indent pill. Pure
    /// heuristic; never fails.
    fn detect_indent(&self, text: &str) -> String;

    /// "Pretty-print" / reflow `text`. For formats whose editor owns
    /// formatting (TOML/YAML) this round-trips through the editor; for
    /// `.properties` it is the identity (every byte already preserved).
    /// Errors when `text` does not parse.
    fn pretty(&self, text: &str) -> StudioResult<String>;

    // ── Structured mutation (text → text) ────────────────────────────

    /// Apply one structured mutation to `text` and return the new text.
    /// The impl parses its own AST, mutates, re-emits. Errors on a bad
    /// path or an op the format can't express.
    fn mutate(&self, text: &str, mutation: SimpleMutation) -> StudioResult<String>;

    // ── Node metadata for NodeView / QueryHit ────────────────────────

    /// Kind string for a value node (drives the FE chip palette). Note
    /// some formats (TOML) lose precision here because the JSON
    /// projection can't carry datetime / array-of-tables — that is the
    /// existing behavior and is preserved.
    fn node_kind(&self, v: &Value) -> String;

    /// Short preview string for a value node (the tree-pane / query row).
    fn preview_for(&self, v: &Value) -> String;

    /// Variant tag for a node — always `None` for the simple formats
    /// (only RON has variant tags). Defaulted so impls need not write it.
    fn variant_tag(&self, _v: &Value) -> Option<String> {
        None
    }
}
