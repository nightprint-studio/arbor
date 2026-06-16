//! Emitter golden-string tests, driven by hand-built ASTs (no parser needed).
//!
//! Asserts the **canonical** `.nemus` text for each node shape. The full
//! round-trip (`parse(emit(x)) == x`) arrives with the Tree-sitter front end;
//! here we pin the exact format the parser will have to read back.

use arbor_nemus_lang::ast::*;
use arbor_nemus_lang::prelude::{emit, emit_expr};

// ── AST builders ──────────────────────────────────────────────────────────────

fn sp() -> arbor_nemus_pattern::prelude::SourceSpan {
    arbor_nemus_pattern::prelude::SourceSpan::new(0, 0)
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
fn bin(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    e(ExprKind::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    })
}
fn neg(x: Expr) -> Expr {
    e(ExprKind::Unary {
        op: UnOp::Neg,
        rhs: Box::new(x),
    })
}
fn range(lo: Expr, hi: Expr, inclusive: bool) -> Expr {
    e(ExprKind::Range {
        lo: Box::new(lo),
        hi: Box::new(hi),
        inclusive,
    })
}
fn lambda(params: &[&str], body: Expr) -> Expr {
    e(ExprKind::Lambda {
        params: params.iter().map(|p| id(p)).collect(),
        body: Box::new(body),
    })
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
    m(MiniKind::Leaf(Leaf::Sound(name.to_string())))
}
fn note(name: &str) -> Mini {
    m(MiniKind::Leaf(Leaf::NoteName(name.to_string())))
}
fn s_island(root: Mini) -> Expr {
    e(ExprKind::Island(Island {
        kind: IslandKind::Sound,
        root,
        span: sp(),
    }))
}
fn n_island(root: Mini) -> Expr {
    e(ExprKind::Island(Island {
        kind: IslandKind::Note,
        root,
        span: sp(),
    }))
}

// ── Numbers ───────────────────────────────────────────────────────────────────

#[test]
fn numbers_are_shortest_round_trip() {
    assert_eq!(emit_expr(&num(4.0)), "4");
    assert_eq!(emit_expr(&num(0.5)), "0.5");
    assert_eq!(emit_expr(&num(0.125)), "0.125");
    assert_eq!(emit_expr(&num(800.0)), "800");
}

// ── Host note literals ────────────────────────────────────────────────────────

#[test]
fn host_note_literals_emit_bare() {
    // A host pitch literal prints as the bare token — no `n(...)` wrapper.
    assert_eq!(emit_expr(&note_lit("c4")), "c4");
    assert_eq!(emit_expr(&note_lit("ef3")), "ef3");
    // choose(c4, ef4, g4) — the canonical example from transforms.md.
    let expr = call("choose", vec![note_lit("c4"), note_lit("ef4"), note_lit("g4")]);
    assert_eq!(emit_expr(&expr), "choose(c4, ef4, g4)");
}

// ── Arithmetic: tight, minimal parens ─────────────────────────────────────────

#[test]
fn arithmetic_is_tight() {
    // i*0.1
    assert_eq!(emit_expr(&bin(BinOp::Mul, var("i"), num(0.1))), "i*0.1");
    // a-1
    assert_eq!(emit_expr(&bin(BinOp::Sub, var("a"), num(1.0))), "a-1");
}

#[test]
fn unary_neg_is_tight() {
    assert_eq!(emit_expr(&neg(var("x"))), "-x");
    assert_eq!(emit_expr(&neg(num(2.0))), "-2");
}

#[test]
fn parens_are_minimal_but_correct() {
    // (a+b)*c — left child looser than parent
    let inner = bin(BinOp::Add, var("a"), var("b"));
    assert_eq!(
        emit_expr(&bin(BinOp::Mul, inner, var("c"))),
        "(a+b)*c"
    );
    // a-(b-c) — right child same precedence, left-assoc
    let rhs = bin(BinOp::Sub, var("b"), var("c"));
    assert_eq!(emit_expr(&bin(BinOp::Sub, var("a"), rhs)), "a-(b-c)");
    // a/(b*c) — right child same precedence
    let rhs = bin(BinOp::Mul, var("b"), var("c"));
    assert_eq!(emit_expr(&bin(BinOp::Div, var("a"), rhs)), "a/(b*c)");
    // a-b+c stays flat (Add over a left Sub)
    let lhs = bin(BinOp::Sub, var("a"), var("b"));
    assert_eq!(emit_expr(&bin(BinOp::Add, lhs, var("c"))), "a-b+c");
}

