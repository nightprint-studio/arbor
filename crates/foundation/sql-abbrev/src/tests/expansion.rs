//! The worked examples, and the resolution behind them.

use super::fixture::{schema, sql};
use crate::prelude::*;

#[test]
fn the_five_worked_examples() {
    assert_eq!(sql("s#localstrings"), "SELECT * FROM LOCALSTRINGS");
    assert_eq!(
        sql("s#localstrings(keycode,value)[keycode='ita']"),
        "SELECT KEYCODE, VALUE FROM LOCALSTRINGS WHERE KEYCODE = 'ita'"
    );
    assert_eq!(
        sql("i#localstrings(keycode,value)"),
        "INSERT INTO LOCALSTRINGS (KEYCODE, VALUE) VALUES (?, ?)"
    );
    assert_eq!(
        sql("u#localstrings(value='x')[keycode='ita']"),
        "UPDATE LOCALSTRINGS SET VALUE = 'x' WHERE KEYCODE = 'ita'"
    );
    assert_eq!(
        sql("d#localstrings[keycode='ita']"),
        "DELETE FROM LOCALSTRINGS WHERE KEYCODE = 'ita'"
    );
}

#[test]
fn the_user_types_lower_case_and_gets_the_schemas_spelling_back() {
    // The whole reason a name is looked up rather than passed through.
    assert_eq!(sql("s#LoCaLsTrInGs(KeyCode)"), "SELECT KEYCODE FROM LOCALSTRINGS");
}

#[test]
fn a_join_reads_its_condition_from_the_foreign_key() {
    assert_eq!(
        sql("s#ordini>prodotti[nome='vite']"),
        "SELECT * FROM ORDINI O JOIN PRODOTTI P ON O.ID_PRODOTTO = P.ID WHERE P.NOME = 'vite'"
    );
}

#[test]
fn an_ambiguous_join_is_resolved_by_naming_a_column() {
    assert_eq!(
        sql("s#ordini>clienti:id_cliente_fatturazione[nome='rossi']"),
        "SELECT * FROM ORDINI O JOIN CLIENTI C ON O.ID_CLIENTE_FATTURAZIONE = C.ID \
         WHERE C.NOME = 'rossi'"
    );
}

#[test]
fn a_chain_is_joined_link_by_link_with_stable_aliases() {
    let statement = expand("s#ordini>clienti:id_cliente>ordini:id_cliente", &schema()).expect("expands");
    let Statement::Select { tables, joins, .. } = &statement else { panic!("{statement:?}") };
    // First letter, deduplicated — and the third table is ORDINI again, so `O2`.
    assert_eq!(
        tables.iter().map(|t| t.alias.clone().unwrap()).collect::<Vec<_>>(),
        vec!["O", "C", "O2"]
    );
    // Each link attaches to the one before it, never to the root by default.
    assert_eq!(joins.iter().map(|j| (j.to, j.table)).collect::<Vec<_>>(), vec![(0, 1), (1, 2)]);
}

#[test]
fn a_single_table_statement_gets_no_alias() {
    let statement = expand("s#ordini(codice)", &schema()).expect("expands");
    let Statement::Select { tables, columns, .. } = &statement else { panic!() };
    assert_eq!(tables[0].alias, None, "an alias nobody needs is noise");
    assert_eq!(columns[0].alias, None);
}

#[test]
fn quoting_follows_the_columns_type() {
    // The single most valuable thing the feature does, in four abbreviations.
    assert!(sql("s#ordini[quantita=15]").ends_with("WHERE QUANTITA = 15"), "a number stays bare");
    assert!(sql("s#ordini[codice=007]").ends_with("WHERE CODICE = '007'"), "a code keeps its zeros");
    assert!(sql("s#ordini[evaso=true]").ends_with("WHERE EVASO = true"));
    assert!(sql("s#ordini[allegato=1]").ends_with("WHERE ALLEGATO = '1'"), "unclassified quotes");
    assert!(sql("s#ordini[data=current_timestamp]").ends_with("WHERE DATA = current_timestamp"));
}

#[test]
fn a_value_the_user_quoted_is_left_alone() {
    // Explicit intent beats the column's type: `'15'` in a numeric column is a
    // string the user meant to write.
    assert!(sql("s#ordini[quantita='15']").ends_with("WHERE QUANTITA = '15'"));
    assert!(sql("s#ordini[codice='ita']").ends_with("WHERE CODICE = 'ita'"));
}

#[test]
fn a_quote_inside_a_value_cannot_end_the_literal() {
    assert!(
        sql("s#clienti[nome='d''annunzio']").ends_with("WHERE NOME = 'd''annunzio'"),
        "{}",
        sql("s#clienti[nome='d''annunzio']")
    );
    // The hostile shape stays one literal.
    let injected = sql("s#clienti[nome='x''; DROP TABLE T; --']");
    assert!(injected.ends_with("WHERE NOME = 'x''; DROP TABLE T; --'"), "{injected}");
}

