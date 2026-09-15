//! Shared readings of the tree and the resolver the checks build on. No diagnostics of their own.

pub(crate) mod assignable;
pub mod bare_call;
pub mod constant;
pub(crate) mod constructors;
/// The tree-sitter adaptation of `bennu-lombok` - internal, because what it exposes is a CST detail.
/// The knowledge itself is the dependency-free crate every consumer shares.
pub(crate) mod lombok;
pub mod method_sig;
pub mod nodes;
pub mod resolve;
pub mod scopes;
pub mod supertypes;
pub mod switch_label;
pub mod text;
pub mod throws_of;
pub mod type_scope;
pub mod walk;
