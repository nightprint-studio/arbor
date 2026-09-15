//! Whether a call is AssertJ's.
//!
//! `assertThat` is the most overloaded name in Java testing: AssertJ's, Hamcrest's `MatcherAssert`,
//! JUnit 4's `Assert`, Truth's, and whatever a project's own test helper is called. Every check in
//! this crate is about AssertJ's semantics specifically — Hamcrest's `assertThat(x, is(1))` asserts
//! perfectly well on its own — so a name match would be wrong about precisely the code it looks at.
//! Resolution goes through the file's imports the way the compiler does
//! ([`static_call_resolves_to`]), and anything it cannot settle is not AssertJ's.

use bennu_facts::prelude::{static_call_resolves_to, JavaFacts};
use bennu_java::prelude::{annotations_of, node_text};
use tree_sitter::Node;

use crate::chain::call_name;
use crate::scope::{declarations, Decl};
use crate::syntax::{ancestors, children, unparen};
use crate::unit::Unit;

pub(crate) const API_PACKAGE: &str = "org.assertj.core.api";

/// The classes a static `assertThat` may come from. `BDDAssertions` extends `Assertions`, so its
/// `assertThat` is the same method under another owner.
pub(crate) const ASSERT_OWNERS: &[&str] = &[
    "org.assertj.core.api.Assertions",
    "org.assertj.core.api.AssertionsForClassTypes",
    "org.assertj.core.api.AssertionsForInterfaceTypes",
    "org.assertj.core.api.Java6Assertions",
    "org.assertj.core.api.BDDAssertions",
];
const BDD_OWNERS: &[&str] = &["org.assertj.core.api.BDDAssertions"];

/// The calls that start a chain and assert nothing by themselves. `assertThatThrownBy` and
/// `assertThatExceptionOfType` are deliberately absent: the first asserts that something was thrown,
/// the second starts a chain that is not an assertion object at all.
const ASSERT_ENTRIES: &[&str] = &["assertThat", "assertThatObject", "assertThatCode"];
const BDD_ENTRIES: &[&str] = &["then", "thenObject", "thenCode"];

/// Every soft-assertions type, auto-closing ones included: a variable of any of them collects rather
/// than throws.
const SOFT_TYPES: &[&str] = &[
    "SoftAssertions",
    "BDDSoftAssertions",
    "AutoCloseableSoftAssertions",
    "AutoCloseableBDDSoftAssertions",
];
const EXTENSION_PACKAGE: &str = "org.assertj.core.api.junit.jupiter";

/// How a chain reaches AssertJ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Entry {
    /// A static entry point — `assertThat(x)`, `Assertions.assertThat(x)`, `then(x)`.
    Static,
    /// A soft-assertions object — `softly.assertThat(x)`.
    Soft,
}

/// What a call is made on.
pub(crate) enum Receiver {
    /// Nothing — `assertThat(x)`.
    Bare,
    /// A name, possibly dotted — `Assertions`, `org.assertj.core.api.Assertions`, `softly`.
    Qualifier(String),
    /// Anything else — a call, `this.helper()`, an array element.
    Other,
}

pub(crate) fn receiver(call: Node<'_>, source: &str) -> Receiver {
    let Some(object) = call.child_by_field_name("object") else { return Receiver::Bare };
    if !matches!(object.kind(), "identifier" | "field_access" | "scoped_identifier") {
        return Receiver::Other;
    }
    let written = node_text(&object, source);
    let dotted = !written.is_empty()
        && written.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'));
    if dotted {
        Receiver::Qualifier(written.to_string())
    } else {
        Receiver::Other
    }
}

/// Whether `call` is an AssertJ entry point, and through what.
pub(crate) fn assertj_entry(unit: &Unit<'_>, call: Node<'_>) -> Option<Entry> {
    let name = call_name(call, unit.source);
    let is_assert = ASSERT_ENTRIES.contains(&name);
    let owners = if is_assert {
        ASSERT_OWNERS
    } else if BDD_ENTRIES.contains(&name) {
        BDD_OWNERS
    } else {
        return None;
    };
    let facts = &unit.facts;
    match receiver(call, unit.source) {
        Receiver::Bare => {
            let resolved = static_call_resolves_to(name, None, facts, owners)
                || (is_assert && inherits_with_assertions(unit, call, name));
            resolved.then_some(Entry::Static)
        }
        Receiver::Qualifier(q) => {
            if static_call_resolves_to(name, Some(q.as_str()), facts, owners) {
                Some(Entry::Static)
            } else if !q.contains('.') && is_soft_variable(unit, &q) {
                Some(Entry::Soft)
            } else {
                None
            }
        }
        Receiver::Other => None,
    }
}

