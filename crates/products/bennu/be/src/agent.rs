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
    /// Which engine answers navigation questions here — `bennu-index` on a Java project, the
    /// language server's name on any other. Load-bearing for a caller deciding what to trust:
    /// they warm up differently, fail differently, and are asked to hurry up differently.
    pub engine: String,
    /// Indexed type / member counts, and whether the index has finished building.
    ///
    /// **Absent on a Cargo project**, and that absence is the honest answer rather than a gap.
    /// This is the *Java* index; on a Rust root it has nothing to build, so it reports zero types
    /// and `ready: false` **forever** — which a caller reads as "still indexing" and acts on by
    /// waiting for something that will never happen. That is not a hypothetical: it is what sent
    /// a session away from a project it had open, told to come back later.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<IndexSummary>,
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
/// **Read `engine` before anything else.** It names what actually answers questions here, and
/// the two answer differently. On a Java project it is Bennu's own index, reported in `index`:
/// while `index.ready` is false the project is usable but navigation is incomplete, so call again
/// in a few seconds rather than concluding a symbol does not exist. On a Cargo project it is a
/// **language server**, `index` is absent entirely — there is no Java index there and none is
/// being built — and what has to warm up is the server, whose state `engine` carries.
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
        // Warm: this door opens ONE project, the one the client just asked about. It is the
        // workspace restore's many-at-once that must not.
        true,
    )?;
    let stats = IndexService::global().index_stats(&args.root);
    let capabilities = detected_capabilities(&info.capabilities);
    let java = info.kind.is_java();
    let server = server_for_root(&args.root);

    Ok(ProjectSummary {
        root: info.root.clone(),
        name: info.name.clone(),
        kind: format!("{:?}", info.kind).to_lowercase(),
        modules: info.modules.clone(),
        jdk: info.jdk.as_ref().map(|j| j.version.clone()),
        source_encoding: info.source_encoding.clone(),
        next_steps: next_steps(java, &stats, server.as_ref(), &capabilities),
        engine: match (java, &server) {
            (true, _) => "bennu-index".to_string(),
            (false, Some(s)) => format!("{} ({})", s.name, s.state),
            (false, None) => "none — no language server is running for this project".to_string(),
        },
        index: java.then(|| IndexSummary {
            ready: stats.ready,
            types: stats.types,
            members: stats.members,
            actions: stats.actions,
            beans: stats.beans,
        }),
        capabilities,
    })
}

