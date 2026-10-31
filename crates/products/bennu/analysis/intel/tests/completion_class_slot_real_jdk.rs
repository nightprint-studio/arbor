//! A `Class<T>` parameter (`as_scalar(Boo|)`, `parse(Or|)`)... 
//! `service.list(C|)` through the **provider** and the **real JDK**: the class the parameter takes heads the class
//! names offered, above every other `C…` class — it is the one the author is about to write
//! (`list(ComunicazioniFilter.builder()…build())`).
//!
//! Through the provider because that is where the class-name index, the scope and the lead lists are
//! merged, and where "first" is decided. Skipped, loudly, when no JDK 21 resolves.

use std::path::PathBuf;

use bennu_intel::prelude::{
    build_project_index_from_sources, CompletionItem, CompletionOptions, NativeJavaProvider, Position,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "bennu-argument-class-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
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

fn provider_at(marked_body: &str) -> Option<(NativeJavaProvider, String, usize, TempDir)> {
    let marked = format!("package com.acme;\n\npublic class Use {{\n    private static final String ALPHA = \"a\";\n    private static final String BETA = \"b\";\n    {marked_body}\n}}\n");
    let caret = marked.find('|').expect("a caret marker");
    let source = marked.replacen('|', "", 1);
    let temp = TempDir::new();
    let file = |path: &str, text: &str| (PathBuf::from(path), text.to_string());
    let files = vec![
        file("com/acme/ParserXml.java", "package com.acme;\npublic interface ParserXml<X, Z> { Z parse(X input); }\n"),
        file("com/acme/OrderParser.java", "package com.acme;\npublic class OrderParser implements ParserXml<String, Integer> { public Integer parse(String s) { return 0; } }\n"),
        file("com/acme/InvoiceParser.java", "package com.acme;\npublic class InvoiceParser implements ParserXml<String, Integer> { public Integer parse(String s) { return 0; } }\n"),
        file("com/acme/Orderly.java", "package com.acme;\npublic class Orderly { }\n"),
        file(
            "com/acme/Svc.java",
            "package com.acme;\npublic class Svc {\n    public <T> T as_scalar(final Class<T> type) { return null; }\n    public <T extends ParserXml<X, Z>, X, Z> Z parse(Class<T> parse_xml, X input) { return null; }\n}\n",
        ),
        (PathBuf::from(USER), source.clone()),
    ];
    let built = build_project_index_from_sources(&files, &temp.0);
    built.builder.persist().expect("persist");
    let names: Vec<(String, String)> =
        built.type_map.iter().map(|(s, b)| (s.clone(), b.clone())).collect();
    match NativeJavaProvider::for_project(&temp.0, "21", &names, None, None) {
        Ok(provider) => Some((provider, source, caret, temp)),
        Err(why) => {
            eprintln!("SKIPPED: no JDK 21 on this machine ({why})");
            None
        }
    }
}

fn labels(marked_body: &str) -> Option<Vec<String>> {
    let (provider, source, caret, _t) = provider_at(marked_body)?;
    let items = provider
        .complete_at(&Position { file: USER.to_string(), offset: caret }, Some(&source), CompletionOptions::default())
        .expect("completion");
    Some(items.into_iter().map(|i| i.label).collect())
}

fn first(body: &str, n: usize) -> Option<Vec<String>> {
    labels(body).map(|l| l.into_iter().take(n).collect())
}

/// A method that takes a class and is rarely called: the plain value types lead, already written as `Type.class`.
#[test]
fn a_class_parameter_opens_on_the_plain_value_types() {
    let Some(top) = first("void f(Svc svc) { svc.as_scalar(|); }", 4) else { return };
    assert_eq!(top, ["String.class", "Integer.class", "Long.class", "Boolean.class"]);
}

#[test]
fn a_few_letters_reach_the_class_literal_before_anything_else() {
    let Some(top) = first("void f(Svc svc) { svc.as_scalar(Boo|); }", 2) else { return };
    assert_eq!(top[0], "Boolean.class", "{top:?}");
}

/// After `Boolean.` in a slot that takes a class, `class` is what is wanted.
#[test]
fn after_a_type_name_the_class_literal_is_first() {
    let Some(top) = first("void f(Svc svc) { svc.as_scalar(Boolean.|); }", 1) else { return };
    assert_eq!(top, ["class"]);
}

/// `<T extends ParserXml<X, Z>>`: the classes that implement the bound, not constants and not the plain types.
#[test]
fn a_bounded_class_parameter_offers_the_classes_that_implement_the_bound() {
    let Some(top) = first("void f(Svc svc) { svc.parse(|); }", 2) else { return };
    assert_eq!(top, ["InvoiceParser.class", "OrderParser.class"]);
    let Some(typed) = first("void f(Svc svc) { svc.parse(Or|); }", 1) else { return };
    assert_eq!(typed, ["OrderParser.class"]);
}
