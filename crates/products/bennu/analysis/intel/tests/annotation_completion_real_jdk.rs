//! Annotation completion after an `@`, against the **real JDK**.
//!
//! The reported symptom was "the annotation list only starts at the third character": `@S` and
//! `@Su` offered nothing (so the editor fell through to the buffer's own words, which is where the
//! nonsense came from) and `@Sup` finally offered `SuppressWarnings`.
//!
//! The cause was a cap in the wrong place. The sweep took the best three hundred names under the
//! prefix and filtered THOSE down to the annotation types — and the JDK alone has more than three
//! hundred classes beginning with `S`, none of the first three hundred an annotation. The answer
//! existed and was cut before anything looked at it. The cap now counts annotations kept, not
//! names looked at.
//!
//! Real JDK on purpose: a fake member index with a handful of classes cannot have this bug, which
//! is exactly why the suite did not catch it.

use std::path::PathBuf;

use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::{
    build_project_index_from_sources, CompletionOptions, NativeJavaProvider, Position,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "bennu-annot-{}-{}",
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

const SRC: &str = "package p;\npublic class Use {\n    @\n    void go() { }\n}\n";

/// The labels offered with `typed` written after the `@`. `None` when no JDK 21 resolves here.
fn offered(typed: &str) -> Option<Vec<String>> {
    offered_in(SRC, typed)
}

fn offered_in(src: &str, typed: &str) -> Option<Vec<String>> {
    let temp = TempDir::new();
    let files = vec![(PathBuf::from("Use.java"), src.to_string())];
    let built = build_project_index_from_sources(&files, &temp.0);
    built.builder.persist().expect("persist");
    PersistedIndex::open(built.builder.blob_path(), built.builder.fst_path()).expect("open");
    let names: Vec<(String, String)> =
        built.type_map.iter().map(|(s, b)| (s.clone(), b.clone())).collect();
    let provider = match NativeJavaProvider::for_project(&temp.0, "21", &names, None, None) {
        Ok(p) => p,
        Err(why) => {
            eprintln!("SKIPPED: no JDK 21 on this machine ({why})");
            return None;
        }
    };
    let at = src.find('@').expect("the fixture has an `@`") + 1;
    let source = format!("{}{typed}{}", &src[..at], &src[at..]);
    let items = provider
        .complete_at(
            &Position { file: "Use.java".to_string(), offset: at + typed.len() },
            Some(&source),
            CompletionOptions::default(),
        )
        .expect("completion");
    Some(items.into_iter().map(|i| i.label).collect())
}

/// Every one of these is an annotation the JDK declares whose first letter is shared by hundreds
/// of ordinary classes — which is the whole of the bug.
#[test]
fn one_letter_is_enough_to_reach_a_jdk_annotation() {
    for (typed, want) in [
        ("S", "SuppressWarnings"),
        ("F", "FunctionalInterface"),
        ("D", "Deprecated"),
        ("O", "Override"),
    ] {
        let Some(labels) = offered(typed) else { return };
        assert!(
            labels.iter().any(|l| l == want),
            "@{typed} did not offer {want}; offered ({}): {labels:?}",
            labels.len()
        );
    }
}

#[test]
fn the_second_and_third_letters_still_work() {
    for typed in ["Su", "Sup"] {
        let Some(labels) = offered(typed) else { return };
        assert!(
            labels.iter().any(|l| l == "SuppressWarnings"),
            "@{typed}: {labels:?}"
        );
    }
}

/// Ordinary classes are still not annotations, however well they match. The filter is what makes
/// the list short enough to be worth opening unasked.
#[test]
fn a_class_that_is_not_an_annotation_is_not_offered() {
    let Some(labels) = offered("S") else { return };
    for not_an_annotation in ["String", "System", "StringBuilder", "Set"] {
        assert!(
            !labels.iter().any(|l| l == not_an_annotation),
            "{not_an_annotation} is not an annotation: {labels:?}"
        );
    }
}

/// A bare `@` above a method still says nothing on its own — every annotation there is, in no
/// order anyone would want.
#[test]
fn a_bare_at_with_a_known_target_offers_a_bounded_list() {
    let Some(labels) = offered("") else { return };
    assert!(labels.len() <= 40, "unbounded list of {} items", labels.len());
}

/// A bare `@` with no declaration after it has no target to rank by, and used to offer nothing — so the editor filled the
/// popup with keywords. What the file imports is still worth offering: it is what the person would pick from.
#[test]
fn a_bare_at_with_no_target_offers_the_annotations_the_file_imports() {
    let src = "package p;\nimport java.lang.annotation.Documented;\nimport java.util.List;\npublic class Use {\n    @\n}\n";
    let Some(labels) = offered_in(src, "") else { return };
    assert!(labels.iter().any(|l| l == "Documented"), "{labels:?}");
    assert!(!labels.iter().any(|l| l == "List"), "an import that is not an annotation: {labels:?}");
}