/// The language server running for `root`, if one is.
///
/// The longest matching root, so a workspace member opened in its own right reports its own
/// server rather than the outer workspace's.
pub(crate) fn server_for_root(root: &str) -> Option<bennu_proto::prelude::LspStatus> {
    let needle = root.replace('\\', "/");
    crate::lsp_registry::LspRegistry::global()
        .statuses()
        .into_iter()
        .filter(|s| {
            let r = s.root.replace('\\', "/");
            needle == r || needle.starts_with(&format!("{}/", r.trim_end_matches('/')))
        })
        .max_by_key(|s| s.root.len())
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
fn next_steps(
    java: bool,
    stats: &bennu_proto::prelude::IndexStats,
    server: Option<&bennu_proto::prelude::LspStatus>,
    capabilities: &[String],
) -> Vec<String> {
    let mut steps = Vec::new();

    // **Which engine, first.** Everything below depends on it, and getting it wrong is not a
    // missing sentence but a wrong instruction: a Cargo project was told the semantic index was
    // "still building" — the *Java* index, which has nothing to build on a Rust root and therefore
    // reports not-ready for ever — and to reach for `bennu_class_index`, which walks `.java` files
    // and can only ever come back empty. A caller acted on both, correctly, and went away from a
    // project it had open to wait for something that was never going to happen.
    if !java {
        steps.push(match server {
            Some(s) if s.state == "ready" => format!(
                "Navigation here is answered by {}, which is up. There is no Java semantic index                  on a Cargo project and none is being built.",
                s.name,
            ),
            Some(s) if s.state == "starting" => format!(
                "{} is still loading this project{}. Until it is up, an empty result means it has                  nothing loaded to answer from — not that nothing was found. Seconds, not minutes.",
                s.name,
                match s.progress.is_empty() {
                    true => String::new(),
                    false => format!(" ({})", s.progress),
                },
            ),
            Some(s) => format!(
                "{} is not running for this project ({}), and it is what answers questions about                  this language. Nothing here can be resolved until that is fixed — it is not a                  statement about the code.",
                s.name,
                match s.message.is_empty() {
                    true => s.state.clone(),
                    false => s.message.clone(),
                },
            ),
            None => "No language server is running for this project yet. One starts on the first                      question about a source file, so ask — do not wait for an index; there is no                      Java semantic index on a Cargo project and none is being built."
                .to_string(),
        });
        steps.push(
            "Use bennu_find_symbol to reach a type or function by name (it asks the server here, \
             not Bennu's Java index, and it takes `Owner.member` as well as a bare name), \
             bennu_references for its use sites — each one says whether it is a call, a \
             construction or an import — bennu_callers to walk the callers transitively, and \
             bennu_implementors for who implements a trait. Before reading an unfamiliar file, \
             bennu_outline lists what it declares with the line range of each — then read that \
             range, not the file. bennu_problems says what is currently wrong without a build. \
             bennu_class_index is Java-only and will be empty here."
                .to_string(),
        );
        return steps;
    }

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
    /// 1-based character column of the name, when it is known.
    ///
    /// Carried so a hit can be handed **straight** to `bennu_references` or `bennu_implementors`,
    /// which are addressed by position. Without it, finding a symbol by name and then asking about
    /// it meant opening the file to count columns — which is the step this whole pair exists to
    /// remove.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<i64>,
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
/// On a project a **language server** owns — Rust, TypeScript, Svelte — the same question is put
/// to the server's own workspace symbol search instead, so the answer is its index rather than
/// Bennu's. One caveat that is worth reading before concluding anything from an empty result: a
/// server answers **nothing** until it has loaded the project, and it loads it lazily. The `note`
/// says which of the two an empty answer is.
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

    // A project Bennu does not index itself is answered by whoever does. This used to return
    // nothing on a Cargo root and say so — which is honest and useless: "where does `MoleEntities`
    // live" is the question somebody asks ten times a day on a twenty-three crate workspace, and
    // without it the caret-addressed half of this toolset can only be reached from a position the
    // caller already has. That is a tool that starts where the problem is nearly solved.
    // A Maven root keeps Bennu's index even when a server happens to be running for some file
    // inside it: that index is what knows about the Struts and Spring halves, which no server does.
    if is_cargo_root(&args.root) {
        return find_symbol_via_server(&args.root, &args.query, args.kind.as_deref(), limit);
    }
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
                    // Bennu's class index records the declaration line and not its column.
                    column: None,
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
                    column: None,
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

/// The doc comment attached to the declaration starting at `from_line` (1-based), as its first
/// **paragraph**, markers stripped.
///
/// Why this is read out of the source rather than asked of the server: `documentSymbol` carries a
/// name, a kind and a `detail` — for rust-analyzer the signature — and **no documentation at all**.
/// The protocol puts docs in `hover`, which is one round trip per symbol; an outline of thirty
/// declarations would be thirty requests to answer a question about one file.
///
/// The first paragraph and not the whole comment, because an outline is a map and these are its
/// labels. In a codebase whose comments carry the reasoning, a full doc per entry is the file
/// again — and the convention in both Rust and Java is that the first paragraph is the summary,
/// so the cut lands where the author already put a break. The rest arrives with the declaration
/// when it is read.
///
/// **Both attachment shapes**, because which one applies depends on where the engine decided the
/// declaration starts and that is not worth depending on: if `from_line` is itself a doc line the
/// comment is read downwards, otherwise upwards from the line above. Attributes and annotations in
/// between are stepped over — `#[derive(Debug)]` and `@Override` sit between a doc and its item —
/// but a **blank** line is not, because in both languages a blank line detaches the comment.
fn doc_summary(source: &str, from_line: u32) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let idx = from_line.checked_sub(1)? as usize;
    let at = lines.get(idx)?;

    let mut collected: Vec<String> = Vec::new();
    if is_doc_line(at) {
        // The engine's range already begins at the comment: read forwards.
        for line in &lines[idx..] {
            match strip_doc(line) {
                Some(text) => collected.push(text),
                None => break,
            }
        }
    } else {
        // Above the declaration, past whatever decorates it.
        let mut i = idx;
        while i > 0 {
            let above = lines[i - 1].trim();
            if above.starts_with('#') || above.starts_with('@') || above.ends_with(',') {
                i -= 1;
                continue;
            }
            break;
        }
        while i > 0 {
            match strip_doc(lines[i - 1]) {
                Some(text) => {
                    collected.push(text);
                    i -= 1;
                }
                None => break,
            }
        }
        collected.reverse();
    }

    // The first paragraph: everything up to the first blank doc line.
    let paragraph: Vec<&String> = collected
        .iter()
        .skip_while(|l| l.trim().is_empty())
        .take_while(|l| !l.trim().is_empty())
        .collect();
    if paragraph.is_empty() {
        return None;
    }
    let mut text = paragraph.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ");
    // A paragraph that is itself an essay is cut rather than carried: the cap is what keeps an
    // outline an outline.
    const CAP: usize = 400;
    if text.chars().count() > CAP {
        text = text.chars().take(CAP - 1).collect::<String>() + "…";
    }
    Some(text)
}

/// Whether a line is part of a doc comment.
fn is_doc_line(line: &str) -> bool {
    strip_doc(line).is_some()
}

/// A doc line's text without its marker, or `None` when the line is not one.
///
/// `//` is deliberately **not** a doc marker in Rust — an ordinary comment above an item is not
/// attached to it and regularly says something about the line above instead. Java has no such
/// distinction, so `/** … */` is the only form taken there; a `//` above a method is a note, not
/// its documentation.
fn strip_doc(line: &str) -> Option<String> {
    let t = line.trim_start();
    for marker in ["///", "//!"] {
        if let Some(rest) = t.strip_prefix(marker) {
            return Some(rest.trim_start().to_string());
        }
    }
    if let Some(rest) = t.strip_prefix("/**") {
        return Some(rest.trim_start_matches('*').trim().trim_end_matches("*/").trim().to_string());
    }
    if t.starts_with("*/") {
        return Some(String::new());
    }
    // A continuation line of a block doc: ` * text`. Not a bare `*`, which is multiplication.
    if let Some(rest) = t.strip_prefix("* ") {
        return Some(rest.trim_end_matches("*/").trim_end().to_string());
    }
    if t == "*" {
        return Some(String::new());
    }
    None
}

#[cfg(test)]
mod doc_summary_tests {
    use super::doc_summary;

    #[test]
    fn a_rust_doc_above_a_declaration_is_found_past_its_attributes() {
        let src = "\
/// Applies the mole's animation for this frame.
///
/// The long explanation nobody wants in an outline.
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn apply_mole_anim() {}
";
        // Addressed at the `pub fn` line — the shape when the engine's range excludes the doc.
        assert_eq!(
            doc_summary(src, 6).as_deref(),
            Some("Applies the mole's animation for this frame."),
        );
    }

    #[test]
    fn a_range_that_already_starts_at_the_comment_reads_forwards() {
        let src = "\
/// Applies the mole's animation.
/// Second line of the same paragraph.
///
/// A second paragraph, left out.
pub fn apply_mole_anim() {}
";
        assert_eq!(
            doc_summary(src, 1).as_deref(),
            Some("Applies the mole's animation. Second line of the same paragraph."),
        );
    }

    #[test]
    fn a_javadoc_block_is_read_the_same_way() {
        let src = "\
    /**
     * Recalculates the order total.
     *
     * The rest of it.
     */
    @Override
    public BigDecimal total() {}
";
        assert_eq!(doc_summary(src, 7).as_deref(), Some("Recalculates the order total."));
    }

    #[test]
    fn a_comment_that_is_not_attached_is_not_this_declarations() {
        // A blank line detaches it in both languages, and taking it anyway would put the previous
        // item's explanation on this one — a wrong label is worse than none on a map.
        let src = "\
/// Belongs to something else.

pub fn apply_mole_anim() {}
";
        assert_eq!(doc_summary(src, 3), None);

        // A plain `//` above an item is a note about the code, not its documentation.
        let src = "\
// Bumped in the loop below.
pub fn apply_mole_anim() {}
";
        assert_eq!(doc_summary(src, 2), None);

        // Nothing above at all.
        assert_eq!(doc_summary("pub fn a() {}\n", 1), None);
    }

    #[test]
    fn a_read_by_symbol_starts_at_the_doc_and_at_the_attributes() {
        use super::doc_start;
        let src = "\
/// What it is for.
#[inline]
pub fn apply_mole_anim() {}
";
        // Line 3 is the `pub fn`. The range has to widen to line 1, or reading a declaration
        // returns the half of it that says the least.
        assert_eq!(doc_start(src, 3), 1);

        // No doc, but the attribute is still part of the declaration.
        let src = "#[inline]\npub fn a() {}\n";
        assert_eq!(doc_start(src, 2), 1);

        // Already at the comment: nothing to widen.
        let src = "/// Doc.\npub fn a() {}\n";
        assert_eq!(doc_start(src, 1), 1);

        // Detached by a blank line — the widening stops where the attachment does.
        let src = "/// Somebody else's.\n\npub fn a() {}\n";
        assert_eq!(doc_start(src, 3), 3);
    }

    #[test]
    fn an_essay_is_cut_rather_than_carried() {
        let long = "x".repeat(900);
        let src = format!("/// {long}\npub fn a() {{}}\n");
        let out = doc_summary(&src, 2).unwrap();
        assert!(out.chars().count() <= 400, "{}", out.chars().count());
        assert!(out.ends_with('…'));
    }
}

/// Args for [`bennu_outline`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OutlineArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Absolute path to the file to describe.
    pub file: String,
}

/// One declaration in a file's outline.
#[derive(Debug, Serialize)]
pub struct OutlineEntry {
    /// The name as declared.
    pub name: String,
    /// The engine's own word: `struct`, `function`, `field`, `impl`, `class`, `method`.
    pub kind: String,
    /// The signature or type, when the engine gave one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The **first paragraph** of the declaration's doc comment, when it has one.
    ///
    /// Here because a signature says what a thing takes and returns, and a doc comment says what
    /// it is for — and on a well-commented codebase the second is most of what an outline is
    /// worth reading for. The rest of the comment comes with the declaration when it is read; the
    /// summary is the label on the map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// 1-based line of the name.
    pub line: u32,
    /// 1-based column of the name — so an entry can be handed straight to a positional call.
    pub column: u32,
    /// 1-based first and last line of the whole declaration, body included. What to pass to
    /// `bennu_read_file` to read exactly this and nothing else.
    pub from_line: u32,
    pub to_line: u32,
    /// Nesting: `0` is top level, `1` is a member of the entry above it.
    pub depth: usize,
}

