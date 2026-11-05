//! The values an evaluation works with, independent of which debugger supplied them.

/// A primitive.
///
/// Java reads `byte` / `short` / `int` as [`Prim::Int`], `long` as [`Prim::Long`], and `float` /
/// `double` as [`Prim::Double`]. Rust reads every integer as [`Prim::Long`] — see the crate docs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Prim {
    Bool(bool),
    /// A UTF-16 code unit in Java, a Unicode scalar in Rust.
    Char(u32),
    Int(i32),
    Long(i64),
    Double(f64),
}

/// A value: what a target hands the evaluator, and what an evaluation produces.
///
/// `R` is the target's handle on something it keeps — an object id for a JVM, a variable (with its
/// reference) for a DAP adapter. The evaluator never looks inside it; it only compares two for
/// identity and gives them back to the target to read from.
#[derive(Debug, Clone, PartialEq)]
pub enum Val<R> {
    Prim(Prim),
    /// A string that is already text: a literal, a concatenation, or one the target resolved.
    Str(String),
    Null,
    Ref(R),
}

impl Prim {
    /// What it is, for an error a person reads.
    pub fn describe(&self) -> String {
        match self {
            Prim::Bool(v) => format!("`{v}`"),
            Prim::Char(c) => match char::from_u32(*c) {
                Some(ch) => format!("the character {ch:?}"),
                None => format!("the character \\u{c:04x}"),
            },
            Prim::Int(v) => format!("the number {v}"),
            Prim::Long(v) => format!("the number {v}"),
            Prim::Double(v) => format!("the number {v}"),
        }
    }
}

impl<R> Val<R> {
    /// What it is, for an error a person reads.
    pub fn describe(&self) -> String {
        match self {
            Val::Prim(p) => p.describe(),
            Val::Str(s) => format!("the string {s:?}"),
            Val::Null => "null".to_string(),
            Val::Ref(_) => "an object".to_string(),
        }
    }
}
