//! The schema, as the **host** hands it over.
//!
//! A plain data struct rather than a trait, and that is a decision worth stating:
//! a trait would make every host write an impl and every test write a mock, and
//! there is nothing here to abstract over — it is four fields of facts a host
//! already has. Building one is a `map` over whatever the host's own schema type
//! is; building one in a test is three lines.
//!
//! **Build it once per connection and keep it.** Nothing in this crate mutates it
//! and nothing caches on the host's behalf, so a host that rebuilds it per
//! keystroke — completion runs on every keystroke — will pay for the whole schema
//! on every keystroke. That is the obvious misuse and it is the only performance
//! note this crate has.

use serde::{Deserialize, Serialize};

/// How coarse a column's type is, for the one decision that depends on it:
/// whether a value needs quotes around it.
///
/// Deliberately five words rather than a type system. The mapping from
/// `character varying(30)` / `NUMBER(10,2)` / `timestamptz` to one of these is the
/// **host's** problem: the host is the one that knows which engine reported the
/// type, and a type name is exactly the thing that differs between engines. What
/// this crate promises in exchange is that quoting depends on nothing else.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueKind {
    Text,
    Number,
    Boolean,
    Date,
    /// Anything the host did not classify — binary, JSON, an enum, a UDT.
    ///
    /// The default, and it quotes. An unclassified column that emitted its values
    /// bare would produce SQL that either fails or, far worse, silently compares
    /// against an identifier of the same name.
    #[default]
    Other,
}

impl ValueKind {
    /// Does a value of this kind normally need quotes?
    ///
    /// "Normally" because a value can override it in both directions — the user
    /// can quote a number, and a bare `NULL` is never quoted whatever the column
    /// is. See [`Value::needs_quotes`](crate::statement::Value::needs_quotes),
    /// which is the function to actually ask.
    pub fn quotes_by_default(self) -> bool {
        match self {
            ValueKind::Number | ValueKind::Boolean => false,
            ValueKind::Text | ValueKind::Date | ValueKind::Other => true,
        }
    }
}

/// One column, and the only two things the abbreviation language needs about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnMeta {
    /// Spelled exactly as the server reports it. This spelling is what comes back
    /// in a resolved statement — the user types `keycode` and gets whatever the
    /// schema calls it.
    pub name: String,
    #[serde(default)]
    pub kind: ValueKind,
}

impl ColumnMeta {
    pub fn new(name: impl Into<String>, kind: ValueKind) -> Self {
        Self { name: name.into(), kind }
    }
}

/// A referential constraint — the thing that makes `>` mean something.
///
/// No constraint *name*: nothing here identifies a foreign key by name, because
/// the user disambiguates by naming a **column** (`>clienti:id_cliente_fatturazione`)
/// and an error that named `FK_ORD_CLI_2` would tell them nothing they could act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyMeta {
    /// The referencing columns, on the table that owns this key.
    pub columns: Vec<String>,
    pub referenced_table: String,
    /// The referenced columns, positionally paired with [`columns`](Self::columns).
    pub referenced_columns: Vec<String>,
}

impl ForeignKeyMeta {
    /// The common single-column case.
    pub fn new(column: impl Into<String>, table: impl Into<String>, referenced: impl Into<String>) -> Self {
        Self {
            columns: vec![column.into()],
            referenced_table: table.into(),
            referenced_columns: vec![referenced.into()],
        }
    }

    /// A key is usable only if both sides pair up; a host that supplies a ragged
    /// one is ignored rather than trusted, because half a join condition is worse
    /// than none.
    pub fn is_well_formed(&self) -> bool {
        !self.columns.is_empty() && self.columns.len() == self.referenced_columns.len()
    }
}

/// One table or view. Views simply arrive with no foreign keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableMeta {
    pub name: String,
    #[serde(default)]
    pub columns: Vec<ColumnMeta>,
    #[serde(default)]
    pub foreign_keys: Vec<ForeignKeyMeta>,
}

impl TableMeta {
    pub fn new(name: impl Into<String>, columns: Vec<ColumnMeta>) -> Self {
        Self { name: name.into(), columns, foreign_keys: Vec::new() }
    }

    pub fn with_foreign_keys(mut self, keys: Vec<ForeignKeyMeta>) -> Self {
        self.foreign_keys = keys;
        self
    }

    /// Find a column, preferring an exact match over a case-insensitive one.
    ///
    /// Exact first because a schema *may* hold `Foo` beside `foo`; the user who
    /// typed the exact spelling meant it.
    pub fn column(&self, name: &str) -> Option<&ColumnMeta> {
        self.columns
            .iter()
            .find(|c| c.name == name)
            .or_else(|| self.columns.iter().find(|c| c.name.eq_ignore_ascii_case(name)))
    }

    pub fn column_names(&self) -> Vec<&str> {
        self.columns.iter().map(|c| c.name.as_str()).collect()
    }
}

/// Everything this crate is allowed to know about the database.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaView {
    /// Tables and views together, in the order the host wants them offered. A
    /// view is joinable and selectable and differs only in having no foreign
    /// keys, so it is not a separate list.
    pub tables: Vec<TableMeta>,
}

impl SchemaView {
    pub fn new(tables: Vec<TableMeta>) -> Self {
        Self { tables }
    }

    /// Find a table, preferring an exact match over a case-insensitive one.
    pub fn table(&self, name: &str) -> Option<&TableMeta> {
        self.tables
            .iter()
            .find(|t| t.name == name)
            .or_else(|| self.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name)))
    }

    pub fn table_names(&self) -> Vec<&str> {
        self.tables.iter().map(|t| t.name.as_str()).collect()
    }
}
