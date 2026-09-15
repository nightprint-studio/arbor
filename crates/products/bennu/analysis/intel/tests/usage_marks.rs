//! Usage counts — the number drawn above every declaration, and the fading of the ones nothing
//! reaches.
//!
//! Two halves worth testing, and neither is the arithmetic. A name greyed out wrongly is an
//! invitation to delete working code; a "no usages" above every method of a test file is an editor
//! that has learnt to say something true and useless. So most of what follows is about the three
//! answers — used, unused, and *not worth a word* — rather than about the counting.

mod common;
use common::*;

// ── the count itself ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_method_is_counted_wherever_it_is_called() {
    let p = Project::new(&[
        ("Svc.java", "package app;\npublic class Svc {\n    public int run() { return 1; }\n}\n"),
        (
            "Caller.java",
            "package app;\npublic class Caller {\n    int a(Svc s) { return s.run(); }\n    int b(Svc s) { return s.run(); }\n}\n",
        ),
    ]);
    assert_eq!(p.mark("Svc.java", "run").count, 2);
}

#[test]
fn the_declaration_is_not_one_of_its_own_uses() {
    let p = Project::new(&[(
        "Solo.java",
        "package app;\npublic class Solo {\n    private int n;\n    int get() { return 0; }\n}\n",
    )]);
    assert_eq!(p.mark("Solo.java", "n").count, 0);
    assert_eq!(p.mark("Solo.java", "get").count, 0);
}

#[test]
fn the_span_is_the_name_and_the_row_goes_above_the_declaration() {
    let src = "package app;\npublic class A {\n    private int total;\n}\n";
    let p = Project::new(&[("A.java", src)]);
    let m = p.mark("A.java", "total");
    assert_eq!(&src[m.start..m.end], "total");
    // The row is drawn above the declaration, which begins at `private`, not at the name.
    assert!(m.decl < m.start, "the row must anchor above the whole declaration");
    assert_eq!(&src[m.decl..m.decl + 7], "private");
}

#[test]
fn two_names_on_one_line_are_two_declarations() {
    let p = Project::new(&[(
        "A.java",
        "package app;\npublic class A {\n    private int a, b;\n    int use() { return a; }\n}\n",
    )]);
    assert_eq!(p.mark("A.java", "a").count, 1);
    assert_eq!(p.mark("A.java", "b").count, 0);
}

#[test]
fn a_nested_type_and_its_members_are_reported_too() {
    let p = Project::new(&[(
        "Outer.java",
        "package app;\npublic class Outer {\n    public static class Inner {\n        int deep;\n    }\n}\n",
    )]);
    let names: Vec<String> = p.usage_marks("Outer.java").into_iter().map(|m| m.name).collect();
    assert!(names.contains(&"Inner".to_string()), "{names:?}");
    assert!(names.contains(&"deep".to_string()), "{names:?}");
}

// ── when zero does NOT mean unused ───────────────────────────────────────────────────────────

#[test]
fn a_bean_factory_is_not_drawn_at_all() {
    // The case the whole rule exists for: a `@Bean` method is called by Spring, by reflection, and
    // no index anywhere can see that. Greying it would be telling someone to delete their
    // application context — and saying "no usages" about it is true and useless.
    let p = Project::new(&[(
        "Cfg.java",
        "package app;\npublic class Cfg {\n    @Bean\n    public String thing() { return \"x\"; }\n}\n",
    )]);
    let m = p.mark("Cfg.java", "thing");
    assert_eq!(m.count, 0);
    assert!(!m.is_unused(), "an annotated method must never be drawn as unused");
    assert!(m.is_silent(), "and a framework's own entry point is not worth a row");
}

#[test]
fn a_bean_factory_another_one_calls_still_shows_its_count() {
    // Silence is for a count of ZERO. A `@Bean` method called from another `@Bean` method in the
    // same configuration is an ordinary call, and the number is exactly what somebody wants.
    let p = Project::new(&[(
        "Cfg.java",
        "package app;\npublic class Cfg {\n\
         \x20   @Bean\n    public String thing() { return \"x\"; }\n\
         \x20   @Bean\n    public String other() { return thing(); }\n}\n",
    )]);
    let m = p.mark("Cfg.java", "thing");
    assert_eq!(m.count, 1);
    assert!(!m.is_silent(), "a count worth having is worth drawing");
}

