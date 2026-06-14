//! Front-end round-trip, exercising the real Tree-sitter parser.
//!
//! Two directions close Fase 1's text↔Pattern loop:
//!   - `emit(parse(src)) == src` for **canonical** sources (a semantic
//!     round-trip; comments / incidental whitespace are not in the AST);
//!   - `parse` → `eval` on real `.nemus` text produces the expected haps.

use std::rc::Rc;
use std::sync::Arc;

use arbor_nemus_lang::prelude::{emit, evaluate, parse, EvalConfig, Item, NoImports, SilentLog};
use arbor_nemus_pattern::prelude::{ControlMap, Hap, Pattern, TimeSpan};

// ── source → AST → source ──────────────────────────────────────────────────────

/// The emitter ends every file with a newline, so canonical single-line input
/// `src` re-emits as `"{src}\n"`.
fn emits_back(src: &str) {
    let program = parse(src).unwrap_or_else(|e| panic!("parse `{src}`: {e}"));
    assert_eq!(emit(&program), format!("{src}\n"), "round-trip of `{src}`");
}

#[test]
fn host_sources_round_trip() {
    emits_back("choose(c4, ef4, g4)");
    emits_back("i*0.1");
    emits_back("a-b+c");
    emits_back("-x");
    emits_back("-2+3");
    emits_back("(a+b)*c");
    emits_back("a-(b-c)");
    emits_back("0..8");
    emits_back("0..=7");
    emits_back("(i, j) => i");
    emits_back("(0..8).par(i => n($i))");
    emits_back("f(a, b, c)");
    emits_back("lead.gain(0.5).pan(0.3)");
    emits_back("let bass = n(c2 g1)");
    emits_back("fn bassline(root) = n($root).lpf(800)");
    emits_back(r#"n(c2 g1).inst("synth.bass")"#);
    emits_back(r#"n(0 2 4).scale("c:minor")"#);
}

#[test]
fn island_sources_round_trip() {
    emits_back("s(bd ~ sd ~)");
    emits_back("n(c4 e4 g4)");
    emits_back("n(cs4 bf3)");
    emits_back("s(cp:2(3,8))");
    emits_back("s(bd(3,8,2))");
    emits_back("s(bd*2 sd!3)");
    emits_back("s(bd/2)");
    emits_back("s(a@2 b)");
    emits_back("n(c4'min7)");
    emits_back("s(bd ~ sd ~ & hh*8)");
    emits_back("s(a [b & c] d)");
    emits_back("s([bd sd] hh)");
    emits_back("s(bd <sd cp>)");
    emits_back("n(c5 $motif g4)");
}

#[test]
fn patterned_args_and_polymeter_round_trip() {
    // Patterned `*`/`/` factors (alternation, sequence-group, polymeter).
    emits_back("s(bd*<2 3>)");
    emits_back("s(bd*[2 3])");
    emits_back("s(bd*{2 3})");
    emits_back("s(bd/<2 3>)");
    // Patterned euclid counts.
    emits_back("s(bd(<3 5>,8))");
    emits_back("s(bd(3,<8 16>))");
    emits_back("s(bd(3,8,<0 2>))");
    // Polymeter atoms.
    emits_back("n({c4 e4 g4}%2)");
    emits_back("n({c4 e4 g4})");
    emits_back("n({c4 e4 g4 & d4 f4}%3)");
}

#[test]
fn parse_then_eval_patterned_fast() {
    // s(bd*<2 3>) → two onsets on even cycles, three on odd.
    let p = eval_first_track("s(bd*<2 3>)");
    assert_eq!(onsets(&p, 0).len(), 2);
    assert_eq!(onsets(&p, 1).len(), 3);
}

#[test]
fn parse_then_eval_patterned_euclid() {
    // s(bd(<3 5>,8)) → tresillo then quintillo across cycles.
    let p = eval_first_track("s(bd(<3 5>,8))");
    assert_eq!(onsets(&p, 0).len(), 3);
    assert_eq!(onsets(&p, 1).len(), 5);
}

#[test]
fn parse_then_eval_polymeter_default_steps() {
    // n({c4 e4 g4}) — default steps = first lane length (3) → 3 onsets/cycle.
    let p = eval_first_track("n({c4 e4 g4})");
    assert_eq!(onsets(&p, 0).len(), 3);
}

#[test]
fn parse_then_eval_polymeter_steps() {
    // n({c4 e4 g4}%2) — 2 steps/cycle, looping the 3-item lane: cycle 0 → c4,e4.
    let p = eval_first_track("n({c4 e4 g4}%2)");
    let notes: Vec<f64> = onsets(&p, 0).iter().filter_map(|h| h.value.note).collect();
    assert_eq!(notes.len(), 2);
    assert!(notes.contains(&60.0) && notes.contains(&64.0)); // c4, e4
}

#[test]
fn arrange_multiline_round_trips() {
    let src = "\
arrange(
  cycles(4, intro),
  cycles(16, main),
)
";
    let program = parse(src).unwrap_or_else(|e| panic!("parse: {e}"));
    assert_eq!(emit(&program), src);
}

#[test]
fn comments_are_dropped() {
    // Comments live only in the source; the AST (and so the emitter) drops them.
    let program = parse("// header\nn(c4) // trailing\n").unwrap();
    assert_eq!(emit(&program), "n(c4)\n");
}

#[test]
fn import_statement_structure() {
    let program = parse(r#"import { kick, snare } from "lib/drums.nemus""#).unwrap();
    match &program.items[0] {
        Item::Import(im) => {
            assert_eq!(im.names.len(), 2);
            assert_eq!(im.names[0].name, "kick");
            assert_eq!(im.path, "lib/drums.nemus");
        }
        other => panic!("expected an import, got {other:?}"),
    }
}

#[test]
fn multiline_program_round_trips() {
    let src = "\
import { kick, snare } from \"lib/drums.nemus\"
let bass = n(c2 g1)
tracks(
  track(\"bass\", bass),
  track(\"drums\", kick),
)
";
    let program = parse(src).unwrap_or_else(|e| panic!("parse: {e}"));
    assert_eq!(emit(&program), src);
}

#[test]
fn note_alias_normalises_to_short_form() {
    // `sound(...)` / `note(...)` are accepted but re-emit as `s` / `n`.
    let program = parse("sound(bd) note(c4)").unwrap();
    // two bare expressions, each on its own line
    assert_eq!(emit(&program), "s(bd)\nn(c4)\n");
}

// ── source → AST → eval ─────────────────────────────────────────────────────────

fn eval_first_track(src: &str) -> Pattern<ControlMap> {
    let out = evaluate(
        &parse(src).unwrap_or_else(|e| panic!("parse `{src}`: {e}")),
        Rc::new(NoImports),
        Arc::new(SilentLog),
        EvalConfig::default(),
    )
    .unwrap_or_else(|e| panic!("eval `{src}`: {e}"));
    out.tracks.tracks[0].pattern.clone()
}

#[test]
fn parse_then_eval_scene_declaration() {
    // `scene(...)` is a side-channel like `cps`/`tempo`: it registers a launchable
    // scene, adds no pattern to the output tracks, and keeps source order + names.
    let src = r#"
tracks(track("drums", s(bd sn)), track("bass", n(c2 g1)))
scene("chorus", track("drums", s(bd bd sn bd)), track("bass", n(c2 ef2)))
scene("break", track("drums", s(bd ~ ~ ~)))
"#;
    let out = evaluate(
        &parse(src).unwrap_or_else(|e| panic!("parse: {e}")),
        Rc::new(NoImports),
        Arc::new(SilentLog),
        EvalConfig::default(),
    )
    .unwrap_or_else(|e| panic!("eval: {e}"));

    // Scenes don't leak into the channel list.
    assert_eq!(out.tracks.tracks.len(), 2);
    assert_eq!(out.scenes.len(), 2);
    assert_eq!(out.scenes[0].name, "chorus");
    assert_eq!(out.scenes[0].clips.len(), 2);
    assert_eq!(out.scenes[0].clips[0].name, "drums");
    assert_eq!(out.scenes[1].name, "break");
    assert_eq!(out.scenes[1].clips.len(), 1);
}

fn onsets(p: &Pattern<ControlMap>, cyc: i64) -> Vec<Hap<ControlMap>> {
    // Queries are unordered (Tidal-style); `rev` reflects times without
    // reordering the Vec, so the consumer sorts by onset — as the engine will.
    let mut haps: Vec<_> = p
        .query(TimeSpan::cycle(cyc))
        .into_iter()
        .filter(|h| h.has_onset())
        .collect();
    haps.sort_by_key(|h| h.part.begin);
    haps
}

#[test]
fn parse_then_eval_humanize() {
    // humanize wobbles gain (here only gain, no timing jitter) seeded per onset:
    // every onset survives, each gets a gain near 1, and two onsets differ.
    let p = eval_first_track("s(bd sd hh sd).humanize(0, 0.2)");
    let h = onsets(&p, 0);
    assert_eq!(h.len(), 4); // none lost
    let gains: Vec<f64> = h.iter().map(|x| x.value.gain.unwrap()).collect();
    assert!(gains.iter().all(|g| (0.8..=1.2).contains(g)));
    assert!(gains[0] != gains[1]); // independent per onset
    // Deterministic every loop.
    let g_again: Vec<f64> = onsets(&p, 0).iter().map(|x| x.value.gain.unwrap()).collect();
    assert_eq!(gains, g_again);
}

#[test]
fn parse_then_eval_note_sequence() {
    let p = eval_first_track("n(c4 e4 g4)");
    let mut notes: Vec<f64> = onsets(&p, 0).iter().filter_map(|h| h.value.note).collect();
    notes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(notes, vec![60.0, 64.0, 67.0]);
}

#[test]
fn parse_then_eval_choose_note_literals() {
    let p = eval_first_track("choose(c4, ef4, g4)");
    let h = onsets(&p, 0);
    assert_eq!(h.len(), 1);
    assert!([60.0, 63.0, 67.0].contains(&h[0].value.note.unwrap()));
}

#[test]
fn parse_then_eval_euclid() {
    // s(bd(3,8)) → onsets at 0, 3/8, 6/8.
    let p = eval_first_track("s(bd(3,8))");
    assert_eq!(onsets(&p, 0).len(), 3);
}

#[test]
fn parse_then_eval_degrees_with_scale() {
    // n(0 2 4).scale("c:minor") → C, Eb, G = 60, 63, 67.
    let p = eval_first_track(r#"n(0 2 4).scale("c:minor")"#);
    let notes: Vec<f64> = onsets(&p, 0).iter().filter_map(|h| h.value.note).collect();
    assert_eq!(notes, vec![60.0, 63.0, 67.0]);
}

#[test]
fn parse_then_eval_let_reference() {
    // A `let` binding is referenced by a later bare expression (the output).
    let p = eval_first_track("let lead = n(c4 e4)\nlead");
    let notes: Vec<f64> = onsets(&p, 0).iter().filter_map(|h| h.value.note).collect();
    assert_eq!(notes, vec![60.0, 64.0]);
}

#[test]
fn parse_then_eval_fn_call_with_splice() {
    // A `fn` whose body splices its parameter into an island.
    let p = eval_first_track("fn lead(x) = n($x)\nlead(c4)");
    assert_eq!(onsets(&p, 0)[0].value.note, Some(60.0));
}

#[test]
fn parse_then_eval_method_chain() {
    // s(bd sd hh).rev() reverses the cycle; .gain(0.5) sets the control.
    let p = eval_first_track("s(bd sd hh).rev().gain(0.5)");
    let h = onsets(&p, 0);
    let names: Vec<String> = h.iter().map(|x| x.value.sound.clone().unwrap()).collect();
    assert_eq!(names, vec!["hh", "sd", "bd"]);
    assert!(h.iter().all(|x| x.value.gain == Some(0.5)));
}

#[test]
fn parse_then_eval_fast() {
    // s(bd*2) → two onsets per cycle.
    assert_eq!(onsets(&eval_first_track("s(bd*2)"), 0).len(), 2);
}

#[test]
fn parse_reports_syntax_error_with_span() {
    let err = parse("n(c4").unwrap_err();
    assert!(err.span.is_some(), "syntax error should carry a span");
}

// ── §F transforms (parse → eval) ────────────────────────────────────────────────

#[test]
fn add_transposes_notes_in_semitones() {
    // n(c4 e4).add(7) → +7 semitones → g4, b4.
    let p = eval_first_track("n(c4 e4).add(7)");
    let mut notes: Vec<f64> = onsets(&p, 0).iter().filter_map(|h| h.value.note).collect();
    notes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(notes, vec![67.0, 71.0]);
}

#[test]
fn add_deg_transposes_before_scale() {
    // n(0 2).addDeg(2).scale("c:minor") → degrees 2,4 → Eb4, G4 = 63, 67.
    let p = eval_first_track(r#"n(0 2).addDeg(2).scale("c:minor")"#);
    let mut notes: Vec<f64> = onsets(&p, 0).iter().filter_map(|h| h.value.note).collect();
    notes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(notes, vec![63.0, 67.0]);
}

#[test]
fn degrade_by_drops_a_fraction() {
    // s(hh*16).degradeBy(0) keeps all; degradeBy(1) drops all.
    let kept_none = eval_first_track("s(hh*16).degradeBy(1)");
    assert_eq!(onsets(&kept_none, 0).len(), 0);
    let kept_all = eval_first_track("s(hh*16).degradeBy(0)");
    assert_eq!(onsets(&kept_all, 0).len(), 16);
}

#[test]
fn iter_rotates_across_cycles() {
    // s(a b c d).iter(4) rotates one slot left each cycle.
    let p = eval_first_track("s(a b c d).iter(4)");
    let names = |c| {
        onsets(&p, c)
            .iter()
            .map(|h| h.value.sound.clone().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(0), vec!["a", "b", "c", "d"]);
    assert_eq!(names(1), vec!["b", "c", "d", "a"]);
}

#[test]
fn palindrome_alternates_direction() {
    let p = eval_first_track("s(a b c).palindrome()");
    let names = |c| {
        onsets(&p, c)
            .iter()
            .map(|h| h.value.sound.clone().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(0), vec!["a", "b", "c"]);
    assert_eq!(names(1), vec!["c", "b", "a"]);
}

#[test]
fn chunk_keeps_event_count() {
    // chunk applies a transform to a rotating slice; the grid stays 4 onsets.
    let p = eval_first_track("s(a b c d).chunk(4, rev)");
    for c in 0..4 {
        assert_eq!(onsets(&p, c).len(), 4);
    }
}

#[test]
fn swing_by_preserves_events() {
    let p = eval_first_track("s(a b c d).swingBy(0.05, 2)");
    assert_eq!(onsets(&p, 0).len(), 4);
}

#[test]
fn delay_sets_the_control_map_fields() {
    // s(bd).delay(0.25) → delay time set, default fb/mix applied.
    let p = eval_first_track("s(bd).delay(0.25)");
    let h = &onsets(&p, 0)[0];
    assert_eq!(h.value.delay, Some(0.25));
    assert_eq!(h.value.feedback, Some(0.3)); // default
    assert_eq!(h.value.delay_mix, Some(0.5)); // default
    // explicit fb/mix override the defaults.
    let p2 = eval_first_track("s(bd).delay(0.125, 0.6, 0.4)");
    let h2 = &onsets(&p2, 0)[0];
    assert_eq!(h2.value.feedback, Some(0.6));
    assert_eq!(h2.value.delay_mix, Some(0.4));
}

#[test]
fn signal_range_patternises_a_control() {
    // s(hh*2).lpf(saw.range(200, 2000)) → per-event cutoff from the saw ramp.
    let p = eval_first_track("s(hh*2).lpf(saw.range(200, 2000))");
    let h = onsets(&p, 0);
    assert_eq!(h.len(), 2);
    // both cutoffs lie in the rescaled range; the two slots differ (ramp rises).
    let c0 = h[0].value.lpf.unwrap();
    let c1 = h[1].value.lpf.unwrap();
    assert!((200.0..=2000.0).contains(&c0) && (200.0..=2000.0).contains(&c1));
    assert_ne!(c0, c1);
}

#[test]
fn every_with_palindrome_nullary() {
    // s(a b).every(2, palindrome) — palindrome passed nullary into a HOF.
    let p = eval_first_track("s(a b).every(2, palindrome)");
    // cycle 0: palindrome's cycle 0 (forward) → a, b
    let names: Vec<String> = onsets(&p, 0)
        .iter()
        .map(|h| h.value.sound.clone().unwrap())
        .collect();
    assert_eq!(names, vec!["a", "b"]);
}