/// A file's shape.
#[derive(Debug, Serialize)]
pub struct OutlineResult {
    /// How many lines the file has, so the cost of reading it in full is visible.
    pub total_lines: u32,
    pub entries: Vec<OutlineEntry>,
    pub note: Option<String>,
}

/// List what a file declares, without reading it.
///
/// **The call to make before reading a file you do not know.** A two-thousand-line module is
/// thirty lines of outline, and each entry carries the line range of its own declaration — so the
/// next step is `bennu_read_file` with `symbol` or a range, not the whole file. Reading a file in
/// full to find out what is in it is the most expensive way to ask the cheapest question.
///
/// Each entry carries the **first paragraph of its doc comment**, which a language server's own
/// outline does not: the protocol keeps documentation in `hover`, one round trip per symbol. A
/// signature says what a thing takes; the comment says what it is for, and on a codebase that
/// explains itself that is most of what an outline is worth reading for.
///
/// Flattened rather than nested, with a `depth`, because that is what a caller scans; the nesting
/// is still legible and nothing has to be walked to count what is there.
#[arbor_rpc::handler(mcp(
    title = "List what a file declares",
    safety = read,
))]
fn bennu_outline(_ctx: &BennuState, args: OutlineArgs) -> Result<OutlineResult, String> {
    let source = read_source(&args.root, &args.file)?;
    let total_lines = source.lines().count() as u32;
    let tree = crate::lsp_route::document_symbols(&args.file, &source);

    let mut entries = Vec::new();
    flatten_outline(&tree, &source, 0, &mut entries);

    let note = match entries.is_empty() {
        true => Some(server_wait_note(&args.file).unwrap_or_else(|| {
            "Nothing was outlined. Either the file declares nothing, or the engine that would              know does not serve this file type."
                .to_string()
        })),
        false => None,
    };
    Ok(OutlineResult { total_lines, entries, note })
}

/// Walk a symbol tree into the flat list with a depth.
fn flatten_outline(
    nodes: &[bennu_proto::prelude::LspSymbol],
    source: &str,
    depth: usize,
    out: &mut Vec<OutlineEntry>,
) {
    for node in nodes {
        let (line, column) = line_col_of(source, node.name_start);
        let (from_line, _) = line_col_of(source, node.start);
        let (to_line, _) = line_col_of(source, node.end.saturating_sub(1).max(node.start));
        out.push(OutlineEntry {
            name: node.name.clone(),
            kind: node.kind.clone(),
            detail: node.detail.clone().filter(|d| !d.is_empty()),
            doc: doc_summary(source, from_line),
            line,
            column,
            from_line,
            to_line,
            depth,
        });
        flatten_outline(&node.children, source, depth + 1, out);
    }
}

/// Args for [`bennu_problems`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProblemsArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Keep only these severities: `error`, `warning`, `info`, `hint`. Omit for all.
    #[serde(default)]
    pub severity: Option<Vec<String>>,
    /// Cap on problems returned. Defaults to 200.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Everything wrong in one file.
#[derive(Debug, Serialize)]
pub struct FileProblems {
    pub file: String,
    /// How many this file has, before the cap.
    pub count: usize,
    pub problems: Vec<ProblemEntry>,
}

/// One reported problem.
#[derive(Debug, Serialize)]
pub struct ProblemEntry {
    pub severity: String,
    pub message: String,
    /// 1-based line, and column, of where it starts.
    pub line: u32,
    pub column: u32,
    /// The rule or error code, when there is one (`E0432`, `unused_imports`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// What a project currently reports as wrong.
#[derive(Debug, Serialize)]
pub struct ProblemsResult {
    pub files: Vec<FileProblems>,
    pub errors: usize,
    pub warnings: usize,
    pub total: usize,
    pub note: Option<String>,
}

/// Every problem the project's engine currently reports — **without building**.
///
/// The cheap "did I break anything". A language server has already run the project's own checker
/// and publishes as it finishes, including for files nobody has opened, so this is a read of an
/// answer that already exists rather than a build that produces one. `bennu_build` is still the
/// call when you want a compile; this is the one for after an edit.
///
/// **Only this project's own files.** A server reports on the whole crate graph it built, which
/// for a `path` dependency means files in another repository — real, and not yours to fix from
/// here.
///
/// An empty result is not proof of a clean project: a server that has not finished its first
/// check has published nothing yet, and the note says so when that is the case.
#[arbor_rpc::handler(mcp(
    title = "List the project's current problems",
    safety = read,
))]
fn bennu_problems(_ctx: &BennuState, args: ProblemsArgs) -> Result<ProblemsResult, String> {
    let limit = args.limit.unwrap_or(200).clamp(1, 2_000);
    let wanted: Option<Vec<String>> =
        args.severity.as_ref().map(|v| v.iter().map(|s| s.to_lowercase()).collect());

    let mut files = Vec::new();
    let (mut errors, mut warnings, mut total) = (0usize, 0usize, 0usize);
    let mut budget = limit;

    for fd in crate::lsp_route::problems(&args.root) {
        // The wire carries byte offsets and a caller counts lines. Read once per file that has
        // problems — which is exactly the set worth reading — and only when one survived the
        // severity filter, so asking for errors on a project full of warnings reads nothing.
        let mut text: Option<Option<String>> = None;
        let kept: Vec<ProblemEntry> = fd
            .diagnostics
            .into_iter()
            .filter(|d| wanted.as_ref().is_none_or(|w| w.contains(&d.severity.to_lowercase())))
            .map(|d| {
                match d.severity.as_str() {
                    "error" => errors += 1,
                    "warning" => warnings += 1,
                    _ => {}
                }
                let source = text.get_or_insert_with(|| read_source(&args.root, &fd.file).ok());
                let (line, column) = match source.as_deref() {
                    Some(source) => line_col_of(source, d.start),
                    // Unreadable — deleted since the server last spoke, or outside the encoding
                    // it was opened with. The problem is still worth reporting; only its
                    // coordinates are lost.
                    None => (0, 0),
                };
                ProblemEntry {
                    severity: d.severity,
                    message: d.message,
                    line,
                    column,
                    code: Some(d.code).filter(|c| !c.is_empty()),
                }
            })
            .collect();
        if kept.is_empty() {
            continue;
        }
        total += kept.len();
        let count = kept.len();
        let mut kept = kept;
        kept.truncate(budget);
        budget -= kept.len();
        files.push(FileProblems { file: fd.file, count, problems: kept });
    }

    // Heaviest first, like every other grouped answer here.
    files.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.file.cmp(&b.file)));

    let note = if total > limit {
        Some(format!("Showing {limit} of {total} problems. Narrow with `severity`, or raise `limit`."))
    } else if total == 0 {
        Some(
            "Nothing is currently reported. That is not the same as a clean build: a server that              has not finished its first check has published nothing yet — bennu_index_stats says              whether it is up."
                .to_string(),
        )
    } else {
        None
    };
    Ok(ProblemsResult { files, errors, warnings, total, note })
}

/// Args for [`bennu_callers`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CallersArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Absolute path to the file holding the function. Not needed when `symbol` is given.
    #[serde(default)]
    pub file: String,
    /// 1-based line of its name. Ignored when `symbol` is given.
    #[serde(default)]
    pub line: u32,
    /// 1-based character column. Any column within the identifier works.
    #[serde(default)]
    pub column: Option<u32>,
    /// Address it **by name** instead: `MoleAnim.apply` for a method of a type.
    #[serde(default)]
    pub symbol: Option<String>,
    /// How many levels of caller to walk. `1` is the direct callers; `2` is their callers too.
    /// Defaults to 2, capped at 5.
    ///
    /// Two is the default because it is the depth that answers the question this exists for —
    /// "is this reached from that command" — while one level answers only "who calls it", which
    /// is what `bennu_references` already says.
    #[serde(default)]
    pub depth: Option<usize>,
    /// Stop as soon as a caller's name contains this, case-insensitively, and report the chain
    /// that got there. The direct way to ask "is this reachable from X".
    #[serde(default)]
    pub reaches: Option<String>,
    /// Cap on nodes visited. Defaults to 200.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// One function in the caller tree.
