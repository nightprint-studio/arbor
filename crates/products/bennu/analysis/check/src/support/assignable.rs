//! Whether a value of one type can be passed or assigned where another is declared — answered only
//! on evidence nothing unread could overturn.
//!
//! The rule is the invocation context's (JLS §5.3), which an argument is judged by
//! ([`crate::calls::arguments`]). An assignment ([`crate::typing::casts`]) allows one conversion more
//! — a constant narrowed to `byte`, `short` or `char` — and sets those pairs aside before asking.
//!
//! * two primitives no widening joins (`long` where `int`, `boolean` where `int`). An invocation
//!   context never narrows, not even a constant: `takesByte(1)` does not compile, so the widening
//!   table `bennu_java` applies overloads with is the whole rule;
//! * `String` against any primitive, either way;
//! * a primitive where a reference type its box class does not reach is declared (`int` where
//!   `Animal`, `int` where `Long`);
//! * a reference where a primitive is declared, when it is not one of the eight box classes
//!   (`Widget` where `int`);
//! * `null` where a primitive is declared;
//! * two reference types, arrays included, the value proven not a subtype of the target by the same
//!   verdict the applicability rules use (`bennu_java`'s `subtype_verdict`: a readable superclass
//!   chain settles a class, a `final` class needs nothing, an interface needs the whole hierarchy).
//!
//! `Object` as the value's type — or as its element type — is judged only when the expression spells
//! it ([`spells_its_type`]): an inferred `Object` is what an erased type variable reads as. Type
//! variables themselves are never judged.

use bennu_java::prelude::{primitive_widens, subtype_verdict, TypeRef, TypeResolver};
use tree_sitter::Node;

use crate::support::nodes::{is_primitive, is_type_var, simple_name};

const OBJECT: &str = "java/lang/Object";

/// A value refused: its type and the declared one, rendered for the message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mismatch {
    pub(crate) found: String,
    pub(crate) expected: String,
}

/// The definite mismatch of a `value` where `target` is declared, or `None` when the pair is
/// compatible or not provably incompatible. `written` says the expression spells its own type.
pub(crate) fn definite_mismatch(
    value: &TypeRef,
    target: &TypeRef,
    written: bool,
    resolver: &dyn TypeResolver,
) -> Option<Mismatch> {
    let (a, p) = (value.binary_name.as_str(), target.binary_name.as_str());
    if a.is_empty() || p.is_empty() || is_type_var(a) || is_type_var(p) {
        return None;
    }
    // The legacy `Foo[]` spelling keeps its depth in the name, where no verdict looks for it.
    if a.ends_with("[]") || p.ends_with("[]") {
        return None;
    }
    if a == OBJECT && !written {
        return None;
    }
    if value.dims > 0 || target.dims > 0 {
        return array_mismatch(value, target, resolver);
    }
    match (is_primitive(a), is_primitive(p)) {
        (true, true) => primitive_mismatch(a, p),
        (true, false) => boxing_mismatch(a, p, resolver),
        (false, true) => unboxing_mismatch(a, p, resolver),
        (false, false) => reference_mismatch(a, p, resolver),
    }
}

/// `null` fits every reference and no primitive.
pub(crate) fn null_mismatch(target: &TypeRef) -> Option<Mismatch> {
    let p = target.binary_name.as_str();
    (target.dims == 0 && is_primitive(p) && p != "void")
        .then(|| Mismatch { found: "null".to_string(), expected: p.to_string() })
}

/// Whether `node` spells the type it has — `new Object()`, `new Object[0]`, `(Object) value` —
/// which is what makes an `Object` trustworthy.
pub(crate) fn spells_its_type(node: Node) -> bool {
    matches!(
        node.kind(),
        "object_creation_expression" | "array_creation_expression" | "cast_expression"
    )
}

