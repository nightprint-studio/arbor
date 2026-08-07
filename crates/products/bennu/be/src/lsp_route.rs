//! Per-language routing: the language-server answer to a shared handler, or nothing.
//!
//! This module is what makes "one protocol for every language" true in practice. The FE calls
//! `bennu_completion` / `bennu_declaration` / `bennu_references` / `bennu_hover` /
//! `bennu_diagnostics` exactly as it always has; each of those handlers asks here first, and
//! falls through to the native Java engine when the answer is `None`.
//!
//! ## The one distinction that matters
//!
//! `None` means **"not ours"** — no server serves this extension, or there is no workspace root
//! above the file — so the caller should carry on to the Java path.
//!
//! `Some(empty)` means **"ours, and the answer is nothing yet"** — the server is still starting,
//! failed, or genuinely has nothing to say. The caller must *stop* there.
//!
//! Collapsing those two would route a `.rs` buffer into the Java resolver during
//! rust-analyzer's startup, and that resolver would answer: it parses anything as Java, so a
//! `fn` becomes a mangled tree and an identifier match somewhere in the index becomes a
//! confident go-to into the wrong file. An empty answer is correct; a Java answer about Rust is
//! not.

use bennu_lsp::prelude::{LspError, LspSession, SpanTarget};
use bennu_proto::prelude::{
    CompletionItem, DeclarationTarget, Diagnostic, HoverInfo, LspAction, LspSignature, LspSymbol,
    LspToken, RenameEdit, RenameFileEdits, RenamePreview, SourceEdit, UsageHit, UsagesResult,
};
use bennu_proto::prelude::{
    LspCallSite, LspFold, LspHierarchyNode, LspHighlight, LspLens, LspMacroExpansion, SnippetStop,
};

use crate::lsp_registry::LspRegistry;

/// Whether a language server is the engine for `file`.
pub fn owns(file: &str) -> bool {
    LspRegistry::global().is_lsp_file(file)
}

/// Run `f` against the session serving `file`.
///
/// The single gate every routed handler goes through — see the module docs for why the two
/// `None`-shaped outcomes are not the same thing.
fn route<T>(file: &str, empty: T, f: impl FnOnce(&LspSession) -> T) -> Option<T> {
    let registry = LspRegistry::global();
    if !registry.is_lsp_file(file) {
        return None;
    }
    match registry.ensure(file) {
        Some(session) => Some(f(&session)),
        // Ours, but there is nothing to ask yet. Never falls through.
        None => Some(empty),
    }
}

/// Unwrap a server answer, turning every failure into the empty one.
///
/// A transient failure — the request raced an edit, or arrived while the workspace was still
/// loading — is silent by design: the editor will ask again on the next keystroke, and a toast
/// per cancelled request would teach the user that a working feature is unreliable. A real
/// failure is logged once to stderr, where the server's own output already is.
fn tolerate<T: Default>(result: Result<T, LspError>, what: &str) -> T {
    match result {
        Ok(value) => value,
        Err(e) if e.is_transient() => T::default(),
        Err(e) => {
            eprintln!("[lsp] {what}: {e}");
            T::default()
        }
    }
}

// ---------------------------------------------------------------------------
// The routed handlers
// ---------------------------------------------------------------------------

/// Completion at `offset`, or `None` when this file is not a language server's.
pub fn completion(file: &str, offset: usize, source: &str) -> Option<Vec<CompletionItem>> {
    route(file, Vec::new(), |session| {
        // The trigger character, taken from the buffer rather than passed down from the
        // editor: the server offers a different list after `.` than after `::`, and the char
        // immediately before the identifier prefix is exactly what it wants to know.
        let trigger = trigger_char(source, offset, &session.completion_trigger_characters());
        let items = tolerate(
            session.completion(file, offset, source, trigger.as_deref()),
            "completion",
        );
        items.into_iter().map(completion_wire).collect()
    })
}

/// Go-to-declaration: the first target the server resolved.
pub fn declaration(file: &str, source: &str, offset: usize) -> Option<Option<DeclarationTarget>> {
    route(file, None, |session| {
        let targets = tolerate(session.definition(file, offset, source), "definition");
        targets.into_iter().next().map(|t| declaration_wire(t, "definition"))
    })
}

/// Go-to-type-definition.
pub fn type_definition(file: &str, source: &str, offset: usize) -> Option<Option<DeclarationTarget>> {
    route(file, None, |session| {
        let targets = tolerate(session.type_definition(file, offset, source), "typeDefinition");
        targets.into_iter().next().map(|t| declaration_wire(t, "type"))
    })
}

