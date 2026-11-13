//! What each provider Imber can use has left, for the credit meter in the title bars.
//!
//! One shape for every provider, so the meter does not change when a second one arrives (JetBrains AI through
//! Central is the one in view: `docs/mockups/imber-jetbrains-ai.html`): a headline figure, how much is left as a
//! share when a limit is known, and the lines that explain it. The meter's ring is the **average** left over the
//! providers that know a limit; its colour is the **worst** one, because an average hides a provider about to run
//! out (100% and 5% make 52%).
//!
//! For Anthropic with an API key there is no balance to read: the API does not expose an account's prepaid
//! credit. What is known is Imber's own spending (the ledger) against the limits the user set, and — only with an
//! Admin key, which exists only in an Anthropic organization — what the whole organization really spent this
//! month, from `GET /v1/organizations/cost_report`. The entry says which of these it is.

use serde::Serialize;
use serde_json::Value;

use crate::limits::{eur, Level, Limits, Period, Spent};

/// Under this share left, a provider turns the meter amber; at nothing left, red.
pub const WARN_LEFT: f64 = 0.2;

/// One line of a provider's detail: a label and its figure.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreditLine {
    pub label: String,
    pub value: String,
}

/// A group of lines under a heading ("Spent by Imber · your limits").
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreditSection {
    pub title: String,
    pub lines: Vec<CreditLine>,
    /// A sentence under the lines: what the figures are not, what is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Something the user can do about it, by id (`add_admin_key`): the panel draws the button.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// One provider in the meter.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderCredit {
    /// `anthropic`, `jetbrains`.
    pub id: String,
    pub name: String,
    /// Imber is using it now.
    pub active: bool,
    /// "Active · API key", "Not set up".
    pub subtitle: String,
    /// The figure on its row: "€0.43", "62 / 100", "—".
    pub figure: String,
    /// Share left, 0–1, when a limit is known; `None` keeps it out of the average.
    pub left: Option<f64>,
    pub sections: Vec<CreditSection>,
}

/// Every provider, and what the meter draws from them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreditSummary {
    pub providers: Vec<ProviderCredit>,
    /// Average share left over the providers with a known limit.
    pub average_left: Option<f64>,
    /// The provider with the least left, and how much.
    pub lowest: Option<(String, f64)>,
    /// `ok`, `warn` or `out`, from the lowest.
    pub level: &'static str,
}

/// The meter's figures from its providers; the active one first.
pub fn summarize(mut providers: Vec<ProviderCredit>) -> CreditSummary {
    providers.sort_by_key(|p| !p.active);
    let known: Vec<(&str, f64)> = providers.iter().filter_map(|p| p.left.map(|l| (p.id.as_str(), l.clamp(0.0, 1.0)))).collect();
    let average_left = (!known.is_empty()).then(|| known.iter().map(|(_, l)| l).sum::<f64>() / known.len() as f64);
    let lowest = known.iter().min_by(|a, b| a.1.total_cmp(&b.1)).map(|(id, l)| (id.to_string(), *l));
    let level = match &lowest {
        Some((_, l)) if *l <= 0.0 => "out",
        Some((_, l)) if *l <= WARN_LEFT => "warn",
        _ => "ok",
    };
    CreditSummary { providers, average_left, lowest, level }
}

/// What the organization spent, read with an Admin key.
#[derive(Debug, Clone, PartialEq)]
pub enum OrgSpend {
    /// No Admin key is stored.
    NoAdminKey,
    /// The month so far and, when a balance was entered, since that day — in dollars, by whole UTC days.
    Known { month_usd: f64, since_balance_usd: Option<f64> },
    /// The key is there and the call failed: why, in words.
    Failed(String),
}

/// The account's credit balance as the user entered it — read in the Claude Console, because the API does not
/// report it — and when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ManualBalance {
    pub usd: f64,
    /// Epoch milliseconds.
    pub set_at: u64,
}

impl ManualBalance {
    /// The balance from its two config fields; `None` when it was never entered.
    pub fn from_config(usd: f64, set_at: u64) -> Option<Self> {
        (usd.is_finite() && usd > 0.0 && set_at > 0).then_some(Self { usd, set_at })
    }
}

/// What a balance's estimate is subtracted from, and so how far to trust it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpentSince {
    /// The organization's real spending after the balance was entered, from the cost report (Admin key): everything
    /// on the account from the next day, plus Imber's own for the rest of the entry day; minutes late.
    Organization(f64),
    /// What Imber recorded since the balance was entered: anything else billed to the key is not seen.
    ImberOnly(f64),
}

