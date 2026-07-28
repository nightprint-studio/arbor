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
use picus_types::prelude::{DialectScope, EngineKind, FolderRole};

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
#[derive(Debug, Clone, Copy)]
pub struct Placement<'a> {
    pub folder: &'a FolderNode,
    pub file: &'a ScriptFile,
}

impl<'a> Placement<'a> {
    /// The column this file's statements count towards: **the folder's path**,
    /// which is its identity everywhere else too.
    pub fn coverage_key(&self) -> &'a str {
        &self.folder.path
    }

    /// The **single** dialect these scripts are written in, or `None` when there
    /// is not exactly one — nobody declared an engine, the engine is one Picus
    /// does not read, or the folder is **portable** and answers for both.
    ///
    /// A rule asking "which side of the comparison is this" wants
    /// [`covers`](Self::covers); this one is for the rules that need one dialect
    /// and genuinely have nothing to say without it.
    pub fn effective_dialect(&self) -> Option<EngineKind> {
        self.folder.effective_dialect()
    }

    /// What these scripts have to be valid in — the parse and emit target.
    ///
    /// `None` only for an engine Picus does not read and for one nobody declared.
    pub fn scope(&self) -> Option<DialectScope> {
        self.folder.scope()
    }

    /// Does what is written here count as present for `dialect`?
    ///
    /// True of **both** for a portable folder. Every cross-dialect rule asks this
    /// rather than comparing `effective_dialect`, because a row inserted by a
    /// portable script really is present on both engines and reporting it as
    /// missing from either would be reporting the opposite of the truth.
    pub fn covers(&self, dialect: EngineKind) -> bool {
        self.folder.covers(dialect)
    }

    /// Every dialect this placement answers for — two for a portable folder.
    pub fn dialects(&self) -> &'static [EngineKind] {
        self.folder.effective_engine.map(|e| e.dialects()).unwrap_or(&[])
    }

    /// Portable SQL: written to run on every dialect Picus supports.
    pub fn is_generic(&self) -> bool {
        self.folder.is_generic()
    }

    /// What the folder is for, after inheritance.
    pub fn effective_role(&self) -> FolderRole {
        self.folder.effective_role
    }
}

/// A repository's tree with its files parsed.
#[derive(Debug)]
pub struct ParsedProject<'a> {
    project: &'a Project,
    scripts: Vec<ParsedScript<'a>>,
    placement: HashMap<&'a str, Placement<'a>>,
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
            for file in &folder.files {
                placement.insert(file.path.as_str(), Placement { folder, file });
            }
        }
        let orphans = scripts
            .iter()
            .filter(|s| !placement.contains_key(s.path))
            .map(|s| s.path)
            .collect();
        ParsedProject { project, scripts, placement, orphans }
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
        self.scripts.iter().find(|s| s.path == path)
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
    /// So are folders written in an engine Picus does not support. The paragraph
    /// above is exactly why: their files are deliberately never parsed, so their
    /// column can only ever read zero — and a permanent row of zeroes for the SQL
    /// Server folders would read as "these are missing everything" when the truth
    /// is "these are none of Picus's business". An unclassified folder is a
    /// different case and keeps its column: there, the zeroes are the question.
    pub fn coverage_keys(&self) -> Vec<String> {
        self.project
            .walk()
            .filter(|folder| !folder.files.is_empty() && !folder.engine_is_unsupported())
            .map(|folder| folder.path.clone())
            .collect()
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
