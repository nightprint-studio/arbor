//! [`InsertionRule`] — where a generated block lands in a destination file.
//!
//! Deliberately dull, and *stated*: the rule that placed a block is written into
//! the diff's hunk header, so a predictable rule the reader can recite beats a
//! clever one they cannot. There are three, and no fourth is planned.
//!
//! ## Why the role decides
//!
//! An **update** script is a chronological log — one transition, applied whole —
//! so a new block belongs after everything already in it. An **initialisation**
//! script is read by table: the CREATE, then the rows, then the grants, grouped.
//! Dropping a new block for `PARAMETRI` at the bottom of such a file separates it
//! from the three statements it belongs with, and the next person to read the file
//! has to search for it.
//!
//! So the default is per role, and a project that disagrees says so in
//! `.arbor/picus/project.toml` under `[generation.insertion]`.
//!
//! ## Why it is stored as a string
//!
//! Same reasoning as the row limit and the naming pattern: an unrecognised value
//! must degrade to the default, not fail the whole file's parse and silently reset
//! every other setting the user had. The typed value is reached through
//! [`InsertionRule::from_wire`], never through a serde enum.

use picus_types::prelude::FolderRole;

/// Where a generated block is inserted into a destination file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionRule {
    /// After the last complete statement in the file.
    EndOfFile,
    /// After the last statement touching the same table, falling back to
    /// [`InsertionRule::EndOfFile`] when the file never mentions it.
    AfterLastOnTable,
    /// Immediately before the file's final `COMMIT`, falling back to
    /// [`InsertionRule::EndOfFile`] when there is none.
    BeforeFinalCommit,
}

impl InsertionRule {
    /// Every rule, in the order the settings UI lists them.
    pub const ALL: [InsertionRule; 3] =
        [InsertionRule::EndOfFile, InsertionRule::AfterLastOnTable, InsertionRule::BeforeFinalCommit];

    /// The stable wire string — the value in the TOML and the one the frontend
    /// sends.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::EndOfFile => "end-of-file",
            Self::AfterLastOnTable => "after-last-on-table",
            Self::BeforeFinalCommit => "before-final-commit",
        }
    }

    /// Parse a wire string; `None` for anything unrecognised, so the caller picks
    /// the fallback appropriate to what it is placing.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "end-of-file" => Some(Self::EndOfFile),
            "after-last-on-table" => Some(Self::AfterLastOnTable),
            "before-final-commit" => Some(Self::BeforeFinalCommit),
            _ => None,
        }
    }

    /// The rule a folder of this role gets when neither the project nor the user
    /// has said otherwise.
    ///
    /// `Update` appends; everything else groups by table. `Ignored` never receives
    /// a generation at all, so its answer is only ever a formality.
    pub fn default_for(role: FolderRole) -> InsertionRule {
        match role {
            FolderRole::Update => InsertionRule::EndOfFile,
            _ => InsertionRule::AfterLastOnTable,
        }
    }

    /// The rule in words, for the diff's hunk header. Single-sourced here so the
    /// preview and the settings screen cannot describe it differently.
    pub fn describe(self) -> &'static str {
        match self {
            Self::EndOfFile => "at the end of the file, after the last complete statement",
            Self::AfterLastOnTable => "after the last statement touching the same table",
            Self::BeforeFinalCommit => "immediately before the file's final COMMIT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_round_trips_through_its_wire_string() {
        for rule in InsertionRule::ALL {
            assert_eq!(InsertionRule::from_wire(rule.as_wire()), Some(rule));
        }
        assert_eq!(InsertionRule::from_wire("whatever"), None);
    }

    #[test]
    fn an_update_folder_appends_and_the_others_group_by_table() {
        // The two defaults the product decided on. A change here changes where
        // every generated block in every repository lands, so it is asserted.
        assert_eq!(InsertionRule::default_for(FolderRole::Update), InsertionRule::EndOfFile);
        assert_eq!(InsertionRule::default_for(FolderRole::Init), InsertionRule::AfterLastOnTable);
        assert_eq!(InsertionRule::default_for(FolderRole::Data), InsertionRule::AfterLastOnTable);
    }
}
