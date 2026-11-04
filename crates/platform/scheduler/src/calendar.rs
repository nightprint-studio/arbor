//! Calendar recurrences and their **next run** — the pure half of "do X at 9:00 every weekday".
//!
//! [`Trigger`](crate::trigger::Trigger) drives a task that is *registered with the engine and lives in
//! memory*. A job that must be listed, edited, survive a restart and answer "when does it run next?" owns
//! its schedule as data instead; this module is that data and the one function that reads it,
//! [`next_after`]. It is deliberately a function of `(recurrence, instant, time zone)`: no clock, no
//! threads, so every rule — a daily time that falls in a DST gap, a weekday set, a cron match — is a test.
//!
//! Times of day and cron expressions are **local time** (what a person means by "at 9:00"); the instant
//! is unix milliseconds. Cron is the same 6-field Spring form the engine uses (`sec min hour dom mon dow`).

use std::str::FromStr;

use chrono::{DateTime, Datelike, Duration as ChronoDuration, Local, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};

/// The shortest repeat interval: a minute is the resolution anyone needs from "every N minutes".
pub const MIN_INTERVAL_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recurrence {
    /// Once, at this instant.
    Once { at_ms: i64 },
    /// Every `secs`, counted from `anchor_ms` (the first run is the anchor itself).
    Interval { secs: u64, anchor_ms: i64 },
    /// Every day at `hour:minute`.
    Daily { hour: u8, minute: u8 },
    /// On these weekdays (0 = Monday … 6 = Sunday) at `hour:minute`.
    Weekly { days: Vec<u8>, hour: u8, minute: u8 },
    /// A 6-field cron expression.
    Cron { expr: String },
}

/// Whether the recurrence is well formed; the message is what to show the user.
pub fn validate(r: &Recurrence) -> Result<(), String> {
    let clock = |hour: u8, minute: u8| {
        if hour > 23 || minute > 59 { Err(format!("{hour:02}:{minute:02} is not a time of day")) } else { Ok(()) }
    };
    match r {
        Recurrence::Once { .. } => Ok(()),
        Recurrence::Interval { secs, .. } if *secs < MIN_INTERVAL_SECS => Err(format!("the interval must be at least {MIN_INTERVAL_SECS} seconds")),
        Recurrence::Interval { .. } => Ok(()),
        Recurrence::Daily { hour, minute } => clock(*hour, *minute),
        Recurrence::Weekly { days, hour, minute } => {
            if days.is_empty() {
                return Err("pick at least one weekday".into());
            }
            if let Some(bad) = days.iter().find(|d| **d > 6) {
                return Err(format!("{bad} is not a weekday (0 = Monday … 6 = Sunday)"));
            }
            clock(*hour, *minute)
        }
        Recurrence::Cron { expr } => cron::Schedule::from_str(expr).map(|_| ()).map_err(|e| format!("invalid cron expression \"{expr}\": {e}")),
    }
}

/// The first run strictly after `after_ms`, in the local time zone. `None` when there is none (a one-time
/// run already past).
pub fn next_after(r: &Recurrence, after_ms: i64) -> Result<Option<i64>, String> {
    next_after_in(r, after_ms, &Local)
}

/// [`next_after`] in an explicit time zone — what the tests use.
pub fn next_after_in<Tz: TimeZone>(r: &Recurrence, after_ms: i64, tz: &Tz) -> Result<Option<i64>, String> {
    validate(r)?;
    Ok(match r {
        Recurrence::Once { at_ms } => (*at_ms > after_ms).then_some(*at_ms),
        Recurrence::Interval { secs, anchor_ms } => {
            let step = (*secs as i64).saturating_mul(1000);
            if after_ms < *anchor_ms {
                Some(*anchor_ms)
            } else {
                Some(anchor_ms + ((after_ms - anchor_ms) / step + 1) * step)
            }
        }
        Recurrence::Daily { hour, minute } => at_time(after_ms, tz, *hour, *minute, |_| true),
        Recurrence::Weekly { days, hour, minute } => at_time(after_ms, tz, *hour, *minute, |wd| days.contains(&wd)),
        Recurrence::Cron { expr } => {
            let schedule = cron::Schedule::from_str(expr).map_err(|e| e.to_string())?;
            let from = tz.timestamp_millis_opt(after_ms).single().ok_or("the instant is out of range")?;
            schedule.after(&from).next().map(|d| d.timestamp_millis())
        }
    })
}

/// The next `hour:minute` after `after_ms` on a day whose weekday (0 = Monday) `allowed` accepts. A local time
/// that does not exist that day (the hour a clock skips forward) is passed over, not shifted.
fn at_time<Tz: TimeZone>(after_ms: i64, tz: &Tz, hour: u8, minute: u8, allowed: impl Fn(u8) -> bool) -> Option<i64> {
    let after: DateTime<Tz> = tz.timestamp_millis_opt(after_ms).single()?;
    let time = NaiveTime::from_hms_opt(hour.into(), minute.into(), 0)?;
    // Eight days cover a full week plus the day already under way.
    (0..=8).find_map(|offset| {
        let date = after.date_naive() + ChronoDuration::days(offset);
        let weekday = date.weekday().num_days_from_monday() as u8;
        if !allowed(weekday) {
            return None;
        }
        let at = tz.from_local_datetime(&date.and_time(time)).earliest()?;
        (at.timestamp_millis() > after_ms).then(|| at.timestamp_millis())
    })
}

