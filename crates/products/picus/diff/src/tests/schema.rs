//! Relations, columns, and the objects hanging off them.

use picus_types::prelude::{RelationKind, SchemaSnapshot};

use crate::config::{ColumnFilter, ConstraintCheck, DiffConfig, IndexCheck, SchemaCheck};
use crate::schema::{
    compare_constraints, compare_indexes, compare_schema, compare_sequences, compare_triggers,
    ConstraintKind,
};
use crate::tests::{
    column, foreign_key, index, key_column, sequence, snapshot, table, trigger, view,
};

fn default_config() -> DiffConfig {
    DiffConfig::default()
}

#[test]
fn a_relation_one_side_does_not_have_is_reported_once() {
    let a = snapshot(vec![
        table("orders", vec![column("id", "integer")]),
        table("archive", vec![column("id", "integer")]),
    ]);
    let b = snapshot(vec![
        table("orders", vec![column("id", "integer")]),
        table("audit", vec![column("id", "integer")]),
    ]);

    let out = compare_schema(&a, &b, &default_config());
    assert_eq!(out.only_in_a.len(), 1);
    assert_eq!(out.only_in_a[0].name, "archive");
    assert_eq!(out.only_in_a[0].kind, RelationKind::Table);
    assert_eq!(out.only_in_b.len(), 1);
    assert_eq!(out.only_in_b[0].name, "audit");
    assert!(out.changed.is_empty());
    assert!(out.has_differences());
}

#[test]
fn one_table_written_in_two_engines_case_is_one_table() {
    let a = snapshot(vec![table("ORDERS", vec![column("CODE", "varchar2(30)")])]);
    let b = snapshot(vec![table("orders", vec![column("code", "varchar2(30)")])]);

    let folding = compare_schema(&a, &b, &default_config());
    assert!(!folding.has_differences(), "case alone is not a difference by default");

    let strict = DiffConfig { case_insensitive: false, ..default_config() };
    let out = compare_schema(&a, &b, &strict);
    assert_eq!(out.only_in_a.len(), 1);
    assert_eq!(out.only_in_b.len(), 1);
}

#[test]
fn a_column_added_removed_or_retyped_is_three_different_findings() {
    let a = snapshot(vec![table(
        "orders",
        vec![column("id", "integer"), column("label", "varchar(30)"), column("gone", "text")],
    )]);
    let b = snapshot(vec![table(
        "orders",
        vec![column("id", "integer"), column("label", "varchar(60)"), column("added", "text")],
    )]);

    let out = compare_schema(&a, &b, &default_config());
    let diff = &out.changed[0];
    assert_eq!(diff.columns_only_in_a, vec!["gone"]);
    assert_eq!(diff.columns_only_in_b, vec!["added"]);
    assert_eq!(diff.columns_changed.len(), 1);

    let changed = &diff.columns_changed[0];
    assert_eq!(changed.name, "label");
    let types = changed.data_type.as_ref().expect("the type changed");
    // Reported as each server spelled it — never normalised.
    assert_eq!(types.a, "varchar(30)");
    assert_eq!(types.b, "varchar(60)");
    assert!(changed.not_null.is_none());
}

#[test]
fn nullability_and_defaults_are_separate_properties() {
    let mut left = column("code", "text");
    left.default_value = Some("'x'".into());
    let mut right = column("code", "text");
    right.not_null = true;
    right.default_value = Some("''::text".into());

    let a = snapshot(vec![table("orders", vec![left])]);
    let b = snapshot(vec![table("orders", vec![right])]);

    let out = compare_schema(&a, &b, &default_config());
    let changed = &out.changed[0].columns_changed[0];
    assert!(changed.not_null.is_some());
    assert!(changed.default_value.is_some());

    // The two servers spell the same intent differently often enough that the
    // whole property is switchable.
    let lenient = DiffConfig {
        columns: ColumnFilter { ignore_defaults: true, ..ColumnFilter::default() },
        ..default_config()
    };
    let out = compare_schema(&a, &b, &lenient);
    let changed = &out.changed[0].columns_changed[0];
    assert!(changed.default_value.is_none());
    assert!(changed.not_null.is_some(), "the other properties still report");
}

#[test]
fn an_ignored_column_is_not_compared_at_all() {
    let a = snapshot(vec![table(
        "orders",
        vec![column("id", "integer"), column("updated_at", "timestamp")],
    )]);
    let b = snapshot(vec![table("orders", vec![column("id", "integer")])]);

    let config = DiffConfig {
        columns: ColumnFilter {
            ignore_patterns: vec!["updated_*".into()],
            ..ColumnFilter::default()
        },
        ..default_config()
    };
    assert!(!compare_schema(&a, &b, &config).has_differences());
    assert!(compare_schema(&a, &b, &default_config()).has_differences());
}

