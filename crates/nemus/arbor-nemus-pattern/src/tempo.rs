//! Tempo automation — a **piecewise-constant tempo map**.
//!
//! The clock is global and constant within a segment; `cps` changes at whole-cycle
//! boundaries (`design/nemus/semantics.md`: the clock is absolute and re-anchors,
//! it never resets). A [`TempoMap`] is the scripted sequence of those changes,
//! built from the language's `tempo(cycles(n, cps), …)` and consumed by the
//! engine's transport, which re-anchors its clock at each boundary.
//!
//! It lives in the pattern crate because it is the shared vocabulary between
//! `lang` (which produces it) and `engine` (which plays it), and neither depends
//! on the other. Continuous tempo curves / rubato (smooth sub-cycle tempo) are a
//! future extension — see `design/nemus/roadmap.md`.

/// A piecewise-constant tempo automation over the looping timeline.
///
/// `points` are `(start_cycle, cps)` anchors, sorted ascending with the first at
/// cycle `0`; the map repeats every [`period`](TempoMap::period) cycles. An empty
/// map means "no automation" — the clock runs at a single constant `cps`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TempoMap {
    /// `(start_cycle, cps)` anchors, sorted, first at cycle 0. Empty = constant.
    pub points: Vec<(u32, f64)>,
    /// Loop length in cycles (sum of segment durations); `0` when empty.
    pub period: u32,
}

impl TempoMap {
    /// An empty map (no automation — the transport keeps its constant `cps`).
    pub fn none() -> Self {
        TempoMap::default()
    }

    /// Whether this map carries no automation.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty() || self.period == 0
    }

    /// Build from `(duration_cycles, cps)` segments — the
    /// `tempo(cycles(n, cps), …)` form. Zero-duration segments are kept as
    /// anchors but contribute nothing to the period. An empty input is the empty
    /// map.
    pub fn from_segments(segments: &[(u32, f64)]) -> Self {
        if segments.is_empty() {
            return TempoMap::none();
        }
        let mut points = Vec::with_capacity(segments.len());
        let mut acc: u32 = 0;
        for &(dur, cps) in segments {
            points.push((acc, cps));
            acc = acc.saturating_add(dur);
        }
        TempoMap { points, period: acc }
    }

    /// The `cps` in force at absolute `cycle` (looping by [`period`](Self::period)),
    /// or `None` when the map is empty.
    pub fn cps_at(&self, cycle: i64) -> Option<f64> {
        if self.is_empty() {
            return None;
        }
        let m = cycle.rem_euclid(self.period as i64) as u32;
        // points are sorted by start; take the last one starting at or before `m`.
        let mut cps = self.points[0].1;
        for &(start, c) in &self.points {
            if start <= m {
                cps = c;
            } else {
                break;
            }
        }
        Some(cps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_segments_accumulates_starts_and_period() {
        let m = TempoMap::from_segments(&[(8, 0.5), (8, 0.6), (4, 0.45)]);
        assert_eq!(m.points, vec![(0, 0.5), (8, 0.6), (16, 0.45)]);
        assert_eq!(m.period, 20);
    }

    #[test]
    fn cps_at_picks_the_active_segment_and_loops() {
        let m = TempoMap::from_segments(&[(8, 0.5), (8, 0.6)]); // period 16
        assert_eq!(m.cps_at(0), Some(0.5));
        assert_eq!(m.cps_at(7), Some(0.5));
        assert_eq!(m.cps_at(8), Some(0.6));
        assert_eq!(m.cps_at(15), Some(0.6));
        assert_eq!(m.cps_at(16), Some(0.5)); // wraps
        assert_eq!(m.cps_at(24), Some(0.6));
    }

    #[test]
    fn empty_map_has_no_tempo() {
        assert!(TempoMap::none().is_empty());
        assert_eq!(TempoMap::none().cps_at(3), None);
        assert!(TempoMap::from_segments(&[]).is_empty());
    }
}
