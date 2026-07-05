//! The [`IntelProvider`] trait + its two impl slots.
//!
//! One protocol for every language (docs §2). The Phase-0 skeleton defines the trait
//! and both impls' *shapes*; the bodies are stubs that return empty / unimplemented,
//! so `bennu-be` can wire the seam now and later waves fill the native engine in
//! (and, post-MVP, the LSP client).

use std::path::{Path, PathBuf};

use bennu_index::prelude::SymbolKind;
use bennu_proto::prelude::{CompletionItem, Diagnostic};

use bennu_query::prelude::{IndexResolver, JdkMemberIndex};

/// One project member (method / field) enumerated from the built symbol index, for the
/// index inspector's "members" list. A be-agnostic view: the be layer maps this onto its
/// wire `IndexEntry` (and resolves the declaring type's line off its own class cache).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMember {
    /// The member's simple name (`getOrder`, `count`).
    pub name: String,
    /// The owning type's binary name (slash form, `com/acme/Order`).
    pub owner_binary: String,
    /// The rendered member signature (`Order getOrder(long id)` / `int count`).
    pub signature: String,
    /// Absolute path (forward slashes) of the project source file declaring the member,
    /// or empty when the member carries no source location.
    pub file: String,
    /// `true` for a method, `false` for a field.
    pub is_method: bool,
}

/// A location in a file — byte offset, matching the wire diagnostics (docs §3: byte
/// ranges, the FE maps them). Used by definition / references results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// Absolute path to the file.
    pub file: String,
    /// Start byte offset.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

/// A document symbol for the outline (docs §5 #16 outline / #9 "everywhere").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSymbol {
    pub name: String,
    /// Kind tag (`"class"` / `"method"` / `"field"` / …).
    pub kind: String,
    pub location: Location,
}

/// A file edit for rename / format results: replace `[start, end)` with `new_text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub new_text: String,
}

/// Errors a provider can return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntelError {
    /// The capability isn't implemented by this provider (the predisposed LSP slot,
    /// and any Phase-0 stub method that isn't a benign empty answer).
    Unimplemented(&'static str),
    /// A provider-specific failure (index miss, transport error).
    Provider(String),
}

impl std::fmt::Display for IntelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntelError::Unimplemented(what) => write!(f, "intel: {what} not implemented"),
            IntelError::Provider(e) => write!(f, "intel: {e}"),
        }
    }
}

impl std::error::Error for IntelError {}

/// A request position: a file + a byte offset into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub file: String,
    pub offset: usize,
}

/// The single code-intel protocol the FE speaks, for every language (docs §2). Java
/// binds [`NativeJavaProvider`]; Rust will bind [`LspClientProvider`].
///
/// The full capability set (completion / hover / definition / references /
/// diagnostics / rename / format / symbols) is declared now so the seam is complete;
/// Phase-0 impls stub the semantic ones and return empty for the list-shaped ones.
pub trait IntelProvider: Send + Sync {
    /// Completion candidates at a position (docs §5 #4). `source` is the live editor buffer when the
    /// caller has it — the caret `offset` is in ITS coordinates (the just-typed, unsaved `.` lives
    /// only there), so completion MUST parse that text, not the stale on-disk file. `None` falls back
    /// to reading the file from disk (a programmatic query with no buffer). Phase-0 native impl → `[]`.
    fn completion(
        &self,
        at: &Position,
        source: Option<&str>,
    ) -> Result<Vec<CompletionItem>, IntelError>;

    /// Hover documentation / type at a position.
    fn hover(&self, at: &Position) -> Result<Option<String>, IntelError>;

    /// Go-to-definition target(s) (docs §5 #8).
    fn definition(&self, at: &Position) -> Result<Vec<Location>, IntelError>;

    /// Find-usages / references (docs §5 #7).
    fn references(&self, at: &Position) -> Result<Vec<Location>, IntelError>;

    /// Diagnostics for a file (docs §5 #2). Phase-0 native impl → `[]`.
    fn diagnostics(&self, file: &str) -> Result<Vec<Diagnostic>, IntelError>;

