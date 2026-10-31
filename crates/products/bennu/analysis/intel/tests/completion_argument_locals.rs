//! In a call's argument slot the names in scope that fit its type lead, the one that resembles the parameter's name
//! first — `transfer(|` with `(Account from, Account to, long amount)`.

mod common;
use bennu_proto::prelude::CompletionItem;
use common::*;

fn project(body: &str) -> Project {
    Project::new(&[
        ("Account.java", "package z;\npublic class Account { }\n"),
        (
            "Bank.java",
            "package z;\npublic class Bank {\n    public void transfer(Account from, Account to, long amount) { }\n    public void log(String s) { }\n    public void log(int n) { }\n}\n",
        ),
        (
            "Use.java",
            &format!("package z;\npublic class Use {{\n    private Account fallback;\n    void run(Bank bank, Account target, Account from, long amount, String note) {{\n        Account source = null;\n        {body}\n    }}\n}}\n"),
        ),
    ])
}

fn offered(body: &str, trigger: &str) -> Vec<CompletionItem> {
    let p = project(body);
    let off = at_last(p.source("Use.java"), trigger) + trigger.len();
    p.argument_locals_items("Use.java", off)
}

fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn the_name_matching_the_parameter_leads_the_first_slot() {
    let items = offered("bank.transfer();", "transfer(");
    let names = labels(&items);
    assert_eq!(names.first(), Some(&"from"), "{names:?}");
    for want in ["target", "source", "fallback"] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    assert!(!names.contains(&"amount") && !names.contains(&"note"), "names of another type are not offered: {names:?}");
    assert!(items[0].preselect);
}

#[test]
fn a_later_slot_takes_its_own_parameter_name_and_type() {
    let items = offered("bank.transfer(from, target, );", "target, ");
    assert_eq!(labels(&items), ["amount"]);
    assert!(items[0].preselect);
}

#[test]
fn a_typed_start_narrows_the_names() {
    let items = offered("bank.transfer(so);", "transfer(so");
    assert_eq!(labels(&items), ["source"]);
}

#[test]
fn overloads_that_disagree_offer_nothing() {
    assert!(offered("bank.log();", "log(").is_empty());
}

