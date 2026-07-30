//! A default renderer — **a convenience, not the output**.
//!
//! [`expand`](crate::expand::expand) returns a [`Statement`], and that is the
//! crate's real answer. This module exists for hosts that have no emitter of
//! their own and just want a line of SQL back.
//!
//! Picus, which does have one, uses it for `s#` and `d#` and **ignores it for
//! `i#` and `u#`**: those go through `DmlModel` → `picus-emit`, so that quoting,
//! identifier casing and the Oracle/PostgreSQL differences stay in the one place
//! that already owns them and one abbreviation can produce both dialects. That
//! split is the reason [`Statement`] is a structure rather than a string, and if
//! you are adding a host it is the decision to make first: do you have an
//! emitter, or do you want this one?
//!
//! What this renderer deliberately does **not** do: dialect. There is no
//! `SYSDATE`-versus-`now()` translation here.
//!
//! The three verbs that have no single spelling — `m#` (merge), `a#` (alter) and
//! `fc#` (cursor loop) — are written here in their **standard** form, and a host
//! with an engine overrides all three. That is not a fudge, it is the same split
//! as `i#`/`u#`: this file answers "what does the standard say", and a host that
//! knows it is talking to Oracle answers "and what does *this* engine say".
//! Picus overrides them; a host with no engine gets SQL that is at least
//! defensible.

use serde::{Deserialize, Serialize};

use crate::statement::{
    ColumnChange, ColumnRef, InsertRow, Join, Predicate, Statement, TableRef, Value,
};
use crate::syntax::ChangeKind;

/// How a word comes out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Case {
    #[default]
    Upper,
    Lower,
    /// Exactly as the schema spells it. Only sensible for identifiers.
    AsIs,
}

impl Case {
    fn apply(self, word: &str) -> String {
        match self {
            Case::Upper => word.to_uppercase(),
            Case::Lower => word.to_lowercase(),
            Case::AsIs => word.to_string(),
        }
    }
}

/// The few things hosts genuinely disagree about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderStyle {
    pub keywords: Case,
    /// Defaults to [`Case::AsIs`]: the schema's spelling is the schema's answer,
    /// and re-casing it is how a statement stops matching the code around it.
    pub identifiers: Case,
    /// The string-literal quote. Doubled to escape itself, which is what every
    /// engine Arbor deals with does.
    pub quote: char,
    /// What an `INSERT` writes where a value has not been supplied.
    pub placeholder: String,
    /// Appended to the statement. `None` by default — an abbreviation expands to
    /// *a statement*, and whether it needs a terminator is a fact about the
    /// document it is being pasted into.
    pub terminator: Option<char>,
}

impl Default for RenderStyle {
    fn default() -> Self {
        Self {
            keywords: Case::Upper,
            identifiers: Case::AsIs,
            quote: '\'',
            placeholder: "?".to_string(),
            terminator: None,
        }
    }
}

impl RenderStyle {
    /// Lower-case keywords, for a schema and a codebase that are written that way.
    pub fn lowercase_keywords() -> Self {
        Self { keywords: Case::Lower, ..Self::default() }
    }

    fn kw(&self, word: &str) -> String {
        self.keywords.apply(word)
    }

    fn id(&self, name: &str) -> String {
        self.identifiers.apply(name)
    }

    fn column(&self, column: &ColumnRef) -> String {
        match &column.alias {
            Some(alias) => format!("{}.{}", self.id(alias), self.id(&column.name)),
            None => self.id(&column.name),
        }
    }

    fn table(&self, table: &TableRef) -> String {
        match &table.alias {
            Some(alias) => format!("{} {}", self.id(&table.name), self.id(alias)),
            None => self.id(&table.name),
        }
    }

    /// A value, quoted or not according to the column it belongs to.
    fn value(&self, value: &Value, column: &ColumnRef) -> String {
        let text = value.text();
        if !value.needs_quotes(column.kind) {
            return text.to_string();
        }
        let quote = self.quote;
        format!("{quote}{}{quote}", text.replace(quote, &format!("{quote}{quote}")))
    }

