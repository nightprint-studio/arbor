//! Matching rows, and comparing the cells in them.

use crate::error::{DiffError, Side};
use crate::rows::{compare_rows, RowCompareOptions, RowKey, RowSet};
use crate::value::DiffValue;

fn set(columns: &[&str], rows: Vec<Vec<DiffValue>>) -> RowSet {
    RowSet::new(columns.iter().map(|c| c.to_string()).collect(), rows)
}

fn keyed(key: &[&str]) -> RowCompareOptions {
    RowCompareOptions {
        key: key.iter().map(|k| k.to_string()).collect(),
        ..RowCompareOptions::default()
    }
}

#[test]
fn rows_are_paired_by_key_wherever_they_sit_in_the_read() {
    let a = set(
        &["code", "label"],
        vec![
            vec!["A".into(), "first".into()],
            vec!["B".into(), "second".into()],
            vec!["C".into(), "third".into()],
        ],
    );
    // Same rows, different order, one changed, one gone, one new.
    let b = set(
        &["code", "label"],
        vec![
            vec!["C".into(), "third".into()],
            vec!["A".into(), "FIRST".into()],
            vec!["D".into(), "fourth".into()],
        ],
    );

    let out = compare_rows("widgets", &a, &b, &keyed(&["code"])).expect("comparable");
    assert!(out.keyed);
    assert_eq!(out.matched, 1, "C is the same row in both");
    assert_eq!(out.changed_total, 1);
    assert_eq!(out.changed[0].key, RowKey::Values(vec!["A".into()]));
    assert_eq!(out.changed[0].cells.len(), 1);
    assert_eq!(out.changed[0].cells[0].column, "label");
    assert_eq!(out.only_in_a_total, 1);
    assert_eq!(out.only_in_b_total, 1);
    assert_eq!(out.only_in_a[0].key, RowKey::Values(vec!["B".into()]));
    assert_eq!(out.only_in_b[0].key, RowKey::Values(vec!["D".into()]));
    assert!(out.has_differences());
}

#[test]
fn a_composite_key_matches_on_all_of_its_columns() {
    let a = set(
        &["area", "code", "amount"],
        vec![
            vec!["north".into(), "A".into(), 1i64.into()],
            vec!["south".into(), "A".into(), 2i64.into()],
        ],
    );
    let b = set(
        &["area", "code", "amount"],
        vec![
            vec!["south".into(), "A".into(), 2i64.into()],
            vec!["north".into(), "A".into(), 9i64.into()],
        ],
    );

    let out = compare_rows("widgets", &a, &b, &keyed(&["area", "code"])).expect("comparable");
    assert_eq!(out.matched, 1);
    assert_eq!(out.changed_total, 1);
    assert_eq!(out.changed[0].key, RowKey::Values(vec!["north".into(), "A".into()]));
    assert_eq!(out.only_in_a_total, 0);
    assert_eq!(out.only_in_b_total, 0);
    // Matching on `code` alone would have made these one row and reported a
    // change that does not exist.
}

#[test]
fn with_no_key_the_comparison_is_positional_and_says_so() {
    let a = set(&["label"], vec![vec!["first".into()], vec!["second".into()]]);
    let b = set(&["label"], vec![vec!["first".into()], vec!["changed".into()], vec!["extra".into()]]);

    let out = compare_rows("query", &a, &b, &RowCompareOptions::default()).expect("comparable");
    assert!(!out.keyed, "a positional answer must not read as a keyed one");
    assert_eq!(out.matched, 1);
    assert_eq!(out.changed_total, 1);
    assert_eq!(out.changed[0].key, RowKey::Position(1));
    assert_eq!(out.only_in_b_total, 1);
    assert_eq!(out.only_in_b[0].key, RowKey::Position(2));
}

#[test]
fn a_number_a_float_and_a_string_are_three_different_values() {
    let a = set(&["code", "amount"], vec![vec!["A".into(), DiffValue::Int(1)]]);
    let as_float = set(&["code", "amount"], vec![vec!["A".into(), DiffValue::Float(1.0)]]);
    let as_text = set(&["code", "amount"], vec![vec!["A".into(), DiffValue::Text("1".into())]]);

    let options = keyed(&["code"]);
    assert_eq!(compare_rows("t", &a, &as_float, &options).unwrap().changed_total, 1);
    assert_eq!(compare_rows("t", &a, &as_text, &options).unwrap().changed_total, 1);
    assert_eq!(compare_rows("t", &as_float, &as_text, &options).unwrap().changed_total, 1);
}

