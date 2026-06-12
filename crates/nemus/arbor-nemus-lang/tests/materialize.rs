//! Materialiser tests: one cycle of evaluated haps → a mini-notation literal.
//!
//! Builds `Hap`s by hand (no evaluator) and asserts the printed `s(...)` /
//! `n(...)` source, pinning the base render (equal slots, `~`, `@`, `&` lanes).

use arbor_nemus_lang::ast::{IslandKind, MiniKind};
use arbor_nemus_lang::prelude::{materialize_island, materialize_source};
use arbor_nemus_pattern::prelude::{ControlMap, Hap, Time, TimeSpan};

fn t(num: i64, den: i64) -> Time {
    Time::new(num, den)
}

/// A discrete event over `[begin, end)` carrying `value` (onset captured).
fn hap(begin: (i64, i64), end: (i64, i64), value: ControlMap) -> Hap<ControlMap> {
    let whole = TimeSpan::new(t(begin.0, begin.1), t(end.0, end.1));
    Hap::new(Some(whole), whole, value)
}

#[test]
fn empty_renders_a_single_rest() {
    assert_eq!(materialize_source(IslandKind::Sound, &[]), "s(~)");
    assert_eq!(materialize_source(IslandKind::Note, &[]), "n(~)");

    let island = materialize_island(IslandKind::Sound, &[]);
    assert_eq!(island.kind, IslandKind::Sound);
    assert_eq!(island.root.kind, MiniKind::Rest);
}

#[test]
fn single_event_fills_the_cycle() {
    let haps = [hap((0, 1), (1, 1), ControlMap::sound("bd"))];
    assert_eq!(materialize_source(IslandKind::Sound, &haps), "s(bd)");
}

#[test]
fn two_equal_halves() {
    let haps = [
        hap((0, 1), (1, 2), ControlMap::sound("bd")),
        hap((1, 2), (1, 1), ControlMap::sound("sd")),
    ];
    assert_eq!(materialize_source(IslandKind::Sound, &haps), "s(bd sd)");
}

#[test]
fn gap_becomes_weighted_rest() {
    // A single quarter-cycle hit, then silence.
    let haps = [hap((0, 1), (1, 4), ControlMap::sound("bd"))];
    assert_eq!(materialize_source(IslandKind::Sound, &haps), "s(bd ~@3)");
}

#[test]
fn multi_slot_duration_uses_weight() {
    let haps = [
        hap((0, 1), (1, 2), ControlMap::sound("bd")),
        hap((1, 2), (3, 4), ControlMap::sound("sd")),
        hap((3, 4), (1, 1), ControlMap::sound("hh")),
    ];
    assert_eq!(
        materialize_source(IslandKind::Sound, &haps),
        "s(bd@2 sd hh)"
    );
}

#[test]
fn note_pitches_render_by_name() {
    let haps = [
        hap((0, 1), (1, 2), ControlMap::note(60.0)), // c4
        hap((1, 2), (1, 1), ControlMap::note(63.0)), // ef4
    ];
    assert_eq!(materialize_source(IslandKind::Note, &haps), "n(c4 ef4)");
}

#[test]
fn degrees_render_as_integers() {
    let haps = [
        hap((0, 1), (1, 2), ControlMap::degree(0)),
        hap((1, 2), (1, 1), ControlMap::degree(2)),
    ];
    assert_eq!(materialize_source(IslandKind::Note, &haps), "n(0 2)");
}

#[test]
fn sample_variant_is_preserved() {
    let mut bd = ControlMap::sound("bd");
    bd.variant = Some(3);
    let haps = [hap((0, 1), (1, 1), bd)];
    assert_eq!(materialize_source(IslandKind::Sound, &haps), "s(bd:3)");
}

#[test]
fn overlapping_events_split_into_parallel_lanes() {
    // Two pitches sounding the whole cycle (a chord) → `&` lanes.
    let haps = [
        hap((0, 1), (1, 1), ControlMap::note(60.0)), // c4
        hap((0, 1), (1, 1), ControlMap::note(64.0)), // e4
    ];
    assert_eq!(materialize_source(IslandKind::Note, &haps), "n(c4 & e4)");
}
