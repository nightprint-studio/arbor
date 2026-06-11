//! The evaluator: `AST → Pattern`.
//!
//! Host-language semantics live here (let/fn/lambda, ranges + `.map`/`.par`/…,
//! arithmetic, name resolution, the program output); the closed stdlib of
//! combinators and transforms is in [`combinators`] and [`transforms`], and the
//! mini-notation islands in [`island`]. Totality (no recursion) is enforced both
//! statically ([`totality`]) and by a runtime depth guard.

pub(crate) mod chords;
pub(crate) mod combinators;
pub(crate) mod island;
pub(crate) mod totality;
pub(crate) mod transforms;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use arbor_grove_pattern::prelude::{
    cat, parse_note, pure, seq, stack, ControlMap, Pattern, TempoMap, Tracks,
};

use crate::ast::{BinOp, Expr, ExprKind, Import, Item, Program, UnOp};
use crate::config::EvalConfig;
use crate::convert::{as_int, as_number, as_pattern};
use crate::env::Env;
use crate::error::{LangError, LangErrorKind, Result};
use crate::inject::{LogLevel, LogSink, SourceLoader};
use crate::parse::parse;
use crate::value::{EvalOutput, Func, Value};

/// Guards against runaway recursion that the static check can't see (e.g. a
/// `let`-bound lambda that refers to its own binding).
const MAX_CALL_DEPTH: u32 = 1024;

/// Shared, immutable-ish evaluation context. Wrapped in `Rc` so transform
/// closures built from user functions can call back into evaluation.
pub struct Ctx {
    /// Top-level bindings (let/fn/imports), filled as items are processed and
    /// consulted by name at call time — so functions resolve each other and
    /// hold no reference back here.
    pub globals: RefCell<HashMap<String, Value>>,
    pub loader: Rc<dyn SourceLoader>,
    pub log: Arc<dyn LogSink>,
    pub config: EvalConfig,
    /// Set by `cps(...)`.
    pub cps: Cell<Option<f64>>,
    /// Set by `tempo(...)` — piecewise-constant tempo automation (empty = unset).
    pub tempo: RefCell<TempoMap>,
    /// Current call depth (runtime totality guard).
    depth: Cell<u32>,
    /// Import path stack for cycle detection.
    import_stack: RefCell<Vec<String>>,
}

impl std::fmt::Debug for Ctx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ctx").finish_non_exhaustive()
    }
}

/// Evaluate a whole program into its output (clock + tracks).
pub fn evaluate(
    program: &Program,
    loader: Rc<dyn SourceLoader>,
    log: Arc<dyn LogSink>,
    config: EvalConfig,
) -> Result<EvalOutput> {
    totality::check(program)?;

    let ctx = Rc::new(Ctx {
        globals: RefCell::new(HashMap::new()),
        loader,
        log,
        config,
        cps: Cell::new(None),
        tempo: RefCell::new(TempoMap::none()),
        depth: Cell::new(0),
        import_stack: RefCell::new(Vec::new()),
    });

    let mut outputs: Vec<Value> = Vec::new();
    for item in &program.items {
        match item {
            Item::Import(imp) => resolve_import(&ctx, imp)?,
            Item::Let(b) => {
                let v = eval_expr(&ctx, &Env::empty(), &b.value)?;
                ctx.globals.borrow_mut().insert(b.name.name.clone(), v);
            }
            Item::Fn(f) => {
                let func = Value::Func(Func {
                    params: f.params.iter().map(|p| p.name.clone()).collect(),
                    body: Rc::new(f.body.clone()),
                    captured: Env::empty(),
                    name: Some(f.name.name.clone()),
                });
                ctx.globals.borrow_mut().insert(f.name.name.clone(), func);
            }
            Item::Expr(e) => {
                let v = eval_expr(&ctx, &Env::empty(), e)?;
                if !matches!(v, Value::Unit) {
                    outputs.push(v);
                }
            }
        }
    }

    // Bind before the struct literal so the `Ref` is dropped here, not held into
    // the tail expression (which would outlive `ctx`).
    let tempo = ctx.tempo.borrow().clone();
    Ok(EvalOutput {
        cps: ctx.cps.get(),
        tempo,
        tracks: outputs_to_tracks(outputs)?,
    })
}

/// Resolve a name against the local scope, then the globals.
pub(crate) fn resolve(ctx: &Rc<Ctx>, env: &Env, name: &str) -> Option<Value> {
    env.lookup(name)
        .or_else(|| ctx.globals.borrow().get(name).cloned())
}

