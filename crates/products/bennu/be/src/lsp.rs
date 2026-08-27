//! `lsp` domain — the handlers that exist only for language-server-backed languages.
//!
//! Everything the FE already had a call for — completion, go-to, find-usages, hover,
//! diagnostics, rename — is **not** here: those handlers stay where they are and route through
//! [`crate::lsp_route`], so the editor needs no per-language branch to get them. What lives in
//! this module is the surface that had no Java equivalent to inherit:
//!
//! * **semantic tokens** — Bennu's own languages colour from a tree-sitter grammar, so nothing
//!   was asking a backend what colour a token is;
//! * **outline / workspace symbols** — the Java outline is computed in the frontend from its
//!   own parse, and there is no Rust parse there to compute one from;
//! * **format / code actions / signature help** — the Java engine has intentions and no
//!   formatter, which is a different shape;
//! * **server lifecycle** — status, restart, stop, and the installed-server list the settings
//!   panel needs.
//!
//! Every handler is graceful: a file no server serves, or a server still starting, answers with
//! the empty value rather than an error. A missing language server is not a broken editor.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{
    CompletionItem, DeclarationTarget, FileDiagnostics, LspAction, LspFold, LspHighlight, LspLens,
    LspMacroExpansion, LspServerInfo, LspSignature, LspStatus, LspSymbol, LspToken, SourceEdit,
    UsagesResult,
};
use serde::{Deserialize, Serialize};

use crate::lsp_registry::LspRegistry;
use crate::lsp_route;

/// Args for the handlers that classify a caret.
#[derive(Deserialize)]
pub struct LspPositionArgs {
    /// Absolute path (forward slashes) to the file the caret is in.
    pub file: String,
    /// The current (possibly-unsaved) buffer — the request is made against this.
    pub source: String,
    /// UTF-8 byte offset of the caret.
    pub offset: usize,
}

/// Args for the handlers that need a whole buffer but no caret.
#[derive(Deserialize)]
pub struct LspFileArgs {
    pub file: String,
    pub source: String,
}

// ---------------------------------------------------------------------------
// Lifecycle / status
// ---------------------------------------------------------------------------

/// Every language server this session has a slot for, with its state.
///
/// Also the point at which the registry learns the event sink: the FE polls this once on
/// startup, and from then on the registry can push `arbor://bennu/lsp-status` itself instead of
/// being asked. Background work — a start that finishes, a server that dies — has no request
/// behind it, so without this it would have nothing to emit on.
#[arbor_rpc::handler]
fn bennu_lsp_status(ctx: &BennuState) -> Result<Vec<LspStatus>, String> {
    LspRegistry::global().set_sink(ctx.event_sink());
    Ok(LspRegistry::global().statuses())
}

/// The servers Bennu knows how to run, resolved against this machine — the settings list.
///
/// Includes the ones that are **not** installed, with their install hints: a list that hid them
/// would answer "why is there no Rust intelligence" with silence.
#[arbor_rpc::handler]
fn bennu_lsp_servers(_ctx: &BennuState) -> Result<Vec<LspServerInfo>, String> {
    Ok(LspRegistry::global()
        .availability()
        .into_iter()
        .map(|a| LspServerInfo {
            id: a.id,
            name: a.name,
            language: a.language,
            extensions: a.extensions,
            path: a.path,
            command: a.command,
            install_hint: a.install_hint,
            enabled: a.enabled,
            custom: a.custom,
            install: a.install,
        })
        .collect())
}

/// Args for [`bennu_lsp_restart`] / [`bennu_lsp_stop`].
#[derive(Deserialize)]
pub struct LspServerArgs {
    /// The workspace root the server was started for.
    pub root: String,
    /// The language it serves.
    pub language: String,
}

