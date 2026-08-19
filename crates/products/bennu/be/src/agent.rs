//! `agent` domain — the handlers that exist for an AI client rather than for the editor.
//!
//! The editor opens a project once and then asks four more questions about it, because
//! it has a UI to fill and a state store to hold the answers. A model arrives cold on
//! every session and has neither. Its first question is always the same one — *what is
//! this project* — and answering it in four round trips spends context on plumbing.
//!
//! So this module is the small set of verbs whose shape is different for an agent, not
//! whose logic is. Everything here delegates; nothing here re-implements.

use std::collections::HashMap;
use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{DeclarationTarget, HoverInfo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for [`bennu_project_summary`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectSummaryArgs {
    /// Absolute path to the project root — the directory holding `pom.xml` or
    /// `Cargo.toml`.
    pub root: String,
}

/// Everything worth knowing about a project before asking it anything else.
#[derive(Debug, Serialize)]
pub struct ProjectSummary {
    pub root: String,
    pub name: String,
    /// `maven` or `cargo` — which manifest governs the root, and therefore how much of
    /// the Java model applies.
    pub kind: String,
    /// Sub-projects: Maven modules or Cargo workspace members. Empty for a single one.
    pub modules: Vec<String>,
    /// The resolved JDK version, when there is one (never for a Cargo root).
    pub jdk: Option<String>,
    /// The encoding the project's sources are declared in. Frequently not UTF-8 on the
    /// legacy stacks this editor exists for.
    pub source_encoding: String,
    /// Frameworks detected in the source: `struts`, `spring`, `jpa`, … Drives which of
    /// the framework-aware tools will have anything to say.
    pub capabilities: Vec<String>,
    /// Indexed type / member counts, and whether the index has finished building.
    pub index: IndexSummary,
    /// What to do next, in words — written for a caller that has just arrived.
    pub next_steps: Vec<String>,
}

/// The index's state, flattened out of the fuller stats the inspector panel uses.
#[derive(Debug, Serialize)]
pub struct IndexSummary {
    pub ready: bool,
    pub types: usize,
    pub members: usize,
    /// Framework config the index found: Struts actions, Spring beans.
    pub actions: usize,
    pub beans: usize,
}

/// Open a Java or Rust project and describe it: build model, modules, JDK, source
/// encoding, detected frameworks, and how far the semantic index has got.
///
/// **Call this first.** It is idempotent — opening an already-open project re-reads the
/// manifest without discarding the index — and it is what makes every other bennu tool
/// work, because they all answer from the index this starts building.
///
/// The index builds in the background. When `index.ready` is false the project is
/// usable but navigation and diagnostics will be incomplete; call again in a few
/// seconds rather than concluding a symbol does not exist.
#[arbor_rpc::handler(mcp(
    title = "Open a project and summarise it",
    safety = read,
))]
fn bennu_project_summary(
    ctx: &BennuState,
    args: ProjectSummaryArgs,
) -> Result<ProjectSummary, String> {
    // The editor's own open path, verbatim: same warm-up, same index build. An agent
    // opening a project must leave it in exactly the state the editor would.
    // Background: this door is reached by an AI client, and between one call and the next there
    // is nothing on screen that a full-fat language server would be serving.
    let info = crate::project::open_and_start(
        ctx,
        &args.root,
        crate::lsp_registry::SessionOrigin::Background,
    )?;
    let stats = IndexService::global().index_stats(&args.root);
    let capabilities = detected_capabilities(&info.capabilities);

    Ok(ProjectSummary {
        root: info.root.clone(),
        name: info.name.clone(),
        kind: format!("{:?}", info.kind).to_lowercase(),
        modules: info.modules.clone(),
        jdk: info.jdk.as_ref().map(|j| j.version.clone()),
        source_encoding: info.source_encoding.clone(),
        next_steps: next_steps(&stats, &capabilities),
        index: IndexSummary {
            ready: stats.ready,
            types: stats.types,
            members: stats.members,
            actions: stats.actions,
            beans: stats.beans,
        },
        capabilities,
    })
}

/// The names of the capabilities that came back `true`.
///
/// Read off the serialized form rather than matched field by field: the capability set
/// grows every time a framework is taught to Bennu, and a hand-written match would
/// quietly stop mentioning the new ones.
fn detected_capabilities(set: &bennu_proto::prelude::CapabilitySet) -> Vec<String> {
    let Ok(serde_json::Value::Object(map)) = serde_json::to_value(set) else {
        return Vec::new();
    };
    map.into_iter()
        .filter(|(_, v)| v.as_bool().unwrap_or(false))
        .map(|(k, _)| k)
        .collect()
}

/// Guidance in the reply rather than in the tool description.
///
/// A description is static and has to cover every project; this sees *this* project and
/// can say the one useful thing — that the index is still warming, or that a Struts
/// config graph exists and is worth asking about.
fn next_steps(stats: &bennu_proto::prelude::IndexStats, capabilities: &[String]) -> Vec<String> {
    let mut steps = Vec::new();
    if !stats.ready {
        steps.push(
            "The semantic index is still building. Navigation and diagnostics will be \
             incomplete until it finishes — re-check with bennu_index_stats."
                .to_string(),
        );
    }
    steps.push(
        "Use bennu_class_index to map a type name to its file, and bennu_read_file to \
         read source with the project's own encoding."
            .to_string(),
    );
    if stats.actions > 0 || capabilities.iter().any(|c| c.contains("struts")) {
        steps.push(format!(
            "This project has a Struts/XWork config graph ({} actions). Its request \
             routing lives in XML, not in annotations.",
            stats.actions
        ));
    }
    if stats.beans > 0 || capabilities.iter().any(|c| c.contains("spring")) {
        steps.push(format!(
            "Spring wiring is present ({} beans). Some of it is declared in XML rather \
             than by annotation.",
            stats.beans
        ));
    }
    steps
}

// ── Addressing ───────────────────────────────────────────────────────────────
//
// Bennu's navigation verbs are addressed by **byte offset**, which is the right unit
// for an editor holding the buffer and the wrong one for anything else. A model has
// never seen the bytes: it has a line and a column, from a compiler message, a stack
// trace, or its own reading of a file. The two verbs below are that translation, and
// they are the whole reason the caret-addressed half of Bennu is reachable at all.

/// Args for [`bennu_symbol_at`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolAtArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Absolute path to the file.
    pub file: String,
    /// 1-based line, counting the way an editor and a stack trace count.
    pub line: u32,
    /// 1-based column, in **characters** — not bytes. Defaults to 1 (the start of the
    /// line), which is usually enough when the line declares a single symbol.
    #[serde(default)]
    pub column: Option<u32>,
}

/// What sits at a position, and where it is declared.
#[derive(Debug, Serialize)]
pub struct SymbolAt {
    /// The byte offset the line/column resolved to — the coordinate the rest of
    /// Bennu's API speaks, should a follow-up call need it.
    pub offset: usize,
    /// The text of the line, so a caller can check it landed where it meant to.
    pub line_text: String,
    /// Signature, kind, owning type and doc — `None` when the position is not on a
    /// symbol the index can classify.
    pub symbol: Option<HoverInfo>,
    /// Where that symbol is declared, when it resolves.
    pub declaration: Option<DeclarationTarget>,
    /// Why there is nothing here, when there isn't. Written for the caller: "not on a
    /// symbol" and "the index has not finished" are different problems with different
    /// responses, and an empty result cannot tell them apart.
    pub note: Option<String>,
}

/// Identify the symbol at a file position and report where it is declared.
///
/// Addressed by line and column — from a stack trace, a compiler error, or your own
/// reading of the file — rather than by byte offset. Use it to answer "what is this"
/// and "where does this come from" in one call.
///
/// Returns the symbol's signature, kind and owning type, plus its declaration site. A
/// position that is not on a resolvable symbol comes back with `symbol: null` and a
/// note saying why, so an empty answer is never ambiguous.
#[arbor_rpc::handler(mcp(
    title = "Identify the symbol at a position",
    safety = read,
))]
fn bennu_symbol_at(_ctx: &BennuState, args: SymbolAtArgs) -> Result<SymbolAt, String> {
    let contents = read_source(&args.root, &args.file)?;
    let column = args.column.unwrap_or(1);
    let (offset, line_text) = offset_of(&contents, args.line, column)?;

    // Same routing the editor's own handlers use: a language-server-backed file is
    // answered by its server, and only a Java one falls through to Bennu's resolver.
    // Skipping that would send a Rust buffer into the Java resolver, which parses
    // anything as Java and answers confidently about the wrong file.
    let symbol = crate::lsp_route::hover(&args.file, &contents, offset)
        .unwrap_or_else(|| IndexService::global().hover(&args.file, &contents, offset));
    let declaration = crate::lsp_route::declaration(&args.file, &contents, offset)
        .unwrap_or_else(|| IndexService::global().declaration(&args.file, &contents, offset));

    let note = match (&symbol, &declaration) {
        (None, None) if !IndexService::global().index_stats(&args.root).ready => Some(
            "The semantic index has not finished building, so nothing can be resolved yet. \
             Check bennu_index_stats and try again."
                .to_string(),
        ),
        (None, None) => Some(format!(
            "Nothing resolvable at line {} column {}. The position may be on whitespace, a \
             keyword, a comment, or a symbol from a dependency with no attached source. The \
             line reads: {}",
            args.line,
            column,
            line_text.trim()
        )),
        _ => None,
    };

    Ok(SymbolAt { offset, line_text, symbol, declaration, note })
}

/// Args for [`bennu_find_symbol`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindSymbolArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// What to look for. Matched case-insensitively against type names (simple and
    /// fully-qualified) and member names.
    pub query: String,
    /// Narrow the search: `type` for classes/interfaces/enums/records, `member` for
    /// methods and fields. Omit for both.
    #[serde(default)]
    pub kind: Option<String>,
    /// Cap on results. Defaults to 50.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// One thing the project declares.