#[derive(Debug, Serialize)]
pub struct CallerNode {
    pub name: String,
    /// The server's word for what it is (`function`, `method`).
    pub kind: String,
    pub file: String,
    /// 1-based line of the caller's own declaration.
    pub line: u32,
    /// How many steps from the function asked about. `1` is a direct caller.
    pub depth: usize,
    /// The chain from the function asked about up to this one, outermost last —
    /// `["apply_mole_anim", "tick_moles", "on_key_6"]`. The answer to "how is this reached",
    /// which a flat list of callers cannot give.
    pub via: Vec<String>,
}

/// Who calls a function, transitively.
#[derive(Debug, Serialize)]
pub struct CallersResult {
    /// What the position or name resolved to.
    pub target: String,
    /// Every caller found, nearest first.
    pub callers: Vec<CallerNode>,
    pub total: usize,
    /// When `reaches` was given: the chain that got there, or absent if nothing did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reached_by: Option<Vec<String>>,
    /// Why the answer is empty, short, or stopped early.
    pub note: Option<String>,
}

/// Who calls this function — **transitively**, to a given depth.
///
/// The question a flat find-usages cannot answer: not "who calls `apply_mole_anim`" but "is
/// `apply_mole_anim` reached from the ⌘6 handler". One level of callers is a list of names you
/// then have to look up one at a time; two levels is usually the whole answer, and every result
/// carries the **chain** that reached it rather than only its own name.
///
/// `reaches` asks it directly: give a name and the search stops at the first caller matching it,
/// reporting the path. Absent, it returns the whole tree to `depth`.
///
/// Answered by the language server's call hierarchy, so it follows calls rather than text — a
/// function reached through a trait object is found, and a comment naming it is not.
#[arbor_rpc::handler(mcp(
    title = "Find who calls a function, transitively",
    safety = read,
))]
fn bennu_callers(_ctx: &BennuState, args: CallersArgs) -> Result<CallersResult, String> {
    let (file, line, column) = resolve_position(
        &args.root,
        args.symbol.as_deref(),
        &args.file,
        args.line,
        args.column.unwrap_or(1),
    )?;
    let source = read_source(&args.root, &file)?;
    let (offset, line_text) = offset_of(&source, line, column)?;
    let depth = args.depth.unwrap_or(2).clamp(1, 5);
    let limit = args.limit.unwrap_or(200).clamp(1, 2_000);

    let Some(root_item) = crate::lsp_route::prepare_hierarchy(&file, &source, offset, true)
        .into_iter()
        .next()
    else {
        return Ok(CallersResult {
            target: String::new(),
            callers: Vec::new(),
            total: 0,
            reached_by: None,
            note: Some(server_wait_note(&file).unwrap_or_else(|| {
                format!(
                    "Nothing callable at line {line} column {column} — the position may not be on \
                     a function. The line reads: {}",
                    line_text.trim(),
                )
            })),
        });
    };

    let target = root_item.name.clone();
    let wanted = args.reaches.as_deref().map(str::to_lowercase);
    let mut out: Vec<CallerNode> = Vec::new();
    let mut reached_by = None;
    // Cycles are ordinary in a call graph — mutual recursion, a trait method calling itself
    // through a default — so a visited set is not an optimisation here, it is what terminates.
    let mut seen: std::collections::HashSet<(String, usize)> = std::collections::HashSet::new();
    let mut frontier = vec![(root_item, vec![target.clone()])];
    let mut visited = 0usize;
    let mut capped = false;

    'walk: for level in 1..=depth {
        let mut next = Vec::new();
        for (item, chain) in frontier {
            if visited >= limit {
                capped = true;
                break 'walk;
            }
            visited += 1;
            for caller in crate::lsp_route::hierarchy_step(&args.root, item.handle, "incoming") {
                if !seen.insert((caller.file.clone(), caller.start)) {
                    continue;
                }
                let mut here = chain.clone();
                here.push(caller.name.clone());
                out.push(CallerNode {
                    name: caller.name.clone(),
                    kind: caller.kind.clone(),
                    file: caller.file.clone(),
                    line: caller.line as u32,
                    depth: level,
                    via: here.clone(),
                });
                if wanted.as_ref().is_some_and(|w| caller.name.to_lowercase().contains(w)) {
                    reached_by = Some(here.clone());
                    break 'walk;
                }
                next.push((caller, here));
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }

    let total = out.len();
    let note = if reached_by.is_some() {
        None
    } else if let Some(w) = &args.reaches {
        Some(format!(
            "Nothing matching `{w}` calls `{target}` within {depth} level(s). Raise `depth`, or \
             it is genuinely not reached that way.",
        ))
    } else if capped {
        Some(format!(
            "Stopped after {limit} nodes — the tree is wider than the cap, so this is a partial \
             answer. Narrow it with `reaches`, or raise `limit`.",
        ))
    } else if total == 0 {
        Some(server_wait_note(&file).unwrap_or_else(|| {
            format!("Nothing calls `{target}` — it may be an entry point, or called dynamically.")
        }))
    } else {
        None
    };

    Ok(CallersResult { target, callers: out, total, reached_by, note })
}

/// A position, from either address: a `symbol` name or an explicit file/line/column.
///
/// Extracted the moment a second tool needed it. The by-name half is the one that matters — a
/// field or a method is named everywhere else, and requiring a line and a column here is what put
/// a `grep` in front of every one of these calls.
fn resolve_position(
    root: &str,
    symbol: Option<&str>,
    file: &str,
    line: u32,
    column: u32,
) -> Result<(String, u32, u32), String> {
    let Some(name) = symbol.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok((file.to_string(), line, column));
    };
    let Some((owner, member)) = split_qualified(name) else {
        return Err(format!(
            "`{name}` is not a qualified member name. Write it as `Owner.member` (or \
             `Owner::member`), or address the symbol by file, line and column."
        ));
    };
    let Some(hit) = resolve_member(root, owner, member) else {
        return Err(format!(
            "`{name}` did not resolve. Either `{owner}` is not a type this project declares, or \
             it has no member called `{member}` — check with bennu_find_symbol, whose note says \
             whether the server is up."
        ));
    };
    Ok((
        hit.file.unwrap_or_default(),
        hit.line.unwrap_or(1) as u32,
        hit.column.unwrap_or(1) as u32,
    ))
}

/// Args for [`bennu_implementors`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImplementorsArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Absolute path to the file holding the trait, interface or method.
    pub file: String,
    /// 1-based line of its name.
    pub line: u32,
    /// 1-based character column. Any column within the identifier works.
    #[serde(default)]
    pub column: Option<u32>,
    /// Cap on the results. Defaults to 200.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Who implements a trait, an interface, or an abstract method.
#[derive(Debug, Serialize)]
pub struct ImplementorsResult {
    /// What the position resolved to, in the engine's own words.
    pub target: String,
    /// The implementing sites, grouped by file, heaviest first.
    pub files: Vec<UsageFile>,
    pub total: usize,
    /// Why the answer is empty or short, when it is.
    pub note: Option<String>,
}

