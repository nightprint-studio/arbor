//! Assemble idiomatic `.merula` from a [`Song`].
//!
//! The output is built as an `merula-lang` AST and printed through that
//! crate's canonical emitter — so all *formatting* lives in one place and we
//! only decide *structure* here:
//!
//! - per track, per cycle, notes become a mini-notation island (equal slots,
//!   rests, `@`-weights for held notes, `&` lanes for overlaps);
//! - recognised chords fold to a root note + `'symbol`;
//! - a track whose every note fits the detected scale emits **degrees** plus a
//!   trailing `.scale("root:mode")`;
//! - **the timeline is split into phrases** (short, ≤ [`PHRASE_BARS`]-bar blocks,
//!   plus runs of identical bars), repeated phrases are **deduplicated into
//!   `let` bindings**, and the track plays them through `arrange(section(...))`.
//!   So a three-minute take becomes a handful of named, editable phrases with the
//!   chorus written once — never one unreadable inline pattern.
//!
//! A short track that fits in a single phrase stays a bare island (it loops on
//! its own), exactly as before; the arrangement machinery only appears once a
//! track has real structure (more than one phrase).
//!
//! This mirrors the algorithm in `merula-lang`'s `materialize` (single-cycle
//! haps → island), extended here with the import-specific policy materialisation
//! deliberately leaves out: chord-name recovery, scale-degree emission and
//! phrase factoring.

use std::collections::HashMap;

use merula_pattern::prelude::Time;

use merula_lang::prelude::{
    emit as emit_program, Expr, IslandKind, Item, Leaf, Mini, MiniKind, Postfix,
};

use crate::build::{
    call, island, leaf, let_item, method, mini, num, program, string, var,
};
use crate::key::{degree_of, DetectedKey};
use crate::model::{ImportOptions, NoteTrack, Song};
use crate::{chords, gm_drum};

/// Maximum number of distinct bars grouped into one phrase. Four is the classic
/// musical phrase length, so a repeated chorus that lands on a four-bar boundary
/// deduplicates cleanly; it also keeps each `let` small enough to read at a glance.
const PHRASE_BARS: usize = 4;

/// A single bar's mini-notation up to this many leaves stays inline in its
/// `section(...)` rather than becoming a `let` — a `let` for `s([bd ~ sn ~])`
/// would be more noise than it saves. Anything busier, or any phrase reused
/// across the song, is promoted to a binding.
const INLINE_MAX: usize = 6;

/// Render a whole [`Song`] to idiomatic `.merula` source (with trailing newline).
pub fn song_to_merula(song: &Song, opts: &ImportOptions) -> String {
    let key = if opts.detect_key {
        crate::key::detect(song)
    } else {
        None
    };

    let mut phrases = Phrases::new();
    let track_exprs: Vec<Expr> = song
        .tracks
        .iter()
        .map(|t| phrases.track_expr(t, song.len_cycles, key.as_ref(), opts))
        .collect();

    let output = if track_exprs.len() == 1 {
        track_exprs.into_iter().next().unwrap()
    } else {
        let args = song
            .tracks
            .iter()
            .zip(track_exprs)
            .map(|(t, e)| call("track", vec![string(&t.name), e]))
            .collect();
        call("tracks", args)
    };

    // cps · then the phrase bindings · then the output that references them.
    let mut items = vec![Item::Expr(call("cps", vec![num(round4(song.cps))]))];
    items.append(&mut phrases.lets);
    items.push(Item::Expr(output));
    emit_program(&program(items))
}

// ── Phrase factoring (the song's structure) ─────────────────────────────────────

/// Collects the program-level `let` bindings as tracks are emitted, handing out
/// globally-unique variable names so phrases reused anywhere are written once.
struct Phrases {
    lets: Vec<Item>,
    next_id: u32,
}

