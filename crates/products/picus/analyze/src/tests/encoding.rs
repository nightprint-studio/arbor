//! ENC001 / ENC002.

use arbor_fs::prelude::encoding::EncodingSource;

use crate::rule::{RuleId, Severity};
use crate::testing::Fixture;
use crate::tests::open_of;

const ACCENTED: &str = "-- soglia già applicata\nINSERT INTO PARAMETRI (COD) VALUES ('X');";

#[test]
fn a_file_that_drifted_from_its_folders_encoding_is_reported_with_a_way_back() {
    let repo = Fixture::build(&[("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", ACCENTED)])
        .encoded("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", "UTF-8", "windows-1252");
    let report = repo.report();
    let findings = open_of(&report, RuleId::Enc001);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Review);
    assert_eq!(findings[0].fix_label.as_deref(), Some("Convert back to windows-1252"));
}

#[test]
fn a_file_in_the_encoding_its_folder_expects_is_clean() {
    let repo = Fixture::build(&[("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", ACCENTED)]);
    assert!(open_of(&repo.report(), RuleId::Enc001).is_empty());
}

#[test]
fn an_encoding_the_user_pinned_is_a_decision_not_a_drift() {
    // Reporting somebody's own override back at them every run is how a report
    // earns a permanent "ignore all".
    let repo = Fixture::build(&[("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", ACCENTED)])
        .encoded("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", "UTF-8", "windows-1252")
        .encoding_source("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", EncodingSource::Forced);
    assert!(open_of(&repo.report(), RuleId::Enc001).is_empty());
}

#[test]
fn a_character_the_folders_encoding_cannot_hold_is_blocking() {
    // This is the guard on ENC001's corrective action: converting this file to
    // windows-1252 would replace the character with a question mark, and the
    // description it is in would install wrong with nothing to show for it.
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "-- fornitore\nINSERT INTO PARAMETRI (COD, DESCR) VALUES ('X', '中文');",
    )]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Enc002);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Blocking);
    assert_eq!(findings[0].line, Some(2));
    assert!(findings[0].consequence.contains("question mark"), "{}", findings[0].consequence);
}

#[test]
fn an_accented_character_the_encoding_does_hold_is_not_a_finding() {
    // `à` has a byte in windows-1252. If this fired, every Italian repository
    // Picus was built for would be one long blocking list.
    let repo = Fixture::build(&[("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", ACCENTED)]);
    assert!(open_of(&repo.report(), RuleId::Enc002).is_empty());
}

#[test]
fn a_folder_that_expects_utf8_can_hold_anything() {
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "INSERT INTO PARAMETRI (COD) VALUES ('中文');",
    )])
    .encoded("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", "UTF-8", "UTF-8");
    let report = repo.report();
    assert!(open_of(&report, RuleId::Enc002).is_empty());
    assert!(open_of(&report, RuleId::Enc001).is_empty());
}

#[test]
fn the_two_encoding_rules_fire_together_when_the_fix_would_destroy_data() {
    // The pairing that gives ENC002 its reason to exist: the file has drifted
    // AND it now contains something the folder's encoding cannot represent, so
    // the offered conversion is not safe.
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "INSERT INTO PARAMETRI (COD) VALUES ('中文');",
    )])
    .encoded("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", "UTF-8", "windows-1252");
    let report = repo.report();
    assert_eq!(open_of(&report, RuleId::Enc001).len(), 1);
    assert_eq!(open_of(&report, RuleId::Enc002).len(), 1);
}
