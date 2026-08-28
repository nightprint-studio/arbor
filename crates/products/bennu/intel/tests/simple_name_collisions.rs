//! A project declares several types with the same SIMPLE name, and a file names one of them
//! without an import because it lives in the same package.
//!
//! The project-wide simple→binary map keeps ONE binary per simple name, so on a collision every
//! consumer that falls through to it gets whichever type the map happened to hold. Commons-lang
//! declares six nested `Builder` classes plus the real `org.apache.commons.lang3.builder.Builder`
//! interface, and the six classes that implement the interface were each reported as implementing
//! something that is not one — thirteen of that project's seventeen false positives.
//!
//! These are index-level tests on purpose: a mock resolver answers the *right* type by
//! construction, so the bug is invisible to a check's own unit tests.

mod common;
use common::Project;

/// `p/Strings.java` — a nested class also called `Builder`, in ANOTHER package. Declared first so
/// it is what a flat simple→binary map is most likely to keep.
const STRINGS: &str = r#"package p;
public abstract class Strings {
    public static class Builder {
        public Strings get() { return null; }
    }
}
"#;

/// The real interface, in `p.builder`.
const BUILDER: &str = r#"package p.builder;
public interface Builder<T> {
    T build();
}
"#;

/// A class in the SAME package as the interface, naming it with no import, and declaring a NESTED
/// class of that same simple name — the shape of every `*Builder` in commons-lang.
///
/// The nested `Builder` is in scope in the class BODY but not in the class HEADER (JLS §6.3: a
/// member type's scope is the body of its class), which is why javac binds the `implements` clause
/// to the same-package interface and compiles this.
const HASH_CODE_BUILDER: &str = r#"package p.builder;
public class HashCodeBuilder implements Builder<Integer> {
    public static class Builder {
        public HashCodeBuilder build() { return new HashCodeBuilder(); }
    }
    @Override
    public Integer build() {
        return Integer.valueOf(17);
    }
    Builder nested() { return new Builder(); }
}
"#;

fn project() -> Project {
    Project::new(&[
        ("p/Strings.java", STRINGS),
        ("p/builder/Builder.java", BUILDER),
        ("p/builder/HashCodeBuilder.java", HASH_CODE_BUILDER),
    ])
}

/// The same-package interface is what `implements Builder<Integer>` names — not the nested class of
/// the same simple name in another package.
#[test]
fn a_same_package_supertype_is_not_confused_with_a_nested_namesake() {
    let errors = project().validate_errors("p/builder/HashCodeBuilder.java");
    assert!(
        errors.is_empty(),
        "a legal same-package `implements` was reported: {errors:?}"
    );
}

/// The `@Override` on `build()` resolves against the interface too: judged against the nested class
/// (which has no `build`) it reads as overriding nothing.
#[test]
fn an_override_of_a_same_package_interface_method_is_seen() {
    let errors = project().validate_errors("p/builder/HashCodeBuilder.java");
    assert!(
        !errors.iter().any(|e| e.starts_with("override-overrides-nothing")),
        "the override was not seen: {errors:?}"
    );
}

/// A nested type named through a PARAMETERISED qualifier: `extends AbstractMultiset<E>.EntrySet`.
///
/// Guava's `ConcurrentHashMultiset` is written that way. Every reader truncated the text at the
/// first `<` before resolving it, so the supertype came out as `AbstractMultiset` — an abstract
/// class — and all six of its abstract methods were reported as unimplemented on a class that
/// inherits them from the nested type it actually extends.
const OUTER: &str = r#"package p;
public abstract class Outer<E> {
    public abstract int size();
    public abstract java.util.Iterator<E> iterate();
    public abstract class Rows {
        @Override
        public int hashCode() { return 0; }
        public int size() { return 0; }
        public java.util.Iterator<E> iterate() { return null; }
    }
}
"#;

const SUB: &str = r#"package p;
public final class Sub<E> extends Outer<E> {
    @Override
    public int size() { return 0; }
    @Override
    public java.util.Iterator<E> iterate() { return null; }
    private final class Rows extends Outer<E>.Rows {
        @Override
        public int hashCode() { return 1; }
    }
}
"#;

#[test]
fn a_supertype_named_through_a_parameterised_qualifier_is_not_truncated() {
    let p = Project::new(&[("p/Outer.java", OUTER), ("p/Sub.java", SUB)]);
    let errors = p.validate_errors("p/Sub.java");
    assert!(
        errors.is_empty(),
        "a nested supertype behind a type-argument list was lost: {errors:?}"
    );
}
