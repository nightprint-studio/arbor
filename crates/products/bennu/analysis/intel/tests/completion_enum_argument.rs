//! An argument that takes an **enum** is offered that enum's constants first, the one that fits the names around the
//! caret on top — `Device.builder().type(|)` with a field called `pin` in the file puts `DeviceType.PIN` first.

mod common;
use bennu_proto::prelude::CompletionItem;
use common::*;

fn project(body: &str) -> Project {
    Project::new(&[
        ("DeviceType.java", "package z;\npublic enum DeviceType { USB, BLUETOOTH, PIN, SERIAL }\n"),
        (
            "Device.java",
            "package z;\npublic class Device {\n    public static Device builder() { return null; }\n    public Device type(DeviceType t) { return this; }\n    public Device name(String n) { return this; }\n}\n",
        ),
        ("Other.java", "package z;\npublic class Other { public void set(int a) { } public void set(String a) { } }\n"),
        ("Use.java", &format!("package z;\npublic class Use {{\n    private String pin;\n    void run(Other other) {{\n        {body}\n    }}\n}}\n")),
    ])
}

/// The rows offered with the caret just after `trigger`.
fn offered(body: &str, trigger: &str) -> Vec<CompletionItem> {
    let p = project(body);
    let off = at(p.source("Use.java"), trigger) + trigger.len();
    p.enum_argument_items("Use.java", off)
}

#[test]
fn the_constants_of_the_enum_lead_and_the_one_matching_a_nearby_name_is_first() {
    let items = offered("Device.builder().type();", "type(");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels.first(), Some(&"DeviceType.PIN"), "{labels:?}");
    for want in ["DeviceType.USB", "DeviceType.BLUETOOTH", "DeviceType.SERIAL"] {
        assert!(labels.contains(&want), "{want} missing from {labels:?}");
    }
    assert!(items[0].preselect);
    assert_eq!(items[0].auto_import.as_deref(), Some("z.DeviceType"));
}

#[test]
fn a_typed_start_narrows_to_the_constants_it_begins() {
    let items = offered("Device.builder().type(BLU);", "type(BLU");
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("DeviceType.BLUETOOTH"));
    assert!(!items.iter().any(|i| i.label == "DeviceType.PIN"));
}

#[test]
fn an_argument_that_is_not_an_enum_offers_no_constants() {
    assert!(offered("Device.builder().name();", "name(").is_empty());
}

#[test]
fn overloads_that_disagree_offer_no_constants() {
    assert!(offered("other.set();", "set(").is_empty());
}
