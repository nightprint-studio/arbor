//! Mini-notation evaluation: an [`Island`] AST → `Pattern<ControlMap>`.
//!
//! Structural operators map onto the pattern crate: space → `timecat`
//! (weighted, so `@`/`_` work), `&` → `stack`, `<>` → `slowcat`, `*`/`/` →
//! `fast`/`slow`, `(n,k)` → `euclid`. `!` (replicate) and `'chord` (chord
//! expansion) are AST-level expansions; the chord table lives in [`chords`].
//! Every leaf is `tag_span`-stamped so each hap points back at its source.

use std::rc::Rc;

use arbor_grove_pattern::prelude::{
    parse_note, pure, silence, slowcat, stack, timecat, ControlMap, Hap, Pattern, SourceSpan,
    TimeSpan,
};

use crate::ast::{Island, IslandKind, Leaf, Mini, MiniKind, Postfix};
use crate::convert::f64_to_time;
use crate::env::Env;
use crate::error::{LangError, LangErrorKind, Result};
use crate::eval::chords::chord_intervals;
use crate::eval::{resolve, Ctx};

/// A sequence element after evaluation: its pattern plus how it occupies slots.
struct TermEval {
    pattern: Pattern<ControlMap>,
    weight: u32,
    replicate: u32,
}

/// Evaluate a whole island. The result is span-tagged with the island span as a
/// fallback (leaf spans, set deeper, win).
pub fn eval_island(ctx: &Rc<Ctx>, env: &Env, island: &Island) -> Result<Pattern<ControlMap>> {
    let pat = eval_mini(ctx, env, island.kind, &island.root)?;
    Ok(pat.tag_span(island.span))
}

fn eval_mini(
    ctx: &Rc<Ctx>,
    env: &Env,
    kind: IslandKind,
    mini: &Mini,
) -> Result<Pattern<ControlMap>> {
    match &mini.kind {
        MiniKind::Parallel(lanes) => {
            let pats = lanes
                .iter()
                .map(|m| eval_mini(ctx, env, kind, m))
                .collect::<Result<Vec<_>>>()?;
            Ok(stack(pats))
        }
        MiniKind::Sequence(items) => build_sequence(ctx, env, kind, items),
        MiniKind::Term { atom, postfixes } => {
            let te = eval_term(ctx, env, kind, atom, postfixes, mini.span)?;
            if te.replicate > 1 {
                let slots = (0..te.replicate).map(|_| (1, te.pattern.clone())).collect();
                Ok(timecat(slots))
            } else {
                Ok(te.pattern)
            }
        }
        MiniKind::Group(inner) => eval_mini(ctx, env, kind, inner),
        MiniKind::Alt(inner) => eval_alt(ctx, env, kind, inner),
        MiniKind::Rest => Ok(silence()),
        MiniKind::Extend => Ok(silence()), // a lone `_`; in a sequence it merges left
        MiniKind::Splice(ident) => eval_splice(ctx, env, kind, &ident.name, mini.span),
        MiniKind::Leaf(leaf) => build_leaf(ctx, kind, leaf, mini.span),
    }
}

/// `< ... >` — one element per cycle (slowcat over the alternatives).
fn eval_alt(ctx: &Rc<Ctx>, env: &Env, kind: IslandKind, inner: &Mini) -> Result<Pattern<ControlMap>> {
    let alts: Vec<&Mini> = match &inner.kind {
        MiniKind::Sequence(items) => items.iter().collect(),
        MiniKind::Parallel(lanes) => lanes.iter().collect(),
        _ => vec![inner],
    };
    let pats = alts
        .into_iter()
        .map(|m| eval_mini(ctx, env, kind, m))
        .collect::<Result<Vec<_>>>()?;
    Ok(slowcat(pats))
}

/// Build a space-separated sequence into a weighted `timecat`, handling `_`
/// (extend the previous slot), `!n` (replicate into separate slots) and `@n`
/// (weight).
fn build_sequence(
    ctx: &Rc<Ctx>,
    env: &Env,
    kind: IslandKind,
    items: &[Mini],
) -> Result<Pattern<ControlMap>> {
    let mut slots: Vec<(u32, Pattern<ControlMap>)> = Vec::new();
    for item in items {
        if is_extend(item) {
            match slots.last_mut() {
                Some(last) => last.0 += 1,
                None => slots.push((1, silence())),
            }
            continue;
        }
        let te = eval_seq_item(ctx, env, kind, item)?;
        for _ in 0..te.replicate {
            slots.push((te.weight, te.pattern.clone()));
        }
    }
    Ok(match slots.len() {
        0 => silence(),
        1 if slots[0].0 == 1 => slots.pop().unwrap().1,
        _ => timecat(slots),
    })
}

/// Is this sequence element a `_` extension?
fn is_extend(item: &Mini) -> bool {
    match &item.kind {
        MiniKind::Extend => true,
        MiniKind::Term { atom, .. } => matches!(atom.kind, MiniKind::Extend),
        _ => false,
    }
}

