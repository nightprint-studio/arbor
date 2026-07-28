//! What is under the caret.
//!
//! Written against the *end* of each input wherever possible, because that is
//! where a caret actually is while somebody types.

use crate::prelude::{context_at, CursorContext};

/// The caret at the end of what was typed — the common case.
fn at_end(input: &str) -> CursorContext {
    context_at(input, input.len())
}

fn tables(context: &CursorContext) -> Vec<String> {
    match context {
        CursorContext::Column { tables, .. }
        | CursorContext::ColumnValue { tables, .. }
        | CursorContext::PredicateColumn { tables, .. }
        | CursorContext::PredicateOperator { tables, .. }
        | CursorContext::PredicateValue { tables, .. } => tables.clone(),
        other => panic!("no tables in {other:?}"),
    }
}

#[test]
fn the_empty_line_asks_for_a_verb() {
    assert_eq!(context_at("", 0), CursorContext::Verb { prefix: String::new() });
    assert_eq!(at_end("s"), CursorContext::Verb { prefix: "s".into() });
    // The `#` itself is still the verb's business.
    assert_eq!(context_at("s#loc", 1), CursorContext::Verb { prefix: "s".into() });
}

#[test]
fn a_half_typed_table() {
    assert_eq!(at_end("s#loc"), CursorContext::Table { prefix: "loc".into() });
    assert_eq!(at_end("s#"), CursorContext::Table { prefix: String::new() });
}

#[test]
fn a_bare_arrow_asks_for_a_table_to_join() {
    assert_eq!(
        at_end("s#ordini>"),
        CursorContext::JoinTable { from: "ordini".into(), prefix: String::new() }
    );
    assert_eq!(
        at_end("s#ordini>cli"),
        CursorContext::JoinTable { from: "ordini".into(), prefix: "cli".into() }
    );
    // The second link hangs off the first, not off the root.
    assert_eq!(
        at_end("s#ordini>clienti>ord"),
        CursorContext::JoinTable { from: "clienti".into(), prefix: "ord".into() }
    );
}

#[test]
fn a_colon_asks_for_the_column_that_picks_the_key() {
    assert_eq!(
        at_end("s#ordini>clienti:id"),
        CursorContext::JoinColumn { from: "ordini".into(), to: "clienti".into(), prefix: "id".into() }
    );
}

#[test]
fn inside_a_column_list() {
    assert_eq!(
        at_end("s#localstrings(key"),
        CursorContext::Column { tables: vec!["localstrings".into()], prefix: "key".into() }
    );
    // After a comma — the slot the parser leaves behind is what answers this.
    assert_eq!(
        at_end("i#localstrings(keycode,"),
        CursorContext::Column { tables: vec!["localstrings".into()], prefix: String::new() }
    );
    // …and inside empty brackets.
    assert_eq!(
        context_at("i#localstrings()", 15),
        CursorContext::Column { tables: vec!["localstrings".into()], prefix: String::new() }
    );
}

#[test]
fn the_columns_offered_are_the_whole_chains() {
    // Both tables, as typed: resolving them is the caller's job, and keeping the
    // schema out of here is what lets this run on every keystroke.
    assert_eq!(tables(&at_end("s#ordini>clienti(no")), vec!["ordini", "clienti"]);
    assert_eq!(tables(&at_end("s#ordini>clienti[no")), vec!["ordini", "clienti"]);
}

#[test]
fn inside_a_set_value() {
    assert_eq!(
        at_end("u#localstrings(value="),
        CursorContext::ColumnValue {
            tables: vec!["localstrings".into()],
            column: Some("value".into()),
            prefix: String::new(),
        }
    );
}

#[test]
fn inside_a_condition() {
    assert_eq!(
        at_end("s#localstrings[keyc"),
        CursorContext::PredicateColumn { tables: vec!["localstrings".into()], prefix: "keyc".into() }
    );
    // Right after `=` the user wants a value, not another operator — which is
    // why the value slot is asked about before the operator slot.
    assert_eq!(
        at_end("s#localstrings[keycode="),
        CursorContext::PredicateValue {
            tables: vec!["localstrings".into()],
            column: Some("keycode".into()),
            prefix: String::new(),
        }
    );
    // The opening quote is punctuation, not part of what is being filtered on.
    assert_eq!(
        at_end("s#localstrings[keycode='it"),
        CursorContext::PredicateValue {
            tables: vec!["localstrings".into()],
            column: Some("keycode".into()),
            prefix: "it".into(),
        }
    );
}

#[test]
fn between_the_two_characters_of_an_operator() {
    // "s#ordini[quantita>=5]" — the caret sits between `>` and `=`.
    assert_eq!(
        context_at("s#ordini[quantita>=5]", 18),
        CursorContext::PredicateOperator {
            tables: vec!["ordini".into()],
            column: Some("quantita".into()),
            prefix: ">".into(),
        }
    );
}

#[test]
fn after_a_star() {
    assert_eq!(at_end("i#localstrings*"), CursorContext::Multiplier { prefix: String::new() });
    assert_eq!(at_end("i#localstrings*1"), CursorContext::Multiplier { prefix: "1".into() });
}

#[test]
fn punctuation_and_the_end_of_everything_offer_nothing() {
    // On the closing bracket, and past the last thing the grammar allows.
    assert_eq!(at_end("s#localstrings(keycode)"), CursorContext::None);
    assert_eq!(at_end("s#localstrings[keycode='ita']"), CursorContext::None);
}

#[test]
fn a_caret_from_a_different_editor_cannot_panic_this() {
    // Past the end, and inside a multi-byte character.
    assert_eq!(at_end("s#loc"), context_at("s#loc", 9999));
    let accented = "s#clienti[nome='città";
    assert!(matches!(context_at(accented, accented.len() - 1), CursorContext::PredicateValue { .. }));
}

#[test]
fn the_context_and_the_expansion_come_from_one_parse() {
    // The property, asserted the only way it can be: a table the caret is inside
    // of is the same text the expansion would look up.
    let input = "s#ordini>clienti:id_cliente(codice)[quantita>5]";
    let CursorContext::Table { prefix } = context_at(input, 8) else { panic!("not the table") };
    assert_eq!(prefix, "ordini");
    assert_eq!(crate::prelude::parse(input).table.text, prefix);
}
