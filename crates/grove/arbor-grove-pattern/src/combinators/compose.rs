//! Composition: constructors and the combinators that build patterns out of
//! other patterns (`pure`/`silence`/`stack`/`fastcat`/`slowcat`/`arrange`/
//! `tracks`).
//!
//! `par`/`seq`/`cat` are the grove host-language names; they are thin aliases
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

/// Alias of [`stack`] under the grove host name.
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

/// Alias of [`slowcat`] under the grove host name.
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

/// Alias of [`fastcat`] under the grove host name.
pub fn seq<T: Clone + Send + Sync + 'static>(pats: Vec<Pattern<T>>) -> Pattern<T> {
    fastcat(pats)
}

/// A timeline section: `pattern` occupies `cycles` whole cycles. Produced by
/// [`cycles`], consumed by [`arrange`].
#[derive(Clone, Debug)]
pub struct Section<T> {
    pub cycles: u32,
    pub pattern: Pattern<T>,
}

/// Mark that `pattern` occupies `n` cycles in an [`arrange`].
pub fn cycles<T: Clone + Send + Sync + 'static>(n: u32, pattern: Pattern<T>) -> Section<T> {
    Section { cycles: n, pattern }
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
}

/// Build a named track.
pub fn track<T: Clone + Send + Sync + 'static>(
    name: impl Into<String>,
    pattern: Pattern<T>,
) -> Track<T> {
    Track {
        name: name.into(),
        pattern,
    }
}

/// The output of a `.grove`: a list of named channels.
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

    #[test]
    fn stack_overlays() {
        let p = stack(vec![pure("a"), pure("b")]);
        let haps = first_cycle(&p);
        assert_eq!(haps.len(), 2);
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
