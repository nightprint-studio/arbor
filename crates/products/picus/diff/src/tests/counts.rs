//! Counts and the thresholds that make them mean something.

use crate::change::Severity;
use crate::config::{CountCheck, NameFilter};
use crate::counts::{compare_counts, TableCount};

fn check() -> CountCheck {
    CountCheck {
        enabled: true,
        warning_threshold_percent: 10.0,
        error_threshold_percent: 50.0,
        ..CountCheck::default()
    }
}

fn severity_of(a: i64, b: i64) -> Severity {
    let out = compare_counts(&[TableCount::new("orders", a)], &[TableCount::new("orders", b)], &check(), true);
    out[0].severity
}

#[test]
fn a_count_inside_the_tolerance_is_still_a_difference_just_not_a_loud_one() {
    let out = compare_counts(
        &[TableCount::new("orders", 1_000)],
        &[TableCount::new("orders", 1_050)],
        &check(),
        true,
    );
    assert_eq!(out[0].delta, Some(50));
    assert_eq!(out[0].delta_percent, Some(5.0));
    assert_eq!(out[0].severity, Severity::Ok);
    assert!(out[0].differs(), "5% apart is not 'the same'");
}

#[test]
fn the_two_thresholds_are_where_the_report_changes_its_tone() {
    assert_eq!(severity_of(1_000, 1_000), Severity::Ok);
    assert_eq!(severity_of(1_000, 1_090), Severity::Ok);
    assert_eq!(severity_of(1_000, 1_100), Severity::Warning, "exactly at the threshold");
    assert_eq!(severity_of(1_000, 1_499), Severity::Warning);
    assert_eq!(severity_of(1_000, 1_500), Severity::Error);
    // Direction does not change the severity: losing half the rows is as bad as
    // doubling them, and worse in practice.
    assert_eq!(severity_of(1_000, 500), Severity::Error);
}

#[test]
fn growing_away_from_zero_has_no_percentage_and_is_not_pretended_to() {
    let out = compare_counts(
        &[TableCount::new("orders", 0)],
        &[TableCount::new("orders", 7)],
        &check(),
        true,
    );
    assert_eq!(out[0].delta, Some(7));
    assert_eq!(out[0].delta_percent, None, "no infinity crosses the wire");
    assert_eq!(out[0].severity, Severity::Error);

    // Two empty tables agree, and that is 0%.
    let both_empty = compare_counts(
        &[TableCount::new("orders", 0)],
        &[TableCount::new("orders", 0)],
        &check(),
        true,
    );
    assert_eq!(both_empty[0].delta_percent, Some(0.0));
    assert_eq!(both_empty[0].severity, Severity::Ok);
    assert!(!both_empty[0].differs());
}

#[test]
fn a_table_that_could_not_be_counted_is_reported_rather_than_dropped() {
    let out = compare_counts(
        &[TableCount::new("orders", 10), TableCount::unknown("archive")],
        &[TableCount::new("orders", 10)],
        &check(),
        true,
    );
    assert_eq!(out.len(), 2, "the relation is in the report even with no number");

    let archive = out.iter().find(|c| c.table == "archive").expect("present");
    assert_eq!(archive.count_a, None);
    assert_eq!(archive.count_b, None);
    assert_eq!(archive.delta, None);
    assert_eq!(archive.severity, Severity::Warning);
    assert!(!archive.differs(), "two absences are not a difference in the data");
}

#[test]
fn a_relation_only_one_side_has_shows_up_with_one_number() {
    let out = compare_counts(
        &[TableCount::new("orders", 10)],
        &[TableCount::new("orders", 10), TableCount::new("added", 3)],
        &check(),
        true,
    );
    let added = out.iter().find(|c| c.table == "added").expect("present");
    assert_eq!(added.count_a, None);
    assert_eq!(added.count_b, Some(3));
    assert!(added.differs());
}

#[test]
fn the_filter_decides_which_counts_are_in_the_report() {
    let reference_only = CountCheck { filter: NameFilter::include(["ref_*"]), ..check() };
    let out = compare_counts(
        &[TableCount::new("ref_status", 4), TableCount::new("orders", 10)],
        &[TableCount::new("ref_status", 5), TableCount::new("orders", 99)],
        &reference_only,
        true,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].table, "ref_status");
}

#[test]
fn one_table_read_from_two_engines_is_one_row() {
    let out = compare_counts(
        &[TableCount::new("ORDERS", 10)],
        &[TableCount::new("orders", 10)],
        &check(),
        true,
    );
    assert_eq!(out.len(), 1);
    assert!(!out[0].differs());

    let strict = compare_counts(
        &[TableCount::new("ORDERS", 10)],
        &[TableCount::new("orders", 10)],
        &check(),
        false,
    );
    assert_eq!(strict.len(), 2, "without folding they are two relations");
}
