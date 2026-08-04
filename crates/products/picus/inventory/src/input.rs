//! [`ParsedProject`] — the bundle every consumer of the script half reads.
//!
//! A parsed file on its own says nothing useful about consistency: the same
//! INSERT means one thing in an initialisation folder and another in an update
//! folder, and *which dialect it is written in* is the whole point. So the input
//! to both this crate and `picus-analyze` is the parse **joined to the project
//! tree**, and the join is done once, here, rather than re-derived by every rule.
//!
//! The join is to a folder, and the folder already knows everything a rule asks:
//! its dialect and its role are the resolved ones, inherited from wherever in the
//! tree they were declared.
//!
//! Deliberately borrowed throughout. A `ParsedFile` is a map of a string the
//! caller still owns (`picus-parse`'s invariant); copying either into this crate
//! would double the memory of a large repository for no gain and would make the
//! byte ranges point at a second copy of the text.

use std::collections::HashMap;

use picus_parse::prelude::ParsedFile;
use picus_project::prelude::{FolderNode, Project, ScriptFile};
use picus_types::prelude::{DialectScope, EngineKind, FolderEngine, FolderRole};

/// One parsed script, keyed by the path that joins it to the project tree.
#[derive(Debug, Clone, Copy)]
pub struct ParsedScript<'a> {
    /// Project-relative path, POSIX separators — the same string
    /// [`ScriptFile::path`] holds. This is the identity of a file everywhere in
    /// Picus, including on Windows.
    pub path: &'a str,
    /// The decoded text every byte range in `parsed` indexes into.
    ///
    /// *Decoded*: the file may be windows-1252 on disk, and every range here is
    /// an offset into the `str`, never into the bytes that were read.
    pub source: &'a str,
    pub parsed: &'a ParsedFile,
}

/// Where a file sits in the repository, and therefore what is expected of it.
///
/// Two facts, from two levels. The **role** is the folder's — a directory of
/// scripts is *for* something and the file beside this one is for the same thing.
/// The **engine** is the file's, which for all but a handful of files is its
/// folder's anyway; the exceptions are the untidy repositories where both engines
/// share a directory and only the file name knows which is which.
#[derive(Debug, Clone, Copy)]
pub struct Placement<'a> {
    pub folder: &'a FolderNode,
    pub file: &'a ScriptFile,
    /// The folder holds files of more than one engine, so its coverage column is
    /// split per engine — see [`coverage_key`](Self::coverage_key).
    ///
    /// Decided once per folder when the tree is joined, rather than re-derived
    /// per statement: it is a question about every file in the folder, and asking
    /// it in the indexing loop would be a scan per statement.
    split: bool,
}

impl<'a> Placement<'a> {
    /// The column this file's statements count towards.
    ///
    /// **The folder's path**, which is its identity everywhere else too — except
    /// in a folder that holds more than one engine, where the path alone would
    /// merge the Oracle statements and the PostgreSQL ones into a single number
    /// and destroy the one comparison this table exists to make. There the column
    /// is split, and the engine is named in the header.
    ///
    /// Tidy repositories — the overwhelming majority, and every repository that
    /// existed before a file could carry an engine — are unaffected: one engine
    /// per folder means one column per folder, spelled exactly as before.
    pub fn coverage_key(&self) -> String {
        coverage_key(self.folder, self.file, self.split)
    }

    /// The **single** dialect this script is written in, or `None` when there is
    /// not exactly one — nobody declared an engine, the engine is one Picus does
    /// not read, or it is **portable** and answers for both.
    ///
    /// A rule asking "which side of the comparison is this" wants
    /// [`covers`](Self::covers); this one is for the rules that need one dialect
    /// and genuinely have nothing to say without it.
    pub fn effective_dialect(&self) -> Option<EngineKind> {
        self.file.effective_dialect()
    }

    /// What this script has to be valid in — the parse and emit target.
    ///
    /// `None` only for an engine Picus does not read and for one nobody declared.
    pub fn scope(&self) -> Option<DialectScope> {
        self.file.scope()
    }

