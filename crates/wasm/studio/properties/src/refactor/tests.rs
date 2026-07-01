//! §6 .properties F12/F13 tests — the SPECIAL hand-written refactor.
//!
//! Covers the two behavior-load-bearing divergences: key-scoped F12
//! rename (Key + Value scope) and the F13 `(empty)` string-coercion
//! sentinel + divergent preview.

use super::*;
use crate::line_model::{
    apply_rename_in_text, PropertiesBulkOp, PropertiesRenameScope, PropertiesRenameSite,
    PropertiesSetValue,
};
use arbor_studio_types::prelude::{BulkEditAction, BulkEditLiteral, BulkEditValueSource};
use serde_json::Value;

// ── F12 — key-scoped rename ─────────────────────────────────────────────

/// Key-scope rename (F12) renames the dotted KEY itself, and comments on
/// untouched lines survive.
#[test]
fn key_rename_renames_key_and_keeps_comments() {
    let src = "# database\ndb.url=postgres://localhost\n";
    let sites = vec![PropertiesRenameSite {
        field_path: vec!["db".into(), "url".into()],
        scope:      PropertiesRenameScope::Key,
    }];
    let out = apply_rename_in_text(src, &sites, "db.url", "db.uri").unwrap();
    assert!(out.contains("# database\n"), "comment survived: {out:?}");
    assert!(out.contains("db.uri=postgres://localhost\n"), "key renamed: {out:?}");
    assert!(!out.contains("db.url="), "old key gone: {out:?}");
}

/// Value-scope rename (F12) renames the RHS value of a `key=value` line
/// (a reference to the renamed identifier), leaving the key intact.
#[test]
fn value_rename_renames_rhs() {
    let src = "alias=db.url\nother=v\n";
    let sites = vec![PropertiesRenameSite {
        field_path: vec!["alias".into()],
        scope:      PropertiesRenameScope::Value,
    }];
    let out = apply_rename_in_text(src, &sites, "db.url", "db.uri").unwrap();
    assert!(out.contains("alias=db.uri\n"), "value renamed: {out:?}");
    assert!(out.contains("other=v\n"));
}

/// A Key-scope site that doesn't match `old_value` aborts the whole pass
/// (pre-flush validation).
#[test]
fn key_rename_mismatch_aborts() {
    let src = "db.url=x\n";
    let sites = vec![PropertiesRenameSite {
        field_path: vec!["db".into(), "url".into()],
        scope:      PropertiesRenameScope::Key,
    }];
    // old_value `nope` != flat key `db.url` → error.
    let res = apply_rename_in_text(src, &sites, "nope", "db.uri");
    assert!(res.is_err(), "mismatched old_value must abort");
}

// ── F13 — `(empty)` sentinel + string coercion ──────────────────────────

/// Every set coerces to a STRING; the null literal coerces to the
/// `Empty` sentinel with the divergent `(empty)` preview.
#[test]
fn bulk_set_coerces_to_string_and_empty() {
    // Number literal → string "8080".
    let num = compute_new_value(
        &Value::String("old".into()),
        &Some(BulkEditValueSource::Literal { literal: BulkEditLiteral::Number(8080.0) }),
        None,
    )
    .unwrap();
    assert!(matches!(&num, PropertiesSetValue::String(s) if s == "8080"));
    assert_eq!(render_set_preview(&num), "\"8080\"");

    // Bool literal → string "true".
    let b = compute_new_value(
        &Value::String("old".into()),
        &Some(BulkEditValueSource::Literal { literal: BulkEditLiteral::Bool(true) }),
        None,
    )
    .unwrap();
    assert!(matches!(&b, PropertiesSetValue::String(s) if s == "true"));

    // Null literal → Empty sentinel + `(empty)` preview.
    let nul = compute_new_value(
        &Value::String("old".into()),
        &Some(BulkEditValueSource::Literal { literal: BulkEditLiteral::Null }),
        None,
    )
    .unwrap();
    assert!(matches!(nul, PropertiesSetValue::Empty));
    assert_eq!(render_set_preview(&nul), "(empty)");
}

/// Float with a fractional part keeps its decimal form.
#[test]
fn bulk_set_float_keeps_fraction() {
    let f = compute_new_value(
        &Value::String("old".into()),
        &Some(BulkEditValueSource::Literal { literal: BulkEditLiteral::Number(1.5) }),
        None,
    )
    .unwrap();
    assert!(matches!(&f, PropertiesSetValue::String(s) if s == "1.5"));
}

/// The `(empty)` set writes `key=` (key preserved, value emptied) through
/// the line-model bulk apply.
#[test]
fn bulk_empty_writes_blank_value() {
    let src = "a=1\nb=2\n";
    let ops = vec![(vec!["a".into()], PropertiesBulkOp::Set(PropertiesSetValue::Empty))];
    let out = crate::line_model::apply_bulk_edits_text(src, &ops).unwrap();
    assert!(out.contains("a=\n"), "a emptied, key kept: {out:?}");
    assert!(out.contains("b=2\n"));
}

/// build_site_for_preview marks a container `set` as skipped, and a
/// delete on a leaf as `(removed)`.
#[test]
fn preview_skips_container_set() {
    let src = "server.port=8080\n";
    let root = crate::project::parse_to_value(src).unwrap();
    let server = root.get("server").unwrap();
    let site = build_site_for_preview(
        "a.properties", "a.properties", "a.properties",
        &["server".into()], server,
        BulkEditAction::Set,
        &Some(BulkEditValueSource::Literal { literal: BulkEditLiteral::String("x".into()) }),
        None,
    );
    assert!(site.will_skip, "container set skipped");
    assert!(site.skip_reason.contains("container"));
}
