//! The credit meter's figures: what each provider Imber can use has left (see `bennu_assistant::credits`).
//!
//! Served from here because the ledger and the limits are here; Corvus's title bar borrows this backend for it.
//! Anthropic is the one provider so far. The API does not report an account's credit, so what is left is an
//! **estimate**: the balance the user entered (read in the Claude Console), less what was spent after it — the
//! organization's real spending when an Admin key is in Imber's vault (`anthropic-admin`), Imber's own record
//! otherwise. Without a balance the figure is what is left of the user's spending limits.
//!
//! The organization's figures cost an HTTP call, so they are kept for five minutes; the panel's Refresh asks again at
//! once. Nothing here is on the path of a request to the model.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use bennu_assistant::prelude::{
    anthropic_credit, cost_report_days, cost_report_url, rfc3339_utc, spent_from_day, summarize_credits, utc_day_start, ErrorKind,
    ManualBalance, OrgSpend,
};
use bennu_core::prelude::BennuState;
use serde_json::{json, Value};

use super::ledger::{self, Periods};
use super::transport::HttpTransport;

/// The vault's account for the Admin key: read-only use, never sent with a conversation.
const ADMIN_PROVIDER: &str = "anthropic-admin";
/// How long the organization's figures are reused before they are asked for again.
const ORG_TTL: Duration = Duration::from_secs(300);
/// The cost report wants a version header like any API call.
const API_VERSION: &str = "2023-06-01";
/// One day, in milliseconds.
const DAY_MS: u64 = 86_400_000;
/// Pages of 31 days followed at most: a balance entered four months ago is still covered.
const MAX_PAGES: usize = 4;

/// The last answer, with what it was asked for: the first day read, the month's day, the balance's day.
static ORG_CACHE: Mutex<Option<(Instant, (u64, u64, Option<u64>), OrgSpend)>> = Mutex::new(None);

fn key(ctx: &BennuState, provider: &str) -> Option<String> {
    ctx.host_call("__assistant_key", json!(provider))
        .ok()
        .and_then(|v| v.as_str().map(str::trim).filter(|k| !k.is_empty()).map(str::to_string))
}

/// Every day of the organization's cost report from `from` (epoch ms, a UTC day start), with the Admin key.
fn org_days(admin_key: &str, from: u64) -> Result<Vec<(String, f64)>, String> {
    let transport = HttpTransport::new()?;
    let headers = vec![("x-api-key".to_string(), admin_key.to_string()), ("anthropic-version".to_string(), API_VERSION.to_string())];
    let base = cost_report_url(&rfc3339_utc(from));
    let mut url = base.clone();
    let mut days = Vec::new();
    for _ in 0..MAX_PAGES {
        let body = transport.get_text(&url, &headers).map_err(|e| match e.kind {
            ErrorKind::Auth => "The Admin key was refused: check it is an Admin key (sk-ant-admin…) of an organization.".to_string(),
            _ => format!("The cost report could not be read: {}", e.friendly()),
        })?;
        let page: Value = serde_json::from_str(&body).map_err(|e| format!("The cost report was not JSON: {e}"))?;
        let (mut part, next) = cost_report_days(&page).map_err(|e| format!("The cost report was not understood: {e}"))?;
        days.append(&mut part);
        match next {
            Some(cursor) => url = format!("{base}&page={cursor}"),
            None => break,
        }
    }
    Ok(days)
}

/// The organization's spending this month and since the balance's day, from the cache while it is fresh and for the
/// same days.
fn org_spend(ctx: &BennuState, month_start: u64, balance_day: Option<u64>, refresh: bool) -> OrgSpend {
    let Some(admin) = key(ctx, ADMIN_PROVIDER) else {
        return OrgSpend::NoAdminKey;
    };
    // The report's buckets are whole UTC days: the month and the balance are read from the days they fall in.
    let month_day = utc_day_start(month_start);
    let from = balance_day.map_or(month_day, |b| b.min(month_day));
    let asked = (from, month_day, balance_day);
    if !refresh {
        if let Ok(cache) = ORG_CACHE.lock() {
            if let Some((at, key, spend)) = cache.as_ref() {
                if *key == asked && at.elapsed() < ORG_TTL {
                    return spend.clone();
                }
            }
        }
    }
    let spend = match org_days(&admin, from) {
        // The day the balance was entered is only partly after it, and the report cannot split a day: from it, only
        // the days after count, and the caller adds what Imber spent in the rest of that day.
        Ok(days) => OrgSpend::Known {
            month_usd: spent_from_day(&days, &rfc3339_utc(month_day)),
            since_balance_usd: balance_day.map(|b| spent_from_day(&days, &rfc3339_utc(b + DAY_MS))),
        },
        Err(why) => OrgSpend::Failed(why),
    };
    if let Ok(mut cache) = ORG_CACHE.lock() {
        *cache = Some((Instant::now(), asked, spend.clone()));
    }
    spend
}

/// What each provider has left, for the credit meter in the title bars. `periods` are where the user's day, week and
/// month began; `refresh` skips the five-minute cache of the organization's figures.
#[arbor_rpc::handler]
fn imber_credits(ctx: &BennuState, periods: Periods, refresh: Option<bool>) -> Result<Value, String> {
    let limits = super::limits_now();
    let config = bennu_core::config::load().assistant.limits;
    let balance = ManualBalance::from_config(config.credit_usd, config.credit_set_at);
    let spent = ledger::spent(&periods, 0.0);
    let imber_since = balance.map_or(0.0, |b| ledger::spent_since(b.set_at));
    let has_key = key(ctx, super::PROVIDER).is_some();
    let mut org = org_spend(ctx, periods.month, balance.map(|b| utc_day_start(b.set_at)), refresh.unwrap_or(false));
    // Nothing spent before the balance was entered is subtracted from it — the conversations and runs that came
    // first are already in the figure the Console showed. The organization's report is read from the day after;
    // the rest of the entry day is what Imber spent from the moment the balance was saved.
    if let (Some(b), OrgSpend::Known { since_balance_usd: Some(after), .. }) = (balance, &mut org) {
        *after += ledger::spent_between(b.set_at, utc_day_start(b.set_at) + DAY_MS);
    }
    let anthropic = anthropic_credit(true, has_key, &limits, &spent, balance, imber_since, &org);
    serde_json::to_value(summarize_credits(vec![anthropic])).map_err(|e| e.to_string())
}
