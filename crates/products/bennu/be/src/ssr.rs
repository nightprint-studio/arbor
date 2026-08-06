//! `ssr` domain — structural search & replace, and the statistics it can be asked for.
//!
//! The language and the matching are [`bennu_ssr`], which knows neither Java nor the
//! filesystem. This module is the four things it deliberately does not do:
//!
//!   1. **which grammar** — the [`Dialect`], which is also what decides which files a query
//!      runs over and what a fragment has to be wrapped in to parse;
//!   2. **which files** — the walk, and the literal pre-filter that makes it fast;
//!   3. **types** — a [`TypeOracle`] backed by the project's resolver;
//!   4. **desugaring `use of`** into the patterns it stands for, so there is one engine and
//!      the panel can show what the shortcut expands to.
//!
//! ## Three dialects, one engine
//!
//! A query is written in the language it searches, so `Dialect` is a choice the user makes and
//! never a guess: `<s:property value="$x$"/>` is a JSP pattern and `log.debug($x$)` is a Java
//! one, and there is no reading of the text that tells them apart reliably enough to bet a
//! search on.
//!
//! The third is the interesting one. **Java in JSP** writes a Java query and walks pages, because
//! a legacy page keeps a great deal of its logic in `<% … %>` — and to the page grammar a
//! scriptlet is a single token, deliberately, so a JSP query can see that Java is there and
//! nothing about it. Each block is lifted out, wrapped in the smallest legal Java that makes its
//! kind parse, matched, and mapped back onto the page.
//!
//! Everything downstream — the pre-filter, the de-duplication, `group`, the preview and the
//! digest-checked apply — is untouched by the choice.
//!
//! ## The pre-filter is what makes this usable
//!
//! A pattern over five thousand files is five thousand parses. But every useful pattern contains
//! **literals that must appear** — `.debug(`, `new SimpleDateFormat`, `extends`. Extracting them
//! from the pattern text and grepping first cuts the candidate set by an order of magnitude on a
//! typical query, and only the survivors are parsed. A pattern that is nothing but placeholders
//! (`$x$::$m$`) yields no literals and the walk falls back to parsing everything, which is
//! reported so the panel can say why it was slow.
//!
//! ## Streaming, for the same reason find-in-files streams
//!
//! A search over a legacy tree takes seconds. It runs on a background thread and emits batches on
//! [`EVT_SSR_PROGRESS`], keyed by the caller's `search_id`, ending in exactly one terminal event
//! carrying the report. The dispatcher is never blocked and the panel fills as it goes.
//!
//! ## Replace never writes what you have not seen
//!
//! `bennu_ssr_preview` returns the before/after of every file it would touch, with a digest.
//! `bennu_ssr_apply` takes those digests back and refuses any file that changed underneath —
//! rewriting a file from a plan built against a different version of it is how a structural
//! replace becomes a bug report.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use arbor_ipc::prelude::EventSink;
use arbor_syntax::prelude::ByteRange;
use bennu_core::prelude::BennuState;
use bennu_ssr::prelude::{
    apply_edits, build_report, check_replacement, compile, edits_for, line_of, parse_query,
    preview_of, search_file, Ask, Denotation, GroupBy, Hit, HitCapture, NoTypes, Query, Report,
    Subject, TypeOracle,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::index_service::IndexService;

/// The BE→FE progress topic. Payloads keyed by the caller's `search_id`:
/// `{ id, hits: SsrHit[] }` per batch, then exactly one
/// `{ id, done: true, report, scanned, parsed, prefiltered }`.
const EVT_SSR_PROGRESS: &str = "arbor://bennu/ssr-progress";

/// Upper bound on hits carried back. A one-placeholder pattern can match a hundred thousand
/// nodes, and a panel is not a place to put them.
const MAX_HITS: usize = 5_000;

/// Flush a batch once this many hits have accumulated, so the list fills as the scan walks.
const BATCH: usize = 50;

/// The contexts a Java **fragment** is parsed in, tried in order after the unwrapped attempt.
///
/// Four shapes, because a pattern is written as the code you are looking for and that code is
/// legal in exactly one place:
///
///   * nothing — a whole compilation unit (`import $p$;`, `@interface $a$ { }`);
///   * a class body — a member declaration (`void go(int i) { $body$ }`);
///   * a method body — a statement (`if ($c$) { … }`, `return $x$;`);
///   * a method body **with a `;` appended** — a bare expression.
///
/// One shape is deliberately absent from those examples, and it is the one people reach for
/// first: `class $c$ extends $b$ { $body...$ }`. A placeholder is substituted with an
/// **identifier**, so a hole only goes where a name is legal — and a bare name is not a class
/// member, not a statement and not a parameter. A run of *arguments* works because arguments are
/// expressions; a run of members does not, and no additional context can make it, because the
/// hole is inside the pattern rather than around it. Pinned in
/// `bennu_ssr::engine`'s `a_run_hole_only_works_where_a_bare_name_is_legal`.
///
/// The last is the common case and the one it is easy to leave out: `$a$.$b$($c$, $d$)` is an
/// expression, Java needs a semicolon to make it a statement, and nobody writing a pattern types
/// one. Without that context every method-call pattern fails to compile, and a query that does
/// not compile finds nothing — which reads as "there is none of that in this project".
const JAVA_CONTEXTS: &[(&str, &str)] = &[
    ("class __BennuQuery__ {\n", "\n}"),
    ("class __BennuQuery__ { void __m__() {\n", "\n} }"),
    ("class __BennuQuery__ { void __m__() {\n", "\n;} }"),
];

/// A JSP fragment needs **no** wrapper, and that is a property of the grammar rather than a
/// shortcut taken here: a page is a sequence of tags, text and blocks, so any run of them is
/// already a legal document. `<s:property value="$x$"/>` parses standing on its own, which is
/// what makes the empty list the correct answer instead of an omission.
const JSP_CONTEXTS: &[(&str, &str)] = &[];

/// Which language a query is written in — and therefore which grammar reads it, which files it
/// runs over, and what a fragment has to be wrapped in to parse.
///
/// One enum rather than a boolean because the third one is coming (`.xml` descriptors are the
/// obvious next ask), and because a boolean called `jsp` at four call sites is how a language
/// gets half-added.
///
/// A legacy Struts codebase keeps as much of its logic in pages as in classes — an `<s:if>` on
/// the wrong property is a bug exactly like a Java one — and a text search over JSP is even
/// weaker than over Java, because the same tag is written across three lines as often as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    /// The default, and what every query written before this existed means.
    #[default]
    Java,
    /// **Java, inside pages.** The query is Java; the files walked are JSPs; what is matched is
    /// the contents of their `<% … %>`, `<%= … %>` and `<%! … %>` blocks.
    ///
    /// The dialect that exists because of what [`Dialect::Jsp`] cannot do. A scriptlet is one
    /// token to the page grammar — deliberately, since a `<` inside Java is not markup — so a
    /// structural search over the page can see *that* there is Java there and nothing about it.
    /// And a legacy page keeps a great deal in scriptlets: `session`, `request`, a DAO call, a
    /// null check that should have been a tag.
    ///
    /// Each block is lifted out, wrapped in the smallest legal Java that makes its **kind** parse
    /// (a scriptlet is statements, a declaration is members, a `<%= %>` is an expression), and
    /// matched with the real Java grammar. Every range is then mapped back onto the page, so a
    /// hit's line, preview and click land where the code actually is and not where it was parsed.
    #[serde(rename = "jsp-java")]
    JspJava,
    /// The JSP family — pages, fragments and tag files.
    ///
    /// One limit worth knowing, and it follows from the grammar rather than from this module:
    /// the `<% … %>` family is lexed as **single tokens**, so a hole cannot be put inside one.
    /// `<%@ taglib prefix="$p$" %>` compiles and then matches nothing, because the placeholder
    /// is a few characters inside a leaf rather than a node of its own. Tags, attributes and
    /// attribute values — which is what a page is searched by — are ordinary nodes and behave.
    Jsp,
}

