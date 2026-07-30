//! The verbs that are not the four: `m#` (merge), `a#` (alter), `fc#` (loop), and
//! the `{…}` row template that makes `*n` produce rows instead of copies.
//!
//! What these have in common is that they are the ones a schema-aware tool can get
//! *right* and a snippet cannot: the merge knows which columns are not the key,
//! the alter knows whether the column is already there, and the loop knows what
//! the query joins on. Every test below is one of those facts.

use super::fixture::{refusal, schema, sql};
use crate::prelude::*;

// ── The row template ─────────────────────────────────────────────────────────

#[test]
fn a_template_gives_every_repetition_its_own_values() {
    // The headline case, exactly as a user would write it.
    assert_eq!(
        sql("i#ordini(id,codice)*3{$, 'COD_$'}"),
        "INSERT INTO ORDINI (ID, CODICE) VALUES (1, 'COD_1'), (2, 'COD_2'), (3, 'COD_3')"
    );
}

#[test]
fn a_templates_values_are_quoted_by_their_columns_type_like_any_other() {
    // `$` in a numeric column is a number; the same `$` in a text column is a
    // string. Nothing about the template escapes the one rule this crate is for.
    let out = sql("i#ordini(quantita,codice)*2{$, $}");
    assert_eq!(out, "INSERT INTO ORDINI (QUANTITA, CODICE) VALUES (1, '1'), (2, '2')");
}

#[test]
fn without_a_template_the_repetitions_are_still_copies() {
    // The old behaviour, unchanged: `*3` alone is three identical rows, which is
    // what a seed-data user wants before editing them apart.
    assert_eq!(
        sql("i#ordini(codice='ab',quantita)*3"),
        "INSERT INTO ORDINI (CODICE, QUANTITA) VALUES ('ab', ?), ('ab', ?), ('ab', ?)"
    );
}

#[test]
fn a_template_with_the_wrong_number_of_values_is_refused() {
    // The failure this refusal exists for: two values into three columns is valid
    // SQL that puts the data in the wrong columns.
    let message = refusal("i#ordini(id,codice,quantita)*2{$, 'x'}");
    assert!(message.contains("2 value(s)"), "{message}");
    assert!(message.contains("ID, CODICE, QUANTITA"), "{message}");
}

#[test]
fn a_template_and_an_assignment_are_refused_together() {
    let message = refusal("i#ordini(codice='ab',id)*2{'x', $}");
    assert!(message.contains("CODICE"), "{message}");
    assert!(message.contains("twice"), "{message}");
}

#[test]
fn a_template_belongs_to_an_insert_and_nothing_else() {
    for input in ["s#ordini{$}", "u#ordini(codice='a')[id=1]{$}", "d#ordini[id=1]{$}"] {
        assert!(refusal(input).contains("`{…}`"), "{input}");
    }
}

#[test]
fn a_template_with_no_multiplier_is_one_row() {
    assert_eq!(sql("i#ordini(id){$}"), "INSERT INTO ORDINI (ID) VALUES (1)");
}

// ── The merge ────────────────────────────────────────────────────────────────

#[test]
fn a_merge_keys_on_the_bracketed_columns_and_updates_the_rest() {
    let statement = expand("m#localstrings[keycode]", &schema()).expect("expands");
    let Statement::Merge { table, columns, keys } = &statement else { panic!("{statement:?}") };
    assert_eq!(table, "LOCALSTRINGS");
    // No column list means the whole table — the point of nine characters.
    assert_eq!(columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["KEYCODE", "VALUE"]);
    assert_eq!(keys.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["KEYCODE"]);
}

#[test]
fn a_merge_renders_the_standard_form_with_named_parameters() {
    let out = sql("m#localstrings[keycode]");
    assert!(out.starts_with("MERGE INTO LOCALSTRINGS d"), "{out}");
    assert!(out.contains(":KEYCODE AS KEYCODE, :VALUE AS VALUE"), "{out}");
    assert!(out.contains("ON (d.KEYCODE = s.KEYCODE)"), "{out}");
    // The key is not in the SET: updating a row's identity to itself is noise at
    // best, and on a real key it is the thing the merge matched on.
    assert!(out.contains("d.VALUE = s.VALUE"), "{out}");
    assert!(!out.contains("d.KEYCODE = s.KEYCODE,"), "the key is never updated — {out}");
}

#[test]
fn a_merge_can_be_narrowed_to_some_columns_and_the_key_joins_them() {
    let statement = expand("m#ordini(codice,quantita)[id]", &schema()).expect("expands");
    let Statement::Merge { columns, keys, .. } = &statement else { panic!() };
    // `ID` was not in the list and is added, because a merge that did not insert
    // its own key would produce a row it could never match again.
    assert_eq!(
        columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["CODICE", "QUANTITA", "ID"]
    );
    assert_eq!(keys.len(), 1);
}

#[test]
fn a_merge_with_no_key_is_refused() {
    let message = refusal("m#ordini");
    assert!(message.contains("m#ORDINI[id]"), "{message}");
}

#[test]
fn a_merges_brackets_are_columns_and_not_conditions() {
    // The one mistake everybody makes, coming from `u#` and `d#`.
    let message = refusal("m#ordini[id=1]");
    assert!(message.contains("write `[id]`"), "{message}");
}

#[test]
fn a_merge_keyed_on_every_column_is_refused_as_an_insert() {
    let message = refusal("m#localstrings[keycode,value]");
    assert!(message.contains("nothing"), "{message}");
    assert!(message.contains("INSERT"), "{message}");
}

