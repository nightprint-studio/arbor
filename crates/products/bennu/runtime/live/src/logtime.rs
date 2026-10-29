//! Times in logs and in the questions asked about them: ISO-8601, epoch numbers, `30s` / `5m` spans.
//!
//! No calendar crate: the only arithmetic needed is "days since 1970 for a civil date" and back, which
//! is a dozen lines (Howard Hinnant's algorithms). A time with no offset is taken as UTC — for a log
//! *file* that means "the clock the file was written with", which is what compares correctly against
//! the other lines of the same file.

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `2026-08-05 12:33:01.123` (UTC) of epoch milliseconds: one clock for every digest.
pub fn iso(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    let rest = ms % 86_400_000;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{:03}", rest / 3_600_000, (rest / 60_000) % 60, (rest / 1000) % 60, rest % 1000)
}

fn num(b: &[u8], at: usize, n: usize) -> Option<i64> {
    let s = b.get(at..at + n)?;
    if !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(s.iter().fold(0i64, |a, c| a * 10 + i64::from(c - b'0')))
}

/// `2026-08-05`, `2026-08-05T12:33`, `2026-08-05 12:33:01,123`, `...Z`, `...+02:00`, `...+0200`.
/// `None` for anything else (a bare time has no day to put it on).
pub fn parse_iso(text: &str) -> Option<u64> {
    let b = text.trim().as_bytes();
    let (y, mo, d) = (num(b, 0, 4)?, num(b, 5, 2)?, num(b, 8, 2)?);
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let mut at = 10;
    let (mut h, mut mi, mut s, mut frac_ms) = (0, 0, 0, 0);
    if matches!(b.get(at), Some(b'T') | Some(b' ')) && num(b, at + 1, 2).is_some() {
        h = num(b, at + 1, 2)?;
        if b.get(at + 3) != Some(&b':') {
            return None;
        }
        mi = num(b, at + 4, 2)?;
        at += 6;
        if b.get(at) == Some(&b':') {
            s = num(b, at + 1, 2)?;
            at += 3;
            if matches!(b.get(at), Some(b'.') | Some(b',')) {
                at += 1;
                let start = at;
                while b.get(at).is_some_and(u8::is_ascii_digit) {
                    at += 1;
                }
                let digits = &b[start..at.min(start + 3)];
                frac_ms = digits.iter().fold(0i64, |a, c| a * 10 + i64::from(c - b'0')) * 10i64.pow(3 - digits.len() as u32);
            }
        }
    }
    if h > 23 || mi > 59 || s > 60 {
        return None;
    }
    let offset_min = match b.get(at) {
        Some(b'Z') | None => 0,
        Some(sign @ (b'+' | b'-')) => {
            let hh = num(b, at + 1, 2)?;
            let mm = if b.get(at + 3) == Some(&b':') { num(b, at + 4, 2)? } else { num(b, at + 3, 2).unwrap_or(0) };
            let total = hh * 60 + mm;
            if *sign == b'+' { total } else { -total }
        }
        _ => return None,
    };
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s - offset_min * 60;
    u64::try_from(secs * 1000 + frac_ms).ok()
}

/// A moment given to a tool: epoch milliseconds, epoch seconds, or ISO-8601.
pub fn parse_time_arg(text: &str) -> Result<u64, String> {
    let t = text.trim();
    if !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit()) {
        let n: u64 = t.parse().map_err(|_| format!("`{t}` is not a time"))?;
        // Seconds until the year 5138; nobody passes milliseconds that small.
        return Ok(if n < 100_000_000_000 { n * 1000 } else { n });
    }
    parse_iso(t).ok_or_else(|| format!("`{t}` is not a time: use ISO-8601 (2026-08-05T12:33:01Z, a missing offset means UTC) or epoch milliseconds"))
}

/// A span: `250ms`, `30s`, `5m`, `2h`, `1d`, decimals allowed (`1.5h`).
pub fn parse_duration(text: &str) -> Result<u64, String> {
    let t = text.trim().to_ascii_lowercase();
    let split = t.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(t.len());
    let (number, unit) = t.split_at(split);
    let value: f64 = number.parse().map_err(|_| format!("`{text}` is not a duration: use 250ms, 30s, 5m, 2h or 1d"))?;
    let unit_ms = match unit.trim() {
        "ms" => 1.0,
        "s" | "sec" => 1000.0,
        "m" | "min" => 60_000.0,
        "h" => 3_600_000.0,
        "d" => 86_400_000.0,
        _ => return Err(format!("`{text}` is not a duration: use 250ms, 30s, 5m, 2h or 1d")),
    };
    Ok((value * unit_ms) as u64)
}

