//! Parsed abbreviation + schema → resolved [`Statement`].
//!
//! Everything that needs the schema happens here and nowhere later: names become
//! the schema's spelling, `>` becomes a foreign key, and every value is paired
//! with the [`ValueKind`](crate::schema::ValueKind) of the column it will be
//! compared against. What comes out is an intent, not text — see
//! [`crate::statement`] for why that split is the point of the crate.

use crate::error::AbbrevError;
use crate::join;
use crate::numbering::number;
use crate::parse::parse;
use crate::resolve::Chain;
use crate::schema::SchemaView;
use crate::statement::{
    Assignment, ColumnChange, ColumnRef, InsertRow, Join, JoinCondition, Operator, Predicate,
    Statement, Value,
};
use crate::syntax::{Block, ChangeKind, ColItem, Parsed, PredItem, RawValue, Verb};

/// The largest `*n` an abbreviation may ask for.
///
/// A limit rather than none, because the number reaches a host that will build
/// that many rows, and `*100000` is a typo far more often than it is a request.
pub const MAX_ROWS: usize = 1000;

/// Expand one abbreviation against one schema.
///
/// Either an exact statement or a refusal — never a plausible approximation.
pub fn expand(input: &str, schema: &SchemaView) -> Result<Statement, AbbrevError> {
    let parsed = parse(input);
    if let Some(error) = parsed.error {
        return Err(AbbrevError::Syntax { at: error.at, message: error.message });
    }

    let verb = verb_of(&parsed)?;
    if parsed.table.is_blank() {
        return Err(AbbrevError::MissingTable { verb });
    }
    check_chain_is_typed_out(&parsed)?;

    let chain = Chain::build(schema, &parsed.table_names())?;
    // `+`/`~` shape a table, and only `a#` shapes tables. Checked once, here,
    // rather than in six places that would each have to remember.
    if verb != Verb::Alter && !parsed.changes.is_empty() {
        return Err(AbbrevError::ChangesNotAllowed { verb });
    }
    match verb {
        Verb::Select => select(&parsed, &chain),
        Verb::Insert => insert(&parsed, &chain),
        Verb::Update => update(&parsed, &chain),
        Verb::Delete => delete(&parsed, &chain),
        Verb::Merge => merge(&parsed, &chain),
        Verb::Alter => alter(&parsed, &chain),
        Verb::ForCursor => for_cursor(&parsed, &chain),
    }
}

fn verb_of(parsed: &Parsed) -> Result<Verb, AbbrevError> {
    if parsed.hash.is_none() {
        return Err(if parsed.verb.is_blank() {
            AbbrevError::Empty
        } else {
            AbbrevError::MissingSeparator { verb: parsed.verb.text.clone() }
        });
    }
    Verb::from_word(&parsed.verb.text)
        .ok_or_else(|| AbbrevError::UnknownVerb { verb: parsed.verb.text.clone() })
}

