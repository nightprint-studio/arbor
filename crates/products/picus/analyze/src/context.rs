//! [`Context`] — the lookups every rule needs, resolved once.
//!
//! Nothing here is a rule. It is the answers to the questions a rule keeps
//! asking: which dialects the repository actually has, which folders play a role
//! for one of them, what this folder's naming scheme is, what the version table
//! is called in comparison form. Resolving them in one place is not tidiness — it
//! is the difference between one rule reading the configuration slightly
//! differently from its neighbour and every rule agreeing.
//!
//! ## The unit of comparison is a **lane**, not a branch
//!
//! `(dialect, role)`. The folders that initialise Oracle are one lane, the
//! folders that initialise PostgreSQL are another, and the cross-dialect rules
//! compare the two. A lane is plural on purpose: a repository that keeps its
//! updates in `AGGIORNAMENTO/2024/ORA` and `AGGIORNAMENTO/2025/ORA` still has one
//! update story, and a rule that took the first folder would compare half of it.
//!
//! Folders no ancestor declares a dialect for are in **no** lane, so they take
//! part in nothing cross-dialect. `picus-project` refuses to guess an engine, and
//! a rule that compared an unclassified folder with the Oracle ones would report
//! every object in the repository as missing from it — a first run that produces
//! nothing but noise is a tool nobody opens twice.

use picus_inventory::prelude::{Inventory, ParsedProject};
use picus_project::prelude::{EngineKind, FolderDeclaration, FolderNode, NamingScheme, ProjectConfig};
use picus_types::prelude::FolderRole;

/// What the whole analysis works against.
#[derive(Debug)]
pub struct Context<'a> {
    pub project: &'a ParsedProject<'a>,
    pub config: &'a ProjectConfig,
    pub inventory: &'a Inventory,
    /// The version table in comparison form, or `None` when the project has
    /// switched version guards off by leaving the name empty. The version rules
    /// report themselves as skipped in that state rather than passing.
    pub version_table: Option<String>,
    /// Every dialect the repository answers for, resolved once.
    dialects: Vec<EngineKind>,
    /// Every lane, resolved once — `(dialect, role)` to the folders in it.
    ///
    /// There are `dialects × roles` of them, which is at most ten, and each is one
    /// walk of the tree. Computed here because the alternative is what this used
    /// to do: a fresh walk and a fresh `Vec` on **every** call, and the
    /// cross-dialect rules ask about eight lanes per object in the inventory. On
    /// a repository with a thousand objects that is eight thousand walks of the
    /// whole tree to answer ten distinct questions.
    lanes: Vec<((EngineKind, FolderRole), Vec<&'a FolderNode>)>,
}

impl<'a> Context<'a> {
    pub fn new(
        project: &'a ParsedProject<'a>,
        config: &'a ProjectConfig,
        inventory: &'a Inventory,
    ) -> Context<'a> {
        let table = config.version_table.table.trim();
        let version_table = (!table.is_empty()).then(|| fold_identifier(table));
        let dialects = project.project().dialects();
        let lanes = dialects
            .iter()
            .flat_map(|dialect| {
                FolderRole::ALL.iter().map(move |role| {
                    ((*dialect, *role), project.project().lane(*dialect, *role).collect())
                })
            })
            .collect();
        Context { project, config, inventory, version_table, dialects, lanes }
    }

    /// Every dialect the repository declares somewhere, in a stable order.
    pub fn dialects(&self) -> Vec<EngineKind> {
        self.dialects.clone()
    }

    /// The folders that play `role` for `dialect`.
    pub fn lane(&self, dialect: EngineKind, role: FolderRole) -> &[&'a FolderNode] {
        self.lanes
            .iter()
            .find(|((d, r), _)| *d == dialect && *r == role)
            .map(|(_, folders)| folders.as_slice())
            .unwrap_or(&[])
    }

    /// Every folder of the repository, whatever it resolved to.
    pub fn folders(&self) -> impl Iterator<Item = &'a FolderNode> {
        self.project.project().walk()
    }

    /// What a folder or one of its ancestors declares, if anything.
    pub fn declaration(&self, folder: &FolderNode) -> Option<&'a FolderDeclaration> {
        self.config.declaration(&folder.path)
    }

    /// The naming scheme in force for a folder — the nearest declared one, or the
    /// project's.
    pub fn naming_for(&self, folder: &FolderNode) -> &'a NamingScheme {
        self.config.naming_for(&folder.path)
    }
}

/// The display name of an engine, as the interface writes it.
///
/// Lives here rather than in one rule because five of them write it into a
/// message, and a dialect that was "PostgreSQL" in one finding and "postgres" in
/// the next would read as two different things.
pub fn engine_label(engine: EngineKind) -> &'static str {
    match engine {
        EngineKind::Oracle => "Oracle",
        EngineKind::Postgres => "PostgreSQL",
    }
}

/// An identifier in comparison form: unquoted folds to upper case, quoted keeps
/// its contents.
///
/// The same rule as `picus-parse`'s `ObjectRef::folded_name`, applied to a name
/// that comes from the configuration rather than from the source. It is repeated
/// here only because that crate exposes the fold on a parsed reference and not on
/// a bare string; if it ever exposes `fold_name(&str)`, this goes.
pub fn fold_identifier(written: &str) -> String {
    let written = written.trim();
    if written.len() >= 2 && written.starts_with('"') && written.ends_with('"') {
        written[1..written.len() - 1].replace("\"\"", "\"")
    } else {
        written.to_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_name_folds_the_same_way_a_parsed_one_does() {
        assert_eq!(fold_identifier("versione_db"), "VERSIONE_DB");
        assert_eq!(fold_identifier(" VERSIONE_DB "), "VERSIONE_DB");
        assert_eq!(fold_identifier("\"Versione DB\""), "Versione DB");
    }
}