/// Find every type that implements the trait or interface at a position.
///
/// The **reverse** of go-to-definition, and the question the other tools here cannot be asked.
/// `bennu_references` on a trait finds the places its name is written — the `impl` headers, the
/// bounds, the imports — mixed together. This finds the implementations and only those, which is
/// what "who would break if I add a method to this" actually means.
///
/// Also answers it for a **method**: given a trait method, the overrides of it.
///
/// Answered by the language server, so it is exact rather than textual — a `impl Trait for Foo`
/// written through a type alias is still found, and a comment mentioning the name is not.
#[arbor_rpc::handler(mcp(
    title = "Find who implements a trait or interface",
    safety = read,
))]
fn bennu_implementors(
    _ctx: &BennuState,
    args: ImplementorsArgs,
) -> Result<ImplementorsResult, String> {
    let source = read_source(&args.root, &args.file)?;
    let column = args.column.unwrap_or(1);
    let (offset, line_text) = offset_of(&source, args.line, column)?;
    let limit = args.limit.unwrap_or(200).clamp(1, 2_000);

    let Some(found) = crate::lsp_route::implementations(&args.file, &source, offset) else {
        // No server owns this file. Said plainly rather than as an empty list: Bennu's own Java
        // engine has no implementations query, so an empty result here would be a claim about the
        // code that nothing actually checked.
        return Ok(ImplementorsResult {
            target: String::new(),
            files: Vec::new(),
            total: 0,
            note: Some(
                "This question is answered by a language server, and none serves this file. \
                 Nothing was checked — this is not a statement about the code."
                    .to_string(),
            ),
        });
    };

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
        // Every one of these IS an implementation; the field is carried so the shape matches
        // `bennu_references` and a caller can hand either result to the same code.
        let site = UsageSite { line, column, preview: hit.preview, kind: UsageKind::DECL };
        match by_file.iter_mut().find(|(file, _)| *file == hit.file) {
            Some((_, sites)) => sites.push(site),
            None => by_file.push((hit.file, vec![site])),
        }
    }

    let total: usize = by_file.iter().map(|(_, sites)| sites.len()).sum();
    by_file.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));

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
        Some(format!("Showing {limit} of {total} implementations. Pass a larger limit for the rest."))
    } else if total == 0 {
        Some(server_wait_note(&args.file).unwrap_or_else(|| {
            format!(
                "Nothing implements what is at line {} column {} — or that position is not a \
                 trait, an interface or an overridable method. The line reads: {}",
                args.line,
                column,
                line_text.trim(),
            )
        }))
    } else {
        None
    };

    Ok(ImplementorsResult { target: found.target_label, files, total, note })
}

/// A name written as `Owner.member` / `Owner::member`, split into its two halves.
///
/// `None` when there is no separator — a bare name is not a qualified one, and treating the last
/// segment of `crate::field::FieldCrystal` as a member would resolve the wrong thing entirely.
/// So a path is only read as qualified when it has **exactly one** separator, which is what
/// somebody writes when they mean "this field of that type".
fn split_qualified(query: &str) -> Option<(&str, &str)> {
    let q = query.trim();
    let parts: Vec<&str> = match q.contains("::") {
        true => q.split("::").collect(),
        false => q.split('.').collect(),
    };
    match parts.as_slice() {
        [owner, member] if !owner.is_empty() && !member.is_empty() => Some((owner, member)),
        _ => None,
    }
}

/// Find `member` inside `owner`, and say exactly where its name is written.
///
/// The gap this closes, in the words of the person who hit it four times in one session: asking
/// "who reads this field, now that it exists" is a `bennu_references` call, and `bennu_references`
/// wants a line and a column — so every time it began with a `grep` to find the line. A field is
/// addressed by its name in every other context; it should be here too.
///
/// Two steps, because that is what the protocol offers. The owner is found by workspace search;
/// its **document symbols** are then walked for a child of that name. The second step is what
/// makes this exact rather than a guess: a `hue` field of `FieldCrystal` and a `hue` of `Palette`
/// are two symbols with one name, and only the tree knows which is inside which.
fn resolve_member(root: &str, owner: &str, member: &str) -> Option<SymbolHit> {
    let owners: Vec<bennu_proto::prelude::LspSymbol> = crate::lsp_route::workspace_symbols(root, owner)
        .into_iter()
        .filter(|s| s.name == owner)
        .collect();

    for candidate in owners {
        let Ok(text) = read_source(root, &candidate.file) else { continue };
        let tree = crate::lsp_route::document_symbols(&candidate.file, &text);
        let Some(node) = find_in_tree(&tree, owner) else { continue };
        let Some(field) = node.children.iter().find(|c| c.name == member) else { continue };
        let (line, column) = line_col_of(&text, field.name_start);
        return Some(SymbolHit {
            kind: "member".to_string(),
            name: format!("{owner}.{member}"),
            detail: match field.detail.clone().filter(|d| !d.is_empty()) {
                Some(detail) => format!("{} · {detail}", field.kind),
                None => field.kind.clone(),
            },
            file: Some(candidate.file.clone()),
            line: Some(line as i64),
            column: Some(column as i64),
        });
    }
    None
}

/// The 1-based line range of the declaration `name` in `file`, from the file's own symbol tree.
///
/// `name` is `Owner.member` or a bare type / function name. The range is the **whole**
/// declaration — body included, and whatever the server counted as belonging to it, which for
/// rust-analyzer includes the doc comment above it.
///
/// Exists so a file can be read one declaration at a time. Without it, "show me `apply_mole_anim`"
/// is a whole-file read followed by the caller counting lines — which is the file spent to learn a
/// twentieth of it, every time.
pub(crate) fn declaration_lines(file: &str, source: &str, name: &str) -> Option<(u32, u32)> {
    let tree = crate::lsp_route::document_symbols(file, source);
    let node = match split_qualified(name) {
        Some((owner, member)) => {
            find_in_tree(&tree, owner)?.children.iter().find(|c| c.name == member)?
        }
        None => find_in_tree(&tree, name)?,
    };
    let (from, _) = line_col_of(source, node.start);
    let (to, _) = line_col_of(source, node.end.saturating_sub(1).max(node.start));
    // Widened upwards over the doc comment when the engine's range starts below it. Reading a
    // declaration without the paragraph that says why it is the way it is means reading the half
    // that a careful codebase puts the least information in.
    Some((doc_start(source, from), to))
}

/// The first line of the doc comment attached to a declaration starting at `line`, or `line`
/// itself when there is none. Attributes and annotations are stepped over; a blank line is not.
fn doc_start(source: &str, line: u32) -> u32 {
    let lines: Vec<&str> = source.lines().collect();
    let mut i = line.saturating_sub(1) as usize;
    if lines.get(i).is_some_and(|l| is_doc_line(l)) {
        return line;
    }
    while i > 0 {
        let above = lines[i - 1].trim();
        if above.starts_with('#') || above.starts_with('@') || above.ends_with(',') {
            i -= 1;
            continue;
        }
        break;
    }
    let after_decoration = i;
    while i > 0 && is_doc_line(lines[i - 1]) {
        i -= 1;
    }
    match i < after_decoration {
        true => (i + 1) as u32,
        // No doc: the decoration is part of the declaration and worth keeping, so the range
        // starts where the attributes do rather than where the engine put it.
        false => (after_decoration + 1) as u32,
    }
}

/// The node named `name` anywhere in a document-symbol tree, outermost first.
fn find_in_tree<'a>(
    nodes: &'a [bennu_proto::prelude::LspSymbol],
    name: &str,
) -> Option<&'a bennu_proto::prelude::LspSymbol> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(found) = find_in_tree(&node.children, name) {
            return Some(found);
        }
    }
    None
}

