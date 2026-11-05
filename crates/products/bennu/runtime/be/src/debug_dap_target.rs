//! A stopped native program, read through the DAP **variables tree**, as a [`DebugTarget`] for the
//! shared expression language.
//!
//! The tree is the same source the panel shows, so `v[0] + 1` in the watch box reads the row `[0]`
//! under `v` — and means the same on CodeLLDB, lldb-dap and GDB, which disagree about everything an
//! `evaluate` request would be handed. No expression is sent to the adapter here: the shared
//! evaluator decides what an operator means, and this only fetches variables.
//!
//! ## Reading a Rust value
//!
//! The adapter hands every value over as **text** plus a type name, so a scalar is read back from
//! its rendering ([`scalar`]): `42` with type `i32` is an integer, `"hi"` with `&str` a string, a
//! `0x0` raw pointer null. Everything else — a struct, a `Vec`, an enum — stays a reference into the
//! tree, which can be followed with `.field` / `[i]` / `*` but not compared.
//!
//! Integers are all read as `i64`. A `u64` above `i64::MAX` is refused by name, not wrapped.

use bennu_dap::prelude::Variable;
use bennu_debug_expr::prelude::{DebugTarget, Prim, Step, Val};
use bennu_proto::prelude::DebugValue;

use crate::debug_dap::{value_dto, DapSession};
use crate::debug_expr;

/// A variable of the stopped frame, kept whole: the adapter's own rendering, type and handle.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DRef(pub(crate) Variable);

pub(crate) struct DapTarget<'a> {
    session: &'a DapSession,
    frame_id: i64,
}

impl<'a> DapTarget<'a> {
    pub(crate) fn new(session: &'a DapSession, frame_id: i64) -> Self {
        DapTarget { session, frame_id }
    }

    fn lift(&self, variable: Variable) -> Result<Val<DRef>, String> {
        match scalar(variable.type_name.as_deref().unwrap_or(""), &variable.value)
            .map_err(|why| format!("`{}`: {why}", variable.name))?
        {
            Some(value) => Ok(value),
            None => Ok(Val::Ref(DRef(variable))),
        }
    }
}

impl DebugTarget for DapTarget<'_> {
    type Ref = DRef;

    fn variable(&self, name: &str) -> Result<Option<Val<DRef>>, String> {
        // No fallback to a field of `self`, deliberately — Rust has no implicit receiver. A name
        // that is not in the frame is an error that lists what is, not "maybe a type".
        let variable = debug_expr::root(self.session, self.frame_id, name)?;
        self.lift(variable).map(Some)
    }

    fn field(&self, owner: &DRef, name: &str, of: &str) -> Result<Val<DRef>, String> {
        let next =
            debug_expr::follow(self.session, &owner.0, &Step::Field(name.to_string()), of)?;
        self.lift(next)
    }

    fn index(&self, owner: &DRef, at: i64, of: &str) -> Result<Val<DRef>, String> {
        let at = i32::try_from(at).map_err(|_| format!("index {at} is outside `{of}`"))?;
        let next = debug_expr::follow(self.session, &owner.0, &Step::Index(at), of)?;
        self.lift(next)
    }

    fn deref(&self, pointer: &DRef, of: &str) -> Result<Val<DRef>, String> {
        let next = debug_expr::follow(self.session, &pointer.0, &Step::Deref, of)?;
        self.lift(next)
    }
}

/// The row for a computed result.
pub(crate) fn row_of(session: &DapSession, name: &str, value: Val<DRef>) -> DebugValue {
    let plain = |text: String, type_name: &str| DebugValue {
        name: name.to_string(),
        kind: "watch".to_string(),
        type_name: type_name.to_string(),
        value: text,
        object: None,
    };
    match value {
        Val::Prim(Prim::Bool(b)) => plain(b.to_string(), "bool"),
        Val::Prim(Prim::Char(c)) => {
            plain(char::from_u32(c).map(|ch| format!("{ch:?}")).unwrap_or_default(), "char")
        }
        Val::Prim(Prim::Int(v)) => plain(v.to_string(), "i64"),
        Val::Prim(Prim::Long(v)) => plain(v.to_string(), "i64"),
        Val::Prim(Prim::Double(v)) => plain(format!("{v:?}"), "f64"),
        Val::Str(text) => plain(format!("{text:?}"), "&str"),
        Val::Null => plain("null".to_string(), ""),
        Val::Ref(r) => DebugValue { name: name.to_string(), ..value_dto(session, &r.0, "watch") },
    }
}

// ── reading a scalar back from its rendering ────────────────────────────────────

/// What a variable's text and type say it is, when it is a scalar the evaluator can compute with.
///
/// `Ok(None)` is *not a scalar* — a struct, a `Vec`, an enum: the caller keeps it as a reference.
/// An `Err` is a scalar that cannot be read faithfully (a `u64` above `i64::MAX`), refused by name
/// because a wrapped number would be trusted.
pub(crate) fn scalar<R>(type_name: &str, value: &str) -> Result<Option<Val<R>>, String> {
    let base = type_name
        .trim()
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim_start_matches('&')
        .trim();
    let text = value.trim();
    match base {
        "bool" => match text {
            "true" => Ok(Some(Val::Prim(Prim::Bool(true)))),
            "false" => Ok(Some(Val::Prim(Prim::Bool(false)))),
            other => Err(format!("reads as `{other}`, which is not a bool")),
        },
        "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "usize" | "u64" | "i128"
        | "u128" => integer(base, text).map(|v| Some(Val::Prim(Prim::Long(v)))),
        "f32" | "f64" => text
            .parse::<f64>()
            .map(|v| Some(Val::Prim(Prim::Double(v))))
            .map_err(|_| format!("reads as `{text}`, which is not a number")),
        "char" => character(text).map(|c| Some(Val::Prim(Prim::Char(c)))),
        _ if is_string_type(base) => Ok(quoted(text).map(Val::Str)),
        _ if base.starts_with('*') => Ok(is_null_address(text).then_some(Val::Null)),
        _ => Ok(None),
    }
}

