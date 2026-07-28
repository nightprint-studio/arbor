//! [`InventoryKind`] — which of `picus-parse`'s object kinds get a row.
//!
//! `picus-parse` names twenty-one kinds because it reports what the source says.
//! An *inventory* is a narrower thing: it lists the objects a maintainer compares
//! between dialects. A column, a constraint or a tablespace is not one of those —
//! it is part of something else, and giving it a row would bury the four hundred
//! rows that matter under four thousand that do not.
//!
//! So the set here is exactly the frontend's `ObjectKind` union
//! (`src/lib/types/picus/index.ts`), and everything else is deliberately absent.

use picus_parse::prelude::ObjectKind;
use serde::{Deserialize, Serialize};

/// Where a name is looked up — see [`InventoryKind::namespace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Namespace {
    /// Tables, views, sequences, packages, procedures, functions: one name, one
    /// object.
    Schema,
    /// Triggers, which have a namespace of their own in both engines.
    Trigger,
}

/// A kind of object the inventory indexes. Field-for-field with the frontend's
/// `ObjectKind` union — a new member has to be added on both sides.
/// `Deserialize` as well as `Serialize`: the drill-down asks for one object's
/// usages by `(kind, name)`, so the kind travels back the other way too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InventoryKind {
    Table,
    View,
    Sequence,
    Package,
    Procedure,
    Function,
    Trigger,
}

impl InventoryKind {
    /// Every kind, in the order the inventory view groups them.
    pub const ALL: [InventoryKind; 7] = [
        InventoryKind::Table,
        InventoryKind::View,
        InventoryKind::Sequence,
        InventoryKind::Package,
        InventoryKind::Procedure,
        InventoryKind::Function,
        InventoryKind::Trigger,
    ];

    /// The wire word — the same string the frontend's union uses.
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryKind::Table => "table",
            InventoryKind::View => "view",
            InventoryKind::Sequence => "sequence",
            InventoryKind::Package => "package",
            InventoryKind::Procedure => "procedure",
            InventoryKind::Function => "function",
            InventoryKind::Trigger => "trigger",
        }
    }

    /// Which row a parsed object kind belongs to. `None` means "not indexed".
    ///
    /// Two foldings are deliberate:
    ///
    /// * a **materialized view** is a view for comparison purposes — the two
    ///   dialects routinely spell the same object differently and a maintainer
    ///   comparing them wants one row, not two;
    /// * a package **body** shares the package's row, because a spec and a body
    ///   with the same name are one object to a human. They are still told apart
    ///   where it matters — [`crate::entry::ObjectSite::declared_kind`] keeps the
    ///   exact kind, so a spec in one file and a body in another is not mistaken
    ///   for the same thing defined twice.
    pub fn from_parse(kind: ObjectKind) -> Option<InventoryKind> {
        match kind {
            ObjectKind::Table => Some(InventoryKind::Table),
            ObjectKind::View | ObjectKind::MaterializedView => Some(InventoryKind::View),
            ObjectKind::Sequence => Some(InventoryKind::Sequence),
            ObjectKind::Package | ObjectKind::PackageBody => Some(InventoryKind::Package),
            ObjectKind::Procedure => Some(InventoryKind::Procedure),
            ObjectKind::Function => Some(InventoryKind::Function),
            ObjectKind::Trigger => Some(InventoryKind::Trigger),
            // Indexes, types, schemas, synonyms, columns, constraints, roles,
            // databases, tablespaces, domains, extensions and the unclassified.
            _ => None,
        }
    }

    /// Whether two objects of the same name are the same object.
    ///
    /// A trigger has its own namespace in both engines, so `TRG_ORDINI` the
    /// trigger and `TRG_ORDINI` the table can both exist and are unrelated.
    /// Everything else shares one namespace: a name is a table **or** a view
    /// **or** a sequence, never two of them.
    ///
    /// This matters because a *reference* carries no kind. `FROM V_ORDINI` is
    /// parsed as a table reference — the grammar cannot know what `V_ORDINI`
    /// turned out to be — so a view that is created once and read fifty times
    /// would otherwise produce two rows: a `view` with one statement in it and a
    /// `table` with fifty. Both wrong, and the second one is what a reader sees.
    pub fn namespace(self) -> Namespace {
        match self {
            InventoryKind::Trigger => Namespace::Trigger,
            _ => Namespace::Schema,
        }
    }

    /// How much a mention of this kind is worth believing.
    ///
    /// `Table` is what the grammar falls back to for every reference, so it is
    /// the *absence* of information rather than a claim. Anything else was
    /// spelled out — `CREATE VIEW`, `CREATE SEQUENCE` — and wins.
    pub fn confidence(self) -> u8 {
        match self {
            InventoryKind::Table => 0,
            _ => 1,
        }
    }

    /// Does this kind exist in both engines?
    ///
    /// `false` for packages, which are Oracle-only. It is the single most
    /// important false positive the cross-dialect rules have to avoid: an Oracle
    /// package has no PostgreSQL counterpart to be missing from, and reporting
    /// one would put a permanent, unfixable finding at the top of the report.
    pub fn exists_in_both_engines(self) -> bool {
        !matches!(self, InventoryKind::Package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_words_match_the_frontend_union() {
        // `src/lib/types/picus/index.ts` declares exactly these seven.
        let words: Vec<&str> = InventoryKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(
            words,
            ["table", "view", "sequence", "package", "procedure", "function", "trigger"]
        );
        for kind in InventoryKind::ALL {
            assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{}\"", kind.as_str()));
        }
    }

    #[test]
    fn a_package_body_shares_the_packages_row() {
        assert_eq!(InventoryKind::from_parse(ObjectKind::PackageBody), Some(InventoryKind::Package));
        assert_eq!(
            InventoryKind::from_parse(ObjectKind::MaterializedView),
            Some(InventoryKind::View)
        );
    }

    #[test]
    fn the_kinds_that_are_part_of_something_else_get_no_row() {
        for kind in [
            ObjectKind::Column,
            ObjectKind::Constraint,
            ObjectKind::Index,
            ObjectKind::Tablespace,
            ObjectKind::Role,
            ObjectKind::Unknown,
        ] {
            assert_eq!(InventoryKind::from_parse(kind), None, "{kind:?} must not be indexed");
        }
    }

    #[test]
    fn only_packages_are_single_engine() {
        assert!(!InventoryKind::Package.exists_in_both_engines());
        for kind in InventoryKind::ALL.iter().filter(|k| **k != InventoryKind::Package) {
            assert!(kind.exists_in_both_engines());
        }
    }
}
