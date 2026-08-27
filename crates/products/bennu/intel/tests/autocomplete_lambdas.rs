//! Completion **inside a lambda body** — where the receiver is a lambda parameter.
//!
//! A lambda parameter has no declared type: it is *target-typed* from the functional interface the
//! lambda is being passed to. Completion therefore has to find the call the lambda is an argument
//! to, resolve that parameter's interface, take its single abstract method and read the parameter at
//! the lambda's own index — substituting the interface's generics on the way through. Every one of
//! those steps is a place the type can be lost, and losing it means `x.` inside the lambda offers
//! nothing at all, which is what "completion doesn't work in lambdas" looks like from the outside.
//!
//! These tests pin down which shapes work, so a regression names the shape it broke. The receivers
//! are all PROJECT-declared types (their members are baked into the index), so no live JDK is
//! needed — which is also why the functional interfaces are declared in the fixture rather than
//! borrowed from `java.util.function`.
//!
//! Every case puts the caret after a `HERE.` marker, and where a lambda is involved names the
//! lambda's parameter `HERE` too — so one marker locates both the binding and the use, and the
//! offset is unambiguous (`HERE ->` contains no `HERE.`).

mod common;
use common::*;

/// A project-local `Eater`/`Mapper` pair plus the classes that take them, so the whole
/// target-typing chain resolves without a JDK.
const FIXTURE: [(&str, &str); 5] = [
    (
        "Pet.java",
        "package z;\n\
         public class Pet {\n\
         \x20   private String nickname;\n\
         \x20   public String getNickname() { return nickname; }\n\
         \x20   public int getAge() { return 0; }\n\
         }\n",
    ),
    (
        "Eater.java",
        "package z;\n\
         public interface Eater<T> {\n\
         \x20   void eat(T item);\n\
         }\n",
    ),
    (
        "Mapper.java",
        "package z;\n\
         public interface Mapper<T, R> {\n\
         \x20   R map(T item);\n\
         }\n",
    ),
    (
        "Holder.java",
        "package z;\n\
         public class Holder<T> {\n\
         \x20   public void each(Eater<T> e) { }\n\
         \x20   public <R> Holder<R> convert(Mapper<T, R> m) { return null; }\n\
         \x20   public T only() { return null; }\n\
         }\n",
    ),
    (
        "Repo.java",
        "package z;\n\
         public class Repo {\n\
         \x20   public Holder<Pet> pets() { return null; }\n\
         \x20   public void feed(Eater<Pet> e) { }\n\
         }\n",
    ),
];

/// The fixture plus a `Use.java` whose one method body is `body`, and the caret offset just after
/// the `HERE.` marker `body` must contain.
fn with_body(body: &str) -> (Project, String, usize) {
    let use_src = format!(
        "package z;\n\
         public class Use {{\n\
         \x20   public void run(Repo repo, Holder<Pet> holder) {{\n\
         \x20       {body}\n\
         \x20   }}\n\
         }}\n"
    );
    let mut files: Vec<(&str, &str)> = FIXTURE.to_vec();
    files.push(("Use.java", &use_src));
    let p = Project::new(&files);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "HERE.") + "HERE.".len();
    (p, s, off)
}

/// The members a `Pet` receiver must offer, whatever route the type arrived by.
#[track_caller]
fn assert_offers_pet_members(labels: &[String], shape: &str) {
    for want in ["getNickname", "getAge"] {
        assert!(
            labels.iter().any(|l| l == want),
            "{shape}: expected {want:?} among {labels:?}"
        );
    }
}

// ── The baseline: no lambda at all ───────────────────────────────────────────────────────────

/// If this fails, nothing below means anything — the fixture itself is wrong.
#[test]
fn a_plain_local_offers_its_members() {
    let (p, _s, off) = with_body("Pet HERE = null; HERE.");
    let labels = p.complete_labels("Use.java", off);
    assert_offers_pet_members(&labels, "plain local");
}

// ── Target-typed from a method parameter ─────────────────────────────────────────────────────

/// `repo.feed(p -> p.…)` — the interface is written out on the parameter (`Eater<Pet>`), so the
/// element type needs no inference at all. The simplest shape there is.
#[test]
fn a_lambda_param_typed_from_a_concrete_interface_parameter() {
    let (p, _s, off) = with_body("repo.feed(HERE -> HERE.);");
    let labels = p.complete_labels("Use.java", off);
    assert_offers_pet_members(&labels, "lambda on a concretely-typed parameter");
}

