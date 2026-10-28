//! *Create method* writes the types the call site has, not `Object` for everything the text cannot
//! declare.
//!
//! The reported bug: the method was created, with every parameter typed `Object`, because the tree
//! alone knows a literal and a plain local and nothing else. An argument that is a method call, a
//! chain, a `var` local or an expression is typed by the resolver now, and the position the result
//! is used in names the return type. The popup offer and the quick fix make the same signature (one
//! reading, two places), so this pins it through the popup, which the harness can reach.

mod common;
use common::*;

fn project(use_class: &str) -> Project {
    Project::new(&[
        (
            "Order.java",
            "package shop;\n\
             public class Order {\n\
             \x20   public long total() { return 0; }\n\
             \x20   public Customer customer() { return null; }\n\
             }\n",
        ),
        (
            "Customer.java",
            "package shop;\npublic class Customer {\n    public String name() { return \"\"; }\n}\n",
        ),
        ("Use.java", use_class),
    ])
}

/// The offer for the missing method `report`, as the popup lists it, typed `rep` in the class body.
fn offered(p: &Project) -> Option<bennu_intel::prelude::CompletionItem> {
    let src = p.source("Use.java");
    let at = at(src, "    rep") + "    rep".len();
    p.generated("Use.java", at).into_iter().find(|i| i.label == "report")
}

#[test]
fn arguments_that_are_calls_are_typed_by_what_they_return() {
    let p = project(
        "package shop;\n\
         public class Use {\n\
         \x20   void f(Order o) {\n\
         \x20       report(o.total(), o.customer().name());\n\
         \x20   }\n\
         \x20   rep\n\
         }\n",
    );
    let item = offered(&p).expect("the missing method is offered");
    assert_eq!(item.detail.as_deref(), Some("(long, String) : void"), "{item:?}");
}

#[test]
fn a_var_local_is_typed_by_its_initializer() {
    let p = project(
        "package shop;\n\
         public class Use {\n\
         \x20   void f(Order o) {\n\
         \x20       var customer = o.customer();\n\
         \x20       report(customer);\n\
         \x20   }\n\
         \x20   rep\n\
         }\n",
    );
    let item = offered(&p).expect("the missing method is offered");
    assert_eq!(item.detail.as_deref(), Some("(Customer) : void"), "{item:?}");
    assert!(item.insert_text.as_deref().is_some_and(|t| t.contains("report(Customer customer)")), "{item:?}");
}

/// The position the result is used in names the return type when the text is silent about it.
#[test]
fn the_return_type_comes_from_where_the_result_goes() {
    let p = project(
        "package shop;\n\
         public class Use {\n\
         \x20   private long total;\n\
         \x20   void f() {\n\
         \x20       total = report();\n\
         \x20   }\n\
         \x20   rep\n\
         }\n",
    );
    let item = offered(&p).expect("the missing method is offered");
    assert_eq!(item.detail.as_deref(), Some("() : long"), "{item:?}");
}

/// What the text declares is still copied as written, and what nothing can say stays `Object`.
#[test]
fn a_declared_type_is_kept_and_an_unknown_one_stays_object() {
    let p = project(
        "package shop;\n\
         public class Use {\n\
         \x20   void f(String label) {\n\
         \x20       report(label, mystery());\n\
         \x20   }\n\
         \x20   rep\n\
         }\n",
    );
    let item = offered(&p).expect("the missing method is offered");
    assert_eq!(item.detail.as_deref(), Some("(String, Object) : void"), "{item:?}");
}
