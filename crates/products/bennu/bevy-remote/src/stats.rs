//! Frame-time statistics: a rolling window of frame durations and the numbers a profiler reader wants
//! from it. Pure — the Bevy side only feeds it `Time` deltas.

use std::collections::VecDeque;

/// About ten seconds at 60 fps.
pub const WINDOW: usize = 600;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Summary {
    pub frames: usize,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    /// 1000 / average frame time.
    pub fps: f64,
}

#[derive(Debug)]
pub struct FrameHistory {
    ms: VecDeque<f64>,
    capacity: usize,
}

impl Default for FrameHistory {
    fn default() -> Self {
        Self::with_capacity(WINDOW)
    }
}

impl FrameHistory {
    pub fn with_capacity(capacity: usize) -> Self {
        Self { ms: VecDeque::with_capacity(capacity), capacity: capacity.max(1) }
    }

    pub fn push(&mut self, frame_ms: f64) {
        if !frame_ms.is_finite() || frame_ms < 0.0 {
            return;
        }
        if self.ms.len() == self.capacity {
            self.ms.pop_front();
        }
        self.ms.push_back(frame_ms);
    }

    /// The last `limit` frame times, oldest first — what a live graph draws.
    pub fn recent(&self, limit: usize) -> Vec<f64> {
        let skip = self.ms.len().saturating_sub(limit);
        self.ms.iter().skip(skip).copied().collect()
    }

    pub fn summary(&self) -> Option<Summary> {
        if self.ms.is_empty() {
            return None;
        }
        let mut sorted: Vec<f64> = self.ms.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        let avg = sorted.iter().sum::<f64>() / sorted.len() as f64;
        Some(Summary {
            frames: sorted.len(),
            avg_ms: avg,
            min_ms: sorted[0],
            max_ms: sorted[sorted.len() - 1],
            p50_ms: percentile(&sorted, 50.0),
            p95_ms: percentile(&sorted, 95.0),
            p99_ms: percentile(&sorted, 99.0),
            fps: if avg > 0.0 { 1000.0 / avg } else { 0.0 },
        })
    }
}

/// Nearest-rank percentile of an ascending slice.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_history_has_no_summary() {
        assert_eq!(FrameHistory::default().summary(), None);
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let mut h = FrameHistory::default();
        for ms in 1..=100 {
            h.push(f64::from(ms));
        }
        let s = h.summary().unwrap();
        assert_eq!((s.min_ms, s.max_ms, s.frames), (1.0, 100.0, 100));
        assert_eq!((s.p50_ms, s.p95_ms, s.p99_ms), (50.0, 95.0, 99.0));
        assert!((s.avg_ms - 50.5).abs() < 1e-9);
        assert!((s.fps - 1000.0 / 50.5).abs() < 1e-9);
    }

    #[test]
    fn the_window_forgets_the_oldest_and_ignores_garbage() {
        let mut h = FrameHistory::with_capacity(3);
        for ms in [1.0, 2.0, 3.0, 4.0, f64::NAN, -1.0] {
            h.push(ms);
        }
        assert_eq!(h.recent(10), vec![2.0, 3.0, 4.0]);
        assert_eq!(h.recent(2), vec![3.0, 4.0]);
    }

    #[test]
    fn one_spike_shows_in_the_tail_not_the_median() {
        let mut h = FrameHistory::default();
        for _ in 0..99 {
            h.push(16.0);
        }
        h.push(200.0);
        let s = h.summary().unwrap();
        assert_eq!(s.p50_ms, 16.0);
        assert_eq!(s.max_ms, 200.0);
    }
}
