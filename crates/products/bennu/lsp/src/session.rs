//! One language server, initialized and ready to answer questions about a workspace.
//!
//! This is where the protocol becomes a *service*: [`LspClient`] moves messages, and this
//! layer decides which messages to send, keeps the server's copy of each document in step
//! with the editor's, and hands answers back in Bennu's own coordinates ([`crate::model`]).
//!
//! Four things it is responsible for, each of which is a bug if skipped:
//!
//! * **The handshake.** `initialize` → read the capabilities → `initialized`. Nothing may
//!   be sent in between, and everything afterwards is gated on what came back: asking a
//!   server for a capability it never advertised earns a "method not found" per keystroke.
//! * **Position encoding.** Negotiated in the handshake and remembered. A server that says
//!   nothing means UTF-16, *not* what we asked for — assuming otherwise puts every position
//!   in the wrong units on exactly the files that contain non-ASCII.
//! * **Document sync.** The editor owns the buffer; the server has a copy. Every
//!   position-based request re-syncs first ([`LspSession::sync`]), because a request whose
//!   offsets refer to text the server has not seen is answered confidently and wrongly.
//! * **State worth showing.** rust-analyzer takes tens of seconds to index a cold project
//!   and answers almost nothing until it has. A client that hides that is a client that
//!   looks broken; the progress and message traffic is kept so the UI can say what is
//!   happening.
//!
//! The feature requests themselves live in [`crate::ops`], and the response → model
//! conversions in [`crate::convert`], to keep each file about one thing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::client::{LspClient, LspError, ServerHandler};
use crate::line_index::PositionEncoding;
use crate::model::{FileEdit, FileOp, ServerStatus, SessionState};
use crate::types::{
    self, capability_on, method, InitializeParams, InitializeResult, SemanticTokensLegend,
    ServerCapabilities, WorkspaceFolder,
};
use crate::uri;

/// How long to wait for `initialize`.
///
/// Generous because it is once per session and the alternative failure is worse: a server
/// that was still starting gets killed and reported as broken. Note that this covers the
/// *handshake* only — rust-analyzer answers `initialize` promptly and then indexes in the
/// background, reporting progress, which is why indexing time does not belong here.
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to let the stderr drain catch up before reporting a failed start.
///
/// Only on the failure path. A server that dies on startup writes its reason and exits, and the
/// caller is released by the reader hitting EOF — a different thread from the one reading stderr.
const STDERR_GRACE: Duration = Duration::from_millis(300);

/// Everything needed to start one server for one workspace.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Catalogue / config id (`rust-analyzer`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// The LSP `languageId` for documents in this session.
    pub language: String,
    /// The resolved executable.
    pub command: String,
    pub args: Vec<String>,
    /// The workspace root the server is opened on.
    pub root: PathBuf,
    pub init_options: Option<serde_json::Value>,
    /// Extra environment for the child.
    pub env: Vec<(String, String)>,
}

/// Why a session could not be started, with the evidence.
///
/// The `log_tail` is the point: a server that starts and then dies during the handshake
/// says why on stderr and nowhere else, so a failure that dropped it could only ever
/// report "the handshake failed".
#[derive(Debug, Clone)]
pub struct StartFailure {
    pub message: String,
    pub log_tail: Vec<String>,
}

impl std::fmt::Display for StartFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// What the host wants to hear about, as it happens.
///
/// Implemented by the be layer, which turns each callback into a frontend event. Kept
/// narrow on purpose: a session reports *what changed*, never *what to draw*.
pub trait SessionObserver: Send + Sync {
    /// The server published diagnostics for `file` (an absolute forward-slashed path).
    /// Push, not poll — rust-analyzer's real diagnostics arrive seconds after a save, when
    /// `cargo check` finishes, so nothing else would surface them.
    fn diagnostics_published(&self, _file: &str) {}

    /// The session's state, progress line or message changed — re-read [`LspSession::status`].
    fn status_changed(&self) {}

    /// A `window/showMessage`: `level` is `"error"` / `"warning"` / `"info"` / `"log"`.
    fn message(&self, _level: &str, _text: &str) {}

    /// The server wants to edit the workspace itself. Return whether it was applied.
    ///
    /// Runs on a worker thread, so this may block.
    fn apply_edit(&self, _edits: Vec<FileEdit>, _file_ops: Vec<FileOp>) -> bool {
        false
    }
}