/// One stretch of a track's timeline: `bars` is the distinct content (one bar for
/// a held/looping run, up to [`PHRASE_BARS`] for a phrase), `cycles` how many
/// cycles it occupies (a run loops its single bar; a phrase plays once through).
struct Segment {
    bars: Vec<Mini>,
    cycles: u32,
}

impl Phrases {
    fn new() -> Self {
        Phrases {
            lets: Vec::new(),
            next_id: 0,
        }
    }

    fn fresh_var(&mut self) -> String {
        self.next_id += 1;
        format!("phrase{}", self.next_id)
    }

    /// One track → an expression: a bare island when it fits a single phrase,
    /// otherwise `arrange(section(...), ...)` over deduplicated phrase bindings.
    fn track_expr(
        &mut self,
        t: &NoteTrack,
        len: u32,
        key: Option<&DetectedKey>,
        opts: &ImportOptions,
    ) -> Expr {
        let folded = !t.is_drum && opts.detect_chords && has_recognizable_chord(t);
        let use_degrees =
            !t.is_drum && !folded && key.is_some_and(|k| track_fits(k, t, opts.ref_octave));
        let kind = if t.is_drum {
            IslandKind::Sound
        } else {
            IslandKind::Note
        };

        // Per-cycle minis, then split into phrases / runs.
        let mut cycles: Vec<Mini> = Vec::with_capacity(len as usize);
        for cyc in 0..len as i64 {
            let mut cells =
                build_cells(t, cyc, opts.detect_chords, use_degrees, key, opts.ref_octave);
            cells.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
            cycles.push(cycle_mini(&cells));
        }
        let segments = segment(&cycles);

        // A single phrase loops on its own — keep the compact bare island.
        if segments.len() <= 1 {
            let body = collapse(cycles, opts.detect_loops);
            return island_with_scale(kind, body, use_degrees, key);
        }

        // A phrase used more than once is shared as a binding even when trivial.
        let reused =
            |bars: &[Mini]| segments.iter().filter(|s| s.bars.as_slice() == bars).count() >= 2;

        let mut seen: Vec<(Vec<Mini>, String, Option<String>)> = Vec::new();
        let mut next_label = 0usize;
        let mut sections: Vec<Expr> = Vec::with_capacity(segments.len());
        for seg in &segments {
            let body_expr = if let Some(e) = seen.iter().find(|e| e.0 == seg.bars) {
                // A phrase seen before is always a binding (reuse it verbatim).
                (e.1.clone(), var(e.2.as_deref().unwrap_or(e.1.as_str())))
            } else {
                let label = column_label(next_label);
                next_label += 1;
                let body = collapse(seg.bars.clone(), opts.detect_loops);
                let isl = island_with_scale(kind, body, use_degrees, key);
                let promote = reused(&seg.bars)
                    || seg.bars.len() > 1
                    || mini_complexity(&seg.bars[0]) > INLINE_MAX;
                if promote {
                    let vname = self.fresh_var();
                    self.lets.push(let_item(&vname, isl));
                    seen.push((seg.bars.clone(), label.clone(), Some(vname.clone())));
                    (label, var(&vname))
                } else {
                    seen.push((seg.bars.clone(), label.clone(), None));
                    (label, isl)
                }
            };
            let (label, expr) = body_expr;
            sections.push(call(
                "section",
                vec![string(&label), num(seg.cycles as f64), expr],
            ));
        }
        call("arrange", sections)
    }
}

/// Wrap a cycle body as an island, adding `.scale(...)` when the track emits
/// degrees (the spec lives on each phrase so `arrange`'s bands survive — a
/// transform applied to `arrange(...)` itself would drop them).
fn island_with_scale(
    kind: IslandKind,
    body: Mini,
    use_degrees: bool,
    key: Option<&DetectedKey>,
) -> Expr {
    let base = island(kind, body);
    if use_degrees {
        method(base, "scale", vec![string(&key.unwrap().spec)]) // safe: track_fits gave a key
    } else {
        base
    }
}

