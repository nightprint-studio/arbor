//! `intel` domain — `bennu_completion` / `bennu_diagnostics` / `bennu_definition` /
//! `bennu_did_change`.
//!
//! `bennu_completion` serves member-access candidates from the per-project index the
//! [`crate::index_service`] builds off-thread on `bennu_open_project`. Until that build
//! lands (or when no open project owns the file), it returns the benign empty list —
//! the FE shows nothing gracefully.
//!
//! `bennu_definition` resolves a JSP form/link **action reference** to its
//! go-to-definition target — the config fragment the `<action>` is declared in, the
//! implementation class it maps to (the C1 chain: action → Spring bean-id → FQCN), and
//! the view JSP (the Tiles chain). Served from the config-graph resolver the index
//! service builds; empty while the config is still loading.
//!
//! `bennu_diagnostics` reports the conservative **"action inesistente"** diagnostic for
//! JSP action references passed in `actions`: a reference is flagged only when it maps
//! to no concrete action AND no wildcard/computed path could match it (docs §8). A
//! Java-file diagnostics request (no `actions`) stays the empty stub for now.
//!
//! `bennu_did_change` is the **live-edit re-index** hook: on an editor change it
//! re-extracts just the edited file and patches the persisted index. The serve loop
//! dispatches each request on its **own thread** (see `arbor_ipc::serve_stdio`), so this
//! runs off the IPC read loop and never blocks other requests; the patch is truly
//! incremental (only the changed file is re-parsed — no whole-project walk).

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{ActionVerdict, CompletionItem, IntelProvider, NativeJavaProvider};
use bennu_proto::prelude::Diagnostic;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for [`bennu_completion`].
#[derive(Deserialize)]
pub struct CompletionArgs {
    /// Absolute path to the file the caret is in.
    pub file: String,
    /// Byte offset of the caret in the file.
    pub offset: usize,
}

/// Completion candidates at a position — served from the owning project's built index
/// (empty while the index is still building, per the async lifecycle).
#[arbor_rpc::handler]
fn bennu_completion(_ctx: &BennuState, args: CompletionArgs) -> Result<Vec<CompletionItem>, String> {
    Ok(IndexService::global().completion(&args.file, args.offset))
}

/// One JSP action reference to check for existence: its qualified name plus the byte
/// range in the file, so a "missing" verdict maps back to the offending text.
#[derive(Deserialize)]
pub struct ActionRef {
    /// The action qualified name the JSP refers to (`/do/Category/viewTree`).
    pub qualified_name: String,
    /// Start byte offset of the reference in the file.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

/// Args for [`bennu_diagnostics`].
#[derive(Deserialize)]
pub struct DiagnosticsArgs {
    /// Absolute path to the file to diagnose.
    pub file: String,
    /// JSP action references extracted from the file by the FE. When present, each is
    /// checked for existence conservatively (exists / missing / inconclusive). Absent /
    /// empty for a plain Java file (→ the empty stub for now).
    #[serde(default)]
    pub actions: Vec<ActionRef>,
}

/// Diagnostics for a file. For a JSP with `actions`, reports the conservative
/// "action inesistente" diagnostic (a genuinely-missing reference → a `warning`; a
/// wildcard/computed candidate → nothing, never a false positive). For a plain Java
/// file, the empty stub (syntactic diagnostics land with tree-sitter in a later wave).
#[arbor_rpc::handler]
fn bennu_diagnostics(_ctx: &BennuState, args: DiagnosticsArgs) -> Result<Vec<Diagnostic>, String> {
    if args.actions.is_empty() {
        let provider = NativeJavaProvider::new();
        return provider.diagnostics(&args.file).map_err(|e| e.to_string());
    }
    let svc = IndexService::global();
    let mut out = Vec::new();
    for a in &args.actions {
        // Conservative: only a genuine `Missing` (no action, no wildcard, no OGNL) is a
        // diagnostic. `Exists` and `Inconclusive` produce nothing (docs §8).
        if let ActionVerdict::Missing = svc.diagnose_action(&args.file, &a.qualified_name) {
            out.push(Diagnostic {
                message: format!("Struts action `{}` does not exist", a.qualified_name),
                severity: "warning".to_string(),
                start: a.start,
                end: a.end,
            });
        }
    }
    Ok(out)
}

/// Args for [`bennu_definition`].
#[derive(Deserialize)]
pub struct DefinitionArgs {
    /// Absolute path to a file inside the project (to pick the owning project's config).
    pub file: String,
    /// The JSP action reference to resolve (`/do/Category/viewTree`).
    pub action: String,
}

/// A resolved go-to-definition target for a JSP action reference.
#[derive(Serialize)]
pub struct DefinitionResult {
    /// The struts config fragment the `<action>` is declared in.
    pub config_file: String,
    /// The resolved implementation class FQCN (the C1 chain), if resolvable.
    pub class_fqcn: Option<String>,
    /// The resolved view JSP (the Tiles chain), if resolvable.
    pub view_jsp: Option<String>,
}

/// Resolve a JSP form/link action reference to its definition (config fragment + the
/// implementation class + the view JSP). `None`-shaped empty result when no project owns
/// the file, the config isn't built yet, or the action is unknown.
#[arbor_rpc::handler]
fn bennu_definition(
    _ctx: &BennuState,
    args: DefinitionArgs,
) -> Result<Option<DefinitionResult>, String> {
    Ok(IndexService::global().definition_action(&args.file, &args.action).map(|d| {
        DefinitionResult {
            config_file: d.config_file,
            class_fqcn: d.class_fqcn,
            view_jsp: d.view_jsp,
        }
    }))
}

/// Args for [`bennu_did_change`].
#[derive(Deserialize)]
pub struct DidChangeArgs {
    /// Absolute path to the edited file.
    pub file: String,
    /// The new full text of the file. `None` means the file was deleted.
    #[serde(default)]
    pub text: Option<String>,
}

/// Live-edit re-index: patch the persisted index for the edited file so completion /
/// definition reflect the change without reopening the project. Runs off the IPC read loop
/// (the serve loop dispatches each request on its own thread) and is truly incremental —
/// only the changed file is re-parsed. Returns `true` when a project owns the file (the
/// patch ran), `false` otherwise.
#[arbor_rpc::handler]
fn bennu_did_change(_ctx: &BennuState, args: DidChangeArgs) -> Result<bool, String> {
    IndexService::global().patch_file(&args.file, args.text.as_deref());
    Ok(true)
}