/// A duration printed in a log (`12 ms`, `0.5s`, `340µs`) in milliseconds.
pub fn to_ms(value: f64, unit: &str) -> f64 {
    match unit {
        "µs" | "us" => value / 1000.0,
        "s" => value * 1000.0,
        _ => value,
    }
}

/// The window a question is about: `after` / `before` as epoch ms, either end open.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TimeWindow {
    pub after_ms: Option<u64>,
    pub before_ms: Option<u64>,
}

impl TimeWindow {
    /// From the tool's `after` / `before` / `last`; `last` wins over `after`. `now` is the reference
    /// for `last` (the clock for a process, the newest line for a file with its own clock).
    pub fn from_args(after: Option<&str>, before: Option<&str>, last: Option<&str>, now: u64) -> Result<Self, String> {
        let mut w = TimeWindow::default();
        if let Some(a) = after.filter(|s| !s.is_empty()) {
            w.after_ms = Some(parse_time_arg(a)?);
        }
        if let Some(b) = before.filter(|s| !s.is_empty()) {
            w.before_ms = Some(parse_time_arg(b)?);
        }
        if let Some(l) = last.filter(|s| !s.is_empty()) {
            w.after_ms = Some(now.saturating_sub(parse_duration(l)?));
        }
        Ok(w)
    }

    pub fn is_open(&self) -> bool {
        self.after_ms.is_none() && self.before_ms.is_none()
    }

    pub fn contains(&self, ts: u64) -> bool {
        self.after_ms.is_none_or(|a| ts >= a) && self.before_ms.is_none_or(|b| ts <= b)
    }

    pub fn describe(&self) -> String {
        match (self.after_ms, self.before_ms) {
            (Some(a), Some(b)) => format!("between {} and {}", iso(a), iso(b)),
            (Some(a), None) => format!("after {}", iso(a)),
            (None, Some(b)) => format!("before {}", iso(b)),
            (None, None) => "any time".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_round_trips_and_honours_offsets() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        let t = parse_iso("2026-08-05 12:33:01,123").unwrap();
        assert_eq!(iso(t), "2026-08-05 12:33:01.123");
        assert_eq!(parse_iso("2026-08-05T14:33:01.123+02:00"), Some(t));
        assert_eq!(parse_iso("2026-08-05T14:33:01.123+0200"), Some(t));
        assert_eq!(parse_iso("2026-08-05T12:33:01.5Z").map(|v| v % 1000), Some(500));
        assert!(parse_iso("12:33:01").is_none() && parse_iso("2026-13-01").is_none() && parse_iso("nope").is_none());
    }

    #[test]
    fn leap_days_are_counted() {
        let a = parse_iso("2024-02-29T00:00:00Z").unwrap();
        let b = parse_iso("2024-03-01T00:00:00Z").unwrap();
        assert_eq!(b - a, 86_400_000);
        assert_eq!(iso(a), "2024-02-29 00:00:00.000");
    }

    #[test]
    fn a_time_argument_may_be_epoch_seconds_millis_or_iso() {
        assert_eq!(parse_time_arg("1700000000"), Ok(1_700_000_000_000));
        assert_eq!(parse_time_arg("1700000000123"), Ok(1_700_000_000_123));
        assert_eq!(parse_time_arg("2023-11-14T22:13:20Z"), Ok(1_700_000_000_000));
        assert!(parse_time_arg("yesterday").is_err());
    }

    #[test]
    fn durations_have_units() {
        assert_eq!(parse_duration("30s"), Ok(30_000));
        assert_eq!(parse_duration("5m"), Ok(300_000));
        assert_eq!(parse_duration("1.5h"), Ok(5_400_000));
        assert_eq!(parse_duration("250ms"), Ok(250));
        assert!(parse_duration("5").is_err() && parse_duration("soon").is_err());
    }

    #[test]
    fn last_is_relative_to_the_reference_and_wins_over_after() {
        let w = TimeWindow::from_args(Some("2020-01-01"), None, Some("1m"), 1_000_000).unwrap();
        assert_eq!(w.after_ms, Some(940_000));
        assert!(w.contains(940_000) && !w.contains(939_999));
        let w = TimeWindow::from_args(None, Some("1000"), None, 0).unwrap();
        assert!(w.contains(1_000_000) && !w.contains(1_000_001));
    }

    #[test]
    fn units_convert_to_milliseconds() {
        assert_eq!(to_ms(1500.0, "µs"), 1.5);
        assert_eq!(to_ms(2.0, "s"), 2000.0);
        assert_eq!(to_ms(7.0, "ms"), 7.0);
    }
}
