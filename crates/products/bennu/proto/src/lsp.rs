//! Wire types for the **language-server** side of the contract.
//!
//! Their own module rather than more of `contract.rs`, which is already long: everything
//! here answers one of the LSP-backed handlers, and a reader looking for "what does the
//! Rust editor talk to" should find one file rather than a section.
//!
//! Same conventions as the rest of the contract — **UTF-8 byte offsets** and absolute
//! forward-slashed paths, so the frontend maps them against the buffer it already holds and
//! applies every edit through CodeMirror (which is what keeps undo working).
//!
//! Note what these types are *not*: they are not the LSP structs. `bennu-lsp` converts the
//! protocol's `{line, character}` positions into byte offsets before anything reaches this
//! layer, because the conversion needs the document text and only the session has it.

use serde::{Deserialize, Serialize};

/// A plain text edit: replace `[start, end)` of `file` with `new_text`.
///
/// The generic edit shape, distinct from
/// [`RenameEdit`](crate::contract::RenameEdit) — that one carries a rename's own preview
/// metadata (`old` / `reason` / `inferred`), which a formatter or a quick fix has nothing to
/// say about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEdit {
    /// Absolute path (forward slashes) of the file to edit.
    pub file: String,
    /// Start byte offset.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The replacement text.
    pub new_text: String,
}

/// One language server's live state, for the status bar and the settings panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspStatus {
    /// The catalogue / config id (`rust-analyzer`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// The language it serves (`rust`).
    pub language: String,
    /// The workspace root it was started for.
    pub root: String,
    /// The executable that was resolved and run.
    pub command: String,
    /// The server's self-reported name + version, when it gave one.
    #[serde(default)]
    pub version: Option<String>,
    /// `"starting"` | `"ready"` | `"failed"` | `"exited"`.
    pub state: String,
    /// Why it failed, or the last error it chose to show. Empty when healthy.
    pub message: String,
    /// The long-running operation the server is reporting (`"Indexing 43%"`), or empty.
    ///
    /// The difference between a Rust project that looks broken for its first ten seconds and
    /// one that says what it is doing.
    pub progress: String,
    /// Which editor features this server can actually serve (`completion`, `references`,
    /// `semantic-tokens`, …) — so the frontend never offers a menu item that would answer
    /// nothing.
    #[serde(default)]
    pub features: Vec<String>,
    /// The tail of the server's stderr. Often the only place the reason for a failed start
    /// is written down, so it is surfaced rather than kept in a log nobody opens.
    #[serde(default)]
    pub log_tail: Vec<String>,
}

/// A server Bennu knows how to run, resolved against this machine — the settings list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspServerInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    /// The file extensions it serves, without dots.
    pub extensions: Vec<String>,
    /// The resolved absolute path, or `None` when nothing was found.
    #[serde(default)]
    pub path: Option<String>,
    /// The bare command name it was looked up under.
    pub command: String,
    /// How to install it. Shown instead of a bare "not found", which leaves the user with no
    /// next step.
    pub install_hint: String,
    /// `false` when the user turned it off.
    pub enabled: bool,
    /// `true` when it comes from the user's `[[lsp.servers]]` rather than the catalogue.
    pub custom: bool,
    /// The command that installs it, argv-style — `["cargo", "install", …]`. Empty when
    /// there is none Bennu will run: a system package, or a server the user defined
    /// themselves. The settings page offers an Install button exactly when this is non-empty.
    ///
    /// `#[serde(default)]` so a frontend talking to an older backend gets an empty list —
    /// i.e. no button — rather than a missing field.
    #[serde(default)]
    pub install: Vec<String>,
}

/// One semantically-highlighted span.
///
/// Decoded in the backend, not the frontend: the protocol sends tokens delta-encoded in the
/// server's position units, so turning them into offsets needs both the buffer and its line
/// index — which is where they already are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspToken {
    /// Start byte offset.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The editor token class (`type`, `macro`, `lifetime`, `parameter`, …).
    pub class: String,
    /// Extra modifier classes the theme styles on top (`mutable`, `unsafe`, …).
    #[serde(default)]
    pub modifiers: Vec<String>,
}

/// A quick fix or refactoring offered at the caret.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspAction {
    pub title: String,
    /// The action's kind (`quickfix`, `refactor.extract`, …), or empty.
    pub kind: String,
    /// The server marked this as the obvious one to apply — the frontend puts it first.
    pub preferred: bool,
    /// Why it cannot be applied right now. Shown greyed rather than hidden: "selection
    /// crosses a block" is information, and a silently missing action is not.
    #[serde(default)]
    pub disabled: Option<String>,
    /// The edits to apply. Applied by the frontend through CodeMirror so undo works.
    #[serde(default)]
    pub edits: Vec<SourceEdit>,
    /// File creations / renames / deletions the action also wants, as human descriptions.
    ///
    /// Bennu does not perform these — it edits buffers, it does not move files on a server's
    /// behalf — so they are listed for the user rather than applied. Non-empty means the
    /// action cannot be fully carried out here.
    #[serde(default)]
    pub file_ops: Vec<String>,
    /// A server command to run instead of (or after) the edits: some actions compute their
    /// edit lazily and send it back through `workspace/applyEdit`.
    #[serde(default)]
    pub command: Option<String>,
    /// The command's arguments, opaque and echoed back untouched.
    #[serde(default)]
    pub arguments: Vec<serde_json::Value>,
}

