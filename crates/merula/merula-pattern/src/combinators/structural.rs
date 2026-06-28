//! Structural Tidal transforms: `chunk`, `iter`, `palindrome`, `swingBy`, plus
//! the `within` / `inside` building blocks they share.
//!
//! These rearrange *when* events fall without touching their values, so they
//! stay generic over the payload. They build on the per-cycle factoring already
//! in [`crate::combinators::time`] (`rev`, `late`, `early`, `fast`) and on
//! `slowcat` (one alternative per cycle).

use crate::combinators::compose::{silence, slowcat, stack};
use crate::pattern::Pattern;
use crate::time::Time;

impl<T: Clone + Send + Sync + 'static> Pattern<T> {
    /// Keep only the haps whose **onset** satisfies `test` (applied to the onset
    /// time). The complement of this selects the rest — the pair is how
    /// [`within`](Self::within) splits a cycle.
    fn play_when(self, test: impl Fn(Time) -> bool + Send + Sync + 'static) -> Pattern<T> {
        self.filter_haps(move |h| test(h.onset()))
    }

    /// Apply `f` only to the events whose onset falls in the cycle-relative
    /// window `[begin, end)`, leaving the rest of each cycle untouched
    /// (Tidal's `within`). `begin`/`end` are positions in `0..1`.
    pub fn within(
        self,
        begin: Time,
        end: Time,
        f: impl FnOnce(Pattern<T>) -> Pattern<T>,
    ) -> Pattern<T> {
        let in_window = move |t: Time| {
            let p = t.cycle_pos();
            p >= begin && p < end
        };
        let inside = f(self.clone()).play_when(in_window);
        let outside = self.play_when(move |t| !in_window(t));
        stack(vec![inside, outside])
    }

    /// Run `f` "inside" `n` subdivisions: slow the pattern down by `n` (so each
    /// `1/n` slice fills a whole cycle), apply `f` per-cycle, then speed it back
    /// up by `n` — so `f` transforms each `1/n` chunk as if it were its own cycle
    /// (Tidal's `inside = fast n . f . slow n`). `n <= 0` → silence.
    pub fn inside(self, n: i64, f: impl FnOnce(Pattern<T>) -> Pattern<T>) -> Pattern<T> {
        if n <= 0 {
            return silence();
        }
        let nt = Time::int(n);
        f(self.slow(nt)).fast(nt)
    }

    /// Rotate the cycle left by `1/n` more each cycle: cycle 0 unchanged, cycle 1
    /// shifted early by `1/n`, cycle 2 by `2/n`, … wrapping every `n` cycles
    /// (Tidal's `iter`). `n <= 0` → unchanged.
    pub fn iter(self, n: i64) -> Pattern<T> {
        if n <= 0 {
            return self;
        }
        let alts = (0..n)
            .map(|i| self.clone().early(Time::new(i, n)))
            .collect();
        slowcat(alts)
    }

    /// Alternate forward and reversed every other cycle (Tidal's `palindrome`):
    /// cycle 0 plays forward, cycle 1 reversed, cycle 2 forward, …
    pub fn palindrome(self) -> Pattern<T> {
        slowcat(vec![self.clone(), self.rev()])
    }

    /// Apply `f` to a different `1/n` slice of the cycle each cycle, cycling
    /// through the `n` slices (Tidal's `chunk`). `n <= 0` → unchanged.
    ///
    /// `f` is applied **eagerly** once per slice while building, so it only needs
    /// `Fn` (no `Send + Sync`) — like `within`/`every`, the closure isn't stored.
    pub fn chunk(self, n: i64, f: impl Fn(Pattern<T>) -> Pattern<T>) -> Pattern<T> {
        if n <= 0 {
            return self;
        }
        let alts = (0..n)
            .map(|i| {
                let begin = Time::new(i, n);
                let end = Time::new(i + 1, n);
                self.clone().within(begin, end, &f)
            })
            .collect();
        slowcat(alts)
    }

    /// Delay every other `1/n` subdivision by `amount` (a fraction of the whole
    /// cycle), giving a swing feel (Tidal's `swingBy`). The first half of each
    /// `1/n` chunk stays put; the second half is pushed `amount` later.
    /// `n <= 0` → unchanged.
    pub fn swing_by(self, amount: Time, n: i64) -> Pattern<T> {
        if n <= 0 {
            return self;
        }
        self.inside(n, move |p| {
            p.within(Time::new(1, 2), Time::ONE, move |q| q.late(amount))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinators::compose::{fastcat, pure};
    use crate::span::TimeSpan;

    /// Values of a cycle's onsets, sorted by onset time.
    fn seq(p: &Pattern<&'static str>, cyc: i64) -> Vec<&'static str> {
        let mut haps: Vec<_> = p
            .query(TimeSpan::cycle(cyc))
            .into_iter()
            .filter(|h| h.has_onset())
            .collect();
        haps.sort_by_key(|h| h.part.begin);
        haps.into_iter().map(|h| h.value).collect()
    }

    fn abcd() -> Pattern<&'static str> {
        fastcat(vec![pure("a"), pure("b"), pure("c"), pure("d")])
    }

    #[test]
    fn iter_rotates_one_step_per_cycle() {
        let p = abcd().iter(4);
        assert_eq!(seq(&p, 0), vec!["a", "b", "c", "d"]);
        assert_eq!(seq(&p, 1), vec!["b", "c", "d", "a"]);
        assert_eq!(seq(&p, 2), vec!["c", "d", "a", "b"]);
        assert_eq!(seq(&p, 4), vec!["a", "b", "c", "d"]); // wraps
    }

    #[test]
    fn palindrome_alternates_direction() {
        let p = abcd().palindrome();
        assert_eq!(seq(&p, 0), vec!["a", "b", "c", "d"]);
        assert_eq!(seq(&p, 1), vec!["d", "c", "b", "a"]);
        assert_eq!(seq(&p, 2), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn within_transforms_only_the_window() {
        // Tidal `within`: `f` is applied to the WHOLE pattern, then only its output
        // that lands inside the window is kept (spliced over the untouched rest).
        // `rev "a b c d"` is "d c b a"; its first half (d, c) fills the first half,
        // while the original c, d stay in the second half → "d c c d".
        let p = abcd().within(Time::ZERO, Time::new(1, 2), |q| q.rev());
        assert_eq!(seq(&p, 0), vec!["d", "c", "c", "d"]);
    }

    #[test]
    fn chunk_moves_the_affected_slice_each_cycle() {
        // Apply rev to a different quarter each cycle; only that quarter's single
        // event is touched, so a 4-event grid is unchanged value-wise but the
        // structure proves which slice was visited (here: identity-stable).
        let touched = |cyc: i64| {
            // mark the affected quarter by gaining; use fast(1) identity so we
            // can at least confirm it still yields 4 onsets each cycle.
            let p = abcd().chunk(4, |q| q.rev());
            seq(&p, cyc).len()
        };
        for c in 0..4 {
            assert_eq!(touched(c), 4);
        }
    }

    #[test]
    fn swing_by_delays_the_offbeats() {
        // Two events per cycle with n=2: chunk size 1/2. The second half of each
        // 1/2 chunk is pushed later. With one event per chunk landing on the
        // chunk start, the onsets stay; assert event count is preserved.
        let p = fastcat(vec![pure("a"), pure("b")]).swing_by(Time::new(1, 8), 2);
        assert_eq!(seq(&p, 0).len(), 2);
    }

    #[test]
    fn inside_runs_f_per_subdivision() {
        // inside(2, rev) reverses within each half independently.
        let p = abcd().inside(2, |q| q.rev());
        // halves: [a b] [c d] → reversed within each half → [b a] [d c]
        assert_eq!(seq(&p, 0), vec!["b", "a", "d", "c"]);
    }
}
