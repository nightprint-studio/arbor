//! The enum's constants lead wherever the surrounding code fixes the type being written: the other side of a
//! comparison, a `case` label, a constructor argument, a ternary branch, an array element, `equals(…)`, `List.of(…)`.

mod common;
use bennu_proto::prelude::CompletionItem;
use common::*;

fn project(body: &str) -> Project {
    Project::new(&[
        ("DeviceType.java", "package z;\npublic enum DeviceType { USB, BLUETOOTH, PIN, SERIAL }\n"),
        ("Gadget.java", "package z;\npublic class Gadget {\n    public Gadget(DeviceType t) { }\n    public Gadget(DeviceType t, int n) { }\n}\n"),
        (
            "Use.java",
            &format!("package z;\npublic class Use {{\n    void run(DeviceType type, boolean c) {{\n        {body}\n    }}\n}}\n"),
        ),
    ])
}

/// The rows offered with the caret just after `trigger`.
fn offered(body: &str, trigger: &str) -> Vec<CompletionItem> {
    let p = project(body);
    let off = at(p.source("Use.java"), trigger) + trigger.len();
    p.enum_argument_items("Use.java", off)
}

fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn the_other_side_of_a_comparison_offers_the_constants() {
    for body in ["if (type == ) { }", "if (type != ) { }", "boolean b = type == ;"] {
        let trigger = if body.contains("!=") { "!= " } else { "== " };
        let items = offered(body, trigger);
        let l = labels(&items);
        assert!(l.contains(&"DeviceType.PIN") && l.contains(&"DeviceType.USB"), "{body}: {l:?}");
        assert_eq!(items[0].auto_import.as_deref(), Some("z.DeviceType"));
    }
}

#[test]
fn a_case_label_offers_the_bare_constants_not_yet_used() {
    let items = offered("switch (type) { case USB: break; case  }", "case USB: break; case ");
    let l = labels(&items);
    assert_eq!(l, ["BLUETOOTH", "PIN", "SERIAL"], "USB is named by an earlier label, and the label is written bare");
    assert!(items.iter().all(|i| i.auto_import.is_none()));
    // arrow form, several labels
    let items = offered("switch (type) { case USB, PIN -> go(); case  }", "case USB, PIN -> go(); case ");
    assert_eq!(labels(&items), ["BLUETOOTH", "SERIAL"]);
}

#[test]
fn a_constructor_argument_offers_the_constants_when_every_overload_agrees() {
    let items = offered("new Gadget();", "new Gadget(");
    assert!(labels(&items).contains(&"DeviceType.PIN"), "{:?}", labels(&items));
}

#[test]
fn a_ternary_branch_an_array_element_and_equals_offer_the_constants() {
    let items = offered("DeviceType t = c ? DeviceType.USB : ;", ": ");
    assert!(labels(&items).contains(&"DeviceType.PIN"), "ternary: {:?}", labels(&items));
    let items = offered("DeviceType[] xs = { DeviceType.USB,  };", "DeviceType.USB, ");
    assert!(labels(&items).contains(&"DeviceType.PIN"), "array: {:?}", labels(&items));
    let items = offered("type.equals();", "equals(");
    assert!(labels(&items).contains(&"DeviceType.PIN"), "equals: {:?}", labels(&items));
}

#[test]
fn a_position_that_fixes_no_enum_offers_nothing() {
    assert!(offered("int n = 3 < ;", "< ").is_empty());
    assert!(offered("String s = foo();", "foo(").is_empty());
}

#[test]
fn collection_factory_elements_offer_the_constants() {
    let p = Project::with_stream_jdk(&[
        ("DeviceType.java", "package z;\npublic enum DeviceType { USB, BLUETOOTH, PIN }\n"),
        ("Use.java", "package z;\nimport java.util.List;\npublic class Use {\n    void run() {\n        List<DeviceType> l = List.of();\n    }\n}\n"),
    ]);
    let src = p.source("Use.java");
    let off = at(src, "List.of(") + "List.of(".len();
    let l: Vec<String> = p.enum_argument_items("Use.java", off).into_iter().map(|i| i.label).collect();
    assert!(l.iter().any(|x| x == "DeviceType.PIN"), "{l:?}");
}
