//! `return Ra|` through the **provider** and the **real JDK**: the class a method returns heads the
//! class names offered, above every other `Ra…` class — the imported `java.util.Random` included —
//! because it is the one the author is about to write (`return RawIdentity.builder()…build()`).
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
            "bennu-return-type-name-{}-{}",
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

const USER: &str = "com/acme/Factory.java";

fn user_source(body: &str) -> String {
    format!(
        "package com.acme;\n\
         \n\
         import java.util.Optional;\n\
         import java.util.Random;\n\
         \n\
         public class Factory {{\n\
         \x20   private final Random random = new Random();\n\
         \n\
         \x20   {body}\n\
         }}\n"
    )
}

fn provider_at(marked_body: &str) -> Option<(NativeJavaProvider, String, usize, TempDir)> {
    let marked = user_source(marked_body);
    let caret = marked.find('|').expect("a caret marker");
    let source = marked.replacen('|', "", 1);
    let temp = TempDir::new();
    let file = |path: &str, text: &str| (PathBuf::from(path), text.to_string());
    let files = vec![
        file(
            "com/acme/RawIdentity.java",
            "package com.acme;\nimport lombok.Builder;\n@Builder\npublic class RawIdentity {\n    private String id;\n}\n",
        ),
        file("com/acme/Rack.java", "package com.acme;\npublic interface Rack { }\n"),
        file("com/acme/RackImpl.java", "package com.acme;\npublic class RackImpl implements Rack { }\n"),
        file("com/acme/Radio.java", "package com.acme;\npublic class Radio { }\n"),
        file("com/acme/model/RawRecord.java", "package com.acme.model;\npublic class RawRecord { }\n"),
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
            .complete_at(
                &Position { file: USER.to_string(), offset: caret },
                Some(&source),
                CompletionOptions::default(),
            )
            .expect("completion"),
    )
}

fn rank_of(items: &[CompletionItem], label: &str) -> usize {
    items.iter().position(|i| i.label == label).unwrap_or(usize::MAX)
}

/// The reported case, on the unfinished line: no `;`, nothing after the prefix.
#[test]
fn the_returned_class_heads_the_names_and_is_preselected() {
    let Some(items) = complete("public RawIdentity create() {\n        return Ra|\n    }") else { return };
    let first = items.first().expect("something is offered");
    assert_eq!(first.label, "RawIdentity", "{items:?}");
    assert!(first.preselect);
    assert_eq!(items.iter().filter(|i| i.preselect).count(), 1, "one pick: {items:?}");
    // Above the class this file imports, which wins on proximity alone.
    assert!(rank_of(&items, "RawIdentity") < rank_of(&items, "Random"), "{items:?}");
}

#[test]
fn a_line_already_terminated_answers_the_same() {
    let Some(items) = complete("public RawIdentity create() {\n        return Ra|;\n    }") else { return };
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("RawIdentity"), "{items:?}");
}

/// Returned but not imported yet: offered first, carrying the import that makes it compile.
#[test]
fn a_returned_class_not_yet_imported_heads_the_names_with_its_import() {
    let Some(items) = complete("public RawRecord load() {\n        return Ra|\n    }") else { return };
    let first = items.first().expect("something is offered");
    assert_eq!(first.label, "RawRecord", "{items:?}");
    assert_eq!(first.auto_import.as_deref(), Some("com.acme.model.RawRecord"));
}

/// `Optional<RawIdentity>`: the container when its name is typed, the element when that is.
#[test]
fn a_generic_return_offers_the_container_then_the_element() {
    let Some(items) = complete("public Optional<RawIdentity> find() {\n        return Op|\n    }") else { return };
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("Optional"), "{items:?}");

    let Some(items) = complete("public Optional<RawIdentity> find() {\n        return Ra|\n    }") else { return };
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("RawIdentity"), "{items:?}");
}

/// The exact type first, then a class implementing it, then the rest.
#[test]
fn a_subtype_of_the_returned_type_follows_it() {
    let Some(items) = complete("public Rack rack() {\n        return Ra|\n    }") else { return };
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("Rack"), "{items:?}");
    assert!(rank_of(&items, "RackImpl") < rank_of(&items, "Radio"), "{items:?}");
    assert!(rank_of(&items, "RackImpl") < rank_of(&items, "Random"), "{items:?}");
}