    /// Rename the symbol at a position to `new_name`, returning the edits (docs §5
    /// #10–12). Domain-aware for Java (also the `class="…"` in struts.xml — docs §5).
    fn rename(&self, at: &Position, new_name: &str) -> Result<Vec<TextEdit>, IntelError>;

    /// Format a whole file, returning the edits (docs §5 #20).
    fn format(&self, file: &str) -> Result<Vec<TextEdit>, IntelError>;

    /// The document symbols of a file, for the outline (docs §5 #16).
    fn symbols(&self, file: &str) -> Result<Vec<DocumentSymbol>, IntelError>;
}

/// The MVP provider: native, index-backed Java intel.
///
/// Phase 1 implements **member-access completion** end to end: it holds a
/// [`IndexResolver`] over the built project index + the JDK member index, infers the
/// receiver type at the caret (`bennu-java`), walks its members (superclass +
/// interfaces), and prefix-filters. Hover / definition / references / rename / format
/// stay stubbed until later waves.
///
/// A provider with **no resolver** (constructed via [`new`](Self::new), e.g. before a
/// project is opened / while the index is still building) answers completion with the
/// benign empty list — never an error — so the FE degrades gracefully.
#[derive(Default)]
pub struct NativeJavaProvider {
    /// The completion resolver: `Some` once a project index is built + the JDK is
    /// resolved; `None` for the empty (pre-index) provider.
    resolver: Option<IndexResolver<JdkMemberIndex>>,
}

impl std::fmt::Debug for NativeJavaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeJavaProvider")
            .field("has_resolver", &self.resolver.is_some())
            .finish()
    }
}

impl NativeJavaProvider {
    /// Construct the empty native provider (no index yet). Completion returns `[]`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a provider backed by a resolver over a built project index + the JDK
    /// member index — the Phase-1 completion path.
    pub fn with_resolver(resolver: IndexResolver<JdkMemberIndex>) -> Self {
        Self { resolver: Some(resolver) }
    }

    /// Build a provider for a project: open the persisted index at `index_dir`, resolve
    /// the JDK for `jdk_version` (`"1.8"` / `"8"` / `"21"` / …), and seed the project's
    /// own declared simple names. `Err` when the index can't be opened or the JDK isn't
    /// installed — the caller then serves the empty provider.
    ///
    /// `jdk_index_path` (when `Some`) makes the JDK member index **persistent**: it loads the
    /// shared, cross-session memo from that path and writes warmed classes back, so a JDK class is
    /// parsed from bytecode at most once ever. The be layer keys the path by the resolved JDK.
    pub fn for_project(
        index_dir: &Path,
        jdk_version: &str,
        project_simple_names: &[(String, String)],
        jdk_index_path: Option<PathBuf>,
    ) -> Result<Self, String> {
        use bennu_classpath::prelude::resolve_jdk_classpath;
        use bennu_index::prelude::PersistedIndex;

        let blob = index_dir.join("symbols.blob");
        let fst = index_dir.join("names.fst");
        let project = PersistedIndex::open(&blob, &fst).map_err(|e| e.to_string())?;
        let source = resolve_jdk_classpath(jdk_version)?;
        let jdk = match jdk_index_path {
            Some(path) => JdkMemberIndex::persistent(source, path),
            None => JdkMemberIndex::new(source),
        };
        let mut resolver = IndexResolver::new(project, jdk);
        for (simple, binary) in project_simple_names {
            resolver.add_simple_hint(simple, binary);
        }
        Ok(Self::with_resolver(resolver))
    }

    /// Persist the JDK member index's memo now (best-effort; no-op for the empty provider or an
    /// in-memory index). Called at a checkpoint so a session's warmed JDK classes survive.
    pub fn flush_jdk_index(&self) {
        if let Some(resolver) = &self.resolver {
            resolver.jdk_index().flush();
        }
    }

