//! Counters with a memory that fades: how often a source of suggestions was right, in one context.
//!
//! Each count is a weight, halved every `HALF_LIFE_DAYS`, so a habit you dropped last spring stops counting
//! without a reset, and a rate needs `MIN_SAMPLES` of weight before it says anything — a counter that has seen
//! two events knows nothing.

use serde::{Deserialize, Serialize};

use crate::edit::Outcome;

/// Weight halves after this long.
pub const HALF_LIFE_DAYS: f32 = 45.0;
/// Total weight below which no rate is given.
pub const MIN_SAMPLES: f32 = 5.0;

const DAY: f32 = 86_400.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tally {
    pub ignored: f32,
    pub kept: f32,
    /// Right in content, laid out differently afterwards: counted as right, and kept apart to learn the layout from.
    pub reformatted: f32,
    pub shortened: f32,
    pub extended: f32,
    pub replaced: f32,
    pub undone: f32,
    /// Seconds since the epoch of the last event, which the decay is measured from.
    pub updated: u64,
}

impl Tally {
    /// Fade every weight to `now`.
    pub fn age(&mut self, now: u64) {
        let days = now.saturating_sub(self.updated) as f32 / DAY;
        if days > 0.0 {
            let keep = 0.5f32.powf(days / HALF_LIFE_DAYS);
            for w in [&mut self.ignored, &mut self.kept, &mut self.reformatted, &mut self.shortened, &mut self.extended, &mut self.replaced, &mut self.undone] {
                *w *= keep;
            }
        }
        self.updated = now;
    }

    /// Count one event.
    pub fn add(&mut self, outcome: &Outcome, now: u64) {
        self.age(now);
        match outcome {
            Outcome::Ignored => self.ignored += 1.0,
            Outcome::Kept => self.kept += 1.0,
            Outcome::Reformatted { .. } => self.reformatted += 1.0,
            Outcome::Shortened { .. } => self.shortened += 1.0,
            Outcome::Extended { .. } => self.extended += 1.0,
            Outcome::Replaced { .. } => self.replaced += 1.0,
            Outcome::Undone => self.undone += 1.0,
        }
    }

    pub fn total(&self) -> f32 {
        self.ignored + self.kept + self.reformatted + self.shortened + self.extended + self.replaced + self.undone
    }

    /// Share of suggestions taken and not touched: right as drawn.
    pub fn as_drawn_rate(&self) -> Option<f32> {
        self.rate(self.kept + self.reformatted + self.extended)
    }

    /// Share whose beginning was taken and stood: right at least as far as it went.
    pub fn useful_rate(&self) -> Option<f32> {
        self.rate(self.kept + self.reformatted + self.extended + self.shortened)
    }

    /// Share taken and then rewritten or removed: confidently wrong.
    pub fn corrected_rate(&self) -> Option<f32> {
        self.rate(self.replaced + self.undone)
    }

    fn rate(&self, part: f32) -> Option<f32> {
        let total = self.total();
        (total >= MIN_SAMPLES).then(|| part / total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Shape;

    #[test]
    fn a_rate_waits_for_enough_events() {
        let mut t = Tally::default();
        for _ in 0..4 {
            t.add(&Outcome::Kept, 100);
        }
        assert_eq!(t.as_drawn_rate(), None);
        t.add(&Outcome::Kept, 100);
        assert_eq!(t.as_drawn_rate(), Some(1.0));
    }

    #[test]
    fn rates_split_what_was_right_from_what_was_corrected() {
        let mut t = Tally::default();
        for _ in 0..3 {
            t.add(&Outcome::Kept, 0);
        }
        t.add(&Outcome::Shortened { cut: 2 }, 0);
        t.add(&Outcome::Replaced { from: Shape::Null, to: Shape::Name }, 0);
        assert_eq!(t.as_drawn_rate(), Some(0.6));
        assert_eq!(t.useful_rate(), Some(0.8));
        assert_eq!(t.corrected_rate(), Some(0.2));
    }

    #[test]
    fn a_reformat_counts_as_right_not_as_corrected() {
        let mut t = Tally::default();
        for _ in 0..5 {
            t.add(&Outcome::Reformatted { layout: crate::edit::Layout::SpaceAdded }, 0);
        }
        assert_eq!(t.as_drawn_rate(), Some(1.0));
        assert_eq!(t.corrected_rate(), Some(0.0));
    }

    #[test]
    fn weight_halves_after_the_half_life() {
        let mut t = Tally { kept: 8.0, ..Tally::default() };
        t.age((HALF_LIFE_DAYS * DAY) as u64);
        assert!((t.kept - 4.0).abs() < 0.01, "{}", t.kept);
    }
}
