//! The models on offer and what a request to each one costs.
//!
//! The table is data, not code: [`DEFAULT_TOML`] ships with the build and a user file next to Bennu's
//! config can override or add entries, so a price change or a new model needs an edit and not a release.
//! This module owns the *shape* of an entry and the arithmetic; reading the files is the host's job
//! (it has the filesystem and the TOML parser), which keeps this crate free of both.
//!
//! Cost is worked out **per request**, never from a conversation's totals, because some models charge
//! a higher rate for a request whose prompt is long: summing first would price a hundred short requests
//! as if each had been long, or the reverse.

use serde::{Deserialize, Serialize};

use crate::effort::Effort;
use crate::provider::Usage;

/// The built-in table.
pub const DEFAULT_TOML: &str = include_str!("../models.toml");

/// A set of rates, in dollars per million tokens. Cache rates default to the provider's usual ratios
/// of the input price when left out.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write_5m: Option<f64>,
    #[serde(default)]
    pub cache_write_1h: Option<f64>,
    /// Only for `long_prompt`: the rates apply to a request whose prompt is longer than this.
    #[serde(default)]
    pub over_tokens: Option<u64>,
}

impl Rates {
    fn cache_read(&self) -> f64 {
        self.cache_read.unwrap_or(self.input * 0.1)
    }
    fn cache_write_5m(&self) -> f64 {
        self.cache_write_5m.unwrap_or(self.input * 1.25)
    }
    fn cache_write_1h(&self) -> f64 {
        self.cache_write_1h.unwrap_or(self.input * 2.0)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tagline: String,
    /// The largest window the model accepts, in tokens.
    pub context_max: u64,
    /// The effort levels it accepts; empty when it has none.
    #[serde(default)]
    pub efforts: Vec<Effort>,
    pub price: Rates,
    /// Rates for a request whose prompt is longer than its `over_tokens`.
    #[serde(default)]
    pub long_prompt: Option<Rates>,
}

impl ModelInfo {
    /// The rates a request with `prompt_tokens` of prompt is charged at.
    fn rates_for(&self, prompt_tokens: u64) -> &Rates {
        match &self.long_prompt {
            Some(long) if long.over_tokens.is_some_and(|t| prompt_tokens > t) => long,
            _ => &self.price,
        }
    }

    /// What **one request** cost, in dollars, from the usage the provider reported for it.
    ///
    /// The prompt is the input that was not cached plus what was read from or written to the cache —
    /// the provider reports them separately, and the long-prompt rate looks at their sum. Cache writes
    /// are split by lifetime: `cache_write_1h_tokens` is the part of `cache_write_tokens` written to the
    /// one-hour cache, the rest went to the five-minute one.
    pub fn cost(&self, u: &Usage) -> f64 {
        let prompt = u.input_tokens + u.cache_read_tokens + u.cache_write_tokens;
        let r = self.rates_for(prompt);
        let write_1h = u.cache_write_1h_tokens.min(u.cache_write_tokens);
        let write_5m = u.cache_write_tokens - write_1h;
        let per = |tokens: u64, price: f64| tokens as f64 / 1_000_000.0 * price;
        per(u.input_tokens, r.input)
            + per(u.output_tokens, r.output)
            + per(u.cache_read_tokens, r.cache_read())
            + per(write_5m, r.cache_write_5m())
            + per(write_1h, r.cache_write_1h())
    }