    /// Validate a Java `source` (AST checks always; the resolver-backed unknown-member check when a
    /// resolver is built + a JDK is available). `ctx` carries the file location + target Java version
    /// the be layer computed. Runs against THIS provider's own resolver.
    pub fn validate(
        &self,
        source: &str,
        ctx: &bennu_check::prelude::FileContext,
        jdk_available: bool,
    ) -> Vec<Diagnostic> {
        match &self.resolver {
            Some(resolver) => bennu_check::prelude::check_file_resolved(source, ctx, resolver, jdk_available),
            None => bennu_check::prelude::check_file(source, ctx),
        }
    }

    /// Validate `source` while RECORDING the project types the validation reads — the fingerprint
    /// inputs the incremental diagnostic cache stores. Returns the diagnostics paired with the
    /// recorded dependencies. On the empty (pre-index) provider it runs the pure-AST checks and
    /// records nothing (the caller then skips caching, since there's no resolver to check
    /// freshness against).
    pub fn validate_recording(
        &self,
        source: &str,
        ctx: &bennu_check::prelude::FileContext,
        jdk_available: bool,
    ) -> (Vec<Diagnostic>, bennu_query::prelude::RecordedDeps) {
        match &self.resolver {
            Some(resolver) => bennu_query::prelude::record(|| {
                bennu_check::prelude::check_file_resolved(source, ctx, resolver, jdk_available)
            }),
            None => (
                bennu_check::prelude::check_file(source, ctx),
                bennu_query::prelude::RecordedDeps::default(),
            ),
        }
    }

    /// The read-only project view for the diagnostic cache's freshness check, or `None` on the
    /// empty (pre-index) provider (the caller then can't cache — it just validates fresh).
    pub fn project_view(&self) -> Option<&(dyn bennu_query::prelude::ProjectView + '_)> {
        self.resolver
            .as_ref()
            .map(|r| r as &dyn bennu_query::prelude::ProjectView)
    }

    /// Apply one edited `file`'s freshly-extracted [`Symbol`](bennu_index::prelude::Symbol)
    /// records to the resolver's **in-memory overlay** — no disk write, no JDK re-resolve,
    /// no new provider. Completion on the edited file reflects the edit immediately while
    /// the memory-mapped index files stay untouched (they're only rewritten on a full
    /// build, which swaps in a brand-new provider). A no-op on the empty (pre-index)
    /// provider. The overlay tracks each file's prior contributions internally (keyed by
    /// `file`), so a rename/remove drops the stale entries; an empty `records` (a deleted
    /// file) just clears the file's overlay.
    pub fn apply_file_patch(&self, file: &str, records: &[bennu_index::prelude::Symbol]) {
        if let Some(resolver) = &self.resolver {
            resolver.apply_file_patch(file, records);
        }
    }

    /// Enumerate the project's members (methods + fields) from the built index, for the
    /// index inspector's members list. A read-only view of the persisted index (the
    /// analyzer owns how a member symbol maps to a [`ProjectMember`]). An empty vec on the
    /// pre-index (empty) provider — the FE shows the "building" state.
    pub fn project_members(&self) -> Vec<ProjectMember> {
        let Some(resolver) = &self.resolver else {
            return Vec::new();
        };
        resolver
            .member_symbols()
            .into_iter()
            .map(|s| ProjectMember {
                name: s.simple_name,
                owner_binary: s.fqn,
                signature: s.signature,
                file: s.loc_file,
                is_method: matches!(s.kind, SymbolKind::Method),
            })
            .collect()
    }
}