/// Restart a server.
///
/// The only way out of a failed slot — failures are deliberately sticky, so a server that is
/// not installed is reported once instead of being respawned on every keystroke — and therefore
/// the fix for "I just installed it".
#[arbor_rpc::handler]
fn bennu_lsp_restart(ctx: &BennuState, args: LspServerArgs) -> Result<bool, String> {
    LspRegistry::global().set_sink(ctx.event_sink());
    Ok(LspRegistry::global().restart(&args.root, &args.language))
}

/// Stop a server and forget its slot.
#[arbor_rpc::handler]
fn bennu_lsp_stop(ctx: &BennuState, args: LspServerArgs) -> Result<bool, String> {
    LspRegistry::global().set_sink(ctx.event_sink());
    Ok(LspRegistry::global().stop(&args.root, &args.language))
}

// ---------------------------------------------------------------------------
// Highlighting
// ---------------------------------------------------------------------------

/// Semantic tokens for a buffer — the colouring only something that knows the types can do.
///
/// Decoded in the backend rather than the frontend: the protocol sends tokens delta-encoded in
/// the server's own position units, so turning them into offsets needs the buffer *and* its
/// line index, which are both already here.
#[arbor_rpc::handler]
fn bennu_lsp_semantic_tokens(_ctx: &BennuState, args: LspFileArgs) -> Result<Vec<LspToken>, String> {
    Ok(lsp_route::semantic_tokens(&args.file, &args.source))
}

// ---------------------------------------------------------------------------
// Structure
// ---------------------------------------------------------------------------

/// The document outline, for the Structure panel.
#[arbor_rpc::handler]
fn bennu_lsp_document_symbols(
    _ctx: &BennuState,
    args: LspFileArgs,
) -> Result<Vec<LspSymbol>, String> {
    Ok(lsp_route::document_symbols(&args.file, &args.source))
}

/// Every occurrence of the symbol at the caret, in the open buffer.
///
/// The cheap, one-file counterpart of find-usages: no panel, no workspace sweep, and a short
/// timeout, because what it feeds is a decoration the editor paints while the caret rests.
#[arbor_rpc::handler]
fn bennu_lsp_highlights(
    _ctx: &BennuState,
    args: LspPositionArgs,
) -> Result<Vec<LspHighlight>, String> {
    Ok(lsp_route::document_highlights(&args.file, &args.source, args.offset))
}

/// The chain of syntactic ranges enclosing the caret, innermost first — expand/shrink selection.
///
/// The **whole chain** in one answer rather than one range per press. Expanding is then walking a
/// list the editor already holds, which is what keeps a keypress from waiting on a round-trip.
#[arbor_rpc::handler]
fn bennu_lsp_selection_ranges(
    _ctx: &BennuState,
    args: LspPositionArgs,
) -> Result<Vec<[usize; 2]>, String> {
    Ok(lsp_route::selection_ranges(&args.file, &args.source, args.offset))
}

/// The foldable regions of a buffer.
///
/// Worth asking rather than folding on braces locally: the server folds by *item* — a `use` block,
/// a doc comment, a `#[cfg]`-gated module — and brace matching finds the function bodies and
/// nothing else.
#[arbor_rpc::handler]
fn bennu_lsp_folding(_ctx: &BennuState, args: LspFileArgs) -> Result<Vec<LspFold>, String> {
    Ok(lsp_route::folding_ranges(&args.file, &args.source))
}

/// The code lenses for a buffer, already resolved.
///
/// Resolving happens here and not on demand from the frontend because rust-analyzer returns its
/// counts with no title at all — an unresolved lens has nothing to draw, so a lazy frontend would
/// render a column of blanks and then fill it in.
#[arbor_rpc::handler]
fn bennu_lsp_code_lenses(_ctx: &BennuState, args: LspFileArgs) -> Result<Vec<LspLens>, String> {
    Ok(lsp_route::code_lenses(&args.file, &args.source))
}

