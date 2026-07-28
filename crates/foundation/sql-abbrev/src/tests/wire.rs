//! The JSON shapes, asserted as the contract they are.
//!
//! `Statement` and `CursorContext` cross an IPC boundary to a TypeScript
//! frontend. Renaming a variant or a field is a breaking change to code this
//! crate cannot see, so the spellings are pinned here — a rename that was meant
//! fails one assertion and gets updated deliberately; one that was not, does not
//! reach the frontend at all.

use super::fixture::schema;
use crate::prelude::*;
use serde_json::{json, Value as Json};

fn json_of(input: &str) -> Json {
    serde_json::to_value(expand(input, &schema()).expect("expands")).expect("serialises")
}

#[test]
fn a_select_is_tagged_by_its_verb() {
    let json = json_of("s#localstrings(keycode)[keycode='ita']");
    assert_eq!(json["verb"], "select");
    assert_eq!(json["tables"][0]["name"], "LOCALSTRINGS");
    assert_eq!(json["columns"][0], json!({ "name": "KEYCODE", "table": "LOCALSTRINGS", "kind": "text" }));
    assert_eq!(
        json["predicates"][0],
        json!({
            "column": { "name": "KEYCODE", "table": "LOCALSTRINGS", "kind": "text" },
            "op": "eq",
            "value": { "form": "quoted", "text": "ita" }
        })
    );
    // An absent alias is absent, not `null`: the frontend's `alias?: string`
    // reads the two the same way only by luck.
    assert!(json["tables"][0].get("alias").is_none());
}

#[test]
fn a_join_carries_indices_into_the_table_list() {
    let json = json_of("s#ordini>prodotti");
    assert_eq!(json["tables"][1], json!({ "name": "PRODOTTI", "alias": "P" }));
    assert_eq!(json["joins"][0]["to"], 0);
    assert_eq!(json["joins"][0]["table"], 1);
    assert_eq!(json["joins"][0]["on"][0]["left"]["name"], "ID_PRODOTTO");
    assert_eq!(json["joins"][0]["on"][0]["right"]["name"], "ID");
}

#[test]
fn a_supplied_insert_value_and_a_missing_one_look_different_on_the_wire() {
    let json = json_of("i#localstrings(keycode='ita',value)*2");
    assert_eq!(json["verb"], "insert");
    assert_eq!(json["rows"], 2);
    assert_eq!(json["columns"][0]["value"], json!({ "form": "quoted", "text": "ita" }));
    assert!(json["columns"][1].get("value").is_none(), "a placeholder carries no value at all");
}

#[test]
fn the_writing_verbs_are_tagged_too() {
    assert_eq!(json_of("u#localstrings(value='x')[keycode='ita']")["verb"], "update");
    assert_eq!(json_of("d#localstrings[keycode='ita']")["verb"], "delete");
    assert_eq!(json_of("u#localstrings(value='x')[keycode='ita']")["assignments"][0]["value"]["form"], "quoted");
    assert_eq!(json_of("s#ordini[quantita=15]")["predicates"][0]["value"], json!({ "form": "bare", "text": "15" }));
}

#[test]
fn a_statement_survives_the_round_trip() {
    let statement = expand("s#ordini>clienti:id_cliente(codice,nome)[quantita>=5]", &schema())
        .expect("expands");
    let json = serde_json::to_string(&statement).expect("serialises");
    let back: Statement = serde_json::from_str(&json).expect("deserialises");
    assert_eq!(back, statement);
}

#[test]
fn a_cursor_context_is_tagged_by_where_it_is() {
    let cases = [
        ("s#lo", json!({ "at": "table", "prefix": "lo" })),
        ("s", json!({ "at": "verb", "prefix": "s" })),
        ("s#ordini>cl", json!({ "at": "joinTable", "from": "ordini", "prefix": "cl" })),
        ("s#ordini>clienti:id", json!({ "at": "joinColumn", "from": "ordini", "to": "clienti", "prefix": "id" })),
        ("i#ordini(cod", json!({ "at": "column", "tables": ["ordini"], "prefix": "cod" })),
        ("i#ordini*2", json!({ "at": "multiplier", "prefix": "2" })),
        ("s#ordini[codice='a']", json!({ "at": "none" })),
    ];
    for (input, expected) in cases {
        let context = context_at(input, input.len());
        assert_eq!(serde_json::to_value(&context).unwrap(), expected, "{input}");
    }
}

#[test]
fn a_predicate_value_names_the_column_it_belongs_to() {
    let context = context_at("s#ordini[codice='a", 18);
    assert_eq!(
        serde_json::to_value(&context).unwrap(),
        json!({ "at": "predicateValue", "tables": ["ordini"], "column": "codice", "prefix": "a" })
    );
}

#[test]
fn a_schema_view_can_be_handed_over_as_json() {
    // A host on the far side of a seam builds one and sends it; the shapes are
    // plain enough that it can.
    let view: SchemaView = serde_json::from_value(json!({
        "tables": [{
            "name": "T",
            "columns": [{ "name": "N", "kind": "number" }, { "name": "X" }],
            "foreignKeys": [{ "columns": ["A"], "referencedTable": "U", "referencedColumns": ["B"] }]
        }]
    }))
    .expect("reads");
    let table = view.table("t").expect("found");
    assert_eq!(table.column("n").unwrap().kind, ValueKind::Number);
    assert_eq!(table.column("x").unwrap().kind, ValueKind::Other, "unclassified is the default");
    assert_eq!(table.foreign_keys[0].referenced_table, "U");
}
