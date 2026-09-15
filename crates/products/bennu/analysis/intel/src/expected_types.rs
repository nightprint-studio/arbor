//! Which type NAMES a position wants written — the expected type read as a question about the
//! class-name list rather than about a member list.
//!
//! `return Ra|` in a method returning `RawIdentity` is a class name being started, and the one the
//! method needs is the one about to be written: `return RawIdentity.builder()…build()`. The
//! class-name index knows how well `RawIdentity` matches `Ra` and how near its package is, and on
//! both counts it ties with every other `Ra…` class — or loses, to a `Random` the file imports. The
//! `return` is what breaks the tie, and it is a stronger fact than anything the index has.
//!
//! ## Which names, in which order
//!
//! 1. **The expected type itself** — `RawIdentity`, `Optional`, `List`. What the author spelled.
//! 2. **Its type arguments** — `RawIdentity` for an `Optional<RawIdentity>`. The container is
//!    written with a factory (`Optional.of(…)`, `List.of(…)`), and what goes inside is built the
//!    same way the bare type is: `return Optional.of(RawIdentity.builder()…build())` names both, and
//!    the element is the one a `Ra` prefix is reaching for.
//! 3. **A subtype of the expected type** — `ArrayList` for a `List`, `RackImpl` for a `Rack`. It
//!    compiles, it is a common way to build the value, and it comes after what was spelled.
//!
//! The match tier still leads all three (see the caller): this orders names that answer the prefix
//! equally well, and never lifts a camel-hump hit over an exact prefix.

use bennu_java::prelude::TypeRef;

/// How close a type name is to what the position wants. Ordered best first, so it sorts ascending
/// in a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Closeness {
    Exact,
    Element,
    Subtype,
    Unrelated,
}

/// One class the position names: its simple name, and its binary name when the expected type
/// resolved (an unimported return type does not — it is still worth matching by name).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Wanted {
    simple: String,
    binary: Option<String>,
}

impl Wanted {
    /// `None` for what no class name can be: a primitive, `void`, `Object` (which every class is, so
    /// it says nothing about which one).
    fn of(t: &TypeRef) -> Option<Self> {
        let name = t.binary_name.as_str();
        if name == "java/lang/Object" {
            return None;
        }
        let (simple, binary) = if name.contains('/') {
            (name.rsplit(['/', '$']).next()?, Some(name.to_string()))
        } else {
            (name.rsplit('.').next()?, None)
        };
        simple
            .starts_with(|c: char| c.is_uppercase())
            .then(|| Wanted { simple: simple.to_string(), binary })
    }

    /// Whether the class name `simple`, declared at `candidates` (dotted FQNs), is this one.
    fn is(&self, simple: &str, candidates: &[String]) -> bool {
        self.simple == simple
            && self.binary.as_ref().map_or(true, |binary| {
                let binary = binary.replace('$', "/");
                candidates.iter().any(|fqn| fqn.replace('.', "/") == binary)
            })
    }
}

/// The type names an expected type asks for — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WantedTypes {
    outer: Wanted,
    elements: Vec<Wanted>,
}

impl WantedTypes {
    /// `None` when the expected type names no class — `int`, `boolean`, `Object`.
    pub(crate) fn of(expected: &TypeRef) -> Option<Self> {
        let outer = Wanted::of(expected)?;
        let mut elements: Vec<Wanted> = Vec::new();
        for arg in &expected.type_args {
            let Some(w) = Wanted::of(arg) else { continue };
            if w.simple != outer.simple && !elements.iter().any(|e| e.simple == w.simple) {
                elements.push(w);
            }
        }
        Some(Self { outer, elements })
    }

