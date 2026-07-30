//! The glob, which decides what gets compared at all.

use crate::names::{fold_all, glob_match, matches_any, missing_from};

#[test]
fn a_star_stands_for_any_run_including_none() {
    assert!(glob_match("tmp_*", "tmp_orders"));
    assert!(glob_match("tmp_*", "tmp_"));
    assert!(!glob_match("tmp_*", "tmp"));
    assert!(glob_match("*_bak", "orders_bak"));
    assert!(glob_match("*", "anything"));
    assert!(glob_match("*", ""));
}

#[test]
fn a_question_mark_stands_for_exactly_one_character() {
    assert!(glob_match("log_?", "log_1"));
    assert!(glob_match("log_?", "log_a"));
    assert!(!glob_match("log_?", "log_"));
    assert!(!glob_match("log_?", "log_12"));
}

#[test]
fn everything_else_is_literal() {
    assert!(glob_match("orders", "orders"));
    assert!(!glob_match("orders", "orders_2"));
    assert!(!glob_match("orders", "Orders"), "the glob itself does not fold");
    // No character classes, no braces: a pattern that looks like one matches the
    // literal text, which is the behaviour a template written years ago keeps.
    assert!(!glob_match("[ab]_x", "a_x"));
    assert!(glob_match("[ab]_x", "[ab]_x"));
}

#[test]
fn several_stars_still_terminate_and_still_answer_correctly() {
    // The backtracking case: a pattern that keeps re-anchoring against a value
    // that nearly matches.
    assert!(glob_match("*a*a*a*b", "aaaaaaaaaaaaaaaaaaaaaaaaab"));
    assert!(!glob_match("*a*a*a*b", "aaaaaaaaaaaaaaaaaaaaaaaaac"));
    assert!(glob_match("a*b*c", "axxbyyc"));
    assert!(!glob_match("a*b*c", "axxcyyb"));
}

#[test]
fn folding_is_the_callers_decision_and_it_applies_to_the_pattern_too() {
    let patterns = vec!["TMP_*".to_string()];
    assert!(matches_any(&patterns, "tmp_orders", true));
    assert!(!matches_any(&patterns, "tmp_orders", false));
    assert!(matches_any(&patterns, "TMP_ORDERS", false));
}

#[test]
fn an_empty_pattern_list_matches_nothing() {
    // Load-bearing: `NameFilter` reads this as "include nothing" and "exclude
    // nothing", and the two only stay distinguishable because this is not "all".
    assert!(!matches_any(&[], "orders", true));
}

#[test]
fn folding_a_list_leaves_the_originals_alone() {
    let names = vec!["Code".to_string(), "LABEL".to_string()];
    assert_eq!(fold_all(&names, true), vec!["code", "label"]);
    assert_eq!(fold_all(&names, false), names);
}

#[test]
fn missing_from_keeps_the_left_hand_order_and_spelling() {
    let left = vec!["Code".to_string(), "Label".to_string(), "Amount".to_string()];
    let right = vec!["LABEL".to_string()];
    assert_eq!(missing_from(&left, &right, true), vec!["Code", "Amount"]);
    // Without folding, `Label` and `LABEL` are two different columns.
    assert_eq!(missing_from(&left, &right, false), vec!["Code", "Label", "Amount"]);
}