/// Args for [`bennu_lsp_lens_locations`].
#[derive(Deserialize)]
pub struct LspLensPressArgs {
    pub file: String,
    pub source: String,
    /// The lens's title, which becomes the results list's heading — the lens said "3
    /// implementations", so that is what the list of three is called.
    pub title: String,
    /// The lens command's arguments, verbatim.
    pub arguments: Vec<serde_json::Value>,
}

/// The locations a lens carries in its command arguments.
///
/// A lens showing a count is a **client** command: rust-analyzer sends the whole location list along
/// with it, because it had to do the query to count them. So pressing one costs no request — this
/// reads what already arrived. `None` when the command is something else (a runnable), which the
/// caller reads as "nothing to list" rather than as a failure.
#[arbor_rpc::handler]
fn bennu_lsp_lens_locations(
    _ctx: &BennuState,
    args: LspLensPressArgs,
) -> Result<Option<UsagesResult>, String> {
    Ok(lsp_route::lens_locations(&args.file, &args.source, &args.title, &args.arguments))
}

/// Args for [`bennu_lsp_will_rename`].
#[derive(Deserialize)]
pub struct LspRenameFileArgs {
    /// The file as it is now.
    pub file: String,
    /// Where it is going.
    pub new_path: String,
}

/// The edits a file rename implies — a Rust `mod` declaration and every `use` path through the
/// module being moved.
///
/// Asked **before** the move, which is what the protocol method is for: the server answers about the
/// tree as it stands. Applying them, and doing the move, is the caller's job. An empty list is the
/// honest answer both for "nothing to change" and for a server without the capability.
#[arbor_rpc::handler]
fn bennu_lsp_will_rename(
    _ctx: &BennuState,
    args: LspRenameFileArgs,
) -> Result<Vec<SourceEdit>, String> {
    Ok(lsp_route::will_rename(&args.file, &args.new_path))
}

/// Args for [`bennu_lsp_reload_workspace`].
#[derive(Deserialize)]
pub struct LspScopeArgs {
    /// Any path inside the workspace — the project root is the natural one.
    pub scope: String,
}

/// Re-read the project's manifests and rebuild the crate graph.
///
/// What makes editing `Cargo.toml` take effect without restarting the server. `false` when no server
/// covers the scope or it has no such method — both are states, not errors.
#[arbor_rpc::handler]
fn bennu_lsp_reload_workspace(_ctx: &BennuState, args: LspScopeArgs) -> Result<bool, String> {
    Ok(lsp_route::reload_workspace(&args.scope))
}

/// Expand the macro at the caret.
///
/// `None` when the caret is not in a macro call. The expansion is text rather than a file the server
/// knows, so it cannot be navigated — see [`LspMacroExpansion`].
#[arbor_rpc::handler]
fn bennu_lsp_expand_macro(
    _ctx: &BennuState,
    args: LspPositionArgs,
) -> Result<Option<LspMacroExpansion>, String> {
    Ok(lsp_route::expand_macro(&args.file, &args.source, args.offset))
}

/// Args for [`bennu_lsp_workspace_symbols`].
#[derive(Deserialize)]
pub struct LspSearchArgs {
    /// Any file inside the workspace — picks which server answers.
    pub file: String,
    /// The query the user typed.
    pub query: String,
}

/// Search symbols across the workspace — "go to symbol everywhere" for a server-backed
/// language.
#[arbor_rpc::handler]
fn bennu_lsp_workspace_symbols(
    _ctx: &BennuState,
    args: LspSearchArgs,
) -> Result<Vec<LspSymbol>, String> {
    Ok(lsp_route::workspace_symbols(&args.file, &args.query))
}

// ---------------------------------------------------------------------------
// Navigation extras
// ---------------------------------------------------------------------------

/// Go to the **type** of the expression under the caret — a distinct gesture from go-to
/// definition, and the one that answers "what *is* this thing" on a `let` binding.
#[arbor_rpc::handler]
fn bennu_lsp_type_definition(
    _ctx: &BennuState,
    args: LspPositionArgs,
) -> Result<Option<DeclarationTarget>, String> {
    Ok(lsp_route::type_definition(&args.file, &args.source, args.offset).flatten())
}

