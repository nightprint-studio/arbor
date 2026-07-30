//! The verdict, and the wire shape the frontend reads.

use crate::report::{CheckKind, DiffReport, SkipReason, Verdict};
use crate::rows::{compare_rows, RowCompareOptions, RowSet};
use crate::schema::compare_schema;
use crate::tests::{column, snapshot, table};

#[test]
fn a_run_that_compared_everything_and_found_nothing_says_identical() {
    let a = snapshot(vec![table("orders", vec![column("id", "integer")])]);
    let b = snapshot(vec![table("orders", vec![column("id", "integer")])]);

    let mut report = DiffReport::new("production", "staging");
    report.schema = Some(compare_schema(&a, &b, &Default::default()));
    let report = report.finish();

    assert_eq!(report.verdict, Verdict::Identical);
    assert!(!report.has_differences());
    assert!(!report.is_partial());
}

#[test]
fn a_check_that_did_not_run_downgrades_the_verdict() {
    let a = snapshot(vec![table("orders", vec![column("id", "integer")])]);
    let b = snapshot(vec![table("orders", vec![column("id", "integer")])]);

    let mut report = DiffReport::new("production", "staging");
    report.schema = Some(compare_schema(&a, &b, &Default::default()));
    report.skip(
        CheckKind::Contents,
        SkipReason::Disabled,
        "contents are switched off in this template",
    );
    let report = report.finish();

    // The comparison found nothing, and the report still refuses to say
    // "identical" — nobody looked at the data.
    assert_eq!(report.verdict, Verdict::IdenticalWhereChecked);
    assert!(report.is_partial());
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].check, CheckKind::Contents);
}

#[test]
fn one_difference_anywhere_makes_the_verdict_different() {
    let a = snapshot(vec![table("orders", vec![column("id", "integer")])]);
    let b = snapshot(vec![table("orders", vec![column("id", "bigint")])]);

    let mut report = DiffReport::new("production", "staging");
    report.schema = Some(compare_schema(&a, &b, &Default::default()));
    report.skip(CheckKind::Contents, SkipReason::Disabled, "off");
    let report = report.finish();

    assert_eq!(report.verdict, Verdict::Different);
}

#[test]
fn a_relation_whose_rows_could_not_be_matched_is_named_in_the_report() {
    let mut report = DiffReport::new("production", "staging");
    report.skip_scope(
        CheckKind::Contents,
        "orders",
        SkipReason::NoKey,
        "no primary key in the catalogue and none declared in the template",
    );
    let report = report.finish();

    assert_eq!(report.verdict, Verdict::IdenticalWhereChecked);
    assert_eq!(report.skipped[0].scope.as_deref(), Some("orders"));
}

#[test]
fn the_row_comparison_lands_in_the_report_as_one_entry_per_relation() {
    let a = RowSet::new(vec!["code".into()], vec![vec!["A".into()]]);
    let b = RowSet::new(vec!["code".into()], vec![vec!["B".into()]]);
    let options = RowCompareOptions { key: vec!["code".into()], ..RowCompareOptions::default() };

    let mut report = DiffReport::new("production", "staging");
    report.rows.push(compare_rows("orders", &a, &b, &options).expect("comparable"));
    let report = report.finish();

    assert_eq!(report.verdict, Verdict::Different);
    assert_eq!(report.rows[0].label, "orders");
}

#[test]
fn the_wire_names_are_the_ones_the_frontend_reads() {
    let mut report = DiffReport::new("production", "staging");
    report.skip(CheckKind::Sequences, SkipReason::Unsupported, "the engine has no sequences");
    let report = report.finish();

    let json = serde_json::to_value(&report).expect("serialises");
    assert_eq!(json["labelA"], "production");
    assert_eq!(json["labelB"], "staging");
    assert_eq!(json["verdict"], "identicalWhereChecked");
    assert_eq!(json["skipped"][0]["check"], "sequences");
    assert_eq!(json["skipped"][0]["reason"], "unsupported");
    // A check that produced nothing is absent rather than an empty object, so the
    // frontend can tell "not run" from "ran and found nothing".
    assert!(json.get("schema").is_none());
    assert!(json["skipped"][0].get("scope").is_none());

    let back: crate::report::DiffReport =
        serde_json::from_value(json).expect("round-trips");
    assert_eq!(back, report);
}
