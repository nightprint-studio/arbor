//! DIA001 — the other dialect's syntax in a script that will be run against
//! this one.

use crate::rule::RuleId;
use crate::testing::Fixture;
use crate::tests::open_of;

#[test]
fn oracle_syntax_in_a_postgresql_script_is_blocking() {
    let repo = Fixture::build(&[(
        "POSTGRES/INIZIALIZZAZIONE/02_parametri.sql",
        "insert into parametri (cod, descr) values ('X', nvl(descr, 'n/d'));",
    )]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Dia001);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, crate::rule::Severity::Blocking);
    assert!(findings[0].consequence.contains("COALESCE"), "{}", findings[0].consequence);
}

#[test]
fn an_oracle_sequence_is_not_reported_as_postgresql_syntax() {
    // `picus-parse` matches builtins on the last component of a dotted name, so
    // Oracle's own `SEQ.NEXTVAL` looks like PostgreSQL's `nextval()`. Every
    // Oracle script that uses a sequence would carry a false blocking finding.
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "INSERT INTO PARAMETRI (ID, COD) VALUES (SEQ_PARAMETRI.NEXTVAL, 'X');",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dia001).is_empty());
}

#[test]
fn one_statement_using_the_same_foreign_construct_four_times_is_one_finding() {
    let repo = Fixture::build(&[(
        "POSTGRES/INIZIALIZZAZIONE/02_parametri.sql",
        "insert into parametri (a, b, c, d) values (nvl(a,1), nvl(b,2), nvl(c,3), nvl(d,4));",
    )]);
    let report = repo.report();
    assert_eq!(open_of(&report, RuleId::Dia001).len(), 1, "one thing to rewrite, one finding");
}

#[test]
fn oracle_constructs_in_an_oracle_script_are_not_foreign() {
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "INSERT INTO PARAMETRI (COD, DATA_AGG) VALUES (NVL('X', 'Y'), SYSDATE);",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dia001).is_empty());
}

#[test]
fn a_folder_nobody_could_identify_is_not_checked_for_dialect() {
    let repo = Fixture::build(&[("COMMON/misc.sql", "insert into t (a) values (nvl(a, 1));")]);
    assert!(open_of(&repo.report(), RuleId::Dia001).is_empty());
}

#[test]
fn the_rule_that_moved_here_kept_its_severity_and_its_message() {
    // `DIA001` was `CONS003` until the catalogue was realigned with the
    // documentation. The id changed; what it says and how loud it is did not,
    // because a script with foreign syntax is exactly as broken as it was.
    let repo = Fixture::build(&[(
        "POSTGRES/INIZIALIZZAZIONE/02_parametri.sql",
        "insert into parametri (cod, descr) values ('X', nvl(descr, 'n/d'));",
    )]);
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons003).is_empty(), "CONS003 is a datum rule now");
    assert_eq!(open_of(&report, RuleId::Dia001).len(), 1);
}
