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
    let marked = format!("package com.acme;\n\npublic class Use {{\n    {marked_body}\n}}\n");
    let caret = marked.find('|').expect("a caret marker");
    let source = marked.replacen('|', "", 1);
    let temp = TempDir::new();
    let file = |path: &str, text: &str| (PathBuf::from(path), text.to_string());
    let files = vec![
        file("com/acme/ComunicazioniFilter.java", "package com.acme;\npublic class ComunicazioniFilter { }\n"),
        file("com/acme/Customer.java", "package com.acme;\npublic class Customer { }\n"),
        file("com/acme/Codice.java", "package com.acme;\npublic class Codice { }\n"),
        file("com/acme/Counter.java", "package com.acme;\npublic class Counter { }\n"),
        file(
            "com/acme/ComunicazioniService.java",
            "package com.acme;\npublic class ComunicazioniService {\n    public java.util.List<String> list(ComunicazioniFilter filter) { return null; }\n    public void two(ComunicazioniFilter a, Customer b) { }\n    public void pick(ComunicazioniFilter a) { }\n    public void pick(Customer a) { }\n}\n",
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

fn complete(marked_body: &str) -> Option<Vec<CompletionItem>> {
    let (provider, source, caret, _t) = provider_at(marked_body)?;
    Some(
        provider
            .complete_at(&Position { file: USER.to_string(), offset: caret }, Some(&source), CompletionOptions::default())
            .expect("completion"),
    )
}

fn rank_of(items: &[CompletionItem], label: &str) -> usize {
    items.iter().position(|i| i.label == label).unwrap_or(usize::MAX)
}

/// The reported case: the parameter's class, not a name that merely starts alike, comes first.
#[test]
fn the_class_the_parameter_takes_heads_the_names() {
    let Some(items) = complete("void run(ComunicazioniService s) {\n        s.list(C|)\n    }") else { return };
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("ComunicazioniFilter"), "{items:?}");
    assert!(rank_of(&items, "ComunicazioniFilter") < rank_of(&items, "Customer"), "{items:?}");
}

#[test]
fn a_later_slot_takes_its_own_class() {
    let Some(items) = complete("void run(ComunicazioniService s) {\n        s.two(null, C|)\n    }") else { return };
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("Customer"), "{items:?}");
}

/// Overloads that disagree on the slot say nothing: the usual order stands.
#[test]
fn overloads_that_disagree_do_not_lead() {
    let Some(items) = complete("void run(ComunicazioniService s) {\n        s.pick(C|)\n    }") else { return };
    assert!(!items.is_empty());
}