impl Dialect {
    /// The grammar the **query** is written in. Note that it is not the grammar of the files:
    /// `JspJava` writes Java and walks pages, which is the whole of what it is.
    fn language(self) -> tree_sitter::Language {
        match self {
            Dialect::Java | Dialect::JspJava => bennu_java::prelude::java_language(),
            Dialect::Jsp => bennu_jsp_grammar::prelude::jsp_language(),
        }
    }

    fn contexts(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Dialect::Java | Dialect::JspJava => JAVA_CONTEXTS,
            Dialect::Jsp => JSP_CONTEXTS,
        }
    }

    /// Whether this dialect reads the file at `path`.
    fn reads(self, path: &Path) -> bool {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        match self {
            Dialect::Java => ext == "java",
            Dialect::Jsp | Dialect::JspJava => {
                matches!(ext.as_str(), "jsp" | "jspf" | "jspx" | "tag" | "tagx")
            }
        }
    }

    /// Whether a `use of` query means anything here. It desugars to a Java method-call pattern,
    /// so in a page it is not "unsupported" so much as **not a sentence** — and saying so beats
    /// letting it compile to nothing and report an empty project.
    ///
    /// `JspJava` **is** Java, so it keeps it: "who calls `getAttribute` on a session, in the
    /// pages" is a perfectly good question.
    fn has_use_of(self) -> bool {
        self != Dialect::Jsp
    }

    /// Whether types can be resolved for a subject of this dialect.
    ///
    /// They cannot inside a page. The resolver is asked about a *file*, and a scriptlet was
    /// lifted out of one and wrapped in synthetic Java that exists nowhere — so a type constraint
    /// gets `NoTypes` and comes back **undecided**, which is this crate's contract for "unknown"
    /// and the only honest answer. Answering from the page's path instead would be resolving a
    /// question about text that is not in it.
    fn resolves_types(self) -> bool {
        self != Dialect::JspJava
    }
}

// ── Java inside a page ──────────────────────────────────────────────────────────

/// How each kind of `<% … %>` block has to be wrapped for the Java grammar to accept it.
///
/// Three, because the three blocks are three different fragments of Java and one wrapper cannot
/// serve them: a scriptlet is **statements**, a declaration is **members**, and a `<%= %>` is an
/// **expression**. Using the statement wrapper for all of them would parse two of the three as
/// errors, which the grammar would recover from into something no pattern matches.
#[derive(Debug)]
struct Wrapper {
    prefix: &'static str,
    suffix: &'static str,
    /// Bytes of the opener that are not part of the Java — `<%`, `<%=`, `<%!`.
    open: usize,
}

const SCRIPTLET_WRAP: Wrapper =
    Wrapper { prefix: "class __B__ { void __m__() {\n", suffix: "\n} }", open: 2 };
const DECLARATION_WRAP: Wrapper = Wrapper { prefix: "class __B__ {\n", suffix: "\n}", open: 3 };
const EXPRESSION_WRAP: Wrapper =
    Wrapper { prefix: "class __B__ { Object __v__ = (\n", suffix: "\n); }", open: 3 };

fn wrapper_for(kind: &str) -> Option<&'static Wrapper> {
    match kind {
        "jsp_scriptlet" => Some(&SCRIPTLET_WRAP),
        "jsp_declaration" => Some(&DECLARATION_WRAP),
        "jsp_expression" => Some(&EXPRESSION_WRAP),
        _ => None,
    }
}

/// The Java blocks a page carries: `(body range, how to wrap it)`.
///
/// Found with the **page grammar** rather than by scanning for `<%`, so a `<%` inside a comment,
/// a `<%--` block or an attribute value is not mistaken for one — which is the same reason every
/// other reader of a page in this workspace goes through a parse or the shared mask.
fn java_blocks(source: &str) -> Vec<(ByteRange, &'static Wrapper)> {
    let Some(tree) = bennu_jsp_grammar::prelude::parse_jsp(source) else { return Vec::new() };
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    let mut todo = vec![tree.root_node()];
    while let Some(node) = todo.pop() {
        if let Some(wrap) = wrapper_for(node.kind()) {
            let start = node.start_byte() + wrap.open;
            // `%>` — and a block the page never closed simply runs to where the parser stopped.
            let end = node.end_byte().saturating_sub(2).max(start);
            if end > start {
                out.push((ByteRange::new(start, end), wrap));
            }
            continue;
        }
        todo.extend(node.children(&mut cursor));
    }
    out.sort_by_key(|(r, _)| r.start);
    out
}

