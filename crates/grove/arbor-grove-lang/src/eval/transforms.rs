//! The transform vocabulary: building [`Transform`] values from the closed
//! stdlib and applying them.
//!
//! Every transform name maps to a closure over its (already-evaluated)
//! arguments. The higher-order transforms (`every`/`off`/`sometimes`/`jux`)
//! take another transform; they pre-apply it so a user function's evaluation
//! error surfaces with a real result instead of inside the pattern crate's
//! infallible closures.

use std::rc::Rc;
use std::sync::Arc;

use arbor_grove_pattern::prelude::{silence, stack, ControlMap, Pattern, Scale, SourceSpan};

use crate::convert::{as_int, as_number, as_param, as_pattern, as_str, f64_to_time};
use crate::error::{LangError, LangErrorKind, Result};
use crate::eval::{call_func, Ctx};
use crate::inject::{LogLevel, LogSink};
use crate::value::{Transform, Value};

/// Is `name` a transform (vs. a combinator or user binding)?
pub fn is_transform(name: &str) -> bool {
    matches!(
        name,
        "rev" | "degrade" | "palindrome"
            | "fast" | "slow"
            | "gain" | "pan" | "room" | "lpf" | "hpf" | "shift" | "speed" | "crush" | "shape"
            | "vel" | "inst" | "art" | "scale"
            | "add" | "addDeg"
            | "degradeBy" | "sometimesBy"
            | "chunk" | "iter" | "swingBy" | "delay"
            | "every" | "off" | "sometimes" | "jux"
            | "log"
    )
}

/// The transform value of a bare nullary transform (`rev`, `degrade`,
/// `palindrome`).
pub fn nullary_transform(name: &str) -> Option<Transform> {
    match name {
        "rev" => Some(Transform::new(|p| Ok(p.rev()))),
        "degrade" => Some(Transform::new(|p| Ok(p.degrade()))),
        "palindrome" => Some(Transform::new(|p| Ok(p.palindrome()))),
        _ => None,
    }
}

fn arity(name: &str, args: &[Value], expected: usize, span: SourceSpan) -> Result<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(LangError::at(
            span,
            LangErrorKind::Arity {
                name: name.to_string(),
                expected,
                got: args.len(),
            },
        ))
    }
}

/// Coerce a value to a transform: a transform passes through; a one-argument
/// function becomes one (applied to the pattern it receives).
pub fn as_transform(ctx: &Rc<Ctx>, v: &Value, span: SourceSpan) -> Result<Transform> {
    match v {
        Value::Transform(t) => Ok(t.clone()),
        Value::Func(f) if f.params.len() == 1 => {
            let ctx = ctx.clone();
            let f = f.clone();
            Ok(Transform::new(move |p| {
                let out = call_func(&ctx, &f, vec![Value::Pattern(p)], span)?;
                as_pattern(out, span)
            }))
        }
        other => Err(LangError::at(
            span,
            LangErrorKind::Type {
                expected: "transform (or 1-argument function)".to_string(),
                got: other.type_name().to_string(),
            },
        )),
    }
}

