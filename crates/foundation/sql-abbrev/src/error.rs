//! Why an abbreviation was refused.
//!
//! **Refuse rather than guess** is the whole posture of this crate, and it only
//! pays off if every refusal is something a person can act on. So each variant
//! names the thing that went wrong, and — where it can do so cheaply and without
//! guessing — the way out: the nearest table name, the columns of the candidate
//! foreign keys, the qualified spelling that would resolve an ambiguity.
//!
//! These strings are the contract. They cross whatever seam the host has and land
//! in front of the person typing, so "invalid input" is not an acceptable thing
//! for any of them to say.

use std::fmt;

use crate::syntax::Verb;

/// A refusal. Never a partial expansion — an abbreviation either means something
/// exact or it means nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbbrevError {
    /// Nothing has been typed.
    Empty,
    /// A verb with no `#` after it.
    MissingSeparator { verb: String },
    UnknownVerb { verb: String },
    MissingTable { verb: Verb },
    UnknownTable { name: String, suggestion: Option<String> },
    /// A column that is in none of the abbreviation's tables.
    UnknownColumn { name: String, tables: Vec<String>, suggestion: Option<String> },
    /// A column that is in more than one of them.
    AmbiguousColumn { name: String, tables: Vec<String> },
    /// `x.col` where `x` is not one of the abbreviation's tables.
    UnknownQualifier { qualifier: String, tables: Vec<String> },
    /// The two tables are not related, so there is no join condition to read.
    NoForeignKey { from: String, to: String },
    /// They are related more than once. The candidates are named so the user can
    /// pick with `>table:column`.
    AmbiguousJoin { from: String, to: String, candidates: Vec<String>, hint: String },
    /// `>table:column` named a column no foreign key between them uses.
    UnknownJoinColumn { from: String, to: String, column: String, candidates: Vec<String> },
    ChainNotAllowed { verb: Verb },
    ColumnsNotAllowed { verb: Verb },
    PredicatesNotAllowed { verb: Verb },
    MultiplierNotAllowed { verb: Verb },
    BadMultiplier { text: String },
    /// A `{…}` row template on a verb that has no rows.
    TemplateNotAllowed { verb: Verb },
    /// A `{…}` template and a `column=value` in the same abbreviation.
    TemplateAndAssignment { column: String },
    /// A `{…}` with a different number of values than the statement has columns.
    TemplateArity { values: usize, columns: Vec<String> },
    /// `+col:type` / `~col:type` on a verb that does not shape tables.
    ChangesNotAllowed { verb: Verb },
    /// `a#table` with nothing to change.
    ChangesRequired { table: String },
    /// A change with no `:type` after it.
    MissingType { column: String },
    /// `a#table+col:…` where `col` is already there.
    ColumnAlreadyExists { table: String, column: String },
    /// `m#table` with no `[...]`.
    MergeKeyRequired { table: String },
    /// `m#table[id=1]` — the merge's brackets name columns, not conditions.
    MergeKeyIsNotACondition { column: String },
    /// A merge whose key covers every column, leaving nothing to update.
    MergeUpdatesNothing { table: String },
    /// An UPDATE with nothing to set.
    ColumnsRequired { verb: Verb },
    /// An UPDATE column written without its value.
    AssignmentRequired { column: String },
    /// A value written where only a column name belongs.
    AssignmentNotAllowed { verb: Verb, column: String },
    /// An UPDATE or DELETE with no `[...]` at all.
    PredicatesRequired { verb: Verb, table: String },
    MissingOperator { column: String },
    MissingValue { column: String },
    UnknownOperator { symbol: String },
    Syntax { at: usize, message: String },
}

