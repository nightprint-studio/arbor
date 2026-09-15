//! The editor features, as requests: completion, hover, go-to, find-usages, outline,
//! rename, format, quick fixes, signature help, semantic tokens.
//!
//! Every method here follows the same four steps, and each one exists for a reason worth
//! stating once rather than in twelve doc comments:
//!
//! 1. **Gate on the capability.** A request the server never advertised comes back as
//!    "method not found" — once per keystroke, if it is completion. The gate turns that
//!    into an honest [`LspError::Unsupported`] the caller can render as "this server does
//!    not do that".
//! 2. **Sync the document.** The editor holds the buffer and passes it in with the request.
//!    If the server's copy is behind, the offsets in the request describe text it does not
//!    have and it answers about the wrong span — silently.
//! 3. **Convert in, convert out.** Byte offset → position on the way in, position → byte
//!    offset on the way back, both through the same [`LineIndex`] built over the caller's
//!    text.
//! 4. **Bound the wait.** Each request carries its own timeout, sized by what the user is
//!    doing: a completion the user is typing into must fail fast, a find-usages sweep over a
//!    large workspace legitimately takes seconds.

use std::time::Duration;

use crate::client::LspError;
use crate::convert;
use crate::line_index::LineIndex;
use crate::model::{
    ActionEntry, CompletionEntry, DiagEntry, FileEdit, FoldSpan, HierarchyNode, HighlightSpan,
    HoverText, LensEntry, RenameOutcome, SignatureText, SpanTarget, SymbolNode, TokenSpan,
};
use crate::session::LspSession;
use crate::types::{self, capability_on, method};
use crate::uri;

// Timeouts, sized by what the user is doing while they wait.
//
// The short ones are short on purpose: a completion request that is still outstanding when
// the next keystroke arrives is worthless, so failing fast and asking again beats waiting.
// The long ones are long because the alternative is reporting "find usages doesn't work" on
// a workspace where it simply takes four seconds.
const T_COMPLETION: Duration = Duration::from_secs(4);
const T_RESOLVE: Duration = Duration::from_secs(3);
const T_HOVER: Duration = Duration::from_secs(3);
const T_SIGNATURE: Duration = Duration::from_secs(3);
const T_GOTO: Duration = Duration::from_secs(10);
const T_REFERENCES: Duration = Duration::from_secs(30);
const T_SYMBOLS: Duration = Duration::from_secs(10);
const T_WORKSPACE_SYMBOLS: Duration = Duration::from_secs(15);
const T_RENAME: Duration = Duration::from_secs(30);
const T_FORMAT: Duration = Duration::from_secs(15);
const T_CODE_ACTION: Duration = Duration::from_secs(8);
const T_SEMANTIC: Duration = Duration::from_secs(15);
const T_COMMAND: Duration = Duration::from_secs(30);
/// Short on purpose: highlights are painted while the caret rests somewhere, and a late answer is
/// worse than none — it decorates a position the caret has already left.
const T_HIGHLIGHT: Duration = Duration::from_secs(2);
const T_SELECTION: Duration = Duration::from_secs(3);
const T_FOLDING: Duration = Duration::from_secs(5);
const T_CODE_LENS: Duration = Duration::from_secs(10);
const T_HIERARCHY: Duration = Duration::from_secs(20);
const T_RELOAD: Duration = Duration::from_secs(60);
const T_EXPAND_MACRO: Duration = Duration::from_secs(10);

/// How many code lenses a file may have.
///
/// Each one may cost a `codeLens/resolve` round-trip, so this is a bound on requests rather than on
/// rendering. Far above any hand-written file; a generated one with thousands of items would
/// otherwise spend a minute filling in labels nobody scrolls to.
const MAX_LENSES: usize = 500;

/// How many completion candidates to hand back.
///
/// rust-analyzer will happily return two thousand items in a fresh `let x = ` context. The
/// editor filters as the user types, so everything past the first few hundred is
/// serialization cost for candidates nobody scrolls to — and the server's own `sortText`
/// ordering means the cap keeps the *relevant* ones.
const MAX_COMPLETIONS: usize = 400;

impl LspSession {
    // ── Completion ──────────────────────────────────────────────────────────────

    /// Completion candidates at `offset`.
    ///
    /// `trigger` is the character that fired it, when one did: the server offers a different
    /// list after `.` than after `::`, and telling it which is what makes path completion
    /// work at all.
    pub fn completion(
        &self,
        file: &str,
        offset: usize,
        source: &str,
        trigger: Option<&str>,
    ) -> Result<Vec<CompletionEntry>, LspError> {
        self.require(self.caps().completion_provider.is_some(), method::COMPLETION)?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);