impl IntelProvider for NativeJavaProvider {
    fn completion(
        &self,
        at: &Position,
        source: Option<&str>,
    ) -> Result<Vec<CompletionItem>, IntelError> {
        // No index yet (pre-open / still building) → benign empty, not an error.
        let Some(resolver) = &self.resolver else {
            return Ok(Vec::new());
        };
        // Prefer the live buffer the caller hands in: the caret `offset` is in the editor's
        // coordinates, and the `.` the user just typed to trigger completion is unsaved — it exists
        // ONLY in that buffer. Parsing the on-disk file at a live offset would land mid-token and the
        // receiver before the dot would never be found (empty completions after every edit). Fall
        // back to a tolerant disk read (UTF-8-first, recovering via Windows-1252) only when no buffer
        // is supplied — a programmatic query with nothing open.
        let disk;
        let text: &str = match source {
            Some(src) => src,
            None => {
                let Some(decoded) = crate::java_index::read_source_for_index(
                    std::path::Path::new(&at.file),
                    "UTF-8",
                ) else {
                    return Ok(Vec::new());
                };
                disk = decoded.text;
                &disk
            }
        };
        Ok(bennu_query::prelude::completion(text, at.offset, resolver))
    }

    fn hover(&self, _at: &Position) -> Result<Option<String>, IntelError> {
        Ok(None)
    }

    fn definition(&self, _at: &Position) -> Result<Vec<Location>, IntelError> {
        Ok(Vec::new())
    }

    fn references(&self, _at: &Position) -> Result<Vec<Location>, IntelError> {
        Ok(Vec::new())
    }

    fn diagnostics(&self, _file: &str) -> Result<Vec<Diagnostic>, IntelError> {
        // Phase-0: syntactic diagnostics land with tree-sitter in a later wave.
        Ok(Vec::new())
    }

    fn rename(&self, _at: &Position, _new_name: &str) -> Result<Vec<TextEdit>, IntelError> {
        Err(IntelError::Unimplemented("rename"))
    }

    fn format(&self, _file: &str) -> Result<Vec<TextEdit>, IntelError> {
        Err(IntelError::Unimplemented("format"))
    }

    fn symbols(&self, _file: &str) -> Result<Vec<DocumentSymbol>, IntelError> {
        Ok(Vec::new())
    }
}

/// The **predisposed** LSP-client provider (rust-analyzer, post-MVP — docs §2/§4).
/// Present so the seam is complete: the FE speaks the same protocol, and a language
/// bound to this provider forwards to an external LSP server. **Not implemented in
/// the MVP** — every method returns [`IntelError::Unimplemented`]. Wiring the LSP
/// transport later is a fill-in of these bodies, not a new shape (docs §2: "this is
/// the prestabilisci-LSP").
#[derive(Debug, Default)]
pub struct LspClientProvider {
    // Phase (post-MVP) holds the LSP server handle / transport here. Empty for now.
    _private: (),
}

impl LspClientProvider {
    /// Construct the (unimplemented) LSP-client provider slot.
    pub fn new() -> Self {
        Self::default()
    }
}

impl IntelProvider for LspClientProvider {
    fn completion(
        &self,
        _at: &Position,
        _source: Option<&str>,
    ) -> Result<Vec<CompletionItem>, IntelError> {
        Err(IntelError::Unimplemented("lsp completion"))
    }

    fn hover(&self, _at: &Position) -> Result<Option<String>, IntelError> {
        Err(IntelError::Unimplemented("lsp hover"))
    }

    fn definition(&self, _at: &Position) -> Result<Vec<Location>, IntelError> {
        Err(IntelError::Unimplemented("lsp definition"))
    }

    fn references(&self, _at: &Position) -> Result<Vec<Location>, IntelError> {
        Err(IntelError::Unimplemented("lsp references"))
    }

    fn diagnostics(&self, _file: &str) -> Result<Vec<Diagnostic>, IntelError> {
        Err(IntelError::Unimplemented("lsp diagnostics"))
    }

    fn rename(&self, _at: &Position, _new_name: &str) -> Result<Vec<TextEdit>, IntelError> {
        Err(IntelError::Unimplemented("lsp rename"))
    }

    fn format(&self, _file: &str) -> Result<Vec<TextEdit>, IntelError> {
        Err(IntelError::Unimplemented("lsp format"))
    }

    fn symbols(&self, _file: &str) -> Result<Vec<DocumentSymbol>, IntelError> {
        Err(IntelError::Unimplemented("lsp symbols"))
    }
}
