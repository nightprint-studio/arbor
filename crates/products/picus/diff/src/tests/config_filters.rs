//! What the configuration lets through, and what it defaults to.

use picus_types::prelude::RelationKind;

use crate::config::{ContentCheck, DiffConfig, NameFilter, TableRules};
use crate::rows::RowCompareOptions;

#[test]
fn the_three_filter_modes_read_the_patterns_differently() {
    let all = NameFilter::all();
    assert!(all.accepts("anything", true));

    let include = NameFilter::include(["ref_*"]);
    assert!(include.accepts("ref_status", true));
    assert!(!include.accepts("orders", true));

    let exclude = NameFilter::exclude(["tmp_*", "*_bak"]);
    assert!(exclude.accepts("orders", true));
    assert!(!exclude.accepts("tmp_orders", true));
    assert!(!exclude.accepts("orders_bak", true));
}

#[test]
fn an_include_list_nobody_filled_in_accepts_nothing() {
    // The alternative — falling back to "everything" — is a template that
    // silently compares a whole database because a list came out empty.
    let half_written = NameFilter { mode: crate::config::FilterMode::Include, patterns: vec![] };
    assert!(!half_written.accepts("orders", true));
}

#[test]
fn tables_and_views_are_filtered_separately_but_neither_falls_through() {
    let config = DiffConfig {
        tables: NameFilter::exclude(["tmp_*"]),
        views: NameFilter::include(["v_*"]),
        ..DiffConfig::default()
    };

    assert!(config.accepts(RelationKind::Table, "orders"));
    assert!(!config.accepts(RelationKind::Table, "tmp_orders"));
    assert!(config.accepts(RelationKind::View, "v_orders"));
    assert!(!config.accepts(RelationKind::View, "orders"));

    // A name whose kind is not known — a trigger's relation — is in scope if
    // either filter would take it.
    assert!(config.accepts_any_kind("orders"));
    assert!(config.accepts_any_kind("v_orders"));
    assert!(!config.accepts_any_kind("tmp_orders"));
}

#[test]
fn the_defaults_read_the_catalogue_and_nothing_else() {
    let config = DiffConfig::default();
    assert!(config.schema.enabled);
    assert!(config.indexes.enabled);
    assert!(config.constraints.enabled);
    assert!(config.triggers.enabled);
    assert!(config.sequences.enabled);
    // The two that touch data are opt-in.
    assert!(!config.counts.enabled);
    assert!(!config.contents.enabled);
}

#[test]
fn a_declared_key_beats_the_one_from_the_catalogue() {
    let content = ContentCheck {
        enabled: true,
        tables: vec![TableRules {
            name: "settings".into(),
            primary_key: vec!["code".into()],
            ignore_columns: vec!["updated_*".into()],
            ..TableRules::default()
        }],
        ..ContentCheck::default()
    };
    let columns = crate::config::ColumnFilter {
        ignore_patterns: vec!["created_*".into()],
        ..crate::config::ColumnFilter::default()
    };

    let declared =
        RowCompareOptions::resolve(&content, &columns, "settings", &["id".to_string()], true);
    assert_eq!(declared.key, vec!["code"], "the rule the user wrote wins");
    // Both ignore lists apply, not one or the other.
    assert_eq!(declared.ignore_columns, vec!["created_*", "updated_*"]);

    // A relation with no rule falls back to the catalogue's key.
    let fallback =
        RowCompareOptions::resolve(&content, &columns, "orders", &["id".to_string()], true);
    assert_eq!(fallback.key, vec!["id"]);
    assert_eq!(fallback.max_differences, Some(50));
}

#[test]
fn a_zero_cap_means_list_everything() {
    let content = ContentCheck { max_differences_shown: 0, ..ContentCheck::default() };
    let options = RowCompareOptions::resolve(
        &content,
        &crate::config::ColumnFilter::default(),
        "orders",
        &[],
        true,
    );
    assert_eq!(options.max_differences, None);
}

#[test]
fn a_per_table_limit_overrides_the_default_one() {
    let content = ContentCheck {
        default_limit: 1_000,
        tables: vec![TableRules { name: "settings".into(), limit: Some(50), ..TableRules::default() }],
        ..ContentCheck::default()
    };
    assert_eq!(content.limit_for("SETTINGS", true), 50);
    assert_eq!(content.limit_for("orders", true), 1_000);
    // Without folding, `SETTINGS` is not the relation the rule was written for.
    assert_eq!(content.limit_for("SETTINGS", false), 1_000);
}
