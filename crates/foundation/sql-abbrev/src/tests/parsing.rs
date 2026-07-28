//! The parser, on its own — including on input nobody could expand.

use crate::prelude::parse;

#[test]
fn the_parser_never_fails_on_half_typed_input() {
    // Every one of these is a real keystroke on the way to something valid, and
    // every one has to leave a structure a completion can be answered from.
    for input in ["", "s", "s#", "s#loc", "s#ordini>", "s#ordini>clienti:", "i#t(", "i#t(a,", "u#t(a=", "s#t[", "s#t[a", "s#t[a=", "s#t*"] {
        let parsed = parse(input);
        // A syntax error is allowed; a missing structure is not.
        assert_eq!(parsed.verb.span.start, 0, "{input}");
    }
}

#[test]
fn an_empty_list_still_leaves_a_slot_to_stand_in() {
    // `i#t()` has no columns and one place a column could go.
    let parsed = parse("i#t()");
    let cols = parsed.cols.expect("a list");
    assert_eq!(cols.items.len(), 1);
    assert!(cols.items[0].name.is_blank());
    assert!(cols.closed);
}

#[test]
fn a_trailing_comma_leaves_a_slot_after_it() {
    let parsed = parse("i#t(a,");
    let cols = parsed.cols.expect("a list");
    assert_eq!(cols.items.len(), 2);
    assert_eq!(cols.items[0].name.text, "a");
    assert_eq!(cols.items[1].name.span.start, 6, "the empty slot is where the caret is");
    assert!(!cols.closed);
}

#[test]
fn a_chain_link_records_where_its_arrow_was() {
    let parsed = parse("s#ordini>clienti:id_cliente");
    assert_eq!(parsed.chain.len(), 1);
    assert_eq!(parsed.chain[0].arrow, 8);
    assert_eq!(parsed.chain[0].table.text, "clienti");
    assert_eq!(parsed.chain[0].column.as_ref().expect("picked").text, "id_cliente");
}

#[test]
fn an_angle_bracket_inside_a_condition_is_an_operator() {
    // The chain is parsed before the brackets are opened, which is the whole
    // reason `>` can be both.
    let parsed = parse("s#ordini[quantita>=5]");
    assert!(parsed.chain.is_empty(), "no join here");
    let preds = parsed.preds.expect("conditions");
    assert_eq!(preds.items[0].op.text, ">=");
    assert_eq!(preds.items[0].value.slot.text, "5");
}

#[test]
fn a_doubled_quote_does_not_end_a_value() {
    let parsed = parse("s#clienti[nome='d''annunzio']");
    let value = &parsed.preds.expect("conditions").items[0].value;
    assert!(value.quoted && value.terminated);
    assert_eq!(value.inner(), "d'annunzio");
    assert!(parse("s#clienti[nome='d''annunzio']").error.is_none());
}

#[test]
fn the_verb_slot_holds_whatever_was_typed_before_the_hash() {
    assert_eq!(parse("select#t").verb.text, "select");
    assert_eq!(parse("zz#t").verb.text, "zz", "the parser does not judge; expansion does");
    assert_eq!(parse("#t").verb.text, "");
    assert_eq!(parse("s#t").hash, Some(1));
    assert_eq!(parse("st").hash, None);
}
