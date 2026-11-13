//! Which engine answers for a file — decided **once**, from the file, and then asked alone.
//!
//! Bennu has several engines that understand code: a language server (any language, as configured), the Java
//! index, and three of its own for files no server covers — the Cargo manifest, WGSL shaders and Patina mods (the
//! last through an extension package), plus a RON validator. Each question the editor or Imber asks (hover, go to
//! declaration, references, implementations, rename, completion, diagnostics, format) goes to **the one** engine
//! [`engine_for`] names; nothing is tried and nothing falls through.
//!
//! ## Why one decision and not a chain
//!
//! The handlers used to ask each engine in turn, each answering "not mine" for files it did not own — a decision
//! spread over every engine and repeated in every handler, so a handler with a shorter copy of the chain was blind
//! to an engine without anyone noticing: Imber's tools never saw Patina. And when nothing answered, nobody knew who
//! should have, so the sentence the model read had to guess. Here the decision is one function, and a file no
//! engine serves comes with the precise reason ([`Engine::Unserved`]).
//!
//! ## How the engine is chosen
//!
//! By the file, in a fixed precedence:
//!
//! 1. a **language server** whose extension it has, with its project above the file (its root marker, and for
//!    Svelte and Angular the npm dependency) and its binary on this machine — see `LspRegistry::claim_file`;
//! 2. a `Cargo.toml` → the manifest engine;
//! 3. a `.wgsl` → the shader engine (what a shader gets when `wgsl-analyzer` is not there: the server wins);
//! 4. a `.ptn` → the Patina package;
//! 5. a `.ron` → the RON validator (diagnostics only);
//! 6. a file the Java index reads (`.java`, JSP, XML, properties…) → the Java index;
//! 7. otherwise nothing, with the reason.
//!
//! An engine that has no answer to a question (the RON validator has no hover, the shader engine has no rename) answers
//! empty — never "ask the next one".
//!
//! ## What each engine can answer
//!
//! Not every engine answers every question, and not every language server either (a server says what it supports
//! in its handshake). [`Engine::supports`] is the table for Bennu's own engines, [`can_answer`] adds what a running
//! server declared, and [`answers`] is the list a project summary shows per language. Asked lazily: when a tool is
//! called or a project is summarised, never in the standing instructions. A question an engine cannot answer is
//! refused with the reason and what it *can* answer, instead of an empty result that reads as "nothing there".

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{CompletionItem, DeclarationTarget, HierarchyNode, HoverInfo, LspSymbol, RenameEdit, RenamePreview, SourceEdit, UsagesResult};

use bennu_lsp::prelude::ServerFeatures;

use crate::lsp_registry::{LspClaim, LspRegistry};

/// A question about code that an engine may or may not be able to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    Hover,
    Declaration,
    References,
    Implementations,
    Rename,
    Outline,
    Callers,
    Completion,
    Diagnostics,
    Format,
}

impl Question {
    pub const ALL: [Question; 10] = [
        Question::Hover,
        Question::Declaration,
        Question::References,
        Question::Implementations,
        Question::Rename,
        Question::Outline,
        Question::Callers,
        Question::Completion,
        Question::Diagnostics,
        Question::Format,
    ];

    /// What the question is, as a sentence names it.
    pub fn label(self) -> &'static str {
        match self {
            Question::Hover => "what a symbol is",
            Question::Declaration => "where a symbol is declared",
            Question::References => "references",
            Question::Implementations => "implementations",
            Question::Rename => "renames",
            Question::Outline => "a file's outline",
            Question::Callers => "callers",
            Question::Completion => "completion",
            Question::Diagnostics => "problems",
            Question::Format => "formatting",
        }
    }

    /// The tool of Imber's that asks it, when there is one.
    pub fn tool(self) -> Option<&'static str> {
        match self {
            Question::Hover | Question::Declaration => Some("bennu_symbol_at"),
            Question::References => Some("bennu_references"),
            Question::Implementations => Some("bennu_implementors"),
            Question::Rename => Some("bennu_rename_plan"),
            Question::Outline => Some("bennu_outline"),
            Question::Callers => Some("bennu_callers"),
            Question::Diagnostics => Some("bennu_check_file"),
            Question::Completion | Question::Format => None,
        }
    }
}

