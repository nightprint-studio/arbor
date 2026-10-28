//! `catch (|` — the exceptions the `try` above it can throw lead the type-name answer.
//!
//! The right answer there is not a matter of taste: the block says what it throws, through a
//! `throw new` and through the `throws` clause of the methods it calls. These tests pin both
//! sources, the multi-catch bookkeeping, the walk back over earlier `catch` clauses, and the places
//! where the answer must be empty rather than a guess.
//!
//! The ranking tuning is process-wide and untouched here (its switch is a unit test of the ranking).

mod common;
use common::*;

fn project(use_body: &str) -> Project {
    Project::new(&[
        (
            "Repo.java",
            "package shop;\n\
             public class Repo {\n\
             \x20   public void save(Object o) throws java.io.IOException { }\n\
             \x20   public void load() throws PaymentException { }\n\
             \x20   public void ping() { }\n\
             }\n",
        ),
        (
            "PaymentException.java",
            "package shop;\npublic class PaymentException extends Exception { }\n",
        ),
        (
            "Use.java",
            &format!("package shop;\npublic class Use {{\n    public void run(Repo repo) {{\n{use_body}\n    }}\n}}\n"),
        ),
    ])
}

fn caught(p: &Project, trigger: &str) -> Vec<String> {
    let offset = at_last(p.source("Use.java"), trigger) + trigger.len();
    p.caught_types("Use.java", offset)
}

#[test]
fn the_exceptions_a_called_method_declares_are_offered() {
    let p = project("        try {\n            repo.save(null);\n        } catch (");
    let names = caught(&p, "catch (");
    assert!(names.contains(&"IOException".to_string()), "{names:?}");
}

#[test]
fn a_throw_new_in_the_body_is_offered_and_leads() {
    let p = project("        try {\n            repo.save(null);\n            throw new PaymentException();\n        } catch (");
    let names = caught(&p, "catch (");
    assert_eq!(names.first().map(String::as_str), Some("PaymentException"), "{names:?}");
    assert!(names.contains(&"IOException".to_string()), "{names:?}");
}

#[test]
fn the_first_row_is_the_only_one_preselected() {
    let p = project("        try {\n            repo.save(null);\n            repo.load();\n        } catch (");
    let offset = at_last(p.source("Use.java"), "catch (") + "catch (".len();
    let items = p.caught_items("Use.java", offset);
    assert!(items.len() >= 2);
    assert!(items[0].preselect);
    assert!(items[1..].iter().all(|i| !i.preselect));
}

/// The second `catch` of a statement still answers for the same `try`.
#[test]
fn an_earlier_catch_clause_does_not_hide_the_try() {
    let p = project(
        "        try {\n            repo.save(null);\n            repo.load();\n        } catch (java.io.IOException e) {\n        } catch (",
    );
    let names = caught(&p, "} catch (");
    assert!(names.contains(&"PaymentException".to_string()), "{names:?}");
}

#[test]
fn a_multi_catch_does_not_offer_what_it_already_lists() {
    let p = project("        try {\n            repo.save(null);\n            repo.load();\n        } catch (IOException | ");
    let names = caught(&p, "catch (IOException | ");
    assert!(!names.contains(&"IOException".to_string()), "{names:?}");
    assert!(names.contains(&"PaymentException".to_string()), "{names:?}");
}

#[test]
fn a_typed_prefix_filters_the_exceptions() {
    let p = project("        try {\n            repo.save(null);\n            repo.load();\n        } catch (Pay");
    let names = caught(&p, "catch (Pay");
    assert_eq!(names, ["PaymentException"]);
}

/// A lowercase word is a name being written, not a type.
#[test]
fn a_lowercase_prefix_is_not_a_type() {
    let p = project("        try {\n            repo.load();\n        } catch (pay");
    assert!(caught(&p, "catch (pay").is_empty());
}

/// Once the type and the name are written the caret is past the type position.
#[test]
fn nothing_is_offered_after_the_exception_type() {
    let p = project("        try {\n            repo.load();\n        } catch (PaymentException ");
    assert!(caught(&p, "catch (PaymentException ").is_empty());
}

/// A method that throws nothing says nothing: the leaders are what the body can throw.
#[test]
fn a_body_that_throws_nothing_offers_nothing() {
    let p = project("        try {\n            repo.ping();\n        } catch (");
    assert!(caught(&p, "catch (").is_empty());
}

/// A call whose receiver cannot be typed names no overload, and a guess would offer an exception
/// the code may not be able to throw. The `throw new` is still read.
#[test]
fn an_unresolvable_call_contributes_nothing_but_the_throw_new_still_counts() {
    let p = project("        try {\n            mystery.save(null);\n            throw new PaymentException();\n        } catch (");
    let names = caught(&p, "catch (");
    assert_eq!(names, ["PaymentException"]);
}

/// Only a `try` is a try: a `finally` block between is not something to read exceptions from.
#[test]
fn a_catch_that_does_not_follow_a_try_offers_nothing() {
    let p = project("        {\n            repo.load();\n        } catch (");
    assert!(caught(&p, "catch (").is_empty());
}
