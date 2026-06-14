//! Combinators, constructors, generators, and eval-time logging — everything
//! whose `name(args)` form produces a value rather than a transform.

use arbor_nemus_pattern::prelude::{
    arrange, audio, cat, choose, cycles, isaw, rand, sample, saw, section, section_layout, seq,
    sine, square, stack, time_to_index, track, track_with_sections, tracks, tri, ControlMap, Hap,
    Pattern, Section, SourceSpan, TempoMap, TimeSpan, Track,
};

use crate::convert::{as_int, as_number, as_pattern, as_str};
use crate::error::{LangError, LangErrorKind, Result};
use crate::eval::Ctx;
use crate::inject::LogLevel;
use crate::value::{Scene, Value};

use std::rc::Rc;

/// Distinct RNG stream for `choose` over patterns (the float `choose` uses the
/// pattern crate's own seed).
const SEED_CHOOSE_PATTERNS: u64 = 0xc4_05e_9a77e_5_u64;

/// Is `name` a combinator / constructor / generator / log function (anything the
/// evaluator dispatches through [`eval_builtin_call`])?
///
/// The set is **derived from the canonical DSL catalogue** ([`crate::reference`]):
/// the combinators + generators + log functions + the two builtin keywords
/// (`cps`/`tempo`). Keeping the membership test in one place (the catalogue) means
/// a new builtin only has to be added there — the `match` in `eval_builtin_call`
/// below maps the name to its closure, but no longer re-states the name list.
pub fn is_combinator(name: &str) -> bool {
    use crate::reference::{combinator_names, generator_names, log_names};
    combinator_names().iter().any(|n| *n == name)
        || generator_names().iter().any(|n| *n == name)
        || log_names().iter().any(|n| *n == name)
        || matches!(name, "cps" | "tempo")
}

/// A bare continuous signal source (`sine`, `saw`, …) — a unipolar `0..1`
/// `Pattern<f64>` used as a patternised parameter (`.lpf(sine.range(...))`).
/// `None` if `name` is not a signal source.
pub fn signal_source(name: &str) -> Option<Pattern<f64>> {
    Some(match name {
        "sine" => sine(),
        "saw" => saw(),
        "isaw" => isaw(),
        "tri" => tri(),
        "square" => square(),
        _ => return None,
    })
}