#[derive(Debug, Serialize)]
pub struct SymbolHit {
    /// `type` or `member`.
    pub kind: String,
    /// The name as declared.
    pub name: String,
    /// A type's fully-qualified name, or a member's owning type and signature.
    pub detail: String,
    /// Absolute path of the declaring file, when there is one (a member inherited from
    /// a dependency has none).
    pub file: Option<String>,
    /// 1-based declaration line.
    pub line: Option<i64>,
}

/// Results, with an honest word about what was left out.
#[derive(Debug, Serialize)]
pub struct FindSymbolResult {
    pub hits: Vec<SymbolHit>,
    /// How many matched in total, before the limit.
    pub total: usize,
    /// Present when results were cut, so a truncated list is never mistaken for a
    /// complete one.
    pub note: Option<String>,
}

/// Find where a type or member is declared, by name.
///
/// The way in when you know what something is called but not where it lives — which is
/// the normal state of affairs in a large project, where the package layout rarely
/// matches what you would guess. Matches substrings, case-insensitively, against type
/// names (simple and fully-qualified) and member names.
///
/// Answers from the project's semantic index, so it finds declarations rather than text
/// occurrences: a method named `process` is one hit here and two hundred lines in a grep.
///
/// **Java projects only.** That index is built for a Maven/Java project; a Cargo project does
/// not have one, so this returns nothing there — say so rather than concluding the symbol does
/// not exist. On a Rust project use `bennu_symbol_at` for a position you already have, or
/// `bennu_references` from any occurrence of the name.
#[arbor_rpc::handler(mcp(
    title = "Find a type or member by name",
    safety = read,
))]
fn bennu_find_symbol(_ctx: &BennuState, args: FindSymbolArgs) -> Result<FindSymbolResult, String> {
    let needle = args.query.trim().to_lowercase();
    if needle.is_empty() {
        return Err("bennu_find_symbol needs a non-empty query".into());
    }
    let limit = args.limit.unwrap_or(50).clamp(1, 500);
    let want_types = !matches!(args.kind.as_deref(), Some("member"));
    let want_members = !matches!(args.kind.as_deref(), Some("type"));

    let service = IndexService::global();
    let mut hits = Vec::new();

    if want_types {
        for entry in service.class_index(&args.root).unwrap_or_default() {
            if entry.simple.to_lowercase().contains(&needle) || entry.fqcn.to_lowercase().contains(&needle) {
                hits.push(SymbolHit {
                    kind: "type".to_string(),
                    name: entry.simple,
                    detail: entry.fqcn,
                    file: Some(entry.file),
                    line: Some(entry.line as i64),
                });
            }
        }
    }

    if want_members {
        for entry in service.index_entries(&args.root, "members") {
            if entry.primary.to_lowercase().contains(&needle) {
                hits.push(SymbolHit {
                    kind: "member".to_string(),
                    name: entry.primary,
                    detail: entry.secondary,
                    file: entry.file,
                    line: entry.line,
                });
            }
        }
    }

    // An exact name beats a substring of a longer one: searching `Order` should not
    // bury `Order` under `OrderProcessingServiceFactory`.
    hits.sort_by_key(|h| (h.name.to_lowercase() != needle, h.name.len(), h.name.to_lowercase()));

    let total = hits.len();
    let note = if total > limit {
        Some(format!(
            "Showing {limit} of {total} matches. Narrow the query, or pass a larger limit."
        ))
    } else if total == 0 && is_cargo_root(&args.root) {
        // This search reads Bennu's own index and nothing else, and Bennu builds one for Java
        // projects only. On a Cargo root it is therefore always empty — and the index it was
        // waiting on is one that will never be built, so "check the stats and try again" sent a
        // caller to wait forever for a permanent no.
        Some(
            "This search reads Bennu's own symbol index, which is built for Java projects only — \
             a Cargo project never builds one, so this comes back empty however long you wait. \
             It is not a statement about the project. Use bennu_symbol_at to resolve a position \
             you already have, bennu_references from any occurrence of the name, or search the \
             sources directly."
                .to_string(),
        )
    } else if total == 0 && !service.index_stats(&args.root).ready {
        Some(
            "No matches, but the semantic index has not finished building — this is not yet \
             an answer. Check bennu_index_stats and try again."
                .to_string(),
        )
    } else {
        None
    };
    hits.truncate(limit);

    Ok(FindSymbolResult { hits, total, note })
}

/// Read a file in the project's own encoding.
///
/// Not `fs::read_to_string`: the legacy stacks Bennu exists for are frequently Cp1252,
/// and a caret offset computed over mojibake points at the wrong character.
fn read_source(root: &str, file: &str) -> Result<String, String> {
    let cfg = bennu_core::config::load();
    let override_label = cfg
        .encoding_overrides
        .get(file)
        .or_else(|| cfg.encoding_overrides.get(root))
        .map(|s| s.as_str());
    bennu_project::prelude::read_file(Path::new(root), Path::new(file), &cfg.default_encoding, override_label)
        .map(|contents| contents.text)
        .map_err(String::from)
}

/// 1-based line + 1-based **character** column → byte offset, plus that line's text.
///
/// Characters rather than bytes because that is what a caller counts, and the two differ
/// on exactly the accented source this editor is pointed at. A column past the end of
/// the line clamps to the line's end rather than failing: being asked about the end of a
/// line is a reasonable thing, and refusing would cost a round trip to learn its length.
fn offset_of(source: &str, line: u32, column: u32) -> Result<(usize, String), String> {
    if line == 0 {
        return Err("line is 1-based; 0 is not a line".into());
    }
    let mut offset = 0usize;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        if index as u32 + 1 == line {
            let bare = text.trim_end_matches(['\r', '\n']);
            let within: usize = bare
                .char_indices()
                .nth(column.saturating_sub(1) as usize)
                .map(|(byte, _)| byte)
                .unwrap_or(bare.len());
            return Ok((offset + within, bare.to_string()));
        }
        offset += text.len();
    }
    Err(format!("the file has fewer than {line} lines"))
}

// ── Rename ──────────────────────────────────────────────────────────────────────

/// One edit, addressed the way a caller who will actually make it can use.
#[derive(serde::Serialize)]
pub struct RenameSite {
    /// 1-based line.
    pub line: u32,
    /// 1-based character column of the first character to replace.
    pub column: u32,
    /// The text currently there. Check it before editing — if it does not match, the file
    /// changed after this plan was made and the plan is stale.
    pub old: String,
    pub new_text: String,
    /// `declaration` | `reference` | `import` | `spring-bean` | `local`.
    pub reason: String,
    /// True when the engine inferred this site rather than resolving it: a call to a
    /// same-named method whose overload could not be told apart. Read these before
    /// applying them; the rest are exact.
    pub inferred: bool,
}

/// The edits for one file.
#[derive(serde::Serialize)]
pub struct RenameFileSites {
    pub file: String,
    pub edits: Vec<RenameSite>,
}

/// A plan, not a change.
#[derive(serde::Serialize)]
pub struct RenamePlanForAgent {
    pub old_name: String,
    pub new_name: String,
    /// What is being renamed (`"method com.x.Foo.bar()"`, "local `x`", …).
    pub target: String,
    pub files: Vec<RenameFileSites>,
    pub total_edits: usize,
    /// True when any site is `inferred`.
    pub has_inferred: bool,
    /// Why the plan is empty, when it is — the two reasons need different responses.
    pub note: Option<String>,
}

/// Args for [`bennu_rename_plan_at`].
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct RenameAtArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Absolute path to the file holding the symbol.
    pub file: String,
    /// 1-based line of the symbol to rename.
    pub line: u32,
    /// 1-based character column. Any column within the identifier works.
    pub column: Option<u32>,
    /// The new name.
    pub new_name: String,
}

/// Work out every edit a rename needs, addressed by line and column.
///
/// Delegates to the same engine the editor's rename uses — the whole-project reference
/// index — so it separates a real reference from a same-named identifier that merely
/// looks like one, which a textual search-and-replace cannot.
#[arbor_rpc::handler(mcp(
    name = "bennu_rename_plan",
    title = "Plan a rename across a project",
    safety = read,
    description = "Work out every edit renaming a symbol would need, across the whole \
project, addressed by line and column. Prefer this over a textual search-and-replace for \
any rename: it answers from the project's reference index, so it separates a real \
reference from an identifier that merely shares the name, and it follows a class rename \
into imports and Spring bean declarations. It PLANS ONLY — nothing is written, and you \
apply the edits yourself with your own file-editing tools. Each site carries the text \
currently there: check it before editing, and if it does not match, the file changed \
after the plan was made. Sites marked `inferred` are the engine's best guess (a call to \
a same-named method whose overload could not be told apart) — read those before applying \
them; the rest are exact. An empty plan says why in `note`: a position not on a \
renameable identifier and an index still building are different problems.",
))]
fn bennu_rename_plan_at(
    _ctx: &BennuState,
    args: RenameAtArgs,
) -> Result<RenamePlanForAgent, String> {
    let source = read_source(&args.root, &args.file)?;
    let (offset, _) = offset_of(&source, args.line, args.column.unwrap_or(1))?;

    // The same two-step the editor's own handler uses: a language-server-backed file plans
    // through its server, everything else through the Java engine.
    let preview = crate::lsp_route::rename_plan(&args.file, &source, offset, &args.new_name)
        .flatten()
        .or_else(|| {
            crate::index_service::IndexService::global()
                .plan_rename(&args.file, &source, offset, &args.new_name)
                .map(crate::rename::preview_of)
        });

    let Some(preview) = preview else {
        return Ok(RenamePlanForAgent {
            old_name: String::new(),
            new_name: args.new_name,
            target: String::new(),
            files: Vec::new(),
            total_edits: 0,
            has_inferred: false,
            note: Some(
                "Nothing to rename here. Either the position is not on a renameable \
                 identifier — check it with bennu_symbol_at — or the project's index is \
                 still building, which bennu_index_stats will tell you."
                    .into(),
            ),
        });
    };

    // Byte offsets are what the editor wants and the last thing a caller applying edits by
    // hand can use, so every site is converted against its own file, read in that file's
    // real encoding rather than assumed UTF-8.
    let mut files = Vec::new();
    for group in &preview.files {
        let text = if group.file == args.file {
            source.clone()
        } else {
            read_source(&args.root, &group.file)?
        };
        files.push(RenameFileSites {
            file: group.file.clone(),
            edits: group
                .edits
                .iter()
                .map(|e| {
                    let (line, column) = line_col_of(&text, e.start);
                    RenameSite {
                        line,
                        column,
                        old: e.old.clone(),
                        new_text: e.new_text.clone(),
                        reason: e.reason.clone(),
                        inferred: e.inferred,
                    }
                })
                .collect(),
        });
    }

    Ok(RenamePlanForAgent {
        old_name: preview.old_name,
        new_name: preview.new_name,
        target: preview.target_label,
        total_edits: preview.total_edits,
        has_inferred: preview.has_inferred,
        files,
        note: None,
    })
}

