//! Evaluated haps → mini-notation AST: the base of *materialisation*.
//!
//! `design/nemus/editing-model.md §3`: the future editor "freezes" a generative
//! sub-tree by evaluating it to concrete events and re-emitting them as a
//! literal. This module is the **value → AST** half (the AST → text half is
//! [`crate::emit`]); together they turn one cycle of haps into a ready-to-splice
//! `s(...)` / `n(...)` literal, proving the design's claim that mini-notation can
//! express any timed set of events.
//!
//! ## Base scope (intentional limits)
//!
//! - **One cycle** `[0, 1)`: the caller queries a single cycle and passes the
//!   haps. Events are assumed to start and end within it.
//! - **Discrete events only**: a hap is kept when it has an onset in the query
//!   (`whole.begin == part.begin`); continuous signals (`rand`, `choose`, which
//!   have `whole = None`) are skipped.
//! - **Overlap → parallel lanes**: chords and layered voices are split into `&`
//!   lanes by greedy interval partition, so each lane is monophonic.
//! - **Uniform grid**: every onset/offset lands on a `1/N` boundary (`N` = the
//!   LCM of the slot denominators); `~` fills gaps and `@` carries multi-slot
//!   durations.
//! - **Pitches** are rounded to the nearest semitone (`note` is `f64`).
//!
//! Richer renders (chord-name recovery, `<>` alternation across cycles, nested
//! `[]` factoring) are left to the full materialisation feature.

use arbor_nemus_pattern::prelude::{ControlMap, Hap, SourceSpan, Time};

use crate::ast::{Expr, ExprKind, Island, IslandKind, Leaf, Mini, MiniKind, Postfix};
use crate::emit::emit_expr;

/// Build a mini-notation [`Island`] AST from one cycle of evaluated haps.
pub fn materialize_island(kind: IslandKind, haps: &[Hap<ControlMap>]) -> Island {
    let events = collect_events(haps);
    let root = if events.is_empty() {
        mini(MiniKind::Rest)
    } else {
        let lanes = partition_lanes(events);
        if lanes.len() == 1 {
            lane_to_mini(kind, &lanes[0])
        } else {
            mini(MiniKind::Parallel(
                lanes.iter().map(|l| lane_to_mini(kind, l)).collect(),
            ))
        }
    };
    Island {
        kind,
        root,
        span: syn(),
    }
}

/// Build the island AST and print it to canonical source (a one-liner like
/// `n(c4 e4 g4)`), the unit a caller splices back into a `.nemus` file.
pub fn materialize_source(kind: IslandKind, haps: &[Hap<ControlMap>]) -> String {
    let island = materialize_island(kind, haps);
    emit_expr(&Expr {
        kind: ExprKind::Island(island),
        span: syn(),
    })
}

// ── Events ────────────────────────────────────────────────────────────────────

/// One discrete event: when it sounds and what it carries.
struct Event {
    begin: Time,
    end: Time,
    value: ControlMap,
}

/// Keep the discrete, onset-bearing haps and order them in time.
fn collect_events(haps: &[Hap<ControlMap>]) -> Vec<Event> {
    let mut events: Vec<Event> = haps
        .iter()
        .filter_map(|h| {
            let whole = h.whole?;
            // Drop tail fragments: only the slice that caught the onset counts.
            if h.part.begin != whole.begin {
                return None;
            }
            Some(Event {
                begin: whole.begin,
                end: whole.end,
                value: h.value.clone(),
            })
        })
        .collect();
    events.sort_by(|a, b| a.begin.cmp(&b.begin).then(a.end.cmp(&b.end)));
    events
}

/// Greedy interval partition: each event goes into the first lane whose last
/// event has already ended, otherwise it opens a new lane. With events pre-sorted
/// by onset this keeps every lane monophonic and the result deterministic.
fn partition_lanes(events: Vec<Event>) -> Vec<Vec<Event>> {
    let mut lanes: Vec<Vec<Event>> = Vec::new();
    for ev in events {
        let slot = lanes
            .iter()
            .position(|lane| lane.last().map_or(true, |last| last.end <= ev.begin));
        match slot {
            Some(i) => lanes[i].push(ev),
            None => lanes.push(vec![ev]),
        }
    }
    lanes
}

// ── One lane → an equal-slot sequence ─────────────────────────────────────────

