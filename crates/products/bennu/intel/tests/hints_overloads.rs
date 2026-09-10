//! Parameter-name hints where a method is **overloaded**.
//!
//! The rule was arity alone: two overloads taking one argument each, and neither could be the one,
//! so the line got no names. That is right when the code is genuinely ambiguous and wrong the rest
//! of the time — `addAllowedMethod("*")` against `(HttpMethod)` and `(String)` is settled by the
//! argument, and losing its name beside four sibling calls that kept theirs reads as the feature
//! being unreliable rather than careful.
//!
//! Half of this file is the abstentions, because a hint is drawn as if the compiler had said it.

mod common;
use common::*;

const CFG: &str = "package shop;\n\
     public class Cfg {\n\
     \x20   public void addMethod(Verb verb) { }\n\
     \x20   public void addMethod(String method) { }\n\
     \x20   public void addHeader(String header) { }\n\
     \x20   public void put(String key, Verb verb) { }\n\
     \x20   public void put(String key, String value) { }\n\
     }\n";

const VERB: &str = "package shop;\npublic class Verb { }\n";

fn project(body: &str) -> Project {
    Project::new(&[
        ("Verb.java", VERB),
        ("Cfg.java", CFG),
        (
            "Use.java",
            &format!("package shop;\npublic class Use {{\n    void go(Cfg c, Verb v) {{\n{body}\n    }}\n}}\n"),
        ),
    ])
}

fn labels(body: &str) -> Vec<String> {
    project(body).hints("Use.java").into_iter().map(|(_, l)| l).collect()
}

// ── What the arguments settle ────────────────────────────────────────────────────────────────

#[test]
fn a_string_argument_picks_the_string_overload() {
    assert!(
        labels("        c.addMethod(\"*\");").contains(&"method:".to_string()),
        "{:?}",
        labels("        c.addMethod(\"*\");")
    );
}

#[test]
fn the_other_overload_is_picked_by_its_own_argument_type() {
    assert!(
        labels("        c.addMethod(v);").contains(&"verb:".to_string()),
        "{:?}",
        labels("        c.addMethod(v);")
    );
}

#[test]
fn a_later_parameter_can_be_the_one_that_decides() {
    // The first parameter is `String` in both; the second is what tells them apart.
    let got = labels("        c.put(\"k\", v);");
    assert!(got.contains(&"key:".to_string()) && got.contains(&"verb:".to_string()), "{got:?}");
}

#[test]
fn a_method_with_one_overload_is_untouched() {
    assert!(labels("        c.addHeader(\"*\");").contains(&"header:".to_string()));
}

// ── What it still abstains on ────────────────────────────────────────────────────────────────

#[test]
fn an_argument_whose_type_is_unknown_settles_nothing() {
    // `nope` is not declared, so nothing infers — and an overload set narrowed by a guess is
    // exactly the wrong kind of hint.
    let got = labels("        c.addMethod(nope);");
    assert!(!got.contains(&"method:".to_string()), "{got:?}");
    assert!(!got.contains(&"verb:".to_string()), "{got:?}");
}

#[test]
fn null_settles_nothing() {
    // `null` fits both overloads, which is precisely a call a reader has to think about too.
    let got = labels("        c.addMethod(null);");
    assert!(!got.contains(&"method:".to_string()), "{got:?}");
    assert!(!got.contains(&"verb:".to_string()), "{got:?}");
}