/// The kinds a language server calls a **type**, so `kind: "type"` means the same thing whichever
/// engine answered. Everything else it reports is filed as a member.
const SERVER_TYPE_KINDS: &[&str] = &[
    "class", "struct", "enum", "interface", "trait", "object", "namespace", "module",
    "type parameter", "type alias", "impl",
];

/// [`bennu_find_symbol`] for a project a language server owns.
///
/// The server's `workspace/symbol`, mapped onto the same result shape Bennu's index produces —
/// so a caller writes one call and does not branch on the project kind. The vocabulary in
/// `detail` stays the server's, because a Rust `fn` signature is more useful verbatim than
/// translated into Java's words.
///
/// **An empty answer here has two meanings and they need different responses**, which is the
/// whole reason this does not just return a list. A server that has not loaded the project
/// answers nothing at all — measured, not assumed: `workspace/symbol` on a cold `svelteserver`
/// returns zero for a name that is in the tree, and starts answering once a file has been opened.
/// Reporting that as "no such symbol" is how a caller concludes something does not exist and goes
/// and greps a project that could have answered.
fn find_symbol_via_server(
    root: &str,
    query: &str,
    kind: Option<&str>,
    limit: usize,
) -> Result<FindSymbolResult, String> {
    let want_types = !matches!(kind, Some("member"));
    let want_members = !matches!(kind, Some("type"));
    let needle = query.trim().to_lowercase();

    // `SavedWorld.extra_moles` — a member named by its owner. Answered exactly, from the owner's
    // symbol tree, rather than by matching the bare name across the workspace: two structs with a
    // `hue` field are two symbols with one name, and a substring search cannot tell them apart.
    if want_members {
        if let Some((owner, member)) = split_qualified(query) {
            if let Some(hit) = resolve_member(root, owner, member) {
                return Ok(FindSymbolResult { hits: vec![hit], total: 1, note: None });
            }
            // Fall through to the ordinary search rather than returning empty: the query may be a
            // path (`crate::field::Foo`) that happens to have one separator, and the bare-name
            // search will find it.
        }
    }

    let mut hits: Vec<SymbolHit> = crate::lsp_route::workspace_symbols(root, query.trim())
        .into_iter()
        .filter_map(|s| {
            let is_type = SERVER_TYPE_KINDS.contains(&s.kind.as_str());
            if (is_type && !want_types) || (!is_type && !want_members) {
                return None;
            }
            Some(SymbolHit {
                kind: match is_type {
                    true => "type".to_string(),
                    false => "member".to_string(),
                },
                name: s.name,
                // The server's own words for what it is, plus its signature when it gave one.
                // Both, because `struct` and `fn(&self) -> Duration` answer different halves of
                // "is this the one I meant".
                detail: match s.detail.filter(|d| !d.is_empty()) {
                    Some(detail) => format!("{} · {detail}", s.kind),
                    None => s.kind,
                },
                file: Some(s.file),
                line: Some(s.line as i64),
                // The server reports the name's own column, which is what makes a hit here
                // directly usable as the argument to a positional call.
                column: Some(s.col as i64),
            })
        })
        .collect();

    // Same ordering rule as the Java path: an exact name beats a substring of a longer one.
    hits.sort_by_key(|h| (h.name.to_lowercase() != needle, h.name.len(), h.name.to_lowercase()));

    let total = hits.len();
    let server = server_for_root(root);
    let note = if total > limit {
        Some(format!(
            "Showing {limit} of {total} matches. Narrow the query, or pass a larger limit."
        ))
    } else if total == 0 {
        Some(match &server {
            Some(s) if s.state == "ready" => format!(
                "{} is up and has nothing by that name. This is an answer.",
                s.name,
            ),
            Some(s) => format!(
                "{} is {} — it answers nothing at all until it has loaded the project, so this is \
                 not an answer yet.{} Ask again in a few seconds.",
                s.name,
                s.state,
                match s.progress.is_empty() {
                    true => String::new(),
                    false => format!(" ({})", s.progress),
                },
            ),
            None => "No language server is running for this project, and one is what answers this \
                     question here. It starts on the first request about a source file — ask \
                     bennu_symbol_at about any position in one, then repeat this."
                .to_string(),
        })
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
    /// Not needed when `symbol` is given.
    #[serde(default)]
    pub file: String,
    /// 1-based line of the symbol. Ignored when `symbol` is given.
    #[serde(default)]
    pub line: u32,
    /// 1-based character column. Any column within the identifier works.
    #[serde(default)]
    pub column: Option<u32>,
    /// Address the symbol **by name** instead: `SavedWorld.extra_moles` for a field or method of
    /// a type, resolved against that type's own symbol tree.
    ///
    /// Here because a field is named everywhere else and was addressable only by position here —
    /// so "who reads this field, now that it exists" began with a `grep` to find the line. When
    /// this is given, `file`, `line` and `column` are not needed and are ignored.
    #[serde(default)]
    pub symbol: Option<String>,
    /// Cap on the use sites returned. Defaults to 200.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Keep only occurrences of these kinds: `decl`, `import`, `call`, `construct`, `read`.
    /// Omit for all of them. An unknown name here matches nothing rather than being ignored, so a
    /// typo shows as an empty result instead of a silently unfiltered one.
    ///
    /// The one that earns its place is `construct`: "how many places build a `FieldCrystal`" is
    /// the question asked before adding a field to it, and in an unfiltered list it is buried
    /// under the imports. `kind: ["call", "construct"]` before changing a signature leaves
    /// exactly the sites that have to change.
    #[serde(default)]
    pub kind: Option<Vec<String>>,
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
    /// What this occurrence *is*: see [`UsageKind`]. Read it before reading the previews — on a
    /// list of thirteen, the three that are imports are noise for somebody changing a signature.
    pub kind: &'static str,
}

/// What an occurrence of a name is doing there.
///
/// **Read how much of this is certain**, because the answer differs per variant and a caller
/// deciding whether to open a file deserves to know which it is trusting.
///
/// - `decl` is **exact**: the declaration is asked for by name (`textDocument/definition`, or
///   Bennu's own resolver) and matched by position. No guessing.
/// - `import` is as near certain as a line shape gets: a Rust `use` / `pub use`, a Java `import`,
///   a TypeScript `import` / `export … from`. These begin a line and nothing else does.
/// - `call`, `construct` and `read` are read off the **character after the name**, which is right
///   almost always and wrong visibly: the preview line is right there beside it. `construct` is
///   the one worth the separate name — a `FieldCrystal { … }` literal is the thing you count when
///   you are about to add a field, and it is invisible in a list where it reads as a call.
///
/// A string, not an enum, on the wire: the caller is a language model reading JSON, and
/// `"import"` needs no schema to understand.
pub struct UsageKind;

impl UsageKind {
    pub const DECL: &'static str = "decl";
    pub const IMPORT: &'static str = "import";
    pub const CALL: &'static str = "call";
    pub const CONSTRUCT: &'static str = "construct";
    pub const READ: &'static str = "read";
}

/// Whether two paths name the same file, tolerating the separator each engine happens to use.
fn same_path(a: &str, b: &str) -> bool {
    a.replace('\\', "/") == b.replace('\\', "/")
}

/// The full line containing `start`, and the text between `start` and `end` — the occurrence's
/// own line and its own name, which is what the classifier needs and what a trimmed preview has
/// already thrown away.
fn line_and_name(text: &str, start: usize, end: usize) -> (String, String) {
    let from = text[..start.min(text.len())].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let to = text[start.min(text.len())..]
        .find('\n')
        .map(|i| start + i)
        .unwrap_or(text.len());
    let name = text.get(start..end.min(text.len())).unwrap_or_default().to_string();
    (text[from..to].to_string(), name)
}

/// Classify one occurrence from its line and where the name sits in it.
///
/// `name_at` is a **character** index into `line`, matching the column the site carries.
fn usage_kind(line: &str, name_at: usize, name: &str, is_decl: bool) -> &'static str {
    if is_decl {
        return UsageKind::DECL;
    }
    let trimmed = line.trim_start();
    // An import line, in the three languages that reach here. Anchored at the start of the line,
    // which is what makes it safe: `use` inside an expression is not at column zero, and a doc
    // comment mentioning "import" is not either.
    for head in ["use ", "pub use ", "import ", "export ", "from "] {
        if trimmed.starts_with(head) {
            return UsageKind::IMPORT;
        }
    }

    // What follows the name decides the rest. Whitespace is skipped first: `Foo   {` and `foo (`
    // are the same thing written with more room.
    let after: String = line.chars().skip(name_at + name.chars().count()).collect();
    let after = after.trim_start();

    // A path segment — `FieldCrystal::new()`. The call is reached *through* this name, and that
    // is worth counting with the calls rather than with the bare reads: `Type::new()` is one of
    // the places a value is built, which is the question this classification exists to answer.
    if let Some(rest) = after.strip_prefix("::") {
        let rest = rest.trim_start();
        // A turbofish sits between the name and the segment: `Vec::<u8>::new()`.
        let rest = match rest.strip_prefix('<').and_then(|r| r.find('>').map(|i| &r[i + 1..])) {
            Some(after_generics) => after_generics.trim_start().strip_prefix("::").unwrap_or(after_generics).trim_start(),
            None => rest,
        };
        let ident_end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(rest.len());
        return match rest[ident_end..].trim_start().chars().next() {
            Some('(') => UsageKind::CALL,
            Some('{') => UsageKind::CONSTRUCT,
            _ => UsageKind::READ,
        };
    }

    match after.chars().next() {
        Some('(') => UsageKind::CALL,
        // A struct literal, and the one this exists for. `if x {` cannot reach here: the name
        // would have to be the last token before the brace, and a keyword is not the name asked
        // about.
        Some('{') => UsageKind::CONSTRUCT,
        _ => UsageKind::READ,
    }
}

