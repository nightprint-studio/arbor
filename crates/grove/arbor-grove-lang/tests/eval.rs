//! Evaluator tests driven by hand-built ASTs (no parser needed).
//!
//! Mirrors Fase 1's exit criterion: a `.grove` AST → the right haps.

use std::rc::Rc;
use std::sync::Arc;

use arbor_grove_lang::ast::*;
use arbor_grove_lang::prelude::{evaluate, EvalConfig, NoImports, SilentLog};
use arbor_grove_pattern::prelude::{ControlMap, Hap, Pattern, Time, TimeSpan, Tracks};

// ── AST builders ──────────────────────────────────────────────────────────────

fn sp() -> arbor_grove_pattern::prelude::SourceSpan {
    arbor_grove_pattern::prelude::SourceSpan::new(0, 0)
}
fn e(kind: ExprKind) -> Expr {
    Expr { kind, span: sp() }
}
fn num(n: f64) -> Expr {
    e(ExprKind::Number(n))
}
fn string(s: &str) -> Expr {
    e(ExprKind::Str(s.to_string()))
}
fn var(name: &str) -> Expr {
    e(ExprKind::Var(name.to_string()))
}
fn note_lit(name: &str) -> Expr {
    e(ExprKind::Note(name.to_string()))
}
fn id(name: &str) -> Ident {
    Ident {
        name: name.to_string(),
        span: sp(),
    }
}
fn m(kind: MiniKind) -> Mini {
    Mini { kind, span: sp() }
}
fn term(atom: Mini, postfixes: Vec<Postfix>) -> Mini {
    m(MiniKind::Term {
        atom: Box::new(atom),
        postfixes,
    })
}
fn sound(name: &str) -> Mini {
    term(m(MiniKind::Leaf(Leaf::Sound(name.to_string()))), vec![])
}
fn note(name: &str) -> Mini {
    term(m(MiniKind::Leaf(Leaf::NoteName(name.to_string()))), vec![])
}
fn degree(d: i32) -> Mini {
    term(m(MiniKind::Leaf(Leaf::Degree(d))), vec![])
}
fn island(kind: IslandKind, root: Mini) -> Expr {
    e(ExprKind::Island(Island {
        kind,
        root,
        span: sp(),
    }))
}
fn s_island(root: Mini) -> Expr {
    island(IslandKind::Sound, root)
}
fn n_island(root: Mini) -> Expr {
    island(IslandKind::Note, root)
}
fn call(name: &str, args: Vec<Expr>) -> Expr {
    e(ExprKind::Call {
        name: id(name),
        args,
    })
}
fn method(recv: Expr, name: &str, args: Vec<Expr>) -> Expr {
    e(ExprKind::Method {
        recv: Box::new(recv),
        name: id(name),
        args,
    })
}

// ── Harness ───────────────────────────────────────────────────────────────────

fn run(items: Vec<Item>) -> arbor_grove_lang::prelude::Result<Tracks<ControlMap>> {
    evaluate(
        &Program { items },
        Rc::new(NoImports),
        Arc::new(SilentLog),
        EvalConfig::default(),
    )
    .map(|o| o.tracks)
}

fn run_expr(expr: Expr) -> Pattern<ControlMap> {
    let tracks = run(vec![Item::Expr(expr)]).expect("eval ok");
    tracks.tracks[0].pattern.clone()
}

fn onsets(p: &Pattern<ControlMap>, cyc: i64) -> Vec<Hap<ControlMap>> {
    let mut haps: Vec<_> = p
        .query(TimeSpan::cycle(cyc))
        .into_iter()
        .filter(|h| h.has_onset())
        .collect();
    haps.sort_by_key(|h| h.part.begin);
    haps
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn sound_sequence_splits_the_cycle() {
    let p = run_expr(s_island(m(MiniKind::Sequence(vec![sound("bd"), sound("sd")]))));
    let h = onsets(&p, 0);
    assert_eq!(h.len(), 2);
    assert_eq!(h[0].value.sound.as_deref(), Some("bd"));
    assert_eq!(h[1].value.sound.as_deref(), Some("sd"));
    assert_eq!(h[0].whole.unwrap(), TimeSpan::new(Time::ZERO, Time::new(1, 2)));
    assert!(h[0].span.is_some(), "leaf hap should carry a source span");
}

#[test]
fn note_names_resolve_to_midi() {
    let p = run_expr(n_island(m(MiniKind::Sequence(vec![note("c4"), note("e4")]))));
    let h = onsets(&p, 0);
    assert_eq!(h[0].value.note, Some(60.0));
    assert_eq!(h[1].value.note, Some(64.0));
}

#[test]
fn host_note_literal_is_a_single_note_pattern() {
    // A host pitch literal (`ef4`, octave mandatory) evaluates to a one-note
    // pattern: Eb4 = MIDI 63.
    let p = run_expr(note_lit("ef4"));
    let h = onsets(&p, 0);
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].value.note, Some(63.0));
    assert!(h[0].span.is_some(), "note literal hap should carry a source span");
}