/// Build the transform value named `name` from its arguments.
pub fn make_transform(
    ctx: &Rc<Ctx>,
    name: &str,
    args: &[Value],
    span: SourceSpan,
) -> Result<Transform> {
    match name {
        "rev" | "degrade" | "palindrome" => {
            arity(name, args, 0, span)?;
            Ok(nullary_transform(name).expect("nullary"))
        }

        "fast" => {
            arity(name, args, 1, span)?;
            let n = f64_to_time(as_number(&args[0], span)?);
            Ok(Transform::new(move |p| Ok(p.fast(n))))
        }
        "slow" => {
            arity(name, args, 1, span)?;
            let n = f64_to_time(as_number(&args[0], span)?);
            Ok(Transform::new(move |p| Ok(p.slow(n))))
        }

        // Voice/mix: a constant number or a numeric signal.
        "gain" => mix(args, span, |p, x| p.gain(x)),
        "pan" => mix(args, span, |p, x| p.pan(x)),
        "room" => mix(args, span, |p, x| p.room(x)),
        "lpf" => mix(args, span, |p, x| p.lpf(x)),
        "hpf" => mix(args, span, |p, x| p.hpf(x)),
        "shift" => mix(args, span, |p, x| p.shift(x)),
        "speed" => mix(args, span, |p, x| p.speed(x)),
        "crush" => mix(args, span, |p, x| p.crush(x)),
        "shape" => mix(args, span, |p, x| p.shape(x)),
        "vel" => mix(args, span, |p, x| p.vel(x)),

        "inst" => {
            arity(name, args, 1, span)?;
            let s = as_str(&args[0], span)?;
            Ok(Transform::new(move |p| Ok(p.inst(s.clone()))))
        }
        "art" => {
            arity(name, args, 1, span)?;
            let s = as_str(&args[0], span)?;
            Ok(Transform::new(move |p| Ok(p.art(s.clone()))))
        }
        "scale" => {
            arity(name, args, 1, span)?;
            let spec = as_str(&args[0], span)?;
            let scale = Scale::parse(&spec)
                .map_err(|e| LangError::at(span, LangErrorKind::Pitch(e)))?;
            let oct = ctx.config.default_octave;
            Ok(Transform::new(move |p| Ok(p.scale(scale.clone(), oct))))
        }

        // ── §F transposition (two separate transforms) ──────────────────────
        "add" => {
            arity(name, args, 1, span)?;
            let n = as_number(&args[0], span)?;
            Ok(Transform::new(move |p| Ok(p.add_note(n))))
        }
        "addDeg" => {
            arity(name, args, 1, span)?;
            let n = as_int(&args[0], span)? as i32;
            Ok(Transform::new(move |p| Ok(p.add_degree(n))))
        }

        // ── §F parametric probability ───────────────────────────────────────
        "degradeBy" => {
            arity(name, args, 1, span)?;
            let prob = as_number(&args[0], span)?;
            Ok(Transform::new(move |p| Ok(p.degrade_by(prob))))
        }
        "sometimesBy" => {
            arity(name, args, 2, span)?;
            let prob = as_number(&args[0], span)?;
            let tf = as_transform(ctx, &args[1], span)?;
            Ok(Transform::new(move |p| {
                tf.apply(p.clone())?; // surface inner errors up front (see `sometimes`)
                let tf2 = tf.clone();
                Ok(p.sometimes_by(prob, move |q| tf2.apply(q).unwrap_or_else(|_| silence())))
            }))
        }

        // ── §F structural ───────────────────────────────────────────────────
        "chunk" => {
            arity(name, args, 2, span)?;
            let n = as_int(&args[0], span)?;
            let tf = as_transform(ctx, &args[1], span)?;
            Ok(Transform::new(move |p| {
                tf.apply(p.clone())?; // surface inner errors up front
                let tf2 = tf.clone();
                Ok(p.chunk(n, move |q| tf2.apply(q).unwrap_or_else(|_| silence())))
            }))
        }
        "iter" => {
            arity(name, args, 1, span)?;
            let n = as_int(&args[0], span)?;
            Ok(Transform::new(move |p| Ok(p.iter(n))))
        }
        "swingBy" => {
            arity(name, args, 2, span)?;
            let amount = f64_to_time(as_number(&args[0], span)?);
            let n = as_int(&args[1], span)?;
            Ok(Transform::new(move |p| Ok(p.swing_by(amount, n))))
        }

        // ── §F delay (writes the ControlMap delay fields) ───────────────────
        "delay" => make_delay(args, span),

        "every" => {
            arity(name, args, 2, span)?;
            let n = as_int(&args[0], span)?;
            let tf = as_transform(ctx, &args[1], span)?;
            Ok(Transform::new(move |p| {
                let transformed = tf.apply(p.clone())?;
                Ok(p.every(n, move |_| transformed.clone()))
            }))
        }
        "off" => {
            arity(name, args, 2, span)?;
            let t = f64_to_time(as_number(&args[0], span)?);
            let tf = as_transform(ctx, &args[1], span)?;
            Ok(Transform::new(move |p| {
                let copy = tf.apply(p.clone().late(t))?;
                Ok(stack(vec![p, copy]))
            }))
        }
        "sometimes" => {
            arity(name, args, 1, span)?;
            let tf = as_transform(ctx, &args[0], span)?;
            Ok(Transform::new(move |p| {
                // Surface the transform's errors up front; the partition the
                // pattern crate hands its closure is a structural subset, so the
                // inner application can't newly fail.
                tf.apply(p.clone())?;
                let tf2 = tf.clone();
                Ok(p.sometimes(move |q| tf2.apply(q).unwrap_or_else(|_| silence())))
            }))
        }
        "jux" => {
            arity(name, args, 1, span)?;
            let tf = as_transform(ctx, &args[0], span)?;
            Ok(Transform::new(move |p| {
                let right = tf.apply(p.clone())?.pan(1.0);
                Ok(stack(vec![p.pan(0.0), right]))
            }))
        }

        "log" => {
            let level = match args.first() {
                None => LogLevel::Debug,
                Some(Value::Level(l)) => *l,
                Some(Value::Str(s)) => LogLevel::parse(s).ok_or_else(|| {
                    LangError::at(span, LangErrorKind::Other(format!("unknown log level `{s}`")))
                })?,
                Some(other) => {
                    return Err(LangError::at(
                        span,
                        LangErrorKind::Type {
                            expected: "log level".to_string(),
                            got: other.type_name().to_string(),
                        },
                    ))
                }
            };
            let sink = ctx.log.clone();
            Ok(Transform::new(move |p| Ok(log_pattern(p, level, sink.clone()))))
        }

        _ => Err(LangError::at(
            span,
            LangErrorKind::NotCallable(format!("transform `{name}`")),
        )),
    }
}