/// Byte offset → 1-based line and 1-based **character** column.
///
/// The inverse of [`offset_of`], and it counts characters for the same reason that one
/// does: an editor and a compiler both report columns in characters, so a Cp1252 source
/// full of accents would otherwise be addressed a few columns off from where it looks.
fn line_col_of(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut line_start = 0usize;
    for (index, byte) in source.bytes().enumerate() {
        if index >= offset {
            break;
        }
        if byte == b'\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    let column = source
        .get(line_start..offset.min(source.len()))
        .map(|s| s.chars().count() as u32)
        .unwrap_or(0)
        + 1;
    (line, column)
}

// ── Find usages ─────────────────────────────────────────────────────────────────

/// Args for [`bennu_references_at`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReferencesAtArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Absolute path to the file holding the symbol — its declaration, or any use of it.
    pub file: String,
    /// 1-based line of the symbol.
    pub line: u32,
    /// 1-based character column. Any column within the identifier works.
    #[serde(default)]
    pub column: Option<u32>,
    /// Cap on the use sites returned. Defaults to 200.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// One place the symbol is used.
#[derive(Debug, Serialize)]
pub struct UsageSite {
    /// 1-based line.
    pub line: u32,
    /// 1-based character column of the identifier.
    pub column: u32,
    /// The source line, trimmed — enough to tell a call from an assignment without
    /// opening the file.
    pub preview: String,
}

/// The uses in one file, so a caller can see where the weight is before reading any of it.
#[derive(Debug, Serialize)]
pub struct UsageFile {
    pub file: String,
    pub count: usize,
    pub usages: Vec<UsageSite>,
}

/// Where a symbol is used, across the whole project.
#[derive(Debug, Serialize)]
pub struct ReferencesForAgent {
    /// What the position resolved to (`"method com.x.Foo.bar()"`) — check it before
    /// trusting the list, since a column landing on the wrong token answers about the
    /// wrong symbol.
    pub target: String,
    /// Files, heaviest first.
    pub files: Vec<UsageFile>,
    /// How many use sites there are in total, before any cap.
    pub total: usize,
    /// Why the answer is empty or short, when it is.
    pub note: Option<String>,
}

/// Find every place a symbol is used, across the whole project.
///
/// The question a text search answers badly: `process` matches two hundred lines and a
/// handful of them are this method. This answers from the project's reference index, so
/// it resolves each occurrence to a declaration first and returns only the ones that are
/// genuinely this symbol.
///
/// Addressed by line and column — on the declaration or on any use of it. Results are
/// grouped by file, heaviest first, each site with its source line, so the shape of the
/// blast radius is visible before reading a single file.
#[arbor_rpc::handler(mcp(
    name = "bennu_references",
    title = "Find where a symbol is used",
    safety = read,
))]
fn bennu_references_at(
    ctx: &BennuState,
    args: ReferencesAtArgs,
) -> Result<ReferencesForAgent, String> {
    let source = read_source(&args.root, &args.file)?;
    let (offset, line_text) = offset_of(&source, args.line, args.column.unwrap_or(1))?;
    let limit = args.limit.unwrap_or(200).clamp(1, 2_000);

    // The editor's own handler, so a server-backed file answers from its server and a Java
    // one from Bennu's resolver — the same routing, and therefore the same answer.
    let found = crate::references::bennu_references(
        ctx,
        crate::references::ReferencesArgs {
            file: args.file.clone(),
            source: source.clone(),
            offset,
            origin_file: None,
        },
    )?;

    let Some(found) = found else {
        return Ok(ReferencesForAgent {
            target: String::new(),
            files: Vec::new(),
            total: 0,
            note: Some(nothing_here_note(
                &args.root,
                &args.file,
                args.line,
                args.column.unwrap_or(1),
                &line_text,
            )),
        });
    };

    // Both engines report a byte column, which is off by the number of accented characters
    // before it on exactly the sources this editor exists for. Recomputed here against each
    // file read in its own encoding, so a follow-up call at these coordinates lands.
    let mut texts: HashMap<String, Option<String>> = HashMap::new();
    texts.insert(args.file.clone(), Some(source));

    let mut by_file: Vec<(String, Vec<UsageSite>)> = Vec::new();
    for hit in found.usages {
        let text = texts
            .entry(hit.file.clone())
            .or_insert_with(|| read_source(&args.root, &hit.file).ok());
        let (line, column) = match text.as_deref() {
            Some(text) => line_col_of(text, hit.start),
            None => (hit.line as u32, hit.col as u32),
        };
        let site = UsageSite { line, column, preview: hit.preview };
        match by_file.iter_mut().find(|(file, _)| *file == hit.file) {
            Some((_, sites)) => sites.push(site),
            None => by_file.push((hit.file, vec![site])),
        }
    }

    let total: usize = by_file.iter().map(|(_, sites)| sites.len()).sum();
    by_file.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));

    // A cap cuts sites, never files: knowing a symbol is touched in forty files matters more
    // than seeing every line of the first three.
    let mut budget = limit;
    let mut files = Vec::new();
    for (file, mut sites) in by_file {
        sites.sort_by_key(|s| (s.line, s.column));
        let count = sites.len();
        sites.truncate(budget);
        budget -= sites.len();
        files.push(UsageFile { file, count, usages: sites });
    }

    let note = if total > limit {
        Some(format!(
            "Showing {limit} of {total} use sites; each file still reports its full `count`. \
             Pass a larger limit for the rest."
        ))
    } else if total == 0 {
        Some(format!(
            "`{}` resolves, but nothing in the project uses it. {}",
            found.target_label,
            index_caveat(&args.root, &args.file)
        ))
    } else {
        None
    };

    Ok(ReferencesForAgent { target: found.target_label, files, total, note })
}

/// The sentence for a language server that is not in a position to have answered, when there is
/// one — and `None` when the server is fine and the empty answer is therefore real.
///
/// Every empty answer on a server-backed file has to pass through this first. A server whose
/// workspace is still loading returns *nothing* for a symbol that is used in forty places, and a
/// note that blames the caret for it turns a wait into a verdict.
fn server_wait_note(file: &str) -> Option<String> {
    wait_note_for(crate::lsp_registry::LspRegistry::global().readiness_for(file))
}

/// The sentence for one readiness state. Split from the lookup so the wording — which is the part
/// a model acts on — can be tested without a language server on the machine running the tests.
fn wait_note_for(readiness: crate::lsp_registry::ServerReadiness) -> Option<String> {
    use crate::lsp_registry::ServerReadiness;

    match readiness {
        ServerReadiness::Warming { name, detail } => {
            let doing = match detail.is_empty() {
                true => "still starting".to_string(),
                false => format!("still loading this project ({detail})"),
            };
            Some(format!(
                "{name} is {doing}, so this is not an answer yet — an empty result here means the \
                 server has nothing loaded to answer from, not that nothing was found. Try again \
                 in a few seconds."
            ))
        }
        ServerReadiness::Failed { name, message } => Some(format!(
            "{name} could not run for this project ({message}), and it is what answers questions \
             about this language — so nothing can be resolved here until that is fixed. This is \
             not a statement about the code."
        )),
        ServerReadiness::Idle { name } => Some(format!(
            "{name} serves this file but has not been started, so there is nothing to answer from \
             yet. Call bennu_project_summary on the project root first."
        )),
        ServerReadiness::Absent | ServerReadiness::Ready { .. } => None,
    }
}

/// Why a caret-addressed lookup came back with nothing — the reasons need different responses,
/// and an empty result tells them apart for nobody.
fn nothing_here_note(root: &str, file: &str, line: u32, column: u32, line_text: &str) -> String {
    // Before the caret is blamed, because the caret is probably fine.
    if let Some(waiting) = server_wait_note(file) {
        return waiting;
    }
    if !is_cargo_root(root) && !IndexService::global().index_stats(root).ready {
        return "The semantic index has not finished building, so nothing can be resolved yet. \
                Check bennu_index_stats and try again."
            .to_string();
    }
    format!(
        "Nothing referenceable at line {line} column {column}. The position may be on \
         whitespace, a keyword or a comment — check it with bennu_symbol_at. A local variable \
         or parameter also lands here: those are scope-exact and not tracked across files, so \
         read the enclosing method instead. The line reads: {}",
        line_text.trim()
    )
}

/// The sentence to append to an empty answer that might not be an answer yet.
///
/// Two engines can be mid-build behind an empty result and they are not the same one. A Java
/// project has Bennu's own semantic index; a Cargo project has a language server, which is *also*
/// something that is still building for the first half-minute — this said the opposite, and a
/// confident "nothing is still building" is what turns "not yet" into "not used".
fn index_caveat(root: &str, file: &str) -> String {
    if is_cargo_root(root) {
        return server_wait_note(file).unwrap_or_else(|| {
            "The language server has finished loading this project, so this is an answer rather \
             than a wait."
                .to_string()
        });
    }
    match IndexService::global().index_stats(root).ready {
        true => "The index has finished building, so this is an answer rather than a wait.".into(),
        false => "The index is still building, so this is not yet an answer — check \
                  bennu_index_stats and try again."
            .into(),
    }
}

