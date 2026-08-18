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
use bennu_intel::prelude::{ActionVerdict, CompletionItem};
use bennu_proto::prelude::{Diagnostic, UsagesResult};
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for [`bennu_completion`].
#[derive(Deserialize)]
pub struct CompletionArgs {
    /// Absolute path to the file the caret is in.
    pub file: String,
    /// Byte offset of the caret in the file.
    pub offset: usize,
    /// The live (possibly-unsaved) buffer text. The `offset` is in ITS coordinates — the `.` the
    /// user just typed to trigger completion is unsaved, so the BE must parse this text, not the
    /// stale on-disk file. Absent → the BE falls back to reading the file from disk.
    #[serde(default)]
    pub source: Option<String>,
}

/// Completion candidates at a position.
///
/// One handler, two engines: a language-server-backed file (a `.rs` in a Cargo workspace) is
/// answered by [`crate::lsp_route`], everything else by the owning project's built index. The
/// frontend has no per-language branch for this — which is the point of the provider seam.
///
/// Both engines answer the empty list while they are still warming up, so a project that has
/// just been opened degrades the same way regardless of which one owns the file.
#[arbor_rpc::handler]
fn bennu_completion(_ctx: &BennuState, args: CompletionArgs) -> Result<Vec<CompletionItem>, String> {
    // A language server needs the live buffer to answer at all — the caret offset is in its
    // coordinates. Without one there is nothing to ask, and falling through to the Java index
    // for a `.rs` file would be worse than answering nothing.
    if let Some(source) = args.source.as_deref() {
        if let Some(items) = crate::lsp_route::completion(&args.file, args.offset, source) {
            return Ok(items);
        }
    } else if crate::lsp_route::owns(&args.file) {
        return Ok(Vec::new());
    }
    // A `Cargo.toml` — answered from the manifest schema and this machine's crate catalogue. Routed
    // before the index for the same reason a `.rs` file is: the Java engine has nothing to say about
    // it, and letting it answer would offer Java members inside a manifest.
    if let Some(items) =
        crate::cargo_intel::completion(&args.file, args.offset, args.source.as_deref())
    {
        return Ok(items);
    }
    Ok(IndexService::global().completion(&args.file, args.offset, args.source.as_deref()))
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
    /// The current (possibly-unsaved) buffer text. For a Java file it drives the AST-level
    /// validation (`bennu-check`); absent → no Java diagnostics (the on-disk file may be stale).
    #[serde(default)]
    pub source: Option<String>,
    /// Which validation tier to run for a Java file. `Some(false)` = the FAST pure-AST pass only
    /// (syntax / structure / unused imports — instant squiggles while typing); `Some(true)` / absent
    /// = the FULL resolver-backed pass (adds unknown-member / type / inheritance / … checks). The FE
    /// runs the fast pass on a short debounce and the full pass on a longer idle debounce, so a large
    /// file stays responsive. Ignored for a JSP (routed by `actions`).
    #[serde(default)]
    pub resolved: Option<bool>,
    /// JSP action references extracted from the file by the FE. When present, each is
    /// checked for existence conservatively (exists / missing / inconclusive). Absent /
    /// empty for a plain Java file (→ the AST validator).
    #[serde(default)]
    pub actions: Vec<ActionRef>,
}

