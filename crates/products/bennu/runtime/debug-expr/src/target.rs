//! What the evaluator needs from a debugger.

use crate::value::{Prim, Val};

/// A stopped program, as far as an expression is concerned.
///
/// Every method is a **read**. The evaluator decides what to do with what it reads — null, overflow,
/// promotion, short-circuit — so a target stays a thin layer over its protocol (JDWP, DAP) and two
/// targets cannot disagree about what `a + b` means.
///
/// Errors are plain strings, the product's contract: they reach the user as written.
pub trait DebugTarget {
    /// A handle on a value the target keeps. Compared for identity by `==` between two objects.
    type Ref: Clone + PartialEq + std::fmt::Debug;

    /// A name in scope: a local, an argument, or (where the language has one) a field of the
    /// receiver. `Ok(None)` means *no such name* — the evaluator may then try it as a type.
    fn variable(&self, name: &str) -> Result<Option<Val<Self::Ref>>, String>;

    /// A field of an object. `of` is the source text of the expression that produced `owner`, for
    /// a message that names what failed.
    fn field(&self, owner: &Self::Ref, name: &str, of: &str) -> Result<Val<Self::Ref>, String>;

    /// An element of an array or sequence.
    fn index(&self, owner: &Self::Ref, at: i64, of: &str) -> Result<Val<Self::Ref>, String>;

    /// What a reference points at. Rust only; a target that has no `*` never sees this called.
    fn deref(&self, pointer: &Self::Ref, of: &str) -> Result<Val<Self::Ref>, String>;

    /// A static field or an enum constant: `qualifier` names the type (`["Status"]`,
    /// `["com", "acme", "Status"]`), `name` the member. `Ok(None)` means the type is not known —
    /// not loaded yet, or not a type at all.
    fn static_field(
        &self,
        _qualifier: &[String],
        _name: &str,
    ) -> Result<Option<Val<Self::Ref>>, String> {
        Ok(None)
    }

    /// The text behind a reference, when it is a string. Lets `name == "admin"` compare by text,
    /// which is the only comparison anyone means by it.
    fn text(&self, _of: &Self::Ref) -> Result<Option<String>, String> {
        Ok(None)
    }

    /// The primitive inside a box (`Integer`, `Boolean`, …), when the reference is one. Lets
    /// `count > 3` work on an `Integer` the way it does in the source.
    fn unbox(&self, _of: &Self::Ref) -> Result<Option<Prim>, String> {
        Ok(None)
    }

    /// Invoke a function. **Never reached at level 1** — [`crate::syntax::Syntax::calls`] is off, so
    /// a call is refused before it gets here. This is the seam calls will use.
    fn call(
        &self,
        _receiver: Option<&Val<Self::Ref>>,
        name: &str,
        _args: &[Val<Self::Ref>],
    ) -> Result<Val<Self::Ref>, String> {
        Err(format!("`{name}(…)` cannot be called: this debugger does not invoke code"))
    }
}
