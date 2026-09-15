//! What counts as a **definite** argument/parameter mismatch — the one question [`super`] asks per
//! position, answered only on evidence nothing unread could overturn.
//!
//! * `boolean` against a numeric primitive, and `String` against any primitive, either way;
//! * a primitive passed to a reference type its box class does not reach (`int` where `Animal`);
//! * a reference passed to a primitive that is not one of the eight box classes (`Widget` where
//!   `int`) — `Object` excepted, being what an erased type variable reads as;
//! * two reference types, the argument proven not a subtype of the parameter by the same verdict the
//!   applicability rules use (`bennu_java`'s `subtype_verdict`: a readable superclass chain settles a
//!   class, a `final` class needs nothing, an interface needs the whole hierarchy).
//!
//! Numeric widening and narrowing, arrays, type variables and `Object` are never judged.

use bennu_java::prelude::{subtype_verdict, TypeRef, TypeResolver};

use crate::support::nodes::{is_primitive, is_type_var, simple_name};

/// A position refused: the argument's type and the parameter's, rendered for the message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Mismatch {
    pub(super) found: String,
    pub(super) expected: String,
}

/// The definite mismatch of passing `arg` where `param` is declared, or `None` when the pair is
/// compatible or not provably incompatible.
pub(super) fn arg_mismatch(
    arg: &TypeRef,
    param: &TypeRef,
    resolver: &dyn TypeResolver,
) -> Option<Mismatch> {
    let (a, p) = (arg.binary_name.as_str(), param.binary_name.as_str());
    if a.is_empty() || p.is_empty() || arg.is_array() || param.is_array() {
        return None;
    }
    if is_type_var(a) || is_type_var(p) {
        return None;
    }
    match (is_primitive(a), is_primitive(p)) {
        (true, true) => primitive_mismatch(a, p),
        (true, false) => boxing_mismatch(a, p, resolver),
        (false, true) => unboxing_mismatch(a, p, resolver),
        (false, false) => reference_mismatch(a, p, resolver),
    }
}

/// `boolean` converts to no numeric type and none converts to it. Numeric pairs are left to the
/// widening rules, and `void` is not a value.
fn primitive_mismatch(a: &str, p: &str) -> Option<Mismatch> {
    if a == "void" || p == "void" {
        return None;
    }
    ((a == "boolean") != (p == "boolean")).then(|| mismatch(a, p))
}

/// A primitive passed to a reference parameter boxes to exactly one class (JLS §5.3), which must
/// then be a subtype of the parameter.
fn boxing_mismatch(a: &str, p: &str, resolver: &dyn TypeResolver) -> Option<Mismatch> {
    if p == "java/lang/String" {
        return Some(mismatch(a, p));
    }
    let boxed = box_of(a)?;
    definitely_not_subtype(boxed, p, resolver).then(|| mismatch(a, p))
}

/// A reference passed to a primitive parameter must unbox (JLS §5.3), which only the box classes do.
/// `Object` is left alone: it is what a type variable the inference could not bind reads as.
fn unboxing_mismatch(a: &str, p: &str, resolver: &dyn TypeResolver) -> Option<Mismatch> {
    if p == "void" {
        return None;
    }
    if a == "java/lang/String" {
        return Some(mismatch(a, p));
    }
    if is_box(a) || a == "java/lang/Object" || resolver.members_of(a).is_none() {
        return None;
    }
    Some(mismatch(a, p))
}

fn reference_mismatch(a: &str, p: &str, resolver: &dyn TypeResolver) -> Option<Mismatch> {
    if a == "java/lang/Object" || p == "java/lang/Object" {
        return None;
    }
    // Same SIMPLE name, different binaries → almost always the SAME logical type resolved to two
    // different binary FORMS: a nested type spelled `Outer/Inner` (from a source FQN) vs `Outer$Inner`
    // (from bytecode), or two files resolving the simple name through different packages. Passing a
    // value where the SAME type is expected is legal, so the "`ComunicazioneType` cannot be passed
    // where `ComunicazioneType` is expected" report is a false positive → don't flag (sound: at worst
    // a missed genuine same-simple-name mismatch across packages, which is rare and low-value).
    if simple_name(a) == simple_name(p) {
        return None;
    }
    definitely_not_subtype(a, p, resolver).then(|| mismatch(a, p))
}

/// Whether `sub` — a type the classpath can read — is proven NOT to be `sup` or a subtype of it.
fn definitely_not_subtype(sub: &str, sup: &str, resolver: &dyn TypeResolver) -> bool {
    resolver.members_of(sub).is_some()
        && subtype_verdict(resolver, &TypeRef::simple(sub), &TypeRef::simple(sup)) == Some(false)
}

fn mismatch(a: &str, p: &str) -> Mismatch {
    Mismatch { found: render(a), expected: render(p) }
}

/// A primitive as written, a class by its simple name.
fn render(binary: &str) -> String {
    match is_primitive(binary) {
        true => binary.to_string(),
        false => simple_name(binary).to_string(),
    }
}

/// The class a primitive boxes to (JLS §5.1.7).
fn box_of(primitive: &str) -> Option<&'static str> {
    Some(match primitive {
        "boolean" => "java/lang/Boolean",
        "byte" => "java/lang/Byte",
        "short" => "java/lang/Short",
        "char" => "java/lang/Character",
        "int" => "java/lang/Integer",
        "long" => "java/lang/Long",
        "float" => "java/lang/Float",
        "double" => "java/lang/Double",
        _ => return None,
    })
}

fn is_box(binary: &str) -> bool {
    matches!(
        binary,
        "java/lang/Boolean"
            | "java/lang/Byte"
            | "java/lang/Short"
            | "java/lang/Character"
            | "java/lang/Integer"
            | "java/lang/Long"
            | "java/lang/Float"
            | "java/lang/Double"
    )
}
