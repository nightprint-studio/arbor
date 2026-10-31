//! The class picked after `new` writes the call, not just the name: `ArrayList<>()`, `Plain()`, an anonymous body for an
//! interface — through the **provider** and the **real JDK**, since whether a type is generic, abstract or has a
//! no-arg constructor is read out of the class file. Skipped, loudly, when no JDK 21 resolves.

use std::path::PathBuf;

use bennu_intel::prelude::{
    build_project_index_from_sources, CompletionItem, CompletionOptions, NativeJavaProvider, Position,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!("bennu-new-syntax-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const USER: &str = "com/acme/Use.java";

fn complete(marked_body: &str) -> Option<Vec<CompletionItem>> {
    complete_with(&[], marked_body)
}

fn complete_with(extra: &[(&str, &str)], marked_body: &str) -> Option<Vec<CompletionItem>> {
    let marked = format!("package com.acme;\n\npublic class Use {{\n    void run() {{\n        {marked_body}\n    }}\n}}\n");
    let caret = marked.find('|').expect("a caret marker");
    let source = marked.replacen('|', "", 1);
    let temp = TempDir::new();
    let file = |path: &str, text: &str| (PathBuf::from(path), text.to_string());
    let files = vec![
        file("com/acme/Plain.java", "package com.acme;\npublic class Plain {\n    public Plain() { }\n}\n"),
        file("com/acme/Needy.java", "package com.acme;\npublic class Needy {\n    public Needy(int x) { }\n}\n"),
        file("com/acme/Drawable.java", "package com.acme;\npublic interface Drawable { }\n"),
        file("com/acme/Base.java", "package com.acme;\npublic abstract class Base<T> {\n    protected Base() { }\n}\n"),
        file("com/acme/Colour.java", "package com.acme;\npublic enum Colour { RED }\n"),
        (PathBuf::from(USER), source.clone()),
    ];
    let mut files = files;
    files.extend(extra.iter().map(|(p, t)| (PathBuf::from(p), t.to_string())));
    let built = build_project_index_from_sources(&files, &temp.0);
    built.builder.persist().expect("persist");
    let names: Vec<(String, String)> = built.type_map.iter().map(|(s, b)| (s.clone(), b.clone())).collect();
    let provider = match NativeJavaProvider::for_project(&temp.0, &std::env::var("PROBE_JDK").unwrap_or_else(|_| "21".into()), &names, None, None) {
        Ok(p) => p,
        Err(why) => {
            eprintln!("SKIPPED: no JDK 21 on this machine ({why})");
            return None;
        }
    };
    Some(
        provider
            .complete_at(&Position { file: USER.to_string(), offset: caret }, Some(&source), CompletionOptions::default())
            .expect("completion"),
    )
}

/// The row for `label`: what it inserts and where the caret stops (byte offsets into the insertion).
fn written(body: &str, label: &str) -> Option<(String, Vec<usize>)> {
    let items = complete(body)?;
    let item = items.iter().find(|i| i.label == label && i.kind == "class").unwrap_or_else(|| panic!("no `{label}` in {items:?}"));
    Some((item.insert_text.clone().unwrap_or_else(|| item.label.clone()), item.snippet_stops.iter().map(|s| s.start).collect()))
}

#[test]
fn a_generic_class_gets_the_diamond_and_the_caret_after_the_call() {
    let Some((text, stops)) = written("var l = new ArrayLis|", "ArrayList") else { return };
    // `ArrayList` has a no-arg constructor: nothing left to type inside
    assert_eq!(text, "ArrayList<>()");
    assert!(stops.is_empty());
}

#[test]
fn a_map_gets_the_diamond_too() {
    let Some((text, _)) = written("var m = new HashMa|", "HashMap") else { return };
    assert_eq!(text, "HashMap<>()");
}

#[test]
fn a_class_with_a_no_arg_constructor_gets_plain_parentheses() {
    let Some((text, stops)) = written("Plain p = new Pla|", "Plain") else { return };
    assert_eq!(text, "Plain()");
    assert!(stops.is_empty());
}

#[test]
fn a_class_whose_constructors_all_take_something_parks_the_caret_inside() {
    let Some((text, stops)) = written("Needy n = new Nee|", "Needy") else { return };
    assert_eq!(text, "Needy()");
    assert_eq!(stops, vec!["Needy(".len()]);
}

#[test]
fn an_interface_is_an_anonymous_class_with_the_caret_in_the_body() {
    let Some((text, stops)) = written("Drawable s = new Draw|", "Drawable") else { return };
    assert_eq!(text, "Drawable() {}");
    assert_eq!(stops, vec![text.len() - 1]);
}

#[test]
fn a_jdk_interface_is_an_anonymous_class_too() {
    let Some((text, stops)) = written("Runnable r = new Runnabl|", "Runnable") else { return };
    assert_eq!(text, "Runnable() {}");
    assert_eq!(stops, vec![text.len() - 1]);
}

#[test]
fn a_generic_abstract_class_gets_the_diamond_and_a_body() {
    let Some((text, stops)) = written("Base<String> b = new Bas|", "Base") else { return };
    assert_eq!(text, "Base<>() {}");
    assert_eq!(stops, vec![text.len() - 1]);
}

#[test]
fn an_enum_is_left_as_its_bare_name() {
    let Some((text, stops)) = written("Object c = new Colo|", "Colour") else { return };
    assert_eq!(text, "Colour");
    assert!(stops.is_empty());
}

#[test]
fn a_type_that_is_not_after_new_is_left_alone() {
    let Some((text, stops)) = written("Plain p = renew Pla|", "Plain") else { return };
    assert_eq!(text, "Plain");
    assert!(stops.is_empty());
    let Some((text, _)) = written("Pla|", "Plain") else { return };
    assert_eq!(text, "Plain");
}

#[test]
fn a_call_already_written_is_not_written_twice() {
    let Some((text, stops)) = written("Plain p = new Pla|();", "Plain") else { return };
    assert_eq!(text, "Plain");
    assert!(stops.is_empty());
}

/// A JDK class that is imported nowhere in the file, in the places a `new` is written: declaration, `var`, argument, bare
/// statement. The picker writes the call in every one of them (`PROBE_JDK=8` / `11` runs the same against another JDK).
#[test]
fn an_unimported_jdk_class_is_completed_with_its_call_in_every_position() {
    for body in [
        "ByteArrayOutputStream bao = new ByteArrayOut|;",
        "ByteArrayOutputStream bao = new ByteArrayOut|",
        "var bao = new ByteArrayOut|;",
        "System.out.println(new ByteArrayOut|);",
        "new ByteArrayOut|;",
    ] {
        let Some((text, _)) = written(body, "ByteArrayOutputStream") else { return };
        assert_eq!(text, "ByteArrayOutputStream()", "{body}");
    }
}

/// A simple name with two candidates (the JDK's `ByteArrayOutputStream` and a library's of the same name) is completed with
/// its call when both are written the same way, though neither can be imported silently.
#[test]
fn an_ambiguous_name_gets_its_call_when_the_candidates_agree() {
    let Some(items) = complete_with(&[("com/other/ByteArrayOutputStream.java", "package com.other;\npublic class ByteArrayOutputStream {\n    public ByteArrayOutputStream() { }\n}\n")], "val bao = new ByteArrayOut|") else { return };
    let item = items.iter().find(|i| i.label == "ByteArrayOutputStream").expect("the row");
    assert_eq!(item.insert_text.as_deref(), Some("ByteArrayOutputStream()"));
    assert!(item.auto_import.is_none(), "ambiguous: nothing is imported silently");
}