        let context = Some(types::CompletionContext {
            trigger_kind: if trigger.is_some() {
                types::COMPLETION_TRIGGER_CHARACTER
            } else {
                types::COMPLETION_TRIGGER_INVOKED
            },
            trigger_character: trigger.map(str::to_string),
        });
        let response: types::CompletionResponse = self.client.request(
            method::COMPLETION,
            types::CompletionParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
                position: index.position_at(offset, self.encoding),
                context,
            },
            T_COMPLETION,
        )?;

        let mut items = response.into_items();
        items.truncate(MAX_COMPLETIONS);
        let entries: Vec<CompletionEntry> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                convert::completion_item(i, item.clone(), file, &index, self.encoding)
            })
            .collect();
        // Kept so the editor can ask for one item's documentation later, by index.
        *self.shared.last_completion.lock().unwrap_or_else(|p| p.into_inner()) =
            Some((file.to_string(), items));
        Ok(entries)
    }

    /// Fill in one candidate's documentation.
    ///
    /// Servers deliberately answer completion without docs — resolving four hundred items
    /// eagerly would be four hundred round-trips — and fill them in for the one item the
    /// user highlighted. `id` is the [`CompletionEntry::id`] from the answer that produced
    /// the list.
    ///
    /// `Ok(None)` when the list has been superseded, which is normal: the user kept typing.
    pub fn resolve_completion(&self, id: usize) -> Result<Option<CompletionEntry>, LspError> {
        let resolves = self
            .caps()
            .completion_provider
            .as_ref()
            .and_then(|c| c.resolve_provider)
            .unwrap_or(false);
        let (file, item) = {
            let guard = self.shared.last_completion.lock().unwrap_or_else(|p| p.into_inner());
            let Some((file, items)) = guard.as_ref() else { return Ok(None) };
            let Some(item) = items.get(id) else { return Ok(None) };
            (file.clone(), item.clone())
        };
        let Some(text) = self.text_of(&file) else { return Ok(None) };
        let index = LineIndex::new(&text);

        if !resolves {
            // Nothing more to learn; hand back what we already had rather than an error, so
            // the caller has one code path.
            return Ok(Some(convert::completion_item(id, item, &file, &index, self.encoding)));
        }
        let raw = serde_json::to_value(RawCompletionItem::from(&item))
            .map_err(|e| LspError::Transport(e.to_string()))?;
        let resolved: types::CompletionItem =
            self.client.request(method::COMPLETION_RESOLVE, raw, T_RESOLVE)?;
        Ok(Some(convert::completion_item(id, resolved, &file, &index, self.encoding)))
    }

    // ── Hover / signature help ──────────────────────────────────────────────────

    /// The hover card at `offset`.
    pub fn hover(
        &self,
        file: &str,
        offset: usize,
        source: &str,
    ) -> Result<Option<HoverText>, LspError> {
        self.require(capability_on(&self.caps().hover_provider), method::HOVER)?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<types::Hover> = self.client.request(
            method::HOVER,
            self.position_params(file, offset, &index),
            T_HOVER,
        )?;
        Ok(response.and_then(|h| convert::hover(h, &index, self.encoding)))
    }

    /// Signature help at `offset` — the parameter list of the call the caret is inside.
    pub fn signature_help(
        &self,
        file: &str,
        offset: usize,
        source: &str,
    ) -> Result<Option<SignatureText>, LspError> {
        self.require(self.caps().signature_help_provider.is_some(), method::SIGNATURE_HELP)?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<types::SignatureHelp> = self.client.request(
            method::SIGNATURE_HELP,
            types::SignatureHelpParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
                position: index.position_at(offset, self.encoding),
            },
            T_SIGNATURE,
        )?;
        Ok(response.and_then(convert::signature_help))
    }

    // ── Navigation ──────────────────────────────────────────────────────────────

    /// Go to definition.
    pub fn definition(
        &self,
        file: &str,
        offset: usize,
        source: &str,
    ) -> Result<Vec<SpanTarget>, LspError> {
        self.require(capability_on(&self.caps().definition_provider), method::DEFINITION)?;
        self.goto(method::DEFINITION, file, offset, source)
    }

    /// Go to the *type* of the expression under the caret — a distinct gesture from go-to
    /// definition on a `let` binding, and the one that answers "what is this thing".
    pub fn type_definition(
        &self,
        file: &str,
        offset: usize,
        source: &str,
    ) -> Result<Vec<SpanTarget>, LspError> {
        self.require(
            capability_on(&self.caps().type_definition_provider),
            method::TYPE_DEFINITION,
        )?;
        self.goto(method::TYPE_DEFINITION, file, offset, source)
    }

    /// Go to implementations — for a Rust trait method, every `impl` of it.
    pub fn implementation(
        &self,
        file: &str,
        offset: usize,
        source: &str,
    ) -> Result<Vec<SpanTarget>, LspError> {
        self.require(
            capability_on(&self.caps().implementation_provider),
            method::IMPLEMENTATION,
        )?;
        self.goto(method::IMPLEMENTATION, file, offset, source)
    }

    fn goto(
        &self,
        rpc: &'static str,
        file: &str,
        offset: usize,
        source: &str,
    ) -> Result<Vec<SpanTarget>, LspError> {
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: types::GotoResponse =
            self.client.request(rpc, self.position_params(file, offset, &index), T_GOTO)?;
        Ok(self.to_targets(response.targets(), file, source))
    }

    /// Find usages. `include_declaration` mirrors IntelliJ's behaviour of listing the
    /// declaration among its uses when the caret is elsewhere.
    pub fn references(
        &self,
        file: &str,
        offset: usize,
        source: &str,
        include_declaration: bool,
    ) -> Result<Vec<SpanTarget>, LspError> {
        self.require(capability_on(&self.caps().references_provider), method::REFERENCES)?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<Vec<types::Location>> = self.client.request(
            method::REFERENCES,
            types::ReferenceParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
                position: index.position_at(offset, self.encoding),
                context: types::ReferenceContext { include_declaration },
            },
            T_REFERENCES,
        )?;
        let raw: Vec<_> = response
            .unwrap_or_default()
            .into_iter()
            .map(|l| (l.uri, l.range, l.range))
            .collect();
        Ok(self.to_targets(raw, file, source))
    }

    // ── Structure ───────────────────────────────────────────────────────────────

    /// The document outline.
    pub fn document_symbols(
        &self,
        file: &str,
        source: &str,
    ) -> Result<Vec<SymbolNode>, LspError> {
        self.require(
            capability_on(&self.caps().document_symbol_provider),
            method::DOCUMENT_SYMBOL,
        )?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: types::DocumentSymbolResponse = self.client.request(
            method::DOCUMENT_SYMBOL,
            types::DocumentSymbolParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
            },
            T_SYMBOLS,
        )?;
        Ok(match response {
            types::DocumentSymbolResponse::Nested(syms) => {
                convert::symbol_tree(syms, file, &index, self.encoding, self.language())
            }
            types::DocumentSymbolResponse::Flat(syms) => {
                let resolve = |f: &str| self.text_of(f);
                convert::flat_symbols(syms, self.encoding, &resolve, self.language())
            }
            types::DocumentSymbolResponse::Null => Vec::new(),
        })
    }

    /// Search symbols across the workspace — "go to symbol everywhere".
    pub fn workspace_symbols(&self, query: &str) -> Result<Vec<SymbolNode>, LspError> {
        self.require(
            capability_on(&self.caps().workspace_symbol_provider),
            method::WORKSPACE_SYMBOL,
        )?;
        let response: types::WorkspaceSymbolResponse = self.client.request(
            method::WORKSPACE_SYMBOL,
            types::WorkspaceSymbolParams { query: query.to_string() },
            T_WORKSPACE_SYMBOLS,
        )?;
        let resolve = |f: &str| self.text_of(f);
        Ok(match response {
            types::WorkspaceSymbolResponse::Full(syms) => {
                convert::flat_symbols(syms, self.encoding, &resolve, self.language())
            }
            // The lazy shape carries a uri and possibly no range. Those that resolved to a
            // range are usable as-is; the rest would need a `workspaceSymbol/resolve`
            // round-trip per row, which for a type-ahead list is not worth a request storm —
            // they come back pointing at the top of their file.
            types::WorkspaceSymbolResponse::Lazy(syms) => {
                let flat: Vec<types::SymbolInformation> = syms
                    .into_iter()
                    .map(|s| types::SymbolInformation {
                        name: s.name,
                        kind: s.kind,
                        tags: Vec::new(),
                        deprecated: None,
                        location: types::Location {
                            uri: s.location.uri().to_string(),
                            range: s.location.range().unwrap_or_default(),
                        },
                        container_name: s.container_name,
                    })
                    .collect();
                convert::flat_symbols(flat, self.encoding, &resolve, self.language())
            }
            types::WorkspaceSymbolResponse::Null => Vec::new(),
        })
    }

    // ── Refactoring ─────────────────────────────────────────────────────────────

    /// Whether the symbol at `offset` can be renamed, and what to prefill the box with.
    ///
    /// `Ok(None)` means the server says no — which is information worth having *before*
    /// showing a rename dialog that will fail on submit.
    pub fn prepare_rename(
        &self,
        file: &str,
        offset: usize,
        source: &str,
    ) -> Result<Option<(usize, usize, String)>, LspError> {
        let supported = self
            .caps()
            .rename_provider
            .as_ref()
            .and_then(|r| r.options())
            .and_then(|o| o.prepare_provider)
            .unwrap_or(false);
        self.require(supported, method::PREPARE_RENAME)?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: types::PrepareRenameResponse = self.client.request(
            method::PREPARE_RENAME,
            self.position_params(file, offset, &index),
            T_RENAME,
        )?;
        Ok(match response {
            types::PrepareRenameResponse::WithPlaceholder { range, placeholder } => {
                let (s, e) = index.byte_range(range, self.encoding);
                Some((s, e, placeholder))
            }
            types::PrepareRenameResponse::Range(range) => {
                let (s, e) = index.byte_range(range, self.encoding);
                Some((s, e, source.get(s..e).unwrap_or_default().to_string()))
            }
            // "Rename it, I have no opinion about the span." The word under the caret is
            // then the caller's business, so an empty placeholder says so honestly.
            types::PrepareRenameResponse::DefaultBehavior { default_behavior } => {
                default_behavior.then(|| (offset, offset, String::new()))
            }
            types::PrepareRenameResponse::Null => None,
        })
    }

    /// Rename the symbol at `offset`.
    ///
    /// The returned [`RenameOutcome`] may carry `file_ops`. Bennu does not perform them (see
    /// the capability note in [`crate::session`]) — they are reported so the caller can tell
    /// the user rather than apply half a rename.
    pub fn rename(
        &self,
        file: &str,
        offset: usize,
        source: &str,
        new_name: &str,
    ) -> Result<RenameOutcome, LspError> {
        self.require(capability_on(&self.caps().rename_provider), method::RENAME)?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<types::WorkspaceEdit> = self.client.request(
            method::RENAME,
            types::RenameParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
                position: index.position_at(offset, self.encoding),
                new_name: new_name.to_string(),
            },
            T_RENAME,
        )?;
        let Some(edit) = response else {
            return Ok(RenameOutcome { edits: Vec::new(), file_ops: Vec::new() });
        };
        let resolve = |f: &str| self.text_of(f);
        let (edits, file_ops) = convert::workspace_edit(&edit, self.encoding, &resolve);
        Ok(RenameOutcome { edits, file_ops })
    }

    /// Format the whole file (`rustfmt`, for Rust).
    pub fn format(
        &self,
        file: &str,
        source: &str,
        tab_size: u32,
        insert_spaces: bool,
    ) -> Result<Vec<FileEdit>, LspError> {
        self.require(
            capability_on(&self.caps().document_formatting_provider),
            method::FORMATTING,
        )?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<Vec<types::TextEdit>> = self.client.request(
            method::FORMATTING,
            types::DocumentFormattingParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
                options: types::FormattingOptions { tab_size, insert_spaces },
            },
            T_FORMAT,
        )?;
        Ok(response
            .unwrap_or_default()
            .into_iter()
            .map(|e| {
                let (start, end) = index.byte_range(e.range, self.encoding);
                FileEdit { file: file.to_string(), start, end, new_text: e.new_text }
            })
            .collect())
    }

    /// Quick fixes and refactorings offered for the byte range `[start, end)`.
    ///
    /// The diagnostics overlapping that range are echoed back to the server, opaque `data`
    /// included. Skipping that is why "no quick fixes available" appears on an error that
    /// obviously has one: the server matches the fix to the diagnostic it published, and
    /// without the diagnostic there is nothing to match.
    pub fn code_actions(
        &self,
        file: &str,
        source: &str,
        start: usize,
        end: usize,
    ) -> Result<Vec<ActionEntry>, LspError> {
        self.require(capability_on(&self.caps().code_action_provider), method::CODE_ACTION)?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let range = types::Range::new(
            index.position_at(start, self.encoding),
            index.position_at(end.max(start), self.encoding),
        );

        let diagnostics: Vec<serde_json::Value> = {
            let all = self.shared.diagnostics.lock().unwrap_or_else(|p| p.into_inner());
            all.get(file)
                .map(|ds| {
                    ds.iter()
                        .filter(|d| ranges_overlap(d.range, range))
                        .map(convert::diagnostic_wire)
                        .collect()
                })
                .unwrap_or_default()
        };

        let response: Option<Vec<types::CodeActionOrCommand>> = self.client.request(
            method::CODE_ACTION,
            types::CodeActionParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
                range,
                context: types::CodeActionContext {
                    diagnostics,
                    trigger_kind: types::CODE_ACTION_TRIGGER_INVOKED,
                },
            },
            T_CODE_ACTION,
        )?;
        let resolve = |f: &str| self.text_of(f);
        Ok(response
            .unwrap_or_default()
            .into_iter()
            .map(|item| match item {
                types::CodeActionOrCommand::Action(a) => {
                    convert::code_action(a, self.encoding, &resolve)
                }
                types::CodeActionOrCommand::Command(c) => convert::command_action(c),
            })
            .collect())
    }

    /// Run a server command — how an action whose edit is computed lazily is applied. The
    /// resulting edits arrive as a `workspace/applyEdit` request, so the answer here is just
    /// whether the command ran.
    pub fn execute_command(
        &self,
        command: &str,
        arguments: Vec<serde_json::Value>,
    ) -> Result<(), LspError> {
        let _: serde_json::Value = self.client.request(
            method::EXECUTE_COMMAND,
            types::ExecuteCommandParams { command: command.to_string(), arguments },
            T_COMMAND,
        )?;
        Ok(())
    }

    // ── Highlighting ────────────────────────────────────────────────────────────

    /// Semantic tokens for the whole file — what makes a struct, a trait and a macro three
    /// different colours instead of one.
    pub fn semantic_tokens(
        &self,
        file: &str,
        source: &str,
    ) -> Result<Vec<TokenSpan>, LspError> {
        let Some(legend) = self.legend.as_ref() else {
            return Err(LspError::Unsupported(method::SEMANTIC_TOKENS_FULL));
        };
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<types::SemanticTokens> = self.client.request(
            method::SEMANTIC_TOKENS_FULL,
            types::SemanticTokensParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
            },
            T_SEMANTIC,
        )?;
        let Some(tokens) = response else { return Ok(Vec::new()) };
        Ok(crate::semantic::decode(&tokens.data, legend, &index, self.encoding))
    }

    // ── Diagnostics ─────────────────────────────────────────────────────────────

    /// The diagnostics the server last published for `file`.
    ///
    /// Converted here rather than on arrival: they land on the transport's reader thread,
    /// which must not block, and turning a range into a byte offset needs the file's text.
    /// Passing the live buffer in also makes the answer track what is on screen — a
    /// diagnostic computed against the saved file, mapped through the edited buffer, lands
    /// where the user is looking.
    pub fn diagnostics_for(&self, file: &str, source: Option<&str>) -> Vec<DiagEntry> {
        let raw = {
            let all = self.shared.diagnostics.lock().unwrap_or_else(|p| p.into_inner());
            match all.get(file) {
                Some(ds) => ds.clone(),
                None => return Vec::new(),
            }
        };
        let owned;
        let text: &str = match source {
            Some(s) => s,
            None => match self.text_of(file) {
                Some(t) => {
                    owned = t;
                    &owned
                }
                None => return Vec::new(),
            },
        };
        let index = LineIndex::new(text);
        let resolve = |f: &str| self.text_of(f);
        raw.iter().map(|d| convert::diagnostic(d, &index, self.encoding, &resolve)).collect()
    }

    /// Every file the server currently reports problems in — what a project-wide Problems
    /// panel needs, since a server publishes for files nobody has opened.
    pub fn diagnostic_files(&self) -> Vec<String> {
        let all = self.shared.diagnostics.lock().unwrap_or_else(|p| p.into_inner());
        let mut files: Vec<String> = all.keys().cloned().collect();
        files.sort();
        files
    }

    // ── Shared plumbing ─────────────────────────────────────────────────────────

    // ── document highlight ──────────────────────────────────────────────────

    /// Every occurrence of the symbol at `offset`, within this file.
    ///
    /// Not find-usages: that is a workspace question answered by `references` and worth a results
    /// panel. This is the one-file, no-panel version an editor paints while the caret sits
    /// somewhere — which is why its timeout is short and its failure is silence.
    pub fn document_highlights(
        &self,
        file: &str,
        source: &str,
        offset: usize,
    ) -> Result<Vec<HighlightSpan>, LspError> {
        self.require(
            capability_on(&self.caps().document_highlight_provider),
            method::DOCUMENT_HIGHLIGHT,
        )?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<Vec<types::DocumentHighlight>> = self.client.request(
            method::DOCUMENT_HIGHLIGHT,
            self.position_params(file, offset, &index),
            T_HIGHLIGHT,
        )?;
        Ok(response
            .unwrap_or_default()
            .into_iter()
            .filter_map(|h| {
                let (start, end) = index.byte_range(h.range, self.encoding);
                // A zero-width occurrence is a decoration that paints nothing.
                (end > start).then(|| HighlightSpan {
                    start,
                    end,
                    kind: match h.kind {
                        Some(types::HIGHLIGHT_WRITE) => "write".to_string(),
                        Some(types::HIGHLIGHT_READ) => "read".to_string(),
                        _ => "text".to_string(),
                    },
                })
            })
            .collect())
    }

    // ── selection range ─────────────────────────────────────────────────────

    /// The chain of syntactic ranges enclosing `offset`, innermost first.
    ///
    /// One request for the whole chain, which is what makes expand-selection feel instant: pressing
    /// it repeatedly walks a list the editor already has rather than asking again, and shrink walks
    /// back down it. Asking per keypress would put a round-trip between the key and the selection.
    pub fn selection_ranges(
        &self,
        file: &str,
        source: &str,
        offset: usize,
    ) -> Result<Vec<(usize, usize)>, LspError> {
        self.require(
            capability_on(&self.caps().selection_range_provider),
            method::SELECTION_RANGE,
        )?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<Vec<types::SelectionRange>> = self.client.request(
            method::SELECTION_RANGE,
            types::SelectionRangeParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
                positions: vec![index.position_at(offset, self.encoding)],
            },
            T_SELECTION,
        )?;
        let Some(first) = response.unwrap_or_default().into_iter().next() else {
            return Ok(Vec::new());
        };
        let mut out: Vec<(usize, usize)> = Vec::new();
        for range in first.flatten() {
            let span = index.byte_range(range, self.encoding);
            // Each link must be strictly larger than the last, or a press would appear to do
            // nothing. A server repeating a range (rust-analyzer does, between a token and the node
            // that only contains that token) is normal, not an error.
            if out.last().is_some_and(|prev| *prev == span) {
                continue;
            }
            out.push(span);
        }
        Ok(out)
    }

    // ── folding ─────────────────────────────────────────────────────────────

    /// The foldable regions of the file.
    ///
    /// Worth asking rather than folding on braces locally, because the server folds by *item*: a
    /// `use` block, a doc comment, a `#[cfg]`-gated module, a match arm. Brace matching gets the
    /// function bodies and nothing else.
    pub fn folding_ranges(&self, file: &str, source: &str) -> Result<Vec<FoldSpan>, LspError> {
        self.require(
            capability_on(&self.caps().folding_range_provider),
            method::FOLDING_RANGE,
        )?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<Vec<types::FoldingRange>> = self.client.request(
            method::FOLDING_RANGE,
            types::FoldingRangeParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
            },
            T_FOLDING,
        )?;
        Ok(response
            .unwrap_or_default()
            .into_iter()
            .filter_map(|f| {
                // A fold starts at the END of its first line, so the line that names the region
                // stays visible — that is what makes a folded function readable. The protocol's
                // optional `startCharacter` says otherwise when the server means it.
                let start = match f.start_character {
                    Some(ch) => index.offset_at(
                        types::Position { line: f.start_line, character: ch },
                        self.encoding,
                    ),
                    None => index.line_end_offset(f.start_line)?,
                };
                let end = match f.end_character {
                    Some(ch) => index.offset_at(
                        types::Position { line: f.end_line, character: ch },
                        self.encoding,
                    ),
                    None => index.line_end_offset(f.end_line)?,
                };
                (end > start).then(|| FoldSpan {
                    start,
                    end,
                    kind: f.kind.unwrap_or_default(),
                    placeholder: f.collapsed_text.unwrap_or_default(),
                })
            })
            .collect())
    }

    // ── code lens ───────────────────────────────────────────────────────────

    /// The lenses for a file, each resolved if it needs to be.
    ///
    /// The resolve pass is not optional in practice: rust-analyzer returns its reference and
    /// implementation counts as lenses with **no command and no title**, filling them in only when
    /// asked — so a client that skipped it would draw a column of blanks. Lenses that still have no
    /// title after resolving are dropped rather than drawn empty.
    ///
    /// Bounded by [`MAX_LENSES`]: a resolve is a round-trip each, and a generated file with
    /// thousands of items would otherwise spend a minute filling in labels nobody scrolls to.
    pub fn code_lenses(&self, file: &str, source: &str) -> Result<Vec<LensEntry>, LspError> {
        // Unlike its neighbours this capability is an options object, so its mere presence is the
        // answer — there is no `false` shape for it.
        self.require(self.caps().code_lens_provider.is_some(), method::CODE_LENS)?;
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<Vec<types::CodeLens>> = self.client.request(
            method::CODE_LENS,
            types::CodeLensParams {
                text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
            },
            T_CODE_LENS,
        )?;
        let resolves = self
            .caps()
            .code_lens_provider
            .as_ref()
            .and_then(|o| o.resolve_provider)
            .unwrap_or(false);

        let mut out = Vec::new();
        for lens in response.unwrap_or_default().into_iter().take(MAX_LENSES) {
            let lens = match (&lens.command, resolves) {
                // Already complete.
                (Some(_), _) => lens,
                // Needs resolving, and the server can. A failure here is not fatal to the request:
                // one lens that would not resolve should not cost the file its other lenses.
                (None, true) => match self.client.request::<_, types::CodeLens>(
                    method::CODE_LENS_RESOLVE,
                    &lens,
                    T_RESOLVE,
                ) {
                    Ok(resolved) => resolved,
                    Err(_) => continue,
                },
                (None, false) => continue,
            };
            let Some(command) = lens.command else { continue };
            if command.title.trim().is_empty() {
                continue;
            }
            let (start, _) = index.byte_range(lens.range, self.encoding);
            out.push(LensEntry {
                start,
                line: lens.range.start.line as usize + 1,
                title: command.title,
                // An empty command string means "a label, not a button" — the server used the
                // title to say something rather than to offer an action.
                command: (!command.command.is_empty()).then_some(command.command),
                arguments: command.arguments,
            });
        }
        Ok(out)
    }

    /// The locations a **client-side** lens command carries in its arguments.
    ///
    /// The counts a lens shows — "3 implementations", "12 references" — are commands the *client*
    /// is expected to handle: rust-analyzer sends `rust-analyzer.showReferences` with the whole
    /// location list as an argument, because it has already done the query to be able to count
    /// them. So pressing one is not another request, it is reading what is already in hand.
    ///
    /// `request_file` / `request_source` are the caller's live buffer, preferred over the file on
    /// disk for that one path — the lens was computed against the unsaved text, so its offsets only
    /// agree with that.
    pub fn command_locations(
        &self,
        arguments: &[serde_json::Value],
        request_file: &str,
        request_source: &str,
    ) -> Vec<SpanTarget> {
        let raw = locations_in(arguments)
            .into_iter()
            .map(|l| (l.uri, l.range, l.range))
            .collect::<Vec<_>>();
        self.to_targets(raw, request_file, request_source)
    }

    // ── call and type hierarchy ─────────────────────────────────────────────

    /// The item at `offset` a call hierarchy can be built from, or an empty list when there is none.
    pub fn prepare_call_hierarchy(
        &self,
        file: &str,
        source: &str,
        offset: usize,
    ) -> Result<Vec<HierarchyNode>, LspError> {
        self.require(
            capability_on(&self.caps().call_hierarchy_provider),
            method::PREPARE_CALL_HIERARCHY,
        )?;
        self.prepare_hierarchy(method::PREPARE_CALL_HIERARCHY, file, source, offset)
    }

    /// The item at `offset` a type hierarchy can be built from.
    pub fn prepare_type_hierarchy(
        &self,
        file: &str,
        source: &str,
        offset: usize,
    ) -> Result<Vec<HierarchyNode>, LspError> {
        self.require(
            capability_on(&self.caps().type_hierarchy_provider),
            method::PREPARE_TYPE_HIERARCHY,
        )?;
        self.prepare_hierarchy(method::PREPARE_TYPE_HIERARCHY, file, source, offset)
    }

    fn prepare_hierarchy(
        &self,
        method: &'static str,
        file: &str,
        source: &str,
        offset: usize,
    ) -> Result<Vec<HierarchyNode>, LspError> {
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<Vec<types::HierarchyItem>> =
            self.client.request(method, self.position_params(file, offset, &index), T_HIERARCHY)?;
        Ok(self.hierarchy_nodes(response.unwrap_or_default(), Vec::new(), file, source))
    }

    /// Who calls `item`.
    pub fn incoming_calls(&self, item: serde_json::Value) -> Result<Vec<HierarchyNode>, LspError> {
        let item: types::HierarchyItem = serde_json::from_value(item)
            .map_err(|e| LspError::Decode(format!("call-hierarchy item: {e}")))?;
        let response: Option<Vec<types::CallHierarchyIncomingCall>> = self.client.request(
            method::CALL_HIERARCHY_INCOMING,
            types::HierarchyItemParams { item },
            T_HIERARCHY,
        )?;
        Ok(self.calls_to_nodes(
            response.unwrap_or_default().into_iter().map(|c| (c.from, c.from_ranges)).collect(),
        ))
    }

    /// What `item` calls.
    pub fn outgoing_calls(&self, item: serde_json::Value) -> Result<Vec<HierarchyNode>, LspError> {
        let item: types::HierarchyItem = serde_json::from_value(item)
            .map_err(|e| LspError::Decode(format!("call-hierarchy item: {e}")))?;
        let response: Option<Vec<types::CallHierarchyOutgoingCall>> = self.client.request(
            method::CALL_HIERARCHY_OUTGOING,
            types::HierarchyItemParams { item },
            T_HIERARCHY,
        )?;
        Ok(self.calls_to_nodes(
            response.unwrap_or_default().into_iter().map(|c| (c.to, c.from_ranges)).collect(),
        ))
    }

    /// What `item` is built on — supertraits, or the traits a type implements.
    pub fn supertypes(&self, item: serde_json::Value) -> Result<Vec<HierarchyNode>, LspError> {
        self.hierarchy_step(method::TYPE_HIERARCHY_SUPERTYPES, item)
    }

    /// What is built on `item` — the implementors of a trait.
    pub fn subtypes(&self, item: serde_json::Value) -> Result<Vec<HierarchyNode>, LspError> {
        self.hierarchy_step(method::TYPE_HIERARCHY_SUBTYPES, item)
    }

    fn hierarchy_step(
        &self,
        method: &'static str,
        item: serde_json::Value,
    ) -> Result<Vec<HierarchyNode>, LspError> {
        let item: types::HierarchyItem = serde_json::from_value(item)
            .map_err(|e| LspError::Decode(format!("type-hierarchy item: {e}")))?;
        let response: Option<Vec<types::HierarchyItem>> =
            self.client.request(method, types::HierarchyItemParams { item }, T_HIERARCHY)?;
        Ok(self.hierarchy_nodes(response.unwrap_or_default(), Vec::new(), "", ""))
    }

    /// `(item, call sites in it)` pairs → nodes.
    fn calls_to_nodes(
        &self,
        pairs: Vec<(types::HierarchyItem, Vec<types::Range>)>,
    ) -> Vec<HierarchyNode> {
        pairs
            .into_iter()
            .filter_map(|(item, sites)| self.hierarchy_node(item, sites, "", ""))
            .collect()
    }

    fn hierarchy_nodes(
        &self,
        items: Vec<types::HierarchyItem>,
        sites: Vec<types::Range>,
        request_file: &str,
        request_source: &str,
    ) -> Vec<HierarchyNode> {
        items
            .into_iter()
            .filter_map(|i| {
                self.hierarchy_node(i, sites.clone(), request_file, request_source)
            })
            .collect()
    }

    /// One node. `None` when the item's file cannot be located at all, which would leave a row
    /// nothing can be done with.
    fn hierarchy_node(
        &self,
        item: types::HierarchyItem,
        sites: Vec<types::Range>,
        request_file: &str,
        request_source: &str,
    ) -> Option<HierarchyNode> {
        let file = uri::from_uri(&item.uri)?;
        // The handle is serialized BEFORE the item is consumed, because it is what the next level
        // is asked with — reconstructing it from the node's own fields would drop `data` and ask
        // the server about something it never offered.
        let handle = serde_json::to_value(&item).ok()?;
        // `(uri, whole, name)` — the order every caller of `to_targets` uses, and the name is the
        // one that survives: a row must land on the signature. An item's `range` legitimately
        // starts at its doc comment, so getting these round the wrong way shows the row's preview
        // as `/// Does the thing` and jumps two lines above the declaration.
        let targets = self.to_targets(
            vec![(file.clone(), item.range, item.selection_range)],
            request_file,
            request_source,
        );
        let target = targets.into_iter().next()?;
        let call_sites = self.to_targets(
            sites.into_iter().map(|r| (file.clone(), r, r)).collect(),
            request_file,
            request_source,
        );
        Some(HierarchyNode {
            name: item.name,
            kind: types::symbol_kind_name_for(item.kind, self.language()).to_string(),
            detail: item.detail,
            target,
            call_sites,
            handle,
        })
    }

    // ── workspace/willRenameFiles ───────────────────────────────────────────

    /// The edits a file rename implies — for Rust, the `mod` declaration that names it and every
    /// `use` path through it.
    ///
    /// Asked **before** the rename, which is what the method is for: the server answers about the
    /// tree as it stands, and applying the edits afterwards is the caller's job. A server that does
    /// not offer the capability answers nothing, and the rename is then a plain file move — worse,
    /// but not wrong.
    pub fn will_rename_files(
        &self,
        renames: &[(String, String)],
    ) -> Result<RenameOutcome, LspError> {
        let offered = self
            .caps()
            .workspace
            .as_ref()
            .and_then(|w| w.file_operations.as_ref())
            .and_then(|f| f.will_rename.as_ref())
            .is_some();
        self.require(offered, method::WILL_RENAME_FILES)?;
        let response: Option<types::WorkspaceEdit> = self.client.request(
            method::WILL_RENAME_FILES,
            types::RenameFilesParams {
                files: renames
                    .iter()
                    .map(|(old, new)| types::FileRename {
                        old_uri: uri::to_uri(old),
                        new_uri: uri::to_uri(new),
                    })
                    .collect(),
            },
            T_RENAME,
        )?;
        let Some(edit) = response else {
            return Ok(RenameOutcome::default());
        };
        let resolve = |f: &str| self.text_of(f);
        let (edits, file_ops) = convert::workspace_edit(&edit, self.encoding, &resolve);
        Ok(RenameOutcome { edits, file_ops })
    }

    // ── rust-analyzer extensions ────────────────────────────────────────────

    /// Re-read the project's manifests and rebuild the crate graph.
    ///
    /// What makes editing `Cargo.toml` take effect without restarting the server. Silently fine on
    /// a server that has no such method — the request errors, and there is nothing to reload.
    pub fn reload_workspace(&self) -> Result<(), LspError> {
        self.client
            .request::<_, serde_json::Value>(
                method::RA_RELOAD_WORKSPACE,
                serde_json::Value::Null,
                T_RELOAD,
            )
            .map(|_| ())
    }

    /// Expand the macro at `offset`.
    ///
    /// `None` when the caret is not in a macro call. The expansion is **text**, not a file the
    /// server knows — see [`types::MacroExpansion`] for what that costs.
    pub fn expand_macro(
        &self,
        file: &str,
        source: &str,
        offset: usize,
    ) -> Result<Option<(String, String)>, LspError> {
        self.sync(file, source)?;
        let index = LineIndex::new(source);
        let response: Option<types::MacroExpansion> = self.client.request(
            method::RA_EXPAND_MACRO,
            self.position_params(file, offset, &index),
            T_EXPAND_MACRO,
        )?;
        Ok(response.map(|e| (e.name, e.expansion)))
    }

    fn position_params(
        &self,
        file: &str,
        offset: usize,
        index: &LineIndex<'_>,
    ) -> types::TextDocumentPositionParams {
        types::TextDocumentPositionParams {
            text_document: types::TextDocumentIdentifier::new(uri::to_uri(file)),
            position: index.position_at(offset, self.encoding),
        }
    }

    /// Convert targets, preferring the caller's live buffer for the requested file.
    ///
    /// The preference matters: the request was made against unsaved text, so for that one
    /// file the buffer is the only text whose offsets agree with the answer.
    fn to_targets(
        &self,
        raw: Vec<(String, types::Range, types::Range)>,
        request_file: &str,
        request_source: &str,
    ) -> Vec<SpanTarget> {
        let resolve = |f: &str| {
            if f == request_file {
                Some(request_source.to_string())
            } else {
                self.text_of(f)
            }
        };
        convert::targets(raw, self.encoding, &resolve)
    }
}