impl fmt::Display for AbbrevError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AbbrevError::Empty => write!(
                f,
                "nothing to expand — an abbreviation is a verb ({VERBS}), then `#`, \
                 then a table: `s#localstrings`"
            ),
            AbbrevError::MissingSeparator { verb } => {
                write!(f, "`{verb}` is missing its `#` — write `{verb}#<table>`")
            }
            AbbrevError::UnknownVerb { verb } if verb.is_empty() => {
                write!(f, "there is no verb before the `#` — use {VERBS}")
            }
            AbbrevError::UnknownVerb { verb } => {
                write!(f, "`{verb}` is not a verb — use {VERBS}")
            }
            AbbrevError::MissingTable { verb } => write!(
                f,
                "`{}#` names no table — {} what?",
                verb.marker(),
                verb.describe().to_lowercase()
            ),
            AbbrevError::UnknownTable { name, suggestion } => {
                write!(f, "the schema has no table called `{name}`")?;
                suggest(f, suggestion)
            }
            AbbrevError::UnknownColumn { name, tables, suggestion } => {
                write!(f, "there is no column `{name}` in {}", list(tables))?;
                suggest(f, suggestion)
            }
            AbbrevError::AmbiguousColumn { name, tables } => write!(
                f,
                "`{name}` is a column of {} — say which one, `{}.{name}`",
                list(tables),
                tables.first().map(String::as_str).unwrap_or("table")
            ),
            AbbrevError::UnknownQualifier { qualifier, tables } => write!(
                f,
                "`{qualifier}` is not one of this abbreviation's tables — it has {}",
                list(tables)
            ),
            AbbrevError::NoForeignKey { from, to } => write!(
                f,
                "there is no foreign key between {from} and {to}, so there is nothing to join \
                 them on — the condition would have to be guessed, so write the join out instead"
            ),
            AbbrevError::AmbiguousJoin { from, to, candidates, hint } => write!(
                f,
                "{from} and {to} are joined by more than one foreign key ({}) — say which, \
                 with `{hint}`",
                candidates.join(", ")
            ),
            AbbrevError::UnknownJoinColumn { from, to, column, candidates } => write!(
                f,
                "`{column}` is not part of any foreign key between {from} and {to} — the keys \
                 there are {}",
                candidates.join(", ")
            ),
            AbbrevError::ChainNotAllowed { verb } => write!(
                f,
                "`>` joins tables and only a SELECT can — {} writes to one table",
                article(*verb)
            ),
            AbbrevError::ColumnsNotAllowed { verb } => write!(
                f,
                "`(...)` names columns and {} has none to name — its columns are in `[...]`",
                article(*verb)
            ),
            AbbrevError::PredicatesNotAllowed { verb } => {
                write!(f, "`[...]` is a WHERE clause and {} has none", article(*verb))
            }
            AbbrevError::MultiplierNotAllowed { verb } => write!(
                f,
                "`*n` repeats a row and only an INSERT has rows to repeat, not {}",
                article(*verb)
            ),
            AbbrevError::BadMultiplier { text } => {
                write!(f, "`*{text}` is not a row count — write `*3`")
            }
            AbbrevError::TemplateNotAllowed { verb } => write!(
                f,
                "`{{…}}` fills in the rows of an INSERT, and {} has no rows",
                article(*verb)
            ),
            AbbrevError::TemplateAndAssignment { column } => write!(
                f,
                "`{column}` is given a value twice — once with `=` and once in the `{{…}}`. \
                 Keep the template and write `{column}` on its own, or drop the template"
            ),
            AbbrevError::TemplateArity { values, columns } => write!(
                f,
                "the `{{…}}` has {values} value(s) and the statement has {} column(s) ({}) — \
                 they have to line up, or the values land in the wrong columns",
                columns.len(),
                columns.join(", ")
            ),
            AbbrevError::ChangesNotAllowed { verb } => write!(
                f,
                "`+` adds a column and `~` retypes one — that is `a#`, not {}",
                article(*verb)
            ),
            AbbrevError::ChangesRequired { table } => write!(
                f,
                "`a#{table}` changes nothing — add a column with `+nome:varchar(200)`, or retype \
                 one with `~importo:number(12,2)`"
            ),
            AbbrevError::MissingType { column } => write!(
                f,
                "`{column}` has no type — write `{column}:varchar(200)`"
            ),
            AbbrevError::ColumnAlreadyExists { table, column } => write!(
                f,
                "{table} already has a column `{column}` — `~{column}:…` changes its type"
            ),
            AbbrevError::MergeKeyRequired { table } => write!(
                f,
                "a merge needs to know what makes a row the same row — name the key columns, \
                 `m#{table}[id]`"
            ),
            AbbrevError::MergeKeyIsNotACondition { column } => write!(
                f,
                "`[{column}=…]` is a condition, and a merge's brackets name its key columns — \
                 write `[{column}]`"
            ),
            AbbrevError::MergeUpdatesNothing { table } => write!(
                f,
                "every column of {table} is part of the key, so a matched row would have nothing \
                 to update — that is an INSERT, not a merge"
            ),
            AbbrevError::ColumnsRequired { verb } => write!(
                f,
                "{} needs something to set — `(column=value)`",
                article(*verb)
            ),
            AbbrevError::AssignmentRequired { column } => write!(
                f,
                "`{column}` has no value — an UPDATE sets columns: `({column}='…')`"
            ),
            AbbrevError::AssignmentNotAllowed { verb, column } => write!(
                f,
                "`{column}=…` assigns a value, and {} only names columns",
                article(*verb)
            ),
            AbbrevError::PredicatesRequired { verb, table } => write!(
                f,
                "{} with no `[...]` would touch every row of {table} — add a condition, \
                 `[id='…']`, or write the statement out",
                article(*verb)
            ),
            AbbrevError::MissingOperator { column } => {
                write!(f, "`{column}` is compared to nothing — write `[{column}='…']`")
            }
            AbbrevError::MissingValue { column } => {
                write!(f, "`{column}` is compared to nothing — the value after it is missing")
            }
            AbbrevError::UnknownOperator { symbol } => write!(
                f,
                "`{symbol}` is not a comparison — use `=`, `<>`, `<`, `<=`, `>`, `>=`, or `~` \
                 for LIKE"
            ),
            AbbrevError::Syntax { at, message } => write!(f, "{message} (at character {at})"),
        }
    }
}

