//! Composition: constructors and the combinators that build patterns out of
//! other patterns (`pure`/`silence`/`stack`/`fastcat`/`slowcat`/`arrange`/
//! `tracks`).
//!
//! `par`/`seq`/`cat` are the nemus host-language names; they are thin aliases
//! over the descriptive `stack`/`fastcat`/`slowcat` so there is one
//! implementation per concept.
//!
//! These take a `Vec` (the "list" form). The varargs sugar (`par(a, b)`) is a
//! language-layer concern (Fase 1); the algebra only needs the list.

use crate::hap::Hap;
use crate::pattern::Pattern;
use crate::span::TimeSpan;
use crate::time::Time;

/// A pattern that plays `value` once per cycle (the atom).
pub fn pure<T: Clone + Send + Sync + 'static>(value: T) -> Pattern<T> {
    Pattern::new(move |span: TimeSpan| {
        span.split_cycles()
            .into_iter()
            .filter_map(|s| {
                let whole = TimeSpan::cycle(s.begin.floor());
                whole.sect(s).map(|part| Hap::new(Some(whole), part, value.clone()))
            })
            .collect()
    })
}

/// The empty pattern — never produces an event.
pub fn silence<T: Clone + Send + Sync + 'static>() -> Pattern<T> {
    Pattern::new(|_span| Vec::new())
}

/// Overlay all patterns so they sound simultaneously (polyphony). Host: `&`.
pub fn stack<T: Clone + Send + Sync + 'static>(pats: Vec<Pattern<T>>) -> Pattern<T> {
    Pattern::new(move |span| pats.iter().flat_map(|p| p.query(span)).collect())
}

/// Alias of [`stack`] under the nemus host name.
pub fn par<T: Clone + Send + Sync + 'static>(pats: Vec<Pattern<T>>) -> Pattern<T> {
    stack(pats)
}

/// Play one pattern per cycle, alternating and looping (absolute timeline).
/// Host: `< >`.
pub fn slowcat<T: Clone + Send + Sync + 'static>(pats: Vec<Pattern<T>>) -> Pattern<T> {
    if pats.is_empty() {
        return silence();
    }
    Pattern::new(move |span: TimeSpan| {
        let cyc = span.begin.floor();
        let n = pats.len() as i64;
        let i = cyc.rem_euclid(n) as usize;
        // Pattern `i` should be on its own running cycle (`cyc / n`); shift the
        // query back so it sees that cycle, then shift the results forward.
        let shift = Time::int(cyc - cyc.div_euclid(n));
        let qs = span.with_time(|t| t - shift);
        pats[i]
            .query(qs)
            .into_iter()
            .map(|h| h.map_time(|t| t + shift))
            .collect()
    })
    .split_queries()
}

/// Alias of [`slowcat`] under the nemus host name.
pub fn cat<T: Clone + Send + Sync + 'static>(pats: Vec<Pattern<T>>) -> Pattern<T> {
    slowcat(pats)
}

/// Lay the patterns in equal slots inside one cycle. Host: *(space)*.
pub fn fastcat<T: Clone + Send + Sync + 'static>(pats: Vec<Pattern<T>>) -> Pattern<T> {
    let n = pats.len();
    if n == 0 {
        return silence();
    }
    slowcat(pats).fast(Time::int(n as i64))
}

/// Alias of [`fastcat`] under the nemus host name.
pub fn seq<T: Clone + Send + Sync + 'static>(pats: Vec<Pattern<T>>) -> Pattern<T> {
    fastcat(pats)
}

/// Lay patterns in **weighted** slots inside one cycle: slot `i` spans the
/// fraction `weight_i / Σweights` of the cycle. With all weights equal this is
/// exactly [`fastcat`]; the weighted form is what the language layer needs for
/// mini-notation's `@n` (elongate) and `_` (extend the previous slot).
///
/// Each slot plays **one cycle** of its pattern, compressed into the slot and
/// silent outside it (Tidal's `timeCat`). Zero total weight → silence.
pub fn timecat<T: Clone + Send + Sync + 'static>(weighted: Vec<(u32, Pattern<T>)>) -> Pattern<T> {
    let total: i64 = weighted.iter().map(|(w, _)| *w as i64).sum();
    if total == 0 {
        return silence();
    }
    // Precompute each slot's sub-arc within the unit cycle [0, 1).
    let mut slots: Vec<(Time, Time, Pattern<T>)> = Vec::with_capacity(weighted.len());
    let mut acc = 0i64;
    for (w, p) in weighted {
        let begin = Time::new(acc, total);
        acc += w as i64;
        let end = Time::new(acc, total);
        slots.push((begin, end, p));
    }
    Pattern::new(move |span: TimeSpan| {
        // `split_queries` guarantees a single-cycle span, so this floor is exact.
        let cyc = Time::int(span.begin.floor());
        let mut out = Vec::new();
        for (b, e, p) in &slots {
            let width = *e - *b;
            if width == Time::ZERO {
                continue; // zero-weight slot (guarded; total > 0)
            }
            let slot_begin = cyc + *b;
            let slot = TimeSpan::new(slot_begin, cyc + *e);
            let Some(qpart) = span.sect(slot) else {
                continue;
            };
            // Map slot-space ↔ the pattern's own cycle [0, 1).
            let to_inner = move |t: Time| (t - slot_begin) / width;
            let from_inner = move |t: Time| slot_begin + t * width;
            for h in p.query(qpart.with_time(to_inner)) {
                out.push(h.map_time(from_inner));
            }
        }
        out
    })
    .split_queries()
}