#[cfg(test)]
mod qualified_name_tests {
    use super::split_qualified;

    #[test]
    fn a_member_is_named_by_its_owner_and_nothing_else_is() {
        // What somebody writes when they mean "this field of that type".
        assert_eq!(split_qualified("SavedWorld.extra_moles"), Some(("SavedWorld", "extra_moles")));
        assert_eq!(split_qualified("SavedWorld::extra_moles"), Some(("SavedWorld", "extra_moles")));
        assert_eq!(split_qualified("  Palette.hue  "), Some(("Palette", "hue")));

        // A bare name is not qualified.
        assert_eq!(split_qualified("extra_moles"), None);
        // …and neither is a PATH, which is the case that would have resolved the wrong thing:
        // the last segment of a module path is a type, not a member of the one before it.
        assert_eq!(split_qualified("crate::field::FieldCrystal"), None);
        assert_eq!(split_qualified("com.acme.order.Order"), None);
        // Malformed halves resolve to nothing rather than to an empty owner.
        assert_eq!(split_qualified(".hue"), None);
        assert_eq!(split_qualified("Palette."), None);
    }
}

#[cfg(test)]
mod usage_kind_tests {
    use super::{usage_kind, UsageKind};

    #[test]
    fn an_import_is_told_from_a_use() {
        // The three that were noise in a list of thirteen.
        assert_eq!(usage_kind("use crate::field::FieldCrystal;", 12, "FieldCrystal", false), UsageKind::IMPORT);
        assert_eq!(usage_kind("pub use super::FieldCrystal;", 15, "FieldCrystal", false), UsageKind::IMPORT);
        assert_eq!(usage_kind("import com.acme.Order;", 16, "Order", false), UsageKind::IMPORT);
        // …and a `use` that is not the head of the line is not an import.
        assert_eq!(usage_kind("    let f = FieldCrystal::new();", 12, "FieldCrystal", false), UsageKind::CALL);
    }

    #[test]
    fn a_literal_construction_is_its_own_answer() {
        // The question that took counting fourteen sites by hand: which of these build one.
        assert_eq!(
            usage_kind("    let c = FieldCrystal { hue: 3, size: 1 };", 12, "FieldCrystal", false),
            UsageKind::CONSTRUCT,
        );
        assert_eq!(usage_kind("        FieldCrystal {", 8, "FieldCrystal", false), UsageKind::CONSTRUCT);
    }

    #[test]
    fn a_call_survives_the_things_that_sit_between_the_name_and_the_paren() {
        assert_eq!(usage_kind("    tick(dt);", 4, "tick", false), UsageKind::CALL);
        assert_eq!(usage_kind("    tick (dt);", 4, "tick", false), UsageKind::CALL);
        assert_eq!(usage_kind("    let v = Vec::<u8>::new();", 12, "Vec", false), UsageKind::CALL);
    }

