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
     import java.util.function.Function;\n\
     public class Cfg {\n\
     \x20   public void addMethod(Verb verb) { }\n\
     \x20   public void addMethod(String method) { }\n\
     \x20   public void addHeader(String header) { }\n\
     \x20   public void put(String key, Verb verb) { }\n\
     \x20   public void put(String key, String value) { }\n\
     \x20   public void uri(String path, Endpoint endpoint) { }\n\
     \x20   public void uri(String template, Function<Cfg, Endpoint> factory) { }\n\
     \x20   public void join(String sep, String... parts) { }\n\
     \x20   public void join(Verb first, Verb second) { }\n\
     \x20   public void pick(Object value) { }\n\
     \x20   public void pick(String text) { }\n\
     }\n";

const VERB: &str = "package shop;\npublic class Verb { }\n";

const ENDPOINT: &str = "package shop;\npublic class Endpoint { }\n";

fn project(body: &str) -> Project {
    Project::new(&[
        ("Verb.java", VERB),
        ("Endpoint.java", ENDPOINT),
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

/// `uri("/x", b -> …)`: both overloads take a `String` first, and only one can take a lambda second.
/// The lambda itself gets no name (it explains itself), but it decides whose names the rest get.
#[test]
fn a_lambda_argument_picks_the_function_overload() {
    let got = labels("        c.uri(\"/x\", b -> null);");
    assert!(got.contains(&"template:".to_string()), "{got:?}");
    assert!(!got.contains(&"path:".to_string()), "{got:?}");
}

#[test]
fn a_class_argument_picks_the_non_functional_overload() {
    let got = labels("        c.uri(\"/x\", new Endpoint());");
    assert!(got.contains(&"path:".to_string()), "{got:?}");
    assert!(!got.contains(&"template:".to_string()), "{got:?}");
}

#[test]
fn a_varargs_overload_named_by_its_arguments() {
    // Two strings fit neither `(Verb, Verb)` nor `(String, String[])` by fixed arity — only as varargs.
    let got = labels("        c.join(\"a\", \"b\");");
    assert!(got.contains(&"sep:".to_string()) && got.contains(&"parts:".to_string()), "{got:?}");
}

#[test]
fn a_varargs_tail_of_several_arguments_gets_no_names() {
    // The overload is settled, but `parts` is not the name of any ONE of `"b"`, `"c"`.
    let got = labels("        c.join(\"a\", \"b\", \"c\");");
    assert!(!got.contains(&"sep:".to_string()) && !got.contains(&"parts:".to_string()), "{got:?}");
}

#[test]
fn null_picks_the_most_specific_reference_overload() {
    // `pick(String)` is more specific than `pick(Object)`, as javac decides it.
    let got = labels("        c.pick(null);");
    assert!(got.contains(&"text:".to_string()), "{got:?}");
    assert!(!got.contains(&"value:".to_string()), "{got:?}");
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
    // `null` fits both overloads and neither type is below the other — javac calls it ambiguous too.
    let got = labels("        c.addMethod(null);");
    assert!(!got.contains(&"method:".to_string()), "{got:?}");
    assert!(!got.contains(&"verb:".to_string()), "{got:?}");
}
