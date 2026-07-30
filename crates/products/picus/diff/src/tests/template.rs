//! Saved configurations.

use crate::config::{ContentCheck, DiffConfig, NameFilter, TableRules};
use crate::template::{DiffTemplate, DiffTemplates};

#[test]
fn the_shipped_templates_answer_four_different_questions() {
    let templates = DiffTemplates::builtin();
    let ids: Vec<&str> = templates.templates.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        ids,
        ["structure", "structure-and-counts", "reference-data", "environments"]
    );
    assert!(templates.templates.iter().all(|t| t.builtin));
    assert!(
        templates.templates.iter().all(|t| !t.description.is_empty()),
        "a template nobody can tell apart from the next one is not pickable"
    );

    // The first one reads nothing but the catalogue.
    let structure = templates.get("structure").expect("shipped");
    assert!(!structure.config.counts.enabled);
    assert!(!structure.config.contents.enabled);
    assert!(templates.get("structure-and-counts").expect("shipped").config.counts.enabled);
    assert!(templates.get("reference-data").expect("shipped").config.contents.enabled);
}

#[test]
fn an_edited_template_keeps_its_place_in_the_list() {
    let mut templates = DiffTemplates::builtin();
    let renamed = DiffTemplate {
        name: "Structure (mine)".to_string(),
        ..templates.get("structure").expect("shipped").clone()
    };
    templates.upsert(renamed);

    assert_eq!(templates.templates[0].id, "structure");
    assert_eq!(templates.templates[0].name, "Structure (mine)");
    assert_eq!(templates.templates.len(), 4, "an upsert is not an insert");
}

#[test]
fn a_new_template_goes_at_the_end_and_a_builtin_one_cannot_be_removed() {
    let mut templates = DiffTemplates::builtin();
    templates.upsert(
        DiffTemplate::new("mine", "Lookup tables", DiffConfig::default())
            .describe("the three tables that matter"),
    );
    assert_eq!(templates.templates.len(), 5);
    assert_eq!(templates.templates[4].id, "mine");

    assert!(!templates.remove("structure"), "a shipped template stays");
    assert_eq!(templates.templates.len(), 5);
    assert!(templates.remove("mine"));
    assert_eq!(templates.templates.len(), 4);
    assert!(!templates.remove("mine"), "removing it twice is not a second success");
}

#[test]
fn an_unknown_template_falls_back_to_the_defaults_rather_than_failing_a_run() {
    let templates = DiffTemplates::builtin();
    assert_eq!(templates.config_for("deleted-last-week"), DiffConfig::default());
}

#[test]
fn a_template_survives_the_round_trip_it_is_stored_through() {
    let config = DiffConfig {
        tables: NameFilter::exclude(["tmp_*"]),
        contents: ContentCheck {
            enabled: true,
            max_differences_shown: 0,
            tables: vec![TableRules {
                name: "settings".into(),
                primary_key: vec!["code".into()],
                order_by: vec!["code".into()],
                ..TableRules::default()
            }],
            ..ContentCheck::default()
        },
        ..DiffConfig::default()
    };
    let mut templates = DiffTemplates::builtin();
    templates.upsert(DiffTemplate::new("mine", "Lookup tables", config));

    let json = serde_json::to_value(&templates).expect("serialises");
    assert!(json.is_array(), "the list is a list, not an object wrapping one");
    assert_eq!(json[4]["config"]["contents"]["maxDifferencesShown"], 0);
    assert_eq!(json[4]["config"]["tables"]["mode"], "exclude");
    assert_eq!(json[4]["config"]["contents"]["tables"][0]["primaryKey"][0], "code");

    let back: DiffTemplates = serde_json::from_value(json).expect("round-trips");
    assert_eq!(back, templates);
}

#[test]
fn a_configuration_written_by_hand_only_has_to_say_what_it_changes() {
    // Every check struct carries `#[serde(default)]`, so a template file is the
    // difference from the defaults rather than a full dump nobody can maintain.
    let partial = serde_json::json!({ "tables": { "mode": "exclude", "patterns": ["tmp_*"] } });
    let config: DiffConfig = serde_json::from_value(partial).expect("deserialises");

    assert!(!config.tables.accepts("tmp_orders", true));
    assert!(config.schema.enabled, "the untouched checks keep their defaults");
    assert!(config.case_insensitive);
    assert_eq!(config.contents.default_limit, 1_000);
}