// ── Ranges & lambdas ──────────────────────────────────────────────────────────

#[test]
fn ranges_are_tight() {
    assert_eq!(emit_expr(&range(num(0.0), num(8.0), false)), "0..8");
    assert_eq!(emit_expr(&range(num(0.0), num(7.0), true)), "0..=7");
}

#[test]
fn range_receiver_gets_parens_before_method() {
    // (0..8).par(i => n($i))
    let r = range(num(0.0), num(8.0), false);
    let body = n_island(m(MiniKind::Splice(id("i"))));
    let expr = method(r, "par", vec![lambda(&["i"], body)]);
    assert_eq!(emit_expr(&expr), "(0..8).par(i => n($i))");
}

#[test]
fn lambda_forms() {
    assert_eq!(emit_expr(&lambda(&["i"], var("i"))), "i => i");
    assert_eq!(
        emit_expr(&lambda(&["i", "j"], var("i"))),
        "(i, j) => i"
    );
}

// ── Method chains stay inline ─────────────────────────────────────────────────

#[test]
fn method_chain_is_inline() {
    // n(c2 g1).inst("synth.bass")
    let isl = n_island(m(MiniKind::Sequence(vec![note("c2"), note("g1")])));
    let expr = method(isl, "inst", vec![string("synth.bass")]);
    assert_eq!(emit_expr(&expr), r#"n(c2 g1).inst("synth.bass")"#);
}

// ── Islands: structure & postfixes ────────────────────────────────────────────

#[test]
fn island_sequence_with_rest() {
    // s(bd ~ sd ~)
    let root = m(MiniKind::Sequence(vec![
        sound("bd"),
        m(MiniKind::Rest),
        sound("sd"),
        m(MiniKind::Rest),
    ]));
    assert_eq!(emit_expr(&s_island(root)), "s(bd ~ sd ~)");
}

#[test]
fn island_postfixes() {
    // s(cp:2(3,8))
    let cp = term(
        sound("cp"),
        vec![
            Postfix::Variant(2),
            Postfix::Euclid {
                pulses: MiniArg::Const(3.0),
                steps: MiniArg::Const(8.0),
                rotation: None,
            },
        ],
    );
    assert_eq!(emit_expr(&s_island(cp)), "s(cp:2(3,8))");

    // s(bd*2 sd!3)
    let seq = m(MiniKind::Sequence(vec![
        term(sound("bd"), vec![Postfix::Fast(MiniArg::Const(2.0))]),
        term(sound("sd"), vec![Postfix::Replicate(3)]),
    ]));
    assert_eq!(emit_expr(&s_island(seq)), "s(bd*2 sd!3)");

    // n(c4'min7)
    let chord = term(note("c4"), vec![Postfix::Chord("min7".to_string())]);
    assert_eq!(emit_expr(&n_island(chord)), "n(c4'min7)");
}

#[test]
fn island_weight() {
    // s(a@2 b)
    let seq = m(MiniKind::Sequence(vec![
        term(sound("a"), vec![Postfix::Weight(2.0)]),
        sound("b"),
    ]));
    assert_eq!(emit_expr(&s_island(seq)), "s(a@2 b)");
}

#[test]
fn island_parallel_and_group() {
    // s(bd ~ sd ~ & hh*8)
    let lane1 = m(MiniKind::Sequence(vec![
        sound("bd"),
        m(MiniKind::Rest),
        sound("sd"),
        m(MiniKind::Rest),
    ]));
    let lane2 = term(sound("hh"), vec![Postfix::Fast(MiniArg::Const(8.0))]);
    let par = m(MiniKind::Parallel(vec![lane1, lane2]));
    assert_eq!(emit_expr(&s_island(par)), "s(bd ~ sd ~ & hh*8)");

    // s(a [b & c] d) — local parallel via a group
    let group = m(MiniKind::Group(Box::new(m(MiniKind::Parallel(vec![
        sound("b"),
        sound("c"),
    ])))));
    let seq = m(MiniKind::Sequence(vec![sound("a"), group, sound("d")]));
    assert_eq!(emit_expr(&s_island(seq)), "s(a [b & c] d)");
}

#[test]
fn island_alt_and_splice() {
    // s(bd <sd cp>)
    let alt = m(MiniKind::Alt(Box::new(m(MiniKind::Sequence(vec![
        sound("sd"),
        sound("cp"),
    ])))));
    let seq = m(MiniKind::Sequence(vec![sound("bd"), alt]));
    assert_eq!(emit_expr(&s_island(seq)), "s(bd <sd cp>)");

    // n(c5 $motif g4)
    let seq = m(MiniKind::Sequence(vec![
        note("c5"),
        m(MiniKind::Splice(id("motif"))),
        note("g4"),
    ]));
    assert_eq!(emit_expr(&n_island(seq)), "n(c5 $motif g4)");
}

#[test]
fn wide_island_wraps_alt_one_bar_per_line() {
    // A four-bar alternation whose inline form blows past the margin must break:
    // each `[...]` bar on its own indented line, `<`/`>` framing it.
    let bar = |n: &str| {
        m(MiniKind::Group(Box::new(m(MiniKind::Sequence(
            (0..8).map(|_| sound(n)).collect(),
        )))))
    };
    let alt = m(MiniKind::Alt(Box::new(m(MiniKind::Sequence(vec![
        bar("bd"),
        bar("sd"),
        bar("hh"),
        bar("cp"),
    ])))));
    let src = emit_expr(&s_island(alt));
    assert!(src.contains('\n'), "wide island should wrap:\n{src}");
    assert!(src.contains("\n  [bd"), "first bar on its own indented line:\n{src}");
    assert!(src.contains("\n  [sd"), "later bars each on a line:\n{src}");
    assert!(src.contains("\n>"), "closing angle on its own line:\n{src}");
    // The bars themselves are short enough to stay inline.
    assert!(src.contains("[bd bd bd bd bd bd bd bd]"), "bar stays inline:\n{src}");
}

#[test]
fn wide_parallel_wraps_one_lane_per_line() {
    // Many `&` lanes that overflow the margin break one lane per line. Each lane
    // is long enough that the inline parallel exceeds MAX_WIDTH (88) and wraps.
    let lane = |n: &str| m(MiniKind::Sequence((0..8).map(|_| sound(n)).collect()));
    let par = m(MiniKind::Parallel(vec![
        lane("bd"),
        lane("sd"),
        lane("hh"),
        lane("cp"),
    ]));
    let src = emit_expr(&s_island(par));
    assert!(src.contains("\n& "), "lanes should break onto their own lines:\n{src}");
}

#[test]
fn narrow_island_stays_inline() {
    // Below the margin nothing wraps — the canonical short form is preserved.
    let seq = m(MiniKind::Sequence(vec![sound("bd"), sound("sd"), sound("hh")]));
    let src = emit_expr(&s_island(seq));
    assert_eq!(src, "s(bd sd hh)");
}

// ── Items & multi-line output ─────────────────────────────────────────────────

#[test]
fn items_one_per_line() {
    let program = Program {
        items: vec![
            Item::Import(Import {
                names: vec![id("kick"), id("snare")],
                path: "lib/drums.nemus".to_string(),
                span: sp(),
            }),
            Item::Let(LetBind {
                name: id("bass"),
                value: n_island(m(MiniKind::Sequence(vec![note("c2"), note("g1")]))),
                span: sp(),
            }),
            Item::Fn(FnDef {
                name: id("bassline"),
                params: vec![id("root")],
                body: method(
                    n_island(m(MiniKind::Splice(id("root")))),
                    "lpf",
                    vec![num(800.0)],
                ),
                span: sp(),
            }),
        ],
    };
    let expected = "\
import { kick, snare } from \"lib/drums.nemus\"
let bass = n(c2 g1)
fn bassline(root) = n($root).lpf(800)
";
    assert_eq!(emit(&program), expected);
}

#[test]
fn tracks_and_arrange_are_multiline() {
    // tracks(track("bass", bass), track("drums", arrange(cycles(4, x), cycles(8, y))))
    let arrange = call(
        "arrange",
        vec![
            call("cycles", vec![num(4.0), var("x")]),
            call("cycles", vec![num(8.0), var("y")]),
        ],
    );
    let tracks = call(
        "tracks",
        vec![
            call("track", vec![string("bass"), var("bass")]),
            call("track", vec![string("drums"), arrange]),
        ],
    );
    let program = Program {
        items: vec![Item::Expr(tracks)],
    };
    let expected = "\
tracks(
  track(\"bass\", bass),
  track(\"drums\", arrange(
    cycles(4, x),
    cycles(8, y),
  )),
)
";
    assert_eq!(emit(&program), expected);
}