/// Whether a running server's handshake covers `q`. Problems are published, not requested: every server has them.
fn server_answers(f: &ServerFeatures, q: Question) -> bool {
    match q {
        Question::Hover => f.hover,
        Question::Declaration => f.definition,
        Question::References => f.references,
        Question::Implementations => f.implementations,
        Question::Rename => f.rename,
        Question::Outline => f.outline,
        Question::Callers => f.call_hierarchy,
        Question::Completion => f.completion,
        Question::Diagnostics => true,
        Question::Format => f.formatting,
    }
}

/// The questions an engine answers; for a language server, what it declared once running (`features`), or every
/// question a server may answer while it has not started — it will say, then.
pub fn answers(engine: &Engine, features: Option<ServerFeatures>) -> Vec<Question> {
    Question::ALL
        .into_iter()
        .filter(|q| match (engine, features) {
            (Engine::Lsp, Some(f)) => server_answers(&f, *q),
            _ => engine.supports(*q),
        })
        .collect()
}

/// Whether the engine serving `file` can answer `q`. `Err` is the sentence for the model: why not, and what it can
/// answer instead — so a missing feature is never read as "nothing there".
pub fn can_answer(file: &str, q: Question) -> Result<(), String> {
    let engine = engine_for(file);
    if let Engine::Unserved(why) = &engine {
        return Err(format!("nothing can answer {} here: {why}", q.label()));
    }
    let features = match engine {
        Engine::Lsp => LspRegistry::global().features_for(file),
        _ => None,
    };
    let able = answers(&engine, features);
    if able.contains(&q) {
        return Ok(());
    }
    let instead: Vec<String> = able
        .iter()
        .filter_map(|other| other.tool().map(|tool| format!("{} ({tool})", other.label())))
        .collect();
    let who = match (&engine, std::path::Path::new(file).extension().and_then(|e| e.to_str())) {
        (Engine::Lsp, Some(ext)) => format!("the language server for .{ext} files"),
        (other, _) => other.label().to_string(),
    };
    Err(match instead.is_empty() {
        true => format!("{who} does not answer {} — this is not a statement about the code", q.label()),
        false => format!("{who} does not answer {} — this is not a statement about the code. It answers {}", q.label(), instead.join(", ")),
    })
}

/// The engine that answers for a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Engine {
    /// A language server.
    Lsp,
    /// A `Cargo.toml`: its schema and this machine's crate catalogue.
    CargoManifest,
    /// A WGSL shader, with no server for it.
    Wgsl,
    /// A Patina mod script, through the extension package that serves `.ptn`.
    Patina,
    /// A RON file: its syntax, and the Rust type its directive names.
    Ron,
    /// The Java index: Java, JSP, and the XML and properties of the frameworks it models.
    Java,
    /// Nothing serves it; the sentence says why.
    Unserved(String),
}

impl Engine {
    /// The engine, as a sentence names it.
    pub fn label(&self) -> &'static str {
        match self {
            Engine::Lsp => "the language server",
            Engine::CargoManifest => "the Cargo manifest engine",
            Engine::Wgsl => "the shader engine",
            Engine::Patina => "the Patina package",
            Engine::Ron => "the RON validator",
            Engine::Java => "the Java index",
            Engine::Unserved(_) => "no engine",
        }
    }

    /// Whether this engine can answer `q` at all. A language server is decided by its own handshake (see
    /// [`can_answer`]), so here it is "yes, if the server says so".
    pub fn supports(&self, q: Question) -> bool {
        use Question::*;
        match self {
            Engine::Lsp => true,
            Engine::Java => true,
            // Callers need a package with `incoming-calls`; an older one answers with no callers.
            Engine::Patina => matches!(q, Hover | Declaration | References | Implementations | Rename | Outline | Callers | Completion | Diagnostics | Format),
            Engine::Wgsl => matches!(q, Hover | Declaration | References | Completion | Diagnostics),
            Engine::CargoManifest => matches!(q, Completion | Diagnostics),
            Engine::Ron => q == Diagnostics,
            Engine::Unserved(_) => false,
        }
    }

    /// Why nothing answers, when nothing serves the file.
    pub fn unserved_reason(&self) -> Option<&str> {
        match self {
            Engine::Unserved(why) => Some(why),
            _ => None,
        }
    }
}

