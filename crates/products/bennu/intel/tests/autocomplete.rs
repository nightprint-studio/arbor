//! Autocompletion category — the member-access completion query (`receiver.` → candidates).
//!
//! Completion infers the receiver type at the caret, walks its members (own + superclass +
//! interfaces), prefix-filters by whatever is already typed, and returns items sorted
//! fields-then-methods (alphabetical within). These tests exercise the REAL query over a real
//! project index; the receiver types are all PROJECT-declared (their members are baked into the
//! index), so no live JDK is needed. A caret whose receiver can't be inferred yields `[]` — a
//! benign, non-fatal state (never a panic), which the negative cases assert.

mod common;
use common::*;

/// A small class hierarchy + a consumer riddled with completion trigger sites. Each trigger is
/// an incomplete `receiver.` statement (what the editor sees mid-keystroke); tree-sitter's
/// error recovery + the query's parse-repair stub keep the receiver resolvable.
fn zoo() -> Project {
    Project::new(&[
        (
            "Animal.java",
            "package zoo;\n\
             public class Animal {\n\
             \x20   protected String name;\n\
             \x20   public String speak() { return name; }\n\
             \x20   public int legs() { return 4; }\n\
             \x20   public int add(int a, int b) { return a + b; }\n\
             }\n",
        ),
        (
            "Dog.java",
            "package zoo;\n\
             public class Dog extends Animal {\n\
             \x20   private int barks;\n\
             \x20   public void bark() { }\n\
             \x20   public int fetch() { return barks; }\n\
             \x20   @Override public String speak() { return \"woof\"; }\n\
             }\n",
        ),
        (
            "Owner.java",
            "package zoo;\n\
             public class Owner {\n\
             \x20   private Dog pet;\n\
             \x20   public void play(Dog pooch) {\n\
             \x20       pooch.\n\
             \x20   }\n\
             \x20   public void chase(Dog pup) {\n\
             \x20       pup.fe\n\
             \x20   }\n\
             \x20   public void self() {\n\
             \x20       int n = this.\n\
             \x20   }\n\
             \x20   public void viaLocal() {\n\
             \x20       Dog local = new Dog();\n\
             \x20       local.\n\
             \x20   }\n\
             }\n",
        ),
    ])
}

/// Caret just after `pooch.` (empty prefix) in `play`.
fn at_pooch_dot(s: &str) -> usize {
    at(s, "pooch.") + "pooch.".len()
}

// ── Own + inherited members ──────────────────────────────────────────────────────────────────

#[test]
fn member_access_lists_own_members() {
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let labels = p.complete_labels("Owner.java", at_pooch_dot(&s));
    for own in ["bark", "fetch", "speak"] {
        assert!(labels.contains(&own.to_string()), "expected own member {own:?} in {labels:?}");
    }
}

#[test]
fn member_access_includes_inherited_members() {
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let labels = p.complete_labels("Owner.java", at_pooch_dot(&s));
    // Inherited from Animal: legs(), add(), and the protected field `name`.
    for inh in ["legs", "add", "name"] {
        assert!(labels.contains(&inh.to_string()), "expected inherited {inh:?} in {labels:?}");
    }
}

#[test]
fn overridden_method_appears_once() {
    // Dog overrides Animal.speak() — completion must dedup by name+kind, not offer two `speak`.
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let labels = p.complete_labels("Owner.java", at_pooch_dot(&s));
    let speaks = labels.iter().filter(|l| *l == "speak").count();
    assert_eq!(speaks, 1, "overridden speak() must appear exactly once, got {labels:?}");
}

// ── Prefix filtering ─────────────────────────────────────────────────────────────────────────

#[test]
fn typed_prefix_filters_candidates() {
    // `pup.fe` → only members starting with "fe" (fetch), nothing else.
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let off = at(&s, "pup.fe") + "pup.fe".len();
    let labels = p.complete_labels("Owner.java", off);
    assert!(labels.contains(&"fetch".to_string()), "fetch matches prefix fe, got {labels:?}");
    assert!(!labels.contains(&"bark".to_string()), "bark does not match prefix fe, got {labels:?}");
    assert!(!labels.contains(&"speak".to_string()), "speak does not match prefix fe, got {labels:?}");
}