/// Polymeter (`{a b c}%n`): overlay lanes that all advance at `steps` slots per
/// cycle but each loop through its **own** length, so lanes of different lengths
/// drift against each other (Tidal's `polymeter`).
///
/// Each lane is given as `(len, pattern)` where `pattern` is the lane's
/// `len`-item sequence (a [`fastcat`], so it already plays `len` items/cycle).
/// Re-timing it to `steps` items/cycle is `fast(steps / len)`; the lane's own
/// `slowcat`/`fast` construction makes it keep cycling through its `len` items.
/// `steps == 0` or a zero-length lane → that lane is silent.
pub fn polymeter<T: Clone + Send + Sync + 'static>(
    steps: u32,
    lanes: Vec<(u32, Pattern<T>)>,
) -> Pattern<T> {
    if steps == 0 {
        return silence();
    }
    let retimed = lanes
        .into_iter()
        .map(|(len, lane)| {
            if len == 0 {
                silence()
            } else {
                lane.fast(Time::new(steps as i64, len as i64))
            }
        })
        .collect();
    stack(retimed)
}

/// A timeline section: `pattern` occupies `cycles` whole cycles. Produced by
/// [`cycles`] (anonymous) or [`section`] (named), consumed by [`arrange`].
#[derive(Clone, Debug)]
pub struct Section<T> {
    /// Display name (`section("INTRO", …)`), or `None` for a bare [`cycles`].
    pub name: Option<String>,
    pub cycles: u32,
    pub pattern: Pattern<T>,
}

/// Mark that `pattern` occupies `n` cycles in an [`arrange`] (unnamed).
pub fn cycles<T: Clone + Send + Sync + 'static>(n: u32, pattern: Pattern<T>) -> Section<T> {
    Section { name: None, cycles: n, pattern }
}

/// A **named** arrange section (`section("INTRO", n, pattern)`) — identical to
/// [`cycles`] for playback, but carries a label the arrangement view shows as a
/// band. The names surface via [`section_layout`] on the owning [`Track`].
pub fn section<T: Clone + Send + Sync + 'static>(
    name: impl Into<String>,
    n: u32,
    pattern: Pattern<T>,
) -> Section<T> {
    Section { name: Some(name.into()), cycles: n, pattern }
}

/// A named span on a track's timeline (one named arrange [`section`]). Cycles are
/// within a single arrangement period; the arrangement loops, so the view tiles
/// the layout across the timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionSpan {
    pub name: String,
    /// First cycle of the section (inclusive).
    pub start: u32,
    /// One past the last cycle (exclusive).
    pub end: u32,
}

/// The named-section layout of a section list: the absolute cycle range of each
/// `section(...)` (bare `cycles(...)` are skipped — they have no label). Computed
/// alongside [`arrange`] so a track can expose where its sections fall.
pub fn section_layout<T>(sections: &[Section<T>]) -> Vec<SectionSpan> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    for s in sections {
        if let Some(name) = &s.name {
            out.push(SectionSpan { name: name.clone(), start: acc, end: acc + s.cycles });
        }
        acc += s.cycles;
    }
    out
}

/// Concatenate sections on the absolute timeline; the whole arrangement loops
/// at the total length. Each loop restarts every section at its own cycle 0.
pub fn arrange<T: Clone + Send + Sync + 'static>(sections: Vec<Section<T>>) -> Pattern<T> {
    let total: i64 = sections.iter().map(|s| s.cycles as i64).sum();
    if total == 0 {
        return silence();
    }
    Pattern::new(move |span: TimeSpan| {
        let cyc = span.begin.floor();
        let m = cyc.rem_euclid(total); // offset within the (looping) arrangement
        let mut acc = 0i64;
        for s in &sections {
            let n = s.cycles as i64;
            if m < acc + n {
                let local = m - acc; // section-local cycle, resets each loop
                let shift = Time::int(cyc - local);
                let qs = span.with_time(|t| t - shift);
                return s
                    .pattern
                    .query(qs)
                    .into_iter()
                    .map(|h| h.map_time(|t| t + shift))
                    .collect();
            }
            acc += n;
        }
        Vec::new()
    })
    .split_queries()
}