/// Evaluate a builtin call.
pub fn eval_builtin_call(
    ctx: &Rc<Ctx>,
    name: &str,
    args: Vec<Value>,
    span: SourceSpan,
) -> Result<Value> {
    match name {
        "par" | "stack" => Ok(Value::Pattern(stack(collect_patterns(args, span)?))),
        "seq" => Ok(Value::Pattern(seq(collect_patterns(args, span)?))),
        "cat" => Ok(Value::Pattern(cat(collect_patterns(args, span)?))),

        "arrange" => {
            let sections = flatten_varargs(args)
                .into_iter()
                .map(|v| as_section(v, span))
                .collect::<Result<Vec<Section<ControlMap>>>>()?;
            // Keep the named-section layout + loop period alongside the flattened
            // pattern so a wrapping `track(...)` can surface the bands to the view.
            let layout = section_layout(&sections);
            let period: u32 = sections.iter().map(|s| s.cycles).sum();
            Ok(Value::Arrangement(arrange(sections), layout, period))
        }
        "cycles" => {
            arity("cycles", &args, 2, span)?;
            let n = as_int(&args[0], span)?.max(0) as u32;
            // `cycles(n, x)` is overloaded by the 2nd argument: a **number** is a
            // tempo segment (`n` cycles at that `cps`, used inside `tempo(...)`); a
            // **pattern** is an arrange section. A bare number was never a valid
            // arrange section, so there is no ambiguity.
            let second = args.into_iter().nth(1).unwrap();
            match second {
                Value::Number(cps) => Ok(Value::TempoSeg { cycles: n, cps }),
                other => Ok(Value::Section(cycles(n, as_pattern(other, span)?))),
            }
        }
        "section" => {
            arity("section", &args, 3, span)?;
            let name = as_str(&args[0], span)?;
            let n = as_int(&args[1], span)?;
            let pat = as_pattern(args.into_iter().nth(2).unwrap(), span)?;
            Ok(Value::Section(section(name, n.max(0) as u32, pat)))
        }
        // `track(name, pattern, clip(...), …)` — one named channel, plus any number
        // of trailing `clip(...)` launch variations. The clips register into their
        // scene (keyed by clip name), each paired with this track's own name, so the
        // clip launcher's rows are built from the tracks that own the clips.
        "track" => {
            if args.len() < 2 {
                return Err(LangError::at(
                    span,
                    LangErrorKind::Arity { name: "track".to_string(), expected: 2, got: args.len() },
                ));
            }
            let mut it = args.into_iter();
            let name = as_str(&it.next().unwrap(), span)?;
            // A track over an `arrange(...)` carries its section layout; over a plain
            // pattern it has none.
            let chan = match it.next().unwrap() {
                Value::Arrangement(pat, sections, period) => {
                    track_with_sections(name.clone(), pat, sections, period)
                }
                other => track(name.clone(), as_pattern(other, span)?),
            };
            for v in it {
                match v {
                    Value::Clip(scene_name, pat) => {
                        register_clip(ctx, &scene_name, track(name.clone(), pat));
                    }
                    other => return Err(type_err(span, "clip (clip(name, pattern))", &other)),
                }
            }
            Ok(Value::Track(chan))
        }
        // `clip(name, pattern)` — a launchable variation of the enclosing track for
        // the scene `name`. Only meaningful as a trailing argument of `track(...)`.
        "clip" => {
            arity("clip", &args, 2, span)?;
            let name = as_str(&args[0], span)?;
            let pat = as_pattern(args.into_iter().nth(1).unwrap(), span)?;
            Ok(Value::Clip(name, pat))
        }
        "tracks" => {
            let chans = flatten_varargs(args)
                .into_iter()
                .map(|v| as_track(v, span))
                .collect::<Result<Vec<Track<ControlMap>>>>()?;
            Ok(Value::Tracks(tracks(chans)))
        }

        "rand" => {
            arity("rand", &args, 2, span)?;
            let lo = as_number(&args[0], span)?;
            let hi = as_number(&args[1], span)?;
            Ok(Value::NumSignal(rand(lo, hi)))
        }
        "choose" => eval_choose(args, span),
        "sample" => {
            arity("sample", &args, 1, span)?;
            Ok(Value::Pattern(sample(as_str(&args[0], span)?)))
        }
        "audio" => {
            arity("audio", &args, 1, span)?;
            Ok(Value::Pattern(audio(as_str(&args[0], span)?)))
        }

        "cps" => {
            arity("cps", &args, 1, span)?;
            ctx.cps.set(Some(as_number(&args[0], span)?));
            Ok(Value::Unit)
        }
        // `tempo(cycles(n, cps), …)` — piecewise-constant tempo automation. Each
        // arg is a `cycles(n, cps)` tempo segment; the map loops over their total.
        "tempo" => {
            let segs = flatten_varargs(args)
                .into_iter()
                .map(|v| as_tempo_seg(v, span))
                .collect::<Result<Vec<(u32, f64)>>>()?;
            *ctx.tempo.borrow_mut() = TempoMap::from_segments(&segs);
            Ok(Value::Unit)
        }
        "trace" => log_fn(ctx, LogLevel::Trace, args, span),
        "debug" => log_fn(ctx, LogLevel::Debug, args, span),
        "info" => log_fn(ctx, LogLevel::Info, args, span),
        "warn" => log_fn(ctx, LogLevel::Warn, args, span),
        "error" => log_fn(ctx, LogLevel::Error, args, span),

        _ => Err(LangError::at(span, LangErrorKind::UnknownName(name.to_string()))),
    }
}

/// Append a clip to its scene (find-or-create by name), preserving first-seen
/// scene order. The clip is a single-channel `Track` named after its owning track,
/// so the launcher can substitute it into that track when the scene fires.
fn register_clip(ctx: &Rc<Ctx>, scene_name: &str, clip: Track<ControlMap>) {
    let mut scenes = ctx.scenes.borrow_mut();
    if let Some(s) = scenes.iter_mut().find(|s| s.name == scene_name) {
        s.clips.push(clip);
    } else {
        scenes.push(Scene { name: scene_name.to_string(), clips: vec![clip] });
    }
}

// ── Argument helpers ──────────────────────────────────────────────────────────

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