/// `holder.each(p -> p.…)` — the interface is `Eater<T>` and `T` comes from the RECEIVER's type
/// argument, so this exercises the generic substitution on the way to the SAM's parameter.
#[test]
fn a_lambda_param_typed_through_the_receivers_type_argument() {
    let (p, _s, off) = with_body("holder.each(HERE -> HERE.);");
    let labels = p.complete_labels("Use.java", off);
    assert_offers_pet_members(&labels, "lambda through the receiver's type argument");
}

/// The same, with the receiver itself coming from a call rather than a variable — one link of
/// chain before the lambda.
#[test]
fn a_lambda_param_typed_off_a_chained_receiver() {
    let (p, _s, off) = with_body("repo.pets().each(HERE -> HERE.);");
    let labels = p.complete_labels("Use.java", off);
    assert_offers_pet_members(&labels, "lambda off a chained receiver");
}

// ── Parenthesised, typed and block-bodied forms ──────────────────────────────────────────────

/// `(p) -> …` is the same lambda with `inferred_parameters` instead of a bare identifier — a
/// different grammar node, and so a different route to the parameter names.
#[test]
fn a_parenthesised_lambda_parameter_is_typed_the_same() {
    let (p, _s, off) = with_body("repo.feed((HERE) -> HERE.);");
    let labels = p.complete_labels("Use.java", off);
    assert_offers_pet_members(&labels, "parenthesised lambda parameter");
}

/// A block body puts a whole statement list between the parameter and the caret.
#[test]
fn a_block_bodied_lambda_is_typed_the_same() {
    let (p, _s, off) = with_body("repo.feed(HERE -> { HERE. });");
    let labels = p.complete_labels("Use.java", off);
    assert_offers_pet_members(&labels, "block-bodied lambda");
}

/// An explicitly-typed parameter needs no target typing at all — it says what it is.
#[test]
fn an_explicitly_typed_lambda_parameter_is_offered() {
    let (p, _s, off) = with_body("repo.feed((Pet HERE) -> HERE.);");
    let labels = p.complete_labels("Use.java", off);
    assert_offers_pet_members(&labels, "explicitly-typed lambda parameter");
}

// ── Nesting ──────────────────────────────────────────────────────────────────────────────────

/// A lambda inside a lambda: the scope walk has to stop at the lambda that declares the name it is
/// asked about, not at the first one it meets.
#[test]
fn a_lambda_nested_in_a_lambda_types_its_own_parameter() {
    let (p, _s, off) = with_body("repo.feed(outer -> repo.feed(HERE -> HERE.));");
    let labels = p.complete_labels("Use.java", off);
    assert_offers_pet_members(&labels, "inner lambda parameter");
}

/// …and the OUTER parameter is still typed when used from inside the inner lambda.
#[test]
fn an_outer_lambda_parameter_is_visible_from_the_inner_one() {
    let (p, _s, off) = with_body("repo.feed(HERE -> repo.feed(inner -> HERE.));");
    let labels = p.complete_labels("Use.java", off);
    assert_offers_pet_members(&labels, "outer parameter used inside an inner lambda");
}

// ── A call whose RESULT is inferred from the lambda ──────────────────────────────────────────

/// `convert` binds its result type `R` from the lambda it is handed, and typing a lambda's *return*
/// is not something this inference does. The lambda's **parameter** is a different question — it
/// comes from the receiver — and must still be typed.
///
/// Asserted rather than left implicit, because the two are easy to conflate: "the call's type is
/// unknown" is the documented edge of the feature; "the parameter's type is unknown" would be a bug.
#[test]
fn a_lambda_parameter_is_typed_even_when_the_calls_result_is_not() {
    let (p, _s, off) = with_body("holder.convert(HERE -> HERE.);");
    let labels = p.complete_labels("Use.java", off);
    assert_offers_pet_members(&labels, "parameter of a result-inferring call");
}

// ── Negative: an unresolvable receiver is silent, never a panic ───────────────────────────────

#[test]
fn an_unresolvable_lambda_parameter_yields_nothing_rather_than_panicking() {
    let (p, _s, off) = with_body("mystery(HERE -> HERE.);");
    let labels = p.complete_labels("Use.java", off);
    assert!(
        !labels.iter().any(|l| l == "getNickname"),
        "nothing is typed here, so nothing Pet-shaped should be offered — got {labels:?}"
    );
}
