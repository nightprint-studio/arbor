//! `bennu-debug-expr` — the expression language Bennu's two debuggers share.
//!
//! Watches and breakpoint conditions used to be *paths* (`order.items[2].price`) compared with
//! literals, and nothing else. That is still the heart of it, and the principle that kept it small
//! is kept whole: anything the evaluator does not do is **refused by name** — a method call, a cast,
//! a lambda, an assignment, a bitwise operator — rather than approximated. A condition that quietly
//! answers something adjacent to what you typed swallows the stop, with nothing on screen.
//!
//! What changed is that the *useful* things are no longer refused: arithmetic (`i + 1 == n`,
//! `total % 100 == 0`), the ternary, string concatenation in Java, enum constants and other static
//! fields (`status == Status.ACTIVE`), array subscripts that are themselves expressions
//! (`items[i].price > 0`), and `.length` on an array.
//!
//! ## Level 1: no calls, but room for them
//!
//! A call is **parsed** (`Expr::Call`) and refused when it is evaluated, with the reason a person
//! needs: it would run application code inside a paused program. [`Syntax::calls`] and
//! [`DebugTarget::call`] are the seam where invoking methods would plug in — turning them on is a
//! target implementing `call`, not a new grammar.
//!
//! ## The pieces
//!
//! * [`syntax`] — what a language allows ([`JAVA`], [`RUST`]) and how its numbers behave.
//! * [`lexer`] / [`parser`] → [`ast::Expr`] — a precedence parser (ternary < `||` < `&&` <
//!   equality < relational < `+ -` < `* / %` < unary < postfix).
//! * [`eval`] — the evaluator, generic over a [`DebugTarget`]: the target reads variables, fields,
//!   elements and statics out of a stopped program; the evaluator owns every rule about what to do
//!   with them (null, overflow, promotion, short-circuit).
//! * [`ops`] — the per-language numeric semantics, pure and unit-tested on their own.
//! * [`condition`] — a condition is an expression that must read the program and be boolean.
//! * [`path`] — the older watch-path grammar (`a.b[2].c`, `*p`), still what mutation, logpoints and
//!   the assistant's read-only tools accept.
//! * [`java_names`] — turning `Status.ACTIVE` into the class names a JVM could know it by.
//!
//! ## Numbers
//!
//! **Java**: `int` and `long` wrap around on overflow, integer division truncates, `char` promotes
//! to `int`, and `+` joins strings. **Rust**: every integer is evaluated as a checked `i64` — an
//! overflow is an error, never a wrap; integers and floats are never mixed; `char` compares only
//! with `char`; there is no string `+`. Division by zero is an error in both. Rust's narrower types
//! (`u8`, `i32`) are read as `i64`, so their own overflow is not modelled.

pub mod ast;
pub mod condition;
pub mod eval;
pub mod java_names;
pub mod lexer;
pub mod ops;
pub mod parser;
pub mod path;
pub mod prelude;
pub mod syntax;
pub mod target;
pub mod value;
