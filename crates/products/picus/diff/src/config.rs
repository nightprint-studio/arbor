//! What to compare, and how hard to look.
//!
//! ## One filter, not one per check
//!
//! The relation filters ([`DiffConfig::tables`], [`DiffConfig::views`]) sit at the
//! top level and every check obeys them — schema, counts, contents, indexes,
//! constraints, triggers. A per-check copy would let a run compare the columns of
//! a table whose rows it excluded, and then say "identical" about an object it
//! looked at through two different windows. The object filters that *are*
//! per-check ([`IndexCheck::filter`] and friends) select among that check's own
//! objects, which is a different question: `*_pkey` is an index name, not a table.
//!
//! ## Every check is separately switchable, and every check that is off says so
//!
//! `enabled: false` is not "skip silently". The engine records it in
//! [`crate::report::DiffReport::skipped`] and the verdict downgrades from
//! "identical" to "identical where checked" — see [`crate::report`] for why that
//! distinction is the whole point of the report type.

use serde::{Deserialize, Serialize};

use picus_types::prelude::RelationKind;

use crate::names::matches_any;

/// A whole comparison run, serialisable so it can be saved as a
/// [`crate::template::DiffTemplate`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DiffConfig {
    /// Fold identifiers before matching and before deciding that two objects are
    /// one object. On by default: the case that costs a user an afternoon is an
    /// Oracle `PARAMETRI` compared against a PostgreSQL `parametri` reported as
    /// two tables, one missing on each side. Turn it off to compare two databases
    /// of one engine where `"Name"` and `"name"` really are two columns.
    pub case_insensitive: bool,
    pub tables: NameFilter,
    pub views: NameFilter,
    pub columns: ColumnFilter,
    pub schema: SchemaCheck,
    pub counts: CountCheck,
    pub contents: ContentCheck,
    pub indexes: IndexCheck,
    pub sequences: SequenceCheck,
    pub constraints: ConstraintCheck,
    pub triggers: TriggerCheck,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            case_insensitive: true,
            tables: NameFilter::default(),
            views: NameFilter::default(),
            columns: ColumnFilter::default(),
            schema: SchemaCheck::default(),
            counts: CountCheck::default(),
            contents: ContentCheck::default(),
            indexes: IndexCheck::default(),
            sequences: SequenceCheck::default(),
            constraints: ConstraintCheck::default(),
            triggers: TriggerCheck::default(),
        }
    }
}

impl DiffConfig {
    /// Is this relation in scope, given what kind it is?
    pub fn accepts(&self, kind: RelationKind, name: &str) -> bool {
        match kind {
            RelationKind::Table => self.tables.accepts(name, self.case_insensitive),
            RelationKind::View => self.views.accepts(name, self.case_insensitive),
        }
    }

    /// Is this name in scope as *either* a table or a view?
    ///
    /// For the objects that name their relation without saying what it is — a
    /// trigger carries a table name and nothing else — and for the relation that
    /// is a table on one side and a view on the other, which must not fall
    /// through the gap between the two filters.
    pub fn accepts_any_kind(&self, name: &str) -> bool {
        self.tables.accepts(name, self.case_insensitive)
            || self.views.accepts(name, self.case_insensitive)
    }
}

/// How a pattern list is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterMode {
    /// Every object, patterns ignored.
    #[default]
    All,
    /// Only the objects that match.
    Include,
    /// Every object except the ones that match.
    Exclude,
}

/// A glob filter over object names. `*` and `?` only — see [`crate::names`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NameFilter {
    pub mode: FilterMode,
    pub patterns: Vec<String>,
}

impl NameFilter {
    pub fn all() -> Self {
        Self { mode: FilterMode::All, patterns: Vec::new() }
    }