/// The engine for `file`; see the module doc for the precedence.
pub fn engine_for(file: &str) -> Engine {
    let claim = LspRegistry::global().claim_file(file);
    if claim == LspClaim::Served {
        return Engine::Lsp;
    }
    if crate::cargo_intel::owns(file) {
        return Engine::CargoManifest;
    }
    if crate::wgsl_intel::is_wgsl(file) {
        return Engine::Wgsl;
    }
    if crate::patina_intel::is_patina(file) {
        return Engine::Patina;
    }
    if crate::ron_intel::is_ron(file) {
        return Engine::Ron;
    }
    if crate::index_service::understands(file) {
        return Engine::Java;
    }
    Engine::Unserved(unserved_reason(file, &claim))
}

/// The sentence for a file nothing serves, from what the language-server registry found.
fn unserved_reason(file: &str, claim: &LspClaim) -> String {
    let ext = std::path::Path::new(file).extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_else(|| "this kind of".into());
    match claim {
        LspClaim::NotInstalled { server, hint } if hint.is_empty() => {
            format!("{ext} files are served by the {server} language server, which is not installed on this machine")
        }
        LspClaim::NotInstalled { server, hint } => {
            format!("{ext} files are served by the {server} language server, which is not installed on this machine ({hint})")
        }
        LspClaim::NoProject { server, markers } => format!(
            "{ext} files are served by the {server} language server only inside its kind of project, and there is none above this file (it looks for {})",
            markers.join(", ")
        ),
        LspClaim::Off => format!("language servers are switched off (Settings › Language Servers), and no engine of Bennu's own reads {ext} files"),
        LspClaim::NoServer | LspClaim::Served => {
            format!("no engine serves {ext} files: no language server is configured for them (Settings › Language Servers can add one)")
        }
    }
}

// ── The questions ────────────────────────────────────────────────────────────────────────────────
//
// Each goes to the engine `engine_for` names. The Java branch is a closure where the Java side needs what only
// the caller has (a library view's origin file, a mapper statement): the decision stays here.

/// The hover card at `offset`.
pub fn hover(ctx: &BennuState, file: &str, source: &str, offset: usize) -> Option<HoverInfo> {
    match engine_for(file) {
        Engine::Lsp => crate::lsp_route::hover(file, source, offset).flatten(),
        Engine::Wgsl => crate::wgsl_intel::hover(file, source, offset).flatten(),
        Engine::Patina => crate::patina_intel::hover(ctx, file, source, offset).flatten(),
        Engine::Java => crate::index_service::IndexService::global().hover(file, source, offset),
        Engine::CargoManifest | Engine::Ron | Engine::Unserved(_) => None,
    }
}

/// Where the symbol at `offset` is declared.
pub fn declaration(ctx: &BennuState, file: &str, source: &str, offset: usize) -> Option<DeclarationTarget> {
    match engine_for(file) {
        Engine::Lsp => crate::lsp_route::declaration(file, source, offset).flatten(),
        Engine::Wgsl => crate::wgsl_intel::declaration(file, source, offset).flatten(),
        Engine::Patina => crate::patina_intel::declaration(ctx, file, source, offset).flatten(),
        Engine::Java => crate::index_service::IndexService::global().declaration(file, source, offset),
        Engine::CargoManifest | Engine::Ron | Engine::Unserved(_) => None,
    }
}

/// Every use of the symbol at `offset`. `java` answers for a Java file.
pub fn references(ctx: &BennuState, file: &str, source: &str, offset: usize, java: impl FnOnce() -> Option<UsagesResult>) -> Option<UsagesResult> {
    match engine_for(file) {
        Engine::Lsp => crate::lsp_route::references(file, source, offset).flatten(),
        Engine::Wgsl => crate::wgsl_intel::references(file, source, offset).flatten(),
        Engine::Patina => crate::patina_intel::references(ctx, file, source, offset).flatten(),
        Engine::Java => java(),
        Engine::CargoManifest | Engine::Ron | Engine::Unserved(_) => None,
    }
}

/// What implements the type or method at `offset`. `java` answers for a Java file.
pub fn implementations(ctx: &BennuState, file: &str, source: &str, offset: usize, java: impl FnOnce() -> Option<UsagesResult>) -> Option<UsagesResult> {
    match engine_for(file) {
        Engine::Lsp => crate::lsp_route::implementations(file, source, offset),
        Engine::Patina => crate::patina_intel::implementations(ctx, file, source, offset).flatten(),
        // Only Java types and methods have implementations in the index.
        Engine::Java if crate::intel::is_java_file(file) => java(),
        Engine::Java => None,
        Engine::Wgsl | Engine::CargoManifest | Engine::Ron | Engine::Unserved(_) => None,
    }
}