impl std::error::Error for AbbrevError {}

/// The verb list, written once.
///
/// It appears in three refusals, and three copies of it is three chances for the
/// language to grow a verb that one of them forgets to mention — which is exactly
/// how a feature ends up undiscoverable.
const VERBS: &str = "`s` (select), `i` (insert), `u` (update), `d` (delete), `m` (merge), \
    `a` (alter) or `fc` (for loop)";

/// "…, did you mean `X`?" — appended only when there is one.
fn suggest(f: &mut fmt::Formatter<'_>, suggestion: &Option<String>) -> fmt::Result {
    match suggestion {
        Some(name) => write!(f, " — did you mean `{name}`?"),
        None => Ok(()),
    }
}

/// `A`, `A and B`, `A, B and C` — the shape a sentence needs.
fn list(names: &[String]) -> String {
    match names {
        [] => "this abbreviation".to_string(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// "an INSERT" / "a SELECT" — so the sentences above read like sentences.
fn article(verb: Verb) -> String {
    let word = verb.describe();
    let article = if word.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" };
    format!("{article} {word}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_reads_like_a_sentence() {
        assert_eq!(list(&["ORDINI".into()]), "ORDINI");
        assert_eq!(list(&["ORDINI".into(), "CLIENTI".into()]), "ORDINI and CLIENTI");
        assert_eq!(
            list(&["A".into(), "B".into(), "C".into()]),
            "A, B and C"
        );
    }

    #[test]
    fn the_article_matches_the_verb() {
        assert_eq!(article(Verb::Insert), "an INSERT");
        assert_eq!(article(Verb::Update), "an UPDATE");
        assert_eq!(article(Verb::Select), "a SELECT");
        assert_eq!(article(Verb::Delete), "a DELETE");
    }

    #[test]
    fn a_suggestion_is_only_offered_when_there_is_one() {
        let with = AbbrevError::UnknownTable {
            name: "localstring".into(),
            suggestion: Some("LOCALSTRINGS".into()),
        };
        assert!(with.to_string().contains("did you mean `LOCALSTRINGS`?"));

        let without = AbbrevError::UnknownTable { name: "zzz".into(), suggestion: None };
        assert!(!without.to_string().contains("did you mean"), "{without}");
    }
}
