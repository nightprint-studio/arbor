//! The typed AST for a `.nemus` program.
//!
//! This is the **contract** every other layer targets: the Tree-sitter walker
//! produces it from the CST, the evaluator turns it into a
//! [`Pattern`](arbor_nemus_pattern::prelude::Pattern), and the emitter prints it
//! back to source (`design/nemus/editing-model.md` — keep the AST clean and
//! regular so emission is deterministic). It mirrors the grammar in
//! `design/nemus/grammar.md` one-to-one.
//!
//! **Spans everywhere.** Every node carries a [`SourceSpan`] (byte offsets into
//! the original source, reusing the pattern crate's type) so errors point at the
//! exact characters and each hap can be traced back for live highlight.

use arbor_nemus_pattern::prelude::SourceSpan;

/// An identifier with its source span.
#[derive(Clone, Debug, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: SourceSpan,
}

/// A whole `.nemus` source file: a sequence of top-level items.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}

/// A top-level statement.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// `import { a, b } from "path"` — bring `fn`/`let` names into scope.
    Import(Import),
    /// `let name = expr` — bind a value (does not sound).
    Let(LetBind),
    /// `fn name(params) = expr` — an expression-bodied, non-recursive function.
    Fn(FnDef),
    /// A bare top-level expression: the output (`tracks(...)` or a single pattern).
    Expr(Expr),
}

/// `import { names } from "path"`.
#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub names: Vec<Ident>,
    pub path: String,
    pub span: SourceSpan,
}

/// `let name = value`.
#[derive(Clone, Debug, PartialEq)]
pub struct LetBind {
    pub name: Ident,
    pub value: Expr,
    pub span: SourceSpan,
}

/// `fn name(params) = body`.
#[derive(Clone, Debug, PartialEq)]
pub struct FnDef {
    pub name: Ident,
    pub params: Vec<Ident>,
    pub body: Expr,
    pub span: SourceSpan,
}

/// A host-language expression.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: SourceSpan,
}

/// The shape of an [`Expr`].
#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    /// A numeric literal — always unsigned (the grammar's `NUMBER`); a leading
    /// `-` is a [`UnOp::Neg`], never part of the token.
    Number(f64),
    /// A string literal.
    Str(String),
    /// A host pitch literal: `c4`, `ef3`, `cs5`. Unlike a mini-notation note
    /// name, the octave is **mandatory** in the host (so `c`/`ef` stay plain
    /// identifiers and only `c4`/`ef3` lex as notes — see
    /// `design/nemus/grammar.md`). Evaluates to a single-note pattern, so it
    /// flows into combinators like `choose(c4, ef4, g4)`.
    Note(String),
    /// A bare identifier: a variable reference, or a nullary transform (`rev`).
    /// Which one it is is resolved at eval time, not by the parser.
    Var(String),
    /// `name(args)` — a combinator, a partially-applied transform (`gain(0.4)`),
    /// or a function call. Disambiguated by the builtin/binding it resolves to.
    Call { name: Ident, args: Vec<Expr> },
    /// `recv.name(args)` — a method/transform applied to a receiver (chaining).
    Method {
        recv: Box<Expr>,
        name: Ident,
        args: Vec<Expr>,
    },
    /// Unary operator (`-x`).
    Unary { op: UnOp, rhs: Box<Expr> },
    /// Binary arithmetic (`+ - * /`).
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `lo..hi` (exclusive) or `lo..=hi` (inclusive) — a Rust-style range.
    Range {
        lo: Box<Expr>,
        hi: Box<Expr>,
        inclusive: bool,
    },
    /// `param => body` or `(p1, p2) => body` — an anonymous function.
    Lambda { params: Vec<Ident>, body: Box<Expr> },
    /// An island: `s(...)` / `sound(...)` / `n(...)` / `note(...)`.
    Island(Island),
}

/// Unary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
}

/// Binary arithmetic operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

// ── Islands (mini-notation) ──────────────────────────────────────────────────

/// An `s`/`n` island. The structural grammar is shared; only the leaves differ
/// (samples vs. pitches), tracked by [`IslandKind`].
#[derive(Clone, Debug, PartialEq)]
pub struct Island {
    pub kind: IslandKind,
    pub root: Mini,
    pub span: SourceSpan,
}

