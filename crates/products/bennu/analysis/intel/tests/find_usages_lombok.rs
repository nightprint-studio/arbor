//! Find-usages on a field whose accessors LOMBOK generates.
//!
//! On such a class the accessors are the *only* way the field is used from outside: nothing writes
//! `order.customer_name`, everything writes `order.getCustomer_name()`. So a usages list that
//! reported only direct uses of the field reported a field nobody touches — while the getter itself
//! could not be asked about either, because it has no declaration anywhere to put a caret on. The
//! field is the one place the question can be asked, and it has to answer for the whole set.
//!
//! Rename already carried those call sites. These tests pin the other half: whatever a rename would
//! move, a search finds — both from the same `generated_aliases`.
//!
//! The engine reports them as separate groups (`aliases`), each labelled with how it is written;
//! the backend flattens them into one list where each hit carries that label as `via`.

mod common;
use common::{at, Project};

const ORDER: &str = r#"package p;
import lombok.Data;

@Data
public class Order {
    private String customer_name;
    private int quantity;
}
"#;

const REPORT: &str = r#"package p;
public class Report {
    String describe(Order o) {
        return o.getCustomer_name();
    }
    void rename(Order o) {
        o.setCustomer_name("x");
    }
    int howMany(Order o) {
        return o.getQuantity();
    }
}
"#;

/// A plain class, whose accessor someone actually wrote — the control.
const PLAIN: &str = r#"package p;
public class Plain {
    private String label;
    public String getLabel() { return label; }
    public void use() { System.out.println(label); }
}
"#;

fn project() -> Project {
    Project::new(&[("Order.java", ORDER), ("Report.java", REPORT), ("Plain.java", PLAIN)])
}

/// The alias group labels reported for the field declared as `decl` in `file`.
fn alias_labels(p: &Project, file: &str, decl: &str) -> Vec<String> {
    let src = p.source(file).to_string();
    let result = p.find_usages(file, at(&src, decl)).expect("the field is referenceable");
    result.aliases.iter().map(|a| a.label.clone()).collect()
}

#[test]
fn a_lombok_fields_usages_include_its_generated_getter_and_setter() {
    let labels = alias_labels(&project(), "Order.java", "customer_name;");
    assert!(
        labels.iter().any(|l| l == "getCustomer_name()"),
        "the generated getter's call sites are uses of the field: {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "setCustomer_name()"),
        "and so are the setter's: {labels:?}"
    );
}

/// Each group holds the hits actually written that way — the label is a claim about the lines in it.
#[test]
fn a_group_holds_the_call_sites_it_is_named_after() {
    let p = project();
    let src = p.source("Order.java").to_string();
    let result = p
        .find_usages("Order.java", at(&src, "customer_name;"))
        .expect("referenceable");
    assert!(!result.aliases.is_empty(), "nothing was found through an accessor");
    for group in &result.aliases {
        let written = group.label.trim_end_matches("()");
        for u in &group.usages {
            assert!(
                u.preview.contains(written),
                "a hit under `{}` should be a line that writes it: {:?}",
                group.label,
                u.preview
            );
        }
    }
}

/// One field's accessors are not another's.
#[test]
fn the_accessors_of_a_different_field_are_not_included() {
    let labels = alias_labels(&project(), "Order.java", "customer_name;");
    assert!(
        !labels.iter().any(|l| l.contains("Quantity")),
        "`quantity`'s getter belongs to `quantity`: {labels:?}"
    );
}

/// A hand-written accessor is a declaration in its own right; asking about the field it reads must
/// not start reporting the accessor's callers as uses of the field.
#[test]
fn a_plain_classs_field_gains_nothing() {
    let labels = alias_labels(&project(), "Plain.java", "label;");
    assert!(labels.is_empty(), "nothing is generated here: {labels:?}");
}

/// The accessors are added to the field's own direct uses, not put in place of them.
#[test]
fn the_fields_direct_uses_are_still_reported() {
    let p = project();
    let src = p.source("Plain.java").to_string();
    let result = p.find_usages("Plain.java", at(&src, "label;")).expect("referenceable");
    assert!(
        !result.usages.is_empty(),
        "`label` is read by `getLabel` and by `use`"
    );
}