    /// The sum of [`Self::cost`] over a run's requests.
    pub fn cost_of_all(&self, requests: &[Usage]) -> f64 {
        requests.iter().map(|u| self.cost(u)).sum()
    }
}

/// The file's shape: a list of `[[model]]` tables.
#[derive(Debug, Deserialize)]
pub struct ModelFile {
    #[serde(default)]
    pub model: Vec<ModelInfo>,
}

/// Overlay `user` on `defaults`: an entry with the id of a default replaces it in place, any other is
/// added at the end. Replacing whole (not field by field) keeps an entry readable on its own — what
/// the file says is what applies.
pub fn merge(mut defaults: Vec<ModelInfo>, user: Vec<ModelInfo>) -> Vec<ModelInfo> {
    for m in user {
        match defaults.iter_mut().find(|d| d.id == m.id) {
            Some(slot) => *slot = m,
            None => defaults.push(m),
        }
    }
    defaults
}

pub fn find<'a>(models: &'a [ModelInfo], id: &str) -> Option<&'a ModelInfo> {
    models.iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rates(input: f64, output: f64) -> Rates {
        Rates { input, output, cache_read: None, cache_write_5m: None, cache_write_1h: None, over_tokens: None }
    }

    fn model(id: &str) -> ModelInfo {
        ModelInfo {
            id: id.into(), name: id.into(), tagline: String::new(), context_max: 200_000, efforts: vec![],
            price: rates(4.0, 20.0), long_prompt: None,
        }
    }

    fn usage(input: u64, output: u64, read: u64, write: u64, write_1h: u64) -> Usage {
        Usage { input_tokens: input, output_tokens: output, cache_read_tokens: read, cache_write_tokens: write, cache_write_1h_tokens: write_1h }
    }

    #[test]
    fn cache_rates_default_to_the_usual_ratios_of_the_input_price() {
        let m = model("m");
        // 1M of each: 4 in + 20 out + 0.4 read (0.1x) + 5 write (1.25x)
        let c = m.cost(&usage(1_000_000, 1_000_000, 1_000_000, 1_000_000, 0));
        assert!((c - (4.0 + 20.0 + 0.4 + 5.0)).abs() < 1e-9, "{c}");
    }

    #[test]
    fn stated_cache_rates_beat_the_defaults() {
        let mut m = model("m");
        m.price.cache_read = Some(0.2);
        assert!((m.cost(&usage(0, 0, 1_000_000, 0, 0)) - 0.2).abs() < 1e-9);
    }

    #[test]
    fn the_one_hour_cache_is_charged_at_its_own_rate() {
        let mut m = model("m");
        m.price.cache_write_5m = Some(5.0);
        m.price.cache_write_1h = Some(8.0);
        // 1M written, 400k of it to the one-hour cache.
        let c = m.cost(&usage(0, 0, 0, 1_000_000, 400_000));
        assert!((c - (0.6 * 5.0 + 0.4 * 8.0)).abs() < 1e-9, "{c}");
    }

    #[test]
    fn a_long_prompt_is_charged_the_higher_rate_for_the_whole_request() {
        let mut m = model("m");
        m.price = rates(0.10, 0.50);
        m.long_prompt = Some(Rates { over_tokens: Some(100_000), ..rates(0.50, 2.50) });
        let short = m.cost(&usage(50_000, 1_000, 0, 0, 0));
        let long = m.cost(&usage(150_000, 1_000, 0, 0, 0));
        assert!((short - (0.05 * 0.10 + 0.001 * 0.50)).abs() < 1e-9, "{short}");
        assert!((long - (0.15 * 0.50 + 0.001 * 2.50)).abs() < 1e-9, "{long}");
    }

    #[test]
    fn the_prompt_for_the_threshold_includes_what_came_from_the_cache() {
        let mut m = model("m");
        m.long_prompt = Some(Rates { over_tokens: Some(100_000), ..rates(8.0, 40.0) });
        // 10k fresh input, but 120k read from the cache: the prompt is 130k.
        let c = m.cost(&usage(10_000, 0, 120_000, 0, 0));
        assert!(c > 0.01 * 8.0 / 1000.0 * 1000.0 - 1e-9 && (c - (0.01 * 8.0 + 0.12 * 0.8)).abs() < 1e-9, "{c}");
    }

    #[test]
    fn requests_are_priced_one_at_a_time() {
        let mut m = model("m");
        m.price = rates(1.0, 0.0);
        m.long_prompt = Some(Rates { over_tokens: Some(100_000), ..rates(10.0, 0.0) });
        // Two requests of 60k: neither is long, though together they are 120k.
        let each = usage(60_000, 0, 0, 0, 0);
        let together = m.cost_of_all(&[each, each]);
        assert!((together - 0.12).abs() < 1e-9, "{together}");
        assert!(m.cost(&usage(120_000, 0, 0, 0, 0)) > together, "summing first would have overcharged");
    }

    #[test]
    fn more_one_hour_tokens_than_written_cannot_make_a_negative_charge() {
        let m = model("m");
        assert!(m.cost(&usage(0, 0, 0, 100, 500)) >= 0.0);
    }

    #[test]
    fn a_user_entry_replaces_a_default_whole_and_a_new_id_is_added() {
        let mut changed = model("a");
        changed.name = "Mine".into();
        let merged = merge(vec![model("a"), model("b")], vec![changed, model("c")]);
        assert_eq!(merged.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(merged[0].name, "Mine");
    }

    #[test]
    fn the_efforts_in_the_file_are_the_efforts_the_provider_knows() {
        let m: ModelInfo = serde_json::from_value(serde_json::json!({
            "id": "x", "name": "X", "context_max": 1000, "efforts": ["low", "xhigh"],
            "price": { "input": 1.0, "output": 2.0 },
        }))
        .unwrap();
        assert_eq!(m.efforts, vec![Effort::Low, Effort::XHigh]);
    }
}