/// Evaluate an expression to a value.
pub(crate) fn eval_expr(ctx: &Rc<Ctx>, env: &Env, e: &Expr) -> Result<Value> {
    match &e.kind {
        ExprKind::Number(n) => Ok(Value::Number(*n)),
        ExprKind::Str(s) => Ok(Value::Str(s.clone())),
        ExprKind::Note(name) => {
            // Octave is mandatory in the host, so `default_octave` is never used
            // here; pass it for the shared parser's signature.
            let midi = parse_note(name, ctx.config.default_octave)
                .map_err(|err| LangError::at(e.span, LangErrorKind::Pitch(err)))?;
            Ok(Value::Pattern(pure(ControlMap::note(midi)).tag_span(e.span)))
        }
        ExprKind::Var(name) => eval_var(ctx, env, name, e.span),
        ExprKind::Call { name, args } => {
            let argv = eval_args(ctx, env, args)?;
            eval_call(ctx, env, &name.name, argv, e.span)
        }
        ExprKind::Method { recv, name, args } => {
            let recvv = eval_expr(ctx, env, recv)?;
            let argv = eval_args(ctx, env, args)?;
            eval_method(ctx, recvv, &name.name, argv, e.span)
        }
        ExprKind::Unary { op, rhs } => {
            let v = as_number(&eval_expr(ctx, env, rhs)?, rhs.span)?;
            Ok(Value::Number(match op {
                UnOp::Neg => -v,
            }))
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let a = as_number(&eval_expr(ctx, env, lhs)?, lhs.span)?;
            let b = as_number(&eval_expr(ctx, env, rhs)?, rhs.span)?;
            Ok(Value::Number(match op {
                BinOp::Add => a + b,
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                BinOp::Div => a / b,
            }))
        }
        ExprKind::Range {
            lo,
            hi,
            inclusive,
        } => {
            let lo = as_int(&eval_expr(ctx, env, lo)?, lo.span)?;
            let hi = as_int(&eval_expr(ctx, env, hi)?, hi.span)?;
            Ok(Value::Range {
                lo,
                hi,
                inclusive: *inclusive,
            })
        }
        ExprKind::Lambda { params, body } => Ok(Value::Func(Func {
            params: params.iter().map(|p| p.name.clone()).collect(),
            body: Rc::new((**body).clone()),
            captured: env.clone(),
            name: None,
        })),
        ExprKind::Island(isl) => Ok(Value::Pattern(island::eval_island(ctx, env, isl)?)),
    }
}

/// Evaluate a list of argument expressions.
fn eval_args(ctx: &Rc<Ctx>, env: &Env, args: &[Expr]) -> Result<Vec<Value>> {
    args.iter().map(|a| eval_expr(ctx, env, a)).collect()
}

/// A bare identifier: variable, log level, or nullary transform (`rev`).
fn eval_var(ctx: &Rc<Ctx>, env: &Env, name: &str, span: arbor_grove_pattern::prelude::SourceSpan) -> Result<Value> {
    if let Some(v) = resolve(ctx, env, name) {
        return Ok(v);
    }
    if let Some(level) = LogLevel::parse(name) {
        return Ok(Value::Level(level));
    }
    if let Some(sig) = combinators::signal_source(name) {
        return Ok(Value::NumSignal(sig));
    }
    if let Some(tf) = transforms::nullary_transform(name) {
        return Ok(Value::Transform(tf));
    }
    Err(LangError::at(span, LangErrorKind::UnknownName(name.to_string())))
}

/// A `name(args)` call: user function, combinator/constructor, or a
/// partially-applied transform.
fn eval_call(
    ctx: &Rc<Ctx>,
    env: &Env,
    name: &str,
    args: Vec<Value>,
    span: arbor_grove_pattern::prelude::SourceSpan,
) -> Result<Value> {
    if let Some(v) = resolve(ctx, env, name) {
        return match v {
            Value::Func(f) => call_func(ctx, &f, args, span),
            other => Err(LangError::at(
                span,
                LangErrorKind::NotCallable(other.type_name().to_string()),
            )),
        };
    }
    if combinators::is_combinator(name) {
        return combinators::eval_builtin_call(ctx, name, args, span);
    }
    if transforms::is_transform(name) {
        return Ok(Value::Transform(transforms::make_transform(
            ctx, name, &args, span,
        )?));
    }
    Err(LangError::at(span, LangErrorKind::UnknownName(name.to_string())))
}