/// Split a track's per-cycle minis into segments: a maximal run of identical bars
/// becomes one looping segment; otherwise consecutive distinct bars accumulate
/// into a phrase of up to [`PHRASE_BARS`] bars (stopping early when a run begins).
fn segment(cycles: &[Mini]) -> Vec<Segment> {
    let n = cycles.len();
    let mut segs = Vec::new();
    let mut i = 0;
    while i < n {
        let mut run = 1;
        while i + run < n && cycles[i + run] == cycles[i] {
            run += 1;
        }
        if run >= 2 {
            segs.push(Segment {
                bars: vec![cycles[i].clone()],
                cycles: run as u32,
            });
            i += run;
        } else {
            let mut bars = vec![cycles[i].clone()];
            let mut j = i + 1;
            while j < n && bars.len() < PHRASE_BARS {
                // A run starting at j is its own segment — end the phrase before it.
                if j + 1 < n && cycles[j] == cycles[j + 1] {
                    break;
                }
                bars.push(cycles[j].clone());
                j += 1;
            }
            let cycles_len = bars.len() as u32;
            segs.push(Segment { bars, cycles: cycles_len });
            i = j;
        }
    }
    segs
}

/// Total number of leaves in a mini — a cheap "is this bar busy?" measure.
fn mini_complexity(m: &Mini) -> usize {
    match &m.kind {
        MiniKind::Sequence(items) | MiniKind::Parallel(items) => {
            items.iter().map(mini_complexity).sum()
        }
        MiniKind::Term { atom, .. } => mini_complexity(atom),
        MiniKind::Group(inner) | MiniKind::Alt(inner) => mini_complexity(inner),
        MiniKind::Poly { body, .. } => mini_complexity(body),
        MiniKind::Rest | MiniKind::Extend => 0,
        MiniKind::Splice(_) | MiniKind::Leaf(_) => 1,
    }
}

/// A spreadsheet-style column label for a section: `A`, `B`, … `Z`, `AA`, `AB`, …
/// — a short, unique, human-readable name shown as the arrangement band.
fn column_label(mut i: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}

// ── One track → cells → minis ───────────────────────────────────────────────────

/// True when every pitched note of `t` maps to a degree in `key`.
fn track_fits(key: &DetectedKey, t: &NoteTrack, ref_octave: i32) -> bool {
    !t.is_drum
        && !t.notes.is_empty()
        && t
            .notes
            .iter()
            .all(|n| degree_of(key, n.pitch, ref_octave).is_some())
}

/// True if any vertical stack (same onset and duration) of `t` is a chord.
fn has_recognizable_chord(t: &NoteTrack) -> bool {
    let mut groups: HashMap<(Time, Time), Vec<i32>> = HashMap::new();
    for n in &t.notes {
        groups.entry((n.start, n.dur)).or_default().push(n.pitch);
    }
    groups
        .values()
        .any(|ps| ps.len() >= 2 && chords::recognize(ps).is_some())
}

/// A slot's worth of one lane: when it sounds, what plays.
struct Cell {
    start: Time, // cycle-local, in [0, 1)
    end: Time,   // cycle-local, clipped to ≤ 1
    atom: Mini,
    chord: Option<&'static str>,
}

