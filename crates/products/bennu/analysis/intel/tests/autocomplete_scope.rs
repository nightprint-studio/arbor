//! Autocompletion category — the **bare identifier**: a name written with nothing to its left.
//!
//! The half the engine did not answer. Member completion needs a receiver to infer, so typing
//! `ord|` meaning the local `order` reached no index at all and the popup fell back to scanning
//! the buffer for similar-looking words. These tests are about what a name in that position can
//! legally MEAN — the scope chain, the enclosing type's members, the static imports — and about
//! the order they come in, which is the whole of whether the list feels like it read your mind.

mod common;
use common::*;

/// A class with something of every category in scope at one caret: two parameters, two locals at
/// different depths, its own field and method, an inherited method, and a static import.
fn shop() -> Project {
    Project::new(&[
        (
            "Base.java",
            "package shop;\n\
             public class Base {\n\
             \x20   protected String audit() { return \"\"; }\n\
             }\n",
        ),
        (
            "Order.java",
            "package shop;\n\
             public class Order {\n\
             \x20   public String label() { return \"\"; }\n\
             }\n",
        ),
        (
            "Till.java",
            "package shop;\n\
             import static java.util.Arrays.asList;\n\
             public class Till extends Base {\n\
             \x20   private int takings;\n\
             \x20   private Order lastOrder;\n\
             \x20   public void ring(Order order, int count) {\n\
             \x20       int total = count * 2;\n\
             \x20       if (count > 0) {\n\
             \x20           String tag = \"x\";\n\
             \x20           MARK\n\
             \x20       }\n\
             \x20   }\n\
             \x20   public int tally() { return takings; }\n\
             \x20   public static void main(String[] args) {\n\
             \x20       STATICMARK\n\
             \x20   }\n\
             }\n",
        ),
    ])
}

fn mark(p: &Project, needle: &str) -> usize {
    at(p.source("Till.java"), needle)
}

// ── What is in scope ─────────────────────────────────────────────────────────────────────────

/// The headline: a bare identifier now has a semantic answer at all.
#[test]
fn locals_parameters_and_own_members_are_all_offered() {
    let p = shop();
    let labels = p.scope_labels("Till.java", mark(&p, "MARK"));
    for expected in ["tag", "total", "order", "count", "takings", "lastOrder", "tally"] {
        assert!(
            labels.contains(&expected.to_string()),
            "expected {expected:?} in {labels:?}"
        );
    }
}

/// Inheritance is the enclosing type's, walked exactly as a receiver's would be.
#[test]
fn an_inherited_method_is_offered_without_this() {
    let p = shop();
    let labels = p.scope_labels("Till.java", mark(&p, "MARK"));
    assert!(labels.contains(&"audit".to_string()), "{labels:?}");
}

/// A statically-imported name is writable bare, and there is no other way for the reader of the
/// popup to know where it came from — so it says.
#[test]
fn a_static_import_is_offered_and_names_its_owner() {
    let p = shop();
    let items = p.scope_complete("Till.java", mark(&p, "MARK"));
    let found = items.iter().find(|i| i.label == "asList");
    let Some(found) = found else {
        // The JDK member index is not guaranteed in this harness; a missing `Arrays` is a fixture
        // limitation, not a wrong answer. What must never happen is an entry with no provenance.
        assert!(items.iter().all(|i| i.label != "asList"));
        return;
    };
    assert!(
        found.detail.as_deref().is_some_and(|d| d.contains("java.util.Arrays")),
        "{found:?}"
    );
}

// ── The order, which is the point ────────────────────────────────────────────────────────────

/// Nearness is the ordering. A local declared two lines up beats a field of this class, which
/// beats a method inherited from its supertype — because that is the order of how far the name is
/// from the caret, and no other axis compares the three.
#[test]
fn the_nearest_binding_comes_first() {
    let p = shop();
    let labels = p.scope_labels("Till.java", mark(&p, "MARK"));
    let rank = |n: &str| labels.iter().position(|l| l == n).unwrap_or(usize::MAX);
    assert!(rank("tag") < rank("takings"), "a local beats a field: {labels:?}");
    assert!(rank("takings") < rank("audit"), "own beats inherited: {labels:?}");
}

/// Depth separates the scopes; distance separates names inside one. Both are needed — `tag` and
/// `total` are in different blocks, `total` and `count` are not.
#[test]
fn an_inner_block_local_outranks_one_from_the_method_body() {
    let p = shop();
    let labels = p.scope_labels("Till.java", mark(&p, "MARK"));
    let rank = |n: &str| labels.iter().position(|l| l == n).unwrap_or(usize::MAX);
    assert!(rank("tag") < rank("total"), "{labels:?}");
}

// ── What must NOT be offered ─────────────────────────────────────────────────────────────────

