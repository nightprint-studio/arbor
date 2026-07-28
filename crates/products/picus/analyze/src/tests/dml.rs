//! DML001 / DML002.

use crate::rule::RuleId;
use crate::testing::Fixture;
use crate::tests::open_of;

#[test]
fn a_delete_with_no_where_is_reported() {
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "DELETE FROM PARAMETRI;\nINSERT INTO PARAMETRI (COD) VALUES ('X');",
    )]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Dml001);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].line, Some(1));
    // The message has to name the way out, or the only way out is to stop
    // reading the report.
    assert!(findings[0].consequence.contains("picus: ignore DML001"));
}

#[test]
fn a_delete_that_says_what_it_deletes_is_clean() {
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "DELETE FROM PARAMETRI WHERE COD = 'SOGLIA_SCONTO';",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dml001).is_empty());
}

#[test]
fn truncate_is_not_a_delete() {
    // It says what it does in its own name, and nobody writes it by accident.
    let repo = Fixture::build(&[("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", "TRUNCATE TABLE PARAMETRI;")]);
    assert!(open_of(&repo.report(), RuleId::Dml001).is_empty());
}

#[test]
fn a_delete_inside_a_block_is_still_found() {
    // The reason the walker descends: in a real upgrade script everything is
    // three blocks deep.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "BEGIN\n  IF 1 = 1 THEN\n    DELETE FROM PARAMETRI;\n  END IF;\nEND;",
    )]);
    assert_eq!(open_of(&repo.report(), RuleId::Dml001).len(), 1);
}

#[test]
fn an_update_with_no_where_is_reported_too() {
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "UPDATE PARAMETRI SET VALORE = 20;",
    )]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Dml001);
    assert_eq!(findings.len(), 1);
    // The message is about the statement it fired on, not a sentence that covers
    // both: an UPDATE does not empty anything, it rewrites everything.
    assert!(findings[0].title.starts_with("UPDATE"), "{}", findings[0].title);
    assert!(findings[0].consequence.contains("VALORE"), "{}", findings[0].consequence);
    assert!(findings[0].consequence.contains("picus: ignore DML001"));
}

#[test]
fn an_update_that_says_which_rows_it_touches_is_clean() {
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "UPDATE PARAMETRI SET VALORE = 20 WHERE COD = 'SOGLIA_SCONTO';",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dml001).is_empty());
}

#[test]
fn the_closing_version_bump_is_not_a_mass_update() {
    // `UPDATE VERSIONE_DB SET VERSIONE = …` has no WHERE and never will — the
    // table holds one row — and writing it is exactly what VER002 requires. A
    // finding here would land on every correctly written update script there is.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO PARAMETRI (COD) VALUES ('X');\nUPDATE VERSIONE_DB SET VERSIONE = '4.13';",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dml001).is_empty());
}

#[test]
fn a_mass_update_on_the_version_table_of_a_project_that_has_none_is_still_reported() {
    // The exemption is the project's declared version table, not the name
    // `VERSIONE_DB`. A project that switched version guards off has no table to
    // exempt, and an unguarded UPDATE there is an unguarded UPDATE.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "UPDATE VERSIONE_DB SET VERSIONE = '4.13';",
    )])
    .configured(|config| config.version_table.table = String::new());
    assert_eq!(open_of(&repo.report(), RuleId::Dml001).len(), 1);
}

#[test]
fn an_insert_with_no_column_list_is_reported() {
    let repo = Fixture::build(&[(
        "POSTGRES/INIZIALIZZAZIONE/03_clienti.sql",
        "insert into clienti values ('X', 'Rossi');",
    )]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Dml002);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].fix_label.as_deref(), Some("Spell out the columns"));
    assert!(findings[0].consequence.contains("shifts one place"), "{}", findings[0].consequence);
}

#[test]
fn an_insert_that_names_its_columns_is_clean() {
    let repo = Fixture::build(&[(
        "POSTGRES/INIZIALIZZAZIONE/03_clienti.sql",
        "insert into clienti (cod, nome) values ('X', 'Rossi');",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dml002).is_empty());
}

#[test]
fn an_insert_from_a_query_without_a_column_list_is_the_same_hazard() {
    let repo = Fixture::build(&[(
        "POSTGRES/INIZIALIZZAZIONE/03_clienti.sql",
        "insert into clienti select cod, nome from clienti_vecchi;",
    )]);
    assert_eq!(open_of(&repo.report(), RuleId::Dml002).len(), 1);
}
