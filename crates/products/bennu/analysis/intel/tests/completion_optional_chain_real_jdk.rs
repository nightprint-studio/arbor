//! The reported `Optional` chain through the **provider** and the **real JDK** — the whole answer
//! the popup is sent, not the member query alone.
//!
//! The harness tests (`autocomplete_optional_chain.rs`) pin each query. What only the provider can
//! show is how they are put together: whether a caret after a dot falls through to the bare-word
//! answer, and whether the type a function slot receives heads the type names the index offers.
//! Skipped, loudly, when no JDK 21 resolves.

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
            "bennu-optional-chain-{}-{}",
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

const USER: &str = "com/acme/CheckAssignedUser.java";

fn user_source(body: &str) -> String {
    format!(
        "package com.acme;\n\
         \n\
         import lombok.RequiredArgsConstructor;\n\
         import lombok.val;\n\
         import org.springframework.stereotype.Component;\n\
         \n\
         @RequiredArgsConstructor @Component\n\
         public class CheckAssignedUser implements AttributeValidator {{\n\
         \x20   private final IdentityResolver identity_resolver;\n\
         \n\
         \x20   private void check_delegate(final String username) {{\n\
         \x20       {body}\n\
         \x20   }}\n\
         }}\n"
    )
}

/// The provider over the reported project, and the user file with `|` removed at the caret.
fn provider_at(marked_body: &str) -> Option<(NativeJavaProvider, String, usize, TempDir)> {
    let marked = user_source(marked_body);
    let caret = marked.find('|').expect("a caret marker");
    let source = marked.replacen('|', "", 1);
    let temp = TempDir::new();
    let files = vec![
        (
            PathBuf::from("com/acme/IdentityResolver.java"),
            "package com.acme;\nimport java.util.Optional;\npublic interface IdentityResolver {\n    Optional<ResolvedIdentity> resolve_identity();\n}\n"
                .to_string(),
        ),
        (
            PathBuf::from("com/acme/ResolvedIdentity.java"),
            "package com.acme;\npublic record ResolvedIdentity(String identifier, int roles) {\n    public static String describe(ResolvedIdentity identity) { return \"\"; }\n    public boolean has_role(String role) { return false; }\n}\n"
                .to_string(),
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

fn complete(provider: &NativeJavaProvider, source: &str, caret: usize) -> Vec<CompletionItem> {
    provider
        .complete_at(
            &Position { file: USER.to_string(), offset: caret },
            Some(source),
            CompletionOptions::default(),
        )
        .expect("completion")
}

/// Bug 1: `Optional`'s members and the postfix templates — no class, no local, no keyword.
#[test]
fn after_the_dot_only_optional_members_and_templates_are_offered() {
    let Some((provider, source, caret, _t)) = provider_at("identity_resolver.resolve_identity().ma|") else {
        return;
    };
    let items = complete(&provider, &source, caret);
    assert!(items.iter().any(|i| i.label == "map" && i.kind == "method"), "{items:?}");
    for item in &items {
        assert!(
            matches!(item.kind.as_str(), "method" | "field" | "postfix"),
            "{} ({}) cannot follow the dot",
            item.label,
            item.kind
        );
        if item.kind != "postfix" {
            assert!(item.owner.as_deref().is_some_and(|o| o == "java/util/Optional" || o == "java/lang/Object"), "{item:?}");
        }
    }
}

/// Bug 1, the fallthrough: a receiver nothing can type must not be answered as a bare word. Before,
/// `ch` after the dot offered the class's own `check_delegate`.
#[test]
fn after_a_dot_on_an_untypeable_receiver_nothing_bare_is_offered() {
    let Some((provider, source, caret, _t)) = provider_at("mystery_value.ch|") else {
        return;
    };
    let items = complete(&provider, &source, caret);
    assert!(items.iter().all(|i| i.kind == "postfix"), "{items:?}");
}

/// Bug 2: `ResolvedIdentity` heads the `Re…` type names, and is preselected.
#[test]
fn the_type_the_map_function_receives_is_offered_first() {
    let Some((provider, source, caret, _t)) = provider_at("identity_resolver.resolve_identity().map(Re|)") else {
        return;
    };
    let items = complete(&provider, &source, caret);
    let first = items.first().expect("something is offered");
    assert_eq!(first.label, "ResolvedIdentity", "{items:?}");
    assert!(first.preselect);
    assert_eq!(items.iter().filter(|i| i.label == "ResolvedIdentity").count(), 1, "offered once");
}

/// Bug 2, Ctrl+Space with nothing typed.
#[test]
fn an_explicit_request_in_an_empty_map_opens_on_that_type() {
    let Some((provider, source, caret, _t)) = provider_at("identity_resolver.resolve_identity().map(|)") else {
        return;
    };
    let items = complete(&provider, &source, caret);
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("ResolvedIdentity"), "{items:?}");
}

/// Bug 3: after `::` the record's methods, the fitting ones first, and nothing else.
#[test]
fn after_colons_the_records_methods_are_offered_and_nothing_bare() {
    let Some((provider, source, caret, _t)) =
        provider_at("identity_resolver.resolve_identity().map(ResolvedIdentity::|)")
    else {
        return;
    };
    let items = complete(&provider, &source, caret);
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    for want in ["identifier", "describe", "has_role"] {
        assert!(labels.contains(&want), "{want}: {labels:?}");
    }
    let rank = |name: &str| labels.iter().position(|l| *l == name).unwrap_or(usize::MAX);
    assert!(rank("identifier") < rank("has_role"), "{labels:?}");
    assert!(items.iter().all(|i| matches!(i.kind.as_str(), "method" | "constructor" | "postfix")), "{items:?}");
}

/// Bug 4: the hover card of the `val`.
#[test]
fn hovering_the_val_shows_an_optional_of_string() {
    let Some((provider, source, _caret, _t)) = provider_at(
        "val delegate| = identity_resolver.resolve_identity().map(ResolvedIdentity::identifier);",
    ) else {
        return;
    };
    let offset = source.find(" delegate =").expect("the val") + 2;
    let card = provider.var_hover(&source, offset).expect("a card");
    assert!(card.signature.contains("Optional") && card.signature.contains("String"), "{}", card.signature);
}
