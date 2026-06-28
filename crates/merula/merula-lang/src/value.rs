//! Runtime values produced by the evaluator.
//!
//! The key distinctions the type system enforces:
//! - a **musical pattern** (`Pattern<ControlMap>`) vs. a **numeric signal**
//!   (`Pattern<f64>`, e.g. `rand`) — the latter feeds patternised controls
//!   (`.gain(rand(...))`) and coerces to the pattern crate's `Param`;
//! - a **transform value** ([`Transform`]) — the partial-application form
//!   (`gain(0.4)`, `rev`) passed to higher-order transforms — vs. a plain
//!   pattern. Which one a name produces is resolved at eval time, not by the
//!   parser (`design/merula/host-language.md`).
//!
//! Everything is single-threaded (`Rc`, no `Send`/`Sync`): evaluation runs to
//! completion on one thread and only the *resulting* `Pattern` — which the
//! pattern crate guarantees is `Send + Sync` — is handed to the engine later.

use std::fmt;
use std::rc::Rc;

use merula_pattern::prelude::{
    ControlMap, Pattern, Section, SectionSpan, TempoMap, Track, Tracks,
};

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
    /// An `arrange` section (`cycles(n, pat)` / `section(name, n, pat)`).
    Section(Section<ControlMap>),
    /// A tempo-map segment (`cycles(n, cps)` inside `tempo(...)`): `n` cycles at
    /// `cps` cycles-per-second.
    TempoSeg { cycles: u32, cps: f64 },
    /// An `arrange(...)` result: the flattened pattern, the named-section layout,
    /// and the loop period (total cycles). Coerces to its pattern everywhere a
    /// pattern is needed; the layout + period are captured by `track(...)` for the
    /// arrangement view.
    Arrangement(Pattern<ControlMap>, Vec<SectionSpan>, u32),
    /// One named channel (`track(name, pat)`).
    Track(Track<ControlMap>),
    /// A launchable clip variation declared inside a `track(...)`
    /// (`clip(scene_name, pat)`): the scene label it belongs to + its pattern. The
    /// owning track pairs it with its own name when registering the scene.
    Clip(String, Pattern<ControlMap>),
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
            Value::TempoSeg { .. } => "tempo segment",
            Value::Arrangement(..) => "arrangement",
            Value::Track(_) => "track",
            Value::Clip(..) => "clip",
            Value::Tracks(_) => "tracks",
            Value::Unit => "unit",
        }
    }
}

/// A launchable scene declared with `scene("name", track(...), …)`: a bundle of
/// per-track pattern variations the clip launcher fires together. Each clip is a
/// named `Track`; launching the scene overrides the same-named base track at the
/// next cycle boundary (a clip whose name has no matching base track is inert).
///
/// Scenes are a side-channel of evaluation — like `cps`/`tempo` they register a
/// declaration and produce no pattern in the linear output; the launcher reads
/// [`EvalOutput::scenes`] and substitutes the chosen clips into the staged tracks.
#[derive(Clone, Debug)]
pub struct Scene {
    /// The scene label shown as a launcher row (string).
    pub name: String,
    /// One clip per targeted track (`track(name, pat)`), in source order.
    pub clips: Vec<Track<ControlMap>>,
}

/// The result of evaluating a whole program: the clock rate (if `cps(...)` was
/// called), the tempo automation (if `tempo(...)` was called), the output
/// channels, and any launchable `scene(...)` declarations.
#[derive(Clone, Debug)]
pub struct EvalOutput {
    /// Cycles per second, if set by `cps(...)`. Ignored when `tempo` is non-empty.
    pub cps: Option<f64>,
    /// Piecewise-constant tempo automation from `tempo(...)`; empty when unset.
    pub tempo: TempoMap,
    /// The output channels (a bare top-level pattern becomes one anonymous track).
    pub tracks: Tracks<ControlMap>,
    /// Launchable scenes from `scene(...)`, in source order; empty when none.
    pub scenes: Vec<Scene>,
}
