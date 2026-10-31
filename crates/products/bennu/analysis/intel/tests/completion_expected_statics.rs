//! A position that wants a class offers that class's constants and no-argument static factories without the class
//! being written first: `Money m = |` → `Money.ZERO`, `Money.zero()`.

mod common;
use bennu_proto::prelude::CompletionItem;
use common::*;

fn project(body: &str) -> Project {
    Project::new(&[
        (
            "Money.java",
            "package z;\npublic class Money {\n    public static final Money ZERO = null;\n    public static final Money ONE = null;\n    public static final String CURRENCY = \"EUR\";\n    public static Money zero() { return null; }\n    public static Money of(long v) { return null; }\n    private static Money hidden() { return null; }\n}\n",
        ),
        ("Level.java", "package z;\npublic enum Level { LOW, HIGH }\n"),
        ("Wallet.java", "package z;\npublic class Wallet {\n    public void pay(Money m) { }\n    public void rate(Level l) { }\n}\n"),
        ("Use.java", &format!("package z;\npublic class Use {{\n    void run(Wallet w) {{\n        {body}\n    }}\n}}\n")),
    ])
}

fn offered(body: &str, trigger: &str) -> Vec<CompletionItem> {
    let p = project(body);
    let off = at(p.source("Use.java"), trigger) + trigger.len();
    p.expected_statics_items("Use.java", off)
}

fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn a_declaration_wants_the_constants_and_the_no_argument_factories_of_its_class() {
    let items = offered("Money m = ;", "Money m = ");
    let l = labels(&items);
    assert_eq!(l, ["Money.ZERO", "Money.ONE", "Money.zero()"], "{l:?}");
    assert_eq!(items[0].auto_import.as_deref(), Some("z.Money"));
}

#[test]
fn an_argument_slot_wants_it_too_and_a_typed_start_narrows() {
    let items = offered("w.pay(Z);", "w.pay(Z");
    // the typed capital `Z` starts `ZERO`, and under the default case rule not `zero`
    assert_eq!(labels(&items), ["Money.ZERO"]);
}

#[test]
fn an_enum_slot_is_the_other_lists_and_a_string_slot_has_none() {
    assert!(offered("w.rate();", "w.rate(").is_empty());
    assert!(offered("String s = ;", "String s = ").is_empty());
}