/// Run a Java query over every Java block of one page.
///
/// The hits come back expressed against the **page**: the wrapper is subtracted and the block's
/// own offset added, so a click, a line number and a preview all land on the code as it is
/// written rather than on the synthetic class it was parsed inside.
fn search_java_blocks(
    language: &tree_sitter::Language,
    query: &Query,
    compiled: &[arbor_syntax::prelude::Pattern],
    rel: &str,
    page: &str,
    types: &dyn TypeOracle,
) -> Vec<Hit> {
    let mut out = Vec::new();
    for (body, wrap) in java_blocks(page) {
        let Some(text) = body.slice(page) else { continue };
        let wrapped = format!("{}{}{}", wrap.prefix, text, wrap.suffix);
        let subject = Subject { path: rel, source: &wrapped };
        let Ok(found) = search_file(language, query, compiled, &subject, types) else { continue };
        for hit in found {
            out.push(rebase(hit, page, body, wrap.prefix.len()));
        }
    }
    out.sort_by_key(|h| h.range.start);
    out
}

/// Re-express a hit found in a wrapped block against the page it was cut from.
///
/// Clamped into the block on both ends: a pattern made only of holes matches the synthetic class
/// itself, and a range that reported the wrapper's bytes would select text the file does not
/// have.
fn rebase(hit: Hit, page: &str, body: ByteRange, prefix: usize) -> Hit {
    let map = |at: usize| (body.start + at.saturating_sub(prefix)).clamp(body.start, body.end);
    let range = ByteRange::new(map(hit.range.start), map(hit.range.end));
    Hit {
        range,
        line: line_of(page, range.start),
        preview: preview_of(page, range),
        captures: hit
            .captures
            .into_iter()
            .map(|c| HitCapture {
                range: ByteRange::new(map(c.range.start), map(c.range.end)),
                ..c
            })
            .collect(),
        // `class __B__ { void __m__() { … } }` is not an enclosing declaration, it is scaffolding.
        // Naming it would put a class nobody wrote at the top of a report.
        enclosing: None,
        ..hit
    }
}

// ── the wire ────────────────────────────────────────────────────────────────────

/// One match, as the panel draws it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SsrHit {
    /// Absolute, forward-slashed — what a click opens.
    pub file: String,
    /// Project-relative — what a row shows.
    pub rel: String,
    pub line: usize,
    pub range: ByteRange,
    pub preview: String,
    /// The enclosing declaration, when the query grouped by it.
    pub enclosing: Option<String>,
    /// A type constraint here could not be decided. Shown, never hidden — see the crate doc.
    pub unresolved: bool,
}

/// What one file would become.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewedFile {
    pub file: String,
    pub rel: String,
    pub hits: usize,
    pub before: String,
    pub after: String,
    /// What the file was when the preview was built. Handed back to `apply`, which refuses on a
    /// mismatch rather than rewriting a version nobody looked at.
    pub digest: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub files: Vec<PreviewedFile>,
    pub hits: usize,
}

/// What `apply` did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub written: Vec<String>,
    /// Files skipped because they changed since the preview, with the reason.
    pub refused: Vec<Refusal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    pub file: String,
    pub reason: String,
}

/// What a query means before it is run — for the editor's live feedback and for the panel's
/// "what `use of` expands to" note.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Explained {
    /// `null` when the query reads. Otherwise the message, which is written for a human.
    pub error: Option<String>,
    /// 1-based line the error is on, `0` for the query as a whole.
    pub error_line: usize,
    /// How many alternatives it will try.
    pub alternatives: usize,
    /// The names every alternative binds — what a replacement may use.
    pub captures: Vec<String>,
    /// `use of` written out as the shapes it covers, so it is a shortcut and not a black box.
    pub expansion: Vec<String>,
    /// The literals the pre-filter will grep for. Empty means every file gets parsed.
    pub literals: Vec<String>,
}

// ── args ────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SsrSearchArgs {
    pub root: String,
    pub query: String,
    /// The FE-minted id correlating this search's progress events.
    pub search_id: String,
    /// Defaulted rather than required, so every caller written before dialects existed keeps
    /// meaning what it meant.
    #[serde(default)]
    pub dialect: Dialect,
}

#[derive(Deserialize)]
pub struct SsrReplaceArgs {
    pub root: String,
    pub query: String,
    pub replacement: String,
    #[serde(default)]
    pub dialect: Dialect,
}

#[derive(Deserialize)]
pub struct SsrApplyArgs {
    pub root: String,
    /// `(file, digest, after)` as the preview produced them.
    pub files: Vec<ApplyFile>,
}

#[derive(Deserialize)]
pub struct ApplyFile {
    pub file: String,
    pub digest: u64,
    pub after: String,
}

#[derive(Deserialize)]
pub struct SsrExplainArgs {
    pub query: String,
    #[serde(default)]
    pub dialect: Dialect,
}

// ── handlers ────────────────────────────────────────────────────────────────────

/// Read a query and say what it means, without running it.
///
/// Called on every keystroke in the query field, so it touches no files: parsing is string work,
/// and the answer is what turns a syntax error into a message under the field rather than an
/// empty result list you have to interpret.
///
/// It **also compiles the patterns**, which is the more valuable half. A query can read perfectly
/// as a query and still be a fragment the Java grammar rejects — and a pattern that does not
/// compile finds nothing, which is indistinguishable from a project that contains none of it. The
/// compile is a parse of a few dozen bytes; the answer is the difference between a message under
/// the field and a wrong conclusion about the codebase.
#[arbor_rpc::handler]
fn bennu_ssr_explain(_ctx: &BennuState, args: SsrExplainArgs) -> Result<Explained, String> {
    match parse_query(&args.query) {
        Err(e) => Ok(Explained {
            error: Some(e.message),
            error_line: e.line,
            alternatives: 0,
            captures: Vec::new(),
            expansion: Vec::new(),
            literals: Vec::new(),
        }),
        Ok(query) if matches!(query.ask, Ask::UseOf { .. }) && !args.dialect.has_use_of() => {
            Ok(Explained {
                error: Some(
                    "`use of` asks who calls a member, which is a question about Java code — \
                     write the tag you are looking for instead"
                        .to_string(),
                ),
                error_line: 0,
                alternatives: 0,
                captures: Vec::new(),
                expansion: Vec::new(),
                literals: Vec::new(),
            })
        }
        Ok(query) => {
            // `use of` is desugared to patterns, so what has to compile is the desugaring —
            // otherwise a shortcut could be reported as fine and then find nothing.
            let runnable = desugar(&query);
            if let Err(e) =
                compile(&args.dialect.language(), &runnable, args.dialect.contexts())
            {
                return Ok(Explained {
                    error: Some(e.to_string()),
                    error_line: 0,
                    alternatives: 0,
                    captures: Vec::new(),
                    expansion: expansion_of(&query),
                    literals: Vec::new(),
                });
            }
            let (alternatives, captures) = match &query.ask {
                Ask::Patterns(alts) => (
                    alts.len(),
                    // Only the names EVERY alternative binds: a replacement may use those and
                    // nothing else, so listing a name one branch lacks would be an invitation
                    // to write a template that is refused.
                    alts.iter()
                        .map(|a| capture_names(&a.pattern))
                        .reduce(|acc, names| {
                            acc.into_iter().filter(|n| names.contains(n)).collect()
                        })
                        .unwrap_or_default(),
                ),
                Ask::UseOf { member_capture, .. } => (1, vec![member_capture.clone()]),
            };
            Ok(Explained {
                error: None,
                error_line: 0,
                alternatives,
                captures,
                expansion: expansion_of(&query),
                literals: literals_of(&query),
            })
        }
    }
}

