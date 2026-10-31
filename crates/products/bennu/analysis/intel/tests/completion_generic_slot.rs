//! A parameter that is a type variable (`<T> Matcher<T> helper(T value)`) takes what the call around it makes of `T`:
//! `.field(helper(|))` inside a slot that wants a `Matcher<Color>` takes a `Color`, whatever the project passes most
//! often to `helper`. Two enums, one generic helper, one outer method per enum.

mod common;
use bennu_query::prelude::GenericSlot;
use common::*;

fn project(body: &str) -> Project {
    Project::new(&[
        ("Color.java", "package z;\npublic enum Color { RED, GREEN }\n"),
        ("Size.java", "package z;\npublic enum Size { SMALL, LARGE }\n"),
        ("Matcher.java", "package z;\npublic class Matcher<T> { }\n"),
        ("Helper.java", "package z;\npublic class Helper {\n    public static <T> Matcher<T> same(T value) { return null; }\n}\n"),
        (
            "Query.java",
            "package z;\npublic class Query {\n    public Query color(Matcher<Color> m) { return this; }\n    public Query size(Matcher<Size> m) { return this; }\n    public Query name(String n) { return this; }\n    public void plain(Color c) { }\n}\n",
        ),
        ("Use.java", &format!("package z;\npublic class Use {{\n    void run(Query q) {{\n        {body}\n    }}\n}}\n")),
    ])
}

fn after(p: &Project, trigger: &str) -> usize {
    at(p.source("Use.java"), trigger) + trigger.len()
}

fn labels(body: &str, trigger: &str) -> Vec<String> {
    let p = project(body);
    let off = after(&p, trigger);
    p.enum_argument_items("Use.java", off).into_iter().map(|i| i.label).collect()
}

#[test]
fn the_variable_is_settled_by_the_outer_call() {
    let p = project("q.color(Helper.same());");
    match p.generic_slot("Use.java", after(&p, "Helper.same(")) {
        GenericSlot::Settled(t) => assert_eq!(t.binary_name, "z/Color"),
        other => panic!("{other:?}"),
    }
    let p = project("q.size(Helper.same());");
    match p.generic_slot("Use.java", after(&p, "Helper.same(")) {
        GenericSlot::Settled(t) => assert_eq!(t.binary_name, "z/Size"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_variable_nothing_settles_is_free_and_a_concrete_parameter_is_fixed() {
    let p = project("Object o = Helper.same();");
    assert_eq!(p.generic_slot("Use.java", after(&p, "Helper.same(")), GenericSlot::Free);
    let p = project("q.plain();");
    assert_eq!(p.generic_slot("Use.java", after(&p, "q.plain(")), GenericSlot::Fixed);
}

#[test]
fn a_constant_fits_only_the_enum_it_belongs_to() {
    let p = project("q.color(Helper.same());");
    let off = after(&p, "Helper.same(");
    let color = bennu_java::prelude::TypeRef { binary_name: "z/Color".into(), ..Default::default() };
    // the project writes `Size.SMALL` most often to `same`; here the outer call wants a Color
    assert_eq!(p.text_fits("Use.java", off, off, "Color.RED", &color), Some(true));
    assert_eq!(p.text_fits("Use.java", off, off, "Size.SMALL", &color), Some(false));
}

#[test]
fn the_popup_offers_the_constants_of_the_enum_the_outer_call_wants() {
    let got = labels("q.color(Helper.same());", "Helper.same(");
    assert!(got.iter().any(|l| l == "Color.RED") && got.iter().all(|l| !l.starts_with("Size.")), "{got:?}");
    let got = labels("q.size(Helper.same());", "Helper.same(");
    assert!(got.iter().any(|l| l == "Size.LARGE") && got.iter().all(|l| !l.starts_with("Color.")), "{got:?}");
}

#[test]
fn a_free_variable_offers_no_enum() {
    assert!(labels("Object o = Helper.same();", "Helper.same(").is_empty());
}