fn lane_to_mini(kind: IslandKind, lane: &[Event]) -> Mini {
    let n = grid_size(lane);
    let mut terms: Vec<Mini> = Vec::new();
    let mut pos: i64 = 0;
    let mut next = 0; // index of the next not-yet-emitted event
    while pos < n {
        if next < lane.len() && slot_of(lane[next].begin, n) == pos {
            let ev = &lane[next];
            let weight = (slot_of(ev.end, n) - pos).max(1);
            terms.push(leaf_term(kind, &ev.value, weight as u32));
            pos += weight;
            next += 1;
        } else {
            let until = if next < lane.len() {
                slot_of(lane[next].begin, n)
            } else {
                n
            };
            let gap = (until - pos).max(1);
            terms.push(rest_term(gap as u32));
            pos += gap;
        }
    }
    if terms.len() == 1 {
        terms.pop().unwrap()
    } else {
        mini(MiniKind::Sequence(terms))
    }
}

/// Number of equal slots needed so every event boundary lands on a `1/N` grid.
fn grid_size(lane: &[Event]) -> i64 {
    let mut n = 1;
    for ev in lane {
        n = lcm(n, ev.begin.den());
        n = lcm(n, ev.end.den());
    }
    n
}

/// Slot index of a grid-aligned time: `t * n` is an integer by construction.
fn slot_of(t: Time, n: i64) -> i64 {
    (t * Time::int(n)).num()
}

/// A leaf occupying `weight` slots: `bd`, `bd@2`, `cp:3`, `cp:3@2`.
fn leaf_term(kind: IslandKind, value: &ControlMap, weight: u32) -> Mini {
    let mut postfixes = Vec::new();
    let atom = leaf_atom(kind, value, &mut postfixes);
    if weight > 1 {
        postfixes.push(Postfix::Weight(weight));
    }
    if postfixes.is_empty() {
        atom
    } else {
        mini(MiniKind::Term {
            atom: Box::new(atom),
            postfixes,
        })
    }
}

/// The atom for an event's value, kind-aware. May push a `:variant` postfix.
fn leaf_atom(kind: IslandKind, c: &ControlMap, postfixes: &mut Vec<Postfix>) -> Mini {
    match kind {
        IslandKind::Sound => match &c.sound {
            Some(name) => {
                if let Some(v) = c.variant {
                    postfixes.push(Postfix::Variant(v));
                }
                mini(MiniKind::Leaf(Leaf::Sound(name.clone())))
            }
            None => mini(MiniKind::Rest),
        },
        IslandKind::Note => {
            if let Some(midi) = c.note {
                mini(MiniKind::Leaf(Leaf::NoteName(midi_to_name(midi))))
            } else if let Some(degree) = c.degree {
                mini(MiniKind::Leaf(Leaf::Degree(degree)))
            } else {
                mini(MiniKind::Rest)
            }
        }
    }
}

/// A rest occupying `weight` slots: `~` or `~@2`.
fn rest_term(weight: u32) -> Mini {
    if weight > 1 {
        mini(MiniKind::Term {
            atom: Box::new(mini(MiniKind::Rest)),
            postfixes: vec![Postfix::Weight(weight)],
        })
    } else {
        mini(MiniKind::Rest)
    }
}

/// MIDI semitone → a nemus note name (`60 → c4`, `63 → ef4`). Canonical spelling
/// per pitch class (sharps for C/F, flats elsewhere); the octave digit is always
/// written so the value is unambiguous regardless of the default octave.
fn midi_to_name(midi: f64) -> String {
    const NAMES: [&str; 12] = [
        "c", "cs", "d", "ef", "e", "f", "fs", "g", "af", "a", "bf", "b",
    ];
    let m = midi.round() as i32;
    let pc = m.rem_euclid(12) as usize;
    let octave = m.div_euclid(12) - 1;
    format!("{}{}", NAMES[pc], octave)
}

// ── Small numerics ────────────────────────────────────────────────────────────

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn lcm(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 {
        a.max(b)
    } else {
        a / gcd(a, b) * b
    }
}

// ── AST helpers ───────────────────────────────────────────────────────────────

/// Synthetic span for materialised nodes — they have no origin in source text;
/// a fresh span is assigned when the printed literal is re-parsed.
fn syn() -> SourceSpan {
    SourceSpan::new(0, 0)
}

fn mini(kind: MiniKind) -> Mini {
    Mini { kind, span: syn() }
}