/// Which build model governs a root — the same test the editor's own open path makes, so a
/// project cannot be Cargo there and Maven here.
fn is_cargo_root(root: &str) -> bool {
    Path::new(root).join("Cargo.toml").is_file()
}

// ── Tests ───────────────────────────────────────────────────────────────────────

/// Args for [`bennu_tests`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TestsArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Only the tests declared in this file (absolute path).
    #[serde(default)]
    pub file: Option<String>,
    /// Keep only tests whose name or owner contains this, case-insensitively.
    #[serde(default)]
    pub query: Option<String>,
    /// Cap on results. Defaults to 500.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// One test, named the way its runner names it.
#[derive(Debug, Serialize)]
pub struct TestEntry {
    /// How a runner selects exactly this test: `OrderTest#computesTotal` for Maven,
    /// `orders::tests::computes_total` for cargo.
    pub selector: String,
    /// The method / function name on its own.
    pub name: String,
    /// What holds it: the test class for Java, the package and target for Rust.
    pub owner: String,
    /// Absolute path of the declaring file.
    pub file: String,
    /// 1-based line of the declaration.
    pub line: u32,
    /// True when it will not run as things stand — `@Disabled` / `@Ignore`, `#[ignore]`,
    /// or a method on an abstract base class the runner never instantiates.
    pub skipped: bool,
    /// Anything else worth knowing: the framework, why it is disabled, whether one
    /// declaration produces many cases.
    pub note: Option<String>,
}

/// Every test the project declares.
#[derive(Debug, Serialize)]
pub struct TestCatalogue {
    /// `maven` or `cargo` — which build system's rules the selectors follow.
    pub kind: String,
    pub tests: Vec<TestEntry>,
    /// How many were found in total, before the limit.
    pub total: usize,
    pub note: Option<String>,
}

/// List the tests a project declares, with the selector each runner needs to run one.
///
/// Read from the files on disk rather than from a build: it answers on a project that has
/// never been compiled, and it costs nothing. Covers JUnit 3/4/5 and TestNG on a Maven
/// project, and `#[test]` / `#[tokio::test]` / `#[bench]` / `rstest` on a Cargo one.
///
/// Answers "is this covered", "what would I run after touching this", and "what is already
/// disabled" — the last of which a test run cannot tell you, because a disabled test does
/// not appear in its output at all.
///
/// It does not run anything. Take the `selector` and run it with your own shell.
#[arbor_rpc::handler(mcp(
    title = "List the project's tests",
    safety = read,
))]
fn bennu_tests(ctx: &BennuState, args: TestsArgs) -> Result<TestCatalogue, String> {
    let limit = args.limit.unwrap_or(500).clamp(1, 5_000);
    let cargo = is_cargo_root(&args.root);

    let mut tests: Vec<TestEntry> = if cargo {
        crate::cargo_tests::bennu_discover_cargo_tests(
            ctx,
            crate::cargo_tests::DiscoverArgs {
                root: args.root.clone(),
                file: args.file.clone(),
                force: false,
            },
        )?
        .into_iter()
        .map(rust_test_entry)
        .collect()
    } else {
        crate::tests::bennu_discover_tests(
            ctx,
            crate::tests::DiscoverTestsArgs {
                root: args.root.clone(),
                file: args.file.clone(),
                force: false,
            },
        )?
        .into_iter()
        .flat_map(java_test_entries)
        .collect()
    };

    if let Some(needle) = args.query.as_deref().map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty()) {
        tests.retain(|t| {
            t.name.to_lowercase().contains(&needle) || t.owner.to_lowercase().contains(&needle)
        });
    }
    tests.sort_by(|a, b| a.owner.cmp(&b.owner).then_with(|| a.line.cmp(&b.line)));

    let total = tests.len();
    let skipped = tests.iter().filter(|t| t.skipped).count();
    let note = if total > limit {
        Some(format!("Showing {limit} of {total} tests. Narrow with `query`, or pass a larger limit."))
    } else if total == 0 {
        Some(
            "No tests found. On a Maven project a test is a class whose methods carry \
             @Test / extend TestCase; on a Cargo one, a function under #[test]."
                .to_string(),
        )
    } else if skipped > 0 {
        Some(format!("{skipped} of these are disabled and will not run."))
    } else {
        None
    };
    tests.truncate(limit);

    Ok(TestCatalogue {
        kind: if cargo { "cargo".into() } else { "maven".into() },
        tests,
        total,
        note,
    })
}

/// One Java test class → one entry per test method.
///
/// A class with no methods of its own still yields a row: an abstract base holding the
/// shared tests is worth seeing, and dropping it would make its subclasses' inherited
/// coverage look like it came from nowhere.
fn java_test_entries(found: crate::tests::DiscoveredTest) -> Vec<TestEntry> {
    let class = found.class;
    let framework = class.framework.label();
    if class.methods.is_empty() {
        return vec![TestEntry {
            selector: class.selector.clone(),
            name: class.selector.clone(),
            owner: class.fqcn.clone(),
            file: class.file.clone(),
            line: class.line,
            skipped: class.disabled || class.is_abstract,
            note: Some(match class.is_abstract {
                true => format!("{framework}; abstract base class — its tests run through its subclasses"),
                false => format!("{framework}; no test methods of its own"),
            }),
        }];
    }
    class
        .methods
        .iter()
        .map(|m| {
            let mut notes = vec![framework.to_string()];
            if class.is_abstract {
                notes.push("declared on an abstract base — runs through the subclasses".into());
            }
            if let Some(reason) = m.disabled_reason.as_deref().filter(|r| !r.is_empty()) {
                notes.push(format!("disabled: {reason}"));
            } else if m.disabled || class.disabled {
                notes.push("disabled".into());
            }
            if m.dynamic {
                notes.push("one declaration, many cases (parameterised / repeated)".into());
            }
            TestEntry {
                selector: format!("{}#{}", class.selector, m.name),
                name: m.name.clone(),
                owner: class.fqcn.clone(),
                file: class.file.clone(),
                line: m.line,
                skipped: class.disabled || m.disabled || class.is_abstract,
                note: Some(notes.join("; ")),
            }
        })
        .collect()
}

/// One discovered `#[test]` → one entry.
fn rust_test_entry(t: bennu_test::prelude::RustTest) -> TestEntry {
    let mut notes = vec![t.target.label()];
    if t.ignored {
        notes.push("#[ignore] — runs only when named".into());
    }
    if t.should_panic {
        notes.push("#[should_panic] — it passes by panicking".into());
    }
    if matches!(t.kind, bennu_test::prelude::RustTestKind::Parameterized) {
        notes.push("one declaration, many cases — filter by prefix".into());
    }
    TestEntry {
        selector: t.path,
        name: t.name,
        owner: match t.module.is_empty() {
            true => t.package,
            false => format!("{}::{}", t.package, t.module),
        },
        file: t.file,
        line: t.line,
        skipped: t.ignored,
        note: Some(notes.join("; ")),
    }
}

// ── Running the tests ───────────────────────────────────────────────────────────
//
// The editor's runners return the moment the child is up, because a panel is going to
// listen. A caller that cannot listen gets a run id and no idea what happened — so these
// drive the same run to its end and answer with the result. The events still fire, so
// Bennu's own Tests panel fills in live while the caller waits.

/// Args for [`bennu_test_run`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunTestsAtArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Which tests to run, spelled the way `bennu_tests` spells them
    /// (`OrderTest#computesTotal`, `orders::tests::works`). Empty runs everything in
    /// `module` / `module`'s absence, which on a large project is minutes — name what you
    /// changed.
    #[serde(default)]
    pub tests: Vec<String>,
    /// Run everything in one crate or Maven module instead of naming its tests.
    ///
    /// The grade between "one test" and "the whole project", and usually the one you
    /// want: after touching a crate you run that crate. A Cargo package name
    /// (`nd-console`) or a Maven module path relative to the root. Ignored when `tests`
    /// names anything.
    #[serde(default)]
    pub module: Option<String>,
    /// Also run the `#[ignore]`d ones. Cargo only: Maven has no equivalent switch, and a
    /// `@Disabled` test can only be re-enabled by editing it.
    #[serde(default)]
    pub include_ignored: bool,
}

/// Run a project's tests and wait for the result.
///
/// Runs `mvn test` or `cargo test` — really: this compiles and executes the project's own
/// code, which is why it needs approval. Blocks until the run ends, then reports the
/// counts and **every failure with its message**, so a failing assertion arrives as
/// something you can act on rather than as a run id to go and ask about.
///
/// Three grades of selection, and the middle one is usually right: name the tests in
/// `tests` using the selectors `bennu_tests` reports, or name one crate / Maven module in
/// `module` to run all of its, or give neither and run the whole project. The last is the
/// default and is usually the wrong one on a real project: it takes minutes and buries the
/// four failures you caused in the sixty that were already there.
///
/// A run that compiles nothing and tests nothing is reported as such rather than as a pass
/// — "0 failed" out of a build that never started is the one result worth distrusting.
#[arbor_rpc::handler(mcp(
    title = "Run the tests and report what failed",
    safety = destructive,
))]
fn bennu_test_run(
    ctx: &BennuState,
    args: RunTestsAtArgs,
) -> Result<crate::test_report::TestRunReport, String> {
    if is_cargo_root(&args.root) {
        let scope = cargo_scope(ctx, &args)?;
        let run = crate::cargo_tests::start_cargo_run(
            ctx,
            &crate::cargo_tests::RunArgs {
                root: args.root.clone(),
                scope,
                include_ignored: args.include_ignored,
            },
        )?;
        let widened = run.handle().widened;
        let collector = std::sync::Arc::new(crate::test_report::Collector::default());
        let end = run.drive(Some(collector.clone()));
        return Ok(report_of(&collector, "cargo", end, widened, args.tests.len()));
    }

    let (scope, widened) = maven_scope(&args.tests, args.module.as_deref());
    let run = crate::tests::start_maven_run(
        ctx,
        &crate::tests::RunTestsArgs { root: args.root.clone(), scope },
    )?;
    // The plan's own widening (a selection too long for one command line) matters more than
    // ours, and both must reach the caller: a run that quietly ran more than it was asked to
    // is a run whose green is about something else.
    let widened = run.handle().widened.or(widened);
    let collector = crate::test_report::Collector::default();
    let end = run.drive(Some(&collector));
    Ok(report_of(&collector, "maven", end, widened, args.tests.len()))
}