// ── the noise this used to make ───────────────────────────────────────────────────────────────

#[test]
fn nothing_is_said_about_a_test_method_or_its_lifecycle_neighbours() {
    let p = Project::new(&[(
        "OrderTest.java",
        "package app;\npublic class OrderTest {\n\
         \x20   @BeforeEach\n    void setUp() { }\n\
         \x20   @AfterEach\n    void tearDown() { }\n\
         \x20   @Test\n    void totals() { }\n}\n",
    )]);
    for name in ["setUp", "tearDown", "totals"] {
        let m = p.mark("OrderTest.java", name);
        assert!(m.is_silent(), "`{name}` is run by JUnit — there is nothing to say about it");
        assert!(!m.is_unused(), "and it is certainly not dead");
    }
}

#[test]
fn nothing_is_said_about_the_test_class_either() {
    // And without guessing from its name: a class holding `@Test` methods is a class JUnit
    // instantiates, which is derivable from what is in it.
    let p = Project::new(&[(
        "OrderTest.java",
        "package app;\npublic class OrderTest {\n    @Test\n    void totals() { }\n}\n",
    )]);
    let m = p.mark("OrderTest.java", "OrderTest");
    assert!(m.is_silent(), "a class the test runner builds is not a class nobody uses");
}

#[test]
fn a_class_named_like_a_test_but_holding_none_is_still_judged() {
    // The other half of not guessing from the name: `TestUtils` with no test in it is an ordinary
    // class, and if nothing uses it that is worth knowing.
    let p = Project::new(&[(
        "TestUtils.java",
        "package app;\npublic class TestUtils {\n    void helper() { }\n}\n",
    )]);
    assert!(p.mark("TestUtils.java", "TestUtils").is_unused());
}

#[test]
fn nothing_is_said_about_a_spring_component_or_a_web_handler() {
    let p = Project::new(&[(
        "OrderController.java",
        "package app;\n@RestController\npublic class OrderController {\n\
         \x20   @GetMapping\n    public String list() { return \"\"; }\n}\n",
    )]);
    assert!(p.mark("OrderController.java", "OrderController").is_silent());
    assert!(p.mark("OrderController.java", "list").is_silent());
}

#[test]
fn an_annotation_that_only_describes_a_method_still_shows_its_count() {
    // `@Deprecated` makes nothing call it. A deprecated method nobody calls is exactly what
    // somebody is looking for, so the row stays — it is only the greying that is withheld.
    let p = Project::new(&[(
        "A.java",
        "package app;\npublic class A {\n    @Deprecated\n    void old() { }\n}\n",
    )]);
    let m = p.mark("A.java", "old");
    assert!(!m.is_silent(), "an annotation that describes rather than dispatches says nothing");
    assert!(!m.is_unused(), "but no index can vouch for a count of zero on anything annotated");
}

#[test]
fn the_entry_point_is_found_wherever_it_sits_among_the_annotations() {
    // `@Override @Bean` is ordinary, and reading only the first annotation would miss Spring.
    let p = Project::new(&[(
        "Cfg.java",
        "package app;\npublic class Cfg {\n    @Override\n    @Bean\n    public String thing() { return \"\"; }\n}\n",
    )]);
    assert!(p.mark("Cfg.java", "thing").is_silent());
}

#[test]
fn a_qualified_annotation_is_recognised_by_its_last_segment() {
    let p = Project::new(&[(
        "T.java",
        "package app;\npublic class T {\n    @org.junit.jupiter.api.Test\n    void runs() { }\n}\n",
    )]);
    assert!(p.mark("T.java", "runs").is_silent());
}

#[test]
fn an_annotated_field_is_never_called_dead_but_its_count_is_still_shown() {
    // A field is not *called*, so no framework list applies to one. An `@Autowired` field nothing
    // reads is a real finding, and silencing it would hide the thing worth seeing.
    let p = Project::new(&[(
        "Svc.java",
        "package app;\npublic class Svc {\n    @Autowired\n    private String repo;\n}\n",
    )]);
    let m = p.mark("Svc.java", "repo");
    assert!(!m.is_unused());
    assert!(!m.is_silent(), "the count of an injected field nothing reads is worth drawing");
}