    /// Does what is written here count as present for `dialect`?
    ///
    /// True of **both** for a portable script. Every cross-dialect rule asks this
    /// rather than comparing `effective_dialect`, because a row inserted by a
    /// portable script really is present on both engines and reporting it as
    /// missing from either would be reporting the opposite of the truth.
    pub fn covers(&self, dialect: EngineKind) -> bool {
        self.file.covers(dialect)
    }

    /// Every dialect this placement answers for — two for a portable script.
    pub fn dialects(&self) -> &'static [EngineKind] {
        self.file.effective_engine.map(|e| e.dialects()).unwrap_or(&[])
    }

    /// Portable SQL: written to run on every dialect Picus supports.
    pub fn is_generic(&self) -> bool {
        self.file.is_generic()
    }

    /// What the folder is for, after inheritance.
    pub fn effective_role(&self) -> FolderRole {
        self.folder.effective_role
    }

    /// Which installed product this script belongs to, after inheritance.
    ///
    /// `None` for the ordinary repository, which installs one thing — and for
    /// every repository written before anyone declared a product, which is why
    /// the rules that read it have to treat "nobody said" as one group rather
    /// than as a difference.
    ///
    /// The distinction it buys is the one a repository installing **two**
    /// databases cannot express any other way: a version table created by one
    /// module's initialisation and again by the other's is two tables in two
    /// databases, not one table created twice.
    pub fn product(&self) -> Option<&'a str> {
        self.folder.effective_product.as_deref()
    }
}

/// Does this folder hold files of more than one engine?
///
/// Files Picus does not read are left out of the count: a stray T-SQL script does
/// not make an otherwise Oracle folder "mixed", because it contributes no column
/// either way.
fn is_split(folder: &FolderNode) -> bool {
    let mut seen: Option<Option<FolderEngine>> = None;
    for file in folder.files.iter().filter(|f| !f.is_out_of_scope()) {
        match seen {
            None => seen = Some(file.effective_engine),
            Some(first) if first != file.effective_engine => return true,
            Some(_) => {}
        }
    }
    false
}

/// The coverage column one file counts under.
///
/// The single definition, called both by [`Placement::coverage_key`] and by
/// [`ParsedProject::coverage_keys`]. They have to produce the same strings or the
/// table gains a column nothing counts towards and loses one that does — so they
/// go through one function rather than two implementations of one rule.
fn coverage_key(folder: &FolderNode, file: &ScriptFile, split: bool) -> String {
    if !split {
        return folder.path.clone();
    }
    let engine = file.effective_engine.map(FolderEngine::label).unwrap_or(UNCLASSIFIED);
    format!("{}{COLUMN_SEPARATOR}{engine}", folder.path)
}

/// Between a folder's path and the engine, when a column is split. A character
/// that cannot occur in a path, so the key stays unambiguous.
const COLUMN_SEPARATOR: &str = " · ";

/// How the leftover files of a mixed folder are labelled — the ones nobody has
/// classified yet. Named rather than blank: a column headed with nothing reads
/// like a rendering bug.
const UNCLASSIFIED: &str = "unclassified";

/// A repository's tree with its files parsed.
#[derive(Debug)]
pub struct ParsedProject<'a> {
    project: &'a Project,
    scripts: Vec<ParsedScript<'a>>,
    placement: HashMap<&'a str, Placement<'a>>,
    /// Path to index into `scripts`, so [`script_of`](Self::script_of) is a
    /// lookup rather than a scan. Built here because a linear `find` on a public
    /// accessor is a trap: it costs nothing until somebody calls it in a loop
    /// over every file, and then it is quadratic and nobody knows why.
    by_path: HashMap<&'a str, usize>,
    orphans: Vec<&'a str>,
}

impl<'a> ParsedProject<'a> {
    /// Join parses to the tree.
    ///
    /// A parse whose path is not in the tree is **not** dropped silently: it goes
    /// to [`ParsedProject::orphans`], because the only way to get one is a caller
    /// bug (a stale path, a file parsed after a rescan removed it) and a silently
    /// ignored file is how a consistency tool comes to report "all clear" on a
    /// repository it never read.
    pub fn new(project: &'a Project, scripts: Vec<ParsedScript<'a>>) -> Self {
        let mut placement: HashMap<&'a str, Placement<'a>> = HashMap::new();
        for folder in project.walk() {
            let split = is_split(folder);
            for file in &folder.files {
                placement.insert(file.path.as_str(), Placement { folder, file, split });
            }
        }
        let orphans = scripts
            .iter()
            .filter(|s| !placement.contains_key(s.path))
            .map(|s| s.path)
            .collect();
        let by_path =
            scripts.iter().enumerate().map(|(index, script)| (script.path, index)).collect();
        ParsedProject { project, scripts, placement, by_path, orphans }
    }