/// Signature help for the call the caret is inside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspSignature {
    /// The whole signature on one line.
    pub label: String,
    #[serde(default)]
    pub doc: Option<String>,
    /// The parameter labels, in order.
    #[serde(default)]
    pub params: Vec<String>,
    /// Which parameter the caret is in, if known — the frontend bolds it.
    #[serde(default)]
    pub active_param: Option<usize>,
    /// The active parameter's span within `label`, in **UTF-16 code units** so the frontend
    /// can slice the label directly (its strings are UTF-16).
    #[serde(default)]
    pub active_start: Option<u32>,
    #[serde(default)]
    pub active_end: Option<u32>,
}

/// One node of a language-server outline (or one hit of a workspace-symbol search).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspSymbol {
    pub name: String,
    /// A lowercase kind name (`struct`, `function`, `field`).
    pub kind: String,
    /// The server's extra detail — for Rust, the signature or the type.
    #[serde(default)]
    pub detail: Option<String>,
    /// Byte range of the whole declaration.
    pub start: usize,
    pub end: usize,
    /// Byte range of the NAME — where go-to lands.
    pub name_start: usize,
    pub name_end: usize,
    /// 1-based line/col of the name (col in UTF-16 units, CodeMirror's own coordinate).
    pub line: usize,
    pub col: usize,
    /// The file the symbol is in. Redundant in a document outline, load-bearing in a
    /// workspace search.
    pub file: String,
    pub deprecated: bool,
    #[serde(default)]
    pub children: Vec<LspSymbol>,
}

/// One occurrence of the symbol under the caret, in the open buffer.
///
/// Not a find-usages hit: that is a workspace question with a results panel behind it. This is what
/// the editor paints while the caret rests somewhere, so it carries offsets and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspHighlight {
    pub start: usize,
    pub end: usize,
    /// `read` · `write` · `text`. `text` is what a server that did not distinguish gives, and it is
    /// the majority — so it must not be rendered as a lesser kind of occurrence.
    pub kind: String,
}

/// A foldable region, in byte offsets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspFold {
    /// Where the fold begins — the end of the header line, so what names the region stays visible.
    pub start: usize,
    pub end: usize,
    /// `comment` · `imports` · `region`, or empty for an ordinary block.
    pub kind: String,
    /// What to show in place of the folded text, when the server suggested something better than
    /// the editor's default.
    pub placeholder: String,
}

/// One code lens: where it goes, what it says, and what pressing it does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspLens {
    /// Byte offset of the item it belongs to.
    pub start: usize,
    /// 1-based line, so the editor can place it without a second conversion.
    pub line: usize,
    pub title: String,
    /// The server command it runs. Absent for a lens that is only a label — the server used the
    /// title to say something rather than to offer an action.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub arguments: Vec<serde_json::Value>,
}

/// One node of a call or type hierarchy.
///
/// The two share a shape because the protocol gives them one and because the panel that draws them
/// is one panel: a tree whose children are fetched a level at a time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspHierarchyNode {
    pub name: String,
    /// A lowercase kind name (`function`, `struct`, `trait`).
    pub kind: String,
    #[serde(default)]
    pub detail: Option<String>,
    /// Where the declaration is — the name token, so go-to lands on it.
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
    /// The trimmed source line, for a preview.
    pub preview: String,
    /// The call sites inside this node that reach the item asked about; empty for a type hierarchy.
    /// What lets a caller row jump to the call rather than to the function's head.
    #[serde(default)]
    pub call_sites: Vec<LspCallSite>,
    /// The server's own handle on this item, opaque. Sent back **verbatim** to fetch this node's
    /// children — it is a handle, not a description, so re-deriving it from the fields above would
    /// ask about something the server never offered.
    pub handle: serde_json::Value,
}

/// One call site inside a hierarchy node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspCallSite {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub preview: String,
}

/// The expansion of a macro.
///
/// The expansion is **text**, not a file the server knows about — so it cannot be navigated, and a
/// nested macro has to be expanded by pointing at it in the original file rather than in here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspMacroExpansion {
    /// The macro's name.
    pub name: String,
    /// Rust source.
    pub expansion: String,
}

/// A language-server diagnostic, with the extras the native Java checks have no equivalent
/// for.
///
/// The plain [`Diagnostic`](crate::contract::Diagnostic) is what rides the shared
/// `bennu_diagnostics` pipe, so an LSP-backed buffer needs no frontend change to get
/// squiggles. This richer shape is for the Problems panel, which can show what the compiler
/// attached: which tool spoke, and the other end of a borrow error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspDiagnostic {
    pub message: String,
    /// `"error"` | `"warning"` | `"info"` | `"hint"`.
    pub severity: String,
    /// The rule / error identifier (`E0308`, `unused_variables`), or empty.
    pub code: String,
    /// Which tool produced it (`rustc`, `clippy`). Worth showing: "clippy says" and "the
    /// compiler says" are different weights of advice.
    pub source: String,
    pub start: usize,
    pub end: usize,
    /// The server marked the code as unnecessary (unused import, dead code) — dimmed rather
    /// than underlined.
    pub unnecessary: bool,
    pub deprecated: bool,
    /// The "…and here is the other half of the problem" locations rustc attaches.
    #[serde(default)]
    pub related: Vec<LspRelated>,
}

/// One related location of an [`LspDiagnostic`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspRelated {
    pub message: String,
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
    /// The trimmed source line, for a one-line preview.
    pub preview: String,
}