#[test]
fn prefix_with_no_match_is_empty() {
    // A typed prefix that matches no member of the receiver yields no candidates.
    let q = Project::new(&[(
        "Q.java",
        "package q;\n\
         public class Q {\n\
         \x20   public int alpha() { return 1; }\n\
         \x20   void t() { this.zzz\n }\n\
         }\n",
    )]);
    let qs = q.source("Q.java").to_string();
    let off = at(&qs, "this.zzz") + "this.zzz".len();
    assert!(q.complete_labels("Q.java", off).is_empty(), "no member starts with zzz");
}

// ── this. and local receivers ────────────────────────────────────────────────────────────────

#[test]
fn this_dot_lists_enclosing_members() {
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let off = at(&s, "int n = this.") + "int n = this.".len();
    let labels = p.complete_labels("Owner.java", off);
    // Owner's own members declared before the (incomplete) trigger site parse cleanly: the field
    // `pet` + the methods play/chase. (Members after an unfinished `receiver.` in this same
    // synthetic buffer may not extract — that is an artifact of embedding the trigger in the
    // source, not a completion defect.)
    assert!(labels.contains(&"pet".to_string()), "this. offers field pet, got {labels:?}");
    assert!(labels.contains(&"play".to_string()), "this. offers method play, got {labels:?}");
    assert!(labels.contains(&"chase".to_string()), "this. offers method chase, got {labels:?}");
}

#[test]
fn local_variable_receiver_resolves() {
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let off = at(&s, "local.") + "local.".len();
    let labels = p.complete_labels("Owner.java", off);
    // `local` is a Dog → own + inherited members.
    assert!(labels.contains(&"fetch".to_string()), "local Dog offers fetch, got {labels:?}");
    assert!(labels.contains(&"legs".to_string()), "local Dog offers inherited legs, got {labels:?}");
}

// ── Visibility ───────────────────────────────────────────────────────────────────────────────

#[test]
fn private_member_hidden_from_external_receiver() {
    // `pooch` is a Dog parameter in Owner — an external receiver. `Dog.barks` is private, so it
    // must NOT be offered across classes (it was, before visibility filtering).
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let labels = p.complete_labels("Owner.java", at_pooch_dot(&s));
    assert!(
        !labels.contains(&"barks".to_string()),
        "private field must be hidden from an external receiver, got {labels:?}"
    );
}

#[test]
fn private_member_shown_within_same_class() {
    // Within its own class body, a private member IS accessible and must still be offered.
    let d = Project::new(&[(
        "Cat.java",
        "package zoo;\n\
         public class Cat {\n\
         \x20   private int lives;\n\
         \x20   public int look() { return this.\n }\n\
         }\n",
    )]);
    let s = d.source("Cat.java").to_string();
    let off = at(&s, "this.") + "this.".len();
    let labels = d.complete_labels("Cat.java", off);
    assert!(
        labels.contains(&"lives".to_string()),
        "private field visible within its own class, got {labels:?}"
    );
}

// ── Detail rendering ─────────────────────────────────────────────────────────────────────────

#[test]
fn method_detail_has_signature_shape() {
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let items = p.complete("Owner.java", at_pooch_dot(&s));
    let add = items.iter().find(|c| c.label == "add").expect("add present");
    let detail = add.detail.clone().unwrap_or_default();
    // `add(int a, int b) : int` — the renderer strips packages and joins params.
    assert!(detail.starts_with("add("), "detail begins with the method name, got {detail:?}");
    assert!(detail.contains(':'), "method detail has a return-type separator, got {detail:?}");
    assert!(detail.contains("int"), "add's types are int, got {detail:?}");
    assert_eq!(add.kind, "method");
}

#[test]
fn field_detail_is_the_type() {
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let items = p.complete("Owner.java", at_pooch_dot(&s));
    let name = items.iter().find(|c| c.label == "name").expect("inherited field name present");
    assert_eq!(name.kind, "field");
    let detail = name.detail.clone().unwrap_or_default();
    assert!(detail.contains("String"), "field name is a String, got {detail:?}");
}

