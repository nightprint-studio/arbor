//! [`ObjectEntry`] and [`ObjectSite`] — one row of the inventory, and every place
//! the object was named.
//!
//! The row carries two different things and they answer different questions:
//!
//! * **coverage** — a count per **folder path**, which is what the inventory view
//!   renders and what `CONS001` reads. It is deliberately a count of *statements*,
//!   not of occurrences: a statement that names `PARAMETRI` four times has done
//!   one thing to it.
//! * **sites** — where each of those statements is, which is what a rule needs
//!   when it has to point at a line and what "go to definition" follows.

use std::collections::BTreeMap;

use picus_parse::prelude::{ByteRange, ObjectKind};
use picus_types::prelude::{DialectScope, EngineKind, FolderRole};

use crate::kind::InventoryKind;

/// One place an object was named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSite {
    /// Project-relative path, POSIX separators.
    pub path: String,
    /// The folder holding the file — the coverage column this site counts under.
    pub folder_path: String,
    /// What that folder's SQL has to be valid in. `None` when no folder above it
    /// declares an engine, and rules that compare dialects leave those sites
    /// alone. `Portable` for a folder written for **both**, which is why lookups
    /// go through `covers` rather than an equality test.
    pub scope: Option<DialectScope>,
    pub role: FolderRole,
    /// Index into the file's `statements`, so a caller can get back to the whole
    /// statement without searching by range.
    pub statement_index: usize,
    /// The bytes of the name as written, in that file's source.
    pub range: ByteRange,
    /// 1-based line of the name.
    pub line: usize,
    /// The exact kind the source said, before [`InventoryKind`] folded it. A
    /// package spec and a package body share a row but not a `declared_kind`,
    /// which is what stops "spec here, body there" reading as a duplicate.
    pub declared_kind: ObjectKind,
    /// The statement creates or redefines the object (`CREATE …` / `ALTER …`).
    pub defining: bool,
    /// …and it is a `CREATE`, not an `ALTER`.
    ///
    /// The distinction earns its keep in `DUP002`: a table created in the
    /// initialisation folder and altered by three update scripts is a completely
    /// ordinary repository, and counting the ALTERs as definitions would report
    /// every long-lived table as defined four times.
    pub creating: bool,
    /// The statement **changes** the object, rather than merely reading it.
    ///
    /// True for a `CREATE`/`ALTER`, for the target of an `INSERT`/`UPDATE`/
    /// `DELETE`/`MERGE`, and for what a `DROP` or `TRUNCATE` names. False for a
    /// table read in a `FROM`, a `JOIN`, a subquery, or named as the parent of a
    /// foreign key.
    ///
    /// The distinction the grammar does not make: `CREATE VIEW v AS SELECT … FROM
    /// mecatalogo` and `INSERT INTO mecatalogo …` both leave a plain reference,
    /// and only one of them is something the other dialect's scripts ought to be
    /// doing too. Without this, a view reading a table installed by *another*
    /// repository made `CONS001` report that table as untouched by whichever
    /// dialect happened not to read it — a gap in scripts that never installed it
    /// in the first place, and one nobody could close.
    pub writing: bool,
    /// …and it was written `CREATE OR REPLACE`.
    ///
    /// Kept apart from [`creating`](Self::creating), which stays true: the
    /// statement does create the object, and the drill-down is right to say so.
    /// What this adds is the author's stated **intent** — "whatever was there,
    /// this is the definition now" — which is what stops `DUP002` reporting the
    /// throwaway wrapper function that every update script in a repository
    /// defines, calls and replaces.
    pub replacing: bool,
}

impl ObjectSite {
    /// `path:line` — the form the interface shows and `alsoAt` carries.
    pub fn location(&self) -> String {
        format!("{}:{}", self.path, self.line)
    }
}

/// One object, everywhere it appears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectEntry {
    /// The comparison form of the name — see the crate README for the folding
    /// rule. Unqualified: the schema an object was written under is dropped.
    pub name: String,
    pub kind: InventoryKind,
    /// Statements touching this object, per **folder path**. Every column the
    /// project has is present, including the zeroes.
    pub coverage: BTreeMap<String, usize>,
    /// Every place the object was named, in file then source order.
    pub sites: Vec<ObjectSite>,
}

impl ObjectEntry {
    pub fn coverage_in(&self, folder_path: &str) -> usize {
        self.coverage.get(folder_path).copied().unwrap_or(0)
    }

    /// Does this repository only ever **read** this object?
    ///
    /// Nothing anywhere creates it, alters it, writes to it, drops it or empties
    /// it — every mention is a `FROM`, a `JOIN` or a foreign key pointing at it.
    /// Which means it is somebody else's: a table installed by another repository,
    /// read here by a view.
    ///
    /// Reported rather than hidden, because "we read a table we do not own" is
    /// worth being able to see — and it is one of the more useful things this
    /// index knows. What it must not do is **count as a gap**: a column of zeroes
    /// on an object no engine's scripts were ever going to install is not a
    /// difference between the two engines, it is a fact about the boundary of the
    /// repository.
    pub fn is_external(&self) -> bool {
        !self.sites.is_empty() && !self.sites.iter().any(|s| s.writing)
    }

    /// Sites where the object is created — never where it is altered.
    pub fn creations(&self) -> impl Iterator<Item = &ObjectSite> {
        self.sites.iter().filter(|s| s.creating)
    }

    /// Sites in one `(dialect, role)` lane where the scripts **change** the
    /// object — the unit the cross-dialect gap rule compares.
    ///
    /// Reads are deliberately not here. `CONS001` exists to say "this change
    /// landed in one engine's scripts and not the other's", and its own
    /// consequence is written in terms of shape and data; a table a view happens
    /// to read is neither, and is very often installed by a repository these
    /// scripts do not own.
    pub fn writes_in(
        &self,
        dialect: EngineKind,
        role: FolderRole,
    ) -> impl Iterator<Item = &ObjectSite> {
        self.sites_in(dialect, role).filter(|s| s.writing)
    }

    /// Sites in one `(dialect, role)` lane — every mention, reads included.
    pub fn sites_in(
        &self,
        dialect: EngineKind,
        role: FolderRole,
    ) -> impl Iterator<Item = &ObjectSite> {
        self.sites
            .iter()
            .filter(move |s| s.scope.map(|x| x.covers(dialect)).unwrap_or(false) && s.role == role)
    }
}