/// Go to implementations — for a Rust trait method, every `impl` of it. A list rather than a
/// single target, because there is rarely one and picking would hide the rest.
#[arbor_rpc::handler]
fn bennu_lsp_implementations(
    _ctx: &BennuState,
    args: LspPositionArgs,
) -> Result<Option<UsagesResult>, String> {
    Ok(lsp_route::implementations(&args.file, &args.source, args.offset)
        .filter(|r| !r.usages.is_empty()))
}

/// Signature help — the parameter list of the call the caret is inside, with the current
/// parameter marked.
#[arbor_rpc::handler]
fn bennu_lsp_signature_help(
    _ctx: &BennuState,
    args: LspPositionArgs,
) -> Result<Option<LspSignature>, String> {
    Ok(lsp_route::signature_help(&args.file, &args.source, args.offset))
}

/// Args for [`bennu_lsp_resolve_completion`].
#[derive(Deserialize)]
pub struct LspResolveArgs {
    pub file: String,
    /// The `resolve_id` from the completion item the user highlighted.
    pub id: usize,
}

/// Fill in one completion candidate's documentation.
///
/// Servers answer a completion list without docs and fill them in one item at a time — asking
/// for four hundred eagerly would be four hundred round-trips. `None` when the list has been
/// superseded, which is the normal outcome of the user carrying on typing.
#[arbor_rpc::handler]
fn bennu_lsp_resolve_completion(
    _ctx: &BennuState,
    args: LspResolveArgs,
) -> Result<Option<CompletionItem>, String> {
    Ok(lsp_route::resolve_completion(&args.file, args.id))
}

// ---------------------------------------------------------------------------
// Editing
// ---------------------------------------------------------------------------

/// Args for [`bennu_lsp_format`].
#[derive(Deserialize)]
pub struct LspFormatArgs {
    pub file: String,
    pub source: String,
    /// Indent width. Absent → the editor's configured `indent_width`.
    #[serde(default)]
    pub tab_size: Option<u32>,
    /// Absent → spaces, matching Bennu's own editor default.
    #[serde(default)]
    pub insert_spaces: Option<bool>,
}

/// Format the whole buffer (`rustfmt`, for Rust).
///
/// Returns edits rather than the formatted text: the FE applies them through CodeMirror, so the
/// format lands in the undo history like any other change and the caret keeps its place.
#[arbor_rpc::handler]
fn bennu_lsp_format(_ctx: &BennuState, args: LspFormatArgs) -> Result<Vec<SourceEdit>, String> {
    let cfg = bennu_core::config::load();
    let tab_size = args.tab_size.unwrap_or(cfg.indent_width).max(1);
    Ok(lsp_route::format(
        &args.file,
        &args.source,
        tab_size,
        args.insert_spaces.unwrap_or(true),
    ))
}

/// Args for [`bennu_lsp_code_actions`].
#[derive(Deserialize)]
pub struct LspActionArgs {
    pub file: String,
    pub source: String,
    /// Byte range of the selection, or `start == end` for a caret.
    pub start: usize,
    pub end: usize,
}

/// Quick fixes and refactorings offered at the caret / selection — the Alt+Enter list for a
/// server-backed language.
#[arbor_rpc::handler]
fn bennu_lsp_code_actions(_ctx: &BennuState, args: LspActionArgs) -> Result<Vec<LspAction>, String> {
    Ok(lsp_route::code_actions(&args.file, &args.source, args.start, args.end))
}

/// Args for [`bennu_lsp_execute_command`].
#[derive(Deserialize)]
pub struct LspCommandArgs {
    /// Any file in the workspace — picks which server runs it.
    pub file: String,
    pub command: String,
    #[serde(default)]
    pub arguments: Vec<serde_json::Value>,
}