/// Build the cells for cycle `cyc`. Notes are bucketed by `(onset, duration)`;
/// a multi-note bucket folds to a chord symbol when recognised, otherwise its
/// notes become separate cells (laid into `&` lanes by [`cycle_mini`]).
fn build_cells(
    t: &NoteTrack,
    cyc: i64,
    detect_chords: bool,
    use_degrees: bool,
    key: Option<&DetectedKey>,
    ref_octave: i32,
) -> Vec<Cell> {
    let cyc_t = Time::int(cyc);
    let mut groups: HashMap<(Time, Time), Vec<i32>> = HashMap::new();
    for n in &t.notes {
        if n.start.floor() != cyc {
            continue;
        }
        let start = n.start - cyc_t;
        let mut end = n.end() - cyc_t;
        if end > Time::ONE {
            end = Time::ONE; // clip notes that spill into the next bar
        }
        if end <= start {
            continue;
        }
        groups.entry((start, end)).or_default().push(n.pitch);
    }

    let mut cells = Vec::new();
    for ((start, end), pitches) in groups {
        if t.is_drum {
            for p in pitches {
                cells.push(Cell {
                    start,
                    end,
                    atom: leaf(Leaf::Sound(gm_drum::sound_for_key(p).to_string())),
                    chord: None,
                });
            }
            continue;
        }
        if detect_chords && pitches.len() >= 2 {
            if let Some(ch) = chords::recognize(&pitches) {
                cells.push(Cell {
                    start,
                    end,
                    atom: leaf(Leaf::NoteName(note_name(ch.root))),
                    chord: Some(ch.name),
                });
                continue;
            }
        }
        for p in pitches {
            cells.push(Cell {
                start,
                end,
                atom: note_leaf(p, use_degrees, key, ref_octave),
                chord: None,
            });
        }
    }
    cells
}

/// Cells → a cycle's root Mini: one equal-slot sequence per monophonic lane,
/// lanes joined with `&`.
fn cycle_mini(cells: &[Cell]) -> Mini {
    if cells.is_empty() {
        return mini(MiniKind::Rest);
    }
    let lanes = partition_lanes(cells);
    if lanes.len() == 1 {
        lane_to_mini(&lanes[0])
    } else {
        mini(MiniKind::Parallel(
            lanes.iter().map(|l| lane_to_mini(l)).collect(),
        ))
    }
}

/// Greedy interval partition into monophonic lanes (cells are pre-sorted by
/// onset), identical to the materialiser's so overlaps become deterministic
/// `&` lanes.
fn partition_lanes(cells: &[Cell]) -> Vec<Vec<&Cell>> {
    let mut lanes: Vec<Vec<&Cell>> = Vec::new();
    for c in cells {
        let slot = lanes
            .iter()
            .position(|lane| lane.last().map_or(true, |last| last.end <= c.start));
        match slot {
            Some(i) => lanes[i].push(c),
            None => lanes.push(vec![c]),
        }
    }
    lanes
}

