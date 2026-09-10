//! Cron expressions: whether one is valid, and what it says in words.
//!
//! ## Why "in words" is the feature
//!
//! Nobody reads cron. `0 0 2 * * ?` is four seconds of counting fields every single time, and the
//! counting is where the mistake happens — the field that moved is the one you did not count. A
//! line of prose under the caret removes the counting, and it removes it at the moment the
//! expression is being written rather than at the incident review.
//!
//! ## What it will and will not say
//!
//! It validates by **structure**: the number of fields, the characters a field may hold, and the
//! range each field's numbers must fall in. All three are certain, and an expression that fails one
//! of them does not run — Spring and Quartz both refuse it when the context starts.
//!
//! It does **not** guess at intent. `0 0 1 * * *` and `0 0 1 * * ?` are both valid and mean
//! different things, and telling somebody which they meant is not this module's business.
//!
//! The prose is best-effort in the other direction: an expression it cannot phrase confidently
//! yields `None`, and the hover then simply says nothing rather than saying something almost right.

/// Which dialect an expression is written in. They differ in the number of fields and in whether
/// `?` is meaningful, so a checker that assumed one would be wrong about the other half of the
/// projects it looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Spring's `@Scheduled(cron = …)` — six fields, second first, plus the `@daily` macros.
    Spring,
    /// Quartz — six or seven fields, the seventh being the year, and `?` required on exactly one
    /// of day-of-month / day-of-week.
    Quartz,
}

/// Why an expression cannot run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronError {
    pub message: String,
}

/// The bounds of each field, in order.
const FIELDS: [(&str, u32, u32); 6] = [
    ("second", 0, 59),
    ("minute", 0, 59),
    ("hour", 0, 23),
    ("day of month", 1, 31),
    ("month", 1, 12),
    // 0 and 7 are both Sunday in every implementation that matters.
    ("day of week", 0, 7),
];

const MONTHS: [&str; 12] =
    ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

/// Spring's macros, and what each expands to.
const MACROS: [(&str, &str); 7] = [
    ("@yearly", "0 0 0 1 1 *"),
    ("@annually", "0 0 0 1 1 *"),
    ("@monthly", "0 0 0 1 * *"),
    ("@weekly", "0 0 0 * * 0"),
    ("@daily", "0 0 0 * * *"),
    ("@midnight", "0 0 0 * * *"),
    ("@hourly", "0 0 * * * *"),
];

/// Check an expression. `Ok(())` for one that will run — which says nothing about whether it runs
/// when you meant it to.
pub fn check(expression: &str, dialect: Dialect) -> Result<(), CronError> {
    let expression = expression.trim();
    if expression.is_empty() {
        return Err(CronError { message: "the expression is empty".into() });
    }
    // A property placeholder is resolved at startup from configuration this cannot see. Nothing is
    // claimed about it in either direction — which is the whole rule of this codebase applied to
    // the one shape that regularly hides a perfectly good expression.
    if expression.contains("${") || expression.contains("#{") {
        return Ok(());
    }
    if expression.starts_with('-') {
        // Spring's "never run" marker. Legal, and deliberate.
        return Ok(());
    }
    if expression.starts_with('@') {
        return if MACROS.iter().any(|(m, _)| *m == expression) {
            Ok(())
        } else {
            Err(CronError {
                message: format!(
                    "`{expression}` is not one of the macros: {}",
                    MACROS.iter().map(|(m, _)| *m).collect::<Vec<_>>().join(", ")
                ),
            })
        };
    }

    let parts: Vec<&str> = expression.split_whitespace().collect();
    let allowed: &[usize] = match dialect {
        Dialect::Spring => &[6],
        Dialect::Quartz => &[6, 7],
    };
    if !allowed.contains(&parts.len()) {
        let expected = match dialect {
            Dialect::Spring => "six".to_string(),
            Dialect::Quartz => "six or seven".to_string(),
        };
        // Five is worth naming: it is a Unix crontab line, which is what somebody pastes.
        let hint = if parts.len() == 5 {
            " — this is a five-field Unix crontab line, which needs a leading seconds field here"
        } else {
            ""
        };
        return Err(CronError {
            message: format!(
                "{expected} fields expected, {} written{hint}",
                parts.len()
            ),
        });
    }

    for (i, part) in parts.iter().take(6).enumerate() {
        let (name, lo, hi) = FIELDS[i];
        check_field(part, name, lo, hi, i)?;
    }
    if parts.len() == 7 {
        check_field(parts[6], "year", 1970, 2199, 6)?;
    }
    Ok(())
}