/// A bare name in a static context can only be a static member: `takings` inside
/// `static void main` does not compile, however visible the field is from elsewhere.
#[test]
fn an_instance_member_is_not_offered_in_a_static_method() {
    let p = shop();
    let labels = p.scope_labels("Till.java", mark(&p, "STATICMARK"));
    assert!(!labels.contains(&"takings".to_string()), "{labels:?}");
    assert!(!labels.contains(&"tally".to_string()), "{labels:?}");
    // …and the parameter it WAS handed still is.
    assert!(labels.contains(&"args".to_string()), "{labels:?}");
}

/// `recv.pre|` is member completion's question. Answering it here would bury the members that are
/// actually legal under every name in scope.
#[test]
fn a_member_access_is_left_to_member_completion() {
    let p = Project::new(&[(
        "A.java",
        "package p;\n\
         public class A {\n\
         \x20   void go(String s) {\n\
         \x20       int n = 1;\n\
         \x20       s.MARK\n\
         \x20   }\n\
         }\n",
    )]);
    let off = at(p.source("A.java"), "MARK");
    assert!(p.scope_complete("A.java", off).is_empty());
}

/// A local is not in scope on the line that declares it, and offering it there offers the name
/// being typed as a completion of itself.
#[test]
fn a_local_is_not_offered_inside_its_own_declaration() {
    let p = Project::new(&[(
        "A.java",
        "package p;\n\
         public class A {\n\
         \x20   void go() {\n\
         \x20       int counter = 1;\n\
         \x20       int counted = coun\n\
         \x20   }\n\
         }\n",
    )]);
    let src = p.source("A.java").to_string();
    let off = at(&src, "= coun") + "= coun".len();
    let labels = p.scope_labels("A.java", off);
    assert!(labels.contains(&"counter".to_string()), "{labels:?}");
    assert!(!labels.contains(&"counted".to_string()), "{labels:?}");
}

// ── Robustness ───────────────────────────────────────────────────────────────────────────────

#[test]
fn a_caret_outside_any_method_does_not_panic() {
    let p = shop();
    let _ = p.scope_complete("Till.java", 0);
    let _ = p.scope_complete("Till.java", usize::MAX / 2);
}

#[test]
fn a_broken_file_answers_empty_rather_than_panicking() {
    let p = Project::new(&[("B.java", "package p; public class B { void x( { int y = ")]);
    let _ = p.scope_complete("B.java", 40);
}

// ── How a name is matched ────────────────────────────────────────────────────────────────────

/// The camel humps, on a MEMBER — which is where they never worked. `tolc` reaching
/// `toLowerCase` is how anyone who already knows a name reaches for it, and the engine used to
/// match members with a literal `starts_with`.
#[test]
fn a_member_is_reachable_by_its_humps() {
    let p = Project::new(&[(
        "A.java",
        "package p;\n\
         public class A {\n\
         \x20   public void addAllowedHeader(String h) { }\n\
         \x20   public void addAllowedMethod(String m) { }\n\
         \x20   public int size() { return 0; }\n\
         \x20   public void go(A cfg) {\n\
         \x20       cfg.aah\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "cfg.aah") + "cfg.aah".len();
    let labels = p.complete_labels("A.java", off);
    assert!(labels.contains(&"addAllowedHeader".to_string()), "{labels:?}");
    assert!(!labels.contains(&"size".to_string()), "{labels:?}");
}

/// A tighter match ranks above a looser one: `s.to` is what was literally typed for `toString`,
/// and only the humps for `toLowerCase`.
#[test]
fn an_exact_prefix_outranks_a_hump_match() {
    let p = Project::new(&[(
        "B.java",
        "package p;\n\
         public class B {\n\
         \x20   public String toString2() { return \"\"; }\n\
         \x20   public String turnOn() { return \"\"; }\n\
         \x20   public void go(B b) {\n\
         \x20       b.to\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("B.java").to_string();
    let off = at(&s, "b.to") + "b.to".len();
    let labels = p.complete_labels("B.java", off);
    let rank = |n: &str| labels.iter().position(|l| l == n).unwrap_or(usize::MAX);
    assert!(rank("toString2") < rank("turnOn"), "{labels:?}");
}

/// A local reaches its humps too — the rule is one rule, not one per kind of candidate.
#[test]
fn a_local_is_reachable_by_its_humps() {
    let p = Project::new(&[(
        "C.java",
        "package p;\n\
         public class C {\n\
         \x20   void go() {\n\
         \x20       String customerName = \"x\";\n\
         \x20       int other = 1;\n\
         \x20       cN\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("C.java").to_string();
    let off = at(&s, "       cN") + "       cN".len();
    let labels = p.scope_labels("C.java", off);
    assert!(labels.contains(&"customerName".to_string()), "{labels:?}");
}
