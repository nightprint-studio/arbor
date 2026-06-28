//! Integration tests: build patterns by hand, query a given cycle, assert on
//! the resulting haps — timing, whole/part fragmentation, source spans and
//! determinism. This is the Fase 0 deliverable in test form.

use merula_pattern::prelude::*;

fn t(n: i64, d: i64) -> Time {
    Time::new(n, d)
}

/// A four-on-the-floor kick + offbeat hat, queried at an arbitrary cycle, lands
/// on exact boundaries.
#[test]
fn drum_pattern_timing_is_exact() {
    let kick = pure(ControlMap::sound("bd")).fast(t(4, 1)); // 4 kicks/cycle
    let hats = pure(ControlMap::sound("hh")).fast(t(8, 1)); // 8 hats/cycle
    let beat = stack(vec![kick, hats]);

    let haps = beat.query(TimeSpan::cycle(7));
    let kicks: Vec<_> = haps
        .iter()
        .filter(|h| h.value.sound.as_deref() == Some("bd") && h.has_onset())
        .collect();
    assert_eq!(kicks.len(), 4);
    // kick onsets at 7, 7.25, 7.5, 7.75
    let onsets: Vec<Time> = kicks.iter().map(|h| h.whole.unwrap().begin).collect();
    assert_eq!(
        onsets,
        vec![t(7, 1), t(29, 4), t(15, 2), t(31, 4)]
    );
}

/// Querying a sub-cycle window clips `part` but keeps `whole`; onset detection
/// distinguishes the head fragment from the tail.
#[test]
fn whole_and_part_fragmentation() {
    let p = pure("x");

    let head = &p.query(TimeSpan::new(t(0, 1), t(1, 2)))[0];
    assert_eq!(head.whole, Some(TimeSpan::cycle(0)));
    assert_eq!(head.part, TimeSpan::new(t(0, 1), t(1, 2)));
    assert!(head.has_onset());

    let tail = &p.query(TimeSpan::new(t(1, 2), t(1, 1)))[0];
    assert_eq!(tail.whole, Some(TimeSpan::cycle(0)));
    assert_eq!(tail.part, TimeSpan::new(t(1, 2), t(1, 1)));
    assert!(!tail.has_onset());
}

/// Source spans ride along through transforms (here `rev`), so the editor can
/// always map an active hap back to its characters.
#[test]
fn source_spans_survive_transforms() {
    // Two leaves with distinct spans, laid out in a cycle, then reversed.
    let a = Pattern::new(|span: TimeSpan| {
        vec![Hap::new(Some(TimeSpan::cycle(span.begin.floor())), span, "a")
            .with_span(SourceSpan::new(0, 1))]
    });
    let b = Pattern::new(|span: TimeSpan| {
        vec![Hap::new(Some(TimeSpan::cycle(span.begin.floor())), span, "b")
            .with_span(SourceSpan::new(2, 3))]
    });
    let p = fastcat(vec![a, b]).rev();
    let mut haps = p.query(TimeSpan::cycle(0));
    // Results aren't guaranteed time-ordered; sort by onset to read the sequence.
    haps.sort_by_key(|h| h.part.begin);
    // reversed order: b (span 2..3) first, then a (span 0..1)
    assert_eq!(haps[0].value, "b");
    assert_eq!(haps[0].span, Some(SourceSpan::new(2, 3)));
    assert_eq!(haps[1].value, "a");
    assert_eq!(haps[1].span, Some(SourceSpan::new(0, 1)));
}

/// The same cycle queried twice — and queried directly vs. as part of a wider
/// span — yields identical random choices.
#[test]
fn determinism_across_queries() {
    let p = pure(ControlMap::sound("hh")).fast(t(16, 1)).degrade();

    let direct = p.query(TimeSpan::cycle(5));
    let again = p.query(TimeSpan::cycle(5));
    let onsets = |hs: &[Hap<ControlMap>]| -> Vec<Time> {
        hs.iter().filter(|h| h.has_onset()).map(|h| h.onset()).collect()
    };
    assert_eq!(onsets(&direct), onsets(&again));

    // Cycle 5 carved out of a [0,8) sweep matches the direct query.
    let wide = p.query(TimeSpan::new(t(0, 1), t(8, 1)));
    let from_wide: Vec<Time> = wide
        .iter()
        .filter(|h| h.has_onset() && h.onset().floor() == 5)
        .map(|h| h.onset())
        .collect();
    assert_eq!(from_wide, onsets(&direct));
}

/// `arrange` lays sections on the absolute timeline and loops at the total.
#[test]
fn arrangement_timeline_and_loop() {
    let song = arrange(vec![
        cycles(2, pure("intro")),
        cycles(3, pure("main")),
    ]);
    let label = |c: i64| song.query(TimeSpan::cycle(c))[0].value;
    assert_eq!(label(0), "intro");
    assert_eq!(label(1), "intro");
    assert_eq!(label(2), "main");
    assert_eq!(label(4), "main");
    assert_eq!(label(5), "intro"); // period = 5, loops
    assert_eq!(label(9), "main");
}

/// Degrees resolve against a scale; a melody of degrees becomes concrete MIDI.
#[test]
fn scale_degrees_to_pitches() {
    let melody = fastcat(vec![
        pure(ControlMap::degree(0)),
        pure(ControlMap::degree(2)),
        pure(ControlMap::degree(4)),
    ])
    .scale(Scale::parse("c:minor").unwrap(), 4);

    let notes: Vec<f64> = melody
        .query(TimeSpan::cycle(0))
        .iter()
        .map(|h| h.value.note.unwrap())
        .collect();
    assert_eq!(notes, vec![60.0, 63.0, 67.0]); // C4, Eb4, G4
}

/// Patternised parameters: `rand` feeds a per-event control.
#[test]
fn patternized_param_via_rand() {
    let p = pure(ControlMap::note(60.0))
        .fast(t(4, 1))
        .gain(rand(0.2, 0.8));
    let gains: Vec<f64> = p
        .query(TimeSpan::cycle(0))
        .iter()
        .map(|h| h.value.gain.unwrap())
        .collect();
    assert_eq!(gains.len(), 4);
    assert!(gains.iter().all(|g| (0.2..0.8).contains(g)));
    // distinct onsets → not all identical
    assert!(gains.windows(2).any(|w| w[0] != w[1]));
}