/// What renaming the symbol at `offset` would change, for an engine other than Java — whose plan carries a file
/// move and a classification the caller shapes itself. `Err(())` means the file is Java's.
pub fn rename_plan(ctx: &BennuState, file: &str, source: &str, offset: usize, new_name: &str) -> Result<Option<RenamePreview>, ()> {
    match engine_for(file) {
        Engine::Lsp => Ok(crate::lsp_route::rename_plan(file, source, offset, new_name).flatten()),
        Engine::Patina => Ok(crate::patina_intel::rename_plan(ctx, file, source, offset, new_name).flatten()),
        Engine::Java => Err(()),
        Engine::Wgsl | Engine::CargoManifest | Engine::Ron | Engine::Unserved(_) => Ok(None),
    }
}

/// The edits of renaming the symbol at `offset`, for an engine other than Java (see [`rename_plan`]).
pub fn rename_apply(ctx: &BennuState, file: &str, source: &str, offset: usize, new_name: &str) -> Result<Vec<RenameEdit>, ()> {
    match engine_for(file) {
        Engine::Lsp => Ok(crate::lsp_route::rename_apply(file, source, offset, new_name).unwrap_or_default()),
        Engine::Patina => Ok(crate::patina_intel::rename_apply(ctx, file, source, offset, new_name).unwrap_or_default()),
        Engine::Java => Err(()),
        Engine::Wgsl | Engine::CargoManifest | Engine::Ron | Engine::Unserved(_) => Ok(Vec::new()),
    }
}

/// Completion at `offset`. A language server needs the live buffer: without one it has nothing to answer.
/// `java` answers for a Java file.
pub fn completion(ctx: &BennuState, file: &str, offset: usize, source: Option<&str>, java: impl FnOnce() -> Vec<CompletionItem>) -> Vec<CompletionItem> {
    match engine_for(file) {
        Engine::Lsp => source.and_then(|s| crate::lsp_route::completion(file, offset, s)).unwrap_or_default(),
        Engine::CargoManifest => crate::cargo_intel::completion(file, offset, source).unwrap_or_default(),
        Engine::Wgsl => crate::wgsl_intel::completion(file, offset, source).unwrap_or_default(),
        Engine::Patina => crate::patina_intel::completion(ctx, file, offset, source).unwrap_or_default(),
        Engine::Java => java(),
        Engine::Ron | Engine::Unserved(_) => Vec::new(),
    }
}