/// Run a query. Fire-and-forget: validates, spawns the walk, returns immediately.
#[arbor_rpc::handler]
fn bennu_ssr_search(ctx: &BennuState, args: SsrSearchArgs) -> Result<(), String> {
    let query = parse_query(&args.query).map_err(|e| e.message)?;
    let sink = ctx.event_sink();
    let root = args.root.clone();
    let search_id = args.search_id.clone();
    let dialect = args.dialect;

    // A plain background std thread, like `find_in_files`: the walk does no reverse-channel
    // round-trips, it just reads files and emits.
    std::thread::Builder::new()
        .name(format!("bennu-ssr-{search_id}"))
        .spawn(move || {
            // Emitted **as the walk finds them**, not collected and chunked afterwards. The
            // difference is the whole point of streaming: a project-wide query takes seconds,
            // and a list that fills as it goes is a list you can start reading.
            let mut streamed = 0usize;
            let outcome = {
                let sink = sink.clone();
                let id = search_id.clone();
                let root = root.clone();
                run(&query, &root, dialect, &mut |batch: &[Hit]| {
                    streamed += batch.len();
                    emit_hits(&sink, &id, batch, &root);
                })
            };
            // The report needs every hit together — it is an aggregate — so it is built at the
            // end from what the walk kept. Only the *listing* streams.
            debug_assert_eq!(streamed, outcome.hits.len(), "every hit reaches the panel once");
            let report = build_report(&outcome.hits, query.group.as_ref());
            sink.emit(
                EVT_SSR_PROGRESS,
                json!({
                    "id": search_id,
                    "done": true,
                    "report": report,
                    "scanned": outcome.scanned,
                    "parsed": outcome.parsed,
                    "prefiltered": outcome.prefiltered,
                    "capped": outcome.capped,
                    "error": outcome.error,
                }),
            );
        })
        .map_err(|e| format!("spawn ssr thread: {e}"))?;
    Ok(())
}

/// What a replacement would do, file by file. Nothing is written.
#[arbor_rpc::handler]
fn bennu_ssr_preview(_ctx: &BennuState, args: SsrReplaceArgs) -> Result<Preview, String> {
    let query = parse_query(&args.query).map_err(|e| e.message)?;
    // Before a single file is read: a template naming a capture the pattern does not bind is a
    // mistake in the template, and reporting it per file would report it hundreds of times.
    check_replacement(&query, &args.replacement).map_err(|e| e.message)?;

    // Nothing to stream to: a preview is a whole plan or it is not a plan.
    let outcome = run(&query, &args.root, args.dialect, &mut |_| {});
    let mut by_file: HashMap<String, Vec<Hit>> = HashMap::new();
    for hit in outcome.hits {
        by_file.entry(hit.file.clone()).or_default().push(hit);
    }

    let mut files = Vec::new();
    let mut total = 0usize;
    let mut paths: Vec<String> = by_file.keys().cloned().collect();
    paths.sort();
    for rel in paths {
        let hits = &by_file[&rel];
        let absolute = absolute(&args.root, &rel);
        let Some(before) = read_source(&args.root, &absolute) else { continue };
        let after = apply_edits(&before, &edits_for(&args.replacement, hits));
        if after == before {
            continue;
        }
        total += hits.len();
        files.push(PreviewedFile {
            file: absolute,
            rel,
            hits: hits.len(),
            digest: digest(&before),
            before,
            after,
        });
    }
    Ok(Preview { files, hits: total })
}

/// Write what the preview showed.
///
/// Every file is re-read and re-digested first. A file that changed since the preview is
/// **refused by name** rather than overwritten: the `after` in hand was computed from bytes that
/// are no longer there, and applying it would silently undo whatever happened in between.
#[arbor_rpc::handler]
fn bennu_ssr_apply(_ctx: &BennuState, args: SsrApplyArgs) -> Result<Applied, String> {
    let mut written = Vec::new();
    let mut refused = Vec::new();
    for file in &args.files {
        match read_source(&args.root, &file.file) {
            Some(current) if digest(&current) == file.digest => {
                match write_source(&args.root, &file.file, &file.after) {
                    Ok(()) => written.push(file.file.clone()),
                    Err(e) => refused.push(Refusal { file: file.file.clone(), reason: e }),
                }
            }
            Some(_) => refused.push(Refusal {
                file: file.file.clone(),
                reason: "it changed since the preview was built".to_string(),
            }),
            None => refused.push(Refusal {
                file: file.file.clone(),
                reason: "it could not be read".to_string(),
            }),
        }
    }
    Ok(Applied { written, refused })
}

// ── the walk ────────────────────────────────────────────────────────────────────

struct Outcome {
    hits: Vec<Hit>,
    /// Files the scope admitted.
    scanned: usize,
    /// Files actually parsed — the pre-filter's whole point.
    parsed: usize,
    /// Whether the pre-filter applied at all.
    prefiltered: bool,
    capped: bool,
    /// Why the walk produced nothing, when the reason is the query rather than the project.
    ///
    /// A pattern that does not compile finds nothing, and "found nothing" is exactly what a
    /// project containing none of it looks like. The two must never render the same: one means
    /// *go look somewhere else* and the other means *your pattern is wrong*. `explain` already
    /// says so under the field, but it is asked with the query alone — the walk is where a
    /// grammar that cannot read the pattern actually shows up, so the walk has to say it too.
    error: Option<String>,
}