/// Diagnostics for a file. For a JSP with `actions`, reports the conservative
/// "action inesistente" diagnostic (a genuinely-missing reference → a `warning`; a
/// wildcard/computed candidate → nothing, never a false positive). For a plain Java
/// file, the empty stub (syntactic diagnostics land with tree-sitter in a later wave).
#[arbor_rpc::handler]
fn bennu_diagnostics(_ctx: &BennuState, args: DiagnosticsArgs) -> Result<Vec<Diagnostic>, String> {
    // A language-server-backed file first. Its diagnostics are **pushed** by the server (for
    // Rust they arrive when `cargo check` finishes, seconds after a save), so this call reads the
    // last publish rather than computing anything — which is why it is cheap enough to sit on the
    // same debounce as the Java validation, and why it ignores the fast/full `resolved` tier: a
    // server has only one answer.
    if let Some(mut diags) = crate::lsp_route::diagnostics(&args.file, args.source.as_deref()) {
        // …plus whatever a framework extension has to say about the same file. A server answering
        // for a language does not mean nothing else has anything to add: rust-analyzer reports what
        // the compiler reports, and a pair of Bevy systems that contend over a resource with nothing
        // ordering them compiles perfectly. Returning the server's answer alone here is what made
        // every extension diagnostic on a `.rs` unreachable.
        diags.extend(crate::frameworks::diagnostics_for(&args.file, args.source.as_deref()));
        return Ok(diags);
    }
    // A `Cargo.toml`. Its own validator rather than a language server's: rust-analyzer reports very
    // little about a manifest, and what it does report arrives only after a reload — whereas a typo
    // in a key name is worth a squiggle the moment it is typed.
    if let Some(diags) = crate::cargo_intel::diagnostics(&args.file, args.source.as_deref()) {
        return Ok(diags);
    }
    // Action refs to check: the FE's explicit list when present, else — for a JSP — the
    // refs the BE extracts itself (reusing `bennu-web`'s scan), so squiggles work with no
    // FE change. A plain Java file with no actions falls to the native syntactic stub.
    let actions: Vec<ActionRef> = if !args.actions.is_empty() {
        args.actions
    } else if is_jsp_file(&args.file) {
        // Only ABSOLUTE refs (`/ns/name`) are diagnosable: a relative ref needs the
        // package namespace we don't map from a JSP path, so it stays Inconclusive
        // (never a false "missing"). Computed (`${…}`/`%{…}`) refs are already dropped.
        bennu_web::prelude::parse_jsp_file(std::path::Path::new(&args.file))
            .action_refs
            .into_iter()
            .filter(|r| !r.computed && r.name.starts_with('/'))
            .map(|r| ActionRef { qualified_name: r.name, start: r.start, end: r.end })
            .collect()
    } else {
        // A Java file → AST-level validation (syntax errors + unused imports) over the live buffer,
        // no compile needed. Other file types have no diagnostics here.
        let mut java = Vec::new();
        if is_java_file(&args.file) {
            if let Some(source) = &args.source {
                // Route through the owning project's provider so the resolver-backed checks
                // (unknown members via type inference) run when the index is built; falls back to
                // the pure AST checks otherwise. `resolved` picks the tier (fast pure-AST vs full).
                java = IndexService::global().validate_java(
                    &args.file,
                    source,
                    args.resolved.unwrap_or(true),
                );
            }
        }
        // Framework-contributed problems (Spring placeholders / SpEL / bean XML) ride the same
        // pipe as the language's own, so the editor needs no second request and the Problems
        // panel needs no second source. Skipped on the FAST tier: the fast pass exists to paint
        // syntax squiggles within ~120ms of a keystroke, and a framework check is a project-wide
        // question. Not restricted to `.java` — a bean XML has diagnostics and no Java validation.
        if args.resolved.unwrap_or(true) {
            java.extend(crate::frameworks::diagnostics_for(&args.file, args.source.as_deref()));
        }
        return Ok(java);
    };

    let svc = IndexService::global();
    let mut out = Vec::new();
    for a in &actions {
        // Conservative: only a genuine `Missing` (no action, no wildcard, no OGNL) is a
        // diagnostic. `Exists` and `Inconclusive` produce nothing (docs §8).
        if let ActionVerdict::Missing = svc.diagnose_action(&args.file, &a.qualified_name) {
            out.push(Diagnostic {
                message: format!("Struts action `{}` does not exist", a.qualified_name),
                severity: "warning".to_string(),
                code: "struts-action-missing".to_string(),
                start: a.start,
                end: a.end,
            });
        }
    }

    // Include-existence linting (JSP only): a static `<%@ include file>` / `<jsp:include page>`
    // / `<s:include value>` / `<c:import url>` pointing at a file that doesn't exist on disk
    // gets a warning. Computed (`${…}`/`%{…}`) and external (`http(s)://`) references are never
    // flagged — same conservative stance as the action check.
    if is_jsp_file(&args.file) {
        // The tag libraries the page declares: a tag its own TLD does not have, an attribute
        // that does not exist, a required one that is missing, a `uri` nothing on the classpath
        // ships. Same pipe as the action and include checks, and — like them — the live buffer
        // rather than the file on disk, since a page is checked while it is being written.
        // Skipped on the FAST tier for the same reason the Java framework checks are: this is a
        // project-wide question and the fast pass exists to paint syntax within a keystroke.
        if args.resolved.unwrap_or(true) {
            out.extend(crate::frameworks::diagnostics_for(&args.file, args.source.as_deref()));
        }
        for inc in bennu_web::prelude::unresolved_includes_file(std::path::Path::new(&args.file)) {
            out.push(Diagnostic {
                message: format!("Included file `{}` was not found", inc.raw),
                severity: "warning".to_string(),
                code: "included-file-missing".to_string(),
                start: inc.start,
                end: inc.end,
            });
        }
    }
    Ok(out)
}

/// True when `file` is a JSP-family file (case-insensitive extension).
fn is_jsp_file(file: &str) -> bool {
    let f = file.to_ascii_lowercase();
    f.ends_with(".jsp") || f.ends_with(".jspf") || f.ends_with(".tag") || f.ends_with(".tagx")
}

