//! The value of a compile-time constant field, read off its class file.
//!
//! javac writes a `ConstantValue` attribute on every `static final` field of primitive or `String`
//! type whose initializer is a constant expression — `Integer.MAX_VALUE`, `HttpServletResponse.SC_OK`,
//! a library's `DEFAULT_TIMEOUT`. It is the only place the value survives in a jar, and the one thing
//! a reader hovering such a name wants to know.
//!
//! A **separate** lookup, like the class annotations, and not a field on
//! [`ClassMembers`](crate::members::ClassMembers): that struct is memoized for every class the
//! resolver touches and persisted in the shared JDK index, while this is asked once per hover.
//!
//! The value comes back as **Java source**: `42`, `30000L`, `1.5f`, `true`, `'\n'`, `"utf-8"` — the
//! literal you would write to get it. The attribute stores a boolean, a char, a byte and a short as an
//! `int`, so the field's descriptor is what says which one it is.

use cafebabe::constant_pool::LiteralConstant;
use cafebabe::descriptors::FieldType;
use cafebabe::attributes::AttributeData;
use cafebabe::parse_class;

/// A string constant longer than this is cut, with `…` before the closing quote: a hover line is not
/// the place for a whole SQL statement.
const MAX_STRING_CHARS: usize = 120;

/// The constant value of field `field` of the class `binary_name` served by `source`, as a Java
/// literal. `None` when the class is absent or malformed, the field is not there, or it carries no
/// `ConstantValue` (not a compile-time constant).
pub fn field_constant_of(
    source: &dyn crate::source::ClassSource,
    binary_name: &str,
    field: &str,
) -> Option<String> {
    match source.class_bytes(binary_name) {
        Ok(Some(bytes)) => field_constant(&bytes, field),
        _ => None,
    }
}

/// The constant value of field `field` in the class file `bytes`, as a Java literal.
pub fn field_constant(bytes: &[u8], field: &str) -> Option<String> {
    let class = parse_class(bytes).ok()?;
    let f = class.fields.iter().find(|f| f.name == field)?;
    if f.descriptor.dimensions != 0 {
        return None;
    }
    f.attributes.iter().find_map(|a| match &a.data {
        AttributeData::ConstantValue(value) => java_literal(value, &f.descriptor.field_type),
        _ => None,
    })
}

/// `value` written as the Java literal of a field of type `ty`.
fn java_literal(value: &LiteralConstant<'_>, ty: &FieldType<'_>) -> Option<String> {
    Some(match (value, ty) {
        (LiteralConstant::Integer(v), FieldType::Boolean) => (*v != 0).to_string(),
        (LiteralConstant::Integer(v), FieldType::Char) => {
            char_literal(char::from_u32(u32::try_from(*v).ok()?)?)
        }
        (LiteralConstant::Integer(v), _) => v.to_string(),
        (LiteralConstant::Long(v), _) => format!("{v}L"),
        (LiteralConstant::Float(v), _) => float_literal(f64::from(*v), &format!("{v:?}"), "f"),
        (LiteralConstant::Double(v), _) => float_literal(*v, &format!("{v:?}"), ""),
        (LiteralConstant::String(s), _) => string_literal(s),
        // Not valid modified UTF-8: there is no honest way to write it as a literal.
        (LiteralConstant::StringBytes(_), _) => return None,
    })
}

/// A floating value as Java prints one: `NaN` and the infinities by name (they have no literal),
/// everything else as its shortest round-trip digits plus the type suffix.
fn float_literal(v: f64, digits: &str, suffix: &str) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else {
        format!("{digits}{suffix}")
    }
}

fn char_literal(c: char) -> String {
    match c {
        '\'' => "'\\''".to_string(),
        _ => format!("'{}'", escape(c)),
    }
}

fn string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len().min(MAX_STRING_CHARS) + 2);
    out.push('"');
    for (i, c) in s.chars().enumerate() {
        if i == MAX_STRING_CHARS {
            out.push('…');
            break;
        }
        match c {
            '"' => out.push_str("\\\""),
            _ => out.push_str(&escape(c)),
        }
    }
    out.push('"');
    out
}

/// One character as it is written inside a Java literal.
fn escape(c: char) -> String {
    match c {
        '\\' => "\\\\".to_string(),
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        '\u{8}' => "\\b".to_string(),
        '\u{c}' => "\\f".to_string(),
        '\0' => "\\0".to_string(),
        c if c.is_control() => format!("\\u{:04x}", c as u32),
        c => c.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integral_values_read_as_the_type_the_descriptor_names() {
        assert_eq!(java_literal(&LiteralConstant::Integer(1), &FieldType::Boolean).unwrap(), "true");
        assert_eq!(java_literal(&LiteralConstant::Integer(0), &FieldType::Boolean).unwrap(), "false");
        assert_eq!(java_literal(&LiteralConstant::Integer(65), &FieldType::Char).unwrap(), "'A'");
        assert_eq!(java_literal(&LiteralConstant::Integer(10), &FieldType::Char).unwrap(), "'\\n'");
        assert_eq!(java_literal(&LiteralConstant::Integer(39), &FieldType::Char).unwrap(), "'\\''");
        assert_eq!(java_literal(&LiteralConstant::Integer(-7), &FieldType::Integer).unwrap(), "-7");
        assert_eq!(java_literal(&LiteralConstant::Integer(200), &FieldType::Short).unwrap(), "200");
        assert_eq!(java_literal(&LiteralConstant::Long(30_000), &FieldType::Long).unwrap(), "30000L");
    }

    #[test]
    fn floating_values_carry_their_suffix_and_name_the_specials() {
        assert_eq!(java_literal(&LiteralConstant::Float(1.5), &FieldType::Float).unwrap(), "1.5f");
        assert_eq!(java_literal(&LiteralConstant::Double(0.1), &FieldType::Double).unwrap(), "0.1");
        assert_eq!(java_literal(&LiteralConstant::Double(f64::NAN), &FieldType::Double).unwrap(), "NaN");
        assert_eq!(
            java_literal(&LiteralConstant::Float(f32::NEG_INFINITY), &FieldType::Float).unwrap(),
            "-Infinity"
        );
    }

    #[test]
    fn strings_are_quoted_escaped_and_cut_when_long() {
        let s = |v: &str| java_literal(&LiteralConstant::String(v.into()), &FieldType::Integer).unwrap();
        assert_eq!(s("utf-8"), "\"utf-8\"");
        assert_eq!(s("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        let long = "x".repeat(MAX_STRING_CHARS + 10);
        let shown = s(&long);
        assert!(shown.ends_with("…\""), "{shown}");
        assert_eq!(shown.chars().count(), MAX_STRING_CHARS + 3);
    }
}