/// The locations buried in a command's argument list.
///
/// Found by **shape** rather than by position, because the position is per-command: a
/// `showReferences` lens sends `[uri, position, locations]` and a `gotoLocation` one sends a single
/// location. Both are answered by "the first argument that is a location, or a list of them", and
/// that rule needs no table of command names to keep up to date.
///
/// A `Position` argument cannot be mistaken for a location (it has no `uri`), and neither can the
/// bare `uri` string, so scanning is safe.
fn locations_in(arguments: &[serde_json::Value]) -> Vec<types::Location> {
    for arg in arguments {
        if let Ok(list) = serde_json::from_value::<Vec<types::Location>>(arg.clone()) {
            if !list.is_empty() {
                return list;
            }
            // An empty array is a legitimate answer — a lens saying "0 references" — but it is also
            // what an unrelated empty array decodes to, so keep looking before believing it.
            continue;
        }
        if let Ok(one) = serde_json::from_value::<types::Location>(arg.clone()) {
            return vec![one];
        }
    }
    Vec::new()
}

/// Whether two ranges touch. Used to pick the diagnostics relevant to a code-action request.
fn ranges_overlap(a: types::Range, b: types::Range) -> bool {
    let before = |p: crate::line_index::Position, q: crate::line_index::Position| {
        (p.line, p.character) < (q.line, q.character)
    };
    // Not disjoint: `a` does not end before `b` starts, and `b` does not end before `a` does.
    !before(a.end, b.start) && !before(b.end, a.start)
}

