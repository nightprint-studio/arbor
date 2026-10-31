//! `flc_stats` — how often the ghost text is taken: counters, never text.
//!
//! The editor reports three things about every suggestion it draws — shown, accepted with Tab, dismissed — and the
//! counters here keep them **per origin** (`log`, `project`, `name`, `model`) and **per confidence bucket** of five
//! points. Nothing else is stored: no line of code, no file name, no project. The file sits beside the profile's
//! `config.toml` and never leaves the machine.
//!
//! What it is for: a summary in Settings, and one *offered* number — the lowest confidence above which the model's
//! suggestions are taken at least half the time. It is a suggestion the user applies with a button, never a setting
//! that changes by itself.
//!
//! Writing to disk is never on the keystroke path: events arrive in batches every few seconds from the editor and
//! the file is rewritten at most every [`WRITE_EVERY`], or at once when the settings page closes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

/// Width of a confidence bucket, in percent points.
const BUCKET: u32 = 5;
const BUCKETS: usize = (100 / BUCKET) as usize;
/// The threshold is offered only on this many shown suggestions at or above it...
const MIN_SAMPLES: u64 = 30;
/// ...and only when this share of them was accepted.
const TARGET_RATE: f64 = 0.5;
/// The range the settings stepper offers; a suggested threshold stays inside it.
const MIN_THRESHOLD: u32 = 30;
const MAX_THRESHOLD: u32 = 95;
/// The file is rewritten at most this often while events keep arriving.
const WRITE_EVERY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub shown: u64,
    pub accepted: u64,
    pub dismissed: u64,
}

/// Counters per origin, one per confidence bucket.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub by_origin: BTreeMap<String, Vec<Counts>>,
}

/// What the editor did with a suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Shown,
    Accepted,
    Dismissed,
}

/// The bucket a confidence in `0..=1` falls in; anything outside the range is clamped.
pub fn bucket_of(confidence: f32) -> usize {
    let percent = (confidence.clamp(0.0, 1.0) * 100.0).floor() as u32;
    ((percent / BUCKET) as usize).min(BUCKETS - 1)
}

impl Stats {
    pub fn record(&mut self, origin: &str, confidence: f32, outcome: Outcome) {
        let row = self.by_origin.entry(origin.to_string()).or_insert_with(|| vec![Counts::default(); BUCKETS]);
        let c = &mut row[bucket_of(confidence)];
        match outcome {
            Outcome::Shown => c.shown += 1,
            Outcome::Accepted => c.accepted += 1,
            Outcome::Dismissed => c.dismissed += 1,
        }
    }

    /// Totals per origin, in the order the file keeps them.
    pub fn summary(&self) -> Vec<OriginSummary> {
        self.by_origin
            .iter()
            .map(|(origin, row)| {
                let total = row.iter().fold(Counts::default(), |t, c| Counts {
                    shown: t.shown + c.shown,
                    accepted: t.accepted + c.accepted,
                    dismissed: t.dismissed + c.dismissed,
                });
                OriginSummary { origin: origin.clone(), shown: total.shown, accepted: total.accepted, dismissed: total.dismissed }
            })
            .collect()
    }

    /// The lowest threshold, in percent, at or above which the model's suggestions were accepted at least half the
    /// time over at least [`MIN_SAMPLES`] of them; `None` when no threshold in the stepper's range does.
    pub fn suggested_threshold(&self) -> Option<u32> {
        let row = self.by_origin.get("model")?;
        (MIN_THRESHOLD..=MAX_THRESHOLD).step_by(BUCKET as usize).find(|edge| {
            let from = (*edge / BUCKET) as usize;
            let (shown, accepted) = row[from..].iter().fold((0u64, 0u64), |(s, a), c| (s + c.shown, a + c.accepted));
            shown >= MIN_SAMPLES && accepted as f64 / shown as f64 >= TARGET_RATE
        })
    }

    /// Pads every row to the current number of buckets, so a file written with fewer still loads.
    fn normalise(&mut self) {
        for row in self.by_origin.values_mut() {
            row.resize(BUCKETS, Counts::default());
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OriginSummary {
    pub origin: String,
    pub shown: u64,
    pub accepted: u64,
    pub dismissed: u64,
}

// ---- persistence ------------------------------------------------------------------------------------------------

fn stats_path() -> PathBuf {
    arbor_core::prelude::bennu_config_path("flc-stats.json")
}

struct Held {
    stats: Stats,
    dirty: bool,
    last_write: Instant,
}

fn held() -> &'static Mutex<Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(|| {
        let mut stats: Stats = std::fs::read_to_string(stats_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        stats.normalise();
        Mutex::new(Held { stats, dirty: false, last_write: Instant::now() })
    })
}

fn write(held: &mut Held) {
    held.dirty = false;
    held.last_write = Instant::now();
    let path = stats_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // a counter file that cannot be written costs the counters, never the editor
    if let Ok(text) = serde_json::to_string(&held.stats) {
        let _ = std::fs::write(path, text);
    }
}

// ---- handlers ---------------------------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FeedbackEvent {
    pub origin: String,
    /// 0..1, as the suggestion carried it.
    pub confidence: f32,
    pub outcome: Outcome,
}

#[derive(Deserialize)]
pub struct FeedbackArgs {
    pub events: Vec<FeedbackEvent>,
    /// Write to disk now (the editor is going away), instead of waiting for the next interval.
    #[serde(default)]
    pub flush: bool,
}