/// One document as the server currently sees it.
pub(crate) struct DocState {
    pub(crate) version: i32,
    pub(crate) text: String,
}

/// The parts of a session the transport's callbacks also need.
///
/// Split out because of an ownership knot: the client needs a handler at construction, the
/// handler needs to reach the session's state, and the session owns the client. Holding the
/// shared state in its own `Arc` unties it without a `Weak` dance — and nothing in here
/// points back at the client, so there is no cycle.
pub(crate) struct SessionShared {
    pub(crate) status: Mutex<StatusInner>,
    /// Open documents, keyed by absolute forward-slashed path.
    pub(crate) docs: Mutex<HashMap<String, DocState>>,
    /// The latest diagnostics per file, **exactly as the server sent them**.
    ///
    /// Stored unconverted on purpose. They arrive on the reader thread, which must not
    /// block, and converting a range to a byte offset needs the file's text — which for a
    /// file that is not open means reading it from disk. Converting lazily also means the
    /// conversion uses the caller's own live buffer, which is the more accurate answer
    /// anyway.
    pub(crate) diagnostics: Mutex<HashMap<String, Vec<types::Diagnostic>>>,
    /// The items from the most recent completion answer, and the file they were computed
    /// for — the backing store for `completionItem/resolve`.
    ///
    /// One list, replaced each time: completion is inherently a single live interaction, so
    /// anything older than the list currently on screen is unreachable by construction.
    pub(crate) last_completion: Mutex<Option<(String, Vec<types::CompletionItem>)>>,
    /// The negotiated position encoding, written once the handshake settles.
    ///
    /// It lives here rather than on the handler because of the order things are built in:
    /// the handler has to exist before the client, the client before `initialize`, and the
    /// encoding is only known from `initialize`'s answer. Shared state is the seam that
    /// lets the value arrive after its reader was constructed.
    pub(crate) encoding: Mutex<PositionEncoding>,
    pub(crate) observer: Arc<dyn SessionObserver>,
}

/// The mutable half of [`ServerStatus`].
pub(crate) struct StatusInner {
    pub(crate) state: SessionState,
    pub(crate) message: String,
    pub(crate) progress: String,
    pub(crate) version: Option<String>,
    /// Live `$/progress` tokens → their rendered line, so two concurrent operations don't
    /// overwrite each other's text.
    pub(crate) progress_tokens: HashMap<String, String>,
}

impl SessionShared {
    fn new(observer: Arc<dyn SessionObserver>) -> Self {
        Self {
            status: Mutex::new(StatusInner {
                state: SessionState::Starting,
                message: String::new(),
                progress: String::new(),
                version: None,
                progress_tokens: HashMap::new(),
            }),
            docs: Mutex::new(HashMap::new()),
            diagnostics: Mutex::new(HashMap::new()),
            last_completion: Mutex::new(None),
            encoding: Mutex::new(PositionEncoding::default()),
            observer: Arc::clone(&observer),
        }
    }

    fn set_state(&self, state: SessionState, message: &str) {
        {
            let mut s = self.status.lock().unwrap_or_else(|p| p.into_inner());
            s.state = state;
            if !message.is_empty() {
                s.message = message.to_string();
            }
        }
        self.observer.status_changed();
    }
}

/// A running, initialized language server.
pub struct LspSession {
    pub(crate) cfg: SessionConfig,
    pub(crate) client: Arc<LspClient>,
    pub(crate) shared: Arc<SessionShared>,
    /// The capabilities from the handshake. Immutable afterwards — dynamic registration is
    /// acknowledged but not acted on (see `answer_request` in [`crate::client`]).
    pub(crate) caps: ServerCapabilities,
    /// The negotiated position encoding.
    pub(crate) encoding: PositionEncoding,
    /// The semantic-token vocabulary, when the server has one.
    pub(crate) legend: Option<SemanticTokensLegend>,
}

impl LspSession {
    /// Spawn the server and complete the handshake. **Blocks** — the caller runs it off any
    /// thread that must stay responsive.
    pub fn start(
        cfg: SessionConfig,
        observer: Arc<dyn SessionObserver>,
    ) -> Result<Arc<Self>, StartFailure> {
        let shared = Arc::new(SessionShared::new(Arc::clone(&observer)));
        let folders = vec![WorkspaceFolder {
            uri: uri::to_uri(&cfg.root.to_string_lossy()),
            name: cfg
                .root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| cfg.root.to_string_lossy().to_string()),
        }];

