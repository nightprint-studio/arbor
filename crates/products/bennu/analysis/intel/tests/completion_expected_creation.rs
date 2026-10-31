//! The position wants a class: the ghost text writes how the code makes one.

mod common;
use common::*;

fn project(body: &str) -> Project {
    Project::with_stream_jdk(&[
        ("Pair.java", "package z;\npublic class Pair<L, R> {\n    public static <L, R> Pair<L, R> of(L l, R r) { return null; }\n}\n"),
        ("Tabellato.java", "package z;\npublic class Tabellato { }\n"),
        ("Plain.java", "package z;\npublic class Plain {\n    public Plain(int x) { }\n}\n"),
        ("Svc.java", "package z;\npublic class Svc {\n    public void take(Pair<String, String> p, Plain q, Tabellato t) { }\n}\n"),
        (
            "Use.java",
            &format!("package z;\nimport java.util.*;\npublic class Use {{\n    Tabellato tabellato(String code, String name) {{ return null; }}\n    void run(Svc svc) {{\n        {body}\n    }}\n}}\n"),
        ),
    ])
}

fn creation(body: &str, trigger: &str) -> Option<String> {
    let p = project(body);
    let off = at(p.source("Use.java"), trigger) + trigger.len();
    p.expected_creation("Use.java", off)
}

#[test]
fn a_static_factory_of_the_wanted_class() {
    assert_eq!(creation("svc.take(\n", "svc.take("), Some("Pair.of(".to_string()));
}

#[test]
fn a_method_of_this_file_that_returns_the_class_is_how_the_code_writes_one() {
    assert_eq!(creation("svc.take(null, null, \n", "svc.take(null, null, "), Some("tabellato(".to_string()));
}

#[test]
fn a_constructor_when_there_is_no_factory() {
    assert_eq!(creation("svc.take(null, \n", "svc.take(null, "), Some("new Plain(".to_string()));
}

#[test]
fn the_typed_start_must_fit_and_a_member_access_gives_nothing() {
    assert_eq!(creation("svc.take(Pa\n", "svc.take(Pa"), Some("Pair.of(".to_string()));
    assert_eq!(creation("svc.take(Zz\n", "svc.take(Zz"), None);
    assert_eq!(creation("svc.take(x.\n", "svc.take(x."), None);
}

fn generic_project(body: &str) -> Project {
    Project::with_stream_jdk(&[
        ("Pair.java", "package z;\npublic class Pair<L, R> {\n    public static <L, R> Pair<L, R> of(L l, R r) { return null; }\n}\n"),
        ("Tabellato.java", "package z;\npublic class Tabellato { }\n"),
        ("Box.java", "package z;\npublic class Box<E> {\n    public static <E> Box<E> of(E e) { return null; }\n}\n"),
        ("Kind.java", "package z;\npublic enum Kind { NAZIONI }\n"),
        ("Bundle.java", "package z;\npublic interface Bundle<E> { }\n"),
        ("Bag.java", "package z;\npublic class Bag {\n    public static <T> boolean addAll(Bundle<? super T> c, T... xs) { return true; }\n}\n"),
        (
            "Use.java",
            &format!("package z;\nimport java.util.*;\npublic class Use {{\n    void run(Bundle<Pair<Kind, Box<Tabellato>>> items) {{\n        {body}\n    }}\n}}\n"),
        ),
    ])
}

fn generic_creation(body: &str, trigger: &str) -> Option<String> {
    let p = generic_project(body);
    let off = at(p.source("Use.java"), trigger) + trigger.len();
    p.expected_creation("Use.java", off)
}

#[test]
fn a_type_variable_takes_what_the_first_argument_holds() {
    assert_eq!(generic_creation("Bag.addAll(items, \n", "Bag.addAll(items, "), Some("Pair.of(".to_string()));
}

#[test]
fn a_free_variable_takes_what_the_call_around_it_wants() {
    // `R` of `Pair.of` is free; the outer slot wants `Pair<Kind, Box<Tabellato>>`, so it is a `Box`
    assert_eq!(generic_creation("Bag.addAll(items, Pair.of(Kind.NAZIONI, \n", "Pair.of(Kind.NAZIONI, "), Some("Box.of(".to_string()));
}

fn overloaded_project(body: &str) -> Project {
    Project::with_stream_jdk(&[
        ("Entry.java", "package z;\npublic class Entry<K, V> {\n    public static <K, V> Entry<K, V> copyOf(Entry<K, V> e) { return null; }\n}\n"),
        (
            "Pair.java",
            "package z;\npublic class Pair<L, R> {\n    public static <L, R> Pair<L, R> of(Entry<L, R> e) { return null; }\n    public static <L, R> Pair<L, R> of(L l, R r) { return null; }\n}\n",
        ),
        ("Kind.java", "package z;\npublic enum Kind { NAZIONI }\n"),
        ("Bundle.java", "package z;\npublic interface Bundle<E> { }\n"),
        ("Bag.java", "package z;\npublic class Bag {\n    public static <T> boolean addAll(Bundle<? super T> c, T... xs) { return true; }\n}\n"),
        (
            "Use.java",
            &format!("package z;\npublic class Use {{\n    void run(Bundle<Pair<Kind, String>> items) {{\n        {body}\n    }}\n}}\n"),
        ),
    ])
}

/// `Pair.of(|` with the closer of the OUTER call on the next line is not a one-argument call yet: `of(L, R)` is in the
/// running as much as `of(Entry<L, R>)`, so nothing is certain enough to draw.
#[test]
fn overloads_that_disagree_about_the_first_argument_give_no_creation() {
    let body = "Bag.addAll(items,\n            Pair.of(\n        );";
    let p = overloaded_project(body);
    let trigger = "Pair.of(";
    let off = at(p.source("Use.java"), trigger) + trigger.len();
    assert_eq!(p.expected_creation("Use.java", off), None);
}
