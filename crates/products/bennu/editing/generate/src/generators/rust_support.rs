//! What the Rust generators share: a typed name/field pair, struct writing, identifier checks and the
//! output shape (text at the caret, plus a `Cargo.toml` snippet).

use serde::{Deserialize, Serialize};

use crate::code::Lines;
use crate::names::is_identifier;
use crate::output::{MemberTarget, Members, Output};

/// A named, typed thing: a struct field, a query parameter, an action's key.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Field {
    pub name: String,
    pub ty: String,
}

impl Field {
    pub fn new(name: &str, ty: &str) -> Self {
        Field { name: name.to_string(), ty: ty.to_string() }
    }
}

const KEYWORDS: [&str; 20] = [
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
];

/// A Rust identifier that is not a keyword.
pub fn require_ident(name: &str, what: &str) -> Result<(), String> {
    if is_identifier(name) && !KEYWORDS.contains(&name) && !["ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while", "false", "dyn", "async", "await"].contains(&name) {
        Ok(())
    } else {
        Err(format!("{what} `{name}` is not a valid Rust name"))
    }
}

/// Text written at the caret of a Rust file.
pub fn at_caret(text: String) -> Output {
    Output { members: Some(Members { target: MemberTarget::Caret, text: text.trim_end().to_string(), imports: Vec::new() }), ..Output::default() }
}

/// `#[derive(..)] pub struct Name { pub a: T, }` — a unit struct when there are no fields.
pub fn write_struct(l: &mut Lines, name: &str, fields: &[Field], derives: &[&str], attrs: &[String]) {
    l.line(format!("#[derive({})]", derives.join(", ")));
    for a in attrs {
        l.line(a);
    }
    if fields.is_empty() {
        l.line(format!("pub struct {name};"));
        return;
    }
    l.block(format!("pub struct {name}"), |l| {
        for f in fields {
            l.line(format!("pub {}: {},", f.name, f.ty));
        }
    });
}

/// Reject fields with a bad name or no type.
pub fn check_fields(fields: &[Field], what: &str) -> Result<(), String> {
    for f in fields {
        require_ident(&f.name, what)?;
        if f.ty.trim().is_empty() {
            return Err(format!("{what} `{}` needs a type", f.name));
        }
    }
    Ok(())
}
