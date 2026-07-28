//! What is under the caret — the other half of the feature.
//!
//! An abbreviation language nobody can discover is a language nobody uses, so the
//! editor has to be able to offer table names after `#`, column names inside
//! `(...)`, and the columns of the *right* tables inside `[...]`. That means
//! answering "where am I" for input that is, by definition, half-typed.
//!
//! It comes from [`parse`](crate::parse::parse) — **the same parse an expansion
//! uses**. A second, more forgiving parser written for completion is the failure
//! mode this crate is shaped to avoid: two parsers drift, and the day they
//! disagree the editor offers a column for a table the expansion is not going to
//! use.
//!
//! Everything here is text **as typed**, not resolved: the schema is not
//! consulted. `tables` is what the user wrote, and looking those up is the
//! caller's job — it already has the schema, and keeping it out of here is what
//! lets a completion run on every keystroke.

use serde::{Deserialize, Serialize};

use crate::parse::parse;
use crate::span::clamp_to_boundary;
use crate::statement::Operator;
use crate::syntax::{Block, ColItem, Parsed, PredItem, Verb};

/// Where the caret is, and what would be worth offering there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "camelCase")]
pub enum CursorContext {
    /// Before the `#`. Offer the four verbs.
    Verb { prefix: String },
    /// The root table.
    Table { prefix: String },
    /// After a `>`. Offer tables — a host that wants to be helpful offers the
    /// ones `from` has a foreign key to or from, first.
    JoinTable { from: String, prefix: String },
    /// After a `>table:`. Offer the columns of the foreign keys between the two.
    JoinColumn { from: String, to: String, prefix: String },
    /// A name inside `(...)`.
    Column { tables: Vec<String>, prefix: String },
    /// A value inside `(...)`, i.e. what an `UPDATE` sets a column to.
    ColumnValue { tables: Vec<String>, column: Option<String>, prefix: String },
    /// A name inside `[...]`.
    PredicateColumn { tables: Vec<String>, prefix: String },
    /// Between a predicate's column and its value. Offer [`Operator::SYMBOLS`].
    PredicateOperator { tables: Vec<String>, column: Option<String>, prefix: String },
    /// A value inside `[...]`.
    PredicateValue { tables: Vec<String>, column: Option<String>, prefix: String },
    /// After `*`.
    Multiplier { prefix: String },
    /// On punctuation, or past the end of everything the grammar allows. There is
    /// nothing to offer, and offering something anyway is how a completion popup
    /// becomes the thing users turn off.
    None,
}

impl CursorContext {
    /// The operator symbols worth offering, for a host that would rather not name
    /// [`Operator`] itself.
    pub fn operator_symbols() -> Vec<&'static str> {
        Operator::SYMBOLS.iter().map(|(symbol, _)| *symbol).collect()
    }

    /// The verbs worth offering, as `(marker, keyword)`.
    pub fn verbs() -> Vec<(&'static str, &'static str)> {
        Verb::ALL.iter().map(|v| (v.marker(), v.keyword())).collect()
    }
}

/// What is under the caret at `cursor`, a **byte** offset into `input`.
///
/// Clamped and walked back to a character boundary, so a caret from an editor
/// that counts differently cannot panic a completion handler.
pub fn context_at(input: &str, cursor: usize) -> CursorContext {
    let cursor = clamp_to_boundary(input, cursor);
    locate(&parse(input), cursor)
}

fn locate(parsed: &Parsed, cursor: usize) -> CursorContext {
    // Before the separator, everything is still the verb.
    match parsed.hash {
        None => return CursorContext::Verb { prefix: parsed.verb.prefix_to(cursor) },
        Some(hash) if cursor <= hash => {
            return CursorContext::Verb { prefix: parsed.verb.prefix_to(cursor) }
        }
        Some(_) => {}
    }

    if parsed.table.span.holds(cursor) {
        return CursorContext::Table { prefix: parsed.table.prefix_to(cursor) };
    }

    for (index, link) in parsed.chain.iter().enumerate() {
        if link.table.span.holds(cursor) {
            return CursorContext::JoinTable {
                from: parsed.link_source(index),
                prefix: link.table.prefix_to(cursor),
            };
        }
        if let Some(column) = &link.column {
            if column.span.holds(cursor) {
                return CursorContext::JoinColumn {
                    from: parsed.link_source(index),
                    to: link.table.text.clone(),
                    prefix: column.prefix_to(cursor),
                };
            }
        }
    }

    let tables = parsed.table_names();
    if let Some(context) = in_columns(parsed, cursor, &tables) {
        return context;
    }
    if let Some(context) = in_predicates(parsed, cursor, &tables) {
        return context;
    }
    if let Some(slot) = &parsed.mult {
        if slot.span.holds(cursor) {
            return CursorContext::Multiplier { prefix: slot.prefix_to(cursor) };
        }
    }
    CursorContext::None
}

/// `Some` as soon as the caret is anywhere inside the brackets — including on the
/// brackets themselves, where the answer is [`CursorContext::None`] rather than
/// "keep looking". Falling through to the next block would put a caret sitting on
/// `)` into whatever comes after it.
fn in_columns(parsed: &Parsed, cursor: usize, tables: &[String]) -> Option<CursorContext> {
    let block: &Block<ColItem> = parsed.cols.as_ref()?;
    if !block.span.holds(cursor) {
        return None;
    }
    for item in &block.items {
        if item.name.span.holds(cursor) {
            return Some(CursorContext::Column {
                tables: tables.to_vec(),
                prefix: item.name.prefix_to(cursor),
            });
        }
        if let Some(value) = &item.value {
            if value.slot.span.holds(cursor) {
                return Some(CursorContext::ColumnValue {
                    tables: tables.to_vec(),
                    column: named(&item.name.text),
                    prefix: value_prefix(&value.slot.prefix_to(cursor)),
                });
            }
        }
    }
    Some(CursorContext::None)
}

fn in_predicates(parsed: &Parsed, cursor: usize, tables: &[String]) -> Option<CursorContext> {
    let block: &Block<PredItem> = parsed.preds.as_ref()?;
    if !block.span.holds(cursor) {
        return None;
    }
    for item in &block.items {
        if item.name.span.holds(cursor) {
            return Some(CursorContext::PredicateColumn {
                tables: tables.to_vec(),
                prefix: item.name.prefix_to(cursor),
            });
        }
        // The value is asked before the operator, and the order is load-bearing:
        // the two slots meet at the caret you have immediately after typing `=`,
        // and there the user wants a value, not another operator.
        if item.value.slot.span.holds(cursor) {
            return Some(CursorContext::PredicateValue {
                tables: tables.to_vec(),
                column: named(&item.name.text),
                prefix: value_prefix(&item.value.slot.prefix_to(cursor)),
            });
        }
        if item.op.span.holds(cursor) {
            return Some(CursorContext::PredicateOperator {
                tables: tables.to_vec(),
                column: named(&item.name.text),
                prefix: item.op.prefix_to(cursor),
            });
        }
    }
    Some(CursorContext::None)
}

fn named(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_string())
}

/// A value's opening quote is punctuation, not something the user is filtering
/// on: after `[nome='ro` the prefix is `ro`.
fn value_prefix(typed: &str) -> String {
    typed.strip_prefix('\'').unwrap_or(typed).to_string()
}