#[test]
fn an_annotation_inside_the_body_is_not_an_annotation_on_the_declaration() {
    // Read off the modifiers, not off the text: a method whose body happens to mention an
    // annotation is still a method nothing calls.
    let p = Project::new(&[(
        "A.java",
        "package app;\npublic class A {\n    void dead() { Class<?> c = Deprecated.class; }\n}\n",
    )]);
    assert!(p.mark("A.java", "dead").is_unused());
}

#[test]
fn nothing_is_said_about_main() {
    let p = Project::new(&[(
        "App.java",
        "package app;\npublic class App {\n    public static void main(String[] args) { }\n}\n",
    )]);
    let m = p.mark("App.java", "main");
    assert!(!m.is_unused(), "main must never be drawn as unused");
    assert!(m.is_silent(), "and the JVM calling it is not news");
}

#[test]
fn the_serialization_protocol_is_read_by_the_runtime() {
    let p = Project::new(&[(
        "A.java",
        "package app;\npublic class A {\n    private static final long serialVersionUID = 1L;\n}\n",
    )]);
    assert!(!p.mark("A.java", "serialVersionUID").is_unused());
}

#[test]
fn read_object_is_reached_the_same_way_and_from_the_same_list() {
    // Not a second rule: `bennu-check`'s unused-member check owns the list, and this is the proof
    // that the counts read it rather than keeping one of their own.
    let p = Project::new(&[(
        "A.java",
        "package app;\npublic class A {\n    private void readObject(java.io.ObjectInputStream s) { }\n}\n",
    )]);
    assert!(!p.mark("A.java", "readObject").is_unused());
}

#[test]
fn an_implementation_of_an_interface_method_is_reached_through_the_interface() {
    // The failure mode that would have made this feature unusable on a real project: a call goes
    // through the interface, so the index keys it to the INTERFACE, and every implementation in
    // the codebase reads as having zero uses.
    let p = Project::new(&[
        ("Job.java", "package app;\npublic interface Job {\n    void run();\n}\n"),
        (
            "Nightly.java",
            "package app;\npublic class Nightly implements Job {\n    public void run() { }\n}\n",
        ),
        (
            "Runner.java",
            "package app;\npublic class Runner {\n    void go(Job j) { j.run(); }\n}\n",
        ),
    ]);
    assert_eq!(p.mark("Job.java", "run").count, 1, "the interface's method is the one called");
    let impl_mark = p.mark("Nightly.java", "run");
    assert_eq!(impl_mark.count, 0, "the implementation is not what the call is keyed to");
    assert!(!impl_mark.is_unused(), "and it must not be drawn as unused because of that");
    assert!(impl_mark.is_silent(), "nor carry a zero that says nothing about the program");
}

#[test]
fn an_interface_method_nothing_implements_or_calls_is_unused() {
    // The other direction has to still work, or the rule above would simply switch the feature off
    // for every hierarchy.
    let p = Project::new(&[("Job.java", "package app;\npublic interface Job {\n    void never();\n}\n")]);
    assert!(p.mark("Job.java", "never").is_unused());
}

#[test]
fn a_constructor_is_not_reported_at_all() {
    // Its callers are `new` expressions, which the index keys by the type. A count of zero here
    // would be a fact about the index that reads as a fact about the program.
    let p = Project::new(&[(
        "A.java",
        "package app;\npublic class A {\n    public A() { }\n    void m() { }\n}\n",
    )]);
    let names: Vec<String> = p.usage_marks("A.java").into_iter().map(|m| m.name).collect();
    assert_eq!(names, ["A", "m"], "the constructor must not appear as a member");
}

// ── what SHOULD go grey ──────────────────────────────────────────────────────────────────────

#[test]
fn a_plain_private_helper_nothing_calls_is_unused() {
    let p = Project::new(&[(
        "A.java",
        "package app;\npublic class A {\n    private void helper() { }\n    void used() { }\n}\n",
    )]);
    let m = p.mark("A.java", "helper");
    assert_eq!(m.count, 0);
    assert!(m.is_unused(), "this is the case the greying exists for");
    assert!(!m.is_silent(), "and it is the one thing that must always be said");
}

#[test]
fn a_field_read_only_where_it_is_declared_is_still_unused() {
    let p = Project::new(&[(
        "A.java",
        "package app;\npublic class A {\n    private int spare = 0;\n}\n",
    )]);
    assert!(p.mark("A.java", "spare").is_unused());
}