    #[test]
    fn everything_else_is_a_read_and_the_declaration_is_exact() {
        assert_eq!(usage_kind("    let n = crystal.hue;", 20, "hue", false), UsageKind::READ);
        assert_eq!(usage_kind("    let t = &self.tick;", 17, "tick", false), UsageKind::READ);

        // A declaration is asked for, never guessed. Worth an assertion because the shape of one
        // — `fn tick(` — is indistinguishable from a call by anything a line can say, and the
        // only reason that never bites is that the declaration's position is known exactly.
        assert_eq!(usage_kind("    fn tick(&self) {}", 7, "tick", true), UsageKind::DECL);
        assert_eq!(usage_kind("    fn tick(&self) {}", 7, "tick", false), UsageKind::CALL);
    }
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
///
/// **A field or method can be named instead of pointed at**: `symbol: "SavedWorld.extra_moles"`
/// resolves against that type's own symbol tree, so asking "who reads this field" no longer starts
/// with finding its line.
///
/// **Every site says what it is** — `decl`, `import`, `call`, `construct`, `read` — and `kind`
/// filters on it. That is the difference between reading thirteen lines and reading the four that
/// matter: before changing a signature, `kind: ["call", "construct"]`; before adding a field to a
/// struct, `kind: ["construct"]` counts the literals that will stop compiling.
#[arbor_rpc::handler(mcp(
    name = "bennu_references",
    title = "Find where a symbol is used",
    safety = read,
))]
fn bennu_references_at(
    ctx: &BennuState,
    args: ReferencesAtArgs,
) -> Result<ReferencesForAgent, String> {
    // A name, resolved to a position, before anything positional happens. The rest of this
    // function then has one kind of input and does not branch again.
    let (file, line, column) = resolve_position(
        &args.root,
        args.symbol.as_deref(),
        &args.file,
        args.line,
        args.column.unwrap_or(1),
    )?;
    let args = ReferencesAtArgs { file: file.clone(), line, column: Some(column), ..args };

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

    // Where the symbol is DECLARED, so exactly one of the sites below can be marked as such
    // rather than guessed at. Asked once, by the same routing the lookup used; `None` is a
    // perfectly ordinary answer (a local, or an engine with nothing to say) and simply means no
    // site is marked.
    let declared = crate::lsp_route::declaration(&args.file, &source, offset)
        .unwrap_or_else(|| IndexService::global().declaration(&args.file, &source, offset));

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
        // Position equality, not name equality: two symbols can share a name and only one of
        // them is this one's declaration.
        let is_decl = declared
            .as_ref()
            .is_some_and(|d| same_path(&d.file, &hit.file) && d.start == hit.start);
        // The RAW line, because `preview` is trimmed and the column indexes the original. Falling
        // back to the preview costs the classifier its offsets, so it is given the name's own
        // position inside it instead of a column that no longer means anything.
        let kind = match text.as_deref() {
            Some(text) => {
                let (raw, name) = line_and_name(text, hit.start, hit.end);
                usage_kind(&raw, column.saturating_sub(1) as usize, &name, is_decl)
            }
            None => match is_decl {
                true => UsageKind::DECL,
                false => UsageKind::READ,
            },
        };
        if let Some(wanted) = &args.kind {
            if !wanted.iter().any(|k| k == kind) {
                continue;
            }
        }
        let site = UsageSite { line, column, preview: hit.preview, kind };
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
    } else if total == 0 && args.kind.is_some() {
        // The filter is the answer here, and saying "nothing uses it" would be a claim about the
        // code that the caller's own argument caused.
        Some(format!(
            "`{}` is used, but no occurrence is of kind {:?}. Drop the `kind` filter to see them \
             all.",
            found.target_label,
            args.kind.as_ref().unwrap(),
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
pub(crate) fn is_cargo_root(root: &str) -> bool {
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
    /// Give up after this many seconds and report what had finished. Default 600, capped
    /// at 3600. Lower it when you are running one crate and a hang would be a finding.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// How long a waited-on run may take before the watchdog stops it.
///
/// Ten minutes is above a real workspace run and below "nobody is still reading". It is a
/// ceiling on the *call*, not a promise about the tests: a run that hits it comes back
/// saying it was stopped, which is the one thing a silent hang cannot do.
const DEFAULT_TEST_TIMEOUT_SECS: u64 = 600;
/// The most a caller may ask for. Past an hour the answer is not "wait longer", it is
/// "run less".
const MAX_TEST_TIMEOUT_SECS: u64 = 3_600;

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
///
/// It always comes back. A test that never terminates would otherwise hold the call open
/// for as long as anyone is willing to wait, which is indistinguishable from the tool being
/// broken; instead the run is stopped after `timeout_seconds` (600 by default) and the
/// answer says so, with whatever had finished by then. A stopped run is never reported as a
/// pass.
#[arbor_rpc::handler(mcp(
    title = "Run the tests and report what failed",
    safety = destructive,
))]
fn bennu_test_run(
    ctx: &BennuState,
    args: RunTestsAtArgs,
) -> Result<crate::test_report::TestRunReport, String> {
    let limit = std::time::Duration::from_secs(
        args.timeout_seconds.unwrap_or(DEFAULT_TEST_TIMEOUT_SECS).clamp(1, MAX_TEST_TIMEOUT_SECS),
    );

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
        let handle = run.handle();
        // Armed before the wait, disarmed after it: the window in which a hang could hold
        // this call open is exactly the window the watchdog covers.
        let deadline = crate::tests::Deadline::arm(handle.run_id, limit);
        let collector = std::sync::Arc::new(crate::test_report::Collector::default());
        let end = run.drive(Some(collector.clone()));
        let mut report = report_of(&collector, "cargo", end, handle.widened, args.tests.len());
        note_deadline(&mut report, deadline.disarm());
        return Ok(report);
    }

    let (scope, widened) = maven_scope(&args.tests, args.module.as_deref());
    let run = crate::tests::start_maven_run(
        ctx,
        &crate::tests::RunTestsArgs { root: args.root.clone(), scope },
    )?;
    // The plan's own widening (a selection too long for one command line) matters more than
    // ours, and both must reach the caller: a run that quietly ran more than it was asked to
    // is a run whose green is about something else.
    let handle = run.handle();
    let widened = handle.widened.or(widened);
    let deadline = crate::tests::Deadline::arm(handle.run_id, limit);
    let collector = crate::test_report::Collector::default();
    let end = run.drive(Some(&collector));
    let mut report = report_of(&collector, "maven", end, widened, args.tests.len());
    note_deadline(&mut report, deadline.disarm());
    Ok(report)
}

/// Say, in the report, that the run was stopped rather than finished.
///
/// It goes first in the note because it changes what every other number means: the counts
/// are what had been reported when the run was killed, not what the project has.
fn note_deadline(report: &mut crate::test_report::TestRunReport, fired: Option<std::time::Duration>) {
    let Some(limit) = fired else { return };
    let notice = format!(
        "The run was stopped after {}s and did not finish — the counts below are what had \
         reported by then, not the project's. Something is hanging, or the selection is too \
         big for this timeout: name one crate or module, or raise `timeout_seconds`.",
        limit.as_secs()
    );
    report.note = Some(match report.note.take() {
        Some(existing) => format!("{notice} {existing}"),
        None => notice,
    });
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
mod next_steps_tests {
    use super::next_steps;
    use bennu_proto::prelude::{IndexStats, LspStatus};

    /// A Cargo root's index stats, and they never change: there is no Java index to build, so
    /// this is exactly what the summary reported for ever.
    fn stats() -> IndexStats {
        IndexStats {
            types: 0,
            members: 0,
            jdk_version: String::new(),
            jar_count: 0,
            actions: 0,
            beans: 0,
            relations: 0,
            ready: false,
            engine: String::new(),
        }
    }

    fn server(state: &str) -> LspStatus {
        LspStatus {
            id: "rust-analyzer".into(),
            name: "rust-analyzer".into(),
            language: "rust".into(),
            root: "/w/geode".into(),
            command: "rust-analyzer".into(),
            version: None,
            state: state.into(),
            message: String::new(),
            progress: String::new(),
            features: Vec::new(),
            log_tail: Vec::new(),
        }
    }

    #[test]
    fn a_cargo_project_is_never_told_the_java_index_is_still_building() {
        // The report this exists for. `index.ready` is false on a Cargo root and always will be,
        // so the old wording sent a caller away from a project it had open to wait for something
        // that was never going to happen — and pointed it at a Java-only tool on the way out.
        let steps = next_steps(false, &stats(), Some(&server("ready")), &[]);
        let all = steps.join(" ");
        assert!(!all.contains("index is still building"), "{all}");
        assert!(all.contains("rust-analyzer"), "{all}");
        assert!(all.contains("no Java semantic index"), "{all}");
        // …and the tool it does suggest is one that can answer.
        assert!(all.contains("bennu_find_symbol"), "{all}");
        assert!(all.contains("bennu_class_index is Java-only"), "{all}");
    }

    #[test]
    fn a_cargo_project_whose_server_is_warming_is_told_to_wait_for_THAT() {
        let mut warming = server("starting");
        warming.progress = "Indexing 43%".into();
        let all = next_steps(false, &stats(), Some(&warming), &[]).join(" ");
        assert!(all.contains("Indexing 43%"), "{all}");
        assert!(all.contains("not that nothing was found"), "{all}");
    }

    #[test]
    fn a_cargo_project_with_no_server_is_told_to_ask_rather_than_wait() {
        // A server starts on the first question, so "wait" is the one instruction that cannot
        // work — nothing will happen until something asks.
        let all = next_steps(false, &stats(), None, &[]).join(" ");
        assert!(all.contains("ask"), "{all}");
        assert!(all.contains("do not wait"), "{all}");
    }

    #[test]
    fn a_java_project_still_gets_the_java_guidance() {
        let all = next_steps(true, &stats(), None, &[]).join(" ");
        assert!(all.contains("semantic index is still building"), "{all}");
        assert!(all.contains("bennu_class_index"), "{all}");
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
