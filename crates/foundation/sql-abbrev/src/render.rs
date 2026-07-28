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
//! `SYSDATE`-versus-`now()` translation here and no upsert. A host that needs
//! those has an emitter and is not reading this.

use serde::{Deserialize, Serialize};

use crate::statement::{ColumnRef, InsertColumn, Join, Predicate, Statement, TableRef, Value};

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
        Statement::Insert { table, columns, rows } => insert(table, columns, *rows, style),
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
    };
    if let Some(terminator) = style.terminator {
        sql.push(terminator);
    }
    sql
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

/// One statement with `rows` value tuples.
///
/// Standard SQL, and not Oracle's: Oracle has no multi-row `VALUES`. That is
/// exactly why `Statement::Insert` carries a row **count** rather than rendered
/// text — a host with an Oracle destination reads the count and emits one
/// statement per row, which is what Picus does.
///
/// Every tuple is identical, because every row of an `i#…*n` is. A column with
/// no value gets the style's placeholder.
fn insert(table: &str, columns: &[InsertColumn], rows: usize, style: &RenderStyle) -> String {
    let names = columns.iter().map(|c| style.id(&c.column.name)).collect::<Vec<_>>().join(", ");
    let values = columns
        .iter()
        .map(|c| match &c.value {
            Some(value) => style.value(value, &c.column),
            None => style.placeholder.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let tuples = vec![format!("({values})"); rows.max(1)].join(", ");
    format!(
        "{} {} ({names}) {} {tuples}",
        style.kw("INSERT INTO"),
        style.id(table),
        style.kw("VALUES")
    )
}