/// Build the report, folding a widened selection into the note.
///
/// "Widened" and "ran more than you asked for" are not the same thing, and conflating them
/// was a lie in the commonest case: a selection too long to spell falls back to running by
/// scope, and when the tests you named ARE that scope — every test in one crate, say — the
/// fallback covers exactly what you asked for and nothing else. The run's own count is what
/// settles it, so it is checked rather than assumed.
fn report_of(
    collector: &crate::test_report::Collector,
    kind: &str,
    end: crate::test_report::RunEnd,
    widened: Option<String>,
    asked: usize,
) -> crate::test_report::TestRunReport {
    let mut report =
        collector.finish(kind, end.label, end.command, end.code, end.cancelled, end.totals);
    if let Some(widened) = widened {
        let ran = report.passed + report.failed + report.skipped;
        let notice = match ran as usize > asked {
            true => format!("More was run than you asked for: {widened}"),
            // Not a warning, but worth saying: the command line does not look like the
            // request, and a reader comparing the two deserves to know why.
            false => format!(
                "The selection was too long to name, so it ran by scope instead                  ({widened}) — which came to exactly the {asked} tests you asked for."
            ),
        };
        report.note = Some(match report.note {
            Some(existing) => format!("{notice} {existing}"),
            None => notice,
        });
    }
    report
}

/// Maven's selection, plus what had to be widened to express it.
///
/// `-Dtest=` takes classes or cases, not both, so a mixed list runs the whole class of
/// every case named. That is a superset of what was asked for, which is only acceptable
/// said out loud.
fn maven_scope(
    tests: &[String],
    module: Option<&str>,
) -> (bennu_test::prelude::TestScope, Option<String>) {
    use bennu_test::prelude::{TestCaseRef, TestScope};

    if tests.is_empty() {
        return match module.map(str::trim).filter(|m| !m.is_empty()) {
            Some(module) => (TestScope::Module { module: module.to_string() }, None),
            None => (TestScope::All, None),
        };
    }
    if tests.iter().all(|t| t.contains('#')) {
        let cases = tests
            .iter()
            .filter_map(|t| t.split_once('#'))
            .map(|(class, method)| TestCaseRef {
                class: class.to_string(),
                method: method.to_string(),
            })
            .collect();
        return (TestScope::Cases { cases }, None);
    }

    let mut classes: Vec<String> =
        tests.iter().map(|t| t.split('#').next().unwrap_or(t).to_string()).collect();
    classes.sort();
    classes.dedup();
    let widened = tests
        .iter()
        .any(|t| t.contains('#'))
        .then(|| "the selection mixed whole classes with single methods, which Maven's \
                   filter cannot express, so every named class ran in full."
            .to_string());
    (TestScope::Classes { classes }, widened)
}

/// Cargo's selection, resolved against discovery.
///
/// A libtest path alone does not say which package or target it compiles into, and cargo
/// needs both to narrow a run. Discovery knows — and it is cached, so this costs nothing
/// after the first call.
fn cargo_scope(
    ctx: &BennuState,
    args: &RunTestsAtArgs,
) -> Result<bennu_test::prelude::CargoTestScope, String> {
    use bennu_test::prelude::{CargoTestScope, RustCaseRef, RustTestKind};

    if args.tests.is_empty() {
        return Ok(match args.module.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            Some(package) => CargoTestScope::Package { package: package.to_string() },
            None => CargoTestScope::Workspace,
        });
    }
    let found = crate::cargo_tests::bennu_discover_cargo_tests(
        ctx,
        crate::cargo_tests::DiscoverArgs {
            root: args.root.clone(),
            file: None,
            force: false,
        },
    )?;

    let mut cases = Vec::new();
    let mut missing = Vec::new();
    for wanted in &args.tests {
        match found.iter().find(|t| &t.path == wanted) {
            Some(test) => cases.push(RustCaseRef {
                package: test.package.clone(),
                target: test.target.clone(),
                path: test.path.clone(),
                // A parameterised function's real cases are `path::case_1`, `case_2`, … —
                // asking libtest for `path` exactly would run none of them.
                exact: !matches!(test.kind, RustTestKind::Parameterized),
            }),
            None => missing.push(wanted.as_str()),
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "No test in this workspace is named {}. Names come from bennu_tests — they are \
             libtest paths (`module::tests::name`), not file paths or function names on \
             their own.",
            missing.join(", ")
        ));
    }
    Ok(CargoTestScope::Cases { cases })
}

// ── Problems in one file ────────────────────────────────────────────────────────

/// Args for [`bennu_check_file`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckFileArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Absolute path to the file to check.
    pub file: String,
}

/// One problem, addressed the way a caller can act on.
#[derive(Debug, Serialize)]
pub struct Problem {
    /// `error` | `warning` | `info` | `hint`.
    pub severity: String,
    /// A stable slug for the kind of problem (`unknown-member`, `unused-import`, …), when
    /// the emitting check has one.
    pub code: String,
    pub message: String,
    /// 1-based line.
    pub line: u32,
    /// 1-based character column.
    pub column: u32,
    /// The source line, trimmed.
    pub preview: String,
}

/// What is wrong with one file.
#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub file: String,
    pub problems: Vec<Problem>,
    pub errors: usize,
    pub warnings: usize,
    /// What this check could and could not see.
    pub note: Option<String>,
}

/// Check one file for problems, without building anything.
///
/// For a Java file: the resolver-backed validation — unknown members, type and inheritance
/// errors, unused imports, syntax — plus whatever the framework extensions have to say
/// about it (Spring placeholders, SpEL, bean XML). For a JSP: Struts action references
/// that point at no action. For a Rust file: what the language server last published,
/// which means what `cargo check` last found.
///
/// Use it after editing a file, in place of a build you would otherwise have to run and
/// wait for. It reads the file from disk in the project's own encoding, so check the file
/// after saving it rather than before.
///
/// An empty list is a real answer only once the index has finished building — the note
/// says which case you are in.
#[arbor_rpc::handler(mcp(
    title = "Check one file for problems",
    safety = read,
))]
fn bennu_check_file(ctx: &BennuState, args: CheckFileArgs) -> Result<CheckResult, String> {
    let source = read_source(&args.root, &args.file)?;
    let diagnostics = crate::intel::bennu_diagnostics(
        ctx,
        crate::intel::DiagnosticsArgs {
            file: args.file.clone(),
            source: Some(source.clone()),
            // The full resolver-backed pass. The fast tier exists to paint squiggles between
            // keystrokes; a caller that asked a question once wants the complete answer.
            resolved: Some(true),
            actions: Vec::new(),
        },
    )?;

    let problems: Vec<Problem> = diagnostics
        .into_iter()
        .map(|d| {
            let (line, column) = line_col_of(&source, d.start);
            Problem {
                severity: d.severity,
                code: d.code,
                message: d.message,
                line,
                column,
                preview: source
                    .lines()
                    .nth(line.saturating_sub(1) as usize)
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            }
        })
        .collect();

    let errors = problems.iter().filter(|p| p.severity == "error").count();
    let warnings = problems.iter().filter(|p| p.severity == "warning").count();
    let note = match (problems.is_empty(), crate::lsp_route::owns(&args.file)) {
        (false, _) => None,
        // A server publishes; it does not compute on demand. For Rust that publish is the
        // last `cargo check`, which runs on save — so "no problems" on a file edited and not
        // saved means the check has not seen the edit, and saying nothing here would let that
        // read as a clean bill of health.
        // A server that has not finished loading has not published anything either, and "nothing
        // to report" is the most reassuring possible way to say so.
        (true, true) => Some(server_wait_note(&args.file).unwrap_or_else(|| {
            "Nothing to report. These come from the language server's last publish — for Rust \
             that is the last `cargo check`, which runs on save, so a file edited and not saved \
             has not been checked yet."
                .to_string()
        })),
        (true, false) => {
            Some(format!("Nothing to report. {}", index_caveat(&args.root, &args.file)))
        }
    };

    Ok(CheckResult { file: args.file, problems, errors, warnings, note })
}

// ── The shape of the project itself ─────────────────────────────────────────────

/// Args for [`bennu_module_graph_of`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ModuleGraphArgs {
    /// Absolute path to the project root — the directory holding the root `Cargo.toml`
    /// or `pom.xml`.
    pub root: String,
}

/// One module of the project, with its position in the internal graph.
#[derive(Debug, Serialize)]
pub struct ModuleNode {
    /// What the build tool calls it: a Cargo package name, a Maven artifactId.
    pub id: String,
    /// What it builds — `lib`, `bin`, `lib+bin`, `proc-macro`, or Maven's packaging.
    pub kind: String,
    /// How far above the foundation it sits: `0` depends on no other module here.
    pub layer: usize,
    /// The modules of this project it depends on directly.
    pub depends_on: Vec<String>,
    /// How many modules depend on it directly.
    pub dependents: usize,
    /// Third-party dependencies it declares, by distinct coordinate.
    pub external: usize,
    /// How much of the project it is built on — modules reached transitively.
    pub reach: usize,
    /// How much rebuilds when it changes — modules depending on it transitively.
    pub impact: usize,
    /// Whether it sits in a dependency cycle.
    pub in_cycle: bool,
    /// Absolute path of its manifest.
    pub manifest: String,
}

