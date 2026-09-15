//! Postfix templates — write the value first, then say what to do with it.
//!
//! `orders.for` becomes `for (Order order : orders) { … }`. The names are IntelliJ's, because muscle
//! memory is the whole value of a postfix template, and so is the behaviour: what a template writes is
//! decided by the **type** of the expression before the dot, not by its text. `.stream` on an array is
//! `Arrays.stream(ids)`; on an `Iterable` that is not a collection it goes through `StreamSupport`; on
//! a `List` it is not offered at all, because `stream()` is already the member right above it.
//!
//! ## The language level is part of the answer
//!
//! A template only ever writes code the file's module compiles, and "not offered" is the last resort
//! rather than the first:
//!
//! * where a newer form exists and the module has it, the newer form is written (`var`, a pattern in
//!   `instanceof`, `ifPresentOrElse`);
//! * where the older language has an equivalent, **that** is written instead — `.ifpe` on Java 8 is an
//!   `if`/`else` over `isPresent()`, `.stream` on an `Optional` is `map(Stream::of)`;
//! * only where it has none is the template left out.
//!
//! Writing `var` on a Java 8 module produces code that reads right and does not build, which is worse
//! than offering nothing.
//!
//! ## Written once
//!
//! An expression that does work — a call, a `new` — is never written twice. A template whose older
//! form needs the value in two places declares a local for it first, or is not offered when there is
//! nowhere to put one (`.fori` would call it on every iteration).
//!
//! ## What is here and what is the caller's
//!
//! This module is pure: a [`Subject`] (the expression and what its type makes possible) and a
//! [`PostfixContext`] in, [`Expansion`]s out — text, tab stops as byte ranges into it, and the imports
//! the text needs. Inferring the type, filtering by what was typed and turning an expansion into an
//! edit belong to the completion provider.
//!
//! The expression is found by scanning ([`subject_start`]) rather than by parsing: the buffer is
//! mid-edit by definition here — the template name is not valid Java yet — so a parse of it is a parse
//! of something broken.

mod body;
mod catalogue;
mod loops;
// Crate-visible: a declaration's predicted name (`crate::names`) is built from the same word rules,
// so `.var` on a list and a field typed as one propose the same name.
pub(crate) mod names;
mod optional;
mod scan;
mod shape;
#[cfg(test)]
mod tests;

pub use body::indent_unit;
pub use catalogue::expansions;
pub use scan::subject_start;
pub use shape::{shape_of, Element, Length, OptionalShape, ValueShape};

/// A tab stop: a byte range into [`Expansion::text`].
///
/// Stops are listed in the order Tab visits them. Stops sharing a non-zero `group` are one value
/// written in several places — typing in one types in all of them — and `0` is a stop that stands
/// alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stop {
    pub start: usize,
    pub end: usize,
    pub group: u32,
}

/// What accepting a template writes over the expression it was typed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    /// What is typed after the dot — `for`, `nn`, `ifpe`.
    pub name: &'static str,
    /// The popup's right-hand column: what this template writes for THIS expression, at this level.
    pub detail: String,
    /// The replacement for the expression, at column 0 — the caller indents every line after the
    /// first to the line it lands on.
    pub text: String,
    pub stops: Vec<Stop>,
    /// The fully-qualified classes the text names, sorted. Some may already be imported, or need no
    /// import at all; deciding that is the caller's, which has the file.
    pub imports: Vec<String>,
}

/// What an expansion depends on beyond the expression.
#[derive(Debug, Clone, Copy)]
pub struct PostfixContext<'a> {
    /// The Java language level of the module the file belongs to (`8`, `17`).
    pub level: u32,
    /// One indentation step, as the file writes it — see [`indent_unit`].
    pub unit: &'a str,
}

/// A type, as a declaration writes it, and the classes that spelling needs imported.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Written {
    /// `List<Order>`, `Map.Entry<String, Integer>`, `int[]`.
    pub text: String,
    /// Fully-qualified, sorted: `java.util.List`, `com.acme.Order`.
    pub imports: Vec<String>,
}

/// The expression before the dot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// A class name, which only `.new` applies to: every other template wraps a value.
    Type { text: String },
    /// A value, and what its type makes possible.
    Value { text: String, shape: ValueShape },
}