        let handler: Arc<dyn ServerHandler> =
            Arc::new(SessionHandler { shared: Arc::clone(&shared) });

        let client = LspClient::spawn(
            &cfg.id,
            &cfg.command,
            &cfg.args,
            &cfg.root,
            &cfg.env,
            folders.clone(),
            Arc::clone(&handler),
        )
        .map_err(|message| StartFailure { message, log_tail: Vec::new() })?;

        let params = InitializeParams {
            process_id: Some(std::process::id()),
            client_info: Some(types::ClientInfo {
                name: "Bennu".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
            // Both roots are sent: `rootUri` is deprecated, and is still the only one some
            // servers read.
            root_uri: Some(folders[0].uri.clone()),
            workspace_folders: folders,
            capabilities: client_capabilities(),
            initialization_options: cfg.init_options.clone(),
        };

        let result: InitializeResult = client
            .request(method::INITIALIZE, params, INITIALIZE_TIMEOUT)
            .map_err(|e| {
                // The server's own stderr, waited for briefly — it is where a refusal to start
                // explains itself, and a process that dies during the handshake releases this
                // caller before the drain thread has necessarily read the line.
                //
                // This is the difference between "the language server is not running" and
                // "Unknown binary 'rust-analyzer' in official toolchain", which is the one that
                // tells the user what to do.
                let log_tail = client.log_tail_settled(STDERR_GRACE);
                let message = match LspClient::failure_line(&log_tail) {
                    Some(line) => format!("{line} ({e})"),
                    None => e.to_string(),
                };
                client.shutdown();
                StartFailure { message, log_tail }
            })?;

        // The server is only allowed to receive other messages after this.
        let _ = client.notify(method::INITIALIZED, serde_json::json!({}));

        // An absent `positionEncoding` means UTF-16 — the protocol's default, NOT the first
        // thing we asked for. Reading it the other way puts every position in the wrong
        // units on exactly the files that contain non-ASCII.
        let encoding = result.capabilities.position_encoding.unwrap_or_default();
        // Publish it for the callback paths (`workspace/applyEdit` converts ranges, and it
        // runs on a worker thread with no access to this stack).
        *shared.encoding.lock().unwrap_or_else(|p| p.into_inner()) = encoding;

        let version = result.server_info.as_ref().map(|i| match &i.version {
            Some(v) => format!("{} {}", i.name, v),
            None => i.name.clone(),
        });
        {
            let mut s = shared.status.lock().unwrap_or_else(|p| p.into_inner());
            s.state = SessionState::Ready;
            s.version = version;
            s.message.clear();
        }
        observer.status_changed();

        let legend = result
            .capabilities
            .semantic_tokens_provider
            .as_ref()
            .map(|p| p.legend.clone())
            .filter(|l| !l.token_types.is_empty());

        Ok(Arc::new(Self {
            cfg,
            client,
            shared,
            caps: result.capabilities,
            encoding,
            legend,
        }))
    }

    /// The session's config.
    pub fn config(&self) -> &SessionConfig {
        &self.cfg
    }

    /// The workspace root.
    pub fn root(&self) -> &Path {
        &self.cfg.root
    }

    /// The language this session serves.
    pub fn language(&self) -> &str {
        &self.cfg.language
    }

    /// Whether the server process is still up.
    pub fn is_alive(&self) -> bool {
        self.client.is_alive()
    }

    /// The current status, for the status bar and the settings panel.
    pub fn status(&self) -> ServerStatus {
        let s = self.shared.status.lock().unwrap_or_else(|p| p.into_inner());
        let state = if s.state == SessionState::Ready && !self.client.is_alive() {
            SessionState::Exited
        } else {
            s.state
        };
        ServerStatus {
            id: self.cfg.id.clone(),
            name: self.cfg.name.clone(),
            language: self.cfg.language.clone(),
            root: self.cfg.root.to_string_lossy().replace('\\', "/"),
            command: self.cfg.command.clone(),
            version: s.version.clone(),
            state,
            message: s.message.clone(),
            progress: s.progress.clone(),
            log_tail: self.client.log_tail(),
        }
    }

    /// Stop the server.
    pub fn shutdown(&self) {
        self.client.shutdown();
        self.shared.set_state(SessionState::Exited, "stopped");
    }

    /// Bring the server's copy of `file` in step with `source`, opening it if needed.
    ///
    /// Called at the top of every position-based request rather than driven by editor
    /// events alone. The reason is a failure that is invisible when it happens: if the
    /// server's copy is one keystroke behind, the offsets in the request refer to text it
    /// does not have, and it answers — confidently — about the wrong span. Re-syncing costs
    /// a string comparison and removes the whole class.
    pub fn sync(&self, file: &str, source: &str) -> Result<(), LspError> {
        let uri = uri::to_uri(file);
        enum Action {
            Open,
            Change(i32),
            None,
        }
        let action = {
            let mut docs = self.shared.docs.lock().unwrap_or_else(|p| p.into_inner());
            match docs.get_mut(file) {
                None => {
                    docs.insert(file.to_string(), DocState { version: 1, text: source.to_string() });
                    Action::Open
                }
                Some(doc) if doc.text != source => {
                    doc.version += 1;
                    doc.text = source.to_string();
                    Action::Change(doc.version)
                }
                Some(_) => Action::None,
            }
        };
        match action {
            Action::Open => self.client.notify(
                method::DID_OPEN,
                types::DidOpenTextDocumentParams {
                    text_document: types::TextDocumentItem {
                        uri,
                        language_id: self.cfg.language.clone(),
                        version: 1,
                        text: source.to_string(),
                    },
                },
            ),
            Action::Change(version) => self.client.notify(
                method::DID_CHANGE,
                types::DidChangeTextDocumentParams {
                    text_document: types::VersionedTextDocumentIdentifier { uri, version },
                    content_changes: vec![types::TextDocumentContentChangeEvent {
                        text: source.to_string(),
                    }],
                },
            ),
            Action::None => Ok(()),
        }
    }

    /// Tell the server a file was saved.
    ///
    /// Worth its own call rather than folding into [`sync`](Self::sync): for rust-analyzer
    /// this is the trigger for `cargo check`, which is what produces real type and borrow
    /// errors. Skipped entirely when the server did not register for saves — sending them
    /// anyway would provoke builds nobody asked for.
    pub fn did_save(&self, file: &str, source: &str) {
        let Some(sync) = self.caps.text_document_sync.as_ref() else { return };
        if !sync.wants_save() {
            return;
        }
        let _ = self.sync(file, source);
        let _ = self.client.notify(
            method::DID_SAVE,
            types::DidSaveTextDocumentParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
                text: sync.save_includes_text().then(|| source.to_string()),
            },
        );
    }