/// Go-to-implementations — every `impl` of a trait method, as a usages list (there is rarely
/// one, and picking one arbitrarily would hide the rest).
pub fn implementations(file: &str, source: &str, offset: usize) -> Option<UsagesResult> {
    route(file, UsagesResult { target_label: String::new(), usages: Vec::new() }, |session| {
        let targets = tolerate(session.implementation(file, offset, source), "implementation");
        UsagesResult {
            target_label: "implementations".to_string(),
            usages: targets.into_iter().map(usage_wire).collect(),
        }
    })
}

/// Find usages.
pub fn references(file: &str, source: &str, offset: usize) -> Option<Option<UsagesResult>> {
    route(file, None, |session| {
        // `include_declaration` matches the native engine's behaviour: the declaration is one
        // of the places the symbol appears, and IntelliJ lists it.
        let hits = tolerate(session.references(file, offset, source, true), "references");
        if hits.is_empty() {
            return None;
        }
        Some(UsagesResult {
            target_label: word_at(source, offset)
                .map(|w| format!("`{w}`"))
                .unwrap_or_else(|| "symbol".to_string()),
            usages: hits.into_iter().map(usage_wire).collect(),
        })
    })
}

/// The hover card.
pub fn hover(file: &str, source: &str, offset: usize) -> Option<Option<HoverInfo>> {
    route(file, None, |session| {
        let card = tolerate(session.hover(file, offset, source), "hover");
        card.map(|h| hover_wire(&h.markdown))
    })
}

/// Diagnostics for `file`, from the server's last publish.
pub fn diagnostics(file: &str, source: Option<&str>) -> Option<Vec<Diagnostic>> {
    route(file, Vec::new(), |session| {
        session
            .diagnostics_for(file, source)
            .into_iter()
            .map(|d| Diagnostic {
                message: match d.source.is_empty() {
                    // "clippy says" and "the compiler says" are different weights of advice,
                    // and the shared Diagnostic has nowhere else to put it.
                    false => format!("{}: {}", d.source, d.message),
                    true => d.message,
                },
                severity: d.severity,
                code: d.code,
                start: d.start,
                end: d.end,
            })
            .collect()
    })
}

/// An editor change: keep the server's copy in step.
///
/// Returns `Some(true)` when this file is a server's, so the caller knows not to also patch
/// the Java index for it.
pub fn did_change(file: &str, text: Option<&str>) -> Option<bool> {
    route(file, true, |session| {
        match text {
            Some(t) => {
                let _ = session.sync(file, t);
            }
            // A deleted file: closing it is the honest signal, and it makes the server drop
            // its diagnostics for a file that no longer exists.
            None => session.did_close(file),
        }
        true
    })
}

/// Plan a rename — the preview the FE renders before the user confirms.
pub fn rename_plan(
    file: &str,
    source: &str,
    offset: usize,
    new_name: &str,
) -> Option<Option<RenamePreview>> {
    route(file, None, |session| {
        let old_name = word_at(source, offset).unwrap_or_default();
        let outcome = match session.rename(file, offset, source, new_name) {
            Ok(o) => o,
            Err(e) if e.is_transient() => return None,
            Err(e) => {
                // A refused rename is the *useful* case to surface rather than swallow: a Rust
                // `mod` rename needs a file move, which Bennu does not do, and the server says
                // so in words worth showing.
                eprintln!("[lsp] rename: {e}");
                return None;
            }
        };
        if outcome.edits.is_empty() && outcome.file_ops.is_empty() {
            return None;
        }
        Some(rename_preview(outcome, &old_name, new_name))
    })
}

/// The flattened edits for a rename the user confirmed.
pub fn rename_apply(
    file: &str,
    source: &str,
    offset: usize,
    new_name: &str,
) -> Option<Vec<RenameEdit>> {
    route(file, Vec::new(), |session| {
        let old_name = word_at(source, offset).unwrap_or_default();
        let outcome = tolerate(session.rename(file, offset, source, new_name), "rename");
        outcome
            .edits
            .into_iter()
            .map(|e| rename_edit(e, &old_name))
            .collect()
    })
}

// ---------------------------------------------------------------------------
// The LSP-only handlers
// ---------------------------------------------------------------------------

/// Semantic tokens for the whole file.
pub fn semantic_tokens(file: &str, source: &str) -> Vec<LspToken> {
    route(file, Vec::new(), |session| {
        tolerate(session.semantic_tokens(file, source), "semanticTokens")
            .into_iter()
            .map(|t| LspToken {
                start: t.start,
                end: t.end,
                class: t.class,
                modifiers: t.modifiers,
            })
            .collect()
    })
    .unwrap_or_default()
}

/// The document outline.
pub fn document_symbols(file: &str, source: &str) -> Vec<LspSymbol> {
    route(file, Vec::new(), |session| {
        tolerate(session.document_symbols(file, source), "documentSymbol")
            .into_iter()
            .map(symbol_wire)
            .collect()
    })
    .unwrap_or_default()
}