/// Origins the counters keep; anything else an editor sends is ignored rather than growing the file.
const ORIGINS: [&str; 4] = ["log", "project", "name", "model"];

/// Counts a batch of outcomes. A no-op while *Count how often suggestions are taken* is off.
#[arbor_rpc::handler]
fn bennu_flc_feedback(_ctx: &BennuState, args: FeedbackArgs) -> Result<(), String> {
    if !bennu_core::config::load().flc_feedback {
        return Ok(());
    }
    let mut h = held().lock().unwrap_or_else(|p| p.into_inner());
    for e in args.events.iter().filter(|e| ORIGINS.contains(&e.origin.as_str())) {
        h.stats.record(&e.origin, e.confidence, e.outcome);
        h.dirty = true;
    }
    if h.dirty && (args.flush || h.last_write.elapsed() >= WRITE_EVERY) {
        write(&mut h);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct StatsReport {
    pub origins: Vec<OriginSummary>,
    /// Offered, never applied: see [`Stats::suggested_threshold`]. `None` when there is not enough to say, or it is
    /// what the setting already is.
    pub suggested_threshold: Option<u32>,
}

/// What the settings page shows. Also writes pending counters, so closing the page loses nothing.
#[arbor_rpc::handler]
fn bennu_flc_stats(_ctx: &BennuState) -> Result<StatsReport, String> {
    // what you picked from the completion list is written along with the rest when the settings page asks
    crate::flc_choices::flush();
    let current = bennu_core::config::load().flc_min_confidence;
    let mut h = held().lock().unwrap_or_else(|p| p.into_inner());
    if h.dirty {
        write(&mut h);
    }
    Ok(StatsReport { origins: h.stats.summary(), suggested_threshold: h.stats.suggested_threshold().filter(|t| *t != current) })
}

/// Forgets every counter, on disk too.
#[arbor_rpc::handler]
fn bennu_flc_stats_reset(_ctx: &BennuState) -> Result<(), String> {
    let mut h = held().lock().unwrap_or_else(|p| p.into_inner());
    h.stats = Stats::default();
    write(&mut h);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `n` model suggestions at `confidence`, `accepted` of them taken.
    fn model(stats: &mut Stats, confidence: f32, n: u64, accepted: u64) {
        for i in 0..n {
            stats.record("model", confidence, Outcome::Shown);
            stats.record("model", confidence, if i < accepted { Outcome::Accepted } else { Outcome::Dismissed });
        }
    }

    #[test]
    fn a_confidence_lands_in_a_bucket_of_five_points() {
        assert_eq!(bucket_of(0.0), 0);
        assert_eq!(bucket_of(0.74), 14);
        assert_eq!(bucket_of(0.75), 15);
        assert_eq!(bucket_of(1.0), BUCKETS - 1);
        assert_eq!(bucket_of(7.0), BUCKETS - 1);
        assert_eq!(bucket_of(-1.0), 0);
    }

    #[test]
    fn outcomes_are_counted_per_origin() {
        let mut s = Stats::default();
        s.record("model", 0.8, Outcome::Shown);
        s.record("model", 0.8, Outcome::Accepted);
        s.record("log", 0.9, Outcome::Shown);
        s.record("log", 0.9, Outcome::Dismissed);
        let sum = s.summary();
        let get = |o: &str| sum.iter().find(|x| x.origin == o).unwrap();
        assert_eq!((get("model").shown, get("model").accepted, get("model").dismissed), (1, 1, 0));
        assert_eq!((get("log").shown, get("log").accepted, get("log").dismissed), (1, 0, 1));
    }

    #[test]
    fn the_threshold_is_the_lowest_edge_that_is_taken_half_the_time() {
        let mut s = Stats::default();
        model(&mut s, 0.72, 40, 8); // 20% taken
        model(&mut s, 0.77, 40, 14); // 35%
        model(&mut s, 0.82, 40, 26); // 65%
        model(&mut s, 0.92, 40, 30); // 75%
        // from 75 up: 120 shown, 70 taken = 58%
        assert_eq!(s.suggested_threshold(), Some(75));
    }

    #[test]
    fn too_few_samples_or_a_low_rate_offer_nothing() {
        let mut few = Stats::default();
        model(&mut few, 0.9, 20, 20);
        assert_eq!(few.suggested_threshold(), None);
        let mut low = Stats::default();
        model(&mut low, 0.9, 100, 10);
        assert_eq!(low.suggested_threshold(), None);
        assert_eq!(Stats::default().suggested_threshold(), None);
    }

    #[test]
    fn only_the_model_is_asked_about() {
        let mut s = Stats::default();
        for _ in 0..50 {
            s.record("project", 0.9, Outcome::Shown);
            s.record("project", 0.9, Outcome::Accepted);
        }
        assert_eq!(s.suggested_threshold(), None);
    }

    #[test]
    fn the_file_round_trips_and_a_short_row_is_padded() {
        let mut s = Stats::default();
        model(&mut s, 0.6, 3, 1);
        let json = serde_json::to_string(&s).unwrap();
        let mut back: Stats = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        back.by_origin.get_mut("model").unwrap().truncate(4);
        back.normalise();
        assert_eq!(back.by_origin["model"].len(), BUCKETS);
    }

    #[test]
    fn the_file_holds_counts_and_nothing_else() {
        let mut s = Stats::default();
        s.record("model", 0.8, Outcome::Shown);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.starts_with("{\"by_origin\":{\"model\":["), "{json}");
        assert!(!json.contains("path") && !json.contains("text"));
    }
}