    /// Tell the server a file was closed, so it can drop its copy (and, for most servers,
    /// its diagnostics for it).
    pub fn did_close(&self, file: &str) {
        let was_open =
            self.shared.docs.lock().unwrap_or_else(|p| p.into_inner()).remove(file).is_some();
        if !was_open {
            return;
        }
        let _ = self.client.notify(
            method::DID_CLOSE,
            types::DidCloseTextDocumentParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
            },
        );
    }

    /// Whether `file` is open in this session.
    pub fn is_open(&self, file: &str) -> bool {
        self.shared.docs.lock().unwrap_or_else(|p| p.into_inner()).contains_key(file)
    }

    /// The text of `file` as the session knows it — the open buffer, else the file on disk.
    ///
    /// `None` when neither is available, which happens for a real reason: a server can
    /// resolve a target inside a dependency it has sources for and we do not.
    pub(crate) fn text_of(&self, file: &str) -> Option<String> {
        if let Some(doc) = self.shared.docs.lock().unwrap_or_else(|p| p.into_inner()).get(file) {
            return Some(doc.text.clone());
        }
        std::fs::read_to_string(file).ok()
    }

    /// A gate + a name for it, so an unsupported feature reads as "the server does not do
    /// this" instead of as an error from a request that should never have been sent.
    pub(crate) fn require(&self, on: bool, what: &'static str) -> Result<(), LspError> {
        if on {
            Ok(())
        } else {
            Err(LspError::Unsupported(what))
        }
    }

    /// The server capabilities, for the feature gates in [`crate::ops`].
    pub(crate) fn caps(&self) -> &ServerCapabilities {
        &self.caps
    }

    /// The characters that should trigger completion automatically, from the handshake.
    ///
    /// Read rather than assumed: for Rust the set is `.` and `:` (for `::`), and a
    /// Java-shaped guess of just `.` would leave path completion silent.
    pub fn completion_trigger_characters(&self) -> Vec<String> {
        self.caps
            .completion_provider
            .as_ref()
            .map(|c| c.trigger_characters.clone())
            .unwrap_or_default()
    }

    /// The characters that should open signature help.
    pub fn signature_trigger_characters(&self) -> Vec<String> {
        self.caps
            .signature_help_provider
            .as_ref()
            .map(|c| c.trigger_characters.clone())
            .unwrap_or_default()
    }

    /// Which of Bennu's editor features this server can actually serve — what the FE needs
    /// to know so it does not offer a menu item that will answer nothing.
    pub fn features(&self) -> Vec<&'static str> {
        let c = &self.caps;
        let mut out = Vec::new();
        if c.completion_provider.is_some() {
            out.push("completion");
        }
        if capability_on(&c.hover_provider) {
            out.push("hover");
        }
        if capability_on(&c.definition_provider) {
            out.push("definition");
        }
        if capability_on(&c.type_definition_provider) {
            out.push("type-definition");
        }
        if capability_on(&c.implementation_provider) {
            out.push("implementation");
        }
        if capability_on(&c.references_provider) {
            out.push("references");
        }
        if capability_on(&c.document_symbol_provider) {
            out.push("symbols");
        }
        if capability_on(&c.workspace_symbol_provider) {
            out.push("workspace-symbols");
        }
        if capability_on(&c.rename_provider) {
            out.push("rename");
        }
        if capability_on(&c.document_formatting_provider) {
            out.push("format");
        }
        if capability_on(&c.code_action_provider) {
            out.push("code-actions");
        }
        if c.signature_help_provider.is_some() {
            out.push("signature-help");
        }
        if self.legend.is_some() {
            out.push("semantic-tokens");
        }
        out
    }
}