/// The internal shape of a workspace or a reactor.
#[derive(Debug, Serialize)]
pub struct ProjectModules {
    /// `cargo` or `maven`; empty for a root that is neither.
    pub ecosystem: String,
    /// The modules, foundation first, heaviest within a layer.
    pub modules: Vec<ModuleNode>,
    /// Groups of modules that all reach each other — a cycle, by name.
    pub cycles: Vec<Vec<String>>,
    /// The longest chain of modules, which is the depth of the deepest rebuild.
    pub depth: usize,
    /// Distinct third-party dependencies across the whole project.
    pub external_total: usize,
    pub note: Option<String>,
}

/// Map a project's own modules: which depends on which, and what a change to each costs.
///
/// The question that comes before a refactor on anything bigger than one crate. Two
/// numbers carry it: `reach` is how much of the project a module is built on, and
/// `impact` is how much rebuilds when you touch it — a leaf with a high impact is not a
/// leaf, and that is the number to check before proposing a change to something shared.
///
/// `cycles` names groups of modules that all reach each other. The build tool refuses
/// these and names one pair out of the group when it does, which is why the group is the
/// useful form. On a Cargo workspace a cycle through `dev-dependencies` is legal and is
/// not reported.
///
/// Reads the manifests and nothing else — it never runs cargo or maven, so it answers on
/// a project that has never been built. Cargo and Maven both.
#[arbor_rpc::handler(mcp(
    name = "bennu_module_graph",
    title = "Map the project's own modules",
    safety = read,
))]
fn bennu_module_graph_of(
    ctx: &BennuState,
    args: ModuleGraphArgs,
) -> Result<ProjectModules, String> {
    let graph = crate::dependencies::bennu_module_graph(
        ctx,
        crate::dependencies::DependenciesArgs { root: args.root.clone() },
    )?;

    let name_of = |i: usize| graph.nodes.get(i).map(|n| n.id.clone()).unwrap_or_default();
    let mut depends: HashMap<usize, Vec<String>> = HashMap::new();
    for edge in &graph.edges {
        let names = depends.entry(edge.from).or_default();
        let name = name_of(edge.to);
        // A pair can carry several edges — a normal dependency and a dev one are two facts
        // about the same two crates. The list is of modules, so it names each once.
        if !names.contains(&name) {
            names.push(name);
        }
    }

    let mut modules: Vec<ModuleNode> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| ModuleNode {
            id: n.id.clone(),
            kind: n.kind.clone(),
            layer: n.layer,
            depends_on: depends.remove(&i).unwrap_or_default(),
            dependents: n.dependents,
            external: n.external,
            reach: n.reach,
            impact: n.impact,
            in_cycle: n.in_cycle,
            manifest: n.manifest.clone(),
        })
        .collect();
    // Bottom-up, because that is how a workspace reads: the crates everything else is built
    // on first. Within a layer the one whose change costs the most comes first, which is the
    // order in which the list is worth skimming.
    modules.sort_by(|a, b| {
        a.layer.cmp(&b.layer).then(b.impact.cmp(&a.impact)).then(a.id.cmp(&b.id))
    });

    let cycles: Vec<Vec<String>> =
        graph.cycles.iter().map(|c| c.iter().map(|&i| name_of(i)).collect()).collect();

    Ok(ProjectModules {
        note: module_graph_note(&graph, &modules, &cycles),
        ecosystem: graph.ecosystem.clone(),
        modules,
        cycles,
        depth: graph.depth,
        external_total: graph.external_total,
    })
}

/// The one sentence a graph is worth adding to itself.
///
/// Each case is a different answer wearing the same empty shape: a root that is not a
/// workspace at all, a workspace too big to have been walked in full, and a cycle — which
/// is the only one of the three that is a finding rather than a caveat.
fn module_graph_note(
    graph: &bennu_deps::prelude::ModuleGraph,
    modules: &[ModuleNode],
    cycles: &[Vec<String>],
) -> Option<String> {
    if graph.ecosystem.is_empty() || modules.is_empty() {
        return Some(
            "This root is neither a Cargo workspace nor a Maven reactor, so it has no module \
             graph. A single crate with no members is the usual reason."
                .to_string(),
        );
    }
    let mut parts = Vec::new();
    if graph.truncated {
        parts.push(format!(
            "The project has more modules than the graph is built for; these {} are the ones \
             that fit, and the counts only cover them.",
            modules.len()
        ));
    }
    if !cycles.is_empty() {
        parts.push(format!(
            "{} dependency cycle(s) — the build tool refuses these, and it will name only one \
             pair out of each group.",
            cycles.len()
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

// ── What the manifests are behind on ────────────────────────────────────────────

/// Args for [`bennu_outdated`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OutdatedArgs {
    /// Absolute path to the Cargo project or workspace root.
    pub root: String,
}

/// One dependency with a newer release, where it is written.
#[derive(Debug, Serialize)]
pub struct OutdatedDep {
    /// The crate, by its real name — a renamed entry reports the crate, not the local name.
    pub name: String,
    /// The requirement as the manifest writes it.
    pub current: String,
    /// The newest release on crates.io.
    pub latest: String,
    /// The manifest that declares it.
    pub file: String,
    /// 1-based line of the declaration.
    pub line: u32,
}

/// What a project is behind on.
#[derive(Debug, Serialize)]
pub struct OutdatedReport {
    /// Behind dependencies, by manifest and then by line.
    pub behind: Vec<OutdatedDep>,
    /// Manifests read, the workspace root's included.
    pub manifests: usize,
    pub note: Option<String>,
}

/// List the dependencies of a Cargo project that have a newer release on crates.io.
///
/// Sweeps every manifest in the workspace, the root's `[workspace.dependencies]`
/// included — which on a workspace that inherits its versions is where nearly all of the
/// answer lives. Each hit carries the file and line, so it is an edit you can make rather
/// than a name to go and find.
///
/// Deliberately quiet about what it cannot know: a `path`, `git` or workspace-inherited
/// entry has no version here to be behind, and a deliberate pin (`=1.2.3`) or a range that
/// already admits the newest release is left alone. A wrong "update available" on a pin
/// costs more than a missing one.
///
/// Needs the network, and answers from a cache with a freshness window — so it is fast on
/// the second call and works offline on stale data rather than failing. Anything it could
/// not check is counted in the note instead of being passed off as up to date.
#[arbor_rpc::handler(mcp(
    title = "Find dependencies with a newer release",
    safety = read,
    open_world = true,
))]
async fn bennu_outdated(ctx: &BennuState, args: OutdatedArgs) -> Result<OutdatedReport, String> {
    let cfg = bennu_core::config::load().cargo;
    if !cfg.crates_io {
        return Ok(OutdatedReport {
            behind: Vec::new(),
            manifests: 0,
            note: Some(
                "The crates.io index is turned off in Bennu's settings, so nothing was checked. \
                 This is a user setting, not a failure."
                    .to_string(),
            ),
        });
    }
    let report = crate::dependencies::bennu_dependencies(
        ctx,
        crate::dependencies::DependenciesArgs { root: args.root.clone() },
    )?;
    // Which ecosystem owns the root is the report's own answer rather than a second test here.
    // A root holding both a `pom.xml` and a `Cargo.toml` is the Java project everywhere else in
    // Bennu, and a sweep that disagreed would read Maven poms looking for crates.
    if report.ecosystem != "cargo" {
        return Ok(OutdatedReport {
            behind: Vec::new(),
            manifests: 0,
            note: Some(
                "Not a Cargo project. This reads crates.io; a Maven project's versions come from \
                 its own repositories, which Bennu does not query."
                    .to_string(),
            ),
        });
    }

    // The workspace root by hand, and first: a virtual manifest builds no crate, so it is not
    // one of the report's modules — and on a workspace that inherits its versions it is the
    // only file any of them are written in. Forward-slashed, which is the form the report's own
    // paths take: a single-crate project would otherwise be swept twice on Windows, once per
    // spelling of the same file.
    let root = args.root.replace('\\', "/");
    let mut manifests = vec![format!("{}/Cargo.toml", root.trim_end_matches('/'))];
    for module in &report.modules {
        if !manifests.contains(&module.manifest) {
            manifests.push(module.manifest.clone());
        }
    }

    let mut sweep = crate::crates_io::HintSweep::new(crate::crates_io::MAX_FETCHES_PER_SWEEP);
    let mut behind = Vec::new();
    let mut read = 0usize;
    for path in &manifests {
        let Ok(source) = std::fs::read_to_string(path) else { continue };
        read += 1;
        for hint in sweep.manifest(&cfg, &source).await {
            behind.push(OutdatedDep {
                name: hint.name,
                current: hint.current,
                latest: hint.latest,
                file: path.clone(),
                line: hint.line,
            });
        }
    }

    let note = match (behind.is_empty(), sweep.unchecked) {
        (_, unchecked @ 1..) => Some(format!(
            "{unchecked} dependencies could not be checked — this call's budget for reaching \
             crates.io ran out and nothing was cached for them. Call again to pick up where it \
             stopped; the cache carries over."
        )),
        (true, 0) => Some(
            "Every crates.io dependency with a version requirement already admits the newest \
             release. Path, git and workspace-inherited entries were not considered — they have \
             no version here to be behind."
                .to_string(),
        ),
        (false, 0) => None,
    };
    Ok(OutdatedReport { behind, manifests: read, note })
}

/// Args for [`bennu_crate_versions`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateVersionsArgs {
    /// The crate's name on crates.io, spelled exactly — the index has no search.
    pub name: String,
    /// How many releases to return, newest first. 20 by default.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Include pre-releases (`1.0.0-rc.1`), which are left out by default.
    #[serde(default)]
    pub prereleases: bool,
}

