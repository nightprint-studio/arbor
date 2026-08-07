//! The slice of the Language Server Protocol Bennu speaks, as serde types.
//!
//! Hand-rolled, and the reasoning is worth stating once: Bennu drives a **bounded**
//! subset — roughly fifteen requests and six notifications — and every one of them has
//! to be mapped onto Bennu's own wire types anyway, so a crate that models the whole
//! spec would add a dependency, a version treadmill, and a second vocabulary without
//! removing the mapping. What it *would* remove is the risk of getting a shape wrong, so
//! the shapes that are easy to get wrong are the ones with tests at the bottom of this
//! file: the `boolean | Options` capability fields, the three ways a goto answer can
//! come back, and the two ways a completion item can carry its edit.
//!
//! Conventions throughout:
//!
//! * `#[serde(rename_all = "camelCase")]` — the protocol's spelling.
//! * every optional field is `#[serde(default)]` on the way in and
//!   `skip_serializing_if` on the way out. Servers vary in how much of the spec they
//!   populate, and a missing field must read as "not provided" rather than fail the
//!   whole response; sending `null` where the spec says the key may be absent is
//!   accepted by most servers and refused by a few.
//! * outgoing param structs are separate from incoming result structs even where the
//!   spec reuses one interface. They diverge in practice (we send fewer fields than we
//!   accept) and one struct serving both directions ends up `Option`-al everywhere.
//! * unknown fields are **ignored**, never rejected: servers ship protocol extensions
//!   (rust-analyzer's are numerous) and a strict decoder would fail on answers it could
//!   have used.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use crate::line_index::{Position, PositionEncoding, Range};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// A field the spec types as `boolean | SomeOptions`.
///
/// The single most common shape in `ServerCapabilities`, and the one that breaks a naive
/// decoder: `"renameProvider": true` and `"renameProvider": {"prepareProvider": true}`
/// are both legal for the same key, and a server picks whichever it likes.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum BoolOr<T> {
    Bool(bool),
    Options(T),
}

impl<T> BoolOr<T> {
    /// Whether the capability is present at all — `false` only for an explicit `false`.
    pub fn enabled(&self) -> bool {
        match self {
            BoolOr::Bool(b) => *b,
            BoolOr::Options(_) => true,
        }
    }

    /// The options, when the server sent the object form.
    pub fn options(&self) -> Option<&T> {
        match self {
            BoolOr::Bool(_) => None,
            BoolOr::Options(o) => Some(o),
        }
    }
}

/// Whether an `Option<BoolOr<T>>` capability is on. The predicate every feature gate in
/// [`crate::session`] runs before spending a round-trip on a request the server would
/// answer with "method not found".
pub fn capability_on<T>(cap: &Option<BoolOr<T>>) -> bool {
    cap.as_ref().is_some_and(|c| c.enabled())
}

/// Documentation, which the spec types as `string | MarkupContent`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Documentation {
    Plain(String),
    Markup(MarkupContent),
}

impl Documentation {
    /// The text, whichever form it arrived in. The markup *kind* is dropped: Bennu's
    /// hover card renders a restricted markdown itself, and a "plaintext" claim from a
    /// server that then sends backticks is not worth honouring.
    pub fn text(&self) -> &str {
        match self {
            Documentation::Plain(s) => s,
            Documentation::Markup(m) => &m.value,
        }
    }
}

/// A documentation string with a declared markup kind (`plaintext` / `markdown`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkupContent {
    pub kind: String,
    pub value: String,
}

/// A code block or plain string, as `hover.contents` may still be sent (LSP 2.x shape,
/// which plenty of servers keep using).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MarkedString {
    Plain(String),
    Fenced { language: String, value: String },
}

impl MarkedString {
    pub fn value(&self) -> &str {
        match self {
            MarkedString::Plain(s) => s,
            MarkedString::Fenced { value, .. } => value,
        }
    }
}

// ---------------------------------------------------------------------------
// Document identity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