/// The file's declarations as an outline tree: a language server's document symbols, the Java structure (the one the
/// Structure panel shows), or the Patina package's `symbols`.
pub fn document_symbols(ctx: &BennuState, file: &str, source: &str) -> Vec<LspSymbol> {
    match engine_for(file) {
        Engine::Lsp => crate::lsp_route::document_symbols(file, source),
        Engine::Java if crate::intel::is_java_file(file) => crate::java_structure::document_symbols(file, source),
        Engine::Patina => crate::patina_intel::document_symbols(ctx, file, source).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The item at `offset` a hierarchy is built from — a call hierarchy when `calls`, a type hierarchy otherwise — or
/// none. Java's from the index, Patina's from its package (calls only), a server's from its call/type hierarchy.
pub fn prepare_hierarchy(ctx: &BennuState, file: &str, source: &str, offset: usize, calls: bool) -> Vec<HierarchyNode> {
    match engine_for(file) {
        Engine::Java if crate::intel::is_java_file(file) => crate::index_service::IndexService::global()
            .prepare_hierarchy(file, source, offset, calls)
            .into_iter()
            .map(crate::hierarchy::node_of)
            .collect(),
        Engine::Patina if calls => crate::patina_intel::prepare_calls(ctx, file, source, offset).unwrap_or_default(),
        Engine::Lsp => crate::lsp_route::prepare_hierarchy(file, source, offset, calls),
        _ => Vec::new(),
    }
}

/// One level of a hierarchy from the node `handle` names (`incoming`, `outgoing`, `supertypes`, `subtypes`). The
/// handle is the engine's own and is sent back verbatim: a Patina handle says it is Patina's, a Java one is read
/// when `scope` is a Java file, anything else goes to the server covering `scope`.
pub fn hierarchy_step(ctx: &BennuState, scope: &str, handle: serde_json::Value, direction: &str) -> Vec<HierarchyNode> {
    use bennu_intel::prelude::{HierarchyDirection, HierarchyHandle};
    if handle.get("patina").is_some() {
        // The package answers who calls; nothing else is asked of it.
        return match direction {
            "incoming" => crate::patina_intel::incoming_step(ctx, &handle).unwrap_or_default(),
            _ => Vec::new(),
        };
    }
    if crate::intel::is_java_file(scope) {
        // A handle this engine did not issue, or a direction this build has never heard of: nothing, rather than
        // the wrong list under an expanded node.
        let (Some(direction), Ok(handle)) = (HierarchyDirection::from_wire(direction), serde_json::from_value::<HierarchyHandle>(handle)) else {
            return Vec::new();
        };
        return crate::index_service::IndexService::global()
            .hierarchy_step(scope, &handle, direction)
            .into_iter()
            .map(crate::hierarchy::node_of)
            .collect();
    }
    crate::lsp_route::hierarchy_step(scope, handle, direction)
}

/// Formatting edits for an engine other than Java, whose formatter takes the project's style and a range and is
/// called by the handler. `None` means the file is Java's; `Some(Err)` is the engine's own error (a Patina syntax
/// error, a bad config file), for the editor to say.
pub fn format(ctx: &BennuState, file: &str, source: &str, range: Option<(usize, usize)>, tab_size: u32, spaces: bool) -> Option<Result<Vec<SourceEdit>, String>> {
    match engine_for(file) {
        Engine::Lsp => Some(Ok(crate::lsp_route::format(file, source, tab_size, spaces))),
        Engine::Patina => crate::patina_intel::format(ctx, file, source, range),
        // The Java formatter formats Java; a JSP or an XML the index reads has no formatter here.
        Engine::Java if crate::intel::is_java_file(file) => None,
        Engine::Java => Some(Ok(Vec::new())),
        Engine::Wgsl | Engine::CargoManifest | Engine::Ron | Engine::Unserved(_) => Some(Ok(Vec::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_of_bennus_own_engines_goes_to_that_engine_by_its_name() {
        assert_eq!(engine_for("/p/Cargo.toml"), Engine::CargoManifest);
        assert_eq!(engine_for("/p/mods/a/script/main.ptn"), Engine::Patina);
        assert_eq!(engine_for("/p/assets/level.ron"), Engine::Ron);
        assert_eq!(engine_for("/p/src/main/java/A.java"), Engine::Java);
        assert_eq!(engine_for("/p/web/index.jsp"), Engine::Java);
    }

    #[test]
    fn an_engine_that_cannot_answer_a_question_says_so_and_what_it_answers_instead() {
        let why = can_answer("/p/assets/level.ron", Question::Hover).unwrap_err();
        assert!(why.contains("RON validator") && why.contains("problems (bennu_check_file)"), "{why}");
        assert!(can_answer("/p/mods/a/script/main.ptn", Question::Rename).is_ok());
        assert!(can_answer("/p/mods/a/script/main.ptn", Question::Outline).is_ok());
        assert!(can_answer("/p/mods/a/script/main.ptn", Question::Callers).is_ok());
        assert!(can_answer("/p/mods/a/script/main.ptn", Question::Completion).is_ok());
        assert!(can_answer("/p/src/A.java", Question::Outline).is_ok());
        assert!(can_answer("/p/src/A.java", Question::Implementations).is_ok());
    }

    #[test]
    fn a_running_server_is_held_to_what_it_declared() {
        let lua = ServerFeatures { hover: true, definition: true, references: true, completion: true, ..Default::default() };
        let able = answers(&Engine::Lsp, Some(lua));
        assert!(able.contains(&Question::References) && able.contains(&Question::Diagnostics));
        assert!(!able.contains(&Question::Implementations) && !able.contains(&Question::Callers));
        assert_eq!(answers(&Engine::Lsp, None).len(), Question::ALL.len(), "not started yet: it will say");
    }

    #[test]
    fn a_file_nothing_serves_says_why() {
        let e = engine_for("/nowhere/notes.zzq");
        assert!(e.unserved_reason().is_some_and(|w| w.contains(".zzq")), "{e:?}");
    }
}
