//! What a run nobody watches may cost: a cap on one run, and a cap on how many start in a day.
//!
//! They sit on top of the spending limits the panel's meters already show (the day, week and month limits
//! apply to a scheduled run like to any other); these two exist because a mis-set cron can start a hundred runs
//! overnight, each small enough to pass.

use std::io::Write;
use std::path::PathBuf;

use bennu_assistant::prelude::LimitsConfig;

/// One scheduled run's cost cap, in dollars (what the run is priced in), or `None` when the user turned it off.
pub fn run_cap_usd(limits: &LimitsConfig) -> Option<f64> {
    (limits.schedule_run_eur > 0.0).then(|| limits.schedule_run_eur / limits.eur_per_usd.max(0.01))
}

/// How many starts the log text holds at or after `since_ms`. A line that is not a number is skipped.
pub fn starts_since(text: &str, since_ms: i64) -> u32 {
    text.lines().filter_map(|l| l.trim().parse::<i64>().ok()).filter(|t| *t >= since_ms).count() as u32
}

/// Whether one more run may start today, given how many already did. `0` is no cap.
pub fn within_daily_cap(started_today: u32, cap: u32) -> bool {
    cap == 0 || started_today < cap
}

fn log_path() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("assistant").join("schedule-runs.log")
}

/// Starts of scheduled runs since `since_ms` (the beginning of today).
pub fn started_since(since_ms: i64) -> u32 {
    starts_since(&std::fs::read_to_string(log_path()).unwrap_or_default(), since_ms)
}

/// Put a start in the log: one line, the instant. Failing to write is not a reason to refuse the run.
pub fn note_start(now_ms: i64) {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{now_ms}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_cap_is_in_dollars_and_off_at_zero() {
        let mut limits = LimitsConfig::default();
        limits.schedule_run_eur = 0.92;
        limits.eur_per_usd = 0.92;
        assert_eq!(run_cap_usd(&limits), Some(1.0));
        limits.schedule_run_eur = 0.0;
        assert_eq!(run_cap_usd(&limits), None);
        // A rate of zero would divide by it.
        limits.schedule_run_eur = 1.0;
        limits.eur_per_usd = 0.0;
        assert!(run_cap_usd(&limits).unwrap().is_finite());
    }

    #[test]
    fn starts_are_counted_from_the_start_of_the_day() {
        let log = "100\n250\n  300 \nnot a number\n\n400\n";
        assert_eq!(starts_since(log, 250), 3);
        assert_eq!(starts_since(log, 0), 4);
        assert_eq!(starts_since(log, 1_000), 0);
        assert_eq!(starts_since("", 0), 0);
    }

    #[test]
    fn the_daily_cap_stops_the_run_after_the_last_one_allowed() {
        assert!(within_daily_cap(0, 3) && within_daily_cap(2, 3));
        assert!(!within_daily_cap(3, 3) && !within_daily_cap(10, 3));
        assert!(within_daily_cap(1_000, 0), "zero is no cap");
    }
}