/// Every occurrence of the symbol at `offset`, in this file.
pub fn document_highlights(file: &str, source: &str, offset: usize) -> Vec<LspHighlight> {
    route(file, Vec::new(), |session| {
        tolerate(session.document_highlights(file, source, offset), "documentHighlight")
            .into_iter()
            .map(|h| LspHighlight { start: h.start, end: h.end, kind: h.kind })
            .collect()
    })
    .unwrap_or_default()
}

/// The chain of syntactic ranges enclosing `offset`, innermost first.
///
/// The whole chain in one answer, so the editor walks a list on each keypress instead of asking
/// again — see `LspSession::selection_ranges`.
pub fn selection_ranges(file: &str, source: &str, offset: usize) -> Vec<[usize; 2]> {
    route(file, Vec::new(), |session| {
        tolerate(session.selection_ranges(file, source, offset), "selectionRange")
            .into_iter()
            .map(|(start, end)| [start, end])
            .collect()
    })
    .unwrap_or_default()
}

/// The foldable regions of the file.
pub fn folding_ranges(file: &str, source: &str) -> Vec<LspFold> {
    route(file, Vec::new(), |session| {
        tolerate(session.folding_ranges(file, source), "foldingRange")
            .into_iter()
            .map(|f| LspFold {
                start: f.start,
                end: f.end,
                kind: f.kind,
                placeholder: f.placeholder,
            })
            .collect()
    })
    .unwrap_or_default()
}

/// The code lenses for the file, resolved.
pub fn code_lenses(file: &str, source: &str) -> Vec<LspLens> {
    route(file, Vec::new(), |session| {
        tolerate(session.code_lenses(file, source), "codeLens")
            .into_iter()
            .map(|l| LspLens {
                start: l.start,
                line: l.line,
                title: l.title,
                command: l.command,
                arguments: l.arguments,
            })
            .collect()
    })
    .unwrap_or_default()
}

/// The locations a lens command carries — what pressing "3 implementations" shows.
///
/// `None` when the command names something else (a runnable), which the caller reads as "there is
/// nothing to list here" and falls back to executing the command.
pub fn lens_locations(
    file: &str,
    source: &str,
    label: &str,
    arguments: &[serde_json::Value],
) -> Option<UsagesResult> {
    route(file, None, |session| {
        let hits = session.command_locations(arguments, file, source);
        if hits.is_empty() {
            return None;
        }
        Some(UsagesResult {
            target_label: label.to_string(),
            usages: hits.into_iter().map(usage_wire).collect(),
        })
    })
    .flatten()
}

/// The item at `offset` a hierarchy can be built from. `calls` picks which hierarchy.
pub fn prepare_hierarchy(
    file: &str,
    source: &str,
    offset: usize,
    calls: bool,
) -> Vec<LspHierarchyNode> {
    route(file, Vec::new(), |session| {
        let result = if calls {
            session.prepare_call_hierarchy(file, source, offset)
        } else {
            session.prepare_type_hierarchy(file, source, offset)
        };
        tolerate(result, "prepareHierarchy").into_iter().map(hierarchy_wire).collect()
    })
    .unwrap_or_default()
}

/// One level of a hierarchy, from a node's own handle.
///
/// Keyed by `scope` — any path inside the workspace — rather than by the item's file, because a
/// caller can perfectly well live in a dependency's source, which is not a workspace of its own
/// (see `LspRegistry::root_of`). The handle already identifies the item; all this needs is the
/// session that issued it.
pub fn hierarchy_step(
    scope: &str,
    item: serde_json::Value,
    direction: &str,
) -> Vec<LspHierarchyNode> {
    let Some(session) = LspRegistry::global().session_covering(scope) else { return Vec::new() };
    let result = match direction {
        "incoming" => session.incoming_calls(item),
        "outgoing" => session.outgoing_calls(item),
        "supertypes" => session.supertypes(item),
        "subtypes" => session.subtypes(item),
        // A direction this build does not know — from a newer frontend. Answering nothing is right;
        // guessing which of the four was meant would put the wrong list under an expanded node.
        _ => return Vec::new(),
    };
    tolerate(result, "hierarchyStep").into_iter().map(hierarchy_wire).collect()
}

/// The edits a file rename implies. Asked before the rename, and empty when the server has nothing
/// to say — which is also what a server without the capability gives.
pub fn will_rename(file: &str, new_path: &str) -> Vec<SourceEdit> {
    route(file, Vec::new(), |session| {
        let renames = [(file.to_string(), new_path.to_string())];
        tolerate(session.will_rename_files(&renames), "willRenameFiles")
            .edits
            .into_iter()
            .map(source_edit)
            .collect()
    })
    .unwrap_or_default()
}

