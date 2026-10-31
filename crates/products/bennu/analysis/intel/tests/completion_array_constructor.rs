//! `list.toArray(|` knows the array it is asked for: `Pair[]::new` from `List<Pair>`.

mod common;
use common::*;

fn ctor(body: &str, trigger: &str) -> Option<String> {
    let p = Project::with_stream_jdk(&[
        ("Pair.java", "package z;\npublic class Pair { }\n"),
        ("Use.java", &format!("package z;\nimport java.util.*;\npublic class Use<E> {{\n    void run(List<Pair> pairs, List<String> names, List raw, List<E> vars) {{\n        {body}\n    }}\n}}\n")),
    ]);
    let off = at(p.source("Use.java"), trigger) + trigger.len();
    p.array_constructor("Use.java", off)
}

#[test]
fn the_element_type_of_the_collection_gives_the_array_constructor() {
    assert_eq!(ctor("take(pairs.toArray(\n", "pairs.toArray("), Some("Pair[]::new".to_string()));
    assert_eq!(ctor("names.toArray(\n", "names.toArray("), Some("String[]::new".to_string()));
}

#[test]
fn a_raw_collection_a_type_variable_or_another_method_gives_nothing() {
    assert_eq!(ctor("raw.toArray(\n", "raw.toArray("), None);
    assert_eq!(ctor("vars.toArray(\n", "vars.toArray("), None);
    assert_eq!(ctor("pairs.contains(\n", "pairs.contains("), None);
}