impl Outcome {
    /// The empty answer, with the reason attached.
    fn refused(error: String) -> Self {
        Outcome {
            hits: Vec::new(),
            scanned: 0,
            parsed: 0,
            prefiltered: false,
            capped: false,
            error: Some(error),
        }
    }
}

/// Walk the project, reporting hits to `emit` **as they are found**.
///
/// The callback is what lets one walk serve both callers: the search streams each file's hits to
/// the panel the moment it has them, and the preview — which needs the whole set before it can
/// compute a single edit — passes a callback that does nothing. Collecting first and chunking
/// afterwards would have made the search *look* streamed while still taking all its time before
/// the first row appeared.
fn run(query: &Query, root: &str, dialect: Dialect, emit: &mut dyn FnMut(&[Hit])) -> Outcome {
    // `use of` is a shortcut for a set of patterns, not a second engine — see `desugar`.
    let query = &desugar(query);

    let language = dialect.language();
    let compiled = match compile(&language, query, dialect.contexts()) {
        Ok(compiled) => compiled,
        Err(e) => return Outcome::refused(e.to_string()),
    };
    let literals = literals_of(query);
    let project_types = ProjectTypes { root: root.to_string() };
    let no_types = NoTypes;
    let oracle: &dyn TypeOracle =
        if dialect.resolves_types() { &project_types } else { &no_types };

    let mut hits = Vec::new();
    let (mut scanned, mut parsed) = (0usize, 0usize);
    let mut capped = false;

    for path in source_files(root, dialect) {
        let rel = relative(root, &path);
        if !admits(query, &rel) {
            continue;
        }
        scanned += 1;
        let Some(source) = read_source(root, &path) else { continue };
        // Every literal must be present for any alternative to match. Cheap, and it is what
        // keeps a project-wide query from parsing five thousand files.
        if !literals.is_empty() && !literals.iter().any(|l| source.contains(l)) {
            continue;
        }
        parsed += 1;
        let found = match dialect {
            // The page is a container of Java, not the subject itself — see `Dialect::JspJava`.
            Dialect::JspJava => {
                search_java_blocks(&language, query, &compiled, &rel, &source, oracle)
            }
            _ => {
                let subject = Subject { path: &rel, source: &source };
                match search_file(&language, query, &compiled, &subject, oracle) {
                    Ok(found) => found,
                    Err(_) => continue,
                }
            }
        };
        // One batch per file: it is the natural boundary — a file's matches belong together, and
        // it bounds how often the seam is crossed on a project with thousands of small files.
        let room = MAX_HITS.saturating_sub(hits.len());
        capped = found.len() > room;
        let batch = &found[..found.len().min(room)];
        if !batch.is_empty() {
            emit(batch);
            hits.extend_from_slice(batch);
        }
        if capped || hits.len() >= MAX_HITS {
            capped = true;
            break;
        }
    }

    Outcome { hits, scanned, parsed, prefiltered: !literals.is_empty(), capped, error: None }
}

/// Rewrite `use of $m$ on <Type>` into the patterns it stands for.
///
/// A shortcut, not a second engine — which is what lets the panel show the expansion and lets a
/// user copy it, edit it and run it as an ordinary query. Everything downstream (the pre-filter,
/// the type oracle, the de-duplication, `group`) then applies unchanged.
///
/// **Three shapes, and the limit is deliberate.** It finds uses *through a reference to the
/// type*: a call on a receiver, and the two forms of method reference. It does **not** find a
/// class calling its own member (`this.place(o)`, a bare `place(o)`, `super.place(o)`) — those
/// need to know the enclosing class's hierarchy, which a pattern cannot express. That is also
/// almost always what you want: "who uses OrderService" is a question about its consumers, and
/// counting its own internals among them would inflate every answer.
fn desugar(query: &Query) -> Query {
    let Ask::UseOf { member, member_capture, owner, subtypes } = &query.ask else {
        return query.clone();
    };
    // A named member becomes a literal in the pattern; `$m$` stays a capture so `group $m$`
    // can count which members are used.
    let m = match member {
        Some(name) => name.clone(),
        None => format!("${member_capture}$"),
    };
    let plus = if *subtypes { "+" } else { "" };
    let short = owner.rsplit('.').next().unwrap_or(owner);
    let text = format!(
        "$__recv: {owner}{plus}$.{m}($__args...$)\n\
         or {short}::{m}\n\
         or $__recv: {owner}{plus}$::{m}\n",
    );
    let mut rewritten = parse_query(&text).unwrap_or_else(|_| query.clone());
    rewritten.scopes = query.scopes.clone();
    rewritten.group = query.group.clone();
    rewritten
}

/// Whether the query's `in` admits a project-relative path. No scopes means everywhere.
fn admits(query: &Query, rel: &str) -> bool {
    query.scopes.is_empty() || query.scopes.iter().any(|s| rel.starts_with(s.as_str()))
}

/// The literals every alternative must contain for it to have any chance of matching.
///
/// Taken from the pattern with the placeholders removed: `$o$.$m$($a...$)` leaves `.` and `(`,
/// which are useless, so anything shorter than three characters is dropped. What survives is the
/// method name, the class name, the keyword — the parts that make a pattern selective.
///
/// One literal **per alternative**, and a file is a candidate if it contains *any* of them: with
/// `or`, a file matching only the second branch must not be filtered out by the first branch's
/// literal.
fn literals_of(query: &Query) -> Vec<String> {
    let Ask::Patterns(alts) = &query.ask else { return Vec::new() };
    let mut out = Vec::new();
    for alt in alts {
        match longest_literal(&alt.pattern) {
            Some(literal) => out.push(literal),
            // A branch with no literal can match anywhere, so the pre-filter cannot be trusted
            // for the query as a whole.
            None => return Vec::new(),
        }
    }
    out
}