/// Default delay feedback when `fb` is omitted.
const DELAY_DEFAULT_FEEDBACK: f64 = 0.3;
/// Default delay wet mix when `mix` is omitted.
const DELAY_DEFAULT_MIX: f64 = 0.5;

/// `delay(t, fb?, mix?)` — stamp the three `ControlMap` delay fields (`delay`
/// time in fractions of a cycle, `feedback`, `delay_mix` send). The audio engine
/// realises the audible feedback echo; here it is pure data on each hap. `fb`
/// defaults to `0.3`, `mix` to `0.5`.
fn make_delay(args: &[Value], span: SourceSpan) -> Result<Transform> {
    if args.is_empty() || args.len() > 3 {
        return Err(LangError::at(
            span,
            LangErrorKind::Arity {
                name: "delay".to_string(),
                expected: 1, // 1..3; report the minimum
                got: args.len(),
            },
        ));
    }
    let t = as_number(&args[0], span)?;
    let fb = match args.get(1) {
        Some(v) => as_number(v, span)?,
        None => DELAY_DEFAULT_FEEDBACK,
    };
    let mix = match args.get(2) {
        Some(v) => as_number(v, span)?,
        None => DELAY_DEFAULT_MIX,
    };
    Ok(Transform::new(move |p| {
        Ok(p.fmap(move |mut c| {
            c.delay = Some(t);
            c.feedback = Some(fb);
            c.delay_mix = Some(mix);
            c
        }))
    }))
}

/// Build a voice/mix transform from a single patternisable parameter.
fn mix(
    args: &[Value],
    span: SourceSpan,
    apply: impl Fn(Pattern<ControlMap>, arbor_grove_pattern::prelude::Param) -> Pattern<ControlMap>
        + 'static,
) -> Result<Transform> {
    if args.len() != 1 {
        return Err(LangError::at(
            span,
            LangErrorKind::Arity {
                name: "voice/mix control".to_string(),
                expected: 1,
                got: args.len(),
            },
        ));
    }
    let param = as_param(&args[0], span)?;
    Ok(Transform::new(move |p| Ok(apply(p, param.clone()))))
}

/// Wrap a pattern so each onset is logged at `level` (gated by the sink).
fn log_pattern(
    pat: Pattern<ControlMap>,
    level: LogLevel,
    sink: Arc<dyn LogSink>,
) -> Pattern<ControlMap> {
    Pattern::new(move |span| {
        let haps = pat.query(span);
        if sink.enabled(level) {
            for h in &haps {
                if h.has_onset() {
                    sink.log(level, &format!("{:?} @ {:?}", h.value, h.part.begin));
                }
            }
        }
        haps
    })
}