    pub fn include<I, S>(patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            mode: FilterMode::Include,
            patterns: patterns.into_iter().map(Into::into).collect(),
        }
    }

    pub fn exclude<I, S>(patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            mode: FilterMode::Exclude,
            patterns: patterns.into_iter().map(Into::into).collect(),
        }
    }

    /// An `Include` with no patterns accepts nothing, and that is on purpose: it
    /// is a template somebody half-filled, and comparing everything because the
    /// list came out empty is exactly the surprise this crate must not spring.
    pub fn accepts(&self, name: &str, case_insensitive: bool) -> bool {
        match self.mode {
            FilterMode::All => true,
            FilterMode::Include => matches_any(&self.patterns, name, case_insensitive),
            FilterMode::Exclude => !matches_any(&self.patterns, name, case_insensitive),
        }
    }
}

/// Columns to leave out of every comparison, and properties not worth reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColumnFilter {
    /// Globs. The audit columns (`created_at`, `*_by`) that differ between two
    /// environments by construction live here.
    pub ignore_patterns: Vec<String>,
    /// Do not report a column whose only difference is its `DEFAULT`. Two servers
    /// spelling the same intent (`uuid_generate_v4()` vs `gen_random_uuid()`) is
    /// the case this exists for.
    pub ignore_defaults: bool,
    /// Do not report a column that moved. Default **on**: inserting one column in
    /// the middle shifts every column after it, and a report with forty position
    /// changes hiding one real type change is a report nobody reads. Turn it off
    /// when the physical order matters — a script generator that emits
    /// `INSERT` without a column list, for instance.
    pub ignore_position: bool,
}

impl ColumnFilter {
    pub fn ignores(&self, column: &str, case_insensitive: bool) -> bool {
        matches_any(&self.ignore_patterns, column, case_insensitive)
    }
}

/// `ignore_position` defaults to `true`, so this cannot be `#[derive]`d.
impl Default for ColumnFilter {
    fn default() -> Self {
        Self { ignore_patterns: Vec::new(), ignore_defaults: false, ignore_position: true }
    }
}

/// Relations and their columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SchemaCheck {
    pub enabled: bool,
    /// Compare the `SELECT` a view is defined as. Off by default: servers hand
    /// back their own reprint of the definition, so two views that are the same
    /// view differ in whitespace, casing and inserted casts, and the check is
    /// noise everywhere except between two instances of the same engine version.
    pub compare_view_definitions: bool,
}

/// Row counts, with tolerances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CountCheck {
    /// Off by default. Every structural check reads the catalogue; an exact count
    /// is a scan on most engines, and a "quick diff" that quietly scans two
    /// hundred tables of somebody's production database is not quick and was not
    /// asked for.
    pub enabled: bool,
    /// Which tables to count, on top of the top-level relation filter.
    pub filter: NameFilter,
    /// Percentage difference (relative to A) at which a count becomes a warning.
    pub warning_threshold_percent: f64,
    /// …and at which it becomes an error.
    pub error_threshold_percent: f64,
}

/// Actual rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ContentCheck {
    /// Off by default. Every other check reads catalogue metadata; this one reads
    /// data, and turning it on unaware is how a "quick diff" becomes a full scan
    /// of somebody's production database.
    pub enabled: bool,
    /// Rows to read per relation when its own rule does not say. Advisory: this
    /// crate never reads anything, the caller applies it to its query.
    pub default_limit: u32,
    /// Cap on the differences listed per relation. `0` means no cap. Whatever is
    /// left out is still **counted** — see [`crate::rows::RowsComparison`].
    pub max_differences_shown: usize,
    /// Per-relation rules. A relation with no entry is compared with the defaults
    /// and the key the caller took from the catalogue.
    pub tables: Vec<TableRules>,
}

/// What to compare, and how to match rows, for one relation.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TableRules {
    pub name: String,
    /// Columns to compare. Empty means every column both sides have.
    pub columns: Vec<String>,
    /// Advisory `ORDER BY` for the caller's query. It is not this crate's job to
    /// sort, but a positional comparison over two unordered reads is meaningless,
    /// so the ordering the caller must apply is recorded with the rule that needs
    /// it rather than somewhere else.
    pub order_by: Vec<String>,
    /// Columns that identify a row. Empty means "use the key the caller found in
    /// the catalogue", and if there is none either, the comparison falls back to
    /// position — see [`crate::rows::compare_rows`].
    pub primary_key: Vec<String>,
    /// Rows to read, overriding [`ContentCheck::default_limit`]. Advisory, as
    /// above.
    pub limit: Option<u32>,
    /// Globs, applied on top of [`ColumnFilter::ignore_patterns`].
    pub ignore_columns: Vec<String>,
}