#[test]
fn a_column_that_only_moved_is_quiet_until_asked_about() {
    let a = snapshot(vec![table(
        "orders",
        vec![column("id", "integer"), column("code", "text")],
    )]);
    let b = snapshot(vec![table(
        "orders",
        vec![column("code", "text"), column("id", "integer")],
    )]);

    assert!(!compare_schema(&a, &b, &default_config()).has_differences());

    let strict = DiffConfig {
        columns: ColumnFilter { ignore_position: false, ..ColumnFilter::default() },
        ..default_config()
    };
    let out = compare_schema(&a, &b, &strict);
    assert_eq!(out.changed[0].columns_changed.len(), 2);
    let moved = &out.changed[0].columns_changed[0];
    let position = moved.position.as_ref().expect("it moved");
    assert_eq!((position.a, position.b), (0, 1));
}

#[test]
fn a_table_that_became_a_view_is_a_change_and_not_a_pair_of_absences() {
    let a = snapshot(vec![table("summary", vec![column("code", "text")])]);
    let b = SchemaSnapshot {
        views: vec![view("summary", "select code from orders")],
        ..SchemaSnapshot::default()
    };

    let out = compare_schema(&a, &b, &default_config());
    assert!(out.only_in_a.is_empty());
    assert!(out.only_in_b.is_empty());
    let kind = out.changed[0].kind_changed.as_ref().expect("kind changed");
    assert_eq!((kind.a, kind.b), (RelationKind::Table, RelationKind::View));
}

#[test]
fn view_definitions_are_compared_only_when_asked() {
    let a = SchemaSnapshot {
        views: vec![view("v_orders", "select code from orders")],
        ..SchemaSnapshot::default()
    };
    let b = SchemaSnapshot {
        views: vec![view("v_orders", "SELECT code FROM orders WHERE code IS NOT NULL")],
        ..SchemaSnapshot::default()
    };

    assert!(!compare_schema(&a, &b, &default_config()).has_differences());

    let config = DiffConfig {
        schema: SchemaCheck { compare_view_definitions: true, ..SchemaCheck::default() },
        ..default_config()
    };
    assert!(compare_schema(&a, &b, &config).changed[0].definition.is_some());
}

#[test]
fn indexes_that_were_never_read_are_not_indexes_that_matched() {
    let a = snapshot(vec![table("orders", vec![column("id", "integer")])]);
    let b = snapshot(vec![table("orders", vec![column("id", "integer")])]);

    let out = compare_indexes(&a, &b, &default_config());
    assert!(!out.has_differences());
    assert!(out.is_partial(), "a snapshot without indexes must not read as 'no differences'");
    assert_eq!(out.not_read, vec!["orders"]);
}

#[test]
fn an_index_is_compared_on_its_columns_in_order_and_on_uniqueness() {
    let mut ta = table("orders", vec![column("id", "integer")]);
    ta.indexes = Some(vec![
        index("ix_one", &["code", "area"], false),
        index("ix_only_a", &["label"], false),
        index("orders_pkey", &["id"], true),
    ]);
    let mut tb = table("orders", vec![column("id", "integer")]);
    tb.indexes = Some(vec![
        index("ix_one", &["area", "code"], true),
        index("orders_pkey", &["id"], true),
    ]);
    // The index behind a primary key is not a user object; the constraint check
    // speaks about it instead.
    if let Some(list) = tb.indexes.as_mut() {
        list[1].primary_key = true;
    }
    if let Some(list) = ta.indexes.as_mut() {
        list[2].primary_key = true;
    }

    let out = compare_indexes(&snapshot(vec![ta]), &snapshot(vec![tb]), &default_config());
    assert!(out.not_read.is_empty());
    assert_eq!(out.only_in_a.len(), 1);
    assert_eq!(out.only_in_a[0].name, "ix_only_a");
    assert_eq!(out.changed.len(), 1);
    let diff = &out.changed[0];
    assert!(diff.columns.is_some(), "(code, area) is not the index on (area, code)");
    assert!(diff.unique.is_some());
}

#[test]
fn an_index_filter_takes_names_out_of_scope_entirely() {
    let mut ta = table("orders", vec![column("id", "integer")]);
    ta.indexes = Some(vec![index("ix_tmp_build", &["code"], false)]);
    let mut tb = table("orders", vec![column("id", "integer")]);
    tb.indexes = Some(vec![]);

    let config = DiffConfig {
        indexes: IndexCheck {
            filter: crate::config::NameFilter::exclude(["ix_tmp_*"]),
            ..IndexCheck::default()
        },
        ..default_config()
    };
    let out = compare_indexes(&snapshot(vec![ta.clone()]), &snapshot(vec![tb.clone()]), &config);
    assert!(!out.has_differences());

    let out = compare_indexes(&snapshot(vec![ta]), &snapshot(vec![tb]), &default_config());
    assert_eq!(out.only_in_a.len(), 1);
}

