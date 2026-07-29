//! `scripts` domain — reading a repository of install scripts, and answering
//! questions about it.
//!
//! This is the seam between the script crates and the running application. The
//! crates below it are pure and finished — `picus-parse` maps bytes to
//! statements, `picus-inventory` joins those to the tree, `picus-analyze` runs the
//! fourteen rules — and what they all need is the same thing: **every file, read
//! once and decoded once**. Producing that, holding it, and knowing when it is no
//! longer true is what this module does, and it is all it does.
//!
//! ## The shape of a session
//!
//! 1. `picus_open_scripts` reads the repository, decodes every script and keeps
//!    the result. Called once when a connection's repository comes into view.
//! 2. `picus_analyze_scripts` parses that held text and runs the rules. Called
//!    straight after the open, and again whenever the user asks.
//! 3. `picus_refresh_scripts` throws the read away and does it again. Along with a
//!    write, it is the **only** thing that invalidates anything — see
//!    `picus_core::scripts` for why nothing expires on its own.
//!
//! Every one of these is a synchronous handler, which is deliberate: the backend's
//! serve loop dispatches each request on its own worker thread, so reading four
//! hundred files off a network share blocks nothing but the call that asked for it
//! (`docs/backend-architecture.md` §7).
//!
//! ## The parse is not cached, and that is not an oversight
//!
//! `ParsedFile` is a map of a string the caller owns — every position in it is a
//! byte range into that exact `String`. Storing a parse beside its own source is
//! therefore a self-referential struct, and working around that would mean giving
//! up the invariant that makes byte-identical rewriting a theorem. So the parse is
//! produced inside the call that needs it, by [`parse_all`] and nowhere else,
//! which is also the single function a future on-disk parse cache would replace.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arbor_fs::prelude::encoding::{decode_in_context, EncodingContext};
use picus_analyze::prelude::{analyze, Finding, RejectedSuppression, SkippedRule};
use picus_core::prelude::{digest, CachedSource, PicusState, ScriptSnapshot};
use picus_inventory::prelude::{Inventory, InventoryKind, InventoryObject, ParsedProject, ParsedScript};
use picus_parse::prelude::{DialectScope, EngineKind, ParsedFile, SqlParser, StatementKind};
use picus_project::prelude::{discover, label_to_encoding, LineEnding, ScriptFile};
use serde::Serialize;

use crate::project::OpenedProject;

/// Read a repository and hold what was read.
///
/// Answers exactly what `picus_open_project` answers — the tree, the notes,
/// whether this is a proposal, and the problems — so the interface has one shape
/// to render whether it opened a repository or re-opened one. The difference is
/// on this side: the decoded text stays, and every later question is cheap.
#[arbor_rpc::handler]
fn picus_open_scripts(state: &PicusState, root: String) -> Result<OpenedProject, String> {
    let snapshot = snapshot_for(state, &root)?;
    Ok(opened(&snapshot))
}

/// Read the repository again, whatever was held before.
///
/// The manual half of "the cache is invalidated by hand". The other half is a
/// write, which invalidates the repository it wrote into.
#[arbor_rpc::handler]
fn picus_refresh_scripts(state: &PicusState, root: String) -> Result<OpenedProject, String> {
    let root = root_of(&root);
    state.scripts().invalidate(&root);
    let snapshot = Arc::new(read(&root)?);
    state.scripts().put(Arc::clone(&snapshot));
    Ok(opened(&snapshot))
}

/// What the inventory table and the consistency dock render.
///
/// The four lists are the crates' own wire types, unchanged. Re-shaping them here
/// would put a second definition of a finding in the codebase, and the second one
/// is always the one that drifts.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzedScripts {
    /// One row per database object the repository names, with its coverage per
    /// folder. The interesting cell is the zero.
    pub inventory: Vec<InventoryObject>,
    /// Suppressed findings included, and marked as such.
    pub findings: Vec<Finding>,
    /// Rules that could not run, and what would make them run. A rule that
    /// quietly passed for lack of input reads exactly like a rule that passed.
    pub skipped: Vec<SkippedRule>,
    /// `-- picus: ignore …` comments that silence nothing.
    pub rejected_suppressions: Vec<RejectedSuppression>,
    /// Parsed files the tree does not know about. Always empty in practice — both
    /// halves come from one snapshot — and reported anyway, because the day it is
    /// not empty is the day something is wrong in here.
    pub orphans: Vec<String>,
}

/// Parse the held sources and run the fourteen rules over them.
#[arbor_rpc::handler]
fn picus_analyze_scripts(state: &PicusState, root: String) -> Result<AnalyzedScripts, String> {
    let snapshot = snapshot_for(state, &root)?;

    // Parse first, then borrow: `ParsedProject` borrows both the sources and the
    // parses, so the parses have to outlive the join.
    let parses = parse_all(&snapshot);
    let scripts: Vec<ParsedScript<'_>> = parses
        .iter()
        .filter_map(|(path, parsed)| {
            snapshot.source(path).map(|source| ParsedScript {
                path: path.as_str(),
                source: source.text.as_str(),
                parsed,
            })
        })
        .collect();

    let joined = ParsedProject::new(&snapshot.project, scripts);
    let inventory = Inventory::build(&joined);
    let report = analyze(&joined, &snapshot.config, &inventory);

    Ok(AnalyzedScripts {
        inventory: inventory.wire(),
        findings: report.findings,
        skipped: report.skipped,
        rejected_suppressions: report.rejected_suppressions,
        orphans: joined.orphans().iter().map(|p| p.to_string()).collect(),
    })
}

/// One place an object is named — a row of the drill-down behind a coverage cell.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectUsage {
    /// Project-relative path.
    pub path: String,
    /// The folder whose coverage column this counts under.
    pub folder: String,
    /// 1-based line of the name.
    pub line: usize,
    /// The statement creates or redefines the object, rather than merely using it.
    pub defining: bool,
    /// …and it is a `CREATE`, not an `ALTER`.
    pub creating: bool,
    /// `select`, `insert`, `create`, … — what the statement holding it does.
    pub statement: StatementKind,
}