fn eval_seq_item(ctx: &Rc<Ctx>, env: &Env, kind: IslandKind, item: &Mini) -> Result<TermEval> {
    match &item.kind {
        MiniKind::Term { atom, postfixes } => {
            eval_term(ctx, env, kind, atom, postfixes, item.span)
        }
        _ => Ok(TermEval {
            pattern: eval_mini(ctx, env, kind, item)?,
            weight: 1,
            replicate: 1,
        }),
    }
}

/// An atom plus its postfix chain, applied left to right.
fn eval_term(
    ctx: &Rc<Ctx>,
    env: &Env,
    kind: IslandKind,
    atom: &Mini,
    postfixes: &[Postfix],
    span: SourceSpan,
) -> Result<TermEval> {
    let mut pattern = eval_mini(ctx, env, kind, atom)?;
    let mut weight = 1u32;
    let mut replicate = 1u32;
    for pf in postfixes {
        match pf {
            Postfix::Fast(n) => pattern = pattern.fast(f64_to_time(*n)),
            Postfix::Slow(n) => pattern = pattern.slow(f64_to_time(*n)),
            Postfix::Euclid {
                pulses,
                steps,
                rotation,
            } => pattern = pattern.euclid(*pulses, *steps, (*rotation).unwrap_or(0)),
            Postfix::Variant(v) => {
                if kind != IslandKind::Sound {
                    return Err(context(span, "`:n` (sample variant) is only valid in s()/sound()"));
                }
                let v = *v;
                pattern = pattern.fmap(move |mut c| {
                    c.variant = Some(v);
                    c
                });
            }
            Postfix::Chord(name) => {
                if kind != IslandKind::Note {
                    return Err(context(span, "`'chord` is only valid in n()/note()"));
                }
                pattern = chord_expand(pattern, name, span)?;
            }
            Postfix::Replicate(r) => replicate = *r,
            Postfix::Weight(w) => weight = *w,
        }
    }
    Ok(TermEval {
        pattern,
        weight,
        replicate,
    })
}

/// Build a leaf into a single-event pattern, validated against the island kind.
fn build_leaf(
    ctx: &Rc<Ctx>,
    kind: IslandKind,
    leaf: &Leaf,
    span: SourceSpan,
) -> Result<Pattern<ControlMap>> {
    let pat = match (kind, leaf) {
        (IslandKind::Sound, Leaf::Sound(name)) => pure(ControlMap::sound(name.clone())),
        (IslandKind::Note, Leaf::NoteName(name)) => {
            let midi = parse_note(name, ctx.config.default_octave)
                .map_err(|e| LangError::at(span, LangErrorKind::Pitch(e)))?;
            pure(ControlMap::note(midi))
        }
        (IslandKind::Note, Leaf::Degree(d)) => pure(ControlMap::degree(*d)),
        (IslandKind::Sound, _) => {
            return Err(context(span, "a note/degree leaf is not valid in s()/sound()"))
        }
        (IslandKind::Note, Leaf::Sound(_)) => {
            return Err(context(span, "a sound-name leaf is not valid in n()/note()"))
        }
    };
    Ok(pat.tag_span(span))
}

/// `$ident` — splice a host value as a leaf.
fn eval_splice(
    ctx: &Rc<Ctx>,
    env: &Env,
    kind: IslandKind,
    name: &str,
    span: SourceSpan,
) -> Result<Pattern<ControlMap>> {
    use crate::value::Value;
    match resolve(ctx, env, name) {
        Some(Value::Pattern(p)) => Ok(p.tag_span(span)),
        Some(Value::Number(n)) => {
            if kind == IslandKind::Note {
                Ok(pure(ControlMap::degree(n as i32)).tag_span(span))
            } else {
                Err(context(span, "cannot splice a number into s()/sound()"))
            }
        }
        Some(other) => Err(LangError::at(
            span,
            LangErrorKind::Type {
                expected: "pattern or number".to_string(),
                got: other.type_name().to_string(),
            },
        )),
        None => Err(LangError::at(span, LangErrorKind::UnknownName(name.to_string()))),
    }
}

/// Expand each note into a chord (semitone offsets); haps without a concrete
/// note (e.g. unresolved degrees) pass through unchanged.
fn chord_expand(
    pattern: Pattern<ControlMap>,
    name: &str,
    span: SourceSpan,
) -> Result<Pattern<ControlMap>> {
    let intervals = chord_intervals(name)
        .ok_or_else(|| context(span, "unknown chord name"))?;
    Ok(Pattern::new(move |sp: TimeSpan| {
        let mut out: Vec<Hap<ControlMap>> = Vec::new();
        for h in pattern.query(sp) {
            match h.value.note {
                Some(base) => {
                    for iv in intervals {
                        let mut c = h.value.clone();
                        c.note = Some(base + *iv as f64);
                        out.push(Hap {
                            whole: h.whole,
                            part: h.part,
                            value: c,
                            span: h.span,
                        });
                    }
                }
                None => out.push(h),
            }
        }
        out
    }))
}

fn context(span: SourceSpan, msg: &str) -> LangError {
    LangError::at(span, LangErrorKind::Context(msg.to_string()))
}