fn longest_literal(pattern: &str) -> Option<String> {
    let mut best = String::new();
    let mut current = String::new();
    let mut in_hole = false;
    for ch in pattern.chars() {
        if ch == '$' {
            in_hole = !in_hole;
            if current.chars().count() > best.chars().count() {
                best = current.clone();
            }
            current.clear();
            continue;
        }
        if in_hole {
            continue;
        }
        if ch.is_alphanumeric() || ch == '_' || ch == '.' {
            current.push(ch);
        } else {
            if current.chars().count() > best.chars().count() {
                best = current.clone();
            }
            current.clear();
        }
    }
    if current.chars().count() > best.chars().count() {
        best = current;
    }
    let trimmed = best.trim_matches('.').to_string();
    (trimmed.chars().count() >= 3).then_some(trimmed)
}

/// What `use of` covers, written out. Shown in the panel so it is a shortcut, not a black box.
fn expansion_of(query: &Query) -> Vec<String> {
    let Ask::UseOf { member, owner, .. } = &query.ask else { return Vec::new() };
    let m = member.clone().unwrap_or_else(|| "$m$".to_string());
    let short = owner.rsplit('.').next().unwrap_or(owner);
    vec![
        format!("$recv: {owner}$.{m}($args...$)   — a call through a reference to it"),
        format!("{short}::{m}                      — a method reference on the type"),
        format!("$recv: {owner}$::{m}             — a method reference on an instance"),
    ]
}

fn capture_names(pattern: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            i += 1;
            continue;
        }
        let Some(close) = (i + 1..chars.len()).find(|&j| chars[j] == '$') else { break };
        let inner: String = chars[i + 1..close].iter().collect();
        let name = inner.trim().trim_end_matches("...").trim().to_string();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
        i = close + 1;
    }
    out
}

// ── types ───────────────────────────────────────────────────────────────────────

/// The [`TypeOracle`] backed by the project's resolver.
///
/// `None` means **unknown**, never "no" — the crate's contract, and the reason a legacy project
/// with half its dependencies missing reports "380 I could not read" instead of quietly
/// answering 12.
struct ProjectTypes {
    root: String,
}

impl TypeOracle for ProjectTypes {
    /// **Value first, then type** — and the order is Java's, not a preference.
    ///
    /// A local, a parameter or a field *obscures* a type of the same name (JLS §6.4.2): inside a
    /// method holding `Order order`, the name `Order` still means the class, but a variable
    /// actually called `Order` would mean the variable. Asking "is it a type?" first would
    /// therefore call a variable a class in exactly the file where it matters.
    ///
    /// Both halves come back `None` on a receiver the classpath cannot reach, which the engine
    /// reports as undecided rather than as `@value` — see [`bennu_ssr::prelude::TypeOracle`].
    fn denotation_at(&self, file: &str, source: &str, range: ByteRange) -> Option<Denotation> {
        let file = absolute(&self.root, file);
        let service = IndexService::global();
        if let Some(ty) = service.expression_type(&file, source, range.start, range.end) {
            return Some(Denotation::Value(ty));
        }
        service
            .type_name_at(&file, source, range.start, range.end)
            .map(Denotation::Type)
    }

    fn is_subtype_of(&self, candidate: &str, wanted: &str) -> bool {
        candidate == wanted || IndexService::global().is_subtype_of(&self.root, candidate, wanted)
    }
}

// ── files ───────────────────────────────────────────────────────────────────────

/// Every file under `root` the dialect reads, skipping the directories nothing useful lives in.
fn source_files(root: &str, dialect: Dialect) -> Vec<String> {
    let mut out = Vec::new();
    collect(Path::new(root), dialect, &mut out);
    out
}

const SKIP_DIRS: [&str; 4] = ["target", ".git", "node_modules", ".idea"];