/// One published release.
#[derive(Debug, Serialize)]
pub struct CrateVersion {
    pub version: String,
    /// Withdrawn by its author. Still listed, because a lockfile may pin one.
    pub yanked: bool,
    pub prerelease: bool,
}

/// A crate's published history, newest first.
#[derive(Debug, Serialize)]
pub struct CrateVersions {
    pub name: String,
    /// The newest release that is neither yanked nor a pre-release, when there is one.
    pub latest: Option<String>,
    /// The features the newest returned release declares.
    ///
    /// Only that one: features change between releases, so a list per version would be
    /// mostly repetition, and offering an old release's features while adding a new one is
    /// the specific mistake worth not making.
    pub latest_features: Vec<String>,
    pub versions: Vec<CrateVersion>,
    /// How many more exist beyond the ones returned.
    pub older: usize,
    pub note: Option<String>,
}

/// List the versions of a crate published on crates.io, newest first.
///
/// What to check before proposing a version bump: whether the release you have in mind
/// exists, whether it was yanked, and which features it declares. `bennu_outdated` says
/// what is behind; this says what it could move to.
///
/// Answered from a cache with a freshness window, so it costs a request at most once per
/// crate per window. An unknown crate, an unreachable index with nothing cached, and the
/// index being turned off all come back empty with a note rather than as an error.
#[arbor_rpc::handler(mcp(
    title = "List a crate's published versions",
    safety = read,
    open_world = true,
))]
async fn bennu_crate_versions(
    _ctx: &BennuState,
    args: CrateVersionsArgs,
) -> Result<CrateVersions, String> {
    use bennu_cargo::prelude::{is_release, latest_release};

    let name = args.name.trim().to_string();
    let cfg = bennu_core::config::load().cargo;
    let empty = |note: &str| CrateVersions {
        name: name.clone(),
        latest: None,
        latest_features: Vec::new(),
        versions: Vec::new(),
        older: 0,
        note: Some(note.to_string()),
    };
    if !cfg.crates_io {
        return Ok(empty(
            "The crates.io index is turned off in Bennu's settings, so nothing was looked up. \
             This is a user setting, not a failure.",
        ));
    }
    if name.is_empty() {
        return Ok(empty("No crate name was given."));
    }

    let mut published = crate::crates_io::versions_of(&name, &cfg, false).await;
    if published.is_empty() {
        return Ok(empty(
            "Nothing published under that name — or crates.io could not be reached and nothing \
             was cached for it. The index has no search, so the name has to be exact.",
        ));
    }
    let latest = latest_release(&published).map(|v| v.version.clone());
    // The index lists oldest first; every question asked of this list is about the recent end.
    published.reverse();

    let limit = args.limit.unwrap_or(20).max(1);
    let kept: Vec<_> = published
        .iter()
        .filter(|v| args.prereleases || is_release(&v.version))
        .collect();
    let older = kept.len().saturating_sub(limit);
    let latest_features = kept
        .first()
        .map(|v| crate::crates_io::sorted_features(v.features.clone()))
        .unwrap_or_default();
    let versions = kept
        .iter()
        .take(limit)
        .map(|v| CrateVersion {
            prerelease: !is_release(&v.version),
            version: v.version.clone(),
            yanked: v.yanked,
        })
        .collect();

    let note = (!args.prereleases && kept.len() < published.len()).then(|| {
        format!(
            "{} pre-release(s) were left out; pass prereleases to see them.",
            published.len() - kept.len()
        )
    });
    Ok(CrateVersions { name, latest, latest_features, versions, older, note })
}

// ── Does it compile ─────────────────────────────────────────────────────────────

/// How many diagnostics one build reports before it starts summarising.
///
/// A build that broke a widely-used type produces hundreds of errors that are one error, and
/// the first few name it. What is dropped is counted rather than silently cut.
const MAX_BUILD_PROBLEMS: usize = 50;

/// How much of the raw log a failure with no recognised diagnostic carries back.
const MAX_LOG_TAIL_LINES: usize = 40;

/// Args for [`bennu_build_project`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BuildProjectArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Compile only this crate or Maven module, and the ones it is built from.
    ///
    /// A Cargo package name (`nd-console`) or a Maven module path relative to the root.
    /// On a large workspace this is the difference between a check you wait out and one
    /// you read — and after editing one crate it is also the only part of the answer that
    /// changed. Omit it to compile everything.
    #[serde(default)]
    pub module: Option<String>,
}

/// One thing the compiler said, addressed the way a caller can act on.
#[derive(Debug, Serialize)]
pub struct BuildProblem {
    /// `error` · `warning` · `note`.
    pub severity: String,
    /// The file, as the compiler named it — absolute for Maven, relative to the root for
    /// Cargo.
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
}

/// What a compile said.
#[derive(Debug, Serialize)]
pub struct BuildReport {
    /// What ran: `cargo` · `mvn` · `javac` · `up-to-date`.
    pub tool: String,
    /// Whether the compiler exited 0.
    pub ok: bool,
    pub errors: usize,
    pub warnings: usize,
    /// Errors first, then warnings, in the order the compiler reported them.
    pub problems: Vec<BuildProblem>,
    /// How many problems were left out of `problems`.
    pub truncated: usize,
    pub note: Option<String>,
}

/// Compile the project and report what the compiler said.
///
/// The answer to "does this build" — which nothing else here can give, because
/// `bennu_check_file` reads what the language server last published and that is the last
/// `cargo check` *the user's editor ran*, on the file *as it was saved*. After editing,
/// this is what tells you whether the edit compiles.
///
/// Runs `cargo check` on a Cargo root — `check`, not `build`: the answer wanted is the
/// diagnostics, and reaching them without linking is several times faster. On a Maven root
/// it runs `mvn -o compile`, falling back to `javac`, and skips entirely when nothing has
/// changed since the last successful compile (reported as the `up-to-date` tool, which is a
/// real pass).
///
/// Name a `module` unless you mean the whole project. It needs approval because compiling
/// runs code the project supplies — `build.rs`, proc macros, Maven plugins — which is
/// arbitrary execution however ordinary it looks.
///
/// A compile that fails is a normal result carrying its diagnostics, not an error; an error
/// here means no compiler could be started at all.
#[arbor_rpc::handler(mcp(
    name = "bennu_build",
    title = "Compile the project and report the errors",
    safety = destructive,
))]
fn bennu_build_project(ctx: &BennuState, args: BuildProjectArgs) -> Result<BuildReport, String> {
    let outcome = crate::build::compile_project(ctx, &args.root, args.module.as_deref())?;

    let mut problems: Vec<BuildProblem> = outcome
        .diagnostics
        .into_iter()
        .map(|d| BuildProblem {
            severity: d.severity,
            file: d.file,
            line: d.line,
            column: d.col,
            message: d.message,
        })
        .collect();
    let errors = problems.iter().filter(|p| p.severity == "error").count();
    let warnings = problems.iter().filter(|p| p.severity == "warning").count();
    // Stable, so the compiler's own order survives inside each severity — which for a cascade
    // of errors means the one that caused the rest stays at the top.
    problems.sort_by_key(|p| match p.severity.as_str() {
        "error" => 0u8,
        "warning" => 1,
        _ => 2,
    });
    let truncated = problems.len().saturating_sub(MAX_BUILD_PROBLEMS);
    problems.truncate(MAX_BUILD_PROBLEMS);

    Ok(BuildReport {
        note: build_note(&outcome.tool, outcome.ok, errors, truncated, &outcome.raw),
        tool: outcome.tool,
        ok: outcome.ok,
        errors,
        warnings,
        problems,
        truncated,
    })
}