// ── The alter ────────────────────────────────────────────────────────────────

#[test]
fn an_alter_adds_and_retypes_in_the_order_written() {
    let statement =
        expand("a#ordini+nota:varchar(200)~quantita:number(12,2)", &schema()).expect("expands");
    let Statement::Alter { table, changes } = &statement else { panic!("{statement:?}") };
    assert_eq!(table, "ORDINI");
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].kind, ChangeKind::Add);
    assert_eq!(changes[0].column, "nota");
    assert_eq!(changes[0].data_type, "varchar(200)");
    assert_eq!(changes[1].kind, ChangeKind::Modify);
    // Retyping resolves against the schema, so the canonical spelling wins.
    assert_eq!(changes[1].column, "QUANTITA");
    assert_eq!(changes[1].data_type, "number(12,2)");
}

#[test]
fn a_type_may_be_several_words() {
    // `timestamp with time zone` is one type and four words, and a reader that
    // stopped at the first space would be useless for exactly the columns that
    // are most tedious to write out.
    let statement = expand("a#ordini+creato:timestamp with time zone", &schema()).expect("expands");
    let Statement::Alter { changes, .. } = &statement else { panic!() };
    assert_eq!(changes[0].data_type, "timestamp with time zone");
}

#[test]
fn adding_a_column_that_is_already_there_is_refused() {
    // Only a tool holding the schema can say this, which is the whole argument
    // for the feature over a snippet.
    let message = refusal("a#ordini+codice:varchar(30)");
    assert!(message.contains("already has a column `CODICE`"), "{message}");
    assert!(message.contains("~CODICE"), "the way out is named — {message}");
}

#[test]
fn retyping_a_column_that_is_not_there_is_refused_with_the_near_miss() {
    let message = refusal("a#ordini~quantitaa:number");
    assert!(message.contains("did you mean `QUANTITA`?"), "{message}");
}

#[test]
fn a_change_with_no_type_is_refused() {
    assert!(refusal("a#ordini+nota").contains("has no type"));
}

#[test]
fn an_alter_that_changes_nothing_says_how_to_change_something() {
    let message = refusal("a#ordini");
    assert!(message.contains("+nome:varchar(200)"), "{message}");
}

#[test]
fn a_change_marker_belongs_to_the_alter_verb_alone() {
    let message = refusal("s#ordini+nota:varchar(200)");
    assert!(message.contains("that is `a#`"), "{message}");
}

#[test]
fn an_alter_renders_one_statement_per_change() {
    let out = sql("a#ordini+nota:varchar(200)~quantita:number(12,2)");
    assert_eq!(
        out,
        "ALTER TABLE ORDINI ADD COLUMN nota varchar(200);\n\
         ALTER TABLE ORDINI ALTER COLUMN QUANTITA TYPE number(12,2);"
    );
}

// ── The cursor loop ──────────────────────────────────────────────────────────

#[test]
fn a_cursor_loop_wraps_the_query_it_would_have_selected() {
    let statement = expand("fc#ordini[codice='EV']", &schema()).expect("expands");
    let Statement::ForCursor { variable, query } = &statement else { panic!("{statement:?}") };
    assert_eq!(variable, "r");
    // A whole SELECT inside, not a flattened copy of its parts.
    assert!(matches!(**query, Statement::Select { .. }));
}

#[test]
fn a_cursor_loop_renders_a_body_the_user_is_meant_to_replace() {
    assert_eq!(
        sql("fc#ordini[codice='EV']"),
        "FOR r IN SELECT * FROM ORDINI WHERE CODICE = 'EV' LOOP\n  NULL;\nEND LOOP;"
    );
}

#[test]
fn a_cursor_loop_inherits_the_joins_and_the_column_list() {
    // Free, because the query is a `Statement::Select` and nothing about joins was
    // reimplemented for this verb.
    let out = sql("fc#ordini>prodotti(codice,nome)");
    assert!(out.contains("JOIN PRODOTTI P ON O.ID_PRODOTTO = P.ID"), "{out}");
    assert!(out.contains("SELECT O.CODICE, P.NOME"), "{out}");
}

#[test]
fn a_cursor_loop_over_nothing_is_allowed() {
    // Unlike an UPDATE, a loop with no WHERE is a perfectly ordinary thing to
    // write — it reads every row, which is what a loop is usually for.
    assert!(sql("fc#localstrings").starts_with("FOR r IN SELECT * FROM LOCALSTRINGS LOOP"));
}

// ── Discovery ────────────────────────────────────────────────────────────────

#[test]
fn every_verb_is_reachable_by_its_marker_and_by_its_word() {
    assert_eq!(Verb::from_word("m"), Some(Verb::Merge));
    assert_eq!(Verb::from_word("merge"), Some(Verb::Merge));
    assert_eq!(Verb::from_word("upsert"), Some(Verb::Merge));
    assert_eq!(Verb::from_word("a"), Some(Verb::Alter));
    assert_eq!(Verb::from_word("alter"), Some(Verb::Alter));
    assert_eq!(Verb::from_word("fc"), Some(Verb::ForCursor));
    assert_eq!(Verb::from_word("for"), Some(Verb::ForCursor));
    assert_eq!(Verb::from_word("loop"), Some(Verb::ForCursor));
}

#[test]
fn an_unknown_verb_names_every_verb_there_is() {
    // The list lives in one constant precisely so this cannot go stale.
    let message = refusal("zz#ordini");
    for marker in Verb::ALL.iter().map(|v| format!("`{}`", v.marker())) {
        assert!(message.contains(&marker), "{marker} missing from: {message}");
    }
}