/// Re-read the workspace's manifests. `false` when no server covers `scope`, or it refused.
pub fn reload_workspace(scope: &str) -> bool {
    let Some(session) = LspRegistry::global().session_covering(scope) else { return false };
    match session.reload_workspace() {
        Ok(()) => true,
        Err(e) => {
            // Worth a line: a server with no such method is a normal state, but a reload the user
            // asked for and did not get should be explicable from the log.
            eprintln!("[lsp] reloadWorkspace: {e}");
            false
        }
    }
}

/// Expand the macro at `offset`. `None` when the caret is not in a macro call.
pub fn expand_macro(file: &str, source: &str, offset: usize) -> Option<LspMacroExpansion> {
    route(file, None, |session| {
        tolerate(session.expand_macro(file, source, offset), "expandMacro")
            .map(|(name, expansion)| LspMacroExpansion { name, expansion })
    })
    .flatten()
}

/// A hierarchy node on the wire.
fn hierarchy_wire(n: bennu_lsp::prelude::HierarchyNode) -> LspHierarchyNode {
    LspHierarchyNode {
        name: n.name,
        kind: n.kind,
        detail: n.detail,
        file: n.target.file,
        start: n.target.start,
        end: n.target.end,
        line: n.target.line,
        col: n.target.col,
        preview: n.target.preview,
        call_sites: n
            .call_sites
            .into_iter()
            .map(|s| LspCallSite {
                file: s.file,
                start: s.start,
                end: s.end,
                line: s.line,
                preview: s.preview,
            })
            .collect(),
        handle: n.handle,
    }
}

/// Workspace-wide symbol search.
///
/// `scope` is any path inside the workspace — a source file, or **the project root**, which is the
/// natural way to ask a question about the whole workspace. Resolved by containment rather than by
/// extension for exactly that reason (see [`LspRegistry::session_covering`]); the extension-keyed
/// lookup the per-buffer requests use would refuse a directory.
pub fn workspace_symbols(scope: &str, query: &str) -> Vec<LspSymbol> {
    let Some(session) = LspRegistry::global().session_covering(scope) else { return Vec::new() };
    tolerate(session.workspace_symbols(query), "workspaceSymbol")
        .into_iter()
        .map(symbol_wire)
        .collect()
}

/// Format the file.
pub fn format(file: &str, source: &str, tab_size: u32, insert_spaces: bool) -> Vec<SourceEdit> {
    route(file, Vec::new(), |session| {
        tolerate(session.format(file, source, tab_size, insert_spaces), "formatting")
            .into_iter()
            .map(source_edit)
            .collect()
    })
    .unwrap_or_default()
}

