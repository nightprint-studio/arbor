//! The **resolved** statement — what an abbreviation turns out to mean.
//!
//! This is the crate's real output, and it is deliberately not text. Everything
//! that needed the schema has already happened here: every table and column
//! carries the spelling the schema gave it, every join carries the columns the
//! foreign key named, and every value carries the [`ValueKind`] of the column it
//! is being compared against — which is the one fact that decides quoting.
//!
//! What is left is rendering, and rendering is the host's. A host with its own
//! deterministic emitter (Picus routes `INSERT`/`UPDATE` through
//! `DmlModel` → `picus-emit` so that quoting, identifier casing and the
//! Oracle/PostgreSQL split stay in one place) must be able to take the intent and
//! ignore our text entirely. A host without one takes
//! [`render`](crate::render::render) and is done. If this returned a `String`, the
//! first kind of host could not use the crate at all.

use serde::{Deserialize, Serialize};

use crate::schema::ValueKind;
use crate::syntax::Verb;

/// One table in a `FROM`/`JOIN` list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRef {
    /// Canonical spelling, from the schema.
    pub name: String,
    /// `None` when the statement has a single table and needs no alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

/// One column, carrying everything needed to write it and to write a value
/// against it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnRef {
    /// Canonical spelling of the column.
    pub name: String,
    /// Canonical spelling of the table it belongs to.
    pub table: String,
    /// What to qualify it with — `None` when the statement has one table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub kind: ValueKind,
}

/// One equality of a join's `ON`, `left` on the table already in the statement
/// and `right` on the one being joined in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinCondition {
    pub left: ColumnRef,
    pub right: ColumnRef,
}

/// How one table attaches to another. Always an inner join, always read from a
/// foreign key — this crate never invents a join condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Join {
    /// Index into `Statement::Select::tables` of the table being joined in.
    pub table: usize,
    /// Index into `Statement::Select::tables` of the table it attaches to.
    pub to: usize,
    pub on: Vec<JoinCondition>,
}

/// A comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Operator {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Like,
}

impl Operator {
    /// The SQL spelling. `<>` rather than `!=` because it is the standard one and
    /// every engine takes it; `!=` is accepted on input and normalised here.
    pub fn sql(self) -> &'static str {
        match self {
            Operator::Eq => "=",
            Operator::NotEq => "<>",
            Operator::Lt => "<",
            Operator::LtEq => "<=",
            Operator::Gt => ">",
            Operator::GtEq => ">=",
            Operator::Like => "LIKE",
        }
    }

    /// How it is written **in an abbreviation**. `~` is the only one that is not
    /// its own SQL spelling: `LIKE` needs a symbol short enough to be worth
    /// abbreviating, and `~` is not otherwise in the grammar.
    pub fn from_symbol(symbol: &str) -> Option<Operator> {
        Some(match symbol {
            "=" => Operator::Eq,
            "!=" | "<>" => Operator::NotEq,
            "<" => Operator::Lt,
            "<=" => Operator::LtEq,
            ">" => Operator::Gt,
            ">=" => Operator::GtEq,
            "~" => Operator::Like,
            _ => return None,
        })
    }

    /// Every operator, with the symbol to offer for it.
    pub const SYMBOLS: &'static [(&'static str, Operator)] = &[
        ("=", Operator::Eq),
        ("<>", Operator::NotEq),
        ("<", Operator::Lt),
        ("<=", Operator::LtEq),
        (">", Operator::Gt),
        (">=", Operator::GtEq),
        ("~", Operator::Like),
    ];
}

/// The values a user means as SQL **keywords**, not as strings.
///
/// A closed list on purpose. The alternative — deciding whether something "looks
/// like" an expression — eventually passes a user's literal text through
/// unquoted, and in a tool that writes SQL against someone's database that is not
/// a bug anybody finds in review.
const KEYWORDS: [&str; 8] = [
    "NULL",
    "DEFAULT",
    "SYSDATE",
    "NOW()",
    "CURRENT_DATE",
    "CURRENT_TIME",
    "CURRENT_TIMESTAMP",
    "LOCALTIMESTAMP",
];

/// A value, and whether the user forced it to be a literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "form", content = "text", rename_all = "camelCase")]
pub enum Value {
    /// The user wrote it between quotes. Quotes stripped and `''` folded back to
    /// `'` — it is **always** a string literal, whatever the column's kind says,
    /// because quoting it was an explicit statement of intent.
    Quoted(String),
    /// The user wrote it bare. Whether it ends up quoted is the column's decision.
    Bare(String),
}

impl Value {
    /// The payload, without quotes either way.
    pub fn text(&self) -> &str {
        match self {
            Value::Quoted(t) | Value::Bare(t) => t,
        }
    }

    /// **The question this crate exists to answer.** Does this value have to be
    /// written between quotes, against a column of `kind`?
    ///
    /// - quoted by the user → yes, always;
    /// - a SQL keyword (`NULL`, `SYSDATE`, …) → no, whatever the column is;
    /// - a plain number in a numeric column, `true`/`false` in a boolean one → no;
    /// - anything else → yes, which is the safe answer and the one an
    ///   unclassified column always gets.
    ///
    /// The case it is really there for: `007` in a `varchar` account-code column
    /// keeps its leading zeros, and `15` in a `numeric` one does not gain quotes.
    pub fn needs_quotes(&self, kind: ValueKind) -> bool {
        let text = match self {
            Value::Quoted(_) => return true,
            Value::Bare(t) => t.trim(),
        };
        if KEYWORDS.contains(&text.to_ascii_uppercase().as_str()) {
            return false;
        }
        match kind {
            ValueKind::Number => !is_plain_number(text),
            ValueKind::Boolean => !matches!(text.to_ascii_lowercase().as_str(), "true" | "false"),
            _ => true,
        }
    }
}

