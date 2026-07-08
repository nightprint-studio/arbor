//! `CheckId` — the typed catalog of validation KINDS, the way IntelliJ's `JavaErrorKinds` names every
//! error it can raise. Each variant maps to a stable kebab-case `code()` (which rides on the wire
//! [`Diagnostic::code`]) and a default `severity()`. Centralising the kinds here means:
//!
//!   * the FE / settings can group, suppress or re-severity a rule by its `code`;
//!   * a future quick-fix registry can key off the kind instead of matching message text;
//!   * message wording and severity for a kind live in ONE place, not scattered across 50-odd modules.
//!
//! Construction goes through [`CheckId::at`] / [`CheckId::span`], so the emitting check writes
//! `CheckId::UnknownMember.at(node, msg)` and the `code` + `severity` are filled from the catalog — a
//! check can never disagree with itself on either. Diagnostics not yet migrated to a `CheckId` carry an
//! empty `code` (still valid); they're being moved over incrementally.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// The kind of a diagnostic. Every variant has a stable [`code`](CheckId::code) and a default
/// [`severity`](CheckId::severity). Add a variant here when a check gains a new distinct kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckId {
    // ── members & calls ─────────────────────────────────────────────────────────
    /// A method that doesn't exist on the receiver's inferred type.
    UnknownMember,
    /// A field that doesn't exist on the receiver's inferred type.
    UnknownField,
    /// A call / `new` whose argument count matches no overload.
    WrongArgumentCount,
    /// An argument whose type can't bind to the parameter.
    ArgumentType,
    /// A `super.method()` whose name exists nowhere in the super-hierarchy.
    UnresolvedSuperMethod,

    // ── types ───────────────────────────────────────────────────────────────────
    /// A written type name (in a type position) the resolver can't resolve.
    UnresolvedType,
    /// A bare value identifier that resolves to nothing (javac's "cannot find symbol: variable").
    UnresolvedSymbol,
    /// A generic type given the wrong number of type arguments (`List<A, B>`).
    WrongTypeArgumentCount,
    /// An inconvertible cast, or an assignment / return of an incompatible type.
    IncompatibleType,
    /// A lossy primitive narrowing without a cast (`int x = aLong`).
    LossyConversion,
    /// A control-flow condition (`if`/`while`/`?:`) whose type definitely isn't `boolean`.
    NonBooleanCondition,

    // ── exceptions ──────────────────────────────────────────────────────────────
    /// A checked exception thrown / called that isn't caught or declared.
    UnhandledCheckedException,

    // ── enum ────────────────────────────────────────────────────────────────────
    /// A `switch` EXPRESSION over an enum that leaves some constant uncovered and has no `default`.
    NonExhaustiveEnumSwitch,
}

impl CheckId {
    /// The stable machine slug for this kind — rides on [`Diagnostic::code`].
    pub const fn code(self) -> &'static str {
        use CheckId::*;
        match self {
            UnknownMember => "unknown-member",
            UnknownField => "unknown-field",
            WrongArgumentCount => "wrong-argument-count",
            ArgumentType => "argument-type",
            UnresolvedSuperMethod => "unresolved-super-method",
            UnresolvedType => "unresolved-type",
            UnresolvedSymbol => "unresolved-symbol",
            WrongTypeArgumentCount => "wrong-type-argument-count",
            IncompatibleType => "incompatible-type",
            LossyConversion => "lossy-conversion",
            NonBooleanCondition => "non-boolean-condition",
            UnhandledCheckedException => "unhandled-checked-exception",
            NonExhaustiveEnumSwitch => "non-exhaustive-enum-switch",
        }
    }

    /// The default severity string (`"error"` / `"warning"`) for this kind.
    pub const fn severity(self) -> &'static str {
        // Every kind currently catalogued is a compile-level error; warnings join as they're migrated.
        "error"
    }

    /// Build a [`Diagnostic`] of this kind spanning `[start, end)`.
    pub fn span(self, start: usize, end: usize, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            message: message.into(),
            severity: self.severity().to_string(),
            code: self.code().to_string(),
            start,
            end,
        }
    }

    /// Build a [`Diagnostic`] of this kind spanning `node`.
    pub fn at(self, node: Node, message: impl Into<String>) -> Diagnostic {
        self.span(node.start_byte(), node.end_byte(), message)
    }
}