#[test]
fn a_multiplier_asks_for_rows_not_statements() {
    // The seed-data case: the crate reports the count and the host decides how to
    // spell three rows — one statement or three.
    let statement = expand("i#localstrings(keycode,value)*3", &schema()).expect("expands");
    let Statement::Insert { rows, columns, .. } = &statement else { panic!() };
    assert_eq!(*rows, 3);
    assert_eq!(columns.len(), 2);
}

#[test]
fn an_insert_with_no_column_list_takes_the_whole_table() {
    let statement = expand("i#localstrings", &schema()).expect("expands");
    let Statement::Insert { columns, rows, .. } = &statement else { panic!() };
    assert_eq!(
        columns.iter().map(|c| c.column.name.as_str()).collect::<Vec<_>>(),
        vec!["KEYCODE", "VALUE"]
    );
    assert!(columns.iter().all(|c| c.value.is_none()));
    assert_eq!(*rows, 1);
}

#[test]
fn an_insert_takes_values_and_may_mix_them_with_placeholders() {
    // Anyone who has just written `u#t(a='x')` writes this within the minute.
    assert_eq!(
        sql("i#ordini(codice='ab',quantita=15)"),
        "INSERT INTO ORDINI (CODICE, QUANTITA) VALUES ('ab', 15)"
    );
    // Mixed, and deliberately allowed: `QUANTITA` is left for the host.
    assert_eq!(
        sql("i#ordini(codice='ab',quantita)"),
        "INSERT INTO ORDINI (CODICE, QUANTITA) VALUES ('ab', ?)"
    );
}

#[test]
fn an_insert_value_is_quoted_by_its_columns_type_like_any_other() {
    assert!(sql("i#ordini(codice=007)").ends_with("VALUES ('007')"));
    assert!(sql("i#ordini(quantita=15)").ends_with("VALUES (15)"));
    assert!(sql("i#ordini(data=sysdate)").ends_with("VALUES (sysdate)"));
}

#[test]
fn every_row_of_a_multiplied_insert_carries_the_same_values() {
    // Surprising until you have wanted it: it is what a seed-data user types
    // before editing the three rows apart.
    assert_eq!(
        sql("i#ordini(codice='ab',quantita)*3"),
        "INSERT INTO ORDINI (CODICE, QUANTITA) VALUES ('ab', ?), ('ab', ?), ('ab', ?)"
    );
}

#[test]
fn an_update_may_be_keyed_by_any_comparison() {
    // Not the language's business to insist on equality — the host whose model
    // cannot express a range refuses it where it maps this.
    assert_eq!(
        sql("u#ordini(codice='x')[quantita>5]"),
        "UPDATE ORDINI SET CODICE = 'x' WHERE QUANTITA > 5"
    );
    assert!(sql("u#ordini(codice='x')[codice~'a%']").ends_with("WHERE CODICE LIKE 'a%'"));
}

#[test]
fn a_function_call_survives_inside_a_column_list() {
    // `)` would otherwise close the list on the most obvious default anybody
    // could type, which reads as the feature being broken.
    assert_eq!(sql("i#ordini(data=now())"), "INSERT INTO ORDINI (DATA) VALUES (now())");
    assert_eq!(
        sql("u#ordini(data=now())[quantita=1]"),
        "UPDATE ORDINI SET DATA = now() WHERE QUANTITA = 1"
    );
    // Nested, and with a comma and a space inside the parentheses.
    let statement = expand("u#ordini(codice=coalesce(a, upper(b)))[quantita=1]", &schema())
        .expect("expands");
    let Statement::Update { assignments, .. } = &statement else { panic!() };
    assert_eq!(assignments[0].value.text(), "coalesce(a, upper(b))");
}

#[test]
fn a_column_can_be_qualified_by_its_table_or_by_its_alias() {
    let by_table = sql("s#ordini>clienti:id_cliente(clienti.nome)");
    let by_alias = sql("s#ordini>clienti:id_cliente(c.nome)");
    assert!(by_table.starts_with("SELECT C.NOME FROM ORDINI O JOIN CLIENTI C"), "{by_table}");
    assert_eq!(by_table, by_alias);
}

#[test]
fn every_comparison_survives_the_round_trip() {
    for (typed, rendered) in
        [("=", "="), ("!=", "<>"), ("<>", "<>"), ("<", "<"), ("<=", "<="), (">", ">"), (">=", ">="), ("~", "LIKE")]
    {
        let out = sql(&format!("s#ordini[quantita{typed}5]"));
        assert!(out.ends_with(&format!("WHERE QUANTITA {rendered} 5")), "{typed} → {out}");
    }
}

#[test]
fn whitespace_is_tolerated_everywhere_it_could_be_typed() {
    assert_eq!(
        sql("s # localstrings ( keycode , value ) [ keycode = 'ita' ]"),
        sql("s#localstrings(keycode,value)[keycode='ita']")
    );
}

#[test]
fn the_verb_may_be_spelled_out() {
    assert_eq!(sql("select#localstrings"), sql("s#localstrings"));
    assert_eq!(sql("DELETE#localstrings[keycode='ita']"), sql("d#localstrings[keycode='ita']"));
}
