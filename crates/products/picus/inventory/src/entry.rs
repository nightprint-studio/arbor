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

    /// Sites where the object is created — never where it is altered.
    pub fn creations(&self) -> impl Iterator<Item = &ObjectSite> {
        self.sites.iter().filter(|s| s.creating)
    }

    /// Sites in one `(dialect, role)` lane — the unit every cross-dialect rule
    /// compares.
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