impl TextDocumentIdentifier {
    pub fn new(uri: impl Into<String>) -> Self {
        Self { uri: uri.into() }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionedTextDocumentIdentifier {
    pub uri: String,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentItem {
    pub uri: String,
    pub language_id: String,
    pub version: i32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentPositionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFolder {
    pub uri: String,
    pub name: String,
}

/// A replacement of `range` with `new_text` — the unit of every edit the protocol
/// returns (rename, formatting, code action, completion).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

/// An edit carrying a change annotation. Deserialized as a plain [`TextEdit`] would be —
/// the annotation id is metadata for a client that groups edits for review, which Bennu's
/// rename preview does its own way.
pub type AnnotatedTextEdit = TextEdit;

// ---------------------------------------------------------------------------
// Lifecycle: initialize / shutdown
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_info: Option<ClientInfo>,
    /// Deprecated in the spec but still the only root some servers read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_uri: Option<String>,
    pub workspace_folders: Vec<WorkspaceFolder>,
    /// Ours, built as a JSON literal in [`crate::session`].
    ///
    /// Typed as a raw value on purpose: client capabilities are a deep nested bag that we
    /// *declare* and never read back, so a struct for them would be a hundred lines of
    /// `Option` that only ever serialize one way. The JSON literal at the call site is
    /// the readable form — and it is the actual contract with the server, so having it in
    /// one visible block beats having it spread over nested constructors.
    pub capabilities: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initialization_options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    #[serde(default)]
    pub capabilities: ServerCapabilities,
    #[serde(default)]
    pub server_info: Option<ServerInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// What the server says it can do. Every request in [`crate::session`] is gated on the
/// matching field: asking a server for something it never advertised earns a
/// "method not found" error, and one of those per keystroke is how a feature ends up
/// looking broken instead of absent.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// The units the server will use for every position. Absent means UTF-16 (the
    /// protocol default), NOT "whatever the client asked for".
    #[serde(default)]
    pub position_encoding: Option<PositionEncoding>,
    #[serde(default)]
    pub text_document_sync: Option<TextDocumentSync>,
    #[serde(default)]
    pub completion_provider: Option<CompletionOptions>,
    #[serde(default)]
    pub hover_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub signature_help_provider: Option<SignatureHelpOptions>,
    #[serde(default)]
    pub definition_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub type_definition_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub implementation_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub references_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub document_symbol_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub workspace_symbol_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub code_action_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub document_formatting_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub document_range_formatting_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub rename_provider: Option<BoolOr<RenameOptions>>,
    #[serde(default)]
    pub semantic_tokens_provider: Option<SemanticTokensOptions>,
    #[serde(default)]
    pub document_highlight_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub selection_range_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub folding_range_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub code_lens_provider: Option<CodeLensOptions>,
    #[serde(default)]
    pub call_hierarchy_provider: Option<BoolOr<serde_json::Value>>,
    #[serde(default)]
    pub type_hierarchy_provider: Option<BoolOr<serde_json::Value>>,
    /// `workspace.fileOperations.willRename` — nested two levels down, which is why it is its own
    /// struct rather than a bool like its neighbours.
    #[serde(default)]
    pub workspace: Option<WorkspaceServerCapabilities>,
    #[serde(default)]
    pub inlay_hint_provider: Option<BoolOr<serde_json::Value>>,
}

/// `textDocumentSync` is `TextDocumentSyncKind | TextDocumentSyncOptions` — a bare
/// number or an object.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TextDocumentSync {
    Kind(u8),
    Options(TextDocumentSyncOptions),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentSyncOptions {
    #[serde(default)]
    pub open_close: Option<bool>,
    #[serde(default)]
    pub change: Option<u8>,
    #[serde(default)]
    pub save: Option<BoolOr<SaveOptions>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOptions {
    #[serde(default)]
    pub include_text: Option<bool>,
}

impl TextDocumentSync {
    /// Whether the server wants `didSave` at all. Skipping a notification the server
    /// never asked for is not just tidiness: rust-analyzer runs `cargo check` on save,
    /// and sending saves to a server that didn't register for them wastes a build.
    pub fn wants_save(&self) -> bool {
        match self {
            // The number form says nothing about save; the spec's default is off.
            TextDocumentSync::Kind(_) => false,
            TextDocumentSync::Options(o) => o.save.as_ref().is_some_and(|s| s.enabled()),
        }
    }

    /// Whether the server wants the full text with each `didSave`.
    pub fn save_includes_text(&self) -> bool {
        match self {
            TextDocumentSync::Kind(_) => false,
            TextDocumentSync::Options(o) => o
                .save
                .as_ref()
                .and_then(|s| s.options())
                .and_then(|o| o.include_text)
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionOptions {
    /// The characters that should fire completion automatically. Read rather than
    /// hard-coded: for Rust the set is `.` and `:` (for `::`), which a Java-shaped guess
    /// would miss.
    #[serde(default)]
    pub trigger_characters: Vec<String>,
    #[serde(default)]
    pub resolve_provider: Option<bool>,
    #[serde(default)]
    pub all_commit_characters: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureHelpOptions {
    #[serde(default)]
    pub trigger_characters: Vec<String>,
    #[serde(default)]
    pub retrigger_characters: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameOptions {
    #[serde(default)]
    pub prepare_provider: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTokensOptions {
    #[serde(default)]
    pub legend: SemanticTokensLegend,
    /// `boolean | { delta?: boolean }`.
    #[serde(default)]
    pub full: Option<BoolOr<SemanticTokensFullOptions>>,
    #[serde(default)]
    pub range: Option<BoolOr<serde_json::Value>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SemanticTokensFullOptions {
    #[serde(default)]
    pub delta: Option<bool>,
}

/// The server's token vocabulary. Semantic tokens arrive as **indices into these
/// arrays**, so without the legend the numbers mean nothing — which is why the session
/// keeps it from the handshake rather than assuming the spec's standard order (servers
/// are free to extend and reorder it, and rust-analyzer does both).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTokensLegend {
    #[serde(default)]
    pub token_types: Vec<String>,
    #[serde(default)]
    pub token_modifiers: Vec<String>,
}

// ---------------------------------------------------------------------------
// Document sync notifications
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DidOpenTextDocumentParams {
    pub text_document: TextDocumentItem,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DidChangeTextDocumentParams {
    pub text_document: VersionedTextDocumentIdentifier,
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

/// A change event. Bennu always syncs **full** text (the one-element `{text}` form).
///
/// Incremental sync would be less traffic, but it requires that our idea of the document
/// and the server's stay byte-identical through every edit, forever: one dropped or
/// misordered change and the server is analysing a file that does not exist, silently,
/// until the tab is closed. The editor hands us whole buffers anyway, so full sync costs
/// a string copy per keystroke-debounce and removes that entire failure class.
#[derive(Debug, Clone, Serialize)]
pub struct TextDocumentContentChangeEvent {
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DidCloseTextDocumentParams {
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DidSaveTextDocumentParams {
    pub text_document: TextDocumentIdentifier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DidChangeConfigurationParams {
    pub settings: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Diagnostics (server → client)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    #[serde(default)]
    pub version: Option<i32>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub range: Range,
    /// 1 = error, 2 = warning, 3 = information, 4 = hint. Absent means "the server
    /// leaves it to the client", which we read as a warning.
    #[serde(default)]
    pub severity: Option<u8>,
    /// `number | string` — rustc's is a string (`E0308`), other servers use numbers.
    #[serde(default)]
    pub code: Option<DiagnosticCode>,
    #[serde(default)]
    pub source: Option<String>,
    pub message: String,
    #[serde(default)]
    pub tags: Vec<u8>,
    #[serde(default)]
    pub related_information: Vec<DiagnosticRelatedInformation>,
    /// Opaque server payload; must be echoed back in a `codeAction` request for the
    /// server to be able to produce the fix.
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DiagnosticCode {
    Number(i64),
    Str(String),
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosticCode::Number(n) => write!(f, "{n}"),
            DiagnosticCode::Str(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiagnosticRelatedInformation {
    pub location: Location,
    pub message: String,
}

/// The diagnostic tag for "this code is unnecessary" (unused imports, dead code) — the
/// one tag worth acting on, since it changes how the squiggle should look rather than
/// what it says.
pub const DIAGNOSTIC_TAG_UNNECESSARY: u8 = 1;
/// "this is deprecated".
pub const DIAGNOSTIC_TAG_DEPRECATED: u8 = 2;

// ---------------------------------------------------------------------------
// Location / goto
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

/// The richer goto answer: it distinguishes the whole declaration (`target_range`) from
/// the name to put the caret on (`target_selection_range`), which is exactly the
/// distinction Bennu's `DeclarationTarget` makes.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationLink {
    pub target_uri: String,
    pub target_range: Range,
    pub target_selection_range: Range,
    #[serde(default)]
    pub origin_selection_range: Option<Range>,
}

/// `Location | Location[] | LocationLink[] | null` — all four legal answers to a goto
/// request, and a server picks by mood (rust-analyzer answers `LocationLink[]` when the
/// client advertises `linkSupport`, `Location[]` otherwise).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum GotoResponse {
    /// Order matters for an untagged enum: `Links` is tried before `Locations` because a
    /// `LocationLink` has no `uri` field and so cannot be mistaken for a `Location`,
    /// whereas the reverse ordering would let an empty array match either.
    Links(Vec<LocationLink>),
    Locations(Vec<Location>),
    Single(Location),
    Null,
}

impl GotoResponse {
    /// Flatten to `(uri, whole range, name range)` triples.
    pub fn targets(self) -> Vec<(String, Range, Range)> {
        match self {
            GotoResponse::Links(links) => links
                .into_iter()
                .map(|l| (l.target_uri, l.target_range, l.target_selection_range))
                .collect(),
            GotoResponse::Locations(locs) => {
                locs.into_iter().map(|l| (l.uri, l.range, l.range)).collect()
            }
            GotoResponse::Single(l) => vec![(l.uri, l.range, l.range)],
            GotoResponse::Null => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
    pub context: ReferenceContext,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceContext {
    pub include_declaration: bool,
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<CompletionContext>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionContext {
    pub trigger_kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_character: Option<String>,
}

/// The user typed an identifier character.
pub const COMPLETION_TRIGGER_INVOKED: u8 = 1;
/// The user typed one of the server's `triggerCharacters`. Telling the server *which*
/// one matters: rust-analyzer offers a different list after `.` than after `::`.
pub const COMPLETION_TRIGGER_CHARACTER: u8 = 2;

/// `CompletionItem[] | CompletionList | null`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CompletionResponse {
    List(CompletionList),
    Items(Vec<CompletionItem>),
    Null,
}

impl CompletionResponse {
    pub fn into_items(self) -> Vec<CompletionItem> {
        match self {
            CompletionResponse::List(l) => l.items,
            CompletionResponse::Items(i) => i,
            CompletionResponse::Null => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionList {
    #[serde(default)]
    pub is_incomplete: bool,
    #[serde(default)]
    pub items: Vec<CompletionItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    #[serde(default)]
    pub label_details: Option<CompletionItemLabelDetails>,
    /// See [`completion_kind_name`] — an index into the spec's fixed kind list.
    #[serde(default)]
    pub kind: Option<u8>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub documentation: Option<Documentation>,
    #[serde(default)]
    pub deprecated: Option<bool>,
    #[serde(default)]
    pub tags: Vec<u8>,
    #[serde(default)]
    pub preselect: Option<bool>,
    /// The server's own sort key. Honoured rather than re-sorted alphabetically: it is
    /// how rust-analyzer puts the field you actually want above the forty trait methods
    /// that also match.
    #[serde(default)]
    pub sort_text: Option<String>,
    #[serde(default)]
    pub filter_text: Option<String>,
    #[serde(default)]
    pub insert_text: Option<String>,
    /// 1 = plain text, 2 = snippet (`${1:…}` placeholders).
    #[serde(default)]
    pub insert_text_format: Option<u8>,
    #[serde(default)]
    pub text_edit: Option<CompletionTextEdit>,
    /// Edits elsewhere in the file that must be applied together with the insertion —
    /// for Rust this is the `use` line an auto-imported item needs, so dropping them
    /// silently produces code that does not compile.
    #[serde(default)]
    pub additional_text_edits: Vec<TextEdit>,
    /// Opaque; must be echoed back to `completionItem/resolve`.
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompletionItemLabelDetails {
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// `TextEdit | InsertReplaceEdit`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CompletionTextEdit {
    /// Tried first: it has `insert`/`replace` instead of `range`, so it cannot be
    /// mistaken for a plain edit.
    InsertReplace(InsertReplaceEdit),
    Edit(TextEdit),
}

impl CompletionTextEdit {
    /// The range this edit replaces, and the text to put there.
    ///
    /// For the two-range form we take `replace`, which is the "overwrite the identifier
    /// I am standing in" behaviour an IDE user expects from accepting a completion —
    /// `insert` would leave the tail of the old word behind.
    pub fn resolve(&self) -> (Range, &str) {
        match self {
            CompletionTextEdit::Edit(e) => (e.range, &e.new_text),
            CompletionTextEdit::InsertReplace(e) => (e.replace, &e.new_text),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertReplaceEdit {
    pub new_text: String,
    pub insert: Range,
    pub replace: Range,
}

/// The spec's `CompletionItemKind` numbering, as a lowercase name.
///
/// Mapped to a name here rather than passed through as a number because the name is what
/// crosses Bennu's wire (`CompletionItem::kind` is a string) and what the editor's icon
/// map keys off. The numbering is fixed by the spec and has not changed since 3.x.
pub fn completion_kind_name(kind: Option<u8>) -> &'static str {
    match kind {
        Some(1) => "text",
        Some(2) => "method",
        Some(3) => "function",
        Some(4) => "constructor",
        Some(5) => "field",
        Some(6) => "variable",
        Some(7) => "class",
        Some(8) => "interface",
        Some(9) => "module",
        Some(10) => "property",
        Some(11) => "unit",
        Some(12) => "value",
        Some(13) => "enum",
        Some(14) => "keyword",
        Some(15) => "snippet",
        Some(16) => "color",
        Some(17) => "file",
        Some(18) => "reference",
        Some(19) => "folder",
        Some(20) => "enum-member",
        Some(21) => "constant",
        Some(22) => "struct",
        Some(23) => "event",
        Some(24) => "operator",
        Some(25) => "type-parameter",
        _ => "text",
    }
}

/// The completion-item tag for "deprecated" (the modern spelling of the deprecated
/// `deprecated` boolean).
pub const COMPLETION_TAG_DEPRECATED: u8 = 1;
/// `insertTextFormat` = snippet: the text carries `${1:…}` placeholders.
pub const INSERT_TEXT_FORMAT_SNIPPET: u8 = 2;

// ---------------------------------------------------------------------------
// Hover / signature help
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Hover {
    pub contents: HoverContents,
    #[serde(default)]
    pub range: Option<Range>,
}

/// `MarkedString | MarkedString[] | MarkupContent`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum HoverContents {
    Markup(MarkupContent),
    Array(Vec<MarkedString>),
    Scalar(MarkedString),
}

impl HoverContents {
    /// The hover text as one markdown-ish string, sections joined by a rule.
    pub fn text(&self) -> String {
        match self {
            HoverContents::Markup(m) => m.value.clone(),
            HoverContents::Scalar(s) => s.value().to_string(),
            HoverContents::Array(parts) => parts
                .iter()
                .map(|p| p.value().to_string())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureHelpParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureHelp {
    #[serde(default)]
    pub signatures: Vec<SignatureInformation>,
    #[serde(default)]
    pub active_signature: Option<u32>,
    #[serde(default)]
    pub active_parameter: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInformation {
    pub label: String,
    #[serde(default)]
    pub documentation: Option<Documentation>,
    #[serde(default)]
    pub parameters: Vec<ParameterInformation>,
    #[serde(default)]
    pub active_parameter: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParameterInformation {
    pub label: ParameterLabel,
    #[serde(default)]
    pub documentation: Option<Documentation>,
}

/// `string | [uoffset, uoffset]` — either the parameter's text, or its span **within the
/// signature label**, in the negotiated position encoding's units.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ParameterLabel {
    Range([u32; 2]),
    Text(String),
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSymbolParams {
    pub text_document: TextDocumentIdentifier,
}

/// `DocumentSymbol[] | SymbolInformation[]` — the hierarchical shape and the flat legacy
/// one. Both are in the wild; the hierarchical one is what an outline wants.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DocumentSymbolResponse {
    /// Tried first: a `DocumentSymbol` has `selectionRange` and no `location`.
    Nested(Vec<DocumentSymbol>),
    Flat(Vec<SymbolInformation>),
    Null,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSymbol {
    pub name: String,
    #[serde(default)]
    pub detail: Option<String>,
    pub kind: u8,
    #[serde(default)]
    pub tags: Vec<u8>,
    #[serde(default)]
    pub deprecated: Option<bool>,
    /// The whole declaration, body included.
    pub range: Range,
    /// The name token — where go-to should land.
    pub selection_range: Range,
    #[serde(default)]
    pub children: Vec<DocumentSymbol>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInformation {
    pub name: String,
    pub kind: u8,
    #[serde(default)]
    pub tags: Vec<u8>,
    #[serde(default)]
    pub deprecated: Option<bool>,
    pub location: Location,
    #[serde(default)]
    pub container_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceSymbolParams {
    pub query: String,
}

/// `SymbolInformation[] | WorkspaceSymbol[]` — the newer `WorkspaceSymbol` may carry a
/// `location` with only a `uri` (no range) and resolve the range lazily.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum WorkspaceSymbolResponse {
    Full(Vec<SymbolInformation>),
    Lazy(Vec<WorkspaceSymbol>),
    Null,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSymbol {
    pub name: String,
    pub kind: u8,
    #[serde(default)]
    pub container_name: Option<String>,
    pub location: WorkspaceSymbolLocation,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum WorkspaceSymbolLocation {
    Full(Location),
    UriOnly { uri: String },
}

impl WorkspaceSymbolLocation {
    pub fn uri(&self) -> &str {
        match self {
            WorkspaceSymbolLocation::Full(l) => &l.uri,
            WorkspaceSymbolLocation::UriOnly { uri } => uri,
        }
    }

    pub fn range(&self) -> Option<Range> {
        match self {
            WorkspaceSymbolLocation::Full(l) => Some(l.range),
            WorkspaceSymbolLocation::UriOnly { .. } => None,
        }
    }
}

/// The spec's `SymbolKind` numbering, as a lowercase name — the same treatment (and the
/// same reason) as [`completion_kind_name`].
pub fn symbol_kind_name(kind: u8) -> &'static str {
    match kind {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        15 => "string",
        16 => "number",
        17 => "boolean",
        18 => "array",
        19 => "object",
        20 => "key",
        21 => "null",
        22 => "enum-member",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type-parameter",
        _ => "variable",
    }
}

/// [`symbol_kind_name`], re-named into the vocabulary of the language that produced it.
///
/// `SymbolKind` is a fixed enum of 26 values and every language has to squeeze into it, so a server
/// reports things under names its own users have never used: rust-analyzer sends a **trait** as
/// `Interface`, an **impl block** as `Object` and a **type alias** as `TypeParameter`, because those
/// are the closest slots that exist. A list that showed those names would be describing a language
/// the project is not written in — and the icon keyed off the name would be the wrong icon.
///
/// Only the mappings that are unambiguous are made. `Struct` covers both a struct and a **union**,
/// and `Constant` both a `const` and a `static`, so those keep the protocol's word rather than
/// guessing which one it was.
pub fn symbol_kind_name_for(kind: u8, language: &str) -> &'static str {
    let name = symbol_kind_name(kind);
    match (language, name) {
        ("rust", "interface") => "trait",
        ("rust", "object") => "impl",
        // Also `TypeParam` and `SelfType`, neither of which a server reports as a document or
        // workspace symbol — what reaches here in practice is a `type X = …`.
        ("rust", "type-parameter") => "type alias",
        _ => name,
    }
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
    pub new_name: String,
}

/// `Range | { range, placeholder } | { defaultBehavior: bool } | null` — the answer to
/// `prepareRename`, which says whether the symbol under the caret can be renamed at all
/// and what the initial text in the rename box should be.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PrepareRenameResponse {
    WithPlaceholder { range: Range, placeholder: String },
    #[serde(rename_all = "camelCase")]
    DefaultBehavior { default_behavior: bool },
    Range(Range),
    Null,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEdit {
    /// The simple form: uri → edits.
    #[serde(default)]
    pub changes: Option<HashMap<String, Vec<TextEdit>>>,
    /// The versioned form, which may also carry **resource operations** (create / rename /
    /// delete a file). rust-analyzer uses these: renaming a `mod` renames its file.
    #[serde(default)]
    pub document_changes: Option<Vec<DocumentChange>>,
}

/// One entry of `documentChanges`: either edits to a document, or a file-system
/// operation.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DocumentChange {
    /// Tried first — it is the only variant with an `edits` array.
    Edits(TextDocumentEdit),
    Resource(ResourceOperation),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentEdit {
    pub text_document: TextDocumentIdentifier,
    #[serde(default)]
    pub edits: Vec<AnnotatedTextEdit>,
}

/// A create / rename / delete of a file, tagged by its `kind`.
///
/// The per-variant `rename_all` is not redundant: the enum-level one renames the
/// *variants* (→ `"rename"`), not their fields, so without it `oldUri`/`newUri` would
/// never bind and every file rename would decode as a failure.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ResourceOperation {
    Create {
        uri: String,
    },
    #[serde(rename_all = "camelCase")]
    Rename {
        old_uri: String,
        new_uri: String,
    },
    Delete {
        uri: String,
    },
}

// ---------------------------------------------------------------------------
// Formatting / code actions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentFormattingParams {
    pub text_document: TextDocumentIdentifier,
    pub options: FormattingOptions,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormattingOptions {
    pub tab_size: u32,
    pub insert_spaces: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeActionParams {
    pub text_document: TextDocumentIdentifier,
    pub range: Range,
    pub context: CodeActionContext,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeActionContext {
    /// The diagnostics at the caret, echoed back verbatim (their opaque `data` included)
    /// — without them a server cannot produce the fix for a specific error.
    pub diagnostics: Vec<serde_json::Value>,
    pub trigger_kind: u8,
}

/// The user asked for actions (Alt+Enter), as opposed to the editor asking speculatively.
pub const CODE_ACTION_TRIGGER_INVOKED: u8 = 1;

/// `(Command | CodeAction)[]` — the legacy `Command` form and the modern one.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CodeActionOrCommand {
    /// Tried first: a `CodeAction` has no `command` **string** at the top level (its
    /// `command` is an object), so a `Command` cannot match it.
    Action(CodeAction),
    Command(Command),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeAction {
    pub title: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub is_preferred: Option<bool>,
    #[serde(default)]
    pub disabled: Option<CodeActionDisabled>,
    #[serde(default)]
    pub edit: Option<WorkspaceEdit>,
    #[serde(default)]
    pub command: Option<Command>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodeActionDisabled {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub title: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecuteCommandParams {
    pub command: String,
    pub arguments: Vec<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Semantic tokens
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTokensParams {
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTokens {
    #[serde(default)]
    pub result_id: Option<String>,
    /// Five `u32`s per token, **delta-encoded** — see [`crate::semantic`], which is where
    /// this becomes a list of spans.
    #[serde(default)]
    pub data: Vec<u32>,
}

// ── document highlight (textDocument/documentHighlight) ──────────────────────

/// One occurrence of the symbol under the caret.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentHighlight {
    pub range: Range,
    /// `1` text · `2` read · `3` write. Absent means the server did not distinguish, which is the
    /// common case — so a consumer must read `None` as "an occurrence" rather than as an error.
    #[serde(default)]
    pub kind: Option<u8>,
}

/// A read of the symbol — `let a = x;` on `x`.
pub const HIGHLIGHT_READ: u8 = 2;
/// A write to it — `x = 1;` on `x`. Worth telling apart: "where is this assigned" is a different
/// question from "where is this used", and it is the one a mutation bug is found with.
pub const HIGHLIGHT_WRITE: u8 = 3;

// ── selection range (textDocument/selectionRange) ────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRangeParams {
    pub text_document: TextDocumentIdentifier,
    /// One entry per cursor. Bennu asks about one.
    pub positions: Vec<Position>,
}

/// A range and the one that encloses it, as a linked list from the innermost outward.
///
/// The shape is what makes expand-selection a single request rather than one per keypress: the whole
/// chain from the token under the caret out to the file arrives at once, and each press walks one
/// link. Shrink is the same chain read backwards, so it needs no request at all.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRange {
    pub range: Range,
    #[serde(default)]
    pub parent: Option<Box<SelectionRange>>,
}

impl SelectionRange {
    /// The chain flattened innermost-first.
    pub fn flatten(&self) -> Vec<Range> {
        let mut out = vec![self.range];
        let mut node = self.parent.as_deref();
        // Bounded: a malformed server could hand back a cycle, and an editor must not hang on one.
        for _ in 0..64 {
            let Some(n) = node else { break };
            out.push(n.range);
            node = n.parent.as_deref();
        }
        out
    }
}

// ── folding range (textDocument/foldingRange) ────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldingRangeParams {
    pub text_document: TextDocumentIdentifier,
}

/// A foldable region, in **lines** rather than positions — folding is a line-level idea and the
/// protocol says so. The optional characters are for a server that wants the fold to start
/// mid-line; a client that ignores them folds from the end of the start line, which is what an
/// editor does anyway.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldingRange {
    pub start_line: u32,
    #[serde(default)]
    pub start_character: Option<u32>,
    pub end_line: u32,
    #[serde(default)]
    pub end_character: Option<u32>,
    /// `comment` · `imports` · `region`, or absent for an ordinary block.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub collapsed_text: Option<String>,
}

// ── code lens (textDocument/codeLens) ────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodeLensOptions {
    #[serde(default)]
    pub resolve_provider: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeLensParams {
    pub text_document: TextDocumentIdentifier,
}

/// A command attached to a line.
///
/// `command` may be absent, which is not an error and is the normal case for a server that resolves
/// lazily: the lens says *where* it is, and `codeLens/resolve` fills in what it says. rust-analyzer
/// does exactly that for its reference and implementation counts, so a client that skipped resolve
/// would render a row of blanks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeLens {
    pub range: Range,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,
    /// Opaque server state, round-tripped **verbatim** to `codeLens/resolve`. The server keyed its
    /// own bookkeeping by it, so re-deriving or dropping it turns resolve into a no-op.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ── call and type hierarchy ──────────────────────────────────────────────────

/// One node of a call or type hierarchy.
///
/// The two hierarchies share this type because the protocol gives them the same shape, and because
/// the panel that draws them is one panel: a tree whose children are fetched a level at a time.
///
/// Round-tripped to the incoming/outgoing (or super/subtype) request with `data` intact, for the
/// same reason a code lens is — it is the server's handle on the item, not a description of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyItem {
    pub name: String,
    pub kind: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub uri: String,
    /// The whole declaration.
    pub range: Range,
    /// The name token — where go-to lands.
    pub selection_range: Range,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyItemParams {
    pub item: HierarchyItem,
}

/// A caller, plus the call sites inside it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallHierarchyIncomingCall {
    pub from: HierarchyItem,
    /// Where in `from` the calls are — so a row can jump to the call rather than to the function
    /// containing it, which is the difference between one hop and reading a body.
    #[serde(default)]
    pub from_ranges: Vec<Range>,
}

/// A callee, plus the call sites that reach it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallHierarchyOutgoingCall {
    pub to: HierarchyItem,
    #[serde(default)]
    pub from_ranges: Vec<Range>,
}

// ── workspace/willRenameFiles ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFilesParams {
    pub files: Vec<FileRename>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRename {
    pub old_uri: String,
    pub new_uri: String,
}

/// `workspace.fileOperations` — the only part of the server's `workspace` capability Bennu reads.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceServerCapabilities {
    #[serde(default)]
    pub file_operations: Option<FileOperationsCapabilities>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileOperationsCapabilities {
    /// Present when the server wants to be asked before a rename. Its value carries `filters`
    /// describing which paths it cares about; Bennu asks regardless and lets the server answer with
    /// no edits, which is one round-trip against implementing glob matching on its behalf.
    #[serde(default)]
    pub will_rename: Option<serde_json::Value>,
}

// ── rust-analyzer/expandMacro ────────────────────────────────────────────────

/// The result of expanding the macro at a position.
///
/// `expansion` is Rust source as **text** — not a file the server knows about. That is the reason it
/// cannot be navigated, and the reason a nested expansion has to be asked for at a position in the
/// original file rather than at one inside this result.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroExpansion {
    /// The macro's name.
    pub name: String,
    pub expansion: String,
}

// ---------------------------------------------------------------------------
// Progress + window (server → client)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ProgressParams {
    /// `number | string`.
    pub token: crate::jsonrpc::RequestId,
    pub value: ProgressValue,
}

/// The `$/progress` payload, tagged by `kind`. This is how a server reports "indexing"
/// — the thing that makes the difference between a Rust project that looks broken for
/// its first ten seconds and one that says what it is doing.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ProgressValue {
    Begin {
        title: String,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        percentage: Option<u32>,
        #[serde(default)]
        cancellable: Option<bool>,
    },
    Report {
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        percentage: Option<u32>,
    },
    End {
        #[serde(default)]
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShowMessageParams {
    /// 1 = error, 2 = warning, 3 = info, 4 = log.
    #[serde(rename = "type")]
    pub kind: u8,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApplyWorkspaceEditParams {
    #[serde(default)]
    pub label: Option<String>,
    pub edit: WorkspaceEdit,
}

/// Method names, in one place. Typos in these are the kind of bug that costs an hour: a
/// misspelled method is a "method not found" the server reports and nothing else
/// explains.
pub mod method {
    pub const INITIALIZE: &str = "initialize";
    pub const INITIALIZED: &str = "initialized";
    pub const SHUTDOWN: &str = "shutdown";
    pub const EXIT: &str = "exit";
    pub const CANCEL_REQUEST: &str = "$/cancelRequest";

    pub const DID_OPEN: &str = "textDocument/didOpen";
    pub const DID_CHANGE: &str = "textDocument/didChange";
    pub const DID_CLOSE: &str = "textDocument/didClose";
    pub const DID_SAVE: &str = "textDocument/didSave";
    pub const DID_CHANGE_CONFIGURATION: &str = "workspace/didChangeConfiguration";

    pub const COMPLETION: &str = "textDocument/completion";
    pub const COMPLETION_RESOLVE: &str = "completionItem/resolve";
    pub const HOVER: &str = "textDocument/hover";
    pub const SIGNATURE_HELP: &str = "textDocument/signatureHelp";
    pub const DEFINITION: &str = "textDocument/definition";
    pub const TYPE_DEFINITION: &str = "textDocument/typeDefinition";
    pub const IMPLEMENTATION: &str = "textDocument/implementation";
    pub const REFERENCES: &str = "textDocument/references";
    pub const DOCUMENT_SYMBOL: &str = "textDocument/documentSymbol";
    pub const WORKSPACE_SYMBOL: &str = "workspace/symbol";
    pub const CODE_ACTION: &str = "textDocument/codeAction";
    pub const FORMATTING: &str = "textDocument/formatting";
    pub const RENAME: &str = "textDocument/rename";
    pub const PREPARE_RENAME: &str = "textDocument/prepareRename";
    pub const SEMANTIC_TOKENS_FULL: &str = "textDocument/semanticTokens/full";
    pub const DOCUMENT_HIGHLIGHT: &str = "textDocument/documentHighlight";
    pub const SELECTION_RANGE: &str = "textDocument/selectionRange";
    pub const FOLDING_RANGE: &str = "textDocument/foldingRange";
    pub const CODE_LENS: &str = "textDocument/codeLens";
    pub const CODE_LENS_RESOLVE: &str = "codeLens/resolve";
    pub const PREPARE_CALL_HIERARCHY: &str = "textDocument/prepareCallHierarchy";
    pub const CALL_HIERARCHY_INCOMING: &str = "callHierarchy/incomingCalls";
    pub const CALL_HIERARCHY_OUTGOING: &str = "callHierarchy/outgoingCalls";
    pub const PREPARE_TYPE_HIERARCHY: &str = "textDocument/prepareTypeHierarchy";
    pub const TYPE_HIERARCHY_SUPERTYPES: &str = "typeHierarchy/supertypes";
    pub const TYPE_HIERARCHY_SUBTYPES: &str = "typeHierarchy/subtypes";
    pub const WILL_RENAME_FILES: &str = "workspace/willRenameFiles";
    // ── rust-analyzer's own extensions ──────────────────────────────────────
    //
    // Not in the specification, and named here rather than inlined at the call site for the same
    // reason every other method is: a typo in a method name produces a `MethodNotFound` that reads
    // as "the server cannot do this", which is a very different report from "we asked wrongly".
    pub const RA_RELOAD_WORKSPACE: &str = "rust-analyzer/reloadWorkspace";
    pub const RA_EXPAND_MACRO: &str = "rust-analyzer/expandMacro";
    pub const EXECUTE_COMMAND: &str = "workspace/executeCommand";

    pub const PUBLISH_DIAGNOSTICS: &str = "textDocument/publishDiagnostics";
    pub const PROGRESS: &str = "$/progress";
    pub const WORK_DONE_PROGRESS_CREATE: &str = "window/workDoneProgress/create";
    pub const SHOW_MESSAGE: &str = "window/showMessage";
    pub const SHOW_MESSAGE_REQUEST: &str = "window/showMessageRequest";
    pub const LOG_MESSAGE: &str = "window/logMessage";
    pub const APPLY_EDIT: &str = "workspace/applyEdit";
    pub const REGISTER_CAPABILITY: &str = "client/registerCapability";
    pub const UNREGISTER_CAPABILITY: &str = "client/unregisterCapability";
    pub const CONFIGURATION: &str = "workspace/configuration";
    pub const WORKSPACE_FOLDERS: &str = "workspace/workspaceFolders";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chain a server hands back, flattened. The `Box` in `parent` is what makes the type
    /// recursive at all, and the bound is what stops a malformed one hanging the editor.
    #[test]
    fn a_selection_range_chain_flattens_innermost_first() {
        let chain: SelectionRange = serde_json::from_value(serde_json::json!({
            "range": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 7 } },
            "parent": {
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 9 } },
                "parent": {
                    "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 3, "character": 1 } }
                }
            }
        }))
        .expect("the protocol shape decodes");

        let flat = chain.flatten();
        assert_eq!(flat.len(), 3);
        assert_eq!((flat[0].start.character, flat[0].end.character), (4, 7), "innermost first");
        assert_eq!((flat[1].start.character, flat[1].end.character), (0, 9));
        assert_eq!(flat[2].end.line, 3, "outermost last");
    }

    /// A code lens the server left for later carries no command and no title — and a client that
    /// did not round-trip `data` back to `codeLens/resolve` would get nothing when it asked.
    #[test]
    fn an_unresolved_code_lens_keeps_its_opaque_data() {
        let lens: CodeLens = serde_json::from_value(serde_json::json!({
            "range": { "start": { "line": 7, "character": 0 }, "end": { "line": 7, "character": 3 } },
            "data": { "position": 42, "kind": "references" }
        }))
        .expect("decodes");
        assert!(lens.command.is_none(), "nothing to draw yet");

        // Serialized back for the resolve request: `data` verbatim, and no `command: null` key —
        // some servers reject an explicit null where they expect the field to be absent.
        let wire = serde_json::to_value(&lens).unwrap();
        assert_eq!(wire["data"]["kind"], "references");
        assert!(wire.get("command").is_none(), "absent, not null: {wire}");
    }

    /// The same rule for a hierarchy item, which is round-tripped to fetch the level below it.
    #[test]
    fn a_hierarchy_item_round_trips_with_its_handle_intact() {
        let raw = serde_json::json!({
            "name": "parse",
            "kind": 12,
            "uri": "file:///p/src/lib.rs",
            "range": { "start": { "line": 3, "character": 0 }, "end": { "line": 9, "character": 1 } },
            "selectionRange": { "start": { "line": 3, "character": 7 }, "end": { "line": 3, "character": 12 } },
            "data": { "id": 17 }
        });
        let item: HierarchyItem = serde_json::from_value(raw).expect("decodes");
        assert_eq!(item.name, "parse");
        let wire = serde_json::to_value(&item).unwrap();
        assert_eq!(wire["data"]["id"], 17);
        assert_eq!(wire["selectionRange"]["start"]["character"], 7, "camelCase on the way out");
        // Empty optionals stay absent rather than becoming nulls.
        assert!(wire.get("detail").is_none());
        assert!(wire.get("tags").is_none());
    }

    /// A server that says nothing about the new capabilities must read as "cannot", not as a
    /// decode failure — which is what would happen if any of them were non-optional.
    #[test]
    fn the_new_capabilities_are_all_optional() {
        let caps: ServerCapabilities =
            serde_json::from_value(serde_json::json!({})).expect("an empty capability set decodes");
        assert!(!capability_on(&caps.document_highlight_provider));
        assert!(!capability_on(&caps.selection_range_provider));
        assert!(!capability_on(&caps.folding_range_provider));
        assert!(caps.code_lens_provider.is_none());
        assert!(!capability_on(&caps.call_hierarchy_provider));
        assert!(!capability_on(&caps.type_hierarchy_provider));
        assert!(caps.workspace.is_none());
    }

    /// `workspace.fileOperations.willRename` is two levels down, and its value is an object rather
    /// than a bool — so the presence of the key is the capability.
    #[test]
    fn will_rename_is_read_from_the_nested_workspace_capability() {
        let caps: ServerCapabilities = serde_json::from_value(serde_json::json!({
            "workspace": {
                "fileOperations": {
                    "willRename": { "filters": [{ "pattern": { "glob": "**/*.rs" } }] }
                }
            }
        }))
        .expect("decodes");
        assert!(caps
            .workspace
            .as_ref()
            .and_then(|w| w.file_operations.as_ref())
            .and_then(|f| f.will_rename.as_ref())
            .is_some());
    }

    /// The `boolean | Options` shape, which is most of `ServerCapabilities`.
    #[test]
    fn a_capability_decodes_from_both_the_bool_and_the_object_form() {
        let caps: ServerCapabilities = serde_json::from_str(
            r#"{"hoverProvider":true,"renameProvider":{"prepareProvider":true},
                "referencesProvider":false}"#,
        )
        .unwrap();
        assert!(capability_on(&caps.hover_provider), "the bool form");
        assert!(capability_on(&caps.rename_provider), "the object form is also 'on'");
        assert_eq!(
            caps.rename_provider.as_ref().unwrap().options().unwrap().prepare_provider,
            Some(true)
        );
        assert!(!capability_on(&caps.references_provider), "an explicit false is off");
        assert!(!capability_on(&caps.definition_provider), "absent is off");
    }

    #[test]
    fn an_absent_position_encoding_means_utf16() {
        // The trap: a server that says nothing means UTF-16, NOT "whatever the client
        // asked for". Reading it as utf-8 puts every position in the wrong units on
        // exactly the files that contain non-ASCII.
        let caps: ServerCapabilities = serde_json::from_str("{}").unwrap();
        assert_eq!(caps.position_encoding, None);
        assert_eq!(caps.position_encoding.unwrap_or_default(), PositionEncoding::Utf16);

        let caps: ServerCapabilities =
            serde_json::from_str(r#"{"positionEncoding":"utf-8"}"#).unwrap();
        assert_eq!(caps.position_encoding, Some(PositionEncoding::Utf8));
    }

    #[test]
    fn text_document_sync_decodes_from_a_number_and_an_object() {
        let caps: ServerCapabilities = serde_json::from_str(r#"{"textDocumentSync":1}"#).unwrap();
        assert!(matches!(caps.text_document_sync, Some(TextDocumentSync::Kind(1))));
        assert!(!caps.text_document_sync.unwrap().wants_save(), "the number form says no save");

        let caps: ServerCapabilities = serde_json::from_str(
            r#"{"textDocumentSync":{"openClose":true,"change":1,"save":{"includeText":false}}}"#,
        )
        .unwrap();
        let sync = caps.text_document_sync.unwrap();
        assert!(sync.wants_save(), "rust-analyzer registers for save — it runs cargo check on it");
        assert!(!sync.save_includes_text());
    }

    #[test]
    fn save_can_be_registered_with_the_bare_bool() {
        let caps: ServerCapabilities =
            serde_json::from_str(r#"{"textDocumentSync":{"save":true}}"#).unwrap();
        assert!(caps.text_document_sync.unwrap().wants_save());
    }

    /// The four legal goto answers. Getting the untagged order wrong here silently turns
    /// every go-to into "nothing found".
    #[test]
    fn every_goto_answer_shape_flattens_to_targets() {
        let links: GotoResponse = serde_json::from_str(
            r#"[{"targetUri":"file:///a.rs",
                 "targetRange":{"start":{"line":1,"character":0},"end":{"line":9,"character":1}},
                 "targetSelectionRange":{"start":{"line":1,"character":3},"end":{"line":1,"character":7}}}]"#,
        )
        .unwrap();
        let t = links.targets();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].0, "file:///a.rs");
        assert_eq!(t[0].1.end.line, 9, "the whole declaration");
        assert_eq!(t[0].2.start.character, 3, "the name token — where the caret goes");

        let locs: GotoResponse = serde_json::from_str(
            r#"[{"uri":"file:///b.rs","range":{"start":{"line":2,"character":4},
                 "end":{"line":2,"character":8}}}]"#,
        )
        .unwrap();
        let t = locs.targets();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].1, t[0].2, "a plain Location is its own name range");

        let single: GotoResponse = serde_json::from_str(
            r#"{"uri":"file:///c.rs","range":{"start":{"line":0,"character":0},
                "end":{"line":0,"character":1}}}"#,
        )
        .unwrap();
        assert_eq!(single.targets().len(), 1);

        let null: GotoResponse = serde_json::from_str("null").unwrap();
        assert!(null.targets().is_empty());

        let empty: GotoResponse = serde_json::from_str("[]").unwrap();
        assert!(empty.targets().is_empty(), "an empty array is not an error");
    }

    #[test]
    fn a_completion_answer_decodes_from_both_the_list_and_the_array_form() {
        let list: CompletionResponse =
            serde_json::from_str(r#"{"isIncomplete":true,"items":[{"label":"push"}]}"#).unwrap();
        assert_eq!(list.into_items().len(), 1);

        let arr: CompletionResponse = serde_json::from_str(r#"[{"label":"len"}]"#).unwrap();
        assert_eq!(arr.into_items()[0].label, "len");

        let null: CompletionResponse = serde_json::from_str("null").unwrap();
        assert!(null.into_items().is_empty());
    }

    #[test]
    fn an_insert_replace_edit_uses_the_replace_range() {
        // Accepting a completion should overwrite the identifier the caret is inside, not
        // insert in front of its tail.
        let item: CompletionItem = serde_json::from_str(
            r#"{"label":"iter_mut","textEdit":{"newText":"iter_mut",
                "insert":{"start":{"line":0,"character":4},"end":{"line":0,"character":6}},
                "replace":{"start":{"line":0,"character":4},"end":{"line":0,"character":8}}}}"#,
        )
        .unwrap();
        // Bound to a local: `resolve` hands back a `&str` borrowed from the edit, so a temporary
        // would be dropped while the borrow is still live.
        let edit = item.text_edit.unwrap();
        let (range, text) = edit.resolve();
        assert_eq!(text, "iter_mut");
        assert_eq!(range.end.character, 8, "replace, not insert");
    }

    #[test]
    fn a_plain_text_edit_still_decodes() {
        let item: CompletionItem = serde_json::from_str(
            r#"{"label":"x","textEdit":{"newText":"x",
                "range":{"start":{"line":0,"character":1},"end":{"line":0,"character":2}}}}"#,
        )
        .unwrap();
        let edit = item.text_edit.unwrap();
        let (range, _) = edit.resolve();
        assert_eq!(range.start.character, 1);
    }

    #[test]
    fn additional_edits_are_kept() {
        // The `use` line an auto-imported Rust item needs. Dropping it produces code that
        // does not compile, from an accepted completion.
        let item: CompletionItem = serde_json::from_str(
            r#"{"label":"HashMap","additionalTextEdits":[{"newText":"use std::collections::HashMap;\n",
                "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}}}]}"#,
        )
        .unwrap();
        assert_eq!(item.additional_text_edits.len(), 1);
        assert!(item.additional_text_edits[0].new_text.starts_with("use std::"));
    }

    #[test]
    fn hover_contents_decodes_from_all_three_shapes() {
        let markup: Hover = serde_json::from_str(
            r#"{"contents":{"kind":"markdown","value":"```rust\nfn x()\n```"}}"#,
        )
        .unwrap();
        assert!(markup.contents.text().contains("fn x()"));

        let scalar: Hover = serde_json::from_str(r#"{"contents":"plain text"}"#).unwrap();
        assert_eq!(scalar.contents.text(), "plain text");

        let fenced: Hover =
            serde_json::from_str(r#"{"contents":{"language":"rust","value":"fn y()"}}"#).unwrap();
        assert_eq!(fenced.contents.text(), "fn y()");

        let arr: Hover =
            serde_json::from_str(r#"{"contents":["a",{"language":"rust","value":"b"}]}"#).unwrap();
        let text = arr.contents.text();
        assert!(text.contains('a') && text.contains('b'), "{text}");
    }

    #[test]
    fn a_kind_is_named_in_the_language_that_produced_it() {
        // The protocol's own words, for a language that uses them.
        assert_eq!(symbol_kind_name_for(11, "java"), "interface");
        assert_eq!(symbol_kind_name_for(19, "typescript"), "object");

        // Rust squeezes into the same 26 slots and means different things by them: a trait is sent
        // as `Interface`, an impl block as `Object`, a type alias as `TypeParameter`. Showing those
        // names would describe a language the project is not written in.
        assert_eq!(symbol_kind_name_for(11, "rust"), "trait");
        assert_eq!(symbol_kind_name_for(19, "rust"), "impl");
        assert_eq!(symbol_kind_name_for(26, "rust"), "type alias");

        // NOT re-named: `Struct` is also how a union arrives and `Constant` how a `static` does, so
        // both keep the protocol's word rather than guessing which one it was.
        assert_eq!(symbol_kind_name_for(23, "rust"), "struct");
        assert_eq!(symbol_kind_name_for(14, "rust"), "constant");
        assert_eq!(symbol_kind_name_for(12, "rust"), "function");
    }

    #[test]
    fn document_symbols_decode_nested_and_flat() {
        let nested: DocumentSymbolResponse = serde_json::from_str(
            r#"[{"name":"Foo","kind":23,
                "range":{"start":{"line":0,"character":0},"end":{"line":5,"character":1}},
                "selectionRange":{"start":{"line":0,"character":7},"end":{"line":0,"character":10}},
                "children":[{"name":"bar","kind":6,
                  "range":{"start":{"line":1,"character":2},"end":{"line":3,"character":3}},
                  "selectionRange":{"start":{"line":1,"character":5},"end":{"line":1,"character":8}}}]}]"#,
        )
        .unwrap();
        let DocumentSymbolResponse::Nested(syms) = nested else { panic!("nested") };
        assert_eq!(syms[0].children.len(), 1, "the hierarchy is what an outline needs");
        assert_eq!(symbol_kind_name(syms[0].kind), "struct");

        let flat: DocumentSymbolResponse = serde_json::from_str(
            r#"[{"name":"Foo","kind":5,"location":{"uri":"file:///a.rs",
                "range":{"start":{"line":0,"character":0},"end":{"line":1,"character":0}}}}]"#,
        )
        .unwrap();
        assert!(matches!(flat, DocumentSymbolResponse::Flat(_)));
    }

    #[test]
    fn a_workspace_edit_decodes_both_forms_and_keeps_resource_ops() {
        let simple: WorkspaceEdit = serde_json::from_str(
            r#"{"changes":{"file:///a.rs":[{"newText":"y",
                "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}}}]}}"#,
        )
        .unwrap();
        assert_eq!(simple.changes.unwrap().len(), 1);

        // Renaming a Rust `mod` renames its file: the operation must survive decoding so
        // the caller can tell the user instead of applying half a rename.
        let versioned: WorkspaceEdit = serde_json::from_str(
            r#"{"documentChanges":[
                {"textDocument":{"uri":"file:///a.rs","version":3},
                 "edits":[{"newText":"y","range":{"start":{"line":0,"character":0},
                           "end":{"line":0,"character":1}}}]},
                {"kind":"rename","oldUri":"file:///old.rs","newUri":"file:///new.rs"}]}"#,
        )
        .unwrap();
        let changes = versioned.document_changes.unwrap();
        assert_eq!(changes.len(), 2);
        assert!(matches!(changes[0], DocumentChange::Edits(_)));
        assert!(matches!(
            changes[1],
            DocumentChange::Resource(ResourceOperation::Rename { .. })
        ));
    }

    #[test]
    fn a_code_action_and_a_bare_command_both_decode() {
        let action: CodeActionOrCommand = serde_json::from_str(
            r#"{"title":"Import HashMap","kind":"quickfix","isPreferred":true}"#,
        )
        .unwrap();
        assert!(matches!(action, CodeActionOrCommand::Action(_)));

        let cmd: CodeActionOrCommand =
            serde_json::from_str(r#"{"title":"Run","command":"rust-analyzer.run"}"#).unwrap();
        assert!(matches!(cmd, CodeActionOrCommand::Command(_)));
    }

    #[test]
    fn progress_decodes_by_kind() {
        let begin: ProgressParams = serde_json::from_str(
            r#"{"token":"rustAnalyzer/Indexing","value":{"kind":"begin","title":"Indexing","percentage":0}}"#,
        )
        .unwrap();
        assert!(matches!(begin.value, ProgressValue::Begin { .. }));

        let end: ProgressParams =
            serde_json::from_str(r#"{"token":1,"value":{"kind":"end"}}"#).unwrap();
        assert!(matches!(end.value, ProgressValue::End { .. }));
    }

    #[test]
    fn a_diagnostic_code_may_be_a_string_or_a_number() {
        let d: Diagnostic = serde_json::from_str(
            r#"{"message":"mismatched types","code":"E0308","severity":1,
                "range":{"start":{"line":1,"character":4},"end":{"line":1,"character":9}}}"#,
        )
        .unwrap();
        assert_eq!(d.code.unwrap().to_string(), "E0308");

        let d: Diagnostic = serde_json::from_str(
            r#"{"message":"x","code":1234,
                "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}}}"#,
        )
        .unwrap();
        assert_eq!(d.code.unwrap().to_string(), "1234");
    }

    #[test]
    fn unknown_fields_are_ignored_not_rejected() {
        // Servers ship extensions freely; a strict decoder would fail on answers it could
        // have used. rust-analyzer's completion items carry several of its own keys.
        let item: CompletionItem = serde_json::from_str(
            r#"{"label":"x","rustAnalyzerSomethingNew":{"a":1},"unheardOf":true}"#,
        )
        .unwrap();
        assert_eq!(item.label, "x");
    }

    #[test]
    fn prepare_rename_decodes_all_three_shapes() {
        let with: PrepareRenameResponse = serde_json::from_str(
            r#"{"range":{"start":{"line":0,"character":4},"end":{"line":0,"character":7}},
                "placeholder":"foo"}"#,
        )
        .unwrap();
        assert!(matches!(with, PrepareRenameResponse::WithPlaceholder { .. }));

        let plain: PrepareRenameResponse = serde_json::from_str(
            r#"{"start":{"line":0,"character":4},"end":{"line":0,"character":7}}"#,
        )
        .unwrap();
        assert!(matches!(plain, PrepareRenameResponse::Range(_)));

        let default: PrepareRenameResponse =
            serde_json::from_str(r#"{"defaultBehavior":true}"#).unwrap();
        assert!(matches!(default, PrepareRenameResponse::DefaultBehavior { .. }));

        let null: PrepareRenameResponse = serde_json::from_str("null").unwrap();
        assert!(matches!(null, PrepareRenameResponse::Null));
    }

    #[test]
    fn params_serialize_in_the_protocol_spelling() {
        // camelCase, and no stray nulls where the spec says the key may be absent.
        let p = CompletionParams {
            text_document: TextDocumentIdentifier::new("file:///a.rs"),
            position: Position::new(3, 7),
            context: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains(r#""textDocument""#), "{json}");
        assert!(!json.contains("context"), "an absent context is omitted, not null: {json}");

        let r = ReferenceParams {
            text_document: TextDocumentIdentifier::new("file:///a.rs"),
            position: Position::new(0, 0),
            context: ReferenceContext { include_declaration: false },
        };
        assert!(serde_json::to_string(&r).unwrap().contains(r#""includeDeclaration":false"#));
    }

    #[test]
    fn a_parameter_label_may_be_text_or_a_span() {
        let text: ParameterInformation = serde_json::from_str(r#"{"label":"x: u32"}"#).unwrap();
        assert!(matches!(text.label, ParameterLabel::Text(_)));
        let span: ParameterInformation = serde_json::from_str(r#"{"label":[7,13]}"#).unwrap();
        assert!(matches!(span.label, ParameterLabel::Range([7, 13])));
    }
}