impl SpentSince {
    fn usd(self) -> f64 {
        match self {
            SpentSince::Organization(u) | SpentSince::ImberOnly(u) => u,
        }
    }
}

/// The estimate of what a balance has left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreditEstimate {
    pub left_usd: f64,
    /// Share of the balance left, 0–1.
    pub left: f64,
}

/// What is left of `balance` after `spent`.
pub fn estimate_left(balance: ManualBalance, spent: SpentSince) -> CreditEstimate {
    let left_usd = (balance.usd - spent.usd()).max(0.0);
    CreditEstimate { left_usd, left: left_usd / balance.usd }
}

/// The Anthropic entry. With a balance entered, the figure is the estimate of what is left of it — from the
/// organization's real spending when an Admin key is there, from Imber's own record otherwise, and it says which.
/// Without one, it is what is left of the tightest spending limit. Imber's spending against the limits is always
/// listed; the organization's when it is known.
pub fn anthropic_credit(
    active: bool,
    has_key: bool,
    limits: &Limits,
    spent: &Spent,
    balance: Option<ManualBalance>,
    imber_since_balance_usd: f64,
    org: &OrgSpend,
) -> ProviderCredit {
    let meters: Vec<_> = limits.meters(spent).into_iter().filter(|m| m.period != Period::Session).collect();
    // The limit closest to running out is the one that stops a run first.
    let tightest = meters
        .iter()
        .filter_map(|m| m.limit_eur.map(|limit| (m, (limit - m.spent_eur).max(0.0), limit)))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    let month = meters.iter().find(|m| m.period == Period::Month).map_or(0.0, |m| m.spent_eur);

    let since = balance.map(|_| match org {
        OrgSpend::Known { since_balance_usd: Some(usd), .. } => SpentSince::Organization(*usd),
        _ => SpentSince::ImberOnly(imber_since_balance_usd),
    });
    let estimate = balance.zip(since).map(|(b, s)| estimate_left(b, s));

    let (figure, left) = match (estimate, tightest) {
        (Some(e), _) => (format!("~${:.2} left", e.left_usd), Some(e.left)),
        (None, Some((_, left, limit))) => (format!("{} left", eur(left)), Some(left / limit)),
        (None, None) => (eur(month), None),
    };

    let mut sections = Vec::new();
    sections.push(match (balance, since, estimate) {
        (Some(b), Some(s), Some(e)) => CreditSection {
            title: match s {
                SpentSince::Organization(_) => "Credit left · estimate, organization's spending".into(),
                SpentSince::ImberOnly(_) => "Credit left · rough estimate, Imber's spending only".into(),
            },
            lines: vec![
                CreditLine { label: "Balance you entered".into(), value: format!("${:.2} · {}", b.usd, day_of(b.set_at)) },
                CreditLine {
                    label: match s {
                        SpentSince::Organization(_) => "Spent since (organization)".into(),
                        SpentSince::ImberOnly(_) => "Spent since (Imber)".into(),
                    },
                    value: format!("${:.2}", s.usd()),
                },
                CreditLine { label: "Estimated left".into(), value: format!("~${:.2}", e.left_usd) },
            ],
            note: Some(match s {
                SpentSince::Organization(_) => "Close to the real figure: everything the organization spent after you entered it, from \
                    Anthropic, a few minutes late (the day you entered it counts only Imber's spending after that moment). \
                    Nothing spent before is subtracted. Enter the balance again after a top-up."
                    .into(),
                SpentSince::ImberOnly(_) => "Only what Imber spent after you entered it is subtracted — nothing from before, \
                    older conversations included. Anything else billed to the account (Claude Code, other apps, other \
                    machines) is not seen, so the real figure is lower. Enter the balance again from the \
                    Console now and then, and after a top-up."
                    .into(),
            }),
            action: Some("set_balance".into()),
        },
        _ => CreditSection {
            title: "Credit left · not entered".into(),
            lines: Vec::new(),
            note: Some("The API does not report the account's credit. Enter the balance shown in the Claude Console and Imber \
                estimates what is left from what is spent after it."
                .into()),
            action: Some("set_balance".into()),
        },
    });
    sections.push(CreditSection {
        title: "Spent by Imber · your limits".into(),
        lines: meters
            .iter()
            .map(|m| CreditLine {
                label: period_label(m.period).into(),
                value: match m.limit_eur {
                    Some(limit) => format!("{} / {}", eur(m.spent_eur), eur(limit)),
                    None => format!("{} · no limit", eur(m.spent_eur)),
                },
            })
            .collect(),
        note: tightest
            .filter(|(m, _, _)| m.level(limits.warn_percent) != Level::Ok)
            .map(|(m, _, _)| m.describe(m.level(limits.warn_percent))),
        action: None,
    });
    sections.push(match org {
        OrgSpend::Known { month_usd, .. } => CreditSection {
            title: "Real spending · from Anthropic".into(),
            lines: vec![CreditLine { label: "This month (organization)".into(), value: format!("${month_usd:.2}") }],
            note: None,
            action: None,
        },
        OrgSpend::NoAdminKey => CreditSection {
            title: "Real spending · not available".into(),
            lines: Vec::new(),
            note: Some(
                "It needs an Admin key, which only an Anthropic organization has (admin role): an individual account cannot \
                 have one. Without it the estimate above counts only what Imber spent."
                    .into(),
            ),
            action: Some("add_admin_key".into()),
        },
        OrgSpend::Failed(why) => CreditSection {
            title: "Real spending · could not be read".into(),
            lines: Vec::new(),
            note: Some(why.clone()),
            action: Some("add_admin_key".into()),
        },
    });
    ProviderCredit {
        id: "anthropic".into(),
        name: "Anthropic".into(),
        active,
        subtitle: match (active, has_key) {
            (_, false) => "No API key".into(),
            (true, true) => "Active · API key".into(),
            (false, true) => "Not active · API key".into(),
        },
        figure,
        left,
        sections,
    }
}

