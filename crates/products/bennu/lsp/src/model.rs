//! What a session hands back: the protocol's answers, already translated.
//!
//! These types are the crate's real output surface. They are deliberately **not** LSP
//! types and **not** `bennu-proto` wire types:
//!
//! * not LSP, because every consumer would otherwise have to own the coordinate
//!   conversion, and the conversion needs the document text — which the session has and
//!   the consumer does not. Handing out `{line, character}` would push the one genuinely
//!   subtle part of this crate onto everybody who calls it.
//! * not `bennu-proto`, because that would make a language-server client depend on
//!   Bennu's IPC contract, and the mapping between them is a be-layer decision (which
//!   fields a `DeclarationTarget` wants, what a label reads like). Same layering as
//!   `bennu-intel`'s `ProjectMember`, which the be maps onto its wire `IndexEntry`.
//!
//! So: **absolute forward-slashed paths and UTF-8 byte offsets**, the two conventions the
//! rest of Bennu already uses everywhere.

pub use crate::semantic::TokenSpan;

/// A located span in a file, with the display fields a results list needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanTarget {
    /// Absolute path, forward slashes.
    pub file: String,
    /// Start byte offset of the span to select.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// 1-based line of `start`.
    pub line: usize,
    /// 1-based column of `start`, in UTF-16 code units — CodeMirror's own line
    /// coordinate, so the editor can place a caret without a second conversion.
    pub col: usize,
    /// The trimmed source line, for a results-list preview. Empty when the file could
    /// not be read (a target inside a dependency the server can see and we cannot).
    pub preview: String,
}

/// An edit to apply: replace `[start, end)` of `file` with `new_text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEdit {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub new_text: String,
}

/// One completion candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionEntry {
    /// This item's index in the answer it came from.
    ///
    /// The handle for `completionItem/resolve`: servers send a completion list without
    /// documentation and fill it in only when asked about one item (rust-analyzer does, and
    /// resolving all of them eagerly would be one round-trip per candidate). The editor
    /// sends this back for whichever item the user highlighted.
    pub id: usize,
    /// What the list shows.
    pub label: String,
    /// A lowercase kind name (`method`, `struct`, `keyword`) for the list's icon.
    pub kind: String,
    /// The right-aligned detail — a signature or a type.
    pub detail: Option<String>,
    /// Documentation, as the server's markdown.
    pub doc: Option<String>,
    /// The server's own sort key, kept because it encodes relevance the client cannot
    /// recompute (rust-analyzer puts the field you want above forty trait methods).
    pub sort_text: Option<String>,
    /// What to match the typed prefix against, when it differs from the label.
    pub filter_text: Option<String>,
    /// The text to insert. Already resolved from `insertText` / `textEdit`.
    pub insert_text: String,
    /// The byte range in the requested file that accepting this item replaces. `None`
    /// when the server gave no edit, in which case the caller replaces the identifier
    /// under the caret.
    pub replace: Option<(usize, usize)>,
    /// `true` when the server sent this as a snippet.
    ///
    /// Note what it does **not** mean: `insert_text` is plain text either way. The placeholder
    /// syntax is parsed away by [`crate::snippet`] before it reaches here, and what is left of it is
    /// [`CompletionEntry::snippet_stops`] — so a consumer that ignores both still inserts something
    /// sensible rather than `${1:value}`.
    pub is_snippet: bool,
    /// The tab stops of a snippet body, as byte ranges into `insert_text`, in visiting order.
    ///
    /// Empty for a plain completion, and empty for a snippet whose body had no placeholders — both
    /// of which are a plain insertion.
    pub snippet_stops: Vec<crate::snippet::Stop>,
    /// Edits elsewhere that must land with the insertion — for Rust, the `use` line an
    /// auto-imported item needs. Dropping them produces code that does not compile.
    pub additional_edits: Vec<FileEdit>,
    pub deprecated: bool,
    /// The server marked this as the one to pre-select.
    pub preselect: bool,
}

/// A hover card's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverText {
    /// The server's markdown, verbatim.
    pub markdown: String,
    /// The byte range the hover describes, when the server said — lets the editor
    /// underline exactly what the card is about.
    pub range: Option<(usize, usize)>,
}