fn is_string_type(base: &str) -> bool {
    base == "str" || base == "String" || base.ends_with("::String") || base.ends_with(" str")
}

/// An integer as an adapter prints it: decimal, `0x…`, with `_` separators or a trailing
/// annotation (`97 'a'`). Only the first word counts.
fn integer(type_name: &str, text: &str) -> Result<i64, String> {
    let word = text.split_whitespace().next().unwrap_or("").replace('_', "");
    let parsed = match word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")) {
        Some(hex) => i128::from_str_radix(hex, 16),
        None => word.parse::<i128>(),
    };
    let Ok(wide) = parsed else {
        return Err(format!("reads as `{text}`, which is not a whole number"));
    };
    i64::try_from(wide).map_err(|_| {
        format!(
            "holds a {type_name} of {wide}, which does not fit the signed 64-bit numbers this \
             evaluates with — prefix with /nat to hand it to the debugger"
        )
    })
}

/// A `char` as printed: `'a'`, `'\n'`, or a number optionally followed by its glyph.
fn character(text: &str) -> Result<u32, String> {
    if let Some(inner) = text.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')) {
        let unescaped = unescape(inner);
        let mut chars = unescaped.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            return Ok(c as u32);
        }
    }
    text.split_whitespace()
        .next()
        .and_then(|word| word.parse::<u32>().ok())
        .ok_or_else(|| format!("reads as `{text}`, which is not a char"))
}

/// The text between the first and last `"` — which also drops the address an adapter may print
/// in front of a `&str` (`0x7ffe… "hi"`).
fn quoted(text: &str) -> Option<String> {
    let start = text.find('"')?;
    let end = text.rfind('"')?;
    (end > start).then(|| unescape(&text[start + 1..end]))
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Whether a raw pointer's rendering is the null address.
fn is_null_address(text: &str) -> bool {
    let word = text.split_whitespace().next().unwrap_or("");
    word.strip_prefix("0x")
        .map(|hex| !hex.is_empty() && hex.chars().all(|c| c == '0'))
        .unwrap_or(word == "0")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(type_name: &str, value: &str) -> Result<Option<Val<()>>, String> {
        scalar(type_name, value)
    }

    fn long(v: i64) -> Option<Val<()>> {
        Some(Val::Prim(Prim::Long(v)))
    }

    #[test]
    fn every_rust_integer_type_is_read_as_a_long() {
        for ty in ["i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "usize"] {
            assert_eq!(read(ty, "42"), Ok(long(42)), "{ty}");
        }
        assert_eq!(read("i32", "-7"), Ok(long(-7)));
        assert_eq!(read("u32", "1_000"), Ok(long(1000)));
        assert_eq!(read("u8", "0x2a"), Ok(long(42)));
        assert_eq!(read("&i32", "5"), Ok(long(5)), "a reference to an integer reads as the integer");
    }

    #[test]
    fn a_u64_that_does_not_fit_is_refused_not_wrapped() {
        let err = read("u64", "18446744073709551615").unwrap_err();
        assert!(err.contains("does not fit"), "{err}");
        assert_eq!(read("u64", "9223372036854775807"), Ok(long(i64::MAX)));
        assert!(read("i32", "banana").unwrap_err().contains("not a whole number"));
    }

    #[test]
    fn bools_floats_and_chars() {
        assert_eq!(read("bool", "true"), Ok(Some(Val::Prim(Prim::Bool(true)))));
        assert!(read("bool", "maybe").is_err());
        assert_eq!(read("f64", "2.5"), Ok(Some(Val::Prim(Prim::Double(2.5)))));
        assert_eq!(read("f32", "1"), Ok(Some(Val::Prim(Prim::Double(1.0)))));
        assert_eq!(read("char", "'a'"), Ok(Some(Val::Prim(Prim::Char(97)))));
        assert_eq!(read("char", "'\\n'"), Ok(Some(Val::Prim(Prim::Char(10)))));
        assert_eq!(read("char", "97 'a'"), Ok(Some(Val::Prim(Prim::Char(97)))), "lldb-dap prints the code first");
    }

    #[test]
    fn a_string_is_read_between_its_quotes() {
        let hi = Ok(Some(Val::Str("hi".to_string())));
        assert_eq!(read("&str", "\"hi\""), hi);
        assert_eq!(read("alloc::string::String", "\"hi\""), hi);
        assert_eq!(read("&str", "0x00007ffe1234 \"hi\""), hi, "an address in front is dropped");
        assert_eq!(read("&str", "\"a\\tb\""), Ok(Some(Val::Str("a\tb".to_string()))));
        // A string type whose rendering has no quotes is left as a reference to expand.
        assert_eq!(read("&str", "{…}"), Ok(None));
    }

    #[test]
    fn a_null_raw_pointer_is_null_and_any_other_is_a_reference() {
        assert_eq!(read("*const u8", "0x0000000000000000"), Ok(Some(Val::Null)));
        assert_eq!(read("*mut Node", "0x0"), Ok(Some(Val::Null)));
        assert_eq!(read("*const u8", "0x00007ffe1234"), Ok(None));
    }

    #[test]
    fn a_struct_or_a_collection_is_not_a_scalar() {
        assert_eq!(read("alloc::vec::Vec<i32>", "size=3"), Ok(None));
        assert_eq!(read("geode::Order", "{…}"), Ok(None));
        assert_eq!(read("", "whatever"), Ok(None), "no type, no guess");
    }
}