/// True when `file` is a Java source (case-insensitive `.java`).
fn is_java_file(file: &str) -> bool {
    file.to_ascii_lowercase().ends_with(".java")
}

/// Args for [`bennu_definition`].
#[derive(Deserialize)]
pub struct DefinitionArgs {
    /// Absolute path to a file inside the project (to pick the owning project's config).
    pub file: String,
    /// The JSP action reference to resolve (`/do/Category/viewTree`).
    pub action: String,
    /// The live JSP buffer (optional) — lets the resolver fold an enclosing `<s:url namespace="…">`
    /// onto a relative `action="…"` at `offset` when the bare `action` string doesn't resolve.
    #[serde(default)]
    pub source: Option<String>,
    /// Byte offset of the caret in `source` (optional; pairs with it for the namespace-fold fallback).
    #[serde(default)]
    pub offset: Option<usize>,
}

/// A resolved go-to-definition target for a JSP action reference.
#[derive(Serialize)]
pub struct DefinitionResult {
    /// The struts config fragment the `<action>` is declared in.
    pub config_file: String,
    /// Byte offset of the `<action>` element in `config_file` — the FE jumps here so go-to
    /// lands on the declaration line, not the top of the file.
    pub config_offset: usize,
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
    let svc = IndexService::global();
    // 1. The verbatim ref — resolves absolute paths and Entando `<wp:action path="/ExtStr2/…">` URLs.
    let mut resolved = svc.definition_action(&args.file, &args.action);
    // 2. Namespace-fold fallback: a `<s:url namespace="/do/Cat" action="viewTree">` gives the FE only
    //    the bare `viewTree` under the caret, which is ambiguous across namespaces. Re-scan the JSP
    //    buffer at the caret offset — `parse_jsp` folds the enclosing `namespace=` onto the relative
    //    action, yielding the qualified `/do/Cat/viewTree` that resolves unambiguously.
    if resolved.is_none() {
        if let (Some(src), Some(off)) = (&args.source, args.offset) {
            if is_jsp_file(&args.file) {
                if let Some(qname) = action_ref_at(src, off) {
                    resolved = svc.definition_action(&args.file, &qname);
                }
            }
        }
    }
    Ok(resolved.map(|d| DefinitionResult {
        config_file: d.config_file,
        config_offset: d.config_offset,
        class_fqcn: d.class_fqcn,
        view_jsp: d.view_jsp,
    }))
}

/// The (namespace-folded) qualified name of the JSP action reference whose span covers `offset`, if
/// any — `parse_jsp` already folds an enclosing `namespace="…"` onto a relative `action`. Computed
/// refs (`${…}`/`%{…}`) are skipped (never a static target).
fn action_ref_at(source: &str, offset: usize) -> Option<String> {
    bennu_web::prelude::parse_jsp(source)
        .action_refs
        .into_iter()
        .find(|r| !r.computed && offset >= r.start && offset <= r.end)
        .map(|r| r.name)
}

/// Args for [`bennu_decompiled_source`].
#[derive(Deserialize)]
pub struct DecompiledArgs {
    /// A file inside the owning project (to pick its resolver).
    pub file: String,
    /// The live buffer (its imports resolve a bare type name).
    pub source: String,
    /// The type name under the caret — a simple name (`List`) or a dotted FQCN (`java.util.List`).
    pub name: String,
}

/// A generated source-view location: the on-disk `.java` path + the byte offset to jump to, plus
/// whether the FE should offer "Download sources".
#[derive(Serialize)]
pub struct DecompiledLocation {
    pub file: String,
    pub offset: usize,
    /// `true` when a signatures-only stub was served for a third-party dependency — the tab shows a
    /// "Download sources" banner. `false` for real source (JDK / already-downloaded dependency).
    pub can_download: bool,
}

/// Resolve `name` (a library/JDK type under the caret) to an on-disk **source view** and return its
/// path — the real `.java` (JDK `src.zip` / a downloaded dependency `-sources.jar`) when available,
/// else a decompiled-from-bytecode stub. `None`-shaped empty result when the name doesn't resolve,
/// is a project type (real source exists), or can't be decoded.
#[arbor_rpc::handler]
fn bennu_decompiled_source(
    _ctx: &BennuState,
    args: DecompiledArgs,
) -> Result<Option<DecompiledLocation>, String> {
    Ok(IndexService::global().decompiled_stub(&args.file, &args.source, &args.name).map(|v| {
        DecompiledLocation { file: v.file, offset: v.offset, can_download: v.can_download }
    }))
}