fn check_field(field: &str, name: &str, lo: u32, hi: u32, index: usize) -> Result<(), CronError> {
    if field == "*" || field == "?" {
        return Ok(());
    }
    for alternative in field.split(',') {
        // `*/5`, `0/15` — a step over a range or over everything.
        let (base, step) = match alternative.split_once('/') {
            Some((base, step)) => (base, Some(step)),
            None => (alternative, None),
        };
        if let Some(step) = step {
            if step.parse::<u32>().map(|s| s == 0).unwrap_or(true) {
                return Err(CronError {
                    message: format!("`{step}` is not a step for the {name} field"),
                });
            }
        }
        if base == "*" || base == "?" {
            continue;
        }
        // Quartz's modifiers, which are legal and mean nothing to a range check: `L` (last), `W`
        // (nearest weekday), `#` (the nth weekday of the month).
        let base = base.trim_end_matches(['L', 'W']);
        if base.is_empty() {
            continue;
        }
        if let Some((nth, _)) = base.split_once('#') {
            check_value(nth, name, lo, hi, index)?;
            continue;
        }
        match base.split_once('-') {
            Some((from, to)) => {
                check_value(from, name, lo, hi, index)?;
                check_value(to, name, lo, hi, index)?;
            }
            None => check_value(base, name, lo, hi, index)?,
        }
    }
    Ok(())
}

fn check_value(value: &str, name: &str, lo: u32, hi: u32, index: usize) -> Result<(), CronError> {
    let upper = value.to_ascii_uppercase();
    // The three-letter names, which are legal in the month and day-of-week fields.
    if index == 4 && MONTHS.contains(&upper.as_str()) {
        return Ok(());
    }
    if index == 5 && DAYS.contains(&upper.as_str()) {
        return Ok(());
    }
    let Ok(n) = value.parse::<u32>() else {
        return Err(CronError {
            message: format!("`{value}` is not a value for the {name} field"),
        });
    };
    if n < lo || n > hi {
        return Err(CronError {
            message: format!("the {name} field takes {lo}–{hi}, and `{value}` is outside it"),
        });
    }
    Ok(())
}

/// The expression in words, when it is one of the shapes worth phrasing. `None` otherwise, and the
/// hover then says nothing rather than something almost right.
pub fn describe(expression: &str, dialect: Dialect) -> Option<String> {
    let expression = expression.trim();
    if let Some((_, expanded)) = MACROS.iter().find(|(m, _)| *m == expression) {
        return describe(expanded, dialect);
    }
    check(expression, dialect).ok()?;
    let parts: Vec<&str> = expression.split_whitespace().collect();
    if parts.len() < 6 {
        return None;
    }
    let (sec, min, hour, dom, mon, dow) =
        (parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]);

    let time = at_time(sec, min, hour)?;
    let day = on_days(dom, mon, dow)?;
    Some(format!("{day}{time}"))
}

/// The time half — `at 02:00`, `every 15 minutes`, `every 30 seconds`.
fn at_time(sec: &str, min: &str, hour: &str) -> Option<String> {
    let fixed = |f: &str| f.parse::<u32>().ok();
    match (fixed(sec), fixed(min), fixed(hour)) {
        (Some(s), Some(m), Some(h)) => Some(if s == 0 {
            format!(" at {h:02}:{m:02}")
        } else {
            format!(" at {h:02}:{m:02}:{s:02}")
        }),
        // `0 */15 * * * *` — every quarter of an hour.
        (Some(0), None, None) if min.starts_with("*/") && hour == "*" => {
            let step = min.trim_start_matches("*/");
            Some(format!(", every {step} minutes"))
        }
        (None, None, None) if sec.starts_with("*/") && min == "*" && hour == "*" => {
            let step = sec.trim_start_matches("*/");
            Some(format!(", every {step} seconds"))
        }
        // `0 30 * * * *` — half past every hour.
        (Some(0), Some(m), None) if hour == "*" => Some(format!(", at {m:02} past every hour")),
        _ => None,
    }
}