/// One named channel of the output (a mixer strip).
#[derive(Clone, Debug)]
pub struct Track<T> {
    pub name: String,
    pub pattern: Pattern<T>,
    /// Named-section layout when the track's pattern is an [`arrange`] of
    /// [`section`]s (else empty). Spans are within one arrangement period; the
    /// arrangement loops, so the view tiles them by [`Track::period`]. Drives the
    /// arrangement-view bands; does not affect playback.
    pub sections: Vec<SectionSpan>,
    /// Total cycles of the track's arrangement (the loop period), `0` when the
    /// track isn't an arrangement. Lets the view tile [`Track::sections`] across
    /// the timeline.
    pub period: u32,
}

/// Build a named track (no section layout).
pub fn track<T: Clone + Send + Sync + 'static>(
    name: impl Into<String>,
    pattern: Pattern<T>,
) -> Track<T> {
    Track {
        name: name.into(),
        pattern,
        sections: Vec::new(),
        period: 0,
    }
}

/// Build a named track that carries an arrangement's named-section layout (from
/// [`section_layout`]) + loop `period` (total cycles), for the view.
pub fn track_with_sections<T: Clone + Send + Sync + 'static>(
    name: impl Into<String>,
    pattern: Pattern<T>,
    sections: Vec<SectionSpan>,
    period: u32,
) -> Track<T> {
    Track {
        name: name.into(),
        pattern,
        sections,
        period,
    }
}

/// The output of a `.nemus`: a list of named channels.
#[derive(Clone, Debug)]
pub struct Tracks<T> {
    pub tracks: Vec<Track<T>>,
}

/// Build the track list output.
pub fn tracks<T: Clone + Send + Sync + 'static>(tracks: Vec<Track<T>>) -> Tracks<T> {
    Tracks { tracks }
}