    /// One row's values, aligned to the columns — a cell with none takes the
    /// placeholder, and a row shorter than the column list is padded with them
    /// rather than silently shifting the values that follow.
    fn tuple(&self, columns: &[ColumnRef], row: &InsertRow) -> String {
        columns
            .iter()
            .enumerate()
            .map(|(i, column)| match row.get(i).and_then(Option::as_ref) {
                Some(value) => self.value(value, column),
                None => self.placeholder.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn predicates(&self, predicates: &[Predicate]) -> String {
        predicates
            .iter()
            .map(|p| format!("{} {} {}", self.column(&p.column), self.kw(p.op.sql()), self.value(&p.value, &p.column)))
            .collect::<Vec<_>>()
            .join(&format!(" {} ", self.kw("AND")))
    }
}

/// Render a statement as one line of SQL.
pub fn render(statement: &Statement, style: &RenderStyle) -> String {
    let mut sql = match statement {
        Statement::Select { tables, joins, columns, predicates } => {
            select(tables, joins, columns, predicates, style)
        }
        Statement::Insert { table, columns, rows } => insert(table, columns, rows, style),
        Statement::Update { table, assignments, predicates } => {
            let sets = assignments
                .iter()
                .map(|a| format!("{} = {}", style.column(&a.column), style.value(&a.value, &a.column)))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{} {} {} {} {} {}",
                style.kw("UPDATE"),
                style.id(table),
                style.kw("SET"),
                sets,
                style.kw("WHERE"),
                style.predicates(predicates)
            )
        }
        Statement::Delete { table, predicates } => format!(
            "{} {} {} {}",
            style.kw("DELETE FROM"),
            style.id(table),
            style.kw("WHERE"),
            style.predicates(predicates)
        ),
        Statement::Merge { table, columns, keys } => merge(table, columns, keys, style),
        Statement::Alter { table, changes } => alter(table, changes, style),
        Statement::ForCursor { variable, query } => {
            // No parentheses: PL/pgSQL rejects them and PL/SQL merely prefers
            // them, so the form without is the one that parses in more places.
            format!(
                "{} {variable} {} {} {}\n  {};\n{};",
                style.kw("FOR"),
                style.kw("IN"),
                render(query, &RenderStyle { terminator: None, ..style.clone() }),
                style.kw("LOOP"),
                style.kw("NULL"),
                style.kw("END LOOP")
            )
        }
    };
    if let Some(terminator) = style.terminator {
        sql.push(terminator);
    }
    sql
}

/// The SQL:2003 `MERGE`, which is what both engines Arbor targets accept in some
/// form — and which a host with an engine will replace with the shorter one.
///
/// Values are named parameters rather than placeholders: a merge skeleton with
/// eight `?` in it is unreadable, and the column names are right there.
fn merge(table: &str, columns: &[ColumnRef], keys: &[ColumnRef], style: &RenderStyle) -> String {
    let is_key = |c: &ColumnRef| keys.iter().any(|k| k.name == c.name);
    let source = columns
        .iter()
        .map(|c| format!(":{} {} {}", c.name, style.kw("AS"), style.id(&c.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let on = keys
        .iter()
        .map(|c| format!("d.{} = s.{}", style.id(&c.name), style.id(&c.name)))
        .collect::<Vec<_>>()
        .join(&format!(" {} ", style.kw("AND")));
    let set = columns
        .iter()
        .filter(|c| !is_key(c))
        .map(|c| format!("      d.{} = s.{}", style.id(&c.name), style.id(&c.name)))
        .collect::<Vec<_>>()
        .join(",\n");
    let names = columns.iter().map(|c| style.id(&c.name)).collect::<Vec<_>>().join(", ");
    let values = columns.iter().map(|c| format!("s.{}", style.id(&c.name))).collect::<Vec<_>>().join(", ");

    // Assembled line by line rather than in one format string: the statement has
    // six clauses and a positional template of that size is a puzzle for whoever
    // next has to change one of them.
    [
        format!("{} {} d", style.kw("MERGE INTO"), style.id(table)),
        format!("{} ({} {source}) s", style.kw("USING"), style.kw("SELECT")),
        format!("   {} ({on})", style.kw("ON")),
        format!(" {}", style.kw("WHEN MATCHED THEN UPDATE SET")),
        set,
        format!(" {} ({names})", style.kw("WHEN NOT MATCHED THEN INSERT")),
        format!("      {} ({values})", style.kw("VALUES")),
    ]
    .join("\n")
}

/// `ALTER TABLE`, one clause per change, in the order they were written.
///
/// Separate statements rather than one with several clauses: the two engines
/// disagree about how to bundle them and agree about how to write them one at a
/// time, and a host with an engine bundles them itself.
fn alter(table: &str, changes: &[ColumnChange], style: &RenderStyle) -> String {
    changes
        .iter()
        .map(|change| {
            let verb = match change.kind {
                ChangeKind::Add => style.kw("ADD COLUMN"),
                ChangeKind::Modify => style.kw("ALTER COLUMN"),
            };
            let tail = match change.kind {
                ChangeKind::Add => String::new(),
                ChangeKind::Modify => format!("{} ", style.kw("TYPE")),
            };
            format!(
                "{} {} {verb} {} {tail}{};",
                style.kw("ALTER TABLE"),
                style.id(table),
                style.id(&change.column),
                change.data_type
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn select(
    tables: &[TableRef],
    joins: &[Join],
    columns: &[ColumnRef],
    predicates: &[Predicate],
    style: &RenderStyle,
) -> String {
    let list = if columns.is_empty() {
        "*".to_string()
    } else {
        columns.iter().map(|c| style.column(c)).collect::<Vec<_>>().join(", ")
    };
    let mut sql = format!("{} {} {} {}", style.kw("SELECT"), list, style.kw("FROM"), style.table(&tables[0]));
    for join in joins {
        let on = join
            .on
            .iter()
            .map(|c| format!("{} = {}", style.column(&c.left), style.column(&c.right)))
            .collect::<Vec<_>>()
            .join(&format!(" {} ", style.kw("AND")));
        sql.push_str(&format!(
            " {} {} {} {on}",
            style.kw("JOIN"),
            style.table(&tables[join.table]),
            style.kw("ON")
        ));
    }
    if !predicates.is_empty() {
        sql.push_str(&format!(" {} {}", style.kw("WHERE"), style.predicates(predicates)));
    }
    sql
}

/// One statement with a tuple per row.
///
/// Standard SQL, and not Oracle's: Oracle has no multi-row `VALUES`. That is
/// exactly why `Statement::Insert` carries the **rows** rather than rendered text
/// — a host with an Oracle destination emits one statement per row, which is what
/// Picus does.
///
/// A cell with no value gets the style's placeholder.
fn insert(table: &str, columns: &[ColumnRef], rows: &[InsertRow], style: &RenderStyle) -> String {
    let names = columns.iter().map(|c| style.id(&c.name)).collect::<Vec<_>>().join(", ");
    let tuples = rows
        .iter()
        .map(|row| format!("({})", style.tuple(columns, row)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} {} ({names}) {} {tuples}",
        style.kw("INSERT INTO"),
        style.id(table),
        style.kw("VALUES")
    )
}