/// What a build result cannot say for itself.
///
/// The case that earns this function is a failure with no diagnostic: a broken manifest, a
/// missing toolchain, a `build.rs` that panicked. The parser only reads compiler lines, so
/// the report would otherwise be "it failed" with nothing to act on — which is exactly the
/// point at which a caller with no build panel is stuck.
fn build_note(
    tool: &str,
    ok: bool,
    errors: usize,
    truncated: usize,
    raw: &str,
) -> Option<String> {
    if tool == "up-to-date" {
        return Some(
            "Nothing had changed since the last successful compile, so nothing was compiled. \
             That is a pass, not a skipped check."
                .to_string(),
        );
    }
    let mut parts = Vec::new();
    if truncated > 0 {
        parts.push(format!(
            "{truncated} further problem(s) were left out. A single broken type produces \
             hundreds of errors that are one error, and the first ones name it."
        ));
    }
    if !ok && errors == 0 {
        let tail: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
        let start = tail.len().saturating_sub(MAX_LOG_TAIL_LINES);
        parts.push(format!(
            "The build failed without a diagnostic this parser recognises — a broken manifest, \
             a missing toolchain or a build script are the usual reasons. The end of its output:\n{}",
            tail[start..].join("\n")
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

#[cfg(test)]
mod offset_tests {
    use super::{line_col_of, offset_of};

    #[test]
    fn resolves_a_line_and_column_to_a_byte_offset() {
        let src = "class A {\n    int total;\n}\n";
        let (offset, text) = offset_of(src, 2, 9).unwrap();
        assert_eq!(text, "    int total;");
        assert_eq!(&src[offset..offset + 5], "total");
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        // Four accented characters are eight bytes; column 5 is the `x`, byte 8.
        let src = "ééééx";
        let (offset, _) = offset_of(src, 1, 5).unwrap();
        assert_eq!(&src[offset..], "x");
    }

    #[test]
    fn crlf_lines_do_not_shift_the_offset() {
        let src = "alpha\r\nbeta\r\n";
        let (offset, text) = offset_of(src, 2, 1).unwrap();
        assert_eq!(text, "beta");
        assert_eq!(&src[offset..offset + 4], "beta");
    }

    #[test]
    fn a_column_past_the_end_clamps_to_it() {
        let (offset, _) = offset_of("ab\ncd\n", 1, 99).unwrap();
        assert_eq!(offset, 2);
    }

    #[test]
    fn a_line_past_the_end_is_an_error_that_says_so() {
        let err = offset_of("one\n", 9, 1).unwrap_err();
        assert!(err.contains("fewer than"), "{err}");
    }

    #[test]
    fn a_byte_offset_becomes_the_line_and_column_it_looks_like() {
        let src = "alpha\nbeta gamma\n";
        assert_eq!(line_col_of(src, 0), (1, 1));
        assert_eq!(line_col_of(src, 6), (2, 1)); // start of "beta"
        assert_eq!(line_col_of(src, 11), (2, 6)); // start of "gamma"
    }

    #[test]
    fn the_two_conversions_are_inverses_and_both_count_characters() {
        // A Cp1252 source full of accents would otherwise be addressed several columns
        // off from where it visibly is — the same reason `offset_of` counts characters.
        let src = "città è bella";
        let offset = src.find('è').unwrap();
        assert_eq!(line_col_of(src, offset), (1, 7));
        assert_eq!(offset_of(src, 1, 7).unwrap().0, offset);
    }

    #[test]
    fn line_zero_is_refused_rather_than_guessed() {
        assert!(offset_of("one\n", 0, 1).unwrap_err().contains("1-based"));
    }
}

#[cfg(test)]
mod catalogue_tests {
    use super::{java_test_entries, report_of, rust_test_entry, TestEntry};
    use bennu_test::prelude::{
        RustTest, RustTestKind, TestClass, TestFramework, TestMethod, TestTarget,
    };

    fn method(name: &str, disabled: bool) -> TestMethod {
        TestMethod {
            name: name.to_string(),
            line: 12,
            offset: 0,
            disabled,
            disabled_reason: None,
            dynamic: false,
        }
    }

    fn java(is_abstract: bool, methods: Vec<TestMethod>) -> Vec<TestEntry> {
        java_test_entries(crate::tests::DiscoveredTest {
            class: TestClass {
                fqcn: "com.acme.OrderTest".into(),
                selector: "OrderTest".into(),
                package: "com.acme".into(),
                file: "/p/OrderTest.java".into(),
                line: 4,
                offset: 0,
                framework: TestFramework::JUnit5,
                is_abstract,
                disabled: false,
                methods,
            },
            module: None,
        })
    }

    #[test]
    fn a_java_method_is_named_the_way_surefire_selects_it() {
        let entries = java(false, vec![method("computesTotal", false)]);
        assert_eq!(entries[0].selector, "OrderTest#computesTotal");
        assert_eq!(entries[0].owner, "com.acme.OrderTest");
        assert!(!entries[0].skipped);
    }

    #[test]
    fn a_disabled_method_and_an_abstract_owner_both_count_as_skipped() {
        assert!(java(false, vec![method("x", true)])[0].skipped);
        // The runner never instantiates an abstract class, so its methods do not run here
        // however enabled they look.
        assert!(java(true, vec![method("x", false)])[0].skipped);
    }

    #[test]
    fn a_class_with_no_methods_still_appears() {
        let entries = java(true, Vec::new());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].note.as_deref().unwrap_or_default().contains("abstract"));
    }

    fn ended() -> crate::test_report::RunEnd {
        crate::test_report::RunEnd {
            code: Some(0),
            cancelled: false,
            command: "cargo test -p x --lib".into(),
            label: "160 tests".into(),
            totals: Some((160, 0, 0)),
        }
    }

    #[test]
    fn a_widening_that_cost_nothing_does_not_claim_it_did() {
        // Naming every test in a crate falls back to running the crate — which is the same
        // set. Reporting that as "more was run than you asked for" was the note lying.
        let report = report_of(
            &crate::test_report::Collector::default(),
            "cargo",
            ended(),
            Some("too many to name".into()),
            160,
        );
        let note = report.note.unwrap();
        assert!(note.contains("exactly the 160"), "{note}");
        assert!(!note.contains("More was run"), "{note}");
    }

    #[test]
    fn a_widening_that_did_cost_something_says_so() {
        let report = report_of(
            &crate::test_report::Collector::default(),
            "cargo",
            ended(),
            Some("too many to name".into()),
            12,
        );
        assert!(report.note.unwrap().contains("More was run than you asked for"));
    }

    #[test]
    fn a_rust_test_carries_its_libtest_path_and_its_crate() {
        let entry = rust_test_entry(RustTest {
            package: "bennu-be".into(),
            target: TestTarget::Lib,
            module: "agent::tests".into(),
            name: "works".into(),
            path: "agent::tests::works".into(),
            file: "/p/agent.rs".into(),
            line: 9,
            offset: 0,
            kind: RustTestKind::Test,
            ignored: true,
            should_panic: false,
        });
        assert_eq!(entry.selector, "agent::tests::works");
        assert_eq!(entry.owner, "bennu-be::agent::tests");
        assert!(entry.skipped, "#[ignore] does not run unless named");
        assert!(entry.note.as_deref().unwrap_or_default().contains("lib"));
    }
}

#[cfg(test)]
mod project_shape_tests {
    use super::{build_note, module_graph_note, ModuleNode};
    use bennu_deps::prelude::ModuleGraph;

    fn node(id: &str) -> ModuleNode {
        ModuleNode {
            id: id.to_string(),
            kind: "lib".into(),
            layer: 0,
            depends_on: Vec::new(),
            dependents: 0,
            external: 0,
            reach: 0,
            impact: 0,
            in_cycle: false,
            manifest: format!("/p/{id}/Cargo.toml"),
        }
    }

    #[test]
    fn a_root_that_is_no_workspace_says_so_instead_of_answering_empty() {
        let note = module_graph_note(&ModuleGraph::default(), &[], &[]).unwrap();
        assert!(note.contains("neither a Cargo workspace"), "{note}");
    }

    #[test]
    fn a_cycle_is_reported_with_why_the_build_tool_will_understate_it() {
        let graph = ModuleGraph { ecosystem: "cargo".into(), ..ModuleGraph::default() };
        let cycles = vec![vec!["a".to_string(), "b".to_string()]];
        let note = module_graph_note(&graph, &[node("a")], &cycles).unwrap();
        assert!(note.contains("1 dependency cycle"), "{note}");
        assert!(note.contains("only one pair"), "{note}");
    }

    #[test]
    fn an_intact_graph_has_nothing_to_add() {
        let graph = ModuleGraph { ecosystem: "cargo".into(), ..ModuleGraph::default() };
        assert!(module_graph_note(&graph, &[node("a")], &[]).is_none());
    }

    #[test]
    fn nothing_to_compile_is_reported_as_a_pass_and_not_as_a_skip() {
        let note = build_note("up-to-date", true, 0, 0, "").unwrap();
        assert!(note.contains("a pass, not a skipped check"), "{note}");
    }

    #[test]
    fn a_failure_with_no_diagnostic_carries_the_end_of_the_log() {
        // The case the note exists for: a caller with no build panel would otherwise be told
        // only that it failed.
        let raw = "error: failed to parse manifest\n\ncaused by: expected `]`\n";
        let note = build_note("cargo", false, 0, 0, raw).unwrap();
        assert!(note.contains("caused by: expected `]`"), "{note}");
        // Blank lines are dropped so the tail is forty lines of output, not of padding.
        assert!(!note.contains("\n\n"), "{note}");
    }

    #[test]
    fn a_clean_build_says_nothing() {
        assert!(build_note("cargo", true, 0, 0, "").is_none());
    }

    #[test]
    fn dropped_diagnostics_are_counted_rather_than_silently_cut() {
        let note = build_note("cargo", false, 120, 70, "").unwrap();
        assert!(note.contains("70 further problem"), "{note}");
    }
}

#[cfg(test)]
mod wait_note_tests {
    use super::wait_note_for;
    use crate::lsp_registry::ServerReadiness;

    #[test]
    fn a_loading_server_says_the_emptiness_is_not_a_finding() {
        // The whole point: "no references" from a server with nothing loaded is a wait, and a
        // caller that reads it as a verdict deletes a method that is used in forty places.
        let note = wait_note_for(ServerReadiness::Warming {
            name: "rust-analyzer".into(),
            detail: "Indexing 43%".into(),
        })
        .unwrap();
        assert!(note.contains("Indexing 43%"), "{note}");
        assert!(note.contains("not an answer yet"), "{note}");
        assert!(note.contains("not that nothing was found"), "{note}");
    }

    #[test]
    fn a_server_still_shaking_hands_has_no_progress_to_quote() {
        let note = wait_note_for(ServerReadiness::Warming {
            name: "rust-analyzer".into(),
            detail: String::new(),
        })
        .unwrap();
        assert!(note.contains("still starting"), "{note}");
        // No empty parenthetical where the progress line would have gone.
        assert!(!note.contains("()"), "{note}");
    }

    #[test]
    fn a_failed_server_says_it_is_not_about_the_code() {
        let note = wait_note_for(ServerReadiness::Failed {
            name: "rust-analyzer".into(),
            message: "not found — install it with `rustup component add rust-analyzer`".into(),
        })
        .unwrap();
        assert!(note.contains("rustup component add"), "{note}");
        assert!(note.contains("not a statement about the code"), "{note}");
    }

    #[test]
    fn a_server_nobody_started_names_the_call_that_starts_it() {
        let note = wait_note_for(ServerReadiness::Idle { name: "rust-analyzer".into() }).unwrap();
        assert!(note.contains("bennu_project_summary"), "{note}");
    }

    #[test]
    fn a_healthy_or_absent_server_adds_nothing() {
        // Both mean the empty answer is real, and a caveat on a real answer is noise that teaches
        // a reader to discount the ones that matter.
        assert!(wait_note_for(ServerReadiness::Ready { name: "rust-analyzer".into() }).is_none());
        assert!(wait_note_for(ServerReadiness::Absent).is_none());
    }
}