#[test]
fn null_is_not_the_empty_string_and_two_nulls_agree() {
    let null = set(&["code", "label"], vec![vec!["A".into(), DiffValue::Null]]);
    let empty = set(&["code", "label"], vec![vec!["A".into(), DiffValue::Text(String::new())]]);

    let options = keyed(&["code"]);
    assert_eq!(compare_rows("t", &null, &empty, &options).unwrap().changed_total, 1);
    assert_eq!(compare_rows("t", &null, &null, &options).unwrap().matched, 1);
}

#[test]
fn an_ignored_column_takes_no_part_in_the_comparison() {
    let a = set(&["code", "updated_at"], vec![vec!["A".into(), "monday".into()]]);
    let b = set(&["code", "updated_at"], vec![vec!["A".into(), "friday".into()]]);

    let mut options = keyed(&["code"]);
    options.ignore_columns = vec!["updated_*".to_string()];

    let out = compare_rows("widgets", &a, &b, &options).expect("comparable");
    assert_eq!(out.matched, 1);
    assert!(!out.has_differences());
    assert_eq!(out.compared_columns, vec!["code"]);
}

#[test]
fn a_column_only_one_side_has_is_reported_without_stopping_the_comparison() {
    let a = set(&["code", "label"], vec![vec!["A".into(), "first".into()]]);
    let b = set(&["code", "note"], vec![vec!["A".into(), "first".into()]]);

    let out = compare_rows("widgets", &a, &b, &keyed(&["code"])).expect("comparable");
    assert_eq!(out.compared_columns, vec!["code"]);
    assert_eq!(out.columns_only_in_a, vec!["label"]);
    assert_eq!(out.columns_only_in_b, vec!["note"]);
    assert!(out.has_differences(), "a missing column is a difference in itself");
}

#[test]
fn the_cap_limits_what_is_listed_and_never_what_is_counted() {
    let rows_a: Vec<Vec<DiffValue>> =
        (0..10).map(|i| vec![DiffValue::Int(i), "a".into()]).collect();
    let rows_b: Vec<Vec<DiffValue>> =
        (0..10).map(|i| vec![DiffValue::Int(i), "b".into()]).collect();
    let a = set(&["id", "label"], rows_a);
    let b = set(&["id", "label"], rows_b);

    let mut options = keyed(&["id"]);
    options.max_differences = Some(3);

    let out = compare_rows("widgets", &a, &b, &options).expect("comparable");
    assert_eq!(out.changed.len(), 3, "listed");
    assert_eq!(out.changed_total, 10, "counted");
    assert!(out.truncated);
}

#[test]
fn a_key_that_is_not_unique_is_reported_rather_than_silently_dropped() {
    let a = set(
        &["code", "label"],
        vec![vec!["A".into(), "one".into()], vec!["A".into(), "two".into()]],
    );
    let b = set(&["code", "label"], vec![vec!["A".into(), "one".into()]]);

    let out = compare_rows("widgets", &a, &b, &keyed(&["code"])).expect("comparable");
    assert_eq!(out.duplicate_keys_a, vec![RowKey::Values(vec!["A".into()])]);
    assert!(out.duplicate_keys_b.is_empty());
    // The surplus row is not thrown away.
    assert_eq!(out.matched, 1);
    assert_eq!(out.only_in_a_total, 1);
}

#[test]
fn a_key_column_that_does_not_exist_is_an_error_and_not_a_guess() {
    let a = set(&["code"], vec![vec!["A".into()]]);
    let b = set(&["label"], vec![vec!["A".into()]]);

    let err = compare_rows("widgets", &a, &b, &keyed(&["code"])).unwrap_err();
    assert_eq!(
        err,
        DiffError::MissingKeyColumn {
            label: "widgets".into(),
            column: "code".into(),
            side: Side::B,
        }
    );
}

#[test]
fn a_row_that_does_not_fit_its_header_stops_the_comparison() {
    let a = set(&["code", "label"], vec![vec!["A".into()]]);
    let b = set(&["code", "label"], vec![vec!["A".into(), "x".into()]]);

    let err = compare_rows("widgets", &a, &b, &keyed(&["code"])).unwrap_err();
    assert!(matches!(err, DiffError::RowWidthMismatch { side: Side::A, row: 0, .. }));
}

#[test]
fn column_names_fold_when_the_run_says_so() {
    let a = set(&["CODE", "LABEL"], vec![vec!["A".into(), "first".into()]]);
    let b = set(&["code", "label"], vec![vec!["A".into(), "first".into()]]);

    let folding = keyed(&["code"]);
    let out = compare_rows("widgets", &a, &b, &folding).expect("comparable");
    assert_eq!(out.matched, 1);
    assert!(out.columns_only_in_a.is_empty());

    let strict = RowCompareOptions { case_insensitive_names: false, ..keyed(&["code"]) };
    let err = compare_rows("widgets", &a, &b, &strict).unwrap_err();
    assert!(matches!(err, DiffError::MissingKeyColumn { side: Side::A, .. }));
}
