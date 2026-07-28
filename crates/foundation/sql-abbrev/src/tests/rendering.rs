//! The default renderer — the convenience, not the contract.

use super::fixture::{lowercase_schema, schema};
use crate::prelude::*;

fn with(input: &str, style: &RenderStyle) -> String {
    render(&expand(input, &schema()).expect("expands"), style)
}

#[test]
fn the_users_own_example_comes_out_of_a_lower_case_schema() {
    let statement = expand("s#localstrings(keycode,value)[keycode='ita']", &lowercase_schema())
        .expect("expands");
    assert_eq!(
        render(&statement, &RenderStyle::lowercase_keywords()),
        "select keycode, value from localstrings where keycode = 'ita'"
    );
}

#[test]
fn an_alias_follows_the_schemas_case_so_the_line_reads_as_one_thing() {
    let statement = expand("s#ordini>clienti[nome='rossi']", &lowercase_schema()).expect("expands");
    assert_eq!(
        render(&statement, &RenderStyle::lowercase_keywords()),
        "select * from ordini o join clienti c on o.id_cliente = c.id where c.nome = 'rossi'"
    );
}

#[test]
fn nothing_is_terminated_by_default() {
    // An abbreviation expands to *a statement*; whether it needs a `;` is a fact
    // about the document it is going into.
    assert!(!with("s#localstrings", &RenderStyle::default()).ends_with(';'));
    let terminated = RenderStyle { terminator: Some(';'), ..RenderStyle::default() };
    assert_eq!(with("s#localstrings", &terminated), "SELECT * FROM LOCALSTRINGS;");
}

#[test]
fn the_placeholder_and_the_quote_are_the_hosts_to_choose() {
    let style = RenderStyle { placeholder: ":1".into(), ..RenderStyle::default() };
    assert_eq!(
        with("i#localstrings(keycode)", &style),
        "INSERT INTO LOCALSTRINGS (KEYCODE) VALUES (:1)"
    );
    let backtick = RenderStyle { quote: '`', ..RenderStyle::default() };
    assert_eq!(
        with("s#localstrings[keycode='ita']", &backtick),
        "SELECT * FROM LOCALSTRINGS WHERE KEYCODE = `ita`"
    );
}

#[test]
fn a_multiplier_becomes_one_statement_with_several_tuples() {
    // Standard SQL, and deliberately not Oracle's — which is exactly why the
    // statement carries a row *count* and a host with an Oracle destination reads
    // it and emits one statement per row instead.
    assert_eq!(
        with("i#localstrings(keycode,value)*3", &RenderStyle::default()),
        "INSERT INTO LOCALSTRINGS (KEYCODE, VALUE) VALUES (?, ?), (?, ?), (?, ?)"
    );
    // Values given are repeated with the row, not just the placeholders.
    assert_eq!(
        with("i#localstrings(keycode='ita',value)*2", &RenderStyle::default()),
        "INSERT INTO LOCALSTRINGS (KEYCODE, VALUE) VALUES ('ita', ?), ('ita', ?)"
    );
}

#[test]
fn identifiers_can_be_re_cased_when_a_host_insists() {
    let upper = RenderStyle { identifiers: Case::Upper, ..RenderStyle::default() };
    let statement = expand("s#localstrings(keycode)", &lowercase_schema()).expect("expands");
    assert_eq!(render(&statement, &upper), "SELECT KEYCODE FROM LOCALSTRINGS");
}

#[test]
fn a_composite_join_renders_every_condition() {
    let schema = SchemaView::new(vec![
        TableMeta::new(
            "RIGHE",
            vec![ColumnMeta::new("ANNO", ValueKind::Number), ColumnMeta::new("NUMERO", ValueKind::Number)],
        )
        .with_foreign_keys(vec![ForeignKeyMeta {
            columns: vec!["ANNO".into(), "NUMERO".into()],
            referenced_table: "TESTATE".into(),
            referenced_columns: vec!["ANNO".into(), "NUM".into()],
        }]),
        TableMeta::new(
            "TESTATE",
            vec![ColumnMeta::new("ANNO", ValueKind::Number), ColumnMeta::new("NUM", ValueKind::Number)],
        ),
    ]);
    let statement = expand("s#righe>testate", &schema).expect("expands");
    assert_eq!(
        render(&statement, &RenderStyle::default()),
        "SELECT * FROM RIGHE R JOIN TESTATE T ON R.ANNO = T.ANNO AND R.NUMERO = T.NUM"
    );
}

#[test]
fn several_conditions_are_joined_with_and() {
    assert_eq!(
        with("s#ordini[quantita=1,codice='a']", &RenderStyle::default()),
        "SELECT * FROM ORDINI WHERE QUANTITA = 1 AND CODICE = 'a'"
    );
}

#[test]
fn a_column_is_qualified_only_when_there_is_something_to_qualify_against() {
    // One table: no alias, no prefix, and a line that reads like something a
    // person would have typed.
    assert_eq!(
        with("s#ordini(codice,quantita)", &RenderStyle::default()),
        "SELECT CODICE, QUANTITA FROM ORDINI"
    );
    // Two: everything qualified, so no name can be ambiguous in the output either.
    assert_eq!(
        with("s#ordini>prodotti(codice,nome)", &RenderStyle::default()),
        "SELECT O.CODICE, P.NOME FROM ORDINI O JOIN PRODOTTI P ON O.ID_PRODOTTO = P.ID"
    );
}
