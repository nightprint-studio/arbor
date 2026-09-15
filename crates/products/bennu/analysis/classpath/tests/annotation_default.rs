//! The `AnnotationDefault` attribute, decoded from the machine's own JDK.
//!
//! `Member::is_default` answers "the user of this type need not supply it" — an interface `default`
//! method for a class, an element with a `default` clause for an annotation. The annotation half is
//! what tells a required element from an optional one, and the check that reads it fires on every
//! annotation in a codebase: if the attribute were not decoded, every element would look required
//! and every bare `@Deprecated` in a project would be reported.
//!
//! Which is why this is measured against real class files rather than a fixture — a fixture would
//! agree with whatever the decoder does. Skipped, loudly, when no JDK 21 resolves.

use bennu_classpath::prelude::{resolve_jdk_classpath, MemberIndex, MemberKind, SourceMemberIndex};

/// `(name, is_default)` for every element of the annotation type `binary`.
fn elements(binary: &str) -> Option<Vec<(String, bool)>> {
    let source = match resolve_jdk_classpath("21") {
        Ok(s) => s,
        Err(why) => {
            eprintln!("SKIPPED: no JDK 21 on this machine ({why})");
            return None;
        }
    };
    let index = SourceMemberIndex::new(source);
    let members = index.members_of(binary)?;
    assert!(members.flags.is_annotation, "{binary} is not an annotation type");
    Some(
        members
            .methods
            .iter()
            .filter(|m| m.kind == MemberKind::Method)
            .map(|m| (m.name.clone(), m.is_default))
            .collect(),
    )
}

/// `@Deprecated` is the everyday marker: both its elements have defaults, which is what makes a
/// bare `@Deprecated` legal.
#[test]
fn every_element_of_deprecated_has_a_default() {
    let Some(elements) = elements("java/lang/Deprecated") else { return };
    assert!(!elements.is_empty(), "no elements decoded at all");
    for (name, has_default) in &elements {
        assert!(has_default, "`{name}` should carry a default");
    }
}

/// `@Retention` and `@SuppressWarnings` each declare exactly one element and neither has a default
/// — the other side of the same bit, and the reason a bare `@Retention` does not compile.
#[test]
fn value_is_required_on_retention_and_suppresswarnings() {
    for binary in ["java/lang/annotation/Retention", "java/lang/SuppressWarnings"] {
        let Some(elements) = elements(binary) else { return };
        assert_eq!(elements, vec![("value".to_string(), false)], "{binary} decoded wrong");
    }
}