#[test]
fn choose_over_host_note_literals() {
    // `choose(c4, ef4, g4)` — now expressible thanks to host note literals.
    // Each cycle picks exactly one of the three pitches (all-pattern path).
    let p = run_expr(call(
        "choose",
        vec![note_lit("c4"), note_lit("ef4"), note_lit("g4")],
    ));
    for cyc in 0..6 {
        let h = onsets(&p, cyc);
        assert_eq!(h.len(), 1, "exactly one note per cycle");
        let n = h[0].value.note.expect("a concrete pitch");
        assert!([60.0, 63.0, 67.0].contains(&n), "unexpected pitch {n}");
    }
}

#[test]
fn parallel_stacks_voices() {
    let p = run_expr(n_island(m(MiniKind::Parallel(vec![note("c4"), note("e4")]))));
    let h = onsets(&p, 0);
    assert_eq!(h.len(), 2);
    let notes: Vec<_> = h.iter().filter_map(|x| x.value.note).collect();
    assert!(notes.contains(&60.0) && notes.contains(&64.0));
    // both span the whole cycle
    assert!(h.iter().all(|x| x.whole.unwrap() == TimeSpan::cycle(0)));
}

#[test]
fn degrees_need_scale_then_resolve() {
    // n(0 2).scale("c:minor") → C4, Eb4
    let p = run_expr(method(
        n_island(m(MiniKind::Sequence(vec![degree(0), degree(2)]))),
        "scale",
        vec![string("c:minor")],
    ));
    let h = onsets(&p, 0);
    assert_eq!(h[0].value.note, Some(60.0));
    assert_eq!(h[1].value.note, Some(63.0));
}

#[test]
fn weight_postfix_lengthens_a_slot() {
    // s(bd@3 sd) → bd over [0,3/4), sd over [3/4,1)
    let p = run_expr(s_island(m(MiniKind::Sequence(vec![
        term(m(MiniKind::Leaf(Leaf::Sound("bd".into()))), vec![Postfix::Weight(3)]),
        sound("sd"),
    ]))));
    let h = onsets(&p, 0);
    assert_eq!(h[0].whole.unwrap(), TimeSpan::new(Time::ZERO, Time::new(3, 4)));
    assert_eq!(h[1].whole.unwrap(), TimeSpan::new(Time::new(3, 4), Time::ONE));
}

#[test]
fn underscore_extends_previous_term() {
    // s(bd _ sd) → bd over [0,1/2), sd over [2/3? no: 3 slots, bd weight2]
    // bd gets +1 from `_` → weights [2,1] → bd [0,2/3), sd [2/3,1)
    let p = run_expr(s_island(m(MiniKind::Sequence(vec![
        sound("bd"),
        term(m(MiniKind::Extend), vec![]),
        sound("sd"),
    ]))));
    let h = onsets(&p, 0);
    assert_eq!(h.len(), 2);
    assert_eq!(h[0].value.sound.as_deref(), Some("bd"));
    assert_eq!(h[0].whole.unwrap(), TimeSpan::new(Time::ZERO, Time::new(2, 3)));
    assert_eq!(h[1].whole.unwrap(), TimeSpan::new(Time::new(2, 3), Time::ONE));
}

#[test]
fn euclid_postfix_distributes_onsets() {
    // s(bd(3,8)) → onsets at 0, 3/8, 6/8
    let p = run_expr(s_island(term(
        m(MiniKind::Leaf(Leaf::Sound("bd".into()))),
        vec![Postfix::Euclid {
            pulses: 3,
            steps: 8,
            rotation: None,
        }],
    )));
    let starts: Vec<_> = onsets(&p, 0).iter().map(|h| h.whole.unwrap().begin).collect();
    assert_eq!(starts, vec![Time::ZERO, Time::new(3, 8), Time::new(6, 8)]);
}

#[test]
fn fast_postfix_repeats_inside_the_slot() {
    // s(bd*2) → two bd in the cycle
    let p = run_expr(s_island(term(
        m(MiniKind::Leaf(Leaf::Sound("bd".into()))),
        vec![Postfix::Fast(2.0)],
    )));
    assert_eq!(onsets(&p, 0).len(), 2);
}

#[test]
fn chord_postfix_expands_into_a_stack() {
    // n(c4'min7) → 4 notes: 60, 63, 67, 70
    let p = run_expr(n_island(term(
        m(MiniKind::Leaf(Leaf::NoteName("c4".into()))),
        vec![Postfix::Chord("min7".into())],
    )));
    let mut notes: Vec<_> = onsets(&p, 0).iter().filter_map(|h| h.value.note).collect();
    notes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(notes, vec![60.0, 63.0, 67.0, 70.0]);
}

#[test]
fn gain_method_sets_the_control() {
    let p = run_expr(method(s_island(sound("bd")), "gain", vec![num(0.5)]));
    let h = onsets(&p, 0);
    assert_eq!(h[0].value.gain, Some(0.5));
}