#[test]
fn fields_are_offered_before_methods() {
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let items = p.complete("Owner.java", at_pooch_dot(&s));
    // The query sorts by kind ("field" < "method") then label; every field precedes every method.
    let first_method = items.iter().position(|c| c.kind == "method");
    let last_field = items.iter().rposition(|c| c.kind == "field");
    if let (Some(fm), Some(lf)) = (first_method, last_field) {
        assert!(lf < fm, "all fields must precede all methods, got {:?}", items);
    }
}

// ── Negative / robustness ────────────────────────────────────────────────────────────────────

#[test]
fn bare_identifier_without_receiver_is_empty() {
    // A caret inside a bare identifier (no preceding `receiver.`) offers no member completions.
    let p = zoo();
    let s = p.source("Dog.java").to_string();
    let off = at(&s, "barks;") + 2; // inside `barks`
    assert!(p.complete_labels("Dog.java", off).is_empty(), "no member access → no completions");
}

#[test]
fn unresolvable_receiver_is_empty() {
    let p = Project::new(&[(
        "U.java",
        "package u;\n\
         public class U {\n\
         \x20   void t(Unknown m) { m.\n }\n\
         }\n",
    )]);
    let s = p.source("U.java").to_string();
    let off = at(&s, "m.\n") + 2;
    assert!(p.complete_labels("U.java", off).is_empty(), "unknown receiver type → []");
}

#[test]
fn completion_at_offset_zero_does_not_panic() {
    let p = zoo();
    let _ = p.complete("Owner.java", 0);
}

#[test]
fn completion_on_broken_file_does_not_panic() {
    let p = Project::new(&[(
        "Broken.java",
        "package b;\n\
         public class Broken {\n\
         \x20   void oops( { int y = this.\n",
    )]);
    let s = p.source("Broken.java").to_string();
    let off = at(&s, "this.") + "this.".len();
    let _ = p.complete("Broken.java", off);
}

#[test]
fn completion_on_empty_file_does_not_panic() {
    let p = Project::new(&[("Empty.java", "")]);
    assert!(p.complete("Empty.java", 0).is_empty());
}

#[test]
fn completion_offset_past_end_is_clamped() {
    // split_prefix clamps to source length; an out-of-range-style offset must not panic.
    let p = zoo();
    let s = p.source("Owner.java").to_string();
    let _ = p.complete("Owner.java", s.len());
}

// ── Enum constants ───────────────────────────────────────────────────────────────────────────

/// A constant is a member of its enum — `public static final E NAME` — and the index must carry
/// it as one. It didn't, so a project enum looked constant-less everywhere it mattered: nothing
/// completed after `Color.`, `import static p.Color.*` supplied no bare name (the undefined-variable
/// check then called correct code undefined), and switch exhaustiveness gave up on every project
/// enum for want of a constant to check against.
#[test]
fn enum_constants_complete_after_the_enum_name() {
    let p = Project::new(&[
        (
            "Color.java",
            "package p;\n\
             public enum Color {\n\
             \x20   RED, GREEN(\"g\"), BLUE;\n\
             \x20   private String tag;\n\
             \x20   Color() { }\n\
             \x20   Color(String t) { this.tag = t; }\n\
             \x20   public String tag() { return tag; }\n\
             }\n",
        ),
        (
            "Use.java",
            "package p;\n\
             public class Use {\n\
             \x20   void m() {\n\
             \x20       Color c = Color.\n\
             \x20   }\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "= Color.") + "= Color.".len();
    let labels = p.complete_labels("Use.java", off);
    for c in ["RED", "GREEN", "BLUE"] {
        assert!(labels.contains(&c.to_string()), "expected constant {c:?} in {labels:?}");
    }
    // The enum's own members are still there — the constants are additions, not a replacement.
    assert!(labels.contains(&"tag".to_string()), "enum method still offered: {labels:?}");
}
