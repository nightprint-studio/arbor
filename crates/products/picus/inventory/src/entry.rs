//! [`ObjectEntry`] and [`ObjectSite`] — one row of the inventory, and every place
//! the object was named.
//!
//! The row carries two different things and they answer different questions:
//!
//! * **coverage** — a count per `"<branchId>/<folderId>"`, which is what the
//!   inventory view renders and what `CONS001` reads. It is deliberately a count
//!   of *statements*, not of occurrences: a statement that names `PARAMETRI` four
//!   times has done one thing to it.
//! * **sites** — where each of those statements is, which is what a rule needs
//!   when it has to point at a line and what "go to definition" follows.

use std::collections::BTreeMap;

use picus_parse::prelude::{ByteRange, ObjectKind};
use picus_types::prelude::FolderRole;

use crate::kind::InventoryKind;

/// One place an object was named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSite {
    /// Project-relative path, POSIX separators.
    pub path: String,
    pub branch_id: String,
    pub folder_id: String,
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
    /// Statements touching this object, per `"<branchId>/<folderId>"`. Every
    /// column the project has is present, including the zeroes.
    pub coverage: BTreeMap<String, usize>,
    /// Every place the object was named, in file then source order.
    pub sites: Vec<ObjectSite>,
}

impl ObjectEntry {
    pub fn coverage_in(&self, key: &str) -> usize {
        self.coverage.get(key).copied().unwrap_or(0)
    }

    /// How many statements in one branch touch this object, across every folder.
    pub fn coverage_in_branch(&self, branch_id: &str) -> usize {
        let prefix = format!("{branch_id}/");
        self.coverage.iter().filter(|(k, _)| k.starts_with(&prefix)).map(|(_, v)| *v).sum()
    }

    /// Sites where the object is created — never where it is altered.
    pub fn creations(&self) -> impl Iterator<Item = &ObjectSite> {
        self.sites.iter().filter(|s| s.creating)
    }

    /// Sites in one folder role, in one branch.
    pub fn sites_in(&self, branch_id: &str, role: FolderRole) -> impl Iterator<Item = &ObjectSite> {
        let branch = branch_id.to_string();
        self.sites.iter().filter(move |s| s.branch_id == branch && s.role == role)
    }
}