#[test]
fn rev_method_reverses_within_a_cycle() {
    let p = run_expr(method(
        s_island(m(MiniKind::Sequence(vec![sound("bd"), sound("sd"), sound("hh")]))),
        "rev",
        vec![],
    ));
    let names: Vec<_> = onsets(&p, 0)
        .iter()
        .map(|h| h.value.sound.clone().unwrap())
        .collect();
    assert_eq!(names, vec!["hh", "sd", "bd"]);
}

#[test]
fn every_applies_a_transform_on_matching_cycles() {
    // s(bd sd).every(2, rev) → cycle 0 reversed, cycle 1 normal
    let base = s_island(m(MiniKind::Sequence(vec![sound("bd"), sound("sd")])));
    let p = run_expr(method(base, "every", vec![num(2.0), var("rev")]));
    let c0: Vec<_> = onsets(&p, 0)
        .iter()
        .map(|h| h.value.sound.clone().unwrap())
        .collect();
    let c1: Vec<_> = onsets(&p, 1)
        .iter()
        .map(|h| h.value.sound.clone().unwrap())
        .collect();
    assert_eq!(c0, vec!["sd", "bd"]);
    assert_eq!(c1, vec!["bd", "sd"]);
}

#[test]
fn let_binding_and_splice() {
    // let motif = n(e4)
    // n(c4 $motif) → c4 over [0,1/2), e4 over [1/2,1)
    let items = vec![
        Item::Let(LetBind {
            name: id("motif"),
            value: n_island(note("e4")),
            span: sp(),
        }),
        Item::Expr(n_island(m(MiniKind::Sequence(vec![
            note("c4"),
            term(m(MiniKind::Splice(id("motif"))), vec![]),
        ])))),
    ];
    let tracks = run(items).expect("eval ok");
    let p = tracks.tracks[0].pattern.clone();
    let h = onsets(&p, 0);
    assert_eq!(h.len(), 2);
    assert_eq!(h[0].value.note, Some(60.0));
    assert_eq!(h[0].whole.unwrap(), TimeSpan::new(Time::ZERO, Time::new(1, 2)));
    assert_eq!(h[1].value.note, Some(64.0));
}

#[test]
fn lambda_over_range_with_par() {
    // (0..3).par(i => n($i).scale("c:major"))  — three degrees stacked
    let body = method(
        n_island(term(m(MiniKind::Splice(id("i"))), vec![])),
        "scale",
        vec![string("c:major")],
    );
    let lambda = e(ExprKind::Lambda {
        params: vec![id("i")],
        body: Box::new(body),
    });
    let range = e(ExprKind::Range {
        lo: Box::new(num(0.0)),
        hi: Box::new(num(3.0)),
        inclusive: false,
    });
    let p = run_expr(method(range, "par", vec![lambda]));
    let h = onsets(&p, 0);
    // degrees 0,1,2 of C major → 60, 62, 64, all at cycle start (stacked)
    let mut notes: Vec<_> = h.iter().filter_map(|x| x.value.note).collect();
    notes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(notes, vec![60.0, 62.0, 64.0]);
}

#[test]
fn tracks_output_keeps_channels() {
    let out = run(vec![Item::Expr(call(
        "tracks",
        vec![
            call("track", vec![string("drums"), s_island(sound("bd"))]),
            call("track", vec![string("bass"), n_island(note("c2"))]),
        ],
    ))])
    .expect("eval ok");
    assert_eq!(out.tracks.len(), 2);
    assert_eq!(out.tracks[0].name, "drums");
    assert_eq!(out.tracks[1].name, "bass");
}

#[test]
fn cps_is_captured() {
    let out = evaluate(
        &Program {
            items: vec![
                Item::Expr(call("cps", vec![num(0.5)])),
                Item::Expr(s_island(sound("bd"))),
            ],
        },
        Rc::new(NoImports),
        Arc::new(SilentLog),
        EvalConfig::default(),
    )
    .expect("eval ok");
    assert_eq!(out.cps, Some(0.5));
    assert_eq!(out.tracks.tracks.len(), 1);
}

#[test]
fn recursion_is_rejected() {
    // fn f() = f()
    let items = vec![
        Item::Fn(FnDef {
            name: id("f"),
            params: vec![],
            body: call("f", vec![]),
            span: sp(),
        }),
        Item::Expr(s_island(sound("bd"))),
    ];
    assert!(run(items).is_err(), "direct recursion must be rejected");
}

#[test]
fn variant_in_note_island_is_a_context_error() {
    // n(c4:2) — `:n` is sound-only
    let bad = n_island(term(
        m(MiniKind::Leaf(Leaf::NoteName("c4".into()))),
        vec![Postfix::Variant(2)],
    ));
    assert!(run(vec![Item::Expr(bad)]).is_err());
}

#[test]
fn unknown_name_is_an_error() {
    assert!(run(vec![Item::Expr(var("nope"))]).is_err());
}