/// `par`/`seq`/`cat`/`arrange`/`tracks` accept varargs **or** a single list.
fn flatten_varargs(args: Vec<Value>) -> Vec<Value> {
    if args.len() == 1 {
        if let Value::List(items) = &args[0] {
            return items.clone();
        }
    }
    args
}

fn collect_patterns(args: Vec<Value>, span: SourceSpan) -> Result<Vec<Pattern<ControlMap>>> {
    flatten_varargs(args)
        .into_iter()
        .map(|v| as_pattern(v, span))
        .collect()
}

fn as_section(v: Value, span: SourceSpan) -> Result<Section<ControlMap>> {
    match v {
        Value::Section(s) => Ok(s),
        other => Err(type_err(span, "section (cycles(...))", &other)),
    }
}

fn as_track(v: Value, span: SourceSpan) -> Result<Track<ControlMap>> {
    match v {
        Value::Track(t) => Ok(t),
        other => Err(type_err(span, "track (track(...))", &other)),
    }
}

fn as_tempo_seg(v: Value, span: SourceSpan) -> Result<(u32, f64)> {
    match v {
        Value::TempoSeg { cycles, cps } => Ok((cycles, cps)),
        other => Err(type_err(span, "tempo segment (cycles(n, cps))", &other)),
    }
}

fn type_err(span: SourceSpan, expected: &str, got: &Value) -> LangError {
    LangError::at(
        span,
        LangErrorKind::Type {
            expected: expected.to_string(),
            got: got.type_name().to_string(),
        },
    )
}

// ── choose ────────────────────────────────────────────────────────────────────

fn eval_choose(args: Vec<Value>, span: SourceSpan) -> Result<Value> {
    if args.is_empty() {
        return Err(LangError::at(
            span,
            LangErrorKind::Arity {
                name: "choose".to_string(),
                expected: 1,
                got: 0,
            },
        ));
    }
    if args.iter().all(|v| matches!(v, Value::Number(_))) {
        let nums: Vec<f64> = args
            .iter()
            .map(|v| match v {
                Value::Number(n) => *n,
                _ => unreachable!(),
            })
            .collect();
        return Ok(Value::NumSignal(choose(nums)));
    }
    if args.iter().all(|v| matches!(v, Value::Pattern(_))) {
        let pats = args
            .into_iter()
            .map(|v| as_pattern(v, span))
            .collect::<Result<Vec<_>>>()?;
        return Ok(Value::Pattern(choose_patterns(pats)));
    }
    Err(type_err(span, "all numbers or all patterns", &Value::Unit))
}

/// Pick one of `pats` per query, seeded by the query midpoint (deterministic).
fn choose_patterns(pats: Vec<Pattern<ControlMap>>) -> Pattern<ControlMap> {
    Pattern::new(move |span: TimeSpan| -> Vec<Hap<ControlMap>> {
        if pats.is_empty() {
            return Vec::new();
        }
        let idx = time_to_index(span.midpoint(), SEED_CHOOSE_PATTERNS, pats.len());
        pats[idx].query(span)
    })
}

// ── Logging (eval-time) ───────────────────────────────────────────────────────

fn log_fn(ctx: &Rc<Ctx>, level: LogLevel, args: Vec<Value>, span: SourceSpan) -> Result<Value> {
    match args.len() {
        1 => {
            if ctx.log.enabled(level) {
                ctx.log.log(level, &display_value(&args[0]));
            }
            Ok(Value::Unit)
        }
        // `debug(label, x)` (and friends): log "label: x", return x unchanged.
        2 => {
            let label = display_value(&args[0]);
            let val = args.into_iter().nth(1).unwrap();
            if ctx.log.enabled(level) {
                ctx.log.log(level, &format!("{label}: {}", display_value(&val)));
            }
            Ok(val)
        }
        got => Err(LangError::at(
            span,
            LangErrorKind::Arity {
                name: level.as_str().to_string(),
                expected: 2,
                got,
            },
        )),
    }
}

fn display_value(v: &Value) -> String {
    match v {
        Value::Number(n) if n.fract() == 0.0 => format!("{}", *n as i64),
        Value::Number(n) => format!("{n}"),
        Value::Str(s) => s.clone(),
        Value::Level(l) => l.as_str().to_string(),
        other => other.type_name().to_string(),
    }
}