/// A completion item re-encoded for `completionItem/resolve`.
///
/// The server matches its own item by the fields it set — `data` above all, which is opaque
/// and mandatory to echo. Sending a re-serialized subset rather than the decoded struct keeps
/// this honest about what actually has to survive the round trip.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RawCompletionItem<'a> {
    label: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort_text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter_text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    insert_text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    insert_text_format: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<&'a serde_json::Value>,
}

impl<'a> From<&'a types::CompletionItem> for RawCompletionItem<'a> {
    fn from(i: &'a types::CompletionItem) -> Self {
        Self {
            label: &i.label,
            kind: i.kind,
            detail: i.detail.as_deref(),
            sort_text: i.sort_text.as_deref(),
            filter_text: i.filter_text.as_deref(),
            insert_text: i.insert_text.as_deref(),
            insert_text_format: i.insert_text_format,
            data: i.data.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line_index::Position;

    fn range(l1: u32, c1: u32, l2: u32, c2: u32) -> types::Range {
        types::Range::new(Position::new(l1, c1), Position::new(l2, c2))
    }

    #[test]
    fn overlap_is_inclusive_at_the_boundaries() {
        // A caret sitting exactly at the end of a squiggle must still see its quick fix —
        // which is the common case, since a diagnostic ends where the offending token does.
        assert!(ranges_overlap(range(0, 4, 0, 9), range(0, 9, 0, 9)));
        assert!(ranges_overlap(range(0, 4, 0, 9), range(0, 4, 0, 4)));
        assert!(ranges_overlap(range(0, 4, 0, 9), range(0, 6, 0, 7)));
        assert!(ranges_overlap(range(1, 0, 3, 0), range(2, 5, 2, 5)), "multi-line");
    }

    #[test]
    fn disjoint_ranges_do_not_overlap() {
        assert!(!ranges_overlap(range(0, 0, 0, 3), range(0, 4, 0, 5)));
        assert!(!ranges_overlap(range(2, 0, 2, 3), range(0, 0, 1, 9)), "an earlier line");
    }

    #[test]
    fn a_resolve_request_echoes_the_opaque_data() {
        // Without `data` the server cannot recognise its own item, and the documentation
        // comes back empty for every candidate.
        let item: types::CompletionItem = serde_json::from_str(
            r#"{"label":"push","kind":2,"data":{"position":{"line":1},"imports":[]}}"#,
        )
        .unwrap();
        let wire = serde_json::to_value(RawCompletionItem::from(&item)).unwrap();
        assert_eq!(wire["label"], serde_json::json!("push"));
        assert_eq!(wire["data"]["position"]["line"], serde_json::json!(1));
        // Absent fields stay absent rather than becoming nulls a picky server may reject.
        assert!(wire.get("detail").is_none(), "{wire}");
        assert!(wire.get("sortText").is_none());
    }

    #[test]
    fn a_show_references_lens_gives_up_its_locations() {
        // The real argument list of rust-analyzer's implementations / references lens:
        // `[uri, position, locations]`. The first two must not be mistaken for the third.
        let args: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
                "file:///p/src/lib.rs",
                {"line": 10, "character": 4},
                [
                  {"uri":"file:///p/src/a.rs","range":{"start":{"line":1,"character":0},"end":{"line":1,"character":5}}},
                  {"uri":"file:///p/src/b.rs","range":{"start":{"line":7,"character":2},"end":{"line":7,"character":9}}}
                ]
            ]"#,
        )
        .unwrap();
        let found = locations_in(&args);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].uri, "file:///p/src/a.rs");
        assert_eq!(found[1].range.start.line, 7);
    }

    #[test]
    fn a_single_location_argument_counts_as_one() {
        let args: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"uri":"file:///p/src/a.rs","range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}}}]"#,
        )
        .unwrap();
        assert_eq!(locations_in(&args).len(), 1);
    }

    #[test]
    fn a_lens_with_nothing_to_show_yields_nothing() {
        // A runnable's argument is a whole object of its own, and an empty array earlier in the list
        // must not stop the scan — otherwise "0 references" and "here is a runnable" would both
        // read as an answer.
        let args: Vec<serde_json::Value> = serde_json::from_str(
            r#"[[], {"label":"test x","kind":"cargo","args":{"cargoArgs":["test"]}}]"#,
        )
        .unwrap();
        assert!(locations_in(&args).is_empty());
        assert!(locations_in(&[]).is_empty());
    }

    #[test]
    fn the_completion_cap_is_generous_enough_to_be_invisible() {
        // A guard on the number, not on the idea: the cap exists so a 2000-item answer in a
        // fresh `let x = ` context is not serialized in full, and the server's own sortText
        // ordering means the cut keeps the relevant candidates.
        assert!(MAX_COMPLETIONS >= 200, "a real completion list must not be truncated visibly");
    }
}