/// A `>` or a `:` the user has opened and not finished. Caught before the schema
/// is consulted so the message is about the abbreviation, not about a table
/// called "".
fn check_chain_is_typed_out(parsed: &Parsed) -> Result<(), AbbrevError> {
    for link in &parsed.chain {
        if link.table.is_blank() {
            return Err(AbbrevError::Syntax {
                at: link.arrow,
                message: "`>` must be followed by a table".to_string(),
            });
        }
        if let Some(column) = &link.column {
            if column.is_blank() {
                return Err(AbbrevError::Syntax {
                    at: column.span.start,
                    message: "`:` must be followed by the column that picks the foreign key"
                        .to_string(),
                });
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- the verbs --

fn select(parsed: &Parsed, chain: &Chain<'_>) -> Result<Statement, AbbrevError> {
    select_as(parsed, chain, Verb::Select)
}

/// The `SELECT` body, told which verb is asking.
///
/// `fc#` is a `SELECT` with a loop round it and reuses every line of this — the
/// joins, the column resolution, the predicates. The verb is a parameter only so
/// that a refusal says `FOR loop` when that is what the user typed; a message that
/// named `SELECT` for a line beginning `fc#` would send them looking for a
/// statement they never wrote.
fn select_as(parsed: &Parsed, chain: &Chain<'_>, verb: Verb) -> Result<Statement, AbbrevError> {
    reject_multiplier(parsed, verb)?;
    reject_template(parsed, verb)?;

    let mut columns = Vec::new();
    for item in live(&parsed.cols) {
        if item.eq.is_some() {
            return Err(AbbrevError::AssignmentNotAllowed {
                verb,
                column: item.name.text.clone(),
            });
        }
        columns.push(chain.column(&item.name.text)?);
    }

    let mut joins = Vec::new();
    for (index, link) in parsed.chain.iter().enumerate() {
        let left = &chain.bounds[index];
        let right = &chain.bounds[index + 1];
        let pick = link.column.as_ref().map(|c| c.text.as_str());
        let key = join::resolve(left.meta, right.meta, pick)?;
        let on = key
            .oriented(&left.meta.name)
            .into_iter()
            .map(|(near, far)| JoinCondition {
                left: chain.key_column(index, &near),
                right: chain.key_column(index + 1, &far),
            })
            .collect();
        joins.push(Join { table: index + 1, to: index, on });
    }

    Ok(Statement::Select {
        tables: chain.table_refs(),
        joins,
        columns,
        predicates: predicates(parsed, chain)?,
    })
}

fn insert(parsed: &Parsed, chain: &Chain<'_>) -> Result<Statement, AbbrevError> {
    reject_chain(parsed, Verb::Insert)?;
    if parsed.preds.is_some() {
        return Err(AbbrevError::PredicatesNotAllowed { verb: Verb::Insert });
    }

    // A value is optional per column, and mixing them (`i#t(a='x',b)`) is
    // deliberately allowed: the two forms mean different things and a user who
    // knows one of the three values should not have to abandon the abbreviation.
    let mut columns = Vec::new();
    let mut written = Vec::new();
    for item in live(&parsed.cols) {
        let value = match &item.value {
            None => None,
            Some(raw) if raw.is_blank() => {
                return Err(AbbrevError::MissingValue { column: item.name.text.clone() })
            }
            Some(raw) => Some(value_of(raw)),
        };
        columns.push(chain.column(&item.name.text)?);
        written.push(value);
    }
    // Naming no columns means all of them — the schema knows the table's shape,
    // which is exactly the work worth not doing by hand.
    if columns.is_empty() {
        let root = chain.root();
        columns = root
            .meta
            .columns
            .iter()
            .map(|c| ColumnRef {
                name: c.name.clone(),
                table: root.meta.name.clone(),
                alias: None,
                kind: c.kind,
            })
            .collect();
        written = vec![None; columns.len()];
    }

    let count = rows(parsed)?;
    let rows = match &parsed.template {
        Some(template) => numbered_rows(template, &columns, &written, count)?,
        // Every row a copy — see `Statement::Insert`.
        None => vec![written; count],
    };

    Ok(Statement::Insert { table: chain.root().meta.name.clone(), columns, rows })
}

/// The rows a `{…}` template produces: one per repetition, `$` replaced by its
/// number.
///
/// Two refusals here, and both are the kind that would otherwise be found by
/// reading the output:
///
/// * a template and an `=` in the same abbreviation say the same thing twice, and
///   there is no reading of `i#t(a='x')*3{$}` where both are honoured;
/// * a template with the wrong number of values silently shifts every column after
///   the missing one, which produces valid SQL that puts the data in the wrong
///   places — the single worst failure this whole language exists to avoid.
fn numbered_rows(
    template: &Block<RawValue>,
    columns: &[ColumnRef],
    written: &[Option<Value>],
    count: usize,
) -> Result<Vec<InsertRow>, AbbrevError> {
    if let Some(index) = written.iter().position(Option::is_some) {
        return Err(AbbrevError::TemplateAndAssignment {
            column: columns[index].name.clone(),
        });
    }

    let values: Vec<&RawValue> = template.items.iter().filter(|v| !v.is_blank()).collect();
    if values.len() != columns.len() {
        return Err(AbbrevError::TemplateArity {
            values: values.len(),
            columns: columns.iter().map(|c| c.name.clone()).collect(),
        });
    }

    Ok((0..count)
        .map(|index| {
            values
                .iter()
                .map(|raw| {
                    Some(match raw.quoted {
                        true => Value::Quoted(number(&raw.inner(), index, count)),
                        false => Value::Bare(number(raw.slot.text.trim(), index, count)),
                    })
                })
                .collect()
        })
        .collect())
}

/// `m#table[key]` — the upsert skeleton.
///
/// The brackets are **not** a `WHERE` here, and that is the one thing about this
/// verb worth knowing: they name the columns that decide whether the row is
/// already there. Written with an operator (`m#t[id=1]`) they are refused rather
/// than reinterpreted, because a merge keyed on a literal is not a thing.
fn merge(parsed: &Parsed, chain: &Chain<'_>) -> Result<Statement, AbbrevError> {
    reject_chain(parsed, Verb::Merge)?;
    reject_multiplier(parsed, Verb::Merge)?;
    reject_template(parsed, Verb::Merge)?;

    let root = chain.root();
    let mut columns = Vec::new();
    for item in live(&parsed.cols) {
        if item.eq.is_some() {
            return Err(AbbrevError::AssignmentNotAllowed {
                verb: Verb::Merge,
                column: item.name.text.clone(),
            });
        }
        columns.push(chain.column(&item.name.text)?);
    }
    if columns.is_empty() {
        columns = root
            .meta
            .columns
            .iter()
            .map(|c| ColumnRef {
                name: c.name.clone(),
                table: root.meta.name.clone(),
                alias: None,
                kind: c.kind,
            })
            .collect();
    }

    let mut keys = Vec::new();
    for item in live(&parsed.preds) {
        if !item.op.is_blank() || !item.value.is_blank() {
            return Err(AbbrevError::MergeKeyIsNotACondition { column: item.name.text.clone() });
        }
        let key = chain.column(&item.name.text)?;
        if !columns.iter().any(|c| c.name == key.name) {
            columns.push(key.clone());
        }
        keys.push(key);
    }
    if keys.is_empty() {
        return Err(AbbrevError::MergeKeyRequired { table: root.meta.name.clone() });
    }
    if columns.iter().all(|c| keys.iter().any(|k| k.name == c.name)) {
        return Err(AbbrevError::MergeUpdatesNothing { table: root.meta.name.clone() });
    }

    Ok(Statement::Merge { table: root.meta.name.clone(), columns, keys })
}

/// `a#table+col:type~col:type` — columns added, columns retyped.
///
/// The schema is consulted in **opposite directions** for the two, which is the
/// whole value of doing this against a live connection rather than in a snippet:
/// adding a column that is already there and retyping one that is not are both
/// caught before the statement is ever run.
fn alter(parsed: &Parsed, chain: &Chain<'_>) -> Result<Statement, AbbrevError> {
    reject_chain(parsed, Verb::Alter)?;
    reject_multiplier(parsed, Verb::Alter)?;
    reject_template(parsed, Verb::Alter)?;
    if parsed.cols.is_some() {
        return Err(AbbrevError::ColumnsNotAllowed { verb: Verb::Alter });
    }
    if parsed.preds.is_some() {
        return Err(AbbrevError::PredicatesNotAllowed { verb: Verb::Alter });
    }
    if parsed.changes.is_empty() {
        return Err(AbbrevError::ChangesRequired { table: chain.root().meta.name.clone() });
    }

    let meta = chain.root().meta;
    let mut changes = Vec::new();
    for change in &parsed.changes {
        if change.column.is_blank() {
            return Err(AbbrevError::Syntax {
                at: change.at,
                message: format!("`{}` must be followed by a column name", change.kind.symbol()),
            });
        }
        let Some(data_type) = change.data_type.as_ref().filter(|s| !s.is_blank()) else {
            return Err(AbbrevError::MissingType { column: change.column.text.clone() });
        };
        let existing = meta.column(&change.column.text);
        let column = match (change.kind, existing) {
            (ChangeKind::Add, Some(found)) => {
                return Err(AbbrevError::ColumnAlreadyExists {
                    table: meta.name.clone(),
                    column: found.name.clone(),
                })
            }
            // The name is taken as typed: nothing in the schema can spell a column
            // that does not exist yet.
            (ChangeKind::Add, None) => change.column.text.clone(),
            (ChangeKind::Modify, Some(found)) => found.name.clone(),
            (ChangeKind::Modify, None) => {
                return Err(AbbrevError::UnknownColumn {
                    name: change.column.text.clone(),
                    tables: vec![meta.name.clone()],
                    suggestion: crate::resolve::suggest(&change.column.text, meta.column_names()),
                })
            }
        };
        changes.push(ColumnChange {
            kind: change.kind,
            column,
            data_type: data_type.text.trim().to_string(),
        });
    }

    Ok(Statement::Alter { table: meta.name.clone(), changes })
}

/// `fc#table[…]` — a cursor loop over a query.
fn for_cursor(parsed: &Parsed, chain: &Chain<'_>) -> Result<Statement, AbbrevError> {
    let query = select_as(parsed, chain, Verb::ForCursor)?;
    // `r` — short, conventional in both PL/SQL and PL/pgSQL, and the body it is
    // used in is a `TODO` the user is about to replace anyway.
    Ok(Statement::ForCursor { variable: "r".to_string(), query: Box::new(query) })
}

fn update(parsed: &Parsed, chain: &Chain<'_>) -> Result<Statement, AbbrevError> {
    reject_chain(parsed, Verb::Update)?;
    reject_multiplier(parsed, Verb::Update)?;
    reject_template(parsed, Verb::Update)?;

    let mut assignments = Vec::new();
    for item in live(&parsed.cols) {
        let Some(raw) = &item.value else {
            return Err(AbbrevError::AssignmentRequired { column: item.name.text.clone() });
        };
        if raw.is_blank() {
            return Err(AbbrevError::MissingValue { column: item.name.text.clone() });
        }
        assignments.push(Assignment { column: chain.column(&item.name.text)?, value: value_of(raw) });
    }
    if assignments.is_empty() {
        return Err(AbbrevError::ColumnsRequired { verb: Verb::Update });
    }

    // Every operator is allowed here. `[quantita>10]` is an ordinary UPDATE, and
    // the restriction to key equality belongs to whichever host has a model that
    // cannot express the rest — not to the language.
    Ok(Statement::Update {
        table: chain.root().meta.name.clone(),
        assignments,
        predicates: required_predicates(parsed, chain, Verb::Update)?,
    })
}

fn delete(parsed: &Parsed, chain: &Chain<'_>) -> Result<Statement, AbbrevError> {
    reject_chain(parsed, Verb::Delete)?;
    reject_multiplier(parsed, Verb::Delete)?;
    reject_template(parsed, Verb::Delete)?;
    if parsed.cols.is_some() {
        return Err(AbbrevError::ColumnsNotAllowed { verb: Verb::Delete });
    }

    Ok(Statement::Delete {
        table: chain.root().meta.name.clone(),
        predicates: required_predicates(parsed, chain, Verb::Delete)?,
    })
}

// --------------------------------------------------------------- the pieces --

/// The items a user actually typed — a trailing comma leaves a blank one behind
/// so the caret has somewhere to be, and expansion is not interested in it.
fn live<'b, T: Live>(block: &'b Option<Block<T>>) -> impl Iterator<Item = &'b T> + 'b {
    block.iter().flat_map(|b| b.items.iter()).filter(|i| !i.is_untouched())
}

/// "Has the user put anything in this item at all?"
trait Live {
    fn is_untouched(&self) -> bool;
}

impl Live for ColItem {
    fn is_untouched(&self) -> bool {
        self.name.is_blank() && self.eq.is_none()
    }
}

impl Live for PredItem {
    fn is_untouched(&self) -> bool {
        self.name.is_blank() && self.op.is_blank() && self.value.is_blank()
    }
}

impl Live for RawValue {
    fn is_untouched(&self) -> bool {
        self.is_blank()
    }
}

fn value_of(raw: &RawValue) -> Value {
    if raw.quoted {
        Value::Quoted(raw.inner())
    } else {
        Value::Bare(raw.slot.text.trim().to_string())
    }
}

fn predicates(parsed: &Parsed, chain: &Chain<'_>) -> Result<Vec<Predicate>, AbbrevError> {
    let mut out = Vec::new();
    for item in live(&parsed.preds) {
        if item.name.is_blank() {
            return Err(AbbrevError::Syntax {
                at: item.name.span.start,
                message: "a condition starts with a column name".to_string(),
            });
        }
        if item.op.is_blank() {
            return Err(AbbrevError::MissingOperator { column: item.name.text.clone() });
        }
        let op = Operator::from_symbol(&item.op.text)
            .ok_or_else(|| AbbrevError::UnknownOperator { symbol: item.op.text.clone() })?;
        if item.value.is_blank() {
            return Err(AbbrevError::MissingValue { column: item.name.text.clone() });
        }
        out.push(Predicate { column: chain.column(&item.name.text)?, op, value: value_of(&item.value) });
    }
    Ok(out)
}

/// The same, for the two verbs that must not run without one.
///
/// An UPDATE or a DELETE with no `WHERE` touches every row of the table, and four
/// characters is far too few to have typed to mean that. There is no opt-in
/// spelling for it either — the way to write it is to write it.
fn required_predicates(
    parsed: &Parsed,
    chain: &Chain<'_>,
    verb: Verb,
) -> Result<Vec<Predicate>, AbbrevError> {
    let predicates = predicates(parsed, chain)?;
    if predicates.is_empty() {
        return Err(AbbrevError::PredicatesRequired {
            verb,
            table: chain.root().meta.name.clone(),
        });
    }
    Ok(predicates)
}

fn rows(parsed: &Parsed) -> Result<usize, AbbrevError> {
    let Some(slot) = &parsed.mult else { return Ok(1) };
    match slot.text.parse::<usize>() {
        Ok(n) if (1..=MAX_ROWS).contains(&n) => Ok(n),
        _ => Err(AbbrevError::BadMultiplier { text: slot.text.clone() }),
    }
}

fn reject_chain(parsed: &Parsed, verb: Verb) -> Result<(), AbbrevError> {
    if parsed.chain.is_empty() {
        return Ok(());
    }
    Err(AbbrevError::ChainNotAllowed { verb })
}

fn reject_multiplier(parsed: &Parsed, verb: Verb) -> Result<(), AbbrevError> {
    if parsed.mult.is_none() {
        return Ok(());
    }
    Err(AbbrevError::MultiplierNotAllowed { verb })
}

fn reject_template(parsed: &Parsed, verb: Verb) -> Result<(), AbbrevError> {
    if parsed.template.is_none() {
        return Ok(());
    }
    Err(AbbrevError::TemplateNotAllowed { verb })
}