#[test]
fn a_foreign_key_is_the_same_key_whatever_the_server_called_it() {
    let mut ta = table("orders", vec![column("area_id", "integer")]);
    ta.foreign_keys = Some(vec![foreign_key("SYS_C0011423", "area_id", "areas", "id")]);
    let mut tb = table("orders", vec![column("area_id", "integer")]);
    tb.foreign_keys = Some(vec![foreign_key("SYS_C0028871", "area_id", "areas", "id")]);

    let out = compare_constraints(&snapshot(vec![ta.clone()]), &snapshot(vec![tb.clone()]), &default_config());
    assert!(!out.has_differences(), "generated names must not become forty findings");

    // A repository that names its constraints can say so, and then a rename is a
    // change.
    let strict = DiffConfig {
        constraints: ConstraintCheck { ignore_names: false, ..ConstraintCheck::default() },
        ..default_config()
    };
    let out = compare_constraints(&snapshot(vec![ta]), &snapshot(vec![tb]), &strict);
    assert_eq!(out.only_in_a.len(), 1);
    assert_eq!(out.only_in_b.len(), 1);
    assert_eq!(out.only_in_a[0].kind, ConstraintKind::ForeignKey);
}

#[test]
fn a_foreign_key_that_points_somewhere_else_is_a_change() {
    let mut ta = table("orders", vec![column("area_id", "integer")]);
    ta.foreign_keys = Some(vec![foreign_key("fk_area", "area_id", "areas", "id")]);
    let mut tb = table("orders", vec![column("area_id", "integer")]);
    tb.foreign_keys = Some(vec![foreign_key("fk_area", "area_id", "regions", "id")]);

    let strict = DiffConfig {
        constraints: ConstraintCheck { ignore_names: false, ..ConstraintCheck::default() },
        ..default_config()
    };
    let out = compare_constraints(&snapshot(vec![ta]), &snapshot(vec![tb]), &strict);
    assert_eq!(out.changed.len(), 1);
    let target = out.changed[0].referenced_table.as_ref().expect("it points elsewhere");
    assert_eq!((target.a.as_str(), target.b.as_str()), ("areas", "regions"));
}

#[test]
fn a_primary_key_is_compared_on_the_columns_that_make_it_up() {
    let mut ta = table("orders", vec![key_column("id", "integer"), column("code", "text")]);
    ta.foreign_keys = Some(vec![]);
    let mut tb = table("orders", vec![key_column("id", "integer"), key_column("code", "text")]);
    tb.foreign_keys = Some(vec![]);

    let out = compare_constraints(&snapshot(vec![ta]), &snapshot(vec![tb]), &default_config());
    assert_eq!(out.changed.len(), 1);
    assert_eq!(out.changed[0].kind, ConstraintKind::PrimaryKey);
    let columns = out.changed[0].columns.as_ref().expect("the key changed");
    assert_eq!(columns.a, vec!["id"]);
    assert_eq!(columns.b, vec!["id", "code"]);
    assert!(out.foreign_keys_not_read.is_empty());
}

#[test]
fn a_sequence_is_only_worth_mentioning_past_the_threshold() {
    let a = SchemaSnapshot {
        sequences: vec![sequence("s_orders", 1_000), sequence("s_areas", 10)],
        ..SchemaSnapshot::default()
    };
    let b = SchemaSnapshot {
        sequences: vec![sequence("s_orders", 1_050), sequence("s_areas", 5_000)],
        ..SchemaSnapshot::default()
    };

    let out = compare_sequences(&a, &b, &default_config());
    assert_eq!(out.changed.len(), 2, "both moved, and both are reported");
    let orders = out.changed.iter().find(|s| s.name == "s_orders").expect("present");
    assert_eq!(orders.delta, Some(50));
    assert_eq!(orders.severity, crate::change::Severity::Ok);
    let areas = out.changed.iter().find(|s| s.name == "s_areas").expect("present");
    assert_eq!(areas.severity, crate::change::Severity::Warning);
}

#[test]
fn a_trigger_that_is_switched_off_on_one_side_is_a_difference() {
    let mut off = trigger("t_orders_audit", "orders", &["INSERT", "UPDATE"]);
    off.enabled = false;
    let a = SchemaSnapshot {
        tables: vec![table("orders", vec![column("id", "integer")])],
        triggers: vec![trigger("t_orders_audit", "orders", &["UPDATE", "INSERT"])],
        ..SchemaSnapshot::default()
    };
    let b = SchemaSnapshot {
        tables: vec![table("orders", vec![column("id", "integer")])],
        triggers: vec![off],
        ..SchemaSnapshot::default()
    };

    let out = compare_triggers(&a, &b, &default_config());
    assert_eq!(out.changed.len(), 1);
    // The events are the same set written in two orders, and that is not a change.
    assert!(out.changed[0].events.is_none());
    assert!(out.changed[0].enabled.is_some());
}