impl ContentCheck {
    /// The rule for a relation, if one was written.
    pub fn rules_for(&self, table: &str, case_insensitive: bool) -> Option<&TableRules> {
        let wanted = crate::names::fold_name(table, case_insensitive);
        self.tables
            .iter()
            .find(|t| crate::names::fold_name(&t.name, case_insensitive) == wanted)
    }

    /// How many rows the caller should read for this relation.
    pub fn limit_for(&self, table: &str, case_insensitive: bool) -> u32 {
        self.rules_for(table, case_insensitive)
            .and_then(|r| r.limit)
            .unwrap_or(self.default_limit)
    }
}

/// Indexes, wherever they hang.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IndexCheck {
    pub enabled: bool,
    /// Filter over index **names**.
    pub filter: NameFilter,
    /// Skip the index that backs a primary key. On by default: nobody created it
    /// and nobody can drop it, so a difference in it is a difference in the
    /// primary key, which the constraint check already reports — in the words the
    /// user would act on.
    pub ignore_primary_key_indexes: bool,
    /// Compare the access method (`btree`, `gin`, …). Off by default, because a
    /// server that does not report one leaves it unset and every index would then
    /// differ from its twin on a server that does.
    pub compare_kind: bool,
}

/// Sequences and how far apart they have drifted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SequenceCheck {
    pub enabled: bool,
    pub filter: NameFilter,
    /// Absolute difference in `last_value` below which the drift is not worth
    /// mentioning. Two live databases are never at the same number and reporting
    /// that would drown the case that matters: a sequence sitting *behind* the
    /// data, which the next insert will collide with.
    pub warning_threshold: i64,
    /// Also compare increment, bounds, cycle and cache — the parts of a sequence
    /// that are definition rather than state.
    pub compare_attributes: bool,
}

/// Primary and foreign keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConstraintCheck {
    pub enabled: bool,
    /// Filter over constraint **names**.
    pub filter: NameFilter,
    /// Match constraints by what they do rather than by what they are called, and
    /// do not report a name difference. On by default, because a constraint
    /// created without a name gets a generated one (`SYS_C0011423`) that is
    /// different in every database it was ever installed into — and a report
    /// claiming every foreign key is missing on both sides is worse than useless.
    pub ignore_names: bool,
}

/// Triggers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TriggerCheck {
    pub enabled: bool,
    /// Filter over trigger **names**.
    pub filter: NameFilter,
    /// Report a trigger that exists on both sides but is disabled on one. On by
    /// default: it is the difference least visible in a schema dump and the one
    /// most likely to explain why the data diverged.
    pub compare_enabled_state: bool,
}

impl Default for SchemaCheck {
    fn default() -> Self {
        Self { enabled: true, compare_view_definitions: false }
    }
}

impl Default for CountCheck {
    fn default() -> Self {
        Self {
            enabled: false,
            filter: NameFilter::all(),
            warning_threshold_percent: 10.0,
            error_threshold_percent: 50.0,
        }
    }
}

impl Default for ContentCheck {
    fn default() -> Self {
        Self {
            enabled: false,
            default_limit: 1_000,
            max_differences_shown: 50,
            tables: Vec::new(),
        }
    }
}

impl Default for IndexCheck {
    fn default() -> Self {
        Self {
            enabled: true,
            filter: NameFilter::all(),
            ignore_primary_key_indexes: true,
            compare_kind: false,
        }
    }
}

impl Default for SequenceCheck {
    fn default() -> Self {
        Self {
            enabled: true,
            filter: NameFilter::all(),
            warning_threshold: 100,
            compare_attributes: true,
        }
    }
}

impl Default for ConstraintCheck {
    fn default() -> Self {
        Self { enabled: true, filter: NameFilter::all(), ignore_names: true }
    }
}

impl Default for TriggerCheck {
    fn default() -> Self {
        Self { enabled: true, filter: NameFilter::all(), compare_enabled_state: true }
    }
}