fn collect(dir: &Path, dialect: Dialect, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !SKIP_DIRS.contains(&name) {
                collect(&path, dialect, out);
            }
        } else if dialect.reads(&path) {
            out.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Read a source in the project's declared encoding — a legacy tree is frequently Cp1252, and a
/// file this could not decode is a file whose matches would be missing from the count.
fn read_source(root: &str, path: &str) -> Option<String> {
    let label = crate::index_service::resolve_index_encoding(root);
    bennu_intel::prelude::read_source_for_index(Path::new(path), &label).map(|d| d.text)
}

fn write_source(root: &str, path: &str, text: &str) -> Result<(), String> {
    let label = crate::index_service::resolve_index_encoding(root);
    let (bytes, _) = bennu_project::prelude::encode_text(text, &label);
    std::fs::write(path, bytes).map_err(|e| format!("{e}"))
}

fn relative(root: &str, path: &str) -> String {
    let root = root.replace('\\', "/");
    let path = path.replace('\\', "/");
    path.strip_prefix(&format!("{}/", root.trim_end_matches('/')))
        .unwrap_or(&path)
        .to_string()
}

fn absolute(root: &str, rel: &str) -> String {
    if rel.starts_with('/') || rel.chars().nth(1) == Some(':') {
        return rel.to_string();
    }
    format!("{}/{}", root.replace('\\', "/").trim_end_matches('/'), rel)
}

/// A cheap content fingerprint. Not cryptographic — it exists to notice a file that changed
/// between a preview and its apply, which any 64-bit hash notices.
fn digest(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        h ^= *byte as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn emit_hits(sink: &Arc<dyn EventSink>, id: &str, hits: &[Hit], root: &str) {
    for chunk in hits.chunks(BATCH) {
        let payload: Vec<SsrHit> = chunk
            .iter()
            .map(|h| SsrHit {
                file: absolute(root, &h.file),
                rel: h.file.clone(),
                line: h.line,
                range: h.range,
                preview: h.preview.clone(),
                enclosing: h.enclosing.clone(),
                unresolved: h.unresolved,
            })
            .collect();
        sink.emit(EVT_SSR_PROGRESS, json!({ "id": id, "hits": payload }));
    }
}

/// Kept so the compiler notices if the report shape stops being serialisable.
#[allow(dead_code)]
fn _report_is_wire_shaped(report: &Report) -> serde_json::Value {
    serde_json::to_value(report).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str) -> Query {
        parse_query(text).expect("parses")
    }

    // ── dialects ────────────────────────────────────────────────────────────────

    /// Every caller written before dialects existed keeps meaning what it meant — the reason
    /// the field is `#[serde(default)]` and not required.
    #[test]
    fn a_query_with_no_dialect_named_is_a_java_one() {
        assert_eq!(Dialect::default(), Dialect::Java);
    }

    #[test]
    fn each_dialect_reads_its_own_files_and_nobody_elses() {
        let jsp = ["a.jsp", "a.jspf", "a.jspx", "a.tag", "a.tagx", "A.JSP"];
        for name in jsp {
            assert!(Dialect::Jsp.reads(Path::new(name)), "{name}");
            assert!(!Dialect::Java.reads(Path::new(name)), "{name}");
        }
        assert!(Dialect::Java.reads(Path::new("A.java")));
        assert!(!Dialect::Jsp.reads(Path::new("A.java")));
        // Neither reads what neither can parse — a `.tld` is XML, whatever it describes.
        assert!(!Dialect::Java.reads(Path::new("struts.tld")));
        assert!(!Dialect::Jsp.reads(Path::new("struts.tld")));
    }

    /// A page needs no wrapper to parse, and a Java fragment needs four tries. Asserted rather
    /// than assumed, because a wrapper applied to JSP would silently prepend `class {` to every
    /// pattern and match nothing.
    #[test]
    fn only_java_wraps_its_fragments() {
        assert!(Dialect::Jsp.contexts().is_empty());
        assert_eq!(Dialect::Java.contexts().len(), 3);
    }

    /// The three shapes a JSP query is actually written in, compiled against the real grammar.
    /// If a pattern does not compile it finds nothing, which reads as "the project has none of
    /// this" — so this is the test that keeps a wrong answer from looking like a true one.
    #[test]
    fn the_shapes_a_page_is_searched_by_all_compile() {
        for pattern in [
            "<s:property value=\"$x$\"/>",
            "<s:iterator value=\"$list$\" var=\"$v$\">",
            "<td class=\"$c$\">",
            "</s:iterator>",
        ] {
            let q = query(pattern);
            assert!(
                compile(&Dialect::Jsp.language(), &q, Dialect::Jsp.contexts()).is_ok(),
                "{pattern}"
            );
        }
    }

    fn jsp_hits(pattern: &str, source: &str) -> Vec<Hit> {
        let q = query(pattern);
        let compiled = compile(&Dialect::Jsp.language(), &q, Dialect::Jsp.contexts())
            .expect("the pattern compiles");
        let subject = Subject { path: "a.jsp", source };
        search_file(&Dialect::Jsp.language(), &q, &compiled, &subject, &NoTypes)
            .expect("a search")
    }

    /// The point of a *structural* search over pages, and the reason `is_layout` exists in
    /// `arbor-syntax`: the JSP grammar makes tag whitespace an explicit token, so without it a
    /// tag written across four lines and the same tag on one are two different trees.
    #[test]
    fn a_tag_matches_however_it_is_laid_out() {
        let hits = jsp_hits(
            "<s:property value=\"$x$\"/>",
            "<table>\n  <s:property\n      value=\"codice\"\n  />\n</table>",
        );
        assert_eq!(hits.len(), 1, "one tag, however it wraps");
        assert_eq!(
            hits[0].captures.iter().find(|c| c.name == "x").map(|c| c.text.as_str()),
            Some("codice"),
            "the capture is the attribute value's own bytes"
        );
    }

    /// A pattern is still a pattern: the tag it names and no other.
    #[test]
    fn a_tag_pattern_does_not_match_a_different_tag() {
        let source = "<s:property value=\"a\"/><s:hidden value=\"b\"/>";
        assert_eq!(jsp_hits("<s:property value=\"$x$\"/>", source).len(), 1);
    }

    /// `use of` desugars to a Java method call. In a page it is not unsupported so much as not a
    /// sentence, and saying so beats an empty result list nobody can interpret.
    #[test]
    fn use_of_is_refused_for_pages_rather_than_silently_finding_nothing() {
        assert!(Dialect::Java.has_use_of());
        assert!(!Dialect::Jsp.has_use_of());
    }

    // ── Java in JSP ─────────────────────────────────────────────────────────────

    #[test]
    fn the_java_in_jsp_dialect_writes_java_and_walks_pages() {
        assert!(Dialect::JspJava.reads(Path::new("list.jsp")));
        assert!(!Dialect::JspJava.reads(Path::new("List.java")));
        assert_eq!(Dialect::JspJava.contexts().len(), 3, "Java fragments, Java wrappers");
        assert!(Dialect::JspJava.has_use_of(), "it is Java, so `use of` is a sentence");
        assert!(!Dialect::JspJava.resolves_types(), "a lifted scriptlet is in no file");
    }

    const PAGE: &str = concat!(
        "<%@ page contentType=\"text/html\" %>\n",
        "<%! private int count = 0; %>\n",
        "<html>\n",
        "<% String user = (String) session.getAttribute(\"user\"); %>\n",
        "<p>Ciao <%= session.getAttribute(\"nome\") %></p>\n",
        "</html>\n",
    );

    /// The three block kinds are three different fragments of Java, and one wrapper cannot serve
    /// them — a declaration is a member, a scriptlet is statements, a `<%= %>` is an expression.
    #[test]
    fn each_kind_of_block_gets_the_wrapper_its_java_needs() {
        let blocks = java_blocks(PAGE);
        assert_eq!(blocks.len(), 3, "the directive is not Java");
        let bodies: Vec<&str> = blocks.iter().map(|(r, _)| r.slice(PAGE).unwrap().trim()).collect();
        assert_eq!(bodies[0], "private int count = 0;");
        assert!(bodies[1].starts_with("String user ="));
        assert_eq!(bodies[2], "session.getAttribute(\"nome\")");
        // Compared by what the wrapper *is* rather than by address: a `const` is inlined at each
        // use, so two references to one are not required to be the same pointer.
        assert_eq!(blocks[0].1.prefix, DECLARATION_WRAP.prefix);
        assert_eq!(blocks[1].1.prefix, SCRIPTLET_WRAP.prefix);
        assert_eq!(blocks[2].1.prefix, EXPRESSION_WRAP.prefix);
    }

    /// A `<%` inside a JSP comment is not a scriptlet. Found through the page grammar rather than
    /// by scanning for the characters, which is the whole reason it is found through the grammar.
    #[test]
    fn a_commented_out_scriptlet_is_not_java() {
        assert!(java_blocks("<%-- <% int x = 1; %> --%>").is_empty());
    }

    /// **The point of the dialect.** A Java query, matched inside a page.
    #[test]
    fn a_java_pattern_matches_inside_a_scriptlet() {
        let q = query("session.getAttribute($k$)");
        let lang = Dialect::JspJava.language();
        let compiled = compile(&lang, &q, Dialect::JspJava.contexts()).expect("compiles");
        let hits = search_java_blocks(&lang, &q, &compiled, "a.jsp", PAGE, &NoTypes);
        assert_eq!(hits.len(), 2, "the scriptlet and the <%= %>");
        assert_eq!(
            hits[0].captures.iter().find(|c| c.name == "k").map(|c| c.text.as_str()),
            Some("\"user\""),
        );
    }

    /// Every range comes back expressed against the **page**: the wrapper subtracted, the block's
    /// own offset added. A hit that reported the synthetic class's coordinates would open the
    /// right file at the wrong line and select bytes that are not there.
    #[test]
    fn a_hit_is_re_expressed_against_the_page() {
        let q = query("session.getAttribute($k$)");
        let lang = Dialect::JspJava.language();
        let compiled = compile(&lang, &q, Dialect::JspJava.contexts()).expect("compiles");
        let hits = search_java_blocks(&lang, &q, &compiled, "a.jsp", PAGE, &NoTypes);
        assert_eq!(&PAGE[hits[0].range.start..hits[0].range.end], "session.getAttribute(\"user\")");
        assert_eq!(hits[0].line, 4, "the line in the page, not in the wrapper");
        assert_eq!(hits[1].line, 5);
        assert!(hits[0].enclosing.is_none(), "the wrapper class is scaffolding, not a declaration");
    }

    /// A pattern made only of holes matches the synthetic class itself. Its range has to be
    /// clamped into the block, or the hit selects text the page does not contain.
    #[test]
    fn a_match_on_the_wrapper_is_clamped_into_the_block() {
        let q = query("$a$");
        let lang = Dialect::JspJava.language();
        let compiled = compile(&lang, &q, Dialect::JspJava.contexts()).expect("compiles");
        for hit in search_java_blocks(&lang, &q, &compiled, "a.jsp", PAGE, &NoTypes) {
            assert!(hit.range.end <= PAGE.len());
            assert!(PAGE.get(hit.range.start..hit.range.end).is_some(), "a real slice of the page");
        }
    }

    /// The pre-filter's whole value: the selective part of a pattern is its names, not its
    /// punctuation.
    #[test]
    fn the_literal_taken_is_the_longest_one_worth_grepping() {
        assert_eq!(literals_of(&query("log.debug($x$)")), ["log.debug"]);
        assert_eq!(literals_of(&query("new SimpleDateFormat($p...$)")), ["SimpleDateFormat"]);
        assert_eq!(literals_of(&query("$o$.createStatement()")), ["createStatement"]);
    }

    /// A pattern that is all holes can match anywhere, so filtering on it would drop real hits.
    #[test]
    fn a_pattern_with_nothing_to_grep_for_disables_the_prefilter() {
        assert!(literals_of(&query("$o$::$m$")).is_empty());
        assert!(literals_of(&query("$a$.$b$($c$)")).is_empty(), "`.` and `(` are not selective");
    }

    /// With `or`, one branch without a literal must disable the filter for the WHOLE query —
    /// otherwise a file matching only that branch is never even read.
    #[test]
    fn one_unfilterable_branch_disables_the_filter_for_all_of_them() {
        assert!(literals_of(&query("log.debug($x$)\nor $o$::$m$")).is_empty());
    }

    #[test]
    fn each_branch_contributes_its_own_literal() {
        let found = literals_of(&query("log.debug($x$)\nor log.trace($x$)"));
        assert_eq!(found, ["log.debug", "log.trace"]);
    }

    #[test]
    fn scopes_filter_by_path_prefix() {
        let q = query("$x$.close()\nin modules/core");
        assert!(admits(&q, "modules/core/src/main/java/A.java"));
        assert!(!admits(&q, "modules/web/src/main/java/A.java"));
        assert!(admits(&query("$x$.close()"), "anywhere/A.java"), "no scope is everywhere");
    }

    #[test]
    fn paths_round_trip_between_absolute_and_relative() {
        let root = "C:/p/app";
        assert_eq!(relative(root, "C:/p/app/src/A.java"), "src/A.java");
        assert_eq!(absolute(root, "src/A.java"), "C:/p/app/src/A.java");
        assert_eq!(absolute(root, "C:/other/A.java"), "C:/other/A.java", "already absolute");
    }

    #[test]
    fn the_digest_notices_a_change() {
        assert_eq!(digest("class A {}"), digest("class A {}"));
        assert_ne!(digest("class A {}"), digest("class B {}"));
    }

    /// `use of` is a shortcut, and the panel shows what for — six shapes, which is five more
    /// than anyone writes by hand.
    /// `use of` is a shortcut, and the panel shows what for — the expansion IS the query, so it
    /// can be copied, edited and run as an ordinary one.
    #[test]
    fn use_of_expands_to_the_shapes_it_covers() {
        let lines = expansion_of(&query("use of place on com.acme.OrderService"));
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().any(|l| l.contains("OrderService::place")));
    }

    /// The desugaring has to produce a query the engine can actually compile, with the group
    /// carried over — otherwise `use of $m$ … group $m$` would silently lose its grouping.
    #[test]
    fn desugaring_keeps_the_group_and_the_scope() {
        let q = query("use of $m$ on com.acme.OrderService+
in modules/core
group $m$");
        let out = desugar(&q);
        let Ask::Patterns(alts) = &out.ask else { panic!("desugars to patterns") };
        assert_eq!(alts.len(), 3);
        assert_eq!(out.scopes, ["modules/core"]);
        assert_eq!(out.group, Some(GroupBy::Capture("m".into())));
        // Every branch binds $m$, which is what makes the grouping legal.
        for alt in alts { assert!(alt.pattern.contains("$m$")); }
    }

    #[test]
    fn desugaring_a_named_member_makes_it_a_literal() {
        let out = desugar(&query("use of place on com.acme.OrderService"));
        let Ask::Patterns(alts) = &out.ask else { panic!() };
        assert!(alts[0].pattern.contains(".place("));
        // ...and that literal is what the pre-filter greps for.
        assert!(literals_of(&out).iter().any(|l| l == "place"));
    }

    #[test]
    fn a_pattern_query_has_no_expansion() {
        assert!(expansion_of(&query("$o$.place($a$)")).is_empty());
    }

    #[test]
    fn capture_names_reads_both_arities() {
        assert_eq!(capture_names("f($a$, $bs...$)"), ["a", "bs"]);
    }
}