// ── Text in, text out ────────────────────────────────────────────────────────────────────────────

/// `"09:30"` → `(9, 30)`.
pub fn parse_clock(s: &str) -> Option<(u8, u8)> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u8, u8) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h <= 23 && m <= 59).then_some((h, m))
}

/// `"mon"`, `"Monday"`, `"lun"`… → 0 = Monday … 6 = Sunday. English and Italian short names, since the
/// user writes in either.
pub fn parse_weekday(s: &str) -> Option<u8> {
    let w = s.trim().to_lowercase();
    let w = w.get(..3).unwrap_or(&w);
    ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
        .iter()
        .position(|d| *d == w)
        .or_else(|| ["lun", "mar", "mer", "gio", "ven", "sab", "dom"].iter().position(|d| *d == w))
        .map(|i| i as u8)
}

/// `"2026-11-05T09:00"` or `"2026-11-05 09:00"` read as local time → unix ms. `None` when malformed or when
/// that local time does not exist (the hour a clock skips).
pub fn parse_local_datetime_in<Tz: TimeZone>(s: &str, tz: &Tz) -> Option<i64> {
    let s = s.trim().replace('T', " ");
    let naive = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M").ok()?;
    tz.from_local_datetime(&naive).earliest().map(|d| d.timestamp_millis())
}

pub fn parse_local_datetime(s: &str) -> Option<i64> {
    parse_local_datetime_in(s, &Local)
}

/// An instant as `2026-11-05 09:00`, local time.
pub fn format_local_in<Tz: TimeZone>(ms: i64, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    tz.timestamp_millis_opt(ms).single().map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
}

pub fn format_local(ms: i64) -> String {
    format_local_in(ms, &Local)
}

/// What to do with a run whose time has passed while nobody was looking (the app was closed or asleep).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissedPolicy {
    /// Forget it and wait for the next one. The default: a stale "good morning" is noise.
    #[default]
    Skip,
    /// Run once now, however many were missed.
    RunOnce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// Not yet.
    Wait,
    /// Run now.
    Run,
    /// It was missed and the policy says to drop it: move on to the next.
    Skip,
}

/// How far past its time a run may start and still count as on time (the tick of the loop that fires it).
pub const GRACE_MS: i64 = 2 * 60 * 1000;