/// Run a server command — how an action whose edit is computed lazily is applied.
///
/// The resulting edits come back on `arbor://bennu/lsp-apply-edit` for the FE to apply, so the
/// answer here is only whether the command ran.
#[arbor_rpc::handler]
fn bennu_lsp_execute_command(ctx: &BennuState, args: LspCommandArgs) -> Result<bool, String> {
    LspRegistry::global().set_sink(ctx.event_sink());
    Ok(lsp_route::execute_command(&args.file, &args.command, args.arguments))
}

// ---------------------------------------------------------------------------
// Document lifecycle
// ---------------------------------------------------------------------------

/// A buffer was saved.
///
/// Worth its own call: for rust-analyzer this is what triggers `cargo check`, and therefore what
/// produces the real type and borrow errors as opposed to only what the parser can see.
#[arbor_rpc::handler]
fn bennu_lsp_did_save(_ctx: &BennuState, args: LspFileArgs) -> Result<bool, String> {
    Ok(lsp_route::did_save(&args.file, &args.source))
}

/// Args for [`bennu_lsp_did_close`].
#[derive(Deserialize)]
pub struct LspCloseArgs {
    pub file: String,
}

/// A tab was closed — the server can drop its copy of the document (and, for most servers, its
/// diagnostics for it). Never starts a server.
#[arbor_rpc::handler]
fn bennu_lsp_did_close(_ctx: &BennuState, args: LspCloseArgs) -> Result<bool, String> {
    Ok(lsp_route::did_close(&args.file))
}

/// Args for [`bennu_lsp_problems`].
#[derive(Deserialize)]
pub struct LspProblemsArgs {
    /// Any file inside the workspace — picks which server answers.
    pub file: String,
}

/// Every file the server currently reports problems in, for the project-wide Problems panel.
///
/// Cannot be assembled from the open buffers: a server publishes for files nobody has opened,
/// which is the entire value of having `cargo check` behind it.
#[arbor_rpc::handler]
fn bennu_lsp_problems(
    _ctx: &BennuState,
    args: LspProblemsArgs,
) -> Result<Vec<FileDiagnostics>, String> {
    Ok(lsp_route::problems(&args.file))
}

// ── installing a server ─────────────────────────────────────────────────────────

/// Args for [`bennu_lsp_install`].
#[derive(Deserialize)]
pub struct LspInstallArgs {
    /// The catalogue id — `"wgsl-analyzer"`, `"rust-analyzer"`, …
    pub id: String,
}

