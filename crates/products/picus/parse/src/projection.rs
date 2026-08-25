//! [`Projection`] — what a `SELECT` produces, and out of what.
//!
//! ## Why this is separate from [`SelectShape`](crate::select::SelectShape)
//!
//! `SelectShape` answers one question — *may a column be spliced into this
//! projection?* — and answers it for **every statement, on every parse**, because
//! the editor asks constantly. It therefore records the least it can: output names,
//! a star flag, and a refusal.
//!
//! This answers a different question — *where does each output column come from?* —
//! and it is asked rarely and deliberately, by someone tracing a value back through
//! a stack of views. So it records the whole shape: the expression behind every
//! item, the `FROM` with its aliases, derived tables, CTEs and the arms of a set
//! operation. It is built only through [`project`](crate::parser::project), never as
//! part of an ordinary parse, so nothing on the typing path pays for it.
//!
//! ## It is syntax, and only syntax
//!
//! Nothing here consults a catalogue, so nothing here can tell a table from a view,
//! expand a `*`, or follow `V_ELENCHI` to what it selects. It reports what is
//! written and stops. Resolving that into "this value comes from `TIPI.CENINT`" is
//! `picus-lineage`'s job, which needs a database and can therefore be wrong in ways
//! this cannot.
//!
//! ## Everything unmodelled says so
//!
//! A construct the walk does not understand sets [`Projection::opaque`] or becomes
//! [`FromSource::Opaque`] rather than being dropped. A resolver that meets one stops
//! and reports where it stopped, which is the difference between "the trail ends
//! here" and a confident wrong answer — and on a lineage the second is far worse
//! than the first, because the whole point is deciding which table to write to.

use serde::Serialize;

/// A column reference as one statement writes it. Both halves are **folded**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSource {
    /// The alias or relation it is qualified with — `None` for a bare name, which a
    /// resolver has to attribute by searching the `FROM` items.
    pub qualifier: Option<String>,
    pub column: String,
}

impl ColumnSource {
    pub fn new(qualifier: Option<String>, column: String) -> Self {
        Self { qualifier, column }
    }
}

/// One item of a projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Projected {
    /// A plain reference: `cenint`, `t.cenint`, `t.cenint AS codsa`. The only kind
    /// that can be followed to exactly one place.
    Column {
        /// The name the result carries — the alias, or the column's own name.
        output: String,
        source: ColumnSource,
    },
    /// `*` or `t.*`. Expands to whatever the source has, which needs a catalogue —
    /// so it is carried unexpanded and the resolver does the widening.
    Star { qualifier: Option<String> },
    /// Computed: an expression, a function call, a `CASE`, a cast.
    ///
    /// `reads` is every column reference inside it, in order of appearance. The
    /// value is **not** any one of them, and a resolver must present it that way:
    /// `data_invio` derived from `a.inizio` and `b.fine` is not a column of either.
    Computed {
        output: String,
        reads: Vec<ColumnSource>,
    },
}

impl Projected {
    /// The output name, or `""` for a star (which has as many as its source has).
    pub fn output(&self) -> &str {
        match self {
            Self::Column { output, .. } | Self::Computed { output, .. } => output,
            Self::Star { .. } => "",
        }
    }
}

/// What one `FROM` item reads from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum FromSource {
    /// A named relation: a table, a view, or a CTE declared by the same statement.
    /// Which of the three it is takes a catalogue to say.
    Relation {
        /// Folded, schema-qualified when the statement qualified it.
        name: String,
    },
    /// A derived table — `(SELECT …) x` — carrying its own projection.
    Derived { projection: Box<Projection> },
    /// A set-returning function, `dual`, or anything else not modelled. A trail
    /// that reaches one ends here, and says so.
    Opaque,
}

/// One `FROM` item and the name the statement calls it by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FromItem {
    /// What a qualifier has to match: the alias when there is one, otherwise the
    /// relation's own (unqualified) name. Folded.
    pub name: String,
    pub source: FromSource,
    /// `AS x(a, b, c)` — output names imposed on the item, positionally. Empty when
    /// the item declares none.
    pub column_aliases: Vec<String>,
}

/// A `WITH` name and what it stands for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cte {
    /// Folded.
    pub name: String,
    /// `None` when the CTE is a `WITH … AS (INSERT …)` or otherwise not a query
    /// this models — a reference to it then resolves to nothing rather than wrongly.
    pub projection: Option<Projection>,
    pub column_aliases: Vec<String>,
}

/// What a `SELECT` projects and where it reads from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    pub items: Vec<Projected>,
    pub from: Vec<FromItem>,
    /// Names declared by this statement's `WITH`, which shadow the catalogue.
    pub ctes: Vec<Cte>,
    /// The arms of a set operation.
    ///
    /// When this is non-empty, `items` and `from` are **empty**: a `UNION` has no
    /// single projection, and an output column's sources are the union of the arms'
    /// columns at that position. Modelled rather than refused because a union of two
    /// tables is a perfectly ordinary view and a reader tracing one wants both
    /// answers, not a shrug.
    pub arms: Vec<Projection>,
    /// The walk met something it does not model — a `VALUES` list where a query was
    /// expected, a parse error. Whatever else is filled in may be incomplete.
    pub opaque: bool,
}

impl Projection {
    /// The item producing `output`, by folded name.
    ///
    /// `None` for a name this projection does not produce **or** produces through a
    /// star, which cannot be answered without a catalogue.
    pub fn item_named(&self, output: &str) -> Option<&Projected> {
        self.items.iter().find(|item| item.output() == output)
    }

    /// The `FROM` item a qualifier names, or the only one when unqualified.
    ///
    /// An unqualified column in a statement reading from **several** sources cannot
    /// be attributed without knowing which of them has that column, so this answers
    /// `None` and leaves the decision to a caller that has a catalogue.
    pub fn source_named(&self, qualifier: Option<&str>) -> Option<&FromItem> {
        match qualifier {
            Some(name) => self.from.iter().find(|item| item.name == name),
            None if self.from.len() == 1 => self.from.first(),
            None => None,
        }
    }
}