/// A `recv.name(args)` method call.
fn eval_method(
    ctx: &Rc<Ctx>,
    recv: Value,
    name: &str,
    args: Vec<Value>,
    span: arbor_grove_pattern::prelude::SourceSpan,
) -> Result<Value> {
    match recv {
        Value::Pattern(p) => {
            let tf = transforms::make_transform(ctx, name, &args, span)?;
            Ok(Value::Pattern(tf.apply(p)?))
        }
        // A transform after `arrange(...)` drops the section layout (the result
        // is no longer a straight arrangement) — operate on its pattern.
        Value::Arrangement(p, _, _) => {
            let tf = transforms::make_transform(ctx, name, &args, span)?;
            Ok(Value::Pattern(tf.apply(p)?))
        }
        Value::NumSignal(sig) => signal_method(sig, name, args, span),
        seq_recv @ (Value::Range { .. } | Value::List(_)) => {
            seq_method(ctx, seq_recv, name, args, span)
        }
        other => Err(LangError::at(
            span,
            LangErrorKind::Type {
                expected: "pattern, list or range".to_string(),
                got: other.type_name().to_string(),
            },
        )),
    }
}

/// Methods on a continuous numeric signal (`sine`, `saw`, …): `.range(lo, hi)`
/// to rescale the unipolar `0..1` source, and `.fast`/`.slow` to change its rate.
/// The result is still a `NumSignal`, chainable into a patternised control
/// (`.lpf(sine.range(200, 2000))`).
fn signal_method(
    sig: Pattern<f64>,
    name: &str,
    args: Vec<Value>,
    span: arbor_grove_pattern::prelude::SourceSpan,
) -> Result<Value> {
    use crate::convert::{as_number, f64_to_time};
    match name {
        "range" => {
            if args.len() != 2 {
                return Err(LangError::at(
                    span,
                    LangErrorKind::Arity {
                        name: "range".to_string(),
                        expected: 2,
                        got: args.len(),
                    },
                ));
            }
            let lo = as_number(&args[0], span)?;
            let hi = as_number(&args[1], span)?;
            Ok(Value::NumSignal(sig.range(lo, hi)))
        }
        "fast" => {
            let n = f64_to_time(as_number(&args[0], span)?);
            Ok(Value::NumSignal(sig.fast(n)))
        }
        "slow" => {
            let n = f64_to_time(as_number(&args[0], span)?);
            Ok(Value::NumSignal(sig.slow(n)))
        }
        other => Err(LangError::at(
            span,
            LangErrorKind::NotCallable(format!("numeric-signal method `{other}`")),
        )),
    }
}

/// `.map` / `.par` / `.seq` / `.cat` on a range or list.
fn seq_method(
    ctx: &Rc<Ctx>,
    recv: Value,
    name: &str,
    args: Vec<Value>,
    span: arbor_grove_pattern::prelude::SourceSpan,
) -> Result<Value> {
    let items: Vec<Value> = match recv {
        Value::List(v) => v,
        Value::Range {
            lo,
            hi,
            inclusive,
        } => {
            let end = if inclusive { hi + 1 } else { hi };
            (lo..end).map(|i| Value::Number(i as f64)).collect()
        }
        _ => unreachable!("seq_method only called on list/range"),
    };

    let func = match args.into_iter().next() {
        Some(Value::Func(f)) => f,
        Some(other) => {
            return Err(LangError::at(
                span,
                LangErrorKind::Type {
                    expected: "function".to_string(),
                    got: other.type_name().to_string(),
                },
            ))
        }
        None => {
            return Err(LangError::at(
                span,
                LangErrorKind::Arity {
                    name: name.to_string(),
                    expected: 1,
                    got: 0,
                },
            ))
        }
    };

    let mapped: Vec<Value> = items
        .into_iter()
        .map(|it| call_func(ctx, &func, vec![it], span))
        .collect::<Result<_>>()?;

    match name {
        "map" => Ok(Value::List(mapped)),
        "par" | "seq" | "cat" => {
            let pats = mapped
                .into_iter()
                .map(|v| as_pattern(v, span))
                .collect::<Result<Vec<Pattern<ControlMap>>>>()?;
            Ok(Value::Pattern(match name {
                "par" => stack(pats),
                "seq" => seq(pats),
                _ => cat(pats),
            }))
        }
        other => Err(LangError::at(
            span,
            LangErrorKind::NotCallable(format!("range/list method `{other}`")),
        )),
    }
}

/// Call a user function/lambda with already-evaluated arguments.
pub(crate) fn call_func(
    ctx: &Rc<Ctx>,
    func: &Func,
    args: Vec<Value>,
    span: arbor_grove_pattern::prelude::SourceSpan,
) -> Result<Value> {
    if args.len() != func.params.len() {
        return Err(LangError::at(
            span,
            LangErrorKind::Arity {
                name: func.name.clone().unwrap_or_else(|| "lambda".to_string()),
                expected: func.params.len(),
                got: args.len(),
            },
        ));
    }

    let depth = ctx.depth.get() + 1;
    if depth > MAX_CALL_DEPTH {
        return Err(LangError::at(
            span,
            LangErrorKind::Recursion(vec![func
                .name
                .clone()
                .unwrap_or_else(|| "lambda".to_string())]),
        ));
    }
    ctx.depth.set(depth);

    let bindings: HashMap<String, Value> = func
        .params
        .iter()
        .cloned()
        .zip(args)
        .collect();
    let local = func.captured.child(bindings);
    let result = eval_expr(ctx, &local, &func.body);

    ctx.depth.set(depth - 1);
    result
}

