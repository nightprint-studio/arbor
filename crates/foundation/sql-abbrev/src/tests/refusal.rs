//! Every way an abbreviation can be refused, and what it says when it is.
//!
//! Asserted on the **message**, not on the variant, because the message is what
//! the contract actually is: it crosses the host's seam as text and lands in
//! front of the person typing. A refusal they cannot act on is a bug even when
//! the variant is right.

use super::fixture::refusal;

#[test]
fn nothing_typed_yet() {
    assert!(refusal("").contains("nothing to expand"));
    assert!(refusal("   ").contains("nothing to expand"));
}

#[test]
fn a_verb_without_its_separator() {
    let message = refusal("select");
    assert!(message.contains("missing its `#`"), "{message}");
    assert!(message.contains("select#<table>"), "{message}");
}

#[test]
fn a_verb_that_is_not_one() {
    assert!(refusal("x#localstrings").contains("`x` is not a verb"));
    assert!(refusal("#localstrings").contains("no verb before the `#`"));
}

#[test]
fn a_verb_with_no_table() {
    assert!(refusal("s#").contains("names no table"));
}

#[test]
fn a_table_the_schema_does_not_have() {
    // Near enough to name the near miss…
    let near = refusal("s#localstring");
    assert!(near.contains("no table called `localstring`"), "{near}");
    assert!(near.contains("did you mean `LOCALSTRINGS`?"), "{near}");
    // …and far enough not to.
    let far = refusal("s#qqqqqqqqqq");
    assert!(far.contains("no table called `qqqqqqqqqq`"), "{far}");
    assert!(!far.contains("did you mean"), "{far}");
}

#[test]
fn a_column_the_table_does_not_have() {
    let message = refusal("s#localstrings(keycod)");
    assert!(message.contains("no column `keycod` in LOCALSTRINGS"), "{message}");
    assert!(message.contains("did you mean `KEYCODE`?"), "{message}");
}

#[test]
fn a_column_in_more_than_one_of_the_chains_tables() {
    // ID is on ORDINI and on CLIENTI. Binding it to the first would be an
    // accident of the order the tables were typed in.
    let message = refusal("s#ordini>clienti:id_cliente(id)");
    assert!(message.contains("`id` is a column of ORDINI and CLIENTI"), "{message}");
    assert!(message.contains("`ORDINI.id`"), "{message}");
}

#[test]
fn a_qualifier_that_is_not_one_of_the_tables() {
    let message = refusal("s#localstrings(clienti.nome)");
    assert!(message.contains("`clienti` is not one of this abbreviation's tables"), "{message}");
    assert!(message.contains("LOCALSTRINGS"), "{message}");
}

#[test]
fn two_tables_with_no_foreign_key_between_them() {
    let message = refusal("s#ordini>log");
    assert!(message.contains("no foreign key between ORDINI and LOG"), "{message}");
    assert!(message.contains("write the join out"), "{message}");
}

#[test]
fn two_tables_with_more_than_one() {
    let message = refusal("s#ordini>clienti");
    assert!(message.contains("more than one foreign key"), "{message}");
    assert!(message.contains("ORDINI.ID_CLIENTE → CLIENTI.ID"), "{message}");
    assert!(message.contains("ORDINI.ID_CLIENTE_FATTURAZIONE → CLIENTI.ID"), "{message}");
    // The way out, spelled so it can be copied.
    assert!(message.contains(">clienti:id_cliente_fatturazione"), "{message}");
}

#[test]
fn a_disambiguating_column_no_key_uses() {
    let message = refusal("s#ordini>clienti:id_fornitore");
    assert!(message.contains("`id_fornitore` is not part of any foreign key"), "{message}");
    assert!(message.contains("ORDINI.ID_CLIENTE →"), "{message}");
}

#[test]
fn a_chain_on_a_verb_that_writes() {
    for input in ["i#ordini>clienti", "u#ordini>clienti", "d#ordini>clienti"] {
        let message = refusal(input);
        assert!(message.contains("`>` joins tables and only a SELECT can"), "{input}: {message}");
    }
}

#[test]
fn brackets_where_the_verb_has_no_use_for_them() {
    assert!(refusal("i#localstrings(keycode)[keycode='x']").contains("`[...]` is a WHERE clause and an INSERT has none"));
    assert!(refusal("d#localstrings(keycode)[keycode='x']").contains("`(...)` names columns and a DELETE has none"));
}

#[test]
fn a_multiplier_on_something_with_no_rows() {
    for input in ["s#localstrings*3", "u#localstrings(value='x')[keycode='ita']*3", "d#localstrings[keycode='ita']*3"] {
        assert!(refusal(input).contains("only an INSERT has rows to repeat"), "{input}");
    }
}

#[test]
fn a_multiplier_that_is_not_a_count() {
    assert!(refusal("i#localstrings*0").contains("`*0` is not a row count"));
    assert!(refusal("i#localstrings*99999").contains("is not a row count"));
    assert!(refusal("i#localstrings*").contains("is not a row count"));
}

#[test]
fn an_update_with_nothing_to_set() {
    assert!(refusal("u#localstrings[keycode='ita']").contains("an UPDATE needs something to set"));
}

#[test]
fn an_update_column_written_without_its_value() {
    let message = refusal("u#localstrings(value)[keycode='ita']");
    assert!(message.contains("`value` has no value"), "{message}");
    assert!(message.contains("(value='…')"), "{message}");
}

#[test]
fn a_value_where_only_a_column_belongs() {
    // A SELECT's column list is the only one that cannot take a value — an
    // INSERT's can, and does.
    assert!(refusal("s#localstrings(value='x')").contains("`value=…` assigns a value, and a SELECT only names columns"));
}

#[test]
fn an_update_or_delete_that_would_touch_every_row() {
    // There is deliberately no opt-in spelling for "yes, all of them": the way to
    // write it is to write it.
    let update = refusal("u#localstrings(value='x')");
    assert!(update.contains("an UPDATE with no `[...]` would touch every row of LOCALSTRINGS"), "{update}");
    let delete = refusal("d#localstrings");
    assert!(delete.contains("a DELETE with no `[...]` would touch every row of LOCALSTRINGS"), "{delete}");
}

#[test]
fn a_condition_that_is_not_finished() {
    assert!(refusal("s#localstrings[keycode]").contains("`keycode` is compared to nothing"));
    assert!(refusal("s#localstrings[keycode=]").contains("the value after it is missing"));
    assert!(refusal("s#localstrings[keycode!!'x']").contains("`!!` is not a comparison"));
    // …and the same for an INSERT column whose `=` leads nowhere.
    assert!(refusal("i#localstrings(keycode=)").contains("the value after it is missing"));
}

#[test]
fn brackets_and_strings_left_open() {
    assert!(refusal("s#localstrings(keycode").contains("`(` is never closed"));
    assert!(refusal("s#localstrings[keycode='ita").contains("a quoted value is never closed"));
    assert!(refusal("s#localstrings[keycode='ita'").contains("`[` is never closed"));
}

#[test]
fn a_chain_the_user_has_not_finished() {
    assert!(refusal("s#ordini>").contains("`>` must be followed by a table"));
    assert!(refusal("s#ordini>clienti:").contains("`:` must be followed by the column"));
}

#[test]
fn junk_after_the_abbreviation() {
    let message = refusal("s#localstrings ?!");
    assert!(message.contains("unexpected `?`"), "{message}");
    assert!(message.contains("at character"), "{message}");
}
