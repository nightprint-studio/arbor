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

    // ── inheritance & overrides ──────────────────────────────────────────────────
    /// An illegal `extends`/`implements` — extending a `final`/record/enum/interface, or
    /// implementing a non-interface.
    IllegalInheritance,
    /// A concrete class that leaves an inherited abstract method unimplemented.
    MissingAbstractMethod,
    /// A type that transitively extends / implements itself.
    CyclicInheritance,
    /// An `@Override` method that overrides nothing in its (fully-known) supertype hierarchy.
    OverrideOverridesNothing,
    /// A method that overrides a `final` supertype method.
    FinalMethodOverride,
    /// An override whose return type isn't covariant with the overridden method's.
    CovariantReturn,
    /// An override that declares a checked exception the overridden method doesn't.
    CheckedExceptionWidening,
    /// A subclass constructor that must chain `super(...)` because the superclass has no no-arg ctor.
    SuperConstructorRequired,

    // ── lambdas / functional ─────────────────────────────────────────────────────
    /// A lambda whose parameter count doesn't match its target functional interface's SAM.
    LambdaArity,

    // ── access ───────────────────────────────────────────────────────────────────
    /// A `private` / package-private member reached from where it isn't visible.
    InaccessibleMember,
    /// A non-static member referenced from a `static` context.
    StaticContextAccess,

    // ── imports ──────────────────────────────────────────────────────────────────
    /// A single-type `import a.b.C;` the resolver can't resolve.
    UnresolvedImport,

    // ── instanceof / new ─────────────────────────────────────────────────────────
    /// An `instanceof` between inconvertible concrete types.
    IncompatibleInstanceof,
    /// A `new` on an abstract class or interface.
    InstantiateAbstract,

    // ── try / catch ──────────────────────────────────────────────────────────────
    /// A `catch` whose type is already handled by an earlier clause (unreachable).
    UnreachableCatch,
    /// A multi-`catch` listing a type together with its supertype.
    RedundantMultiCatch,
    /// A try-with-resources whose resource type definitely isn't `AutoCloseable`.
    NonAutoCloseableResource,
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
            IllegalInheritance => "illegal-inheritance",
            MissingAbstractMethod => "missing-abstract-method",
            CyclicInheritance => "cyclic-inheritance",
            OverrideOverridesNothing => "override-overrides-nothing",
            FinalMethodOverride => "final-method-override",
            CovariantReturn => "covariant-return",
            CheckedExceptionWidening => "checked-exception-widening",
            SuperConstructorRequired => "super-constructor-required",
            LambdaArity => "lambda-arity",
            InaccessibleMember => "inaccessible-member",
            StaticContextAccess => "static-context-access",
            UnresolvedImport => "unresolved-import",
            IncompatibleInstanceof => "incompatible-instanceof",
            InstantiateAbstract => "instantiate-abstract",
            UnreachableCatch => "unreachable-catch",
            RedundantMultiCatch => "redundant-multi-catch",
            NonAutoCloseableResource => "non-autocloseable-resource",
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