fn lane_to_mini(lane: &[&Cell]) -> Mini {
    let n = grid_size(lane);
    let mut terms: Vec<Mini> = Vec::new();
    let mut pos: i64 = 0;
    let mut next = 0;
    while pos < n {
        if next < lane.len() && slot_of(lane[next].start, n) == pos {
            let c = lane[next];
            let weight = (slot_of(c.end, n) - pos).max(1);
            terms.push(cell_term(c, weight as u32));
            pos += weight;
            next += 1;
        } else {
            let until = if next < lane.len() {
                slot_of(lane[next].start, n)
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

/// A cell as a term: atom + optional `'chord` + optional `@weight`.
fn cell_term(c: &Cell, weight: u32) -> Mini {
    let mut pf = Vec::new();
    if let Some(name) = c.chord {
        pf.push(Postfix::Chord(name.to_string()));
    }
    if weight > 1 {
        pf.push(Postfix::Weight(weight as f64));
    }
    if pf.is_empty() {
        c.atom.clone()
    } else {
        mini(MiniKind::Term {
            atom: Box::new(c.atom.clone()),
            postfixes: pf,
        })
    }
}

fn rest_term(weight: u32) -> Mini {
    if weight > 1 {
        mini(MiniKind::Term {
            atom: Box::new(mini(MiniKind::Rest)),
            postfixes: vec![Postfix::Weight(weight as f64)],
        })
    } else {
        mini(MiniKind::Rest)
    }
}

fn grid_size(lane: &[&Cell]) -> i64 {
    let mut n = 1;
    for c in lane {
        n = lcm(n, c.start.den());
        n = lcm(n, c.end.den());
    }
    n
}

fn slot_of(t: Time, n: i64) -> i64 {
    (t * Time::int(n)).num()
}

// ── Loop collapse (one segment's bars → one mini) ───────────────────────────────

/// Collapse a run of per-cycle minis into one body. If `detect_loops`, find the
/// smallest repeating period and keep one copy: a single repeating cycle stays
/// bare (it loops on its own), a longer period becomes `<...>` alternation.
fn collapse(cycles: Vec<Mini>, detect_loops: bool) -> Mini {
    if cycles.is_empty() {
        return mini(MiniKind::Rest);
    }
    let p = if detect_loops {
        period(&cycles)
    } else {
        cycles.len()
    }
    .max(1);

    if cycles.len() == 1 || p == 1 {
        return cycles.into_iter().next().unwrap();
    }
    let elems: Vec<Mini> = cycles.into_iter().take(p).map(alt_element).collect();
    mini(MiniKind::Alt(Box::new(mini(MiniKind::Sequence(elems)))))
}

/// Smallest `p` dividing the length with `cycles[i] == cycles[i % p]`.
fn period(cycles: &[Mini]) -> usize {
    let len = cycles.len();
    for p in 1..=len {
        if len % p == 0 && (0..len).all(|i| cycles[i] == cycles[i % p]) {
            return p;
        }
    }
    len
}

/// Wrap a composite cycle body so it is one element of `<...>` (a single
/// leaf/term needs no brackets).
fn alt_element(m: Mini) -> Mini {
    match m.kind {
        MiniKind::Sequence(_) | MiniKind::Parallel(_) => mini(MiniKind::Group(Box::new(m))),
        _ => m,
    }
}

// ── Leaves ──────────────────────────────────────────────────────────────────────

fn note_leaf(pitch: i32, use_degrees: bool, key: Option<&DetectedKey>, ref_octave: i32) -> Mini {
    if use_degrees {
        if let Some(k) = key {
            if let Some(d) = degree_of(k, pitch, ref_octave) {
                return leaf(Leaf::Degree(d));
            }
        }
    }
    leaf(Leaf::NoteName(note_name(pitch)))
}

/// MIDI semitone → merula note name (`60 → c4`, `63 → ef4`); octave always
/// written so the value is unambiguous.
fn note_name(midi: i32) -> String {
    const N: [&str; 12] = [
        "c", "cs", "d", "ef", "e", "f", "fs", "g", "af", "a", "bf", "b",
    ];
    let pc = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;
    format!("{}{}", N[pc], octave)
}

// ── Small numerics ──────────────────────────────────────────────────────────────

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

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Note;
    use merula_lang::prelude::parse;

    fn note(start: Time, dur: Time, pitch: i32) -> Note {
        Note {
            start,
            dur,
            pitch,
            vel: 0.8,
        }
    }

    fn render(tracks: Vec<NoteTrack>, len: u32) -> String {
        let song = Song {
            tracks,
            cps: 0.5,
            len_cycles: len,
        };
        song_to_merula(&song, &ImportOptions::default())
    }

    /// A pitched track whose bar `cyc` holds one quarter-note of `pitch`.
    fn bar(cyc: i64, pitch: i32) -> Note {
        note(Time::int(cyc), Time::new(1, 4), pitch)
    }

    #[test]
    fn output_always_reparses() {
        let src = render(
            vec![NoteTrack {
                name: "lead".into(),
                is_drum: false,
                notes: vec![
                    note(Time::ZERO, Time::new(1, 4), 60),
                    note(Time::new(1, 4), Time::new(1, 4), 62),
                    note(Time::new(1, 2), Time::new(1, 2), 64),
                ],
            }],
            1,
        );
        assert!(parse(&src).is_ok(), "emitted source must re-parse:\n{src}");
        assert!(src.contains("cps("));
    }

    #[test]
    fn folds_a_triad_into_a_chord_symbol() {
        let triad = |p: i32| note(Time::ZERO, Time::ONE, p);
        let src = render(
            vec![NoteTrack {
                name: "keys".into(),
                is_drum: false,
                notes: vec![triad(60), triad(64), triad(67)],
            }],
            1,
        );
        assert!(parse(&src).is_ok());
        assert!(src.contains("'maj"), "expected a chord symbol in:\n{src}");
    }

    #[test]
    fn fitting_track_emits_degrees_and_scale() {
        // C major pentatonic melody → degrees + .scale(...).
        let mel: Vec<Note> = [60, 62, 64, 67]
            .iter()
            .enumerate()
            .map(|(i, &p)| note(Time::new(i as i64, 4), Time::new(1, 4), p))
            .collect();
        let src = render(
            vec![NoteTrack {
                name: "mel".into(),
                is_drum: false,
                notes: mel,
            }],
            1,
        );
        assert!(parse(&src).is_ok());
        assert!(src.contains(".scale("), "expected a scale call in:\n{src}");
    }

    #[test]
    fn drum_track_emits_sound_island() {
        let src = render(
            vec![NoteTrack {
                name: "drums".into(),
                is_drum: true,
                notes: vec![
                    note(Time::ZERO, Time::new(1, 4), 36),      // bd
                    note(Time::new(1, 2), Time::new(1, 4), 38), // sn
                ],
            }],
            1,
        );
        assert!(parse(&src).is_ok());
        assert!(src.contains("s("), "expected a sound island in:\n{src}");
        assert!(src.contains("bd"));
    }

    #[test]
    fn identical_cycles_collapse_to_one() {
        // Two identical bars → a single bare island (loops on its own).
        let src = render(
            vec![NoteTrack {
                name: "p".into(),
                is_drum: false,
                notes: vec![bar(0, 60), bar(1, 60)],
            }],
            2,
        );
        assert!(parse(&src).is_ok());
        assert!(
            !src.contains("arrange"),
            "identical bars should not arrange:\n{src}"
        );
        assert!(!src.contains('<'), "identical bars should not alternate:\n{src}");
    }

    #[test]
    fn repeated_phrase_is_factored_into_a_let() {
        // Eight bars = a four-bar phrase played twice → one `let`, reused twice.
        let pitches = [60, 62, 64, 65];
        let mut notes = Vec::new();
        for rep in 0..2 {
            for (i, &p) in pitches.iter().enumerate() {
                notes.push(bar(rep * 4 + i as i64, p));
            }
        }
        let src = render(
            vec![NoteTrack {
                name: "lead".into(),
                is_drum: false,
                notes,
            }],
            8,
        );
        assert!(parse(&src).is_ok(), "emitted source must re-parse:\n{src}");
        assert!(src.contains("arrange("), "expected an arrangement in:\n{src}");
        assert!(src.contains("let phrase1 ="), "expected a phrase let in:\n{src}");
        // The phrase is bound once and referenced twice in the arrangement.
        assert_eq!(src.matches("let phrase1 =").count(), 1);
        assert_eq!(src.matches("phrase1,").count() + src.matches("phrase1)").count(), 2);
    }

    #[test]
    fn distinct_long_track_breaks_into_sections() {
        // Six all-distinct bars → split into phrases (no giant inline pattern).
        let notes: Vec<Note> = (0..6).map(|c| bar(c, 60 + c as i32)).collect();
        let src = render(
            vec![NoteTrack {
                name: "lead".into(),
                is_drum: false,
                notes,
            }],
            6,
        );
        assert!(parse(&src).is_ok(), "emitted source must re-parse:\n{src}");
        assert!(src.contains("arrange("), "expected an arrangement in:\n{src}");
        assert!(src.contains("section("), "expected named sections in:\n{src}");
    }
}