    /// Every simple name asked for, the expected type first — for the sweep that must not cut them.
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.outer.simple.as_str()).chain(self.elements.iter().map(|e| e.simple.as_str()))
    }

    /// How close `simple` (declared at `candidates`) is to what is wanted.
    ///
    /// `is_subtype(candidate_fqn, wanted_binary)` is only asked when neither of the cheap answers
    /// applies, and only of an expected type that resolved — a subtype of a name nothing bound is
    /// not a question with an answer.
    pub(crate) fn closeness(
        &self,
        simple: &str,
        candidates: &[String],
        is_subtype: &dyn Fn(&str, &str) -> bool,
    ) -> Closeness {
        if self.outer.is(simple, candidates) {
            return Closeness::Exact;
        }
        if self.elements.iter().any(|e| e.is(simple, candidates)) {
            return Closeness::Element;
        }
        match &self.outer.binary {
            Some(binary) if candidates.iter().any(|fqn| is_subtype(fqn, binary)) => Closeness::Subtype,
            _ => Closeness::Unrelated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generic(name: &str, args: &[&str]) -> TypeRef {
        TypeRef { type_args: args.iter().map(|a| TypeRef::simple(*a)).collect(), ..TypeRef::simple(name) }
    }

    fn fqns(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn no_subtypes(_: &str, _: &str) -> bool {
        false
    }

    #[test]
    fn the_expected_type_is_exact() {
        let w = WantedTypes::of(&TypeRef::simple("com/acme/RawIdentity")).expect("a class");
        assert_eq!(w.closeness("RawIdentity", &fqns(&["com.acme.RawIdentity"]), &no_subtypes), Closeness::Exact);
        assert_eq!(w.closeness("Random", &fqns(&["java.util.Random"]), &no_subtypes), Closeness::Unrelated);
    }

    /// Same simple name, different package: not the type the method returns.
    #[test]
    fn a_namesake_in_another_package_is_not_the_expected_type() {
        let w = WantedTypes::of(&TypeRef::simple("com/acme/RawIdentity")).unwrap();
        assert_eq!(w.closeness("RawIdentity", &fqns(&["org.other.RawIdentity"]), &no_subtypes), Closeness::Unrelated);
    }

    /// A return type the file has not imported yet resolves to nothing — its name still counts.
    #[test]
    fn an_unresolved_expected_type_matches_by_name() {
        let w = WantedTypes::of(&TypeRef::simple("RawRecord")).unwrap();
        assert_eq!(w.closeness("RawRecord", &fqns(&["com.acme.model.RawRecord"]), &no_subtypes), Closeness::Exact);
    }

    #[test]
    fn a_nested_expected_type_matches_its_dotted_fqn() {
        let w = WantedTypes::of(&TypeRef::simple("com/acme/Order$Line")).unwrap();
        assert_eq!(w.closeness("Line", &fqns(&["com.acme.Order.Line"]), &no_subtypes), Closeness::Exact);
    }

    /// `Optional<RawIdentity>`: the container first, the element right after it.
    #[test]
    fn a_generic_expected_type_wants_its_arguments_next() {
        let w = WantedTypes::of(&generic("java/util/Optional", &["com/acme/RawIdentity"])).unwrap();
        assert_eq!(w.names().collect::<Vec<_>>(), ["Optional", "RawIdentity"]);
        assert_eq!(w.closeness("Optional", &fqns(&["java.util.Optional"]), &no_subtypes), Closeness::Exact);
        assert_eq!(w.closeness("RawIdentity", &fqns(&["com.acme.RawIdentity"]), &no_subtypes), Closeness::Element);
    }

    #[test]
    fn a_subtype_of_the_expected_type_comes_after_the_type_and_its_arguments() {
        let w = WantedTypes::of(&generic("java/util/List", &["com/acme/RawIdentity"])).unwrap();
        let subtype = |candidate: &str, wanted: &str| candidate == "java.util.ArrayList" && wanted == "java/util/List";
        assert_eq!(w.closeness("ArrayList", &fqns(&["java.util.ArrayList"]), &subtype), Closeness::Subtype);
        assert!(Closeness::Exact < Closeness::Element && Closeness::Element < Closeness::Subtype);
    }

    /// No class name is `int`, and every class is an `Object`.
    #[test]
    fn a_primitive_or_object_wants_no_type_name() {
        assert_eq!(WantedTypes::of(&TypeRef::simple("int")), None);
        assert_eq!(WantedTypes::of(&TypeRef::simple("java/lang/Object")), None);
    }
}
