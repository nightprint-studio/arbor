//! `bennu-refactor` — Java refactorings as pure source transforms.
//!
//! ## The shape, and why it is this one
//!
//! Every refactoring here is `(source, selection) -> Plan | Refusal | nothing`. No filesystem, no
//! project index, no Tauri — so each one is exhaustively unit-testable in this crate, which is the
//! only place they *can* be tested: there is no test runner in front of the editor, and a
//! refactoring that is wrong once is a refactoring nobody uses again.
//!
//! | | |
//! |---|---|
//! | [`extract_method`] | a run of statements becomes a method and a call — with its parameters, its one return value, and a refusal by name when it cannot have either |
//! | [`extract_var`] | an expression gets a name: a local, or a `private static final` when it is constant to read |
//! | [`inline_var`] | a local goes back into its uses, parenthesised where the context binds tighter, refused where the value would move |
//! | [`inline_method`] | a one-expression method goes back into its call, with its parameters substituted structurally |
//! | [`field`] | a local becomes a field, with its initialisation left where it ran |
//! | [`switch`] | an `if` chain on one value becomes a `switch`, with the `break`s javac will accept |
//! | [`move_member`] | a member changes type — pulled up, pushed down, or moved across |
//! | [`move_class`] | a nested type gets its own file, beside the one it left |
//!
//! ## Planning is the safety story
//!
//! Nothing here writes. A refactoring reads the parse, decides, and returns byte-range edits; the
//! editor applies them through its own buffer, so a refactoring is undone like any other edit. And
//! when it cannot be done safely it says **why**, in the words of the code in front of the user —
//! see [`plan::Refusal`], and the rules each module documents at its head.
//!
//! ## What is not here
//!
//! Renaming — which is a *project* question (every reference, every Spring bean, every XML
//! config) and lives in `bennu-intel` with the reference index behind it. And anything that has to
//! resolve a type: this crate names the span it needs typed ([`plan::TypeSlot`]) and the caller,
//! which has the classpath, fills it in.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_refactor::prelude::...`.

// Where a member lives, and where a new one goes.
pub mod body;
// A run of statements becomes a method.
pub mod create;
pub mod extract_method;
// An expression gets a name.
pub mod extract_var;
// A one-expression method goes back into its call.
pub mod declaration;
// A local becomes a field.
pub mod field;
pub mod if_statement;
pub mod inline_method;
// A local goes back into its uses.
pub mod inline_var;
// A nested type gets its own file.
pub mod move_class;
// A member changes the type it belongs to.
pub mod move_member;
// The one call the editor makes.
pub mod offers;
// What a refactoring produces, and what it says when it will not.
pub mod plan;
pub mod prelude;
// Finding the thing a refactoring is about.
pub mod selection;
// An `if` chain becomes a `switch`.
pub mod switch;
