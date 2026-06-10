//! Runtime values produced by the evaluator.
//!
//! The key distinctions the type system enforces:
//! - a **musical pattern** (`Pattern<ControlMap>`) vs. a **numeric signal**
//!   (`Pattern<f64>`, e.g. `rand`) — the latter feeds patternised controls
//!   (`.gain(rand(...))`) and coerces to the pattern crate's `Param`;
//! - a **transform value** ([`Transform`]) — the partial-application form
//!   (`gain(0.4)`, `rev`) passed to higher-order transforms — vs. a plain
//!   pattern. Which one a name produces is resolved at eval time, not by the
//!   parser (`design/grove/host-language.md`).
//!
//! Everything is single-threaded (`Rc`, no `Send`/`Sync`): evaluation runs to
//! completion on one thread and only the *resulting* `Pattern` — which the
//! pattern crate guarantees is `Send + Sync` — is handed to the engine later.

use std::fmt;
use std::rc::Rc;

use arbor_grove_pattern::prelude::{ControlMap, Pattern, Section, Track, Tracks};

use crate::ast::Expr;
use crate::env::Env;
use crate::error::Result;
use crate::inject::LogLevel;

/// A pattern-to-pattern function: the transform-value of the language.
///
/// Built from the closed stdlib (`fast(2)`, `gain(0.4)`, `rev`, …) or from a
/// user `fn`/lambda of one argument. Fallible because a user function's body
/// may fail to evaluate; the builtin transforms are validated at construction
/// and never error on application.
#[derive(Clone)]
pub struct Transform(Rc<dyn Fn(Pattern<ControlMap>) -> Result<Pattern<ControlMap>>>);

impl Transform {
    /// Wrap a closure as a transform value.
    pub fn new(f: impl Fn(Pattern<ControlMap>) -> Result<Pattern<ControlMap>> + 'static) -> Self {
        Transform(Rc::new(f))
    }

    /// Apply the transform to a pattern.
    pub fn apply(&self, pat: Pattern<ControlMap>) -> Result<Pattern<ControlMap>> {
        (self.0)(pat)
    }
}

impl fmt::Debug for Transform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Transform")
    }
}

/// A user-defined function (`fn name(p) = …`) or lambda (`p => …`).
///
/// Lexically scoped: `captured` is the local environment at definition; free
/// names not found there resolve against the globals the evaluator threads
/// separately (so a top-level `fn` holds no reference back to the globals — no
/// reference cycle).
#[derive(Clone, Debug)]
pub struct Func {
    pub params: Vec<String>,
    pub body: Rc<Expr>,
    pub captured: Env,
    /// Name for diagnostics and the recursion check; `None` for a lambda.
    pub name: Option<String>,
}

/// A value flowing through evaluation.
#[derive(Clone, Debug)]
pub enum Value {
    /// A number (the only numeric host type — integers are `f64` too).
    Number(f64),
    /// A string literal.
    Str(String),
    /// A musical pattern.
    Pattern(Pattern<ControlMap>),
    /// A continuous numeric signal (`rand`), for patternised controls.
    NumSignal(Pattern<f64>),
    /// A transform value (partial application / nullary transform).
    Transform(Transform),
    /// A callable function/lambda.
    Func(Func),
    /// A list (from a list literal or `.map`).
    List(Vec<Value>),
    /// An integer range (`a..b` / `a..=b`).
    Range {
        lo: i64,
        hi: i64,
        inclusive: bool,
    },
    /// A log level keyword used as a value (`.log(info)`).
    Level(LogLevel),
    /// An `arrange` section (`cycles(n, pat)`).
    Section(Section<ControlMap>),
    /// One named channel (`track(name, pat)`).
    Track(Track<ControlMap>),
    /// The track list output (`tracks(...)`).
    Tracks(Tracks<ControlMap>),
    /// No meaningful value (a log statement, `cps(...)`).
    Unit,
}

impl Value {
    /// A short type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "number",
            Value::Str(_) => "string",
            Value::Pattern(_) => "pattern",
            Value::NumSignal(_) => "numeric signal",
            Value::Transform(_) => "transform",
            Value::Func(_) => "function",
            Value::List(_) => "list",
            Value::Range { .. } => "range",
            Value::Level(_) => "log level",
            Value::Section(_) => "section",
            Value::Track(_) => "track",
            Value::Tracks(_) => "tracks",
            Value::Unit => "unit",
        }
    }
}

/// The result of evaluating a whole program: the clock rate (if `cps(...)` was
/// called) and the output channels.
#[derive(Clone, Debug)]
pub struct EvalOutput {
    /// Cycles per second, if set by `cps(...)`.
    pub cps: Option<f64>,
    /// The output channels (a bare top-level pattern becomes one anonymous track).
    pub tracks: Tracks<ControlMap>,
}