/// The transport callbacks, wired to a session's shared state.
struct SessionHandler {
    shared: Arc<SessionShared>,
}

impl ServerHandler for SessionHandler {
    fn on_diagnostics(&self, params: types::PublishDiagnosticsParams) {
        let Some(file) = uri::from_uri(&params.uri) else { return };
        {
            let mut all = self.shared.diagnostics.lock().unwrap_or_else(|p| p.into_inner());
            if params.diagnostics.is_empty() {
                // An empty list means "this file is clean now" — the server's way of
                // retracting. Dropping the key rather than storing an empty vec keeps the
                // map from growing to one entry per file in the workspace.
                all.remove(&file);
            } else {
                all.insert(file.clone(), params.diagnostics);
            }
        }
        self.shared.observer.diagnostics_published(&file);
    }

    fn on_progress(&self, params: types::ProgressParams) {
        let token = params.token.to_string();
        {
            let mut s = self.shared.status.lock().unwrap_or_else(|p| p.into_inner());
            match params.value {
                types::ProgressValue::Begin { title, message, percentage, .. } => {
                    s.progress_tokens.insert(token, render_progress(&title, &message, percentage));
                }
                types::ProgressValue::Report { message, percentage } => {
                    // Keep the title from `begin`: a bare "43%" says nothing about what is
                    // at 43%.
                    let title = s
                        .progress_tokens
                        .get(&token)
                        .and_then(|line| line.split(['\u{2026}', '(']).next())
                        .map(|t| t.trim().to_string())
                        .unwrap_or_default();
                    s.progress_tokens.insert(token, render_progress(&title, &message, percentage));
                }
                types::ProgressValue::End { .. } => {
                    s.progress_tokens.remove(&token);
                }
            }
            // One line, deterministic: sorted by token so the shown operation does not
            // flicker between two concurrent ones.
            let mut lines: Vec<&String> = s.progress_tokens.values().collect();
            lines.sort();
            s.progress = lines.first().map(|l| l.to_string()).unwrap_or_default();
        }
        self.shared.observer.status_changed();
    }

    fn on_message(&self, params: types::ShowMessageParams, is_log: bool) {
        let level = match params.kind {
            1 => "error",
            2 => "warning",
            3 => "info",
            _ => "log",
        };
        if is_log {
            // A server's log stream is verbose by design; it belongs in the log tail, not
            // in the user's face.
            eprintln!("[lsp] {level}: {}", params.message);
            return;
        }
        // An error the server chose to *show* is worth recording as the session's message:
        // it is usually the reason the next request will fail.
        if params.kind == 1 {
            let mut s = self.shared.status.lock().unwrap_or_else(|p| p.into_inner());
            s.message = params.message.clone();
        }
        self.shared.observer.message(level, &params.message);
    }

