//! [`ParsedProject`] — the bundle every consumer of the script half reads.
//!
//! A parsed file on its own says nothing useful about consistency: the same
//! INSERT means one thing in an initialisation folder and another in an update
//! folder, and *which branch it is in* is the whole point. So the input to both
//! this crate and `picus-analyze` is the parse **joined to the project tree**,
//! and the join is done once, here, rather than re-derived by every rule.
//!
//! Deliberately borrowed throughout. A `ParsedFile` is a map of a string the
//! caller still owns (`picus-parse`'s invariant); copying either into this crate
//! would double the memory of a large repository for no gain and would make the
//! byte ranges point at a second copy of the text.

use std::collections::HashMap;

use picus_parse::prelude::ParsedFile;
use picus_project::prelude::{Branch, Project, ScriptFile, ScriptFolder};

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
    pub branch: &'a Branch,
    pub folder: &'a ScriptFolder,
    pub file: &'a ScriptFile,
}

impl Placement<'_> {
    /// The `"<branchId>/<folderId>"` column this file's statements count towards.
    pub fn coverage_key(&self) -> String {
        coverage_key(&self.branch.id, &self.folder.id)
    }
}

/// The key one coverage cell is stored under. Fixed here so the backend and the
/// interface cannot spell it differently.
pub fn coverage_key(branch_id: &str, folder_id: &str) -> String {
    format!("{branch_id}/{folder_id}")
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
        for branch in &project.branches {
            for folder in &branch.folders {
                for file in &folder.files {
                    placement.insert(file.path.as_str(), Placement { branch, folder, file });
                }
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
    pub fn coverage_keys(&self) -> Vec<String> {
        self.project
            .branches
            .iter()
            .flat_map(|b| b.folders.iter().map(move |f| coverage_key(&b.id, &f.id)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{parsed, project};

    #[test]
    fn a_script_finds_its_branch_folder_and_role() {
        let project = project();
        let parse = parsed("SELECT 1;", picus_parse::prelude::EngineKind::Oracle);
        let scripts = vec![ParsedScript {
            path: "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            source: "SELECT 1;",
            parsed: &parse,
        }];
        let joined = ParsedProject::new(&project, scripts);
        let place = joined.placement_of("ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql").expect("placed");
        assert_eq!(place.branch.id, "ora");
        assert_eq!(place.folder.id, "ora-init");
        assert_eq!(place.coverage_key(), "ora/ora-init");
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
    fn every_folder_gets_a_column_even_with_nothing_parsed() {
        // Otherwise a folder nobody read looks exactly like a folder with no
        // findings, which is the failure this tool must never have.
        let project = project();
        let joined = ParsedProject::new(&project, Vec::new());
        assert_eq!(
            joined.coverage_keys(),
            ["ora/ora-init", "ora/ora-upd", "pg/pg-init", "pg/pg-upd"]
        );
    }
}
