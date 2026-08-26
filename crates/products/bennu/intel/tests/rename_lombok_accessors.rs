//! Renaming a field carries the call sites of the accessors LOMBOK generates for it.
//!
//! Those methods exist only after annotation processing: there is no `getCustomer_name` anywhere in
//! the source for the rename's declaration half to edit. But every caller writes it down, and after
//! renaming the field Lombok generates `getCustomerName` instead — so leaving the call sites alone
//! breaks them all. The new accessor name is derived by re-running Lombok's own naming rule, so it
//! cannot drift from the name the index will actually synthesize.

mod common;
use common::{at, Project};

const ORDER: &str = r#"package p;
import lombok.Data;

@Data
public class Order {
    private String customer_name;
    private boolean is_paid;
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
    boolean paid(Order o) {
        return o.is_paid();
    }
}
"#;

fn project() -> Project {
    Project::new(&[("p/Order.java", ORDER), ("p/Report.java", REPORT)])
}

/// The edit at `needle` in `p/Report.java`, if the plan has one.
fn edit_at(p: &Project, needle: &str, new_name: &str) -> Option<String> {
    let order = p.source("p/Order.java");
    let edits = p.rename_edits("p/Order.java", at(order, "customer_name"), new_name);
    let report = p.source("p/Report.java");
    let off = at(report, needle);
    edits
        .iter()
        .find(|e| e.file == "p/Report.java" && e.start == off)
        .map(|e| e.new_text.clone())
}

#[test]
fn the_generated_getter_call_is_renamed_with_the_field() {
    let p = project();
    assert_eq!(edit_at(&p, "getCustomer_name", "customerName").as_deref(), Some("getCustomerName"));
}

#[test]
fn the_generated_setter_call_is_renamed_too() {
    let p = project();
    assert_eq!(edit_at(&p, "setCustomer_name", "customerName").as_deref(), Some("setCustomerName"));
}

#[test]
fn the_field_declaration_still_comes_along() {
    let p = project();
    let order = p.source("p/Order.java");
    let edits = p.rename_edits("p/Order.java", at(order, "customer_name"), "customerName");
    assert!(edits.iter().any(|e| e.file == "p/Order.java" && e.reason.label() == "declaration"));
}

/// A primitive `boolean` gets an `isX` getter, and Lombok strips a name's own leading `is` before
/// applying the prefix — so `is_paid` reads `is_paid()`. Renaming it to `paid` must produce
/// `isPaid()`, which only the real rule gets right.
#[test]
fn a_boolean_getter_follows_lomboks_is_stripping_rule() {
    let p = project();
    let order = p.source("p/Order.java");
    let edits = p.rename_edits("p/Order.java", at(order, "is_paid"), "paid");
    let report = p.source("p/Report.java");
    let off = at(report, "is_paid()");
    let e = edits
        .iter()
        .find(|e| e.file == "p/Report.java" && e.start == off)
        .expect("the boolean getter call was not renamed");
    assert_eq!(e.new_text, "isPaid");
}

/// A hand-written method of the same name and arity cancels Lombok's synthetic one — it is then a
/// real, independent declaration, and renaming the field must NOT rewrite its call sites.
#[test]
fn a_hand_written_accessor_is_left_alone() {
    let p = Project::new(&[
        (
            "p/Order.java",
            "package p;\nimport lombok.Data;\n\n@Data\npublic class Order {\n    private String customer_name;\n    public String getCustomer_name() { return customer_name; }\n}\n",
        ),
        (
            "p/Report.java",
            "package p;\npublic class Report {\n    String describe(Order o) {\n        return o.getCustomer_name();\n    }\n}\n",
        ),
    ]);
    let order = p.source("p/Order.java");
    let edits = p.rename_edits("p/Order.java", at(order, "customer_name"), "customerName");
    let report = p.source("p/Report.java");
    let call = at(report, "o.getCustomer_name()") + "o.".len();
    assert!(
        !edits.iter().any(|e| e.file == "p/Report.java" && e.start == call),
        "renamed a call to a hand-written method that Lombok never generated"
    );
}

/// No Lombok import → no synthesis → nothing extra to carry. Guards against inventing accessors for
/// a project's own `@Data` annotation.
#[test]
fn without_the_lombok_import_nothing_extra_is_renamed() {
    let p = Project::new(&[
        (
            "p/Order.java",
            "package p;\n\n@Data\npublic class Order {\n    private String customer_name;\n}\n",
        ),
        (
            "p/Report.java",
            "package p;\npublic class Report {\n    String describe(Order o) {\n        return o.getCustomer_name();\n    }\n}\n",
        ),
    ]);
    let order = p.source("p/Order.java");
    let edits = p.rename_edits("p/Order.java", at(order, "customer_name"), "customerName");
    assert!(edits.iter().all(|e| e.file == "p/Order.java"));
}

/// Renaming FROM a generated accessor would rewrite every caller to a name that nothing declares:
/// the method is not written down anywhere, so there is no declaration for the plan to edit and
/// the field would keep producing the old name. Refused — the field is the thing to rename.
#[test]
fn renaming_from_a_generated_accessor_is_refused() {
    let p = project();
    let report = p.source("p/Report.java");
    let off = at(report, "getCustomer_name");
    assert!(
        p.rename("p/Report.java", off, "getFullName").is_none(),
        "planned a rename with no declaration to edit"
    );
}