/// A node of a document outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolNode {
    pub name: String,
    /// A lowercase kind name (`struct`, `function`, `field`).
    pub kind: String,
    /// The server's extra detail — for Rust, the signature or the type.
    pub detail: Option<String>,
    /// Byte range of the whole declaration (used for "reveal the body").
    pub start: usize,
    pub end: usize,
    /// Byte range of the NAME — where go-to should land.
    pub name_start: usize,
    pub name_end: usize,
    /// 1-based line/col of the name.
    pub line: usize,
    pub col: usize,
    /// The file the symbol is in. Redundant for a document outline, load-bearing for a
    /// workspace-symbol search.
    pub file: String,
    pub children: Vec<SymbolNode>,
    pub deprecated: bool,
}

/// One occurrence of the symbol under the caret, in byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    /// `read` · `write` · `text`. `text` is what a server that did not distinguish gives, and it
    /// is the majority — so a consumer must not treat it as a lesser kind of occurrence.
    pub kind: String,
}

/// A foldable region, in byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldSpan {
    /// Byte offset where the fold begins — the end of the header line, so the fold hides the body
    /// and leaves what names it on screen.
    pub start: usize,
    /// Byte offset where it ends.
    pub end: usize,
    /// `comment` · `imports` · `region`, or empty for an ordinary block.
    pub kind: String,
    /// What to show in place of the folded text, when the server suggested something better than
    /// the editor's default.
    pub placeholder: String,
}

/// One code lens: a line, a label, and what pressing it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LensEntry {
    /// Byte offset of the item the lens belongs to.
    pub start: usize,
    /// 1-based line, so a consumer can place the lens without a second conversion.
    pub line: usize,
    /// What it says. Empty when the server left it to be resolved and the resolve failed — such a
    /// lens is dropped rather than drawn blank.
    pub title: String,
    /// The server command it runs, and its arguments. `None` for a lens that is only a label.
    pub command: Option<String>,
    pub arguments: Vec<serde_json::Value>,
}

/// One node of a call or type hierarchy, with the server's own handle kept so the level below it
/// can be asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HierarchyNode {
    pub name: String,
    /// A lowercase kind name (`function`, `struct`, `trait`).
    pub kind: String,
    pub detail: Option<String>,
    /// Where the declaration is — the name token, so go-to lands on it.
    pub target: SpanTarget,
    /// The call sites inside this node that reach the item asked about, empty for a type
    /// hierarchy. What makes a caller row jump to the call rather than to the function's head.
    pub call_sites: Vec<SpanTarget>,
    /// The protocol item, serialized. Round-tripped verbatim to fetch this node's own children:
    /// it is the server's handle, not a description, so re-deriving it from the fields above
    /// would ask about a different item.
    pub handle: serde_json::Value,
}

/// One diagnostic, in byte offsets.
#[derive(Debug, Clone)]
pub struct DiagEntry {
    pub message: String,
    /// `"error"` | `"warning"` | `"info"` | `"hint"`.
    pub severity: String,
    /// The rule/error identifier (`E0308`, `unused_variables`), or empty.
    pub code: String,
    /// Which tool produced it (`rustc`, `clippy`) — worth showing, since "clippy says"
    /// and "the compiler says" are different weights of advice.
    pub source: String,
    pub start: usize,
    pub end: usize,
    /// The server marked the code as unnecessary (unused import, dead code): the editor
    /// dims it instead of underlining it.
    pub unnecessary: bool,
    pub deprecated: bool,
    /// The "…and here is the other half of the problem" locations rustc attaches.
    pub related: Vec<(SpanTarget, String)>,
    /// The diagnostic exactly as it arrived. Kept because a `codeAction` request has to
    /// echo it back — opaque `data` included — for the server to produce its fix.
    pub raw: serde_json::Value,
}

/// The outcome of a rename.
///
/// `Default` is the "nothing to do" outcome — what a caller hands back for a file whose server is
/// not up yet, which must be distinguishable from a fall-through to another engine but is not an
/// error.
#[derive(Debug, Clone, Default)]
pub struct RenameOutcome {
    pub edits: Vec<FileEdit>,
    /// File creations / renames / deletions the server also wants.
    ///
    /// Surfaced rather than applied: renaming a Rust `mod` renames its file, and a client
    /// that silently drops that half leaves the project not compiling with no hint why.
    /// The caller's job is to tell the user, not to guess.
    pub file_ops: Vec<FileOp>,
}

/// A file-system operation attached to a workspace edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOp {
    Create { file: String },
    Rename { from: String, to: String },
    Delete { file: String },
}