/// `2026-11-03`, the UTC day of an instant.
fn day_of(ms: u64) -> String {
    rfc3339_utc(ms)[..10].to_string()
}

/// The start of the UTC day `ms` falls in, epoch milliseconds: the cost report's buckets are whole UTC days.
pub fn utc_day_start(ms: u64) -> u64 {
    ms - ms % 86_400_000
}

fn period_label(p: Period) -> &'static str {
    match p {
        Period::Session => "Conversation",
        Period::Day => "Today",
        Period::Week => "This week",
        Period::Month => "This month",
    }
}

/// The address of the organization's cost report from `starting_at` (RFC 3339), daily buckets, as many as a month
/// can have.
pub fn cost_report_url(starting_at: &str) -> String {
    format!("https://api.anthropic.com/v1/organizations/cost_report?starting_at={starting_at}&bucket_width=1d&limit=31")
}

/// The days of one page of a cost report — each bucket's `starting_at` (RFC 3339) and its dollars, summed from
/// every result's `amount`, which is in **cents** as a decimal string — and the cursor of the next page, if any.
pub fn cost_report_days(page: &Value) -> Result<(Vec<(String, f64)>, Option<String>), String> {
    let buckets = page.get("data").and_then(Value::as_array).ok_or("the cost report has no `data`")?;
    let mut days = Vec::new();
    for bucket in buckets {
        let start = bucket.get("starting_at").and_then(Value::as_str).unwrap_or("").to_string();
        let mut cents = 0.0;
        for result in bucket.get("results").and_then(Value::as_array).into_iter().flatten() {
            let amount = result.get("amount").and_then(Value::as_str).unwrap_or("0");
            cents += amount.parse::<f64>().map_err(|_| format!("`{amount}` is not an amount"))?;
        }
        days.push((start, cents / 100.0));
    }
    let next = page
        .get("has_more")
        .and_then(Value::as_bool)
        .filter(|more| *more)
        .and_then(|_| page.get("next_page").and_then(Value::as_str))
        .map(str::to_string);
    Ok((days, next))
}

/// The dollars of the days that start at or after `from` (RFC 3339, UTC). The instants are all `…Z`, so the
/// strings compare as the times do.
pub fn spent_from_day(days: &[(String, f64)], from: &str) -> f64 {
    days.iter().filter(|(start, _)| start.as_str() >= from).map(|(_, usd)| usd).sum()
}