/// Quick fixes and refactorings for `[start, end)`.
pub fn code_actions(file: &str, source: &str, start: usize, end: usize) -> Vec<LspAction> {
    route(file, Vec::new(), |session| {
        tolerate(session.code_actions(file, source, start, end), "codeAction")
            .into_iter()
            .map(|a| LspAction {
                title: a.title,
                kind: a.kind,
                preferred: a.preferred,
                disabled: a.disabled,
                edits: a.edits.into_iter().map(source_edit).collect(),
                file_ops: a.file_ops.iter().map(bennu_lsp::prelude::FileOp::describe).collect(),
                command: a.command.as_ref().map(|(c, _)| c.clone()),
                arguments: a.command.map(|(_, args)| args).unwrap_or_default(),
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Run a server command (how an action whose edit is computed lazily is applied).
pub fn execute_command(file: &str, command: &str, arguments: Vec<serde_json::Value>) -> bool {
    route(file, false, |session| {
        match session.execute_command(command, arguments) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("[lsp] executeCommand {command}: {e}");
                false
            }
        }
    })
    .unwrap_or(false)
}

/// Signature help at `offset`.
pub fn signature_help(file: &str, source: &str, offset: usize) -> Option<LspSignature> {
    route(file, None, |session| {
        tolerate(session.signature_help(file, offset, source), "signatureHelp").map(|s| {
            LspSignature {
                label: s.label,
                doc: s.doc,
                params: s.params,
                active_param: s.active_param,
                active_start: s.active_param_range.map(|(a, _)| a),
                active_end: s.active_param_range.map(|(_, b)| b),
            }
        })
    })
    .flatten()
}

/// Fill in one completion candidate's documentation.
pub fn resolve_completion(file: &str, id: usize) -> Option<CompletionItem> {
    route(file, None, |session| {
        tolerate(session.resolve_completion(id), "completionItem/resolve").map(completion_wire)
    })
    .flatten()
}

/// A save: for rust-analyzer this is what triggers `cargo check`, and therefore what produces
/// real type and borrow errors rather than only what the parser can see.
pub fn did_save(file: &str, source: &str) -> bool {
    route(file, false, |session| {
        session.did_save(file, source);
        true
    })
    .unwrap_or(false)
}

/// A closed tab.
pub fn did_close(file: &str) -> bool {
    // Deliberately does NOT start a server: closing a tab in a project whose server never ran
    // must not spawn one.
    let Some(session) = LspRegistry::global().session_for(file) else { return false };
    session.did_close(file);
    true
}

/// Every file a server currently reports problems in, with their diagnostics — the
/// project-wide Problems panel.
///
/// A server publishes for files nobody has opened (that is the whole value of `cargo check`),
/// so this cannot be assembled from the open buffers.
pub fn problems(scope: &str) -> Vec<bennu_proto::prelude::FileDiagnostics> {
    // By containment, so the project root is a valid scope — which is how a project-wide panel
    // asks the question.
    let Some(session) = LspRegistry::global().session_covering(scope) else { return Vec::new() };
    session
        .diagnostic_files()
        .into_iter()
        .map(|f| {
            let diags = session
                .diagnostics_for(&f, None)
                .into_iter()
                .map(|d| Diagnostic {
                    message: if d.source.is_empty() {
                        d.message
                    } else {
                        format!("{}: {}", d.source, d.message)
                    },
                    severity: d.severity,
                    code: d.code,
                    start: d.start,
                    end: d.end,
                })
                .collect();
            bennu_proto::prelude::FileDiagnostics { file: f, diagnostics: diags }
        })
        .filter(|fd| !fd.diagnostics.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Mapping to the wire
// ---------------------------------------------------------------------------

fn completion_wire(e: bennu_lsp::prelude::CompletionEntry) -> CompletionItem {
    CompletionItem {
        label: e.label,
        kind: e.kind,
        detail: e.detail,
        // `auto_import` is the native engine's mechanism (a simple name whose FQN Bennu
        // resolves itself). A language server delivers the same effect as an extra edit, so
        // this stays empty and `edits` carries the `use` line.
        auto_import: None,
        insert_text: Some(e.insert_text),
        replace_start: e.replace.map(|(s, _)| s),
        replace_end: e.replace.map(|(_, x)| x),
        snippet: e.is_snippet,
        snippet_stops: e
            .snippet_stops
            .into_iter()
            .map(|t| SnippetStop { start: t.start, end: t.end })
            .collect(),
        sort_text: e.sort_text,
        filter_text: e.filter_text,
        doc: e.doc,
        edits: e.additional_edits.into_iter().map(source_edit).collect(),
        deprecated: e.deprecated,
        preselect: e.preselect,
        resolve_id: Some(e.id),
    }
}

fn source_edit(e: bennu_lsp::prelude::FileEdit) -> SourceEdit {
    SourceEdit { file: e.file, start: e.start, end: e.end, new_text: e.new_text }
}

fn usage_wire(t: SpanTarget) -> UsageHit {
    UsageHit {
        file: t.file,
        start: t.start,
        end: t.end,
        line: t.line,
        col: t.col,
        preview: t.preview,
    }
}

fn declaration_wire(t: SpanTarget, what: &str) -> DeclarationTarget {
    DeclarationTarget {
        label: if t.preview.is_empty() {
            what.to_string()
        } else {
            // The declaration's own source line is the most informative label available: a
            // server does not send a "kind", and `fn insert(&mut self, k: K)` says more than
            // "definition" ever could.
            t.preview.clone()
        },
        file: t.file,
        start: t.start,
        end: t.end,
        line: t.line as u32,
        col: t.col as u32,
    }
}

fn symbol_wire(s: bennu_lsp::prelude::SymbolNode) -> LspSymbol {
    LspSymbol {
        name: s.name,
        kind: s.kind,
        detail: s.detail,
        start: s.start,
        end: s.end,
        name_start: s.name_start,
        name_end: s.name_end,
        line: s.line,
        col: s.col,
        file: s.file,
        deprecated: s.deprecated,
        children: s.children.into_iter().map(symbol_wire).collect(),
    }
}

fn rename_edit(e: bennu_lsp::prelude::FileEdit, old_name: &str) -> RenameEdit {
    RenameEdit {
        file: e.file,
        start: e.start,
        end: e.end,
        new_text: e.new_text,
        // The FE guards on `old` still matching before applying. A server does not send the
        // replaced text, so the identifier under the caret is the honest value: it is what a
        // rename edit replaces at every site the server found.
        old: old_name.to_string(),
        reason: "reference".to_string(),
        // Never `inferred`: a language server resolved these, so unlike the Java engine's
        // same-name-method heuristic there is no guesswork to flag for review.
        inferred: false,
    }
}

fn rename_preview(
    outcome: bennu_lsp::prelude::RenameOutcome,
    old_name: &str,
    new_name: &str,
) -> RenamePreview {
    let total_edits = outcome.edits.len();
    // Group by file, in path order, so the preview is stable between runs.
    let mut by_file: std::collections::BTreeMap<String, Vec<RenameEdit>> =
        std::collections::BTreeMap::new();
    for e in outcome.edits {
        by_file.entry(e.file.clone()).or_default().push(rename_edit(e, old_name));
    }
    let mut files: Vec<RenameFileEdits> = by_file
        .into_iter()
        .map(|(file, mut edits)| {
            edits.sort_by_key(|e| e.start);
            RenameFileEdits { file, edits }
        })
        .collect();
    files.sort_by(|a, b| a.file.cmp(&b.file));

    // File operations cannot be carried out here, and a preview that did not say so would
    // present a rename that silently half-applies. Stated in the label, which is the one line
    // the FE always shows.
    let target_label = if outcome.file_ops.is_empty() {
        format!("`{old_name}`")
    } else {
        let ops: Vec<String> =
            outcome.file_ops.iter().map(bennu_lsp::prelude::FileOp::describe).collect();
        format!("`{old_name}` — also needs: {} (Bennu will not do this)", ops.join(", "))
    };

    RenamePreview {
        old_name: old_name.to_string(),
        new_name: new_name.to_string(),
        target_label,
        files,
        total_edits,
        // The FE nudges review when set. A pending file operation is exactly the case that
        // deserves a look before applying.
        has_inferred: !outcome.file_ops.is_empty(),
    }
}

/// A language server's markdown hover, split into the card's three slots.
///
/// rust-analyzer's hover is a module path in a code fence, then the item's signature in a
/// second fence, then prose after a `---` rule. The card renders `signature` as its title,
/// `container` as a muted meta line and `doc` as body text — which maps onto that almost
/// exactly, and degrades sensibly for a server that only sends prose.
fn hover_wire(markdown: &str) -> HoverInfo {
    let (fences, prose) = split_fences(markdown);
    let (container, signature) = match fences.len() {
        0 => (None, first_line(&prose)),
        1 => (None, fences[0].clone()),
        // Two or more: the first is the path the item lives at, the last is the item.
        _ => (Some(fences[0].clone()), fences[fences.len() - 1].clone()),
    };
    HoverInfo {
        signature,
        // Left empty on purpose: a server sends no kind, and inventing one ("rust") would put
        // a tag on every card that says nothing the file extension does not.
        kind: String::new(),
        container,
        doc: (!prose.trim().is_empty()).then(|| plainish(&prose)),
    }
}

/// Split markdown into its fenced code blocks and everything else.
fn split_fences(markdown: &str) -> (Vec<String>, String) {
    let mut fences: Vec<String> = Vec::new();
    let mut prose = String::new();
    let mut current: Option<String> = None;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            match current.take() {
                Some(block) => fences.push(block.trim().to_string()),
                None => current = Some(String::new()),
            }
            continue;
        }
        match current.as_mut() {
            Some(block) => {
                block.push_str(line);
                block.push('\n');
            }
            None => {
                prose.push_str(line);
                prose.push('\n');
            }
        }
    }
    // An unterminated fence: keep what it held rather than losing it.
    if let Some(block) = current {
        let block = block.trim();
        if !block.is_empty() {
            fences.push(block.to_string());
        }
    }
    fences.retain(|f| !f.is_empty());
    (fences, prose)
}

/// The first non-empty line of `text`.
fn first_line(text: &str) -> String {
    text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string()
}

/// Markdown reduced to something that reads correctly as plain text.
///
/// The hover card sets `textContent`, so a heading's `#` and a horizontal rule's `---` would
/// otherwise show up literally. Deliberately light: this is not a markdown renderer, it just
/// removes the marks that read as noise.
fn plainish(prose: &str) -> String {
    let mut out = String::with_capacity(prose.len());
    for line in prose.lines() {
        let trimmed = line.trim_end();
        // A rule is a paragraph break, not three dashes.
        if trimmed.trim() == "---" || trimmed.trim() == "***" {
            if !out.ends_with("\n\n") && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        let stripped = trimmed.trim_start();
        let body = stripped.strip_prefix("#").map(|s| s.trim_start_matches('#').trim_start());
        out.push_str(body.unwrap_or(trimmed));
        out.push('\n');
    }
    out.trim().to_string()
}

/// The identifier around `offset`, for the labels a server does not provide.
fn word_at(source: &str, offset: usize) -> Option<String> {
    let bytes = source.as_bytes();
    if offset > bytes.len() {
        return None;
    }
    let is_word = |b: u8| b == b'_' || b.is_ascii_alphanumeric();
    let mut start = offset.min(bytes.len());
    while start > 0 && is_word(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset.min(bytes.len());
    while end < bytes.len() && is_word(bytes[end]) {
        end += 1;
    }
    (end > start).then(|| source[start..end].to_string())
}

/// The trigger character immediately before the identifier prefix at `offset`, when it is one
/// the server asked to be told about.
///
/// Read from the buffer rather than plumbed down from the editor: the editor knows *a*
/// keystroke happened, but what the server needs is which of *its* trigger characters this
/// position follows — and for Rust that is the difference between the members after `.` and the
/// paths after `::`.
fn trigger_char(source: &str, offset: usize, triggers: &[String]) -> Option<String> {
    if triggers.is_empty() {
        return None;
    }
    let bytes = source.as_bytes();
    let mut i = offset.min(bytes.len());
    // Skip back over the identifier being typed.
    while i > 0 && (bytes[i - 1] == b'_' || bytes[i - 1].is_ascii_alphanumeric()) {
        i -= 1;
    }
    if i == 0 {
        return None;
    }
    // Longest match first, so `::` wins over `:`.
    let before = &source[..i];
    let mut best: Option<&String> = None;
    for t in triggers {
        if before.ends_with(t.as_str())
            && best.map(|b| t.len() > b.len()).unwrap_or(true)
        {
            best = Some(t);
        }
    }
    best.cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rust_analyzer_hover_splits_into_path_signature_and_prose() {
        let md = "```rust\nstd::vec\n```\n\n```rust\npub struct Vec<T, A = Global>\n```\n\n---\n\nA contiguous growable array type.\n";
        let info = hover_wire(md);
        assert_eq!(info.signature, "pub struct Vec<T, A = Global>");
        assert_eq!(info.container.as_deref(), Some("std::vec"));
        let doc = info.doc.unwrap();
        assert!(doc.contains("contiguous growable array"));
        assert!(!doc.contains("---"), "a rule is a paragraph break, not three dashes: {doc:?}");
    }

    #[test]
    fn a_single_fence_becomes_the_signature_with_no_container() {
        let info = hover_wire("```rust\nfn main()\n```");
        assert_eq!(info.signature, "fn main()");
        assert!(info.container.is_none());
        assert!(info.doc.is_none());
    }

    #[test]
    fn prose_only_hover_uses_its_first_line_as_the_signature() {
        let info = hover_wire("\nA local variable.\nMore detail here.\n");
        assert_eq!(info.signature, "A local variable.");
        assert!(info.doc.unwrap().contains("More detail"));
    }

    #[test]
    fn an_unterminated_fence_is_not_lost() {
        // Servers do send these when they truncate a long hover.
        let info = hover_wire("```rust\nfn f()\n");
        assert_eq!(info.signature, "fn f()");
    }

    #[test]
    fn headings_lose_their_hashes() {
        let info = hover_wire("prose\n\n### Safety\n\nDon't.\n");
        let doc = info.doc.unwrap();
        assert!(doc.contains("Safety"), "{doc:?}");
        assert!(!doc.contains('#'), "{doc:?}");
    }

    #[test]
    fn the_hover_card_gets_no_invented_kind() {
        // A tag reading "rust" on every card says nothing the file extension does not.
        assert!(hover_wire("```rust\nfn f()\n```").kind.is_empty());
    }

    #[test]
    fn the_trigger_character_prefers_the_longest_match() {
        // `::` and `.` are both rust-analyzer triggers, and telling it `:` instead of `::`
        // changes which list it computes.
        let triggers = vec![".".to_string(), ":".to_string(), "::".to_string()];
        assert_eq!(trigger_char("std::ve", 7, &triggers).as_deref(), Some("::"));
        assert_eq!(trigger_char("v.pu", 4, &triggers).as_deref(), Some("."));
        assert_eq!(trigger_char("let x", 5, &triggers), None, "no trigger before a bare word");
        assert_eq!(trigger_char("v.", 2, &triggers).as_deref(), Some("."), "empty prefix");
        assert_eq!(trigger_char("abc", 0, &triggers), None, "start of buffer");
        assert_eq!(trigger_char("v.pu", 4, &[]), None, "a server with no triggers");
    }

    #[test]
    fn the_word_under_the_caret_is_found_from_inside_and_from_either_edge() {
        assert_eq!(word_at("let value = 1;", 6).as_deref(), Some("value"));
        assert_eq!(word_at("let value = 1;", 4).as_deref(), Some("value"), "at the start");
        assert_eq!(word_at("let value = 1;", 9).as_deref(), Some("value"), "at the end");
        assert_eq!(word_at("let value = 1;", 10), None, "on the space");
        assert_eq!(word_at("x", 99), None, "past the end");
        assert_eq!(word_at("snake_case_1", 3).as_deref(), Some("snake_case_1"));
    }

    #[test]
    fn a_rename_preview_groups_by_file_and_sorts_within_it() {
        let outcome = bennu_lsp::prelude::RenameOutcome {
            edits: vec![
                bennu_lsp::prelude::FileEdit {
                    file: "/p/z.rs".into(),
                    start: 10,
                    end: 13,
                    new_text: "bar".into(),
                },
                bennu_lsp::prelude::FileEdit {
                    file: "/p/a.rs".into(),
                    start: 40,
                    end: 43,
                    new_text: "bar".into(),
                },
                bennu_lsp::prelude::FileEdit {
                    file: "/p/a.rs".into(),
                    start: 4,
                    end: 7,
                    new_text: "bar".into(),
                },
            ],
            file_ops: Vec::new(),
        };
        let preview = rename_preview(outcome, "foo", "bar");
        assert_eq!(preview.total_edits, 3);
        assert_eq!(preview.files.len(), 2);
        assert_eq!(preview.files[0].file, "/p/a.rs", "path order, so the preview is stable");
        assert_eq!(preview.files[0].edits[0].start, 4, "offset order within a file");
        assert!(!preview.has_inferred, "a server resolved these — nothing to second-guess");
        assert_eq!(preview.target_label, "`foo`");
    }

    #[test]
    fn a_rename_needing_a_file_move_says_so_in_the_preview() {
        // Otherwise the preview presents a rename that silently half-applies.
        let outcome = bennu_lsp::prelude::RenameOutcome {
            edits: vec![bennu_lsp::prelude::FileEdit {
                file: "/p/lib.rs".into(),
                start: 4,
                end: 7,
                new_text: "bar".into(),
            }],
            file_ops: vec![bennu_lsp::prelude::FileOp::Rename {
                from: "/p/foo.rs".into(),
                to: "/p/bar.rs".into(),
            }],
        };
        let preview = rename_preview(outcome, "foo", "bar");
        assert!(preview.target_label.contains("rename /p/foo.rs"), "{}", preview.target_label);
        assert!(preview.target_label.contains("will not"), "{}", preview.target_label);
        assert!(preview.has_inferred, "the FE nudges review, which is what this deserves");
    }

    #[test]
    fn a_declaration_label_falls_back_when_there_is_no_preview_line() {
        let t = SpanTarget {
            file: "/p/a.rs".into(),
            start: 0,
            end: 3,
            line: 1,
            col: 1,
            preview: String::new(),
        };
        assert_eq!(declaration_wire(t, "definition").label, "definition");

        let t = SpanTarget {
            file: "/p/a.rs".into(),
            start: 0,
            end: 3,
            line: 4,
            col: 8,
            preview: "pub fn insert(&mut self, k: K)".into(),
        };
        let d = declaration_wire(t, "definition");
        assert_eq!(d.label, "pub fn insert(&mut self, k: K)");
        assert_eq!((d.line, d.col), (4, 8));
    }

    #[test]
    fn a_completion_entry_keeps_what_the_editor_needs_to_insert_it_correctly() {
        let e = bennu_lsp::prelude::CompletionEntry {
            id: 7,
            label: "push(…)".into(),
            kind: "method".into(),
            detail: Some("fn(&mut self, T)".into()),
            doc: None,
            sort_text: Some("ffff".into()),
            filter_text: None,
            // Already parsed: the placeholder syntax never reaches this far, and the stop that was
            // written `$0` is a byte range into the plain text.
            insert_text: "push()".into(),
            replace: Some((10, 14)),
            is_snippet: true,
            snippet_stops: vec![bennu_lsp::prelude::SnippetStop { index: 0, start: 5, end: 5 }],
            additional_edits: vec![bennu_lsp::prelude::FileEdit {
                file: "/p/a.rs".into(),
                start: 0,
                end: 0,
                new_text: "use std::x;\n".into(),
            }],
            deprecated: false,
            preselect: true,
        };
        let w = completion_wire(e);
        assert_eq!(w.label, "push(…)", "the display string");
        assert_eq!(w.insert_text.as_deref(), Some("push()"), "what actually goes in the buffer");
        assert_eq!((w.replace_start, w.replace_end), (Some(10), Some(14)));
        assert!(w.snippet);
        // The stop crosses as a byte range into that text, so the editor needs no parser of its own.
        assert_eq!(w.snippet_stops.len(), 1);
        assert_eq!((w.snippet_stops[0].start, w.snippet_stops[0].end), (5, 5));
        assert_eq!(w.edits.len(), 1, "the auto-import must survive");
        assert_eq!(w.resolve_id, Some(7));
        assert!(w.auto_import.is_none(), "that is the native engine's mechanism");
    }
}