    fn on_apply_edit(&self, params: types::ApplyWorkspaceEditParams) -> bool {
        // On a worker thread, so the disk reads the conversion needs are allowed here.
        let encoding = *self.shared.encoding.lock().unwrap_or_else(|p| p.into_inner());
        let read = |file: &str| -> Option<String> {
            self.shared
                .docs
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(file)
                .map(|d| d.text.clone())
                .or_else(|| std::fs::read_to_string(file).ok())
        };
        let (edits, ops) = crate::convert::workspace_edit(&params.edit, encoding, &read);
        self.shared.observer.apply_edit(edits, ops)
    }

    fn on_exit(&self, reason: &str) {
        self.shared.set_state(SessionState::Exited, reason);
    }
}

/// `"Indexing: bennu-lsp 43%"` — one **bounded** line from a progress notification.
///
/// Bounded is the operative word. A server's progress `message` is free text and rust-analyzer
/// routinely puts an absolute path in it (the manifest it is loading), so a naive concatenation
/// produces a status line hundreds of characters wide — which in a single-row footer means the
/// strip stretches and everything else on it moves. The title and the percentage are what carry
/// the meaning; the message is context and is what gets cut.
fn render_progress(title: &str, message: &Option<String>, percentage: Option<u32>) -> String {
    let mut line = title.trim().to_string();
    if let Some(m) = message.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        let m = short_progress_message(m);
        if line.is_empty() {
            line = m;
        } else {
            line.push_str(&format!(": {m}"));
        }
    }
    if let Some(p) = percentage {
        line.push_str(&format!(" {p}%"));
    }
    // A last-resort cap on the whole line, for a server whose *title* is the long part.
    ellipsize(&line, MAX_PROGRESS_LINE)
}

/// The message part of a progress line, reduced to something a footer row can hold.
const MAX_PROGRESS_MESSAGE: usize = 40;
/// The whole rendered line's ceiling.
const MAX_PROGRESS_LINE: usize = 72;

/// Shorten a progress `message` for a one-row status line.
///
/// A path is reduced to its **last segment**, because that is the informative part — "which crate"
/// rather than "where the crate lives" — and it is also what turns a 120-character line into a
/// 12-character one. Anything else is simply truncated.
fn short_progress_message(message: &str) -> String {
    // Only treat it as a path when it looks like one *and* has a usable tail: a message that
    // merely contains a slash (`3/456`, which rust-analyzer sends for cache priming) must keep
    // both halves, since the ratio is the whole content.
    let looks_like_path = message.contains('/') || message.contains('\\');
    let is_ratio = message.split('/').all(|p| p.chars().all(|c| c.is_ascii_digit()));
    if looks_like_path && !is_ratio && message.len() > MAX_PROGRESS_MESSAGE {
        if let Some(tail) = message.rsplit(['/', '\\']).find(|s| !s.is_empty()) {
            return ellipsize(tail, MAX_PROGRESS_MESSAGE);
        }
    }
    ellipsize(message, MAX_PROGRESS_MESSAGE)
}

/// Truncate to `max` **characters** with an ellipsis. Character-wise, not byte-wise, so a
/// multi-byte crate name is never cut in half.
fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}

