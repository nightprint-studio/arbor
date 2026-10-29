//! The ledger: every run's cost, one line each, in a file that outlives conversations.
//!
//! The day, week and month limits are sums over a period, and a period's spending cannot come from the
//! conversations: deleting one would hand its money back. So each finished run appends a line here —
//! when, which conversation, which model, what it cost in dollars — and the windows are summed from it.
//!
//! The windows' start times come from the panel, in the user's own time zone: the backend has no calendar
//! and no way to know what "today" is where the user is, and a day that began at the wrong hour would make
//! a daily limit reset in the middle of the afternoon.

use std::io::Write;
use std::path::PathBuf;

use bennu_assistant::prelude::Spent;
use serde::{Deserialize, Serialize};

/// Where each window began, in epoch milliseconds, as the user's calendar has it.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Periods {
    pub day: u64,
    pub week: u64,
    pub month: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Entry {
    /// Epoch milliseconds.
    pub ts: u64,
    pub conv: String,
    pub model: String,
    /// Dollars.
    pub usd: f64,
}

pub(super) fn path() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("assistant").join("usage.jsonl")
}

pub(super) fn append(entry: &Entry) -> Result<(), String> {
    let path = path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e| e.to_string())?;
    file.write_all(line.as_bytes()).map_err(|e| e.to_string())
}

/// What the ledger text says was spent in each window, with `session_usd` for the conversation (which
/// the conversation itself keeps). A line that does not parse is skipped: one damaged line must not
/// make the whole record unreadable — and a limit that cannot be checked is worse than one slightly off.
pub(super) fn spent_from(text: &str, periods: &Periods, session_usd: f64) -> Spent {
    let mut spent = Spent { session_usd, ..Default::default() };
    for entry in text.lines().filter_map(|l| serde_json::from_str::<Entry>(l).ok()) {
        if entry.ts >= periods.day {
            spent.day_usd += entry.usd;
        }
        if entry.ts >= periods.week {
            spent.week_usd += entry.usd;
        }
        if entry.ts >= periods.month {
            spent.month_usd += entry.usd;
        }
    }
    spent
}

/// [`spent_from`] over the real file.
pub(super) fn spent(periods: &Periods, session_usd: f64) -> Spent {
    let text = std::fs::read_to_string(path()).unwrap_or_default();
    spent_from(&text, periods, session_usd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ts: u64, usd: f64) -> String {
        serde_json::to_string(&Entry { ts, conv: "c".into(), model: "m".into(), usd }).unwrap()
    }

    fn ledger(entries: &[(u64, f64)]) -> String {
        entries.iter().map(|(t, u)| line(*t, *u)).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn each_window_sums_only_what_happened_since_it_began() {
        let text = ledger(&[(100, 1.0), (500, 2.0), (900, 4.0)]);
        let spent = spent_from(&text, &Periods { day: 800, week: 400, month: 0 }, 0.5);
        assert_eq!(spent.day_usd, 4.0);
        assert_eq!(spent.week_usd, 6.0);
        assert_eq!(spent.month_usd, 7.0);
        assert_eq!(spent.session_usd, 0.5);
    }

    #[test]
    fn a_window_includes_the_instant_it_began() {
        let spent = spent_from(&ledger(&[(800, 3.0)]), &Periods { day: 800, week: 800, month: 800 }, 0.0);
        assert_eq!(spent.day_usd, 3.0);
    }

    #[test]
    fn a_damaged_line_is_skipped_and_the_rest_still_counts() {
        let text = format!("{}\n{{ not json\n\n{}", line(10, 1.0), line(20, 2.0));
        assert_eq!(spent_from(&text, &Periods::default(), 0.0).month_usd, 3.0);
    }

    #[test]
    fn an_empty_ledger_is_nothing_spent() {
        assert_eq!(spent_from("", &Periods::default(), 0.0), Spent::default());
    }
}