impl<T: Clone + Send + Sync + 'static> Tracks<T> {
    /// Query every track, returning its haps tagged with the track name.
    pub fn query(&self, span: TimeSpan) -> Vec<(String, Vec<Hap<T>>)> {
        self.tracks
            .iter()
            .map(|t| (t.name.clone(), t.pattern.query(span)))
            .collect()
    }

    /// Collapse all tracks into a single overlaid pattern (anonymous mix).
    pub fn mixed(&self) -> Pattern<T> {
        stack(self.tracks.iter().map(|t| t.pattern.clone()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_cycle<T: Clone + Send + Sync + 'static>(p: &Pattern<T>) -> Vec<Hap<T>> {
        p.query(TimeSpan::cycle(0))
    }

    #[test]
    fn fastcat_splits_cycle_into_equal_slots() {
        let p = fastcat(vec![pure("a"), pure("b"), pure("c")]);
        let haps = first_cycle(&p);
        assert_eq!(haps.len(), 3);
        let third = Time::new(1, 3);
        assert_eq!(haps[0].whole.unwrap(), TimeSpan::new(Time::ZERO, third));
        assert_eq!(haps[1].whole.unwrap(), TimeSpan::new(third, third + third));
        assert_eq!(haps[2].whole.unwrap(), TimeSpan::new(third + third, Time::ONE));
        assert_eq!(haps[0].value, "a");
        assert_eq!(haps[2].value, "c");
    }

    #[test]
    fn slowcat_one_per_cycle_and_loops() {
        let p = slowcat(vec![pure("a"), pure("b")]);
        assert_eq!(p.query(TimeSpan::cycle(0))[0].value, "a");
        assert_eq!(p.query(TimeSpan::cycle(1))[0].value, "b");
        assert_eq!(p.query(TimeSpan::cycle(2))[0].value, "a"); // wraps
        assert_eq!(p.query(TimeSpan::cycle(3))[0].value, "b");
    }

    fn sorted_by_onset<T: Clone + Send + Sync + 'static>(p: &Pattern<T>) -> Vec<Hap<T>> {
        let mut haps = first_cycle(p);
        haps.sort_by_key(|h| h.part.begin);
        haps
    }

    #[test]
    fn timecat_gives_each_slot_its_weighted_share() {
        // weights 3 and 1 → "a" fills the first 3/4, "b" the last 1/4.
        let p = timecat(vec![(3, pure("a")), (1, pure("b"))]);
        let haps = sorted_by_onset(&p);
        assert_eq!(haps.len(), 2);
        assert_eq!(haps[0].value, "a");
        assert_eq!(haps[0].whole.unwrap(), TimeSpan::new(Time::ZERO, Time::new(3, 4)));
        assert_eq!(haps[1].value, "b");
        assert_eq!(haps[1].whole.unwrap(), TimeSpan::new(Time::new(3, 4), Time::ONE));
    }

    #[test]
    fn timecat_with_equal_weights_matches_fastcat() {
        let weighted = timecat(vec![(1, pure("a")), (1, pure("b")), (1, pure("c"))]);
        let equal = fastcat(vec![pure("a"), pure("b"), pure("c")]);
        let a = sorted_by_onset(&weighted);
        let b = sorted_by_onset(&equal);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.value, y.value);
            assert_eq!(x.whole, y.whole);
        }
    }

    #[test]
    fn stack_overlays() {
        let p = stack(vec![pure("a"), pure("b")]);
        let haps = first_cycle(&p);
        assert_eq!(haps.len(), 2);
    }

    #[test]
    fn polymeter_steps_each_lane_at_the_global_rate() {
        // {a b c, d e}%2 → both lanes play 2 items/cycle, drifting against each
        // other. Lane 1 (len 3) at 2 steps: cycle 0 → a,b; lane 2 (len 2) → d,e.
        let lane1 = fastcat(vec![pure("a"), pure("b"), pure("c")]);
        let lane2 = fastcat(vec![pure("d"), pure("e")]);
        let p = polymeter(2, vec![(3, lane1), (2, lane2)]);
        let mut c0: Vec<_> = sorted_by_onset(&p)
            .into_iter()
            .map(|h| (h.part.begin, h.value))
            .collect();
        c0.sort_by_key(|(t, _)| *t);
        // 2 slots × 2 lanes = 4 onsets in cycle 0.
        assert_eq!(c0.len(), 4);
        // First slot of each lane is its first item.
        assert!(c0.iter().any(|(t, v)| *t == Time::ZERO && *v == "a"));
        assert!(c0.iter().any(|(t, v)| *t == Time::ZERO && *v == "d"));
    }

    #[test]
    fn polymeter_default_steps_zero_is_silent() {
        let p = polymeter(0, vec![(2, fastcat(vec![pure("a"), pure("b")]))]);
        assert!(p.query(TimeSpan::cycle(0)).is_empty());
    }

    #[test]
    fn section_layout_records_named_spans_skipping_anonymous() {
        // intro (named, 4) · build (anonymous, 2) · drop (named, 8)
        let sections = vec![
            section("INTRO", 4, pure("i")),
            cycles(2, pure("b")),
            section("DROP", 8, pure("d")),
        ];
        let layout = section_layout(&sections);
        assert_eq!(
            layout,
            vec![
                SectionSpan { name: "INTRO".into(), start: 0, end: 4 },
                SectionSpan { name: "DROP".into(), start: 6, end: 14 },
            ]
        );
    }

    #[test]
    fn named_section_plays_like_cycles() {
        // A named section is identical to `cycles` for playback.
        let named = arrange(vec![section("A", 2, pure("x")), section("B", 1, pure("y"))]);
        let anon = arrange(vec![cycles(2, pure("x")), cycles(1, pure("y"))]);
        for c in 0..6 {
            assert_eq!(
                named.query(TimeSpan::cycle(c))[0].value,
                anon.query(TimeSpan::cycle(c))[0].value,
            );
        }
    }

    #[test]
    fn arrange_places_sections_then_loops() {
        // intro: 2 cycles of "i", main: 1 cycle of "m"  → period 3
        let p = arrange(vec![cycles(2, pure("i")), cycles(1, pure("m"))]);
        let v = |c: i64| p.query(TimeSpan::cycle(c))[0].value;
        assert_eq!(v(0), "i");
        assert_eq!(v(1), "i");
        assert_eq!(v(2), "m");
        assert_eq!(v(3), "i"); // loop restarts the arrangement
        assert_eq!(v(5), "m");
    }

    #[test]
    fn tracks_query_and_mix() {
        let t = tracks(vec![track("a", pure(1)), track("b", pure(2))]);
        let per = t.query(TimeSpan::cycle(0));
        assert_eq!(per.len(), 2);
        assert_eq!(per[0].0, "a");
        assert_eq!(t.mixed().query(TimeSpan::cycle(0)).len(), 2);
    }
}