/// Whether `written` names the type `package.simple` in this file — written qualified, imported by
/// name, imported on demand, or (for `java.lang`) implicitly. A type of that simple name declared in
/// the file, or imported from anywhere else, is not it.
pub(crate) fn names_type(written: &str, package: &str, simple: &str, facts: &JavaFacts) -> bool {
    let qualified = format!("{package}.{simple}");
    if written.contains('.') {
        return written == qualified;
    }
    if written != simple || facts.types.iter().any(|t| t.name == simple) {
        return false;
    }
    let suffix = format!(".{simple}");
    if let Some(import) = facts.imports.iter().find(|i| i.ends_with(&suffix)) {
        return *import == qualified;
    }
    let on_demand = format!("{package}.*");
    package == "java.lang" || facts.imports.iter().any(|i| *i == on_demand)
}

/// Whether `value` is `new T()` for one of AssertJ's `types` — no anonymous subclass, whose methods
/// could do anything.
pub(crate) fn creates(unit: &Unit<'_>, value: Node<'_>, types: &[&str]) -> bool {
    let value = unparen(value);
    if value.kind() != "object_creation_expression"
        || children(value).iter().any(|c| c.kind() == "class_body")
    {
        return false;
    }
    let Some(ty) = value.child_by_field_name("type") else { return false };
    let written = unit.compact(ty);
    types.iter().any(|t| names_type(&written, API_PACKAGE, t, &unit.facts))
}

/// Whether every declaration of `name` in the file is a soft-assertions object.
///
/// Every one, not the nearest: working out which declaration a use refers to is scope analysis, and
/// a name that is a `SoftAssertions` in one method and a `Map` in the next is rare enough that
/// declining is the cheaper way to be right.
fn is_soft_variable(unit: &Unit<'_>, name: &str) -> bool {
    let found = declarations(unit.root(), name, unit.source);
    !found.is_empty() && found.iter().all(|d| is_soft_declaration(unit, d))
}

fn is_soft_declaration(unit: &Unit<'_>, decl: &Decl<'_>) -> bool {
    let injected = annotations_of(decl.owner, unit.source)
        .iter()
        .any(|(written, _)| names_type(written, EXTENSION_PACKAGE, "InjectSoftAssertions", &unit.facts));
    if injected {
        return true;
    }
    let Some(ty) = decl.ty else { return false };
    if decl.dims > 0 {
        return false;
    }
    let written = unit.compact(ty);
    if written == "var" || written == "val" {
        return decl.value.is_some_and(|v| creates(unit, v, SOFT_TYPES));
    }
    SOFT_TYPES.iter().any(|t| names_type(&written, API_PACKAGE, t, &unit.facts))
}

/// Whether a bare `name(…)` inside `call`'s class is `WithAssertions`' default method — the one way
/// to write AssertJ with no import of the method at all.
fn inherits_with_assertions(unit: &Unit<'_>, call: Node<'_>, name: &str) -> bool {
    let facts = &unit.facts;
    let suffix = format!(".{name}");
    // A static import of that name, or a method of that name in the file, and the reading is no
    // longer certain — decline rather than work out which one wins.
    if facts.imports.iter().any(|i| i.ends_with(&suffix))
        || facts.types.iter().any(|t| t.methods.iter().any(|m| m.name == name))
    {
        return false;
    }
    ancestors(call).any(|n| {
        matches!(n.kind(), "class_declaration" | "enum_declaration" | "record_declaration")
            && implements_with_assertions(unit, n)
    })
}

fn implements_with_assertions(unit: &Unit<'_>, declaration: Node<'_>) -> bool {
    let clause = children(declaration).into_iter().find(|c| c.kind() == "super_interfaces");
    clause.is_some_and(|c| {
        unit.text(c)
            .split(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '_' | '$' | '.')))
            .any(|token| names_type(token, API_PACKAGE, "WithAssertions", &unit.facts))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_facts::prelude::scan_java;

    fn facts(src: &str) -> JavaFacts {
        scan_java("/p/T.java", src).unwrap()
    }

    #[test]
    fn a_type_is_named_through_the_same_rules_the_compiler_uses() {
        let f = facts("import org.assertj.core.api.SoftAssertions;\nclass T {}");
        assert!(names_type("SoftAssertions", API_PACKAGE, "SoftAssertions", &f));
        assert!(names_type("org.assertj.core.api.SoftAssertions", API_PACKAGE, "SoftAssertions", &f));
        let own = facts("import com.acme.SoftAssertions;\nclass T {}");
        assert!(!names_type("SoftAssertions", API_PACKAGE, "SoftAssertions", &own));
        let star = facts("import org.assertj.core.api.*;\nclass T {}");
        assert!(names_type("SoftAssertions", API_PACKAGE, "SoftAssertions", &star));
        assert!(!names_type("SoftAssertions", API_PACKAGE, "SoftAssertions", &facts("class T {}")));
    }

    #[test]
    fn java_lang_needs_no_import_but_can_still_be_shadowed() {
        assert!(names_type("String", "java.lang", "String", &facts("class T {}")));
        assert!(!names_type("String", "java.lang", "String", &facts("import com.acme.String;\nclass T {}")));
        assert!(!names_type("String", "java.lang", "String", &facts("class T {} class String {}")));
    }
}