    pub fn project(&self) -> &'a Project {
        self.project
    }

    /// Every script that has a place in the tree, with that place.
    pub fn placed(&self) -> impl Iterator<Item = (&ParsedScript<'a>, Placement<'a>)> {
        self.scripts.iter().filter_map(|s| self.placement.get(s.path).map(|p| (s, *p)))
    }

    /// Parses whose path is not in the tree.
    pub fn orphans(&self) -> &[&'a str] {
        &self.orphans
    }

    pub fn placement_of(&self, path: &str) -> Option<Placement<'a>> {
        self.placement.get(path).copied()
    }

    pub fn script_of(&self, path: &str) -> Option<&ParsedScript<'a>> {
        self.by_path.get(path).map(|index| &self.scripts[*index])
    }

    /// Every coverage column the repository has, in tree order.
    ///
    /// Produced from the **tree**, not from the parses, and that is the load-
    /// bearing part: a folder whose files were all skipped still gets a column,
    /// so its zeroes are visible. A column that only appeared when something was
    /// found would make "nothing here" indistinguishable from "nothing looked".
    ///
    /// Folders that hold no scripts of their own are left out: a directory that
    /// exists only to contain other directories has no statements to count, and a
    /// column that can never be anything but zero is noise in a table whose whole
    /// point is that a zero means something.
    ///
    /// So are files written in an engine Picus does not support. The paragraph
    /// above is exactly why: they are deliberately never parsed, so their column
    /// can only ever read zero — and a permanent row of zeroes for the SQL Server
    /// scripts would read as "these are missing everything" when the truth is
    /// "these are none of Picus's business". An unclassified file is a different
    /// case and keeps its column: there, the zeroes are the question.
    ///
    /// A folder holding more than one engine yields **one column per engine** —
    /// see [`Placement::coverage_key`], which is where that rule lives and which
    /// this walks in lockstep with.
    pub fn coverage_keys(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for folder in self.project.walk() {
            let split = is_split(folder);
            // Deduplicated within the folder, which is the only place a key can
            // repeat, and in tree order — which is what the header reads in. A
            // folder yields one column, or one per engine in it, so this stays a
            // handful of comparisons however many files it holds.
            let first = out.len();
            for file in folder.files.iter().filter(|f| !f.is_out_of_scope()) {
                let key = coverage_key(folder, file, split);
                if !out[first..].contains(&key) {
                    out.push(key);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{parsed, project};

    #[test]
    fn a_script_finds_its_folder_its_dialect_and_its_role() {
        let project = project();
        let parse = parsed("SELECT 1;", picus_parse::prelude::EngineKind::Oracle);
        let scripts = vec![ParsedScript {
            path: "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            source: "SELECT 1;",
            parsed: &parse,
        }];
        let joined = ParsedProject::new(&project, scripts);
        let place = joined.placement_of("ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql").expect("placed");
        assert_eq!(place.folder.path, "ORACLE/INIZIALIZZAZIONE");
        assert_eq!(place.coverage_key(), "ORACLE/INIZIALIZZAZIONE");
        assert_eq!(place.effective_dialect(), Some(EngineKind::Oracle));
        assert_eq!(place.effective_role(), FolderRole::Init);
        assert!(joined.orphans().is_empty());
    }

    #[test]
    fn a_parse_the_tree_does_not_know_about_is_reported_not_dropped() {
        let project = project();
        let parse = parsed("SELECT 1;", picus_parse::prelude::EngineKind::Oracle);
        let scripts =
            vec![ParsedScript { path: "ORACLE/GONE/x.sql", source: "SELECT 1;", parsed: &parse }];
        let joined = ParsedProject::new(&project, scripts);
        assert_eq!(joined.orphans(), ["ORACLE/GONE/x.sql"]);
        assert_eq!(joined.placed().count(), 0);
    }

    // ── When one folder holds two engines ─────────────────────────────────────

    use crate::testing::file_of;
    use picus_project::prelude::{resolve, FolderNode};

    /// One folder, both engines, and a script nobody classified.
    fn mixed() -> Project {
        let mut folder = FolderNode {
            role: Some(FolderRole::Update),
            files: vec![
                file_of("AGG/4_12_ORA.sql", FolderEngine::Supported(EngineKind::Oracle)),
                file_of("AGG/4_12_POS.sql", FolderEngine::Supported(EngineKind::Postgres)),
                crate::testing::file("AGG/note.sql"),
            ],
            ..FolderNode::new("AGG", "AGG")
        };
        folder.children = Vec::new();
        let mut project =
            Project { name: "P".to_string(), root: "/p".to_string(), tree: vec![folder] };
        resolve(&mut project.tree, None, None);
        project
    }

    #[test]
    fn a_mixed_folder_gets_one_column_per_engine() {
        // The whole point of the table is telling the Oracle side from the
        // PostgreSQL one. A single column here would add them together and
        // destroy exactly that.
        let project = mixed();
        let joined = ParsedProject::new(&project, Vec::new());
        assert_eq!(
            joined.coverage_keys(),
            ["AGG · Oracle", "AGG · PostgreSQL", "AGG · unclassified"]
        );

        // …and each file counts under its own.
        let ora = joined.placement_of("AGG/4_12_ORA.sql").expect("placed");
        let pos = joined.placement_of("AGG/4_12_POS.sql").expect("placed");
        assert_eq!(ora.coverage_key(), "AGG · Oracle");
        assert_eq!(pos.coverage_key(), "AGG · PostgreSQL");
        assert_eq!(ora.effective_dialect(), Some(EngineKind::Oracle));
        assert_eq!(pos.effective_dialect(), Some(EngineKind::Postgres));
        // The role is the folder's, for both.
        assert_eq!(ora.effective_role(), FolderRole::Update);
        assert_eq!(pos.effective_role(), FolderRole::Update);
    }

    #[test]
    fn a_tidy_folder_is_spelled_exactly_as_it_always_was() {
        // Every repository that existed before a file could carry an engine must
        // produce byte-identical columns, or every saved view and every habit
        // breaks for a feature it does not use.
        let project = project();
        let joined = ParsedProject::new(&project, Vec::new());
        assert!(
            joined.coverage_keys().iter().all(|k| !k.contains('·')),
            "{:?}",
            joined.coverage_keys()
        );
    }

    #[test]
    fn a_stray_unreadable_file_does_not_split_the_folder_around_it() {
        // A single T-SQL script that wandered in contributes no column either
        // way, so it must not turn its Oracle neighbours into "ORA · Oracle".
        let mut folder = FolderNode {
            role: Some(FolderRole::Update),
            files: vec![
                file_of("ORA/4_12.sql", FolderEngine::Supported(EngineKind::Oracle)),
                file_of(
                    "ORA/4_12_MSQ.sql",
                    FolderEngine::Unsupported(picus_types::prelude::ForeignEngine::SqlServer),
                ),
            ],
            ..FolderNode::new("ORA", "ORA")
        };
        folder.children = Vec::new();
        let mut project =
            Project { name: "P".to_string(), root: "/p".to_string(), tree: vec![folder] };
        resolve(&mut project.tree, None, None);

        let joined = ParsedProject::new(&project, Vec::new());
        assert_eq!(joined.coverage_keys(), ["ORA"]);
    }

    #[test]
    fn every_folder_that_holds_scripts_gets_a_column_even_with_nothing_parsed() {
        // Otherwise a folder nobody read looks exactly like a folder with no
        // findings, which is the failure this tool must never have.
        let project = project();
        let joined = ParsedProject::new(&project, Vec::new());
        assert_eq!(
            joined.coverage_keys(),
            [
                "ORACLE/AGGIORNAMENTO",
                "ORACLE/INIZIALIZZAZIONE",
                "POSTGRES/AGGIORNAMENTO",
                "POSTGRES/INIZIALIZZAZIONE",
            ]
        );
    }
}
