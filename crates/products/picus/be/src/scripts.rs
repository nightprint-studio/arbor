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

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arbor_fs::prelude::encoding::{decode_in_context, EncodingContext};
use picus_analyze::prelude::{analyze, Finding, RejectedSuppression, SkippedRule};
use picus_core::prelude::{digest, CachedSource, PicusState, ScriptSnapshot};
use picus_inventory::prelude::{Inventory, InventoryObject, ParsedProject, ParsedScript};
use picus_parse::prelude::{DialectScope, EngineKind, ParsedFile, SqlParser};
use picus_project::prelude::{discover, label_to_encoding, LineEnding, Project};
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

/// Discover a repository and decode every script in it.
pub(crate) fn read(root: &Path) -> Result<ScriptSnapshot, String> {
    let proposal = discover(root).map_err(|e| e.to_string())?;
    let mut problems = proposal.config.problems();
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
/// ## Files in an engine Picus does not support are not parsed
///
/// Not "parsed and then ignored" — **not parsed**. Handing T-SQL or DB2 SQL to a
/// grammar built for Oracle and PostgreSQL does not fail; it succeeds, and
/// produces a plausible-looking tree of statements that mean nothing. Everything
/// downstream then has to be trusted to throw that away, and the day one rule
/// forgets, the report is confidently wrong about somebody else's scripts.
///
/// It is also the cheapest speed there is on a real repository. Parsing is two
/// thirds of an analysis, and in the repository this was built for the SQL Server
/// and DB2 folders are roughly half the files — none of which anyone will ever
/// look at a finding for.
fn parse_all(snapshot: &ScriptSnapshot) -> Vec<(String, ParsedFile)> {
    // The skip list is built once, from the tree. Asking the tree per file would
    // be a linear walk per file — quadratic on exactly the large repositories
    // this skip exists to speed up, which would be a fine joke to leave in.
    let skip = unparsable_paths(&snapshot.project);
    let sources: Vec<&CachedSource> =
        snapshot.sources.values().filter(|s| !skip.contains(s.path.as_str())).collect();
    // Below this, threads cost more than they save — and it keeps the common case
    // of a handful of files on one obvious code path.
    if sources.len() < PARALLEL_PARSE_THRESHOLD {
        let mut parser = SqlParser::new();
        return sources.iter().map(|s| parse_one(&mut parser, snapshot, s)).collect();
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
                scope.spawn(move || {
                    let mut parser = SqlParser::new();
                    slice.iter().map(|s| parse_one(&mut parser, snapshot, s)).collect::<Vec<_>>()
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
    snapshot: &ScriptSnapshot,
    source: &CachedSource,
) -> (String, ParsedFile) {
    let scope = scope_of(&snapshot.project, &source.path);
    (source.path.clone(), parser.parse(&source.text, scope))
}

/// The files Picus should not read the SQL of at all.
///
/// Only the folders written in an engine Picus **recognises and does not
/// support**. A folder nobody has classified yet is still parsed: it is a
/// question, not an answer, and its inventory is part of what makes the question
/// answerable — the user is choosing between engines partly on what the files say.
///
/// The distinction is the whole reason those two states are not one value.
fn unparsable_paths(project: &Project) -> HashSet<&str> {
    project
        .walk()
        .filter(|folder| folder.engine_is_unsupported())
        .flat_map(|folder| folder.files.iter().map(|file| file.path.as_str()))
        .collect()
}

/// What a file is parsed as — its folder's scope, always, inherited from wherever
/// in the tree it was declared.
///
/// **A portable folder is parsed as `Portable`**, not as one of its dialects, and
/// that is the whole of what `DIA001`'s inversion needs from this layer: under
/// `Portable` the parser accepts the syntax of neither engine, so a construct
/// belonging to either lands in `foreign` and the rule reports the broken promise.
/// Parsing such a file as Oracle would silently hide every Oracle-ism in it —
/// exactly the ones the folder must not contain.
///
/// A folder nobody could identify has no scope, and the fallback is genuinely
/// arbitrary rather than a guess: the grammar is one permissive superset of both
/// dialects, so the statements, the objects and the DML come out the same either
/// way. The engine decides only which constructs count as *foreign*, and `DIA001`
/// refuses to report those for a folder with no engine — so nothing a user ever
/// sees depends on this choice.
///
/// It is a fallback for the **unclassified** case only. A folder in an engine
/// Picus does not support never reaches here, because [`unparsable_paths`] kept it
/// out — the fallback above would be a real guess there, and a wrong one.
fn scope_of(project: &Project, path: &str) -> DialectScope {
    project.scope_of(path).unwrap_or(DialectScope::One(EngineKind::Postgres))
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