/// Every place one object is named.
///
/// A separate call rather than a field on the inventory, and the reason is size:
/// the inventory has a row per object and this has a row per *mention*, which in
/// a real repository is one or two orders of magnitude more. Shipping them with
/// the table would make opening the Inventory tab pay for a drill-down nobody has
/// asked for yet.
///
/// The question it answers is the one the coverage matrix raises and could not
/// previously settle: the cell says three, and the only useful next thought is
/// *which three*. Ordered by file then line, so the answer reads like the
/// repository.
///
/// `folder` restricts the answer to one coverage column — what clicking a single
/// cell asks. Omitted, it covers the whole repository, which is what clicking the
/// row asks.
#[arbor_rpc::handler]
fn picus_object_usages(
    state: &PicusState,
    root: String,
    kind: InventoryKind,
    name: String,
    folder: Option<String>,
) -> Result<Vec<ObjectUsage>, String> {
    let snapshot = snapshot_for(state, &root)?;
    let parses = parse_all(&snapshot);
    let scripts: Vec<ParsedScript<'_>> = parses
        .iter()
        .filter_map(|(path, parsed)| {
            snapshot.source(path).map(|source| ParsedScript {
                path: path.as_str(),
                source: source.text.as_str(),
                parsed,
            })
        })
        .collect();

    let joined = ParsedProject::new(&snapshot.project, scripts);
    let inventory = Inventory::build(&joined);
    let Some(entry) = inventory.find(kind, &name) else { return Ok(Vec::new()) };

    let mut usages: Vec<ObjectUsage> = entry
        .sites
        .iter()
        .filter(|site| folder.as_deref().is_none_or(|want| site.folder_path == want))
        .map(|site| ObjectUsage {
            path: site.path.clone(),
            folder: site.folder_path.clone(),
            line: site.line,
            defining: site.defining,
            creating: site.creating,
            statement: statement_kind_at(&joined, &site.path, site.statement_index),
        })
        .collect();
    usages.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    Ok(usages)
}

/// What the statement holding a site actually does.
///
/// `Unknown` when the statement cannot be found, which cannot happen — the site
/// came from that very parse — but is answered rather than panicked on: an
/// inventory drill-down is not worth taking the backend down for.
fn statement_kind_at(
    project: &ParsedProject<'_>,
    path: &str,
    index: usize,
) -> StatementKind {
    project
        .script_of(path)
        .and_then(|script| script.parsed.statements.get(index))
        .map(|s| s.kind)
        .unwrap_or(StatementKind::Unknown)
}

/// One script file's decoded contents, for the editor.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptText {
    pub text: String,
    /// What the bytes were decoded with — the pill above the editor.
    pub encoding: String,
    pub eol: LineEnding,
}

/// The text of one script, as it was read.
#[arbor_rpc::handler]
fn picus_script_text(
    state: &PicusState,
    root: String,
    path: String,
) -> Result<ScriptText, String> {
    let snapshot = snapshot_for(state, &root)?;
    let source = snapshot.source(&path).ok_or_else(|| {
        format!("{path} is not one of this project's scripts — refresh if it has just been added")
    })?;
    Ok(ScriptText {
        text: source.text.clone(),
        encoding: source.encoding.clone(),
        eol: source.eol,
    })
}

// ── The read itself ────────────────────────────────────────────────────────────

/// The snapshot for a root: the one already held, or a fresh read.
pub(crate) fn snapshot_for(
    state: &PicusState,
    root: &str,
) -> Result<Arc<ScriptSnapshot>, String> {
    let root = root_of(root);
    // Taken and released before anything expensive: the guard must never be alive
    // while a repository is being read or parsed.
    if let Some(held) = state.scripts().get(&root) {
        return Ok(held);
    }
    let snapshot = Arc::new(read(&root)?);
    state.scripts().put(Arc::clone(&snapshot));
    Ok(snapshot)
}

/// Re-read what the **project file** says, and keep the text that was already
/// decoded.
///
/// Called after every write to `project.toml`. Without it the held snapshot keeps
/// the configuration it was opened with, so switching a rule off, changing the
/// initialisation model or classifying a folder changed nothing until the user
/// hit "re-read the repository from disk" — and "re-run the check" gave the same
/// answer as before, which reads as a broken button rather than as a stale cache.
///
/// The decoded text is reused rather than re-read, and that is the point: a
/// configuration write does not change a single byte on disk, and re-reading four
/// hundred files off a network share to learn that a rule is off would make every
/// classification click cost seconds.
///
/// A file whose **expected encoding** changed is re-read, because its text is
/// genuinely different now — that is the one project setting which does change
/// what the bytes mean.
pub(crate) fn reconfigure(root: &Path, held: &ScriptSnapshot) -> Result<ScriptSnapshot, String> {
    let proposal = discover(root).map_err(|e| e.to_string())?;
    let mut problems = crate::project::config_problems(&proposal.config);
    let mut sources = BTreeMap::new();

    for file in proposal.project.all_files() {
        match held.source(&file.path).filter(|held| held.encoding == file.encoding) {
            Some(reused) => {
                sources.insert(reused.path.clone(), reused.clone());
            }
            None => match read_one(root, &file.path, &file.encoding) {
                Ok(source) => {
                    sources.insert(source.path.clone(), source);
                }
                Err(reason) => problems.push(reason),
            },
        }
    }

    Ok(ScriptSnapshot {
        root: root.to_path_buf(),
        project: proposal.project,
        config: proposal.config,
        notes: proposal.notes,
        is_new: proposal.is_new,
        problems,
        sources,
    })
}

/// Swap in a snapshot that reflects the project file as it now stands.
///
/// A no-op when nothing is held — there is then nothing stale to correct, and the
/// next open reads the new configuration anyway. Best-effort: a repository that
/// cannot be re-read must not turn a successful settings write into a failure,
/// since the write already landed.
pub(crate) fn refresh_configuration(state: &PicusState, root: &Path) {
    let key = root_of(&root.display().to_string());
    let Some(held) = state.scripts().get(&key) else { return };
    if let Ok(fresh) = reconfigure(&key, &held) {
        state.scripts().put(Arc::new(fresh));
    }
}

/// Discover a repository and decode every script in it.
pub(crate) fn read(root: &Path) -> Result<ScriptSnapshot, String> {
    let proposal = discover(root).map_err(|e| e.to_string())?;
    let mut problems = crate::project::config_problems(&proposal.config);
    let mut sources = BTreeMap::new();

    for file in proposal.project.all_files() {
        match read_one(root, &file.path, &file.encoding) {
            Ok(source) => {
                sources.insert(source.path.clone(), source);
            }
            // A file that cannot be read is reported and the repository still
            // opens: one unreadable script must not cost the user the other four
            // hundred, and refusing to open would leave nowhere to fix it from.
            Err(reason) => problems.push(reason),
        }
    }

    Ok(ScriptSnapshot {
        root: root.to_path_buf(),
        project: proposal.project,
        config: proposal.config,
        notes: proposal.notes,
        is_new: proposal.is_new,
        problems,
        sources,
    })
}