/// Fold the top-level output expressions into the channel list. A bare pattern
/// becomes one anonymous track; multiple outputs concatenate their channels.
fn outputs_to_tracks(outputs: Vec<Value>) -> Result<Tracks<ControlMap>> {
    use arbor_grove_pattern::prelude::{track, track_with_sections, tracks};
    let mut channels = Vec::new();
    for v in outputs {
        match v {
            Value::Tracks(t) => channels.extend(t.tracks),
            Value::Track(t) => channels.push(t),
            Value::Pattern(p) => channels.push(track("", p)),
            // A bare top-level `arrange(...)` is one anonymous track; keep its bands.
            Value::Arrangement(p, sections, period) => {
                channels.push(track_with_sections("", p, sections, period))
            }
            other => {
                return Err(LangError::unlocated(LangErrorKind::Type {
                    expected: "tracks, track or pattern".to_string(),
                    got: other.type_name().to_string(),
                }))
            }
        }
    }
    Ok(tracks(channels))
}

// ── Imports ──────────────────────────────────────────────────────────────────

/// Resolve an `import { names } from "path"`: load + parse the module, evaluate
/// its top-level definitions, and bind the requested names into the globals.
fn resolve_import(ctx: &Rc<Ctx>, imp: &Import) -> Result<()> {
    let module = load_module(ctx, &imp.path).map_err(|e| e.or_span(imp.span))?;
    for name in &imp.names {
        match module.get(&name.name) {
            Some(v) => {
                ctx.globals
                    .borrow_mut()
                    .insert(name.name.clone(), v.clone());
            }
            None => {
                return Err(LangError::at(
                    name.span,
                    LangErrorKind::UnknownName(name.name.clone()),
                ))
            }
        }
    }
    Ok(())
}

/// Load + parse a module and evaluate its top-level `let`/`fn` into a map
/// (its output is ignored). Detects import cycles via the path stack.
fn load_module(ctx: &Rc<Ctx>, path: &str) -> Result<HashMap<String, Value>> {
    if ctx.import_stack.borrow().iter().any(|p| p == path) {
        let mut chain = ctx.import_stack.borrow().clone();
        chain.push(path.to_string());
        return Err(LangError::unlocated(LangErrorKind::ImportCycle(chain)));
    }

    let source = ctx
        .loader
        .load(path)
        .map_err(|m| LangError::unlocated(LangErrorKind::Other(m)))?;
    let program = parse(&source)?;
    totality::check(&program)?;

    ctx.import_stack.borrow_mut().push(path.to_string());

    // A module has its own globals; reuse a sub-context sharing loader/log/stack.
    let sub = Rc::new(Ctx {
        globals: RefCell::new(HashMap::new()),
        loader: ctx.loader.clone(),
        log: ctx.log.clone(),
        config: ctx.config,
        cps: Cell::new(None),
        tempo: RefCell::new(TempoMap::none()),
        depth: Cell::new(0),
        import_stack: RefCell::new(ctx.import_stack.borrow().clone()),
    });

    let mut result: Result<()> = Ok(());
    for item in &program.items {
        match item {
            Item::Import(inner) => {
                if let Err(e) = resolve_import(&sub, inner) {
                    result = Err(e);
                    break;
                }
            }
            Item::Let(b) => match eval_expr(&sub, &Env::empty(), &b.value) {
                Ok(v) => {
                    sub.globals.borrow_mut().insert(b.name.name.clone(), v);
                }
                Err(e) => {
                    result = Err(e);
                    break;
                }
            },
            Item::Fn(f) => {
                let func = Value::Func(Func {
                    params: f.params.iter().map(|p| p.name.clone()).collect(),
                    body: Rc::new(f.body.clone()),
                    captured: Env::empty(),
                    name: Some(f.name.name.clone()),
                });
                sub.globals.borrow_mut().insert(f.name.name.clone(), func);
            }
            Item::Expr(_) => {} // a library file's output is ignored
        }
    }

    ctx.import_stack.borrow_mut().pop();
    result?;
    let module_globals = sub.globals.borrow().clone();
    Ok(module_globals)
}
