//! The `when` of a tool call: the friendly form an assistant writes, as a [`Recurrence`].

use serde_json::Value;

use crate::calendar::{parse_clock, parse_local_datetime, parse_weekday, Recurrence};

/// The friendly form (`{"kind":"weekly","days":["mon","fri"],"time":"09:00"}`) as a [`Recurrence`].
/// `now_ms` anchors an interval: it first runs one interval from now.
pub fn when_to_recurrence(when: &Value, now_ms: i64) -> Result<Recurrence, String> {
    let text = |k: &str| when.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let clock = || text("time").ok_or("give the time as HH:MM").and_then(|t| parse_clock(t).ok_or("the time must look like 09:30")).map_err(String::from);
    match text("kind").unwrap_or("") {
        "once" => {
            let at = text("at").ok_or("give the date and time as YYYY-MM-DDTHH:MM (local time)")?;
            let at_ms = parse_local_datetime(at).ok_or_else(|| format!("'{at}' is not a local date and time like 2026-11-05T09:00"))?;
            Ok(Recurrence::Once { at_ms })
        }
        "daily" => clock().map(|(hour, minute)| Recurrence::Daily { hour, minute }),
        "weekly" => {
            let names = when.get("days").and_then(Value::as_array).ok_or("give the weekdays, e.g. [\"mon\", \"fri\"]")?;
            let mut days = Vec::new();
            for n in names.iter().filter_map(Value::as_str) {
                let d = parse_weekday(n).ok_or_else(|| format!("'{n}' is not a weekday"))?;
                if !days.contains(&d) {
                    days.push(d);
                }
            }
            let (hour, minute) = clock()?;
            Ok(Recurrence::Weekly { days, hour, minute })
        }
        "every" => {
            let minutes = when.get("minutes").and_then(Value::as_u64).ok_or("give the interval as `minutes`")?;
            Ok(Recurrence::Interval { secs: minutes.saturating_mul(60), anchor_ms: now_ms })
        }
        "cron" => Ok(Recurrence::Cron { expr: text("expr").ok_or("give the 6-field cron expression as `expr`")?.to_string() }),
        other => Err(format!("unknown kind '{other}' (once, daily, weekly, every, cron)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_friendly_when_becomes_a_recurrence() {
        let now = 1_000;
        assert_eq!(when_to_recurrence(&json!({"kind": "daily", "time": "09:30"}), now), Ok(Recurrence::Daily { hour: 9, minute: 30 }));
        assert_eq!(
            when_to_recurrence(&json!({"kind": "weekly", "days": ["Mon", "fri", "lun"], "time": "08:00"}), now),
            Ok(Recurrence::Weekly { days: vec![0, 4], hour: 8, minute: 0 }),
            "names in either language, no duplicates"
        );
        assert_eq!(when_to_recurrence(&json!({"kind": "every", "minutes": 30}), now), Ok(Recurrence::Interval { secs: 1800, anchor_ms: now }));
        assert_eq!(when_to_recurrence(&json!({"kind": "cron", "expr": "0 0 9 * * 1-5"}), now), Ok(Recurrence::Cron { expr: "0 0 9 * * 1-5".into() }));
        match when_to_recurrence(&json!({"kind": "once", "at": "2026-11-05T09:00"}), now) {
            Ok(Recurrence::Once { at_ms }) => assert!((1_793_000_000_000..1_794_000_000_000).contains(&at_ms), "{at_ms}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_bad_when_says_what_is_wrong() {
        let err = |v: Value| when_to_recurrence(&v, 0).unwrap_err();
        assert!(err(json!({"kind": "daily"})).contains("HH:MM"));
        assert!(err(json!({"kind": "daily", "time": "9am"})).contains("09:30"));
        assert!(err(json!({"kind": "weekly", "days": ["funday"], "time": "09:00"})).contains("not a weekday"));
        assert!(err(json!({"kind": "weekly", "time": "09:00"})).contains("weekdays"));
        assert!(err(json!({"kind": "once", "at": "tomorrow"})).contains("not a local date"));
        assert!(err(json!({"kind": "every"})).contains("minutes"));
        assert!(err(json!({"kind": "sometimes"})).contains("unknown kind"));
        assert!(err(json!({})).contains("unknown kind"));
    }
}