/// Args for [`bennu_download_sources`].
#[derive(Deserialize)]
pub struct DownloadSourcesArgs {
    /// A file inside the owning project (to pick its resolver + dependency jars).
    pub file: String,
    /// The live buffer (its imports resolve a bare type name).
    pub source: String,
    /// The library type whose dependency sources to fetch (simple name or dotted FQCN).
    pub name: String,
    /// The open decompiled tab's on-disk path — echoed back in `sources-ready` so the FE reloads the
    /// right tab (and clears its spinner on failure).
    pub view_path: String,
}

/// Fetch the `-sources.jar` for the dependency that owns `name` via `mvn dependency:get`, as a
/// tracked background job (returns immediately). On completion emits `arbor://bennu/sources-ready`
/// for the tab at `view_path`; on success the tab reloads with the real source. `Err` fast only when
/// the type isn't a resolvable library type.
#[arbor_rpc::handler]
fn bennu_download_sources(ctx: &BennuState, args: DownloadSourcesArgs) -> Result<String, String> {
    IndexService::global().download_sources(
        &args.file,
        &args.source,
        &args.name,
        &args.view_path,
        ctx.host_caller(),
        ctx.event_sink(),
    )
}

/// Args for [`bennu_bean_class`].
#[derive(Deserialize)]
pub struct BeanClassArgs {
    /// Absolute path to a file inside the project (to pick the owning project's config).
    pub file: String,
    /// The Spring bean id under the caret in a config XML (`<action class="beanId">`).
    pub name: String,
}

/// Resolve a Spring **bean id** (as written in a struts `<action class="…">` or a spring
/// `<… ref>`) to its implementation class FQCN — for go-to on a config XML. The FE then opens
/// that class from the class index. `None` when no project owns the file, its config isn't
/// built, or the id names no known bean (the FE falls back to treating the value as an FQCN).
#[arbor_rpc::handler]
fn bennu_bean_class(_ctx: &BennuState, args: BeanClassArgs) -> Result<Option<String>, String> {
    Ok(IndexService::global().bean_class(&args.file, &args.name))
}

/// Args for [`bennu_mapper_definition`].
#[derive(Deserialize)]
pub struct MapperDefinitionArgs {
    /// Absolute path to a file inside the project (to pick the owning project's config).
    pub file: String,
    /// The mapper interface FQCN whose method is being resolved (`com.x.FooMapper`).
    pub interface_fqcn: String,
    /// The invoked method name → the `<select|…>` statement `id` (`findById`).
    pub method: String,
}

/// A resolved go-to-definition target for a MyBatis mapper method.
#[derive(Serialize)]
pub struct MapperDefinitionResult {
    /// The mapper XML the `<select|…>` statement is declared in.
    pub config_file: String,
    /// Byte offset of the statement's `id` attribute value (the go-to target).
    pub offset: usize,
    /// The statement kind (`select` / `insert` / `update` / `delete`).
    pub kind: String,
}

/// Resolve a MyBatis mapper interface method (interface FQCN + method name) to its
/// `<select|insert|update|delete id=…>` statement in the mapper XML — go-to from a Java
/// call site to the SQL. `None`-shaped empty result when no project owns the file, the
/// config isn't built yet, or the interface has no such statement.
#[arbor_rpc::handler]
fn bennu_mapper_definition(
    _ctx: &BennuState,
    args: MapperDefinitionArgs,
) -> Result<Option<MapperDefinitionResult>, String> {
    Ok(IndexService::global()
        .definition_mapper(&args.file, &args.interface_fqcn, &args.method)
        .map(|d| MapperDefinitionResult {
            config_file: d.config_file,
            offset: d.offset,
            kind: d.kind,
        }))
}

/// Find-usages for a Struts action: every JSP `action="…"` reference to it across the
/// project (absolute qnames only). Empty when no project owns the file. Reuses
/// [`DefinitionArgs`] (`file` + `action`).
#[arbor_rpc::handler]
fn bennu_action_usages(_ctx: &BennuState, args: DefinitionArgs) -> Result<UsagesResult, String> {
    let usages = IndexService::global().action_usages(&args.file, &args.action);
    Ok(UsagesResult { target_label: format!("action {}", args.action), usages })
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
    // A server-backed file syncs to its server instead of into the Java index. Not "as well
    // as": the Java extractor would parse Rust as Java and patch nonsense into the symbol
    // index, which then surfaces as phantom completions in real Java files.
    if let Some(handled) = crate::lsp_route::did_change(&args.file, args.text.as_deref()) {
        return Ok(handled);
    }
    IndexService::global().patch_file(&args.file, args.text.as_deref());
    Ok(true)
}