/// Which kind of island — decides leaf interpretation and which postfixes are
/// legal (`:n` only in sound, `'chord` only in note).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IslandKind {
    /// `s(...)` / `sound(...)` — sample-name leaves.
    Sound,
    /// `n(...)` / `note(...)` — pitch / degree / chord leaves.
    Note,
}

/// A mini-notation node.
#[derive(Clone, Debug, PartialEq)]
pub struct Mini {
    pub kind: MiniKind,
    pub span: SourceSpan,
}

/// The shape of a [`Mini`] node. Precedence (loosest → tightest):
/// `&` (parallel) → space (sequence) → postfixes (bound on a [`MiniKind::Term`]).
#[derive(Clone, Debug, PartialEq)]
pub enum MiniKind {
    /// `a & b & c` — parallel lanes (stack). Always ≥ 2 lanes when present;
    /// a single lane is represented directly by its sequence.
    Parallel(Vec<Mini>),
    /// Space-separated terms laid in one cycle. ≥ 2 items when present.
    Sequence(Vec<Mini>),
    /// An atom with its left-to-right postfix chain (`bd*2`, `cp:2(3,8)`).
    Term {
        atom: Box<Mini>,
        postfixes: Vec<Postfix>,
    },
    /// `[ ... ]` — group several events into one slot.
    Group(Box<Mini>),
    /// `< ... >` — alternation, one element per cycle (slowcat).
    Alt(Box<Mini>),
    /// `{ ... }%n` — polymeter: lanes step at `steps` slots per cycle, each
    /// looping through its own length. `steps == None` defaults (at eval) to the
    /// length of the first lane (Strudel semantics).
    Poly {
        body: Box<Mini>,
        steps: Option<u32>,
    },
    /// `~` — a silent slot.
    Rest,
    /// `_` — extend the previous term by one more slot.
    Extend,
    /// `$ident` — splice a host variable as a leaf (name only).
    Splice(Ident),
    /// A terminal leaf.
    Leaf(Leaf),
}

/// A mini-notation leaf value.
#[derive(Clone, Debug, PartialEq)]
pub enum Leaf {
    /// A sample/sound name (`bd`, `hh`) — only in `s`/`sound`.
    Sound(String),
    /// A note name (`c4`, `ef3`) — only in `n`/`note`.
    NoteName(String),
    /// A scale degree (`0`, `2`, `7`) — only in `n`/`note`, needs `.scale(...)`.
    Degree(i32),
}

/// A postfix numeric argument: either a literal or a **patternised** sub-pattern
/// (`bd*<2 3>`, `bd(<3 5>,8)`). A patternised arg varies the factor per
/// slot/cycle, evaluated by inner-join (`design/nemus/mini-notation.md`, level
/// Full). The leaves of the sub-pattern are read as numbers at eval time.
#[derive(Clone, Debug, PartialEq)]
pub enum MiniArg {
    /// A literal number (`bd*2`, `bd(3,8)`).
    Const(f64),
    /// A sub-pattern whose per-slot values drive the postfix (`bd*<2 3>`).
    Pat(Box<Mini>),
}

impl MiniArg {
    /// Is this a literal (vs. a patternised sub-pattern)?
    pub fn is_const(&self) -> bool {
        matches!(self, MiniArg::Const(_))
    }

    /// The literal value, or `None` for a patternised arg.
    pub fn const_value(&self) -> Option<f64> {
        match self {
            MiniArg::Const(n) => Some(*n),
            MiniArg::Pat(_) => None,
        }
    }
}

/// A postfix operator attached to a term, applied left to right.
#[derive(Clone, Debug, PartialEq)]
pub enum Postfix {
    /// `*n` — fast (repeat n times inside the slot); `n` may be patternised.
    Fast(MiniArg),
    /// `/n` — slow (play once every n cycles); `n` may be patternised.
    Slow(MiniArg),
    /// `!n` — replicate as n separate slots.
    Replicate(u32),
    /// `@n` — weight (give the term more duration than its siblings).
    Weight(u32),
    /// `(n,k)` / `(n,k,rot)` — Euclidean distribution; each count may be
    /// patternised (`bd(<3 5>,8)`).
    Euclid {
        pulses: MiniArg,
        steps: MiniArg,
        rotation: Option<MiniArg>,
    },
    /// `:n` — sample variant index (only in `s`/`sound`).
    Variant(u32),
    /// `'name` — expand a note into a chord (only in `n`/`note`).
    Chord(String),
}