/// The day half — `every day`, `every Monday`, `on the 1st of every month`.
fn on_days(dom: &str, mon: &str, dow: &str) -> Option<String> {
    let any = |f: &str| f == "*" || f == "?";
    let month = if any(mon) {
        String::new()
    } else if let Ok(n) = mon.parse::<u32>() {
        format!(" in {}", MONTHS.get((n as usize).saturating_sub(1))?)
    } else if MONTHS.contains(&mon.to_ascii_uppercase().as_str()) {
        format!(" in {}", mon.to_ascii_uppercase())
    } else {
        return None;
    };

    if any(dom) && any(dow) {
        return Some(format!("every day{month}"));
    }
    if any(dow) {
        if dom.eq_ignore_ascii_case("L") {
            return Some(format!("on the last day of the month{month}"));
        }
        let n = dom.parse::<u32>().ok()?;
        return Some(format!("on day {n} of the month{month}"));
    }
    if any(dom) {
        let name = day_name(dow)?;
        return Some(format!("every {name}{month}"));
    }
    None
}

fn day_name(field: &str) -> Option<&'static str> {
    const FULL: [&str; 7] =
        ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    if let Ok(n) = field.parse::<u32>() {
        // 7 is Sunday as well as 0, in every implementation that matters.
        return FULL.get((n % 7) as usize).copied();
    }
    let upper = field.to_ascii_uppercase();
    DAYS.iter().position(|d| *d == upper).and_then(|i| FULL.get(i).copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_expression_is_valid_in_both_dialects() {
        assert!(check("0 0 2 * * ?", Dialect::Quartz).is_ok());
        assert!(check("0 0 2 * * *", Dialect::Spring).is_ok());
    }

    /// What people actually paste: a Unix crontab line, into a field that wants six.
    #[test]
    fn a_five_field_crontab_line_is_named_for_what_it_is() {
        let err = check("0 2 * * *", Dialect::Spring).unwrap_err();
        assert!(err.message.contains("five-field Unix crontab"), "{}", err.message);
    }

    #[test]
    fn a_value_outside_its_field_is_refused() {
        let err = check("0 0 25 * * *", Dialect::Spring).unwrap_err();
        assert!(err.message.contains("hour field takes 0–23"), "{}", err.message);
        assert!(check("0 61 * * * *", Dialect::Spring).is_err());
    }

    #[test]
    fn the_names_and_the_modifiers_are_legal() {
        assert!(check("0 0 6 ? * MON-FRI", Dialect::Quartz).is_ok());
        assert!(check("0 0 0 L * ?", Dialect::Quartz).is_ok());
        assert!(check("0 15 10 ? * 6#3", Dialect::Quartz).is_ok());
        assert!(check("0 0 0 1 JAN *", Dialect::Spring).is_ok());
    }

    #[test]
    fn a_seventh_field_is_a_year_and_only_quartz_takes_one() {
        assert!(check("0 0 2 * * ? 2026", Dialect::Quartz).is_ok());
        assert!(check("0 0 2 * * * 2026", Dialect::Spring).is_err());
    }

    /// A placeholder is resolved from configuration this cannot see, so nothing is claimed about
    /// it — the alternative is reporting every externalised schedule in the project as broken.
    #[test]
    fn a_placeholder_is_not_judged() {
        assert!(check("${report.cron}", Dialect::Spring).is_ok());
        assert!(check("#{@config.cron}", Dialect::Spring).is_ok());
    }

    #[test]
    fn the_macros_are_recognised_and_a_misspelling_is_not() {
        assert!(check("@daily", Dialect::Spring).is_ok());
        assert!(check("@dayly", Dialect::Spring).is_err());
    }

    /// Spring's marker for a bean that declares a schedule and is switched off.
    #[test]
    fn the_disabled_marker_is_legal() {
        assert!(check("-", Dialect::Spring).is_ok());
    }

    // ── the words ────────────────────────────────────────────────────────────

    #[test]
    fn the_common_shapes_are_phrased() {
        assert_eq!(describe("0 0 2 * * ?", Dialect::Quartz).as_deref(), Some("every day at 02:00"));
        assert_eq!(
            describe("0 30 4 * * MON", Dialect::Quartz).as_deref(),
            Some("every Monday at 04:30")
        );
        assert_eq!(
            describe("0 0 0 1 * ?", Dialect::Quartz).as_deref(),
            Some("on day 1 of the month at 00:00")
        );
        assert_eq!(
            describe("0 */15 * * * *", Dialect::Spring).as_deref(),
            Some("every day, every 15 minutes")
        );
        assert_eq!(describe("@daily", Dialect::Spring).as_deref(), Some("every day at 00:00"));
    }

    /// An expression it cannot phrase confidently says nothing, rather than something almost right.
    #[test]
    fn an_expression_it_cannot_phrase_says_nothing() {
        assert_eq!(describe("0 0 2 15 * MON", Dialect::Quartz), None);
        assert_eq!(describe("0 0 25 * * *", Dialect::Spring), None, "and an invalid one too");
    }
}