/// A decimal number with no exponent, no sign games and no spaces — conservative
/// on purpose, because anything it declines simply gets quoted, which is safe.
fn is_plain_number(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    if body.is_empty() {
        return false;
    }
    let mut seen_dot = false;
    for (i, c) in body.chars().enumerate() {
        match c {
            '0'..='9' => {}
            '.' if !seen_dot && i > 0 && i < body.chars().count() - 1 => seen_dot = true,
            _ => return false,
        }
    }
    true
}

/// `column op value`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Predicate {
    pub column: ColumnRef,
    pub op: Operator,
    pub value: Value,
}

/// `column = value`, in a `SET` list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub column: ColumnRef,
    pub value: Value,
}

/// One column of an `INSERT`, and the value for it if the user gave one.
///
/// A struct rather than two parallel `Vec`s, so "these two lists are the same
/// length and line up" is not an invariant anybody can break.
///
/// `None` is a real answer and is **not** the same as `Some(Value::Quoted(""))`:
/// the first means the host should write its placeholder there, the second means
/// the user asked for an empty string. `i#t(a='x',b)` produces one of each, in
/// that order, and mixing them is allowed on purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InsertColumn {
    pub column: ColumnRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// What the abbreviation meant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "camelCase")]
pub enum Statement {
    /// `tables[0]` is the root; `joins[i]` says how a later one attaches. An
    /// empty `columns` means `*`.
    Select {
        tables: Vec<TableRef>,
        joins: Vec<Join>,
        columns: Vec<ColumnRef>,
        predicates: Vec<Predicate>,
    },
    /// Columns, the values given for them, and how many rows of them.
    ///
    /// A column with no value is the host's placeholder. `rows` is a count and
    /// **every row is a copy of the same values** — which sounds like a bug and is
    /// not: `i#t(tipo='X')*3` is what a seed-data user types before editing the
    /// three rows apart, and the alternative (three rows of placeholders with one
    /// column pre-filled) is the same thing with more typing.
    Insert {
        table: String,
        columns: Vec<InsertColumn>,
        rows: usize,
    },
    /// `predicates` may use **any** operator this crate supports, not only
    /// equality.
    ///
    /// A host whose own model keys an update by equality — Picus's `DmlModel`
    /// does — refuses the rest at the point it maps this, and that is the right
    /// place for it: "the WHERE must be a key equality" is a fact about that
    /// model, not about SQL, and a general language that bakes in one consumer's
    /// limitation stops being general.
    Update {
        table: String,
        assignments: Vec<Assignment>,
        predicates: Vec<Predicate>,
    },
    Delete {
        table: String,
        predicates: Vec<Predicate>,
    },
}

impl Statement {
    pub fn verb(&self) -> Verb {
        match self {
            Statement::Select { .. } => Verb::Select,
            Statement::Insert { .. } => Verb::Insert,
            Statement::Update { .. } => Verb::Update,
            Statement::Delete { .. } => Verb::Delete,
        }
    }

    /// Every table the statement touches, canonically spelled, root first.
    pub fn tables(&self) -> Vec<&str> {
        match self {
            Statement::Select { tables, .. } => {
                tables.iter().map(|t| t.name.as_str()).collect()
            }
            Statement::Insert { table, .. }
            | Statement::Update { table, .. }
            | Statement::Delete { table, .. } => vec![table.as_str()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_stays_bare_only_in_a_numeric_column() {
        let bare = Value::Bare("15".to_string());
        assert!(!bare.needs_quotes(ValueKind::Number));
        // The one that matters: an account code keeps its leading zeros.
        assert!(Value::Bare("007".to_string()).needs_quotes(ValueKind::Text));
        assert!(bare.needs_quotes(ValueKind::Text));
        assert!(bare.needs_quotes(ValueKind::Other), "an unclassified column quotes");
    }

    #[test]
    fn what_the_user_quoted_stays_quoted() {
        // Explicit intent beats the column's type in both directions.
        assert!(Value::Quoted("007".to_string()).needs_quotes(ValueKind::Number));
        assert!(Value::Quoted("15".to_string()).needs_quotes(ValueKind::Number));
    }

    #[test]
    fn a_keyword_is_never_quoted_and_a_value_that_only_looks_numeric_always_is() {
        for keyword in ["null", "NULL", "sysdate", "current_timestamp", "now()"] {
            assert!(!Value::Bare(keyword.into()).needs_quotes(ValueKind::Text), "{keyword}");
        }
        for odd in ["1e5", "1.2.3", "+3", "0x1F", ".5", "5.", "1 2", ""] {
            assert!(Value::Bare(odd.into()).needs_quotes(ValueKind::Number), "{odd}");
        }
    }

    #[test]
    fn a_boolean_column_takes_true_and_false_bare() {
        assert!(!Value::Bare("true".into()).needs_quotes(ValueKind::Boolean));
        assert!(!Value::Bare("FALSE".into()).needs_quotes(ValueKind::Boolean));
        assert!(Value::Bare("maybe".into()).needs_quotes(ValueKind::Boolean));
        // …and a text column does not: there `true` is five characters.
        assert!(Value::Bare("true".into()).needs_quotes(ValueKind::Text));
    }

    #[test]
    fn not_equal_normalises_to_the_standard_spelling() {
        assert_eq!(Operator::from_symbol("!="), Some(Operator::NotEq));
        assert_eq!(Operator::from_symbol("<>"), Some(Operator::NotEq));
        assert_eq!(Operator::NotEq.sql(), "<>");
        assert_eq!(Operator::from_symbol("=="), None);
    }
}