/// Whether `node` is a local or a parameter DECLARED `Object` — `void m(Object value)`, `Object o = …`.
///
/// The other way an `Object` is trustworthy: an erased type variable reads as `Object` only where
/// inference had to erase it, and a variable whose declaration says `Object` erased nothing. Found
/// lexically, through the enclosing blocks and the enclosing method's or lambda's parameters; a
/// field, or a name nothing encloses, answers `false`.
pub(crate) fn names_a_declared_object(node: Node, bytes: &[u8]) -> bool {
    if node.kind() != "identifier" {
        return false;
    }
    let Ok(name) = node.utf8_text(bytes) else { return false };
    let is_object = |ty: Option<Node>| {
        ty.and_then(|t| t.utf8_text(bytes).ok()).is_some_and(|t| matches!(t.trim(), "Object" | "java.lang.Object"))
    };
    let declares = |declaration: Node| {
        let mut c = declaration.walk();
        let found = declaration.named_children(&mut c).any(|d| {
            d.kind() == "variable_declarator"
                && d.child_by_field_name("dimensions").is_none()
                && d.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) == Some(name)
        });
        found
    };
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            "block" => {
                let mut c = n.walk();
                for stmt in n.named_children(&mut c).take_while(|s| s.start_byte() < node.start_byte()) {
                    if stmt.kind() == "local_variable_declaration" && declares(stmt) {
                        return is_object(stmt.child_by_field_name("type"));
                    }
                }
            }
            "method_declaration" | "constructor_declaration" | "lambda_expression" => {
                if let Some(params) = n.child_by_field_name("parameters") {
                    let mut c = params.walk();
                    for p in params.named_children(&mut c) {
                        let named = p.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) == Some(name);
                        if p.kind() == "formal_parameter" && named {
                            return p.child_by_field_name("dimensions").is_none() && is_object(p.child_by_field_name("type"));
                        }
                    }
                }
                if n.kind() != "lambda_expression" {
                    return false;
                }
            }
            "class_body" => return false,
            _ => {}
        }
        cur = n.parent();
    }
    false
}

/// Only widening joins two primitives in an invocation context, and `void` is not a value.
fn primitive_mismatch(a: &str, p: &str) -> Option<Mismatch> {
    if a == "void" || p == "void" {
        return None;
    }
    (!primitive_widens(a, p)).then(|| mismatch(a, p))
}

/// A primitive where a reference is declared boxes to exactly one class (JLS §5.1.7), which must
/// then be a subtype of the target.
fn boxing_mismatch(a: &str, p: &str, resolver: &dyn TypeResolver) -> Option<Mismatch> {
    if p == "java/lang/String" {
        return Some(mismatch(a, p));
    }
    let boxed = box_of(a)?;
    definitely_not_subtype(&TypeRef::simple(boxed), &TypeRef::simple(p), resolver)
        .then(|| mismatch(a, p))
}

/// A reference where a primitive is declared must unbox, which only the box classes do.
fn unboxing_mismatch(a: &str, p: &str, resolver: &dyn TypeResolver) -> Option<Mismatch> {
    if p == "void" {
        return None;
    }
    if a == "java/lang/String" {
        return Some(mismatch(a, p));
    }
    if is_box(a) || resolver.members_of(a).is_none() {
        return None;
    }
    Some(mismatch(a, p))
}

fn reference_mismatch(a: &str, p: &str, resolver: &dyn TypeResolver) -> Option<Mismatch> {
    if p == OBJECT || same_simple_name(a, p) {
        return None;
    }
    definitely_not_subtype(&TypeRef::simple(a), &TypeRef::simple(p), resolver)
        .then(|| mismatch(a, p))
}

/// An array on either side: `Object[]` where `String[]`, `int[]` where `long[]`, a `String` where
/// `String[]`. The subtype verdict reads arrays itself (JLS §4.10.3); no conversion but identity and
/// subtyping ever reaches or leaves one.
fn array_mismatch(value: &TypeRef, target: &TypeRef, resolver: &dyn TypeResolver) -> Option<Mismatch> {
    if same_simple_name(&value.binary_name, &target.binary_name) && value.dims == target.dims {
        return None;
    }
    definitely_not_subtype(value, target, resolver).then(|| Mismatch {
        found: value.with_brackets(&render(&value.binary_name)),
        expected: target.with_brackets(&render(&target.binary_name)),
    })
}

/// Same SIMPLE name, different binaries → almost always the SAME logical type resolved to two
/// different binary FORMS: a nested type spelled `Outer/Inner` (from a source FQN) vs `Outer$Inner`
/// (from bytecode), or two files resolving the simple name through different packages. Passing a
/// value where the SAME type is expected is legal, so the "`ComunicazioneType` cannot be passed
/// where `ComunicazioneType` is expected" report is a false positive → don't flag (sound: at worst
/// a missed genuine same-simple-name mismatch across packages, which is rare and low-value).
fn same_simple_name(a: &str, p: &str) -> bool {
    simple_name(a) == simple_name(p)
}

/// Whether `sub` — a type the classpath can read, or a primitive — is proven NOT to be `sup` or a
/// subtype of it.
fn definitely_not_subtype(sub: &TypeRef, sup: &TypeRef, resolver: &dyn TypeResolver) -> bool {
    (is_primitive(&sub.binary_name) || resolver.members_of(&sub.binary_name).is_some())
        && subtype_verdict(resolver, sub, sup) == Some(false)
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
