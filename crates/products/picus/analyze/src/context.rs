//! [`Context`] — the lookups every rule needs, resolved once.
//!
//! Nothing here is a rule. It is the answers to the questions a rule keeps
//! asking: which branches have a dialect at all, what this folder's naming scheme
//! is, what the version table is called in comparison form. Resolving them in one
//! place is not tidiness — it is the difference between one rule reading the
//! configuration slightly differently from its neighbour and every rule agreeing.

use picus_inventory::prelude::{Inventory, ParsedProject};
use picus_project::prelude::{
    Branch, EngineKind, FolderConfig, NamingScheme, ProjectConfig, ScriptFolder,
};
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
}

impl<'a> Context<'a> {
    pub fn new(
        project: &'a ParsedProject<'a>,
        config: &'a ProjectConfig,
        inventory: &'a Inventory,
    ) -> Context<'a> {
        let table = config.version_table.table.trim();
        let version_table = (!table.is_empty()).then(|| fold_identifier(table));
        Context { project, config, inventory, version_table }
    }

    /// Branches whose engine is known.
    ///
    /// A branch with no dialect is not half a branch, it is a folder nobody could
    /// identify — `picus-project` refuses to guess because a wrong guess writes
    /// Oracle syntax into a PostgreSQL file. The cross-branch rules honour the
    /// same refusal: comparing a `COMMON/` folder against the Oracle branch would
    /// report every object in the repository as missing from it.
    pub fn dialect_branches(&self) -> Vec<&'a Branch> {
        self.project.project().branches.iter().filter(|b| b.dialect.is_some()).collect()
    }

    /// The configured settings for a folder, matched on its path.
    pub fn folder_config(&self, folder: &ScriptFolder) -> Option<&'a FolderConfig> {
        self.config
            .branches
            .iter()
            .flat_map(|b| b.folders.iter())
            .find(|f| f.path == folder.path)
    }

    /// The naming scheme in force for a folder — its own, or the project's.
    pub fn naming_for(&self, folder: &ScriptFolder) -> &'a NamingScheme {
        match self.folder_config(folder) {
            Some(configured) => self.config.naming_for(configured),
            None => &self.config.naming,
        }
    }
}

/// The display name of an engine, as the interface writes it.
pub fn engine_label(engine: EngineKind) -> &'static str {
    match engine {
        EngineKind::Oracle => "Oracle",
        EngineKind::Postgres => "PostgreSQL",
    }
}

/// A branch as a person names it: the engine when it has one, the folder label
/// otherwise.
///
/// Lives here rather than in one rule because three of them write it into a
/// message, and a branch that was "PostgreSQL" in one finding and "POSTGRES" in
/// the next would read as two different things.
pub fn branch_label(branch: &Branch) -> String {
    branch.dialect.map(engine_label).map(str::to_string).unwrap_or_else(|| branch.label.clone())
}

/// The folders of one role in a branch.
///
/// Plural on purpose: a project that keeps its updates in two folders still has
/// one update story, and a rule that took the first folder would compare half of
/// it.
pub fn folders_with_role(branch: &Branch, role: FolderRole) -> impl Iterator<Item = &ScriptFolder> {
    branch.folders.iter().filter(move |f| f.role == role)
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