/// Read and decode one file, in the encoding discovery decided for it.
///
/// The context is built from that decision rather than re-run from scratch, so a
/// pure-ASCII file keeps the encoding it inherited from its folder instead of
/// picking a different answer here. The full bytes can still overrule it — a file
/// whose only accented character sits past discovery's 64 KiB sample proves itself
/// UTF-8 on the way through, and that is a correction, not a disagreement.
fn read_one(root: &Path, relative: &str, discovered: &str) -> Result<CachedSource, String> {
    let path = root.join(relative);
    let bytes = std::fs::read(&path).map_err(|e| format!("{relative} could not be read: {e}"))?;

    let encoding = label_to_encoding(discovered);
    let context = EncodingContext::new().with_legacy(encoding).with_dominant(encoding);
    let (text, detection) = decode_in_context(&bytes, &context);

    Ok(CachedSource {
        path: relative.to_string(),
        eol: LineEnding::detect(&bytes),
        digest: digest(&bytes),
        encoding: detection.encoding.name().to_string(),
        text,
    })
}

/// **The parse step**, and the only one in the backend.
///
/// Isolated on purpose: an on-disk parse cache is a change to this function's
/// body — look the digest up, parse and store on a miss — and to nothing else.
/// See `picus_core::scripts` for what such a tier has to provide.
///
/// Files in an engine Picus does not support are skipped, per **file** and not
/// per folder — see [`scope_of`] for why they must not be parsed at all. It is
/// also the cheapest speed there is on a real repository: parsing is two thirds
/// of an analysis, and in the repository this was built for the SQL Server and
/// DB2 scripts are roughly half the files, none of which anyone will ever look at
/// a finding for.
fn parse_all(snapshot: &ScriptSnapshot) -> Vec<(String, ParsedFile)> {
    // Both the skip list and the per-file scope come out of **one** walk of the
    // tree. Asking the tree per file would be a linear walk per file — quadratic
    // on exactly the large repositories this skip exists to speed up, which would
    // be a fine joke to leave in.
    let files = snapshot.project.files_by_path();
    let sources: Vec<&CachedSource> = snapshot
        .sources
        .values()
        .filter(|s| files.get(s.path.as_str()).is_some_and(|f| !f.is_out_of_scope()))
        .collect();
    // Below this, threads cost more than they save — and it keeps the common case
    // of a handful of files on one obvious code path.
    if sources.len() < PARALLEL_PARSE_THRESHOLD {
        let mut parser = SqlParser::new();
        return sources.iter().map(|s| parse_one(&mut parser, &files, s)).collect();
    }

    // Parsing is two thirds of an analysis and the files are completely
    // independent — nothing here reads another file's tree — so this is the one
    // place in the backend where threads pay for themselves. Contiguous chunks
    // rather than a work queue: no shared state at all, so there is no lock to
    // contend on and the result order is deterministic, which matters because a
    // report whose findings reorder between runs is one nobody can diff.
    //
    // A parser per thread, not per file: loading the grammar is the expensive part
    // of `SqlParser::new()`, which is why it is reusable in the first place.
    let threads = parse_threads(sources.len());
    let chunk = sources.len().div_ceil(threads);
    let mut parsed: Vec<(String, ParsedFile)> = Vec::with_capacity(sources.len());

    std::thread::scope(|scope| {
        let handles: Vec<_> = sources
            .chunks(chunk)
            .map(|slice| {
                let files = &files;
                scope.spawn(move || {
                    let mut parser = SqlParser::new();
                    slice.iter().map(|s| parse_one(&mut parser, files, s)).collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            // A panic in a parse thread is a bug in the grammar, not a user error.
            // Re-raising it here loses nothing: `join` already carries the payload,
            // and swallowing it would report a repository as clean because part of
            // it silently failed to parse — the exact failure this product exists
            // to prevent.
            parsed.extend(handle.join().expect("a parse thread panicked"));
        }
    });
    parsed
}

/// Files below which parsing stays on one thread.
const PARALLEL_PARSE_THRESHOLD: usize = 24;

/// How many threads to split a parse across.
///
/// Capped well under the core count: this runs inside a backend that is also
/// serving the interface, and a repository scan that saturates every core makes
/// the window it is meant to be filling stutter.
fn parse_threads(files: usize) -> usize {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    cores.saturating_sub(1).clamp(1, 8).min(files)
}

fn parse_one(
    parser: &mut SqlParser,
    files: &HashMap<&str, &ScriptFile>,
    source: &CachedSource,
) -> (String, ParsedFile) {
    let scope = scope_of(files, &source.path);
    (source.path.clone(), parser.parse(&source.text, scope))
}

/// What a file is parsed as — **its own** scope, which for all but a handful of
/// files is the one it inherited from its folder.
///
/// Asked of the file rather than the folder because in an untidy repository the
/// engine is on the file: one directory holding `4_12_ORA.sql` and `4_12_POS.sql`
/// has to hand each of them to the right dialect, and its folder can say nothing
/// about either.
///
/// **A portable file is parsed as `Portable`**, not as one of its dialects, and
/// that is the whole of what `DIA001`'s inversion needs from this layer: under
/// `Portable` the parser accepts the syntax of neither engine, so a construct
/// belonging to either lands in `foreign` and the rule reports the broken promise.
/// Parsing such a file as Oracle would silently hide every Oracle-ism in it —
/// exactly the ones it must not contain.
///
/// A file nobody could identify has no scope, and the fallback is genuinely
/// arbitrary rather than a guess: the grammar is one permissive superset of both
/// dialects, so the statements, the objects and the DML come out the same either
/// way. The engine decides only which constructs count as *foreign*, and `DIA001`
/// refuses to report those for a file with no engine — so nothing a user ever
/// sees depends on this choice.
///
/// It is a fallback for the **unclassified** case only. A file written in an
/// engine Picus does not support never reaches here: [`parse_all`] filtered it
/// out, because the fallback above would be a real guess there, and a wrong one.
///
/// ## Files in an engine Picus does not support are not parsed
///
/// Not "parsed and then ignored" — **not parsed**. Handing T-SQL or DB2 SQL to a
/// grammar built for Oracle and PostgreSQL does not fail; it succeeds, and
/// produces a plausible-looking tree of statements that mean nothing. Everything
/// downstream then has to be trusted to throw that away, and the day one rule
/// forgets, the report is confidently wrong about somebody else's scripts.
///
/// A file nobody has classified *is* still parsed: that is a question, not an
/// answer, and its inventory is part of what makes the question answerable — the
/// user is choosing between engines partly on what the files say. The distinction
/// is the whole reason those two states are not one value.
fn scope_of(files: &HashMap<&str, &ScriptFile>, path: &str) -> DialectScope {
    files
        .get(path)
        .and_then(|file| file.scope())
        .unwrap_or(DialectScope::One(EngineKind::Postgres))
}

/// The reply both `picus_open_scripts` and `picus_refresh_scripts` give.
fn opened(snapshot: &ScriptSnapshot) -> OpenedProject {
    OpenedProject {
        project: snapshot.project.clone(),
        notes: snapshot.notes.clone(),
        is_new: snapshot.is_new,
        problems: snapshot.problems.clone(),
        aliases: snapshot.config.aliases.clone(),
    }
}

/// The root as a path. Kept in one place so every handler in the script half
/// spells it the same way.
pub(crate) fn root_of(root: &str) -> PathBuf {
    PathBuf::from(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use picus_analyze::prelude::RuleId;
    use picus_ast::prelude::{
        Column, DmlModel, DmlOperation, FolderRole, Target, TargetWrap, VersionTableConfig,
    };
    use picus_core::prelude::digest;
    use picus_rewrite::prelude::commit;

    /// The whole seam, over a repository on disk: read → parse → analyse →
    /// preview → write → read again. Every layer below this is tested without a
    /// filesystem; this is the one test that proves they are wired together.
    ///
    /// A real directory rather than a fixture because that is what is being
    /// tested — the decoding, the paths, and the bytes that come back off disk.

    struct Silent;
    impl arbor_ipc::prelude::EventSink for Silent {
        fn emit(&self, _topic: &str, _payload: serde_json::Value) {}
    }

    fn state() -> PicusState {
        PicusState::new(Arc::new(Silent))
    }

    fn cp1252(text: &str) -> Vec<u8> {
        encoding_of("windows-1252", text)
    }

    fn encoding_of(label: &str, text: &str) -> Vec<u8> {
        label_to_encoding(label).encode(text).0.into_owned()
    }

    /// A two-dialect repository shaped like the ones this product was built for:
    /// windows-1252, CRLF, accents, an update folder and an initialisation one.
    fn repository(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("picus-be-{name}"));
        let _ = std::fs::remove_dir_all(&root);

        let files: [(&str, Vec<u8>); 4] = [
            (
                "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
                cp1252("-- tabelle\r\nCREATE TABLE PARAMETRI (COD VARCHAR2(30), VALORE NUMBER);\r\n"),
            ),
            (
                "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
                cp1252(
                    "-- soglia già applicata\r\n\
                     INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);\r\n",
                ),
            ),
            (
                "ORACLE/AGGIORNAMENTO/4_11__4_12.sql",
                cp1252("-- 4.11 -> 4.12\r\nUPDATE PARAMETRI SET VALORE = 11;\r\n"),
            ),
            (
                "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
                cp1252("-- tabelle\r\ncreate table parametri (cod varchar(30), valore numeric);\r\n"),
            ),
        ];
        for (relative, bytes) in files {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
            std::fs::write(&path, bytes).expect("write");
        }
        root
    }

    fn model() -> DmlModel {
        let column = |name: &str, ty: &str| Column {
            name: name.to_string(),
            data_type: ty.to_string(),
            primary_key: false,
            not_null: false,
            default_value: None,
        };
        let mut row = std::collections::BTreeMap::new();
        row.insert("COD".to_string(), "SOGLIA_SCONTO".to_string());
        row.insert("VALORE".to_string(), "42".to_string());

        DmlModel {
            table: "PARAMETRI".to_string(),
            operation: DmlOperation::Insert,
            columns: vec![column("COD", "varchar(30)"), column("VALORE", "numeric")],
            key_columns: vec![column("COD", "varchar(30)")],
            rows: vec![row],
            lowercase_postgres: false,
            version_table: VersionTableConfig::default(),
        }
    }

    fn target(file: &str, role: FolderRole) -> Target {
        Target {
            id: file.to_string(),
            file: file.to_string(),
            dialect: DialectScope::One(EngineKind::Oracle),
            role,
            enabled: true,
            wrap: TargetWrap::Plain,
            guards: Default::default(),
            version_filter: None,
        }
    }

    #[test]
    fn a_repository_is_read_once_and_answers_everything_from_there() {
        let root = repository("read");
        let state = state();

        let first = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        assert_eq!(first.sources.len(), 4);
        assert!(first.problems.is_empty(), "{:?}", first.problems);
        assert!(first.is_new, "a repository with no project.toml is a proposal");

        // The accented file came back as windows-1252 text, and the pure-ASCII
        // ones inherited that from their folder rather than being guessed at.
        let accented = first.source("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql").expect("read");
        assert!(accented.text.contains("soglia già applicata"), "{}", accented.text);
        assert_eq!(accented.encoding, "windows-1252");
        assert_eq!(accented.eol, LineEnding::Crlf);
        assert_eq!(accented.digest.len(), 64);

        // The second open is the same snapshot, not a second read.
        let second = snapshot_for(&state, &root.to_string_lossy()).expect("re-opens");
        assert!(Arc::ptr_eq(&first, &second), "an open must not re-read the repository");

        // …until something asks for it.
        state.scripts().invalidate(&root);
        let third = snapshot_for(&state, &root.to_string_lossy()).expect("re-reads");
        assert!(!Arc::ptr_eq(&first, &third));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_repository_with_the_dialect_at_the_bottom_reads_as_the_tree_it_is() {
        // The shape a real repository has: the role three levels above the
        // dialect, and one leaf nobody can classify. Asserted through the whole
        // seam — a real directory, discovery, resolution — because everything
        // under it is tested without a filesystem.
        let root = std::env::temp_dir().join("picus-be-deep");
        let _ = std::fs::remove_dir_all(&root);
        for relative in [
            "AGGIORNAMENTO/2024/ORA/4_11__4_12.sql",
            "AGGIORNAMENTO/2024/POS/4_11__4_12.sql",
            "AGGIORNAMENTO/2025/ORA/4_12__4_13.sql",
        ] {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
            std::fs::write(&path, cp1252("UPDATE PARAMETRI SET VALORE = 1;\r\n")).expect("write");
        }

        let snapshot = read(&root).expect("reads");
        let tree = &snapshot.project;

        // The role is declared once, at the top, and reaches every leaf.
        for path in ["AGGIORNAMENTO/2024", "AGGIORNAMENTO/2024/ORA", "AGGIORNAMENTO/2025/ORA"] {
            assert_eq!(
                tree.folder_at(path).expect(path).effective_role,
                picus_project::prelude::FolderRole::Update,
                "{path}"
            );
        }
        // The dialect is declared at the leaves, independently.
        assert_eq!(
            tree.dialect_of("AGGIORNAMENTO/2024/ORA/4_11__4_12.sql"),
            Some(EngineKind::Oracle)
        );
        assert_eq!(
            tree.dialect_of("AGGIORNAMENTO/2025/ORA/4_12__4_13.sql"),
            Some(EngineKind::Oracle)
        );
        // …and `POS` matches nothing Picus knows, so it has none and is asked
        // about rather than guessed at.
        assert_eq!(tree.dialect_of("AGGIORNAMENTO/2024/POS/4_11__4_12.sql"), None);
        let note = snapshot
            .notes
            .iter()
            .find(|n| n.path == "AGGIORNAMENTO/2024/POS")
            .expect("a note about the folder nobody could identify");
        assert!(note.needs_attention);

        // The proposed file says the two things the tree could not have known,
        // and nothing else.
        let declared: Vec<(&str, bool, bool)> = snapshot
            .config
            .folders
            .iter()
            .map(|f| (f.path.as_str(), f.dialect.is_some(), f.role.is_some()))
            .collect();
        assert_eq!(
            declared,
            [
                ("AGGIORNAMENTO", false, true),
                ("AGGIORNAMENTO/2024/ORA", true, false),
                ("AGGIORNAMENTO/2025/ORA", true, false),
            ]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_rules_run_over_what_was_read() {
        let root = repository("analyze");
        let snapshot = read(&root).expect("reads");

        let parses = parse_all(&snapshot);
        let scripts: Vec<ParsedScript<'_>> = parses
            .iter()
            .filter_map(|(path, parsed)| {
                snapshot.source(path).map(|s| ParsedScript {
                    path: path.as_str(),
                    source: s.text.as_str(),
                    parsed,
                })
            })
            .collect();
        let joined = ParsedProject::new(&snapshot.project, scripts);
        let inventory = Inventory::build(&joined);
        let report = analyze(&joined, &snapshot.config, &inventory);

        assert!(joined.orphans().is_empty(), "{:?}", joined.orphans());

        // Oracle's PARAMETRI and PostgreSQL's parametri are one row, and the
        // zero is the cell that matters: PostgreSQL has no update folder here.
        let parametri = inventory.wire().into_iter().find(|o| o.name == "PARAMETRI").expect("indexed");
        assert!(parametri.coverage.values().any(|n| *n > 0));

        // The update script changes something and never reads or writes the
        // version table — the two rules this repository exists to demonstrate.
        assert!(report.of_rule(RuleId::Ver001).count() > 0, "{:?}", report.findings);
        assert!(report.of_rule(RuleId::Ver002).count() > 0);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_projects_own_vocabulary_classifies_the_whole_repository_and_stops_the_parse() {
        // The real repository, in miniature: one folder set per delivered version,
        // with POS for PostgreSQL and MSQ for SQL Server — neither of which any
        // global vocabulary can be allowed to know. The project says so once, and
        // everything follows: eleven folders classified by one line, and the SQL
        // Server files never handed to a grammar that would parse them into
        // plausible nonsense.
        let root = std::env::temp_dir().join("picus-be-alias");
        let _ = std::fs::remove_dir_all(&root);
        for version in ["4_11", "4_12"] {
            for engine in ["ORA", "POS", "MSQ"] {
                let path = root.join(format!("AGGIORNAMENTO/{version}/{engine}/{version}.sql"));
                std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
                std::fs::write(&path, cp1252("UPDATE PARAMETRI SET VALORE = 1;\r\n"))
                    .expect("write");
            }
        }

        // Without the vocabulary: four questions, and every file parsed.
        let bare = read(&root).expect("reads");
        assert_eq!(bare.notes.iter().filter(|n| n.needs_attention).count(), 4);
        assert_eq!(parse_all(&bare).len(), 6);

        let project_toml = root.join(".arbor/picus/project.toml");
        std::fs::create_dir_all(project_toml.parent().unwrap()).expect("mkdir");
        std::fs::write(
            &project_toml,
            "version = 2\nname = \"PROD_CORE\"\n\n\
             [[alias]]\nname = \"POS\"\nengine = \"postgres\"\n\n\
             [[alias]]\nname = \"MSQ\"\nengine = \"sqlserver\"\n",
        )
        .expect("write");

        let snapshot = read(&root).expect("re-reads");
        let tree = &snapshot.project;

        // One line classified both POS folders…
        for version in ["4_11", "4_12"] {
            assert_eq!(
                tree.dialect_of(&format!("AGGIORNAMENTO/{version}/POS/{version}.sql")),
                Some(EngineKind::Postgres),
                "{version}"
            );
            // …and the other said "SQL Server", which is an answer and not a
            // dialect: no parse, no lane, no question.
            let msq = tree.folder_at(&format!("AGGIORNAMENTO/{version}/MSQ")).expect("in the tree");
            assert!(msq.engine_is_unsupported() && !msq.engine_is_unknown(), "{version}");
            assert_eq!(msq.effective_dialect(), None, "{version}");
        }

        // Nothing left to ask about, and nothing wrong with the file.
        assert!(snapshot.notes.iter().all(|n| !n.needs_attention), "{:?}", snapshot.notes);
        assert!(snapshot.problems.is_empty(), "{:?}", snapshot.problems);

        // Every file is still READ — an MSQ script opens in the editor like any
        // other — but only the four Picus can speak for are parsed.
        assert_eq!(snapshot.sources.len(), 6);
        let parsed: Vec<String> =
            parse_all(&snapshot).into_iter().map(|(path, _)| path).collect();
        assert_eq!(parsed.len(), 4, "{parsed:?}");
        assert!(parsed.iter().all(|p| !p.contains("/MSQ/")), "{parsed:?}");

        // …and the skipped files are not reported as orphans: an orphan is a
        // parse the tree does not know about, which is the opposite problem.
        let parses = parse_all(&snapshot);
        let scripts: Vec<ParsedScript<'_>> = parses
            .iter()
            .filter_map(|(path, parsed)| {
                snapshot.source(path).map(|s| ParsedScript {
                    path: path.as_str(),
                    source: s.text.as_str(),
                    parsed,
                })
            })
            .collect();
        let joined = ParsedProject::new(&snapshot.project, scripts);
        assert!(joined.orphans().is_empty(), "{:?}", joined.orphans());
        // The coverage matrix leaves the SQL Server folders out rather than
        // showing them as a permanent column of zeroes.
        assert!(
            joined.coverage_keys().iter().all(|k| !k.contains("/MSQ")),
            "{:?}",
            joined.coverage_keys()
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The row the fixture's initialisation already installs — `SOGLIA`, not the
    /// `SOGLIA_SCONTO` the plain model writes.
    fn model_for_an_existing_row() -> DmlModel {
        let mut row = std::collections::BTreeMap::new();
        row.insert("COD".to_string(), "SOGLIA".to_string());
        // A different value: this is the case the feature exists for — the row is
        // there and what is being written is a change to it.
        row.insert("VALORE".to_string(), "99".to_string());
        DmlModel { rows: vec![row], ..model() }
    }

    #[test]
    fn an_update_script_replaces_a_row_the_scripts_already_install() {
        // `SOGLIA` is inserted by ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql, so an
        // update script re-stating it must delete by key first — otherwise it
        // fails on the key of every database that has already been installed.
        let root = repository("replace-update");
        let state = state();
        let targets = [target("ORACLE/AGGIORNAMENTO/4_13__4_14.sql", FolderRole::Update)];

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned =
            crate::apply::plan(&snapshot, &model_for_an_existing_row(), &targets).expect("plans");

        let after = &planned[0].prepared.after;
        assert!(after.contains("DELETE FROM PARAMETRI"), "{after}");
        assert!(after.contains("WHERE COD = 'SOGLIA'"), "{after}");
        assert!(after.contains("INSERT INTO PARAMETRI"), "{after}");
        // The delete matches on the comparison key alone. Matching the whole row
        // would leave a hand-edited row in place and then insert a second copy.
        assert!(!after.contains("VALORE = 99\n"), "the key alone identifies the row:\n{after}");
        assert!(
            planned[0].prepared.reasons.iter().any(|r| r.contains("already installed")),
            "the diff has to say why it is a delete-then-insert: {:?}",
            planned[0].prepared.reasons
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_initialisation_changes_the_row_that_is_already_there() {
        // The other half of the same rule: an initialisation describes the end
        // state, so a row already in it is edited rather than added a second time.
        let root = repository("replace-init");
        let state = state();
        let targets = [target("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", FolderRole::Init)];

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned =
            crate::apply::plan(&snapshot, &model_for_an_existing_row(), &targets).expect("plans");

        let after = &planned[0].prepared.after;
        assert!(after.contains("'SOGLIA', 99"), "the row carries the new value:\n{after}");
        assert_eq!(
            after.matches("'SOGLIA'").count(),
            1,
            "exactly one row for that key, not two:\n{after}"
        );
        // Nothing was appended: there is no block, because there was nothing new.
        assert!(!after.contains("picus: generated"), "{after}");
        // And the accented comment above it is untouched, as always.
        assert!(after.starts_with("-- soglia già applicata\r\n"), "{after}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_upsert_into_the_initialisation_inserts_or_changes_the_row_that_is_there() {
        // The user's rule, in one test: on the initialisation an upsert needs no
        // `MERGE` at all. If the row is not there it is a plain insert; if it is,
        // the original insert is changed.
        let root = repository("upsert-init");
        let state = state();
        let targets = [target("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", FolderRole::Init)];
        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");

        // A row that is already there: changed where it is.
        let existing = DmlModel { operation: DmlOperation::Upsert, ..model_for_an_existing_row() };
        let planned = crate::apply::plan(&snapshot, &existing, &targets).expect("plans");
        let after = &planned[0].prepared.after;
        assert!(!after.contains("MERGE"), "an initialisation needs no MERGE:\n{after}");
        assert!(after.contains("'SOGLIA', 99"), "{after}");
        assert_eq!(after.matches("'SOGLIA'").count(), 1, "one row for that key:\n{after}");

        // A row nobody installs: a plain insert.
        let fresh = DmlModel { operation: DmlOperation::Upsert, ..model() };
        let planned = crate::apply::plan(&snapshot, &fresh, &targets).expect("plans");
        let after = &planned[0].prepared.after;
        assert!(!after.contains("MERGE"), "{after}");
        assert!(after.contains("INSERT INTO PARAMETRI"), "{after}");
        assert!(after.contains("SOGLIA_SCONTO"), "{after}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_portable_initialisation_finds_the_row_the_scripts_already_install() {
        // The destination that most needs reconciling was the only one never given
        // it: a portable folder has no single engine, so "what do this engine's
        // scripts say" was answered with nothing at all — and the row was appended
        // beside the one already there, with the same key.
        let root = repository("portable-known");
        std::fs::create_dir_all(root.join("COMUNE/INIZIALIZZAZIONE")).expect("mkdir");
        std::fs::write(
            root.join("COMUNE/INIZIALIZZAZIONE/parametri.sql"),
            // Written the way the repositories this exists for write it: lower
            // case on the portable side, upper on Oracle's. Same row.
            cp1252("insert into parametri (cod, valore) values ('SOGLIA', 10);\r\n"),
        )
        .expect("write");
        // Portable is never inferred from a folder name — it is a promise about
        // what runs where, and only the repository can make it. So the fixture
        // declares it, exactly as a real project does.
        std::fs::create_dir_all(root.join(".arbor/picus")).expect("mkdir");
        std::fs::write(
            root.join(".arbor/picus/project.toml"),
            "version = 3\nname = \"PROD_CORE\"\n\n\
             [[folder]]\npath = \"COMUNE/INIZIALIZZAZIONE\"\n\
             dialect = \"generic\"\nrole = \"init\"\n",
        )
        .expect("write");

        let state = state();
        let mut portable = target("COMUNE/INIZIALIZZAZIONE/parametri.sql", FolderRole::Init);
        portable.dialect = DialectScope::Portable;
        let targets = [portable];

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned =
            crate::apply::plan(&snapshot, &model_for_an_existing_row(), &targets).expect("plans");
        let after = &planned[0].prepared.after;

        assert_eq!(
            after.matches("'SOGLIA'").count(),
            1,
            "one row for that key, not the old one plus a new one:\n{after}"
        );
        assert!(after.contains("'SOGLIA', 99"), "and it carries the new value:\n{after}");
        assert!(!after.contains("picus: generated"), "nothing was appended:\n{after}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_portable_initialisation_takes_an_upsert() {
        // What the refusal used to block: a portable folder could not receive an
        // upsert at all, when the thing wanted was a plain INSERT.
        let root = repository("upsert-portable");
        std::fs::create_dir_all(root.join("COMUNE/INIZIALIZZAZIONE")).expect("mkdir");
        std::fs::write(
            root.join("COMUNE/INIZIALIZZAZIONE/parametri.sql"),
            cp1252("-- portabile\r\n"),
        )
        .expect("write");

        let state = state();
        let mut portable = target("COMUNE/INIZIALIZZAZIONE/parametri.sql", FolderRole::Init);
        portable.dialect = DialectScope::Portable;
        let targets = [portable];

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let upsert = DmlModel { operation: DmlOperation::Upsert, ..model() };
        let planned = crate::apply::plan(&snapshot, &upsert, &targets).expect("plans");
        let after = &planned[0].prepared.after;
        assert!(!after.contains("nothing generated"), "{after}");
        assert!(after.contains("INSERT INTO PARAMETRI"), "{after}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_conditional_delete_does_not_erase_the_whole_table() {
        // What took the feature to zero on a real repository. These scripts
        // delete-and-reinsert constantly, and the repository is walked in **tree**
        // order — not install order — so one `DELETE … WHERE` in one folder was
        // erasing everything learned from every other. Seventeen thousand INSERTs
        // reduced to nothing by one line, and the only symptom was a block appended
        // beside the row it should have changed.
        let root = repository("conditional-delete");
        std::fs::write(
            root.join("ORACLE/AGGIORNAMENTO/4_11__4_12.sql"),
            cp1252(
                "-- 4.11 -> 4.12\r\n\
                 DELETE FROM PARAMETRI WHERE COD = 'QUALCOSALTRO';\r\n",
            ),
        )
        .expect("write");

        let state = state();
        let targets = [target("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", FolderRole::Init)];
        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned =
            crate::apply::plan(&snapshot, &model_for_an_existing_row(), &targets).expect("plans");
        let after = &planned[0].prepared.after;

        assert!(after.contains("'SOGLIA', 99"), "the row was still found:\n{after}");
        assert_eq!(after.matches("'SOGLIA'").count(), 1, "and changed, not duplicated:\n{after}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unconditional_delete_does_erase_it() {
        // The other half of the same rule: "every row of this table is gone" is
        // readable without evaluating anything, so it is believed.
        let root = repository("unconditional-delete");
        std::fs::write(
            root.join("ORACLE/AGGIORNAMENTO/4_11__4_12.sql"),
            cp1252("-- 4.11 -> 4.12\r\nDELETE FROM PARAMETRI;\r\n"),
        )
        .expect("write");

        let state = state();
        let targets = [target("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", FolderRole::Init)];
        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned =
            crate::apply::plan(&snapshot, &model_for_an_existing_row(), &targets).expect("plans");
        let after = &planned[0].prepared.after;

        // Nothing is remembered, so the row is appended as new — and the old one
        // stays exactly as it was.
        assert!(after.contains("picus: generated"), "{after}");
        assert!(after.contains("'SOGLIA', 10"), "the original is untouched:\n{after}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_key_column_the_older_rows_predate_is_named_in_the_diff() {
        // The failure that is invisible without this: the comparison key includes a
        // column added later — an audit flag — so every row already in the scripts
        // is unmatchable, and the block is appended beside them in silence. The
        // diff now says which column it is, which is the difference between "Picus
        // is broken" and "take CUSTOMIZED out of the key".
        let root = repository("key-gap");
        let state = state();
        let targets = [target("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", FolderRole::Init)];

        let column = |name: &str, ty: &str| Column {
            name: name.to_string(),
            data_type: ty.to_string(),
            primary_key: false,
            not_null: false,
            default_value: None,
        };
        let mut row = std::collections::BTreeMap::new();
        row.insert("COD".to_string(), "SOGLIA".to_string());
        row.insert("VALORE".to_string(), "99".to_string());
        row.insert("PERSONALIZZATO".to_string(), "0".to_string());
        let wide_key = DmlModel {
            columns: vec![
                column("COD", "varchar(30)"),
                column("VALORE", "numeric"),
                column("PERSONALIZZATO", "numeric"),
            ],
            // The trap: a key naming a column the file's own INSERT never mentions.
            key_columns: vec![column("COD", "varchar(30)"), column("PERSONALIZZATO", "numeric")],
            rows: vec![row],
            ..model()
        };

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned = crate::apply::plan(&snapshot, &wide_key, &targets).expect("plans");
        let reasons = planned[0].prepared.reasons.join(" ");
        assert!(reasons.contains("PERSONALIZZATO"), "the diff has to name it: {reasons}");
        assert!(reasons.contains("comparison key"), "{reasons}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_key_one_column_too_wide_is_diagnosed_by_naming_the_column() {
        // The failure with no visible symptom: every key column IS named by the
        // scripts, so nothing looks wrong — the key simply includes a **value**
        // column, and comparing on it can never match a row whose value is what is
        // being changed. Without naming the column there is nothing to act on.
        let root = repository("key-too-wide");
        let state = state();
        let targets = [target("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", FolderRole::Init)];

        let named = |name: &str, ty: &str| Column {
            name: name.to_string(),
            data_type: ty.to_string(),
            primary_key: false,
            not_null: false,
            default_value: None,
        };
        let mut row = std::collections::BTreeMap::new();
        row.insert("COD".to_string(), "SOGLIA".to_string());
        row.insert("VALORE".to_string(), "99".to_string());
        let over_keyed = DmlModel {
            // VALORE is what is being changed, so it cannot also identify the row.
            key_columns: vec![named("COD", "varchar(30)"), named("VALORE", "numeric")],
            rows: vec![row],
            ..model()
        };

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned = crate::apply::plan(&snapshot, &over_keyed, &targets).expect("plans");
        let reasons = planned[0].prepared.reasons.join(" ");

        assert!(reasons.contains("compared on COD, VALORE"), "say what was compared: {reasons}");
        assert!(
            reasons.contains("nearest row in the scripts differs on VALORE"),
            "and which column the nearest row disagrees on: {reasons}"
        );
        assert!(reasons.contains("comparison key"), "{reasons}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_table_nothing_installs_says_the_row_is_new() {
        // The other kind of nothing, which used to look identical.
        let root = repository("never-installed");
        let state = state();
        let targets = [target("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", FolderRole::Init)];
        let elsewhere = DmlModel { table: "CATALOGO_WIDGET".to_string(), ..model() };

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned = crate::apply::plan(&snapshot, &elsewhere, &targets).expect("plans");
        let reasons = planned[0].prepared.reasons.join(" ");
        assert!(reasons.contains("nothing in this engine's scripts inserts into CATALOGO_WIDGET"), "{reasons}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_row_nobody_installs_is_still_a_plain_insert() {
        // The guard on the whole feature: reconciliation must not change what
        // happens in the ordinary case.
        let root = repository("replace-none");
        let state = state();
        let targets = [target("ORACLE/AGGIORNAMENTO/4_13__4_14.sql", FolderRole::Update)];

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned = crate::apply::plan(&snapshot, &model(), &targets).expect("plans");
        let after = &planned[0].prepared.after;
        assert!(after.contains("INSERT INTO PARAMETRI"), "{after}");
        assert!(!after.contains("DELETE FROM"), "nothing installs SOGLIA_SCONTO:\n{after}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn statements_land_inside_a_block_that_already_guards_the_same_versions() {
        // A second block on one version range would run twice on a fresh install
        // and, on an upgraded database, find the version already carried forward
        // and do nothing at all.
        let root = repository("existing-guard");
        let guarded = "-- 4.13 -> 4.14\r\n\
                       DECLARE\r\n\
                       \x20 v_version VARCHAR2(30);\r\n\
                       BEGIN\r\n\
                       \x20 SELECT VERSIONE INTO v_version FROM VERSIONE_DB;\r\n\
                       \x20 IF v_version <> '4.13' THEN\r\n\
                       \x20   RETURN;\r\n\
                       \x20 END IF;\r\n\
                       \x20 INSERT INTO LISTINI (COD) VALUES ('L');\r\n\
                       \x20 UPDATE VERSIONE_DB SET VERSIONE = '4.14';\r\n\
                       \x20 COMMIT;\r\n\
                       END;\r\n/\r\n";
        std::fs::write(
            root.join("ORACLE/AGGIORNAMENTO/4_13__4_14.sql"),
            cp1252(guarded),
        )
        .expect("write");

        let state = state();
        let mut guarded_target = target("ORACLE/AGGIORNAMENTO/4_13__4_14.sql", FolderRole::Update);
        guarded_target.wrap = TargetWrap::Block;
        guarded_target.guards.version =
            Some(picus_ast::prelude::VersionGuard { from: "4.13".into(), to: "4.14".into() });
        let targets = [guarded_target];

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned = crate::apply::plan(&snapshot, &model(), &targets).expect("plans");
        let after = &planned[0].prepared.after;

        // One guard, not two.
        assert_eq!(after.matches("IF v_version <> '4.13'").count(), 1, "{after}");
        assert_eq!(after.matches("DECLARE").count(), 1, "{after}");
        // The new statement sits above the UPDATE that carries the version on.
        let inserted = after.find("SOGLIA_SCONTO").expect("the row was written");
        let carries = after.find("UPDATE VERSIONE_DB").expect("the guard still closes");
        assert!(inserted < carries, "the row has to run before the version moves:\n{after}");
        // And it carries a marker, which is what lets the next run find it again
        // instead of adding a second copy inside the same guard.
        assert!(after.contains("picus: generated"), "{after}");

        // Which is the property worth asserting: generate the same thing again and
        // nothing at all changes.
        let bytes = encoding_of("windows-1252", after);
        std::fs::write(root.join("ORACLE/AGGIORNAMENTO/4_13__4_14.sql"), bytes).expect("write");
        state.scripts().invalidate(&root);
        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("re-opens");
        let again = crate::apply::plan(&snapshot, &model(), &targets).expect("re-plans");
        assert!(
            again[0].prepared.is_noop(),
            "a second run inside the same guard must change nothing:\n{}",
            again[0].prepared.after
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_generation_is_previewed_written_and_then_finds_itself_again() {
        let root = repository("apply");
        let state = state();
        let targets = [
            target("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", FolderRole::Init),
            target("ORACLE/AGGIORNAMENTO/4_13__4_14.sql", FolderRole::Update),
        ];

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let planned = crate::apply::plan(&snapshot, &model(), &targets).expect("plans");
        assert_eq!(planned.len(), 2);

        // An existing file: marked, and the accented comment above it untouched.
        let existing = &planned[0];
        assert!(!existing.prepared.creates_file());
        assert!(existing.prepared.after.starts_with("-- soglia già applicata\r\n"));
        assert!(existing.prepared.after.contains("-- picus: generated PARAMETRI"));
        assert!(existing.prepared.after.contains("SOGLIA_SCONTO"));
        // Generated SQL arrives with \n and lands with the file's own CRLF.
        assert!(!existing.prepared.after.contains("\n\r"));
        assert!(!existing.prepared.after.replace("\r\n", "").contains('\n'));
        assert_eq!(existing.digest.len(), 64, "an existing file has a digest");

        // A file that does not exist yet: created with its folder's conventions.
        let created = &planned[1];
        assert!(created.prepared.creates_file());
        assert_eq!(created.digest, "", "a file that is not there has no digest");
        assert_eq!(created.prepared.encoding, "windows-1252");

        // Nothing has been written yet.
        assert!(!root.join("ORACLE/AGGIORNAMENTO/4_13__4_14.sql").exists());

        let approved: std::collections::BTreeMap<String, String> =
            planned.iter().map(|f| (f.path.clone(), f.digest.clone())).collect();
        let current: Vec<(String, String)> =
            planned.iter().map(|f| (f.path.clone(), f.digest.clone())).collect();
        crate::apply::unchanged_since_preview(&current, &approved).expect("nothing moved");

        let prepared: Vec<_> = planned.into_iter().map(|f| f.prepared).collect();
        let applied = commit(&prepared).expect("writes");
        assert_eq!(applied.written.len(), 1);
        assert_eq!(applied.created.len(), 1);

        // What landed is what the preview showed, byte for byte.
        let bytes = std::fs::read(root.join("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql")).unwrap();
        assert_eq!(bytes, encoding_of("windows-1252", &prepared[0].after));

        // Read it again and generate the same thing: the block is found and
        // replaced, so the second apply changes nothing at all.
        state.scripts().invalidate(&root);
        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("re-opens");
        let again = crate::apply::plan(&snapshot, &model(), &targets).expect("re-plans");
        for file in &again {
            assert!(
                file.prepared.is_noop(),
                "regenerating an unchanged block must not touch {}:\n{}",
                file.path,
                file.prepared.after
            );
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_edited_between_the_preview_and_the_write_stops_the_write() {
        let root = repository("stale");
        let state = state();
        let targets = [target("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", FolderRole::Init)];

        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("opens");
        let previewed = crate::apply::plan(&snapshot, &model(), &targets).expect("plans");
        let approved: std::collections::BTreeMap<String, String> =
            previewed.iter().map(|f| (f.path.clone(), f.digest.clone())).collect();

        // Somebody else saves the file.
        let path = root.join("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql");
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.extend_from_slice(&cp1252("-- toccato da qualcun altro\r\n"));
        std::fs::write(&path, &bytes).unwrap();

        // The apply re-reads, and what it finds no longer matches what was shown.
        state.scripts().invalidate(&root);
        let snapshot = snapshot_for(&state, &root.to_string_lossy()).expect("re-opens");
        let now = crate::apply::plan(&snapshot, &model(), &targets).expect("re-plans");
        let current: Vec<(String, String)> =
            now.iter().map(|f| (f.path.clone(), f.digest.clone())).collect();

        let err = crate::apply::unchanged_since_preview(&current, &approved).unwrap_err();
        assert!(err.contains("02_PARAMETRI.sql"), "{err}");
        assert!(err.contains("changed on disk since the preview"), "{err}");

        // And the file is exactly as the other person left it.
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(digest(&std::fs::read(&path).unwrap()).len(), 64);

        let _ = std::fs::remove_dir_all(&root);
    }
}