impl FileOp {
    /// A one-line description for the user.
    pub fn describe(&self) -> String {
        match self {
            FileOp::Create { file } => format!("create {file}"),
            FileOp::Rename { from, to } => format!("rename {from} → {to}"),
            FileOp::Delete { file } => format!("delete {file}"),
        }
    }
}

/// One offered code action / quick fix.
#[derive(Debug, Clone)]
pub struct ActionEntry {
    pub title: String,
    /// The action's kind (`quickfix`, `refactor.extract`, …), or empty.
    pub kind: String,
    /// The server marked it as the obvious one to apply.
    pub preferred: bool,
    /// Why it cannot be applied right now, when the server said so — shown greyed rather
    /// than hidden, because "cannot extract: selection crosses a block" is information.
    pub disabled: Option<String>,
    /// The edits, when the action carries them inline.
    pub edits: Vec<FileEdit>,
    pub file_ops: Vec<FileOp>,
    /// A server command to run instead of (or after) applying the edits. Some actions are
    /// only available this way — the server computes the edit lazily and sends it back
    /// through `workspace/applyEdit`.
    pub command: Option<(String, Vec<serde_json::Value>)>,
}

/// Signature help at the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureText {
    /// The whole signature as one line.
    pub label: String,
    pub doc: Option<String>,
    /// The parameter labels, in order.
    pub params: Vec<String>,
    /// Which parameter the caret is in, if known — the editor bolds it.
    pub active_param: Option<usize>,
    /// The character range of the active parameter **within `label`**, when the server
    /// expressed it that way. In UTF-16 units, so the editor can slice `label` directly.
    pub active_param_range: Option<(u32, u32)>,
}

/// What a server is doing right now, for the status bar and the settings panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStatus {
    /// The catalogue / config id (`rust-analyzer`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// The language it serves (`rust`).
    pub language: String,
    /// The workspace root it was started for.
    pub root: String,
    /// The resolved executable, or the bare command name when nothing resolved.
    pub command: String,
    /// The server's self-reported name + version from the handshake, when it gave one.
    pub version: Option<String>,
    pub state: SessionState,
    /// Why it is failed / what it is doing — a message for the user.
    pub message: String,
    /// The current long-running operation the server reported via `$/progress`
    /// (`"Indexing 43%"`), or empty.
    pub progress: String,
    /// The tail of the server's stderr. The only place the reason a server refused to
    /// start is ever written down.
    pub log_tail: Vec<String>,
}

/// A session's lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Spawned; the `initialize` handshake has not finished.
    Starting,
    /// Handshake done — requests are served.
    Ready,
    /// The process could not be started, or the handshake failed.
    Failed,
    /// The process exited (crashed, or was stopped).
    Exited,
}

impl SessionState {
    /// The lowercase name for the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::Starting => "starting",
            SessionState::Ready => "ready",
            SessionState::Failed => "failed",
            SessionState::Exited => "exited",
        }
    }
}

/// A server Bennu knows how to run, resolved against this machine — what the settings
/// panel lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAvailability {
    pub id: String,
    pub name: String,
    pub language: String,
    /// File extensions it serves.
    pub extensions: Vec<String>,
    /// The resolved absolute path, or `None` when nothing was found.
    pub path: Option<String>,
    /// The bare command name it was looked up under.
    pub command: String,
    /// How to install it, when it wasn't found. Shown instead of a bare "not found",
    /// because "not found" leaves the user with no next step.
    pub install_hint: String,
    /// `false` when the user turned this server off.
    pub enabled: bool,
    /// `true` when it came from the user's own config rather than the built-in catalogue.
    pub custom: bool,
    /// The command that installs it, argv-style. Empty when there is none Bennu will run —
    /// a system package, or a server the user defined themselves. The settings page offers
    /// an Install button exactly when this is non-empty.
    pub install: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_state_has_a_stable_wire_name() {
        // The FE switches on these strings; renaming one silently breaks a status pill.
        assert_eq!(SessionState::Starting.as_str(), "starting");
        assert_eq!(SessionState::Ready.as_str(), "ready");
        assert_eq!(SessionState::Failed.as_str(), "failed");
        assert_eq!(SessionState::Exited.as_str(), "exited");
    }

    #[test]
    fn a_file_op_describes_itself_for_the_user() {
        assert_eq!(
            FileOp::Rename { from: "/a/old.rs".into(), to: "/a/new.rs".into() }.describe(),
            "rename /a/old.rs → /a/new.rs"
        );
        assert_eq!(FileOp::Delete { file: "/a/x.rs".into() }.describe(), "delete /a/x.rs");
    }
}