/// Whether it worked, and what it said.
#[derive(Serialize)]
pub struct LspInstallResult {
    pub ok: bool,
    /// The command that ran, as it would be typed. Shown so the user can run it themselves
    /// (or read what went wrong in their own terminal) rather than being told only that
    /// something failed.
    pub command: String,
    /// The path the server resolved to afterwards, when it is now there.
    pub path: Option<String>,
    /// The last lines of output, for the failure case. The whole log went to the Build
    /// panel while it ran; this is what a toast can say.
    pub tail: String,
    /// A one-line diagnosis, when the failure is one with a known fix. Shown instead of
    /// [`tail`](Self::tail) — the raw output is still in the Build panel, and a toast has
    /// room for the answer or for the evidence but not for both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Turn a failed install's output into a sentence with a next step in it, when it is one of
/// the failures that has one.
///
/// Only the ones where the raw message is actively misleading about what to do. Everything
/// else keeps its own words: a build error inside a dependency says more than any summary
/// this could write, and replacing it with a guess would be worse than showing it.
fn diagnose(tail: &str) -> Option<String> {
    // `cannot install package `x 0.0.0`, it requires rustc 1.97.1 or newer, while the
    // currently active rustc version is 1.97.0` — a toolchain problem wearing the clothes
    // of a package problem. The fix is one command and it is not in the message.
    if let Some(at) = tail.find("requires rustc ") {
        let needed = tail[at + "requires rustc ".len()..]
            .split_whitespace()
            .next()
            .unwrap_or("a newer version");
        return Some(format!(
            "Your Rust toolchain is too old — this server needs rustc {needed}.              Run `rustup update` and try again."
        ));
    }
    // The package manager itself is missing. Different problem, different fix, and the
    // shell's own wording ("command not found") does not say which server it was for.
    if tail.contains("is not on your PATH") {
        return Some(tail.trim().to_string());
    }
    None
}

/// Install a language server by running the command its own ecosystem ships it through.
///
/// A **command**, not a download, and that is the whole design. Every server in the
/// catalogue is distributed through a package manager the user already has — `rustup`,
/// `cargo`, `go`, `npm` — so running it is shorter than a downloader by everything a
/// downloader has to invent: release-asset naming per platform, archive formats, checksums,
/// where to put the binary, how to upgrade it, what happens when Arbor is uninstalled. It
/// also lands the binary where the rest of the toolchain lives, so it keeps working
/// afterwards and `wgsl-analyzer --version` says the same thing in a terminal.
///
/// Streams into the **Build** panel as it goes: a `cargo install --git` builds a language
/// server from source and takes minutes, and a button that goes quiet for three minutes is
/// a button people press twice.
///
/// Refused for a server with no install command — a system package (clangd, Homebrew's
/// lua-language-server) or a user-defined one. Bennu installs language servers; it does not
/// manage the machine.
#[arbor_rpc::handler]
fn bennu_lsp_install(ctx: &BennuState, args: LspInstallArgs) -> Result<LspInstallResult, String> {
    let server = LspRegistry::global()
        .availability()
        .into_iter()
        .find(|s| s.id == args.id)
        .ok_or_else(|| format!("no language server called {}", args.id))?;

    if server.install.is_empty() {
        return Err(format!(
            "{} is installed through your system's package manager — {}",
            server.name, server.install_hint
        ));
    }
    let printed = server.install.join(" ");
    let out = crate::child::run_streamed(&server.install, ctx.event_sink(), "Installing")?;

    // Whatever happened, stop remembering that this server was missing — an install that
    // worked has just made the memo wrong, and one that failed costs a single `PATH` scan
    // to re-establish.
    LspRegistry::global().forget_missing();

    // Re-resolve rather than trusting the exit code: `cargo install` can succeed having put
    // the binary somewhere that is not on `PATH`, and the only answer that matters to the
    // editor is whether it can find it now.
    let path = LspRegistry::global().availability().into_iter().find(|s| s.id == args.id).and_then(|s| s.path);
    let hint = if out.ok && path.is_none() {
        // Built fine, and the editor still cannot find it. Worth saying explicitly: it is
        // the one outcome where "installed" and "working" come apart, and the field below
        // is where the user fixes it.
        Some(format!(
            "It installed, but `{}` is still not on your PATH — set the executable path below.",
            server.command
        ))
    } else {
        diagnose(&out.tail)
    };
    Ok(LspInstallResult { ok: out.ok && path.is_some(), command: printed, path, tail: out.tail, hint })
}

#[cfg(test)]
mod install_tests {
    use super::diagnose;

    #[test]
    fn a_toolchain_too_old_is_named_as_one() {
        let tail = "error: cannot install package `wgsl-analyzer 0.0.0`, it requires rustc \
                    1.97.1 or newer, while the currently active rustc version is 1.97.0";
        let hint = diagnose(tail).expect("this failure has a known fix");
        assert!(hint.contains("1.97.1"), "the version it needs is the actionable part: {hint}");
        assert!(hint.contains("rustup update"), "and so is the command: {hint}");
    }

    #[test]
    fn an_ordinary_build_error_keeps_its_own_words() {
        let tail = "error[E0432]: unresolved import `foo::bar`\n  --> src/lib.rs:3:5";
        assert!(
            diagnose(tail).is_none(),
            "a compiler error says more than any summary this could write"
        );
    }
}
