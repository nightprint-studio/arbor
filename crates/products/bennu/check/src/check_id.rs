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
    /// A `case` label whose literal type can never match the selector's (`case 1` on an enum).
    IncompatibleCaseLabel,
    /// A `case` label naming something that isn't a constant of the selector's enum.
    UnknownEnumCaseLabel,

    // ── data flow ───────────────────────────────────────────────────────────────
    // The first checks that follow a VALUE rather than reading a declaration. See `crate::dataflow`
    // for the model and for why it is deliberately a small one.
    /// A member reached on a local that is definitely `null` at that point.
    NullDereference,
    /// A condition whose answer is already known — a null check on something definitely non-null.
    ConstantCondition,
    /// A value assigned to a local and overwritten before anything reads it.
    DeadStore,

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
    /// An override that makes the method less visible than the one it overrides.
    WeakerAccessOverride,
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
    /// A `static` method called through an instance rather than through its type. *Warning.*
    StaticViaInstance,

    // ── imports ──────────────────────────────────────────────────────────────────
    /// A single-type `import a.b.C;` the resolver can't resolve.
    UnresolvedImport,
    /// A wildcard import that's already implicitly in scope (`java.lang.*` / own package). *Warning.*
    RedundantImport,
    /// An `import` line repeating one declared above it. *Warning.*
    DuplicateImport,
    /// A single-type import whose name is never used in the file. *Warning.*
    UnusedImport,

    // ── instanceof / new ─────────────────────────────────────────────────────────
    /// An `instanceof` between inconvertible concrete types.
    IncompatibleInstanceof,
    /// A `new` on an abstract class or interface.
    InstantiateAbstract,
    /// A capturing local class instantiated from a `static` member of itself.
    LocalClassFromStatic,

    // ── try / catch ──────────────────────────────────────────────────────────────
    /// A `catch` whose type is already handled by an earlier clause (unreachable).
    UnreachableCatch,
    /// A `catch` for a checked exception the `try` body cannot throw.
    UnthrownCatch,
    /// A multi-`catch` listing a type together with its supertype.
    RedundantMultiCatch,
    /// A try-with-resources whose resource type definitely isn't `AutoCloseable`.
    NonAutoCloseableResource,

    // ── pure-AST: declarations & modifiers ────────────────────────────────────────
    /// An illegal modifier combination or declaration placement (abstract body, default outside an
    /// interface, record instance field, enum-constant/ctor mismatch, `final`+`volatile`, …).
    IllegalDeclaration,
    /// An annotation applied to a target it isn't `@Target`-ed for.
    AnnotationNotApplicable,
    /// A concrete method with no body (nor `abstract`), or a private interface method without one.
    MissingMethodBody,
    /// A `public` type whose name doesn't match its file name.
    TypeNameMismatchFile,
    /// A `package` declaration that doesn't match the file's on-disk location.
    PackageMismatch,
    /// A `package-info.java` / `module-info.java` containing something it may not.
    SpecialFileContent,
    /// A language feature used below the project's target Java version.
    FeatureRequiresNewerJava,

    // ── pure-AST: duplicates & redeclaration ─────────────────────────────────────
    /// Two methods/constructors with the same erased signature in one type.
    DuplicateMethod,
    /// Two methods whose signatures collide only after generic erasure.
    ErasureClash,
    /// A duplicated local/parameter/type/field declaration in one scope.
    DuplicateDeclaration,
    /// The same interface listed twice in an `implements`/`extends` clause.
    DuplicateInterface,
    /// Two `import`s binding the same simple name to different types.
    ImportCollision,

    // ── pure-AST: constructors & records ─────────────────────────────────────────
    /// A constructor that delegates to itself (directly or via a cycle).
    RecursiveConstructor,
    /// Instance state read in the arguments of a `this(…)` / `super(…)`, before the object exists.
    ReferenceBeforeConstructor,
    /// A record component left unassigned by its canonical constructor.
    RecordConstructor,
    /// A concrete method named exactly like its class (a likely missing-`void` typo). *Warning.*
    MethodNamedLikeConstructor,

    // ── pure-AST: finals & definite assignment ───────────────────────────────────
    /// An assignment to a `final` variable or field.
    FinalAssignment,
    /// A blank `final` never assigned, or a definite-assignment violation.
    DefiniteAssignment,
    /// A local captured by a lambda/inner class and then reassigned (not effectively final).
    CapturedVariableNotFinal,

    // ── pure-AST: functional & generics ──────────────────────────────────────────
    /// A `@FunctionalInterface` that doesn't declare exactly one abstract method.
    NotAFunctionalInterface,
    /// An illegal use of generics (generic array creation, `new T()`, generic `instanceof`/`catch`).
    IllegalGenericUsage,

    // ── pure-AST: control flow ───────────────────────────────────────────────────
    /// A statement that can never be reached.
    UnreachableStatement,
    /// A statement position holding an expression that isn't a statement.
    NotAStatement,
    /// A non-`void` method / branch that can fall off the end without returning.
    MissingReturn,
    /// A `return value;` in a `void` method or constructor.
    ReturnValueFromVoid,
    /// A `case` falling through into the next with code in between. *Warning.*
    SwitchFallthrough,
    /// A `finally` block that completes abruptly (swallows the try's outcome). *Warning.*
    FinallyAbrupt,

    // ── pure-AST: switch ─────────────────────────────────────────────────────────
    /// A `switch` on a selector type the language doesn't permit.
    IllegalSwitchSelector,
    /// A `switch` expression whose arms don't all yield a value.
    SwitchExpressionIncomplete,
    /// Two `case` labels with the same constant.
    DuplicateCaseLabel,

    // ── pure-AST: expressions (lints) ────────────────────────────────────────────
    /// `x = x` — a self-assignment with no effect. *Warning.*
    SelfAssignment,
    /// A constant division (or modulo) by zero. *Warning.*
    DivisionByZero,
    /// A stray `;` empty statement. *Warning.*
    EmptyStatement,
    /// A `String` compared with `==`/`!=` (reference identity, not contents). *Warning.*
    StringReferenceEquality,
    /// A local-variable `var` whose initializer gives no inferable type.
    VarTypeInferenceFailed,

    // ── branches & labels ────────────────────────────────────────────────────────
    /// A `break` outside any loop or `switch`, or a `continue` outside any loop.
    BranchOutsideLoop,
    /// A `break`/`continue` naming a label that no enclosing statement declares.
    UnknownLabel,

    /// An `if` whose body is a bare `;` — the guarded statement is outside the `if`, not in it.
    EmptyIfBody,

    // ── initializers ─────────────────────────────────────────────────────────────
    /// A field initializer that reads the field it is initializing.
    SelfReferencingInitializer,

    // ── annotations ──────────────────────────────────────────────────────────────
    /// The same annotation element given a value twice (`@Foo(a = 1, a = 2)`).
    DuplicateAnnotationValue,
    /// A value given for an element the annotation type does not declare (`@Column(nulable = true)`).
    UnknownAnnotationElement,
    /// An annotation used without giving a value for an element that has no `default` (`@Column`
    /// where `name()` is required).
    MissingAnnotationElement,
    /// An `@interface` element declared with a type an annotation element cannot have
    /// (`MyObj[] o();`).
    InvalidAnnotationElementType,
    /// The same annotation written twice on one declaration, without being `@Repeatable`.
    NotRepeatableAnnotation,
    /// An annotation element given a value that is not a constant expression.
    NonConstantAnnotationValue,
    /// An annotation element given a value whose type cannot be the declared one.
    AnnotationValueType,

    // ── pure-AST: syntax ─────────────────────────────────────────────────────────
    /// A tree-sitter `ERROR` node — a genuine syntax error.
    SyntaxError,
    /// A tree-sitter `MISSING` node — a token the grammar expected but didn't find.
    MissingToken,
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
            IncompatibleCaseLabel => "incompatible-case-label",
            UnknownEnumCaseLabel => "unknown-enum-case-label",
            NullDereference => "null-dereference",
            ConstantCondition => "constant-condition",
            DeadStore => "dead-store",
            IllegalInheritance => "illegal-inheritance",
            MissingAbstractMethod => "missing-abstract-method",
            CyclicInheritance => "cyclic-inheritance",
            OverrideOverridesNothing => "override-overrides-nothing",
            FinalMethodOverride => "final-method-override",
            WeakerAccessOverride => "weaker-access-override",
            CovariantReturn => "covariant-return",
            CheckedExceptionWidening => "checked-exception-widening",
            SuperConstructorRequired => "super-constructor-required",
            LambdaArity => "lambda-arity",
            InaccessibleMember => "inaccessible-member",
            StaticContextAccess => "static-context-access",
            StaticViaInstance => "static-via-instance",
            UnresolvedImport => "unresolved-import",
            RedundantImport => "redundant-import",
            DuplicateImport => "duplicate-import",
            UnusedImport => "unused-import",
            IncompatibleInstanceof => "incompatible-instanceof",
            InstantiateAbstract => "instantiate-abstract",
            LocalClassFromStatic => "local-class-from-static",
            UnreachableCatch => "unreachable-catch",
            UnthrownCatch => "unthrown-catch",
            RedundantMultiCatch => "redundant-multi-catch",
            NonAutoCloseableResource => "non-autocloseable-resource",
            IllegalDeclaration => "illegal-declaration",
            AnnotationNotApplicable => "annotation-not-applicable",
            MissingMethodBody => "missing-method-body",
            TypeNameMismatchFile => "type-name-mismatch-file",
            PackageMismatch => "package-mismatch",
            SpecialFileContent => "special-file-content",
            FeatureRequiresNewerJava => "feature-requires-newer-java",
            DuplicateMethod => "duplicate-method",
            ErasureClash => "erasure-clash",
            DuplicateDeclaration => "duplicate-declaration",
            DuplicateInterface => "duplicate-interface",
            ImportCollision => "import-collision",
            RecursiveConstructor => "recursive-constructor",
            ReferenceBeforeConstructor => "reference-before-constructor",
            RecordConstructor => "record-constructor",
            MethodNamedLikeConstructor => "method-named-like-constructor",
            FinalAssignment => "final-assignment",
            DefiniteAssignment => "definite-assignment",
            CapturedVariableNotFinal => "captured-variable-not-final",
            NotAFunctionalInterface => "not-a-functional-interface",
            IllegalGenericUsage => "illegal-generic-usage",
            UnreachableStatement => "unreachable-statement",
            NotAStatement => "not-a-statement",
            MissingReturn => "missing-return",
            ReturnValueFromVoid => "return-value-from-void",
            SwitchFallthrough => "switch-fallthrough",
            FinallyAbrupt => "finally-abrupt-completion",
            IllegalSwitchSelector => "illegal-switch-selector",
            SwitchExpressionIncomplete => "switch-expression-incomplete",
            DuplicateCaseLabel => "duplicate-case-label",
            SelfAssignment => "self-assignment",
            DivisionByZero => "division-by-zero",
            EmptyStatement => "empty-statement",
            StringReferenceEquality => "string-reference-equality",
            VarTypeInferenceFailed => "var-type-inference-failed",
            BranchOutsideLoop => "branch-outside-loop",
            UnknownLabel => "unknown-label",
            EmptyIfBody => "empty-if-body",
            SelfReferencingInitializer => "self-referencing-initializer",
            DuplicateAnnotationValue => "duplicate-annotation-value",
            UnknownAnnotationElement => "unknown-annotation-element",
            MissingAnnotationElement => "missing-annotation-element",
            InvalidAnnotationElementType => "invalid-annotation-element-type",
            NotRepeatableAnnotation => "not-repeatable-annotation",
            NonConstantAnnotationValue => "non-constant-annotation-value",
            AnnotationValueType => "annotation-value-type",
            SyntaxError => "syntax-error",
            MissingToken => "missing-token",
        }
    }

    /// The default severity string (`"error"` / `"warning"`) for this kind.
    pub const fn severity(self) -> &'static str {
        use CheckId::*;
        match self {
            // Style / hygiene lints — not compile errors.
            RedundantImport | DuplicateImport | UnusedImport | MethodNamedLikeConstructor
            | SwitchFallthrough | FinallyAbrupt | SelfAssignment | DivisionByZero | EmptyStatement
            | StringReferenceEquality | ConstantCondition | DeadStore | EmptyIfBody | StaticViaInstance => "warning",
            // Everything else is a compile-level error.
            _ => "error",
        }
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

    /// Every check kind, for a settings screen that lists them.
    ///
    /// Hand-maintained beside the `code()` match rather than derived: there is no reflection over a
    /// Rust enum, and a catalog that silently missed a kind would be a check nobody could ever
    /// configure. A new variant that is added to `code()` and not to this is the one mistake to
    /// watch for — which is what `every_kind_is_in_the_catalog` is for.
    pub const ALL: &'static [CheckId] = {
        use CheckId::*;
        &[
            UnknownMember,
            UnknownField,
            WrongArgumentCount,
            ArgumentType,
            UnresolvedSuperMethod,
            UnresolvedType,
            UnresolvedSymbol,
            WrongTypeArgumentCount,
            IncompatibleType,
            LossyConversion,
            NonBooleanCondition,
            UnhandledCheckedException,
            NonExhaustiveEnumSwitch,
            IncompatibleCaseLabel,
            UnknownEnumCaseLabel,
            NullDereference,
            ConstantCondition,
            DeadStore,
            IllegalInheritance,
            MissingAbstractMethod,
            CyclicInheritance,
            OverrideOverridesNothing,
            FinalMethodOverride,
            WeakerAccessOverride,
            CovariantReturn,
            CheckedExceptionWidening,
            SuperConstructorRequired,
            LambdaArity,
            InaccessibleMember,
            StaticContextAccess,
            StaticViaInstance,
            UnresolvedImport,
            RedundantImport,
            DuplicateImport,
            UnusedImport,
            IncompatibleInstanceof,
            InstantiateAbstract,
            LocalClassFromStatic,
            UnreachableCatch,
            UnthrownCatch,
            RedundantMultiCatch,
            NonAutoCloseableResource,
            IllegalDeclaration,
            AnnotationNotApplicable,
            MissingMethodBody,
            TypeNameMismatchFile,
            PackageMismatch,
            SpecialFileContent,
            FeatureRequiresNewerJava,
            DuplicateMethod,
            ErasureClash,
            DuplicateDeclaration,
            DuplicateInterface,
            ImportCollision,
            RecursiveConstructor,
            ReferenceBeforeConstructor,
            RecordConstructor,
            MethodNamedLikeConstructor,
            FinalAssignment,
            DefiniteAssignment,
            CapturedVariableNotFinal,
            NotAFunctionalInterface,
            IllegalGenericUsage,
            UnreachableStatement,
            NotAStatement,
            MissingReturn,
            ReturnValueFromVoid,
            SwitchFallthrough,
            FinallyAbrupt,
            IllegalSwitchSelector,
            SwitchExpressionIncomplete,
            DuplicateCaseLabel,
            SelfAssignment,
            DivisionByZero,
            EmptyStatement,
            StringReferenceEquality,
            VarTypeInferenceFailed,
            BranchOutsideLoop,
            UnknownLabel,
            EmptyIfBody,
            SelfReferencingInitializer,
            DuplicateAnnotationValue,
            UnknownAnnotationElement,
            MissingAnnotationElement,
            InvalidAnnotationElementType,
            NotRepeatableAnnotation,
            NonConstantAnnotationValue,
            AnnotationValueType,
            SyntaxError,
            MissingToken,
        ]
    };
}