/// What Bennu tells the server it can handle.
///
/// This is the actual contract, so it is one visible literal rather than a hundred lines of
/// nested builders. Each block below is claimed because Bennu implements it; the notable
/// **omission** is `workspace.workspaceEdit.resourceOperations`, and that is deliberate:
/// Bennu applies text edits through the editor (so undo works) but does not create, move or
/// delete files on a server's behalf. A server told we support resource operations would
/// answer a Rust `mod` rename with a file move we would then silently drop, leaving the
/// project not compiling. Not claiming it means the server refuses that rename outright,
/// which is a limitation the user can see.
fn client_capabilities() -> serde_json::Value {
    serde_json::json!({
        "general": {
            // Ours first: Bennu's own coordinate is the UTF-8 byte offset, so a server that
            // agrees removes a conversion. Most will pick utf-16, which is why the
            // conversion exists regardless.
            "positionEncodings": ["utf-8", "utf-16"],
        },
        "workspace": {
            "workspaceFolders": true,
            "configuration": true,
            "applyEdit": true,
            "workspaceEdit": {
                "documentChanges": true,
                "failureHandling": "abort",
            },
            "didChangeConfiguration": { "dynamicRegistration": false },
            "symbol": {
                "symbolKind": { "valueSet": (1..=26).collect::<Vec<u8>>() },
            },
            "executeCommand": {},
            // Asked before a rename so the server can say what else has to change — a Rust `mod`
            // declaration and every `use` path through the moved module. Only `willRename`: Bennu
            // does not announce creations or deletions, and claiming to would have the server
            // waiting for notifications that never come.
            "fileOperations": { "willRename": true },
        },
        "textDocument": {
            "synchronization": {
                "didSave": true,
                "willSave": false,
                "willSaveWaitUntil": false,
            },
            "completion": {
                "contextSupport": true,
                "completionItem": {
                    "snippetSupport": true,
                    "insertReplaceSupport": true,
                    "labelDetailsSupport": true,
                    "deprecatedSupport": true,
                    "preselectSupport": true,
                    "documentationFormat": ["markdown", "plaintext"],
                    "tagSupport": { "valueSet": [1] },
                    "resolveSupport": {
                        "properties": ["documentation", "detail", "additionalTextEdits"],
                    },
                },
                "completionItemKind": { "valueSet": (1..=25).collect::<Vec<u8>>() },
            },
            "hover": { "contentFormat": ["markdown", "plaintext"] },
            "signatureHelp": {
                "signatureInformation": {
                    "documentationFormat": ["markdown", "plaintext"],
                    // Without this the server sends parameter labels as strings, and
                    // highlighting the active one means substring-searching the signature.
                    "parameterInformation": { "labelOffsetSupport": true },
                    "activeParameterSupport": true,
                },
            },
            // `linkSupport` is what makes a goto answer carry the name range separately
            // from the whole declaration — the difference between landing on `fn` and
            // landing on the function's name.
            "definition": { "linkSupport": true },
            "typeDefinition": { "linkSupport": true },
            "implementation": { "linkSupport": true },
            "references": {},
            "documentSymbol": {
                "hierarchicalDocumentSymbolSupport": true,
                "symbolKind": { "valueSet": (1..=26).collect::<Vec<u8>>() },
            },
            "rename": { "prepareSupport": true },
            "formatting": {},
            "documentHighlight": {},
            "selectionRange": {},
            "foldingRange": {
                "lineFoldingOnly": true,
                "foldingRangeKind": { "valueSet": ["comment", "imports", "region"] },
                "foldingRange": { "collapsedText": true },
            },
            // `resolveProvider` on the server's side is only honoured when the client says it will
            // resolve — and rust-analyzer returns its reference and implementation counts with no
            // title at all, filling them in on resolve. Without this the lenses arrive blank.
            "codeLens": { "dynamicRegistration": false },
            "callHierarchy": {},
            "typeHierarchy": {},
            "codeAction": {
                "isPreferredSupport": true,
                "disabledSupport": true,
                "dataSupport": true,
                "codeActionLiteralSupport": {
                    "codeActionKind": {
                        "valueSet": [
                            "", "quickfix", "refactor", "refactor.extract",
                            "refactor.inline", "refactor.rewrite", "source",
                            "source.organizeImports", "source.fixAll",
                        ],
                    },
                },
            },
            "publishDiagnostics": {
                "relatedInformation": true,
                "versionSupport": true,
                "codeDescriptionSupport": true,
                "dataSupport": true,
                "tagSupport": { "valueSet": [1, 2] },
            },
            "semanticTokens": {
                "requests": { "full": true, "range": false },
                "formats": ["relative"],
                // NOT claimed: a token's `length` is counted within its line, and the decoder
                // resolves its end by asking the line index — which clamps to the line's end. A
                // multiline token would therefore be painted only across its first line. Saying
                // no makes the server split it per line, which is exactly what the decoder can
                // represent. Claiming it and truncating would be the silent version.
                "multilineTokenSupport": false,
                "overlappingTokenSupport": false,
                // The standard vocabulary. Servers are free to extend it — rust-analyzer
                // does, heavily — and the decoder handles names it does not know, so
                // declaring the standard set costs nothing and misses nothing.
                "tokenTypes": [
                    "namespace", "type", "class", "enum", "interface", "struct",
                    "typeParameter", "parameter", "variable", "property", "enumMember",
                    "event", "function", "method", "macro", "keyword", "modifier",
                    "comment", "string", "number", "regexp", "operator", "decorator",
                ],
                "tokenModifiers": [
                    "declaration", "definition", "readonly", "static", "deprecated",
                    "abstract", "async", "modification", "documentation", "defaultLibrary",
                ],
            },
        },
        "window": {
            "workDoneProgress": true,
            "showMessage": { "messageActionItem": { "additionalPropertiesSupport": false } },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_renders_a_title_with_its_message_and_percentage() {
        assert_eq!(render_progress("Indexing", &None, Some(43)), "Indexing 43%");
        assert_eq!(
            render_progress("Loading", &Some("bennu-lsp".into()), None),
            "Loading: bennu-lsp"
        );
        assert_eq!(render_progress("", &Some("only a message".into()), None), "only a message");
        assert_eq!(render_progress("Done", &Some("  ".into()), None), "Done", "blank message dropped");
    }

    /// The footer is one row. A progress line that does not fit stretches the strip and moves
    /// everything else on it — which is what a server putting an absolute path in its message does.
    #[test]
    fn a_progress_line_is_bounded() {
        let path = "/Users/christian/sviluppo/mio/workspace/rust/apps/arbor/crates/products/bennu/lsp/Cargo.toml";
        let line = render_progress("Loading", &Some(path.into()), Some(12));
        assert!(line.chars().count() <= MAX_PROGRESS_LINE, "{} chars: {line}", line.chars().count());
        // The tail is the informative half — which crate, not where it lives.
        assert!(line.contains("Cargo.toml"), "{line}");
        assert!(!line.contains("sviluppo"), "the path body is dropped: {line}");
        assert!(line.ends_with("12%"), "the percentage survives the cut: {line}");
    }

    #[test]
    fn a_long_title_alone_is_still_capped() {
        let line = render_progress(&"x".repeat(200), &None, None);
        assert!(line.chars().count() <= MAX_PROGRESS_LINE);
        assert!(line.ends_with('…'));
    }

    #[test]
    fn a_ratio_message_keeps_both_halves() {
        // rust-analyzer sends `3/456` for cache priming. Treating the `/` as a path separator
        // would throw away the numerator, which is the entire content.
        assert_eq!(
            render_progress("Priming caches", &Some("3/456".into()), Some(1)),
            "Priming caches: 3/456 1%"
        );
    }

    #[test]
    fn a_short_path_message_is_left_alone() {
        // Only a long one is worth reducing; `a/b` reads fine and the tail alone would lose half.
        assert_eq!(
            render_progress("Loading", &Some("crates/lsp".into()), None),
            "Loading: crates/lsp"
        );
    }

    #[test]
    fn ellipsize_never_splits_a_character() {
        let s = "città".repeat(20);
        let cut = ellipsize(&s, 10);
        assert_eq!(cut.chars().count(), 10);
        assert!(cut.ends_with('…'));
        // Round-trips as valid UTF-8 by construction — a byte-wise cut could not promise this.
        assert!(std::str::from_utf8(cut.as_bytes()).is_ok());
    }

    #[test]
    fn the_declared_capabilities_claim_what_bennu_implements() {
        let caps = client_capabilities();
        // utf-8 is offered first so a server that supports it removes a conversion.
        assert_eq!(caps["general"]["positionEncodings"][0], serde_json::json!("utf-8"));
        // linkSupport is what separates the name range from the whole declaration.
        assert_eq!(caps["textDocument"]["definition"]["linkSupport"], serde_json::json!(true));
        assert_eq!(
            caps["textDocument"]["signatureHelp"]["signatureInformation"]["parameterInformation"]
                ["labelOffsetSupport"],
            serde_json::json!(true)
        );
        assert_eq!(
            caps["textDocument"]["documentSymbol"]["hierarchicalDocumentSymbolSupport"],
            serde_json::json!(true)
        );
        // The deliberate omission. Claiming it would let a server answer a `mod` rename
        // with a file move Bennu would drop on the floor.
        assert!(
            caps["workspace"]["workspaceEdit"].get("resourceOperations").is_none(),
            "Bennu does not move files for a server"
        );
        // Diagnostic `data` must be claimed or a server may omit it — and without it a
        // quick fix for a specific error cannot be produced.
        assert_eq!(
            caps["textDocument"]["publishDiagnostics"]["dataSupport"],
            serde_json::json!(true)
        );
    }
}