/// Epoch milliseconds as the RFC 3339 instant the report wants (UTC, whole seconds).
pub fn rfc3339_utc(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn limits(day: f64, month: f64) -> Limits {
        Limits { session_eur: 5.0, day_eur: day, week_eur: 0.0, month_eur: month, eur_per_usd: 1.0, warn_percent: 80.0 }
    }

    fn provider(id: &str, left: Option<f64>, active: bool) -> ProviderCredit {
        ProviderCredit { id: id.into(), name: id.into(), active, subtitle: String::new(), figure: String::new(), left, sections: Vec::new() }
    }

    #[test]
    fn the_ring_is_the_average_and_the_colour_the_worst() {
        let s = summarize(vec![provider("a", Some(1.0), false), provider("b", Some(0.05), true), provider("c", None, false)]);
        assert_eq!(s.providers[0].id, "b", "the active provider comes first");
        assert!((s.average_left.unwrap() - 0.525).abs() < 1e-9, "the one with no limit stays out");
        assert_eq!(s.lowest, Some(("b".into(), 0.05)));
        assert_eq!(s.level, "warn");
        assert_eq!(summarize(vec![provider("a", None, true)]).average_left, None);
    }

    #[test]
    fn anthropic_shows_the_tightest_limit_left_without_a_balance() {
        let spent = Spent { session_usd: 0.1, day_usd: 8.0, week_usd: 9.0, month_usd: 12.0 };
        let p = anthropic_credit(true, true, &limits(10.0, 50.0), &spent, None, 0.0, &OrgSpend::NoAdminKey);
        assert_eq!(p.figure, "€2.00 left", "the day has €2 left, the month €38");
        assert!((p.left.unwrap() - 0.2).abs() < 1e-9);
        assert_eq!(p.sections[0].action.as_deref(), Some("set_balance"));
        assert_eq!(p.sections[2].action.as_deref(), Some("add_admin_key"));
        let none = anthropic_credit(true, true, &limits(0.0, 0.0), &spent, None, 0.0, &OrgSpend::Known { month_usd: 184.2, since_balance_usd: None });
        assert_eq!((none.figure.as_str(), none.left), ("€12.00", None), "no limit: what the month cost, out of the average");
        assert_eq!(none.sections[2].lines[0].value, "$184.20");
    }

    #[test]
    fn a_balance_is_estimated_from_the_organization_or_from_imber() {
        let spent = Spent::default();
        let balance = ManualBalance::from_config(50.0, 1_793_491_200_000);
        let org = OrgSpend::Known { month_usd: 30.0, since_balance_usd: Some(20.0) };
        let p = anthropic_credit(true, true, &limits(10.0, 50.0), &spent, balance, 5.0, &org);
        assert_eq!(p.figure, "~$30.00 left", "the organization's spending wins over Imber's");
        assert!((p.left.unwrap() - 0.6).abs() < 1e-9);
        assert!(p.sections[0].title.contains("organization"));
        assert_eq!(p.sections[0].lines[0].value, "$50.00 · 2026-11-01");
        let rough = anthropic_credit(true, true, &limits(10.0, 50.0), &spent, balance, 5.0, &OrgSpend::NoAdminKey);
        assert_eq!(rough.figure, "~$45.00 left");
        assert!(rough.sections[0].title.contains("rough"));
        assert!(ManualBalance::from_config(0.0, 1).is_none() && ManualBalance::from_config(10.0, 0).is_none());
        assert_eq!(estimate_left(balance.unwrap(), SpentSince::ImberOnly(80.0)).left_usd, 0.0, "never below nothing");
    }

    #[test]
    fn a_cost_report_is_read_by_day_in_dollars_and_paged() {
        let page = json!({
            "data": [
                {"starting_at": "2026-11-01T00:00:00Z", "ending_at": "2026-11-02T00:00:00Z", "results": [{"amount": "123.45", "currency": "USD"}, {"amount": "100", "currency": "USD"}]},
                {"starting_at": "2026-11-02T00:00:00Z", "ending_at": "2026-11-03T00:00:00Z", "results": [{"amount": "50", "currency": "USD"}]}
            ],
            "has_more": true, "next_page": "page_x"
        });
        let (days, next) = cost_report_days(&page).unwrap();
        assert!((spent_from_day(&days, "2026-11-01T00:00:00Z") - 2.7345).abs() < 1e-9);
        assert!((spent_from_day(&days, "2026-11-02T00:00:00Z") - 0.5).abs() < 1e-9);
        assert_eq!(next.as_deref(), Some("page_x"));
        assert!(cost_report_days(&json!({})).is_err());
        assert_eq!(utc_day_start(1_793_491_200_000 + 3_600_000), 1_793_491_200_000);
    }

    #[test]
    fn an_instant_is_written_as_rfc_3339_utc() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(1_793_491_200_000), "2026-11-01T00:00:00Z");
    }
}
