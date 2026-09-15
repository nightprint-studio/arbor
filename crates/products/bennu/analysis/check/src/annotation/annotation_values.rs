//! An annotation element given a literal its declared type cannot hold.
//!
//! Decided from the value's SYNTAX against the element's declared type, with the resolver asked only
//! what family that type belongs to (an enum, an annotation):
//!
//!   * **an array where the element is not one** — `@Ann(i = {1, 2})` with `int i()`. This is
//!     javac's `annotation.value.not.allowable.type` proper. The reverse is legal and is NOT flagged:
//!     `@Column(name = "a")` for a `String[]` element is Java's single-element shorthand;
//!   * **a literal of the wrong kind** — a string where a number, a class literal or an enum constant
//!     is declared, a number where a `String` is, a boolean where either is;
//!   * **a numeric literal an assignment cannot convert** — `1L` for an `int`, `1.5` for a `float`,
//!     `200` for a `byte`. An element value is assigned, so the constant narrowing assignment allows
//!     (`byte b() default 1`) is allowed here too.
//!
//! Every entry of an array initialiser is judged against the element type of the array. Only LITERALS
//! are judged: a bare name may be a `static final` constant of any type, and deciding that needs
//! constant folding.

use bennu_java::prelude::{split_array_dims, TypeRef, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;

/// Judge `value`, written for the element `key` declared `declared`.
pub(crate) fn check_value_type(
    value: Node,
    declared: &TypeRef,
    key: &str,
    bytes: &[u8],
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    // Array-ness is asked of the TYPE, never read off its spelling — `TypeRef::is_array` is the one
    // place that knows, and it answers for both shapes an index can hold. (Reading the name was a
    // bug once: every `String[]` element read as holding one value after the depth moved to `dims`.)
    let is_array = declared.is_array();
    if value.kind() == "element_value_array_initializer" {
        if !is_array {
            out.push(CheckId::AnnotationValueType.at(
                value,
                format!("`{key}` is declared `{}`, which holds one value, not a list", pretty(declared)),
            ));
            return;
        }
        let element = element_type(declared);
        let mut c = value.walk();
        let entries: Vec<Node> = value.named_children(&mut c).collect();
        for entry in entries {
            check_one(entry, &element, declared, key, bytes, resolver, out);
        }
        return;
    }
    // A single value for an array element is the shorthand for a one-entry list.
    let element = if is_array { element_type(declared) } else { declared.clone() };
    check_one(value, &element, declared, key, bytes, resolver, out);
}

/// A declared type as a Java reader would write it — `java/lang/String` + `dims: 1` → `String[]`.
///
/// The depth is taken from wherever it is: `dims` for anything the current index built, brackets in
/// the name for a record persisted before `dims` existed.
pub(crate) fn pretty(ty: &TypeRef) -> String {
    let (base, in_name) = split_array_dims(&ty.binary_name);
    let name = base.rsplit(['/', '$']).next().unwrap_or(base);
    format!("{name}{}", "[]".repeat(in_name.max(ty.dims as usize)))
}

/// One level of array stripped off `declared`, from whichever place holds the depth.
fn element_type(declared: &TypeRef) -> TypeRef {
    let (base, in_name) = split_array_dims(&declared.binary_name);
    match (declared.dims, in_name) {
        (d, _) if d > 0 => TypeRef { dims: d - 1, ..declared.clone() },
        (_, n) if n > 0 => TypeRef::simple(format!("{base}{}", "[]".repeat(n - 1))),
        _ => declared.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
fn check_one(
    value: Node,
    element: &TypeRef,
    declared: &TypeRef,
    key: &str,
    bytes: &[u8],
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(got) = literal_of(value, bytes) else { return };
    if element.is_array() {
        return;
    }
    let Some(want) = family_of(element, resolver) else { return };
    if let Some(problem) = mismatch(&got, want) {
        out.push(CheckId::AnnotationValueType.at(
            value,
            format!("`{key}` is declared `{}`, and {problem}", pretty(declared)),
        ));
    }
}

/// What a literal is.
enum Literal {
    String,
    Boolean,
    /// An `int` literal and its value, when it could be read.
    Int(Option<i64>),
    Long,
    Float,
    Double,
    Char,
}

/// The family an element type belongs to.
#[derive(Clone, Copy)]
enum Family {
    String,
    Boolean,
    Numeric(&'static str),
    Class,
    Enum,
    Annotation,
}

fn literal_of(value: Node, bytes: &[u8]) -> Option<Literal> {
    let text = value.utf8_text(bytes).ok()?.trim();
    Some(match value.kind() {
        "string_literal" | "text_block" => Literal::String,
        "true" | "false" => Literal::Boolean,
        "character_literal" => Literal::Char,
        "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal" | "binary_integer_literal" => {
            if text.ends_with(['l', 'L']) {
                Literal::Long
            } else {
                Literal::Int(int_value(text))
            }
        }
        "decimal_floating_point_literal" | "hex_floating_point_literal" => {
            if text.ends_with(['f', 'F']) {
                Literal::Float
            } else {
                Literal::Double
            }
        }
        _ => return None,
    })
}

/// The value of an `int` literal as written — underscores, hex, octal and binary included.
fn int_value(text: &str) -> Option<i64> {
    let digits: String = text.chars().filter(|c| *c != '_').collect();
    let lower = digits.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix("0x") {
        return i64::from_str_radix(hex, 16).ok();
    }
    if let Some(bin) = lower.strip_prefix("0b") {
        return i64::from_str_radix(bin, 2).ok();
    }
    if lower.len() > 1 && lower.starts_with('0') {
        return i64::from_str_radix(&lower[1..], 8).ok();
    }
    lower.parse().ok()
}

fn family_of(element: &TypeRef, resolver: &dyn TypeResolver) -> Option<Family> {
    Some(match element.binary_name.as_str() {
        "java/lang/String" => Family::String,
        "java/lang/Class" => Family::Class,
        "boolean" => Family::Boolean,
        "int" => Family::Numeric("int"),
        "long" => Family::Numeric("long"),
        "short" => Family::Numeric("short"),
        "byte" => Family::Numeric("byte"),
        "char" => Family::Numeric("char"),
        "float" => Family::Numeric("float"),
        "double" => Family::Numeric("double"),
        other => {
            let members = resolver.members_of(other)?;
            if members.flags.is_enum {
                Family::Enum
            } else if members.flags.is_annotation {
                Family::Annotation
            } else {
                return None;
            }
        }
    })
}

/// Why `got` cannot be the value of an element of family `want`, or `None` when it can.
fn mismatch(got: &Literal, want: Family) -> Option<String> {
    let noun = match got {
        Literal::String => "this is a string",
        Literal::Boolean => "this is a boolean",
        _ => "this is a number",
    };
    let wrong_kind = Some(noun.to_string());
    match want {
        Family::String => (!matches!(got, Literal::String)).then(|| noun.to_string()),
        Family::Boolean => (!matches!(got, Literal::Boolean)).then(|| noun.to_string()),
        Family::Class => wrong_kind.map(|n| format!("{n}, not a class literal")),
        Family::Enum => wrong_kind.map(|n| format!("{n}, not an enum constant")),
        Family::Annotation => wrong_kind.map(|n| format!("{n}, not an annotation")),
        Family::Numeric(to) => numeric_mismatch(got, to),
    }
}

/// Assignment conversion of a numeric literal to the primitive `to` (JLS §5.2): widening, plus the
/// narrowing of an `int` or `char` constant to `byte`, `short` or `char` when the value fits.
fn numeric_mismatch(got: &Literal, to: &str) -> Option<String> {
    let lossy = |from: &str| Some(format!("this `{from}` literal is a possible lossy conversion to `{to}`"));
    match got {
        Literal::String => Some("this is a string".to_string()),
        Literal::Boolean => Some("this is a boolean".to_string()),
        Literal::Char => None,
        Literal::Int(value) => {
            let range = match to {
                "byte" => Some((i64::from(i8::MIN), i64::from(i8::MAX))),
                "short" => Some((i64::from(i16::MIN), i64::from(i16::MAX))),
                "char" => Some((0, i64::from(u16::MAX))),
                _ => None,
            };
            match (range, value) {
                (Some((lo, hi)), Some(v)) if *v < lo || *v > hi => {
                    Some(format!("`{v}` does not fit in `{to}`"))
                }
                _ => None,
            }
        }
        Literal::Long => (!matches!(to, "long" | "float" | "double")).then(|| lossy("long")).flatten(),
        Literal::Float => (!matches!(to, "float" | "double")).then(|| lossy("float")).flatten(),
        Literal::Double => (to != "double").then(|| lossy("double")).flatten(),
    }
}