/// Whether a job whose next run was `next_ms` is due at `now_ms`.
pub fn due(next_ms: i64, now_ms: i64, policy: MissedPolicy) -> Due {
    if next_ms > now_ms {
        Due::Wait
    } else if now_ms - next_ms <= GRACE_MS || policy == MissedPolicy::RunOnce {
        Due::Run
    } else {
        Due::Skip
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Utc};

    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;
    const DAY: i64 = 24 * HOUR;

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap().timestamp_millis()
    }

    fn next(r: &Recurrence, after: i64) -> Option<i64> {
        next_after_in(r, after, &Utc).unwrap()
    }

    #[test]
    fn a_one_time_run_is_next_only_while_it_is_ahead() {
        let r = Recurrence::Once { at_ms: 1_000 };
        assert_eq!(next(&r, 0), Some(1_000));
        assert_eq!(next(&r, 1_000), None, "strictly after: a run at this very instant has happened");
        assert_eq!(next(&r, 5_000), None);
    }

    #[test]
    fn an_interval_counts_from_its_anchor() {
        let r = Recurrence::Interval { secs: 600, anchor_ms: 10 * HOUR };
        assert_eq!(next(&r, 0), Some(10 * HOUR), "before the anchor, the anchor is the first run");
        assert_eq!(next(&r, 10 * HOUR), Some(10 * HOUR + 10 * MIN));
        assert_eq!(next(&r, 10 * HOUR + 25 * MIN), Some(10 * HOUR + 30 * MIN));
    }

    #[test]
    fn daily_takes_today_if_still_ahead_else_tomorrow() {
        let r = Recurrence::Daily { hour: 9, minute: 30 };
        assert_eq!(next(&r, utc(2026, 11, 3, 8, 0)), Some(utc(2026, 11, 3, 9, 30)));
        assert_eq!(next(&r, utc(2026, 11, 3, 9, 30)), Some(utc(2026, 11, 4, 9, 30)));
        assert_eq!(next(&r, utc(2026, 11, 3, 23, 59)), Some(utc(2026, 11, 4, 9, 30)));
    }

    #[test]
    fn weekly_picks_the_next_chosen_weekday() {
        // 2026-11-03 is a Tuesday. Monday and Friday at 08:00.
        let r = Recurrence::Weekly { days: vec![0, 4], hour: 8, minute: 0 };
        assert_eq!(next(&r, utc(2026, 11, 3, 12, 0)), Some(utc(2026, 11, 6, 8, 0)), "Friday");
        assert_eq!(next(&r, utc(2026, 11, 6, 8, 0)), Some(utc(2026, 11, 9, 8, 0)), "then the next Monday");
        // The same weekday, later today.
        let tue = Recurrence::Weekly { days: vec![1], hour: 18, minute: 0 };
        assert_eq!(next(&tue, utc(2026, 11, 3, 12, 0)), Some(utc(2026, 11, 3, 18, 0)));
        // The same weekday, already passed: a week on.
        assert_eq!(next(&tue, utc(2026, 11, 3, 18, 0)), Some(utc(2026, 11, 10, 18, 0)));
    }

    #[test]
    fn times_are_local_to_the_zone_they_are_asked_in() {
        let plus2 = FixedOffset::east_opt(2 * 3600).unwrap();
        let r = Recurrence::Daily { hour: 9, minute: 0 };
        // 09:00 at +02:00 is 07:00 UTC.
        assert_eq!(next_after_in(&r, utc(2026, 11, 3, 0, 0), &plus2).unwrap(), Some(utc(2026, 11, 3, 7, 0)));
    }

    #[test]
    fn cron_matches_in_the_zone_too() {
        let r = Recurrence::Cron { expr: "0 0 2 * * *".into() };
        assert_eq!(next(&r, utc(2026, 11, 3, 1, 0)), Some(utc(2026, 11, 3, 2, 0)));
        assert_eq!(next(&r, utc(2026, 11, 3, 2, 0)), Some(utc(2026, 11, 4, 2, 0)));
        let every_15 = Recurrence::Cron { expr: "0 */15 * * * *".into() };
        assert_eq!(next(&every_15, utc(2026, 11, 3, 10, 7)), Some(utc(2026, 11, 3, 10, 15)));
    }

    #[test]
    fn a_malformed_recurrence_is_refused_with_a_reason() {
        let bad = |r: Recurrence| validate(&r).unwrap_err();
        assert!(bad(Recurrence::Daily { hour: 24, minute: 0 }).contains("not a time"));
        assert!(bad(Recurrence::Daily { hour: 9, minute: 60 }).contains("not a time"));
        assert!(bad(Recurrence::Weekly { days: vec![], hour: 9, minute: 0 }).contains("at least one"));
        assert!(bad(Recurrence::Weekly { days: vec![7], hour: 9, minute: 0 }).contains("not a weekday"));
        assert!(bad(Recurrence::Interval { secs: 30, anchor_ms: 0 }).contains("at least 60"));
        assert!(bad(Recurrence::Cron { expr: "every day".into() }).contains("invalid cron"));
        assert!(next_after_in(&Recurrence::Daily { hour: 99, minute: 0 }, 0, &Utc).is_err());
    }

    #[test]
    fn a_daily_time_is_never_more_than_a_day_away() {
        let r = Recurrence::Daily { hour: 3, minute: 15 };
        for after in [0, 5 * HOUR, 3 * HOUR + 15 * MIN, 40 * DAY + 7] {
            let n = next(&r, after).unwrap();
            assert!(n > after && n - after <= DAY, "{after} -> {n}");
        }
    }

    #[test]
    fn a_run_is_on_time_within_the_grace_missed_after_it() {
        let now = 10 * HOUR;
        assert_eq!(due(now + MIN, now, MissedPolicy::Skip), Due::Wait);
        assert_eq!(due(now, now, MissedPolicy::Skip), Due::Run);
        assert_eq!(due(now - GRACE_MS, now, MissedPolicy::Skip), Due::Run, "the loop tick that fires it may be late");
        assert_eq!(due(now - GRACE_MS - 1, now, MissedPolicy::Skip), Due::Skip, "missed, and the policy drops it");
        assert_eq!(due(now - 3 * DAY, now, MissedPolicy::RunOnce), Due::Run, "missed, and the policy catches up once");
    }

    #[test]
    fn times_and_weekdays_are_read_from_words() {
        assert_eq!(parse_clock("09:30"), Some((9, 30)));
        assert_eq!(parse_clock(" 7:05 "), Some((7, 5)));
        assert_eq!(parse_clock("24:00"), None);
        assert_eq!(parse_clock("noon"), None);
        assert_eq!(parse_weekday("Monday"), Some(0));
        assert_eq!(parse_weekday("fri"), Some(4));
        assert_eq!(parse_weekday("Domenica"), Some(6));
        assert_eq!(parse_weekday("lun"), Some(0));
        assert_eq!(parse_weekday("xyz"), None);
    }

    #[test]
    fn a_local_date_time_is_read_and_written_back() {
        let plus1 = FixedOffset::east_opt(3600).unwrap();
        let ms = parse_local_datetime_in("2026-11-05T09:00", &plus1).unwrap();
        assert_eq!(ms, utc(2026, 11, 5, 8, 0));
        assert_eq!(parse_local_datetime_in("2026-11-05 09:00", &plus1), Some(ms));
        assert_eq!(format_local_in(ms, &plus1), "2026-11-05 09:00");
        assert_eq!(parse_local_datetime_in("tomorrow", &plus1), None);
        assert_eq!(parse_local_datetime_in("2026-13-05 09:00", &plus1), None);
    }
}
