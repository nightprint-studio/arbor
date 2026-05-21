//! `[scheduler]` manifest section + the Lua-registered schedule shapes.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// `[scheduler]` section in `plugin.toml`. The manifest only gates the feature
/// on or off — every concrete schedule (action name, interval / cron, focus
/// gate, etc.) is registered from Lua via `arbor.scheduler.register`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginSchedulerSection {
    #[serde(default)]
    pub enabled: bool,
}

/// Spring-style trigger types supported by `arbor.scheduler.register`.
///
///   FixedRate  — fire every N seconds, regardless of how long the previous
///                handler took (next fire = previous start + interval).
///   FixedDelay — wait N seconds AFTER the previous handler returned before
///                firing again (next fire = previous end + delay).
///   Cron       — 6-field Spring cron expression: `sec min hour dom mon dow`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleTrigger {
    FixedRate  { interval_sec: u64 },
    FixedDelay { delay_sec: u64 },
    Cron       { expr: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginSchedule {
    /// Action name fired on the plugin when the trigger elapses.
    pub action: String,
    /// What drives the firing cadence.
    pub trigger: ScheduleTrigger,
    /// Initial wait (seconds) before the first fire. Applies to fixed_rate
    /// and fixed_delay triggers; ignored for cron (cron always uses the next
    /// matching wall-clock instant).
    #[serde(default)]
    pub initial_delay_sec: u64,
    /// If true, fire immediately on plugin load before the first interval
    /// (in addition to the normal cadence — useful for "warm caches now then
    /// keep them warm").
    #[serde(default)]
    pub on_load: bool,
    /// If true, skip firing when the app window is not focused or is minimised.
    /// The clock keeps ticking; the action is simply skipped that tick and
    /// fires normally on the next cycle once focus is restored.
    #[serde(default)]
    pub only_when_focused: bool,
}

/// Shared registry of plugin schedules. Populated from Lua via
/// `arbor.scheduler.register` and consumed by `PluginHost::start_*_schedulers`.
pub type ScheduleRegistry = Arc<Mutex<Vec<PluginSchedule>>>;

/// One scheduler entry as exposed to the frontend — combines the static
/// declaration with the live running state so the Plugin Info modal can render
/// a per-action enable/disable toggle.
#[derive(Debug, Clone, Serialize)]
pub struct PluginScheduleStatus {
    #[serde(flatten)]
    pub schedule: PluginSchedule,
    pub running:  bool,
}

/// Parse a duration string used by `arbor.scheduler.register`. Accepts:
///   - bare integer seconds: `"60"`
///   - suffixed: `"30s"`, `"5m"`, `"2h"`, `"1d"`
///   - ISO-8601 short forms: `"PT30S"`, `"PT5M"`, `"PT1H30M"`
pub fn parse_duration_secs(input: &str) -> std::result::Result<u64, String> {
    let s = input.trim();
    if s.is_empty() { return Err("empty duration".into()); }

    // ISO-8601: PT[xH][yM][zS]
    if let Some(rest) = s.strip_prefix("PT").or_else(|| s.strip_prefix("pt")) {
        let mut total: u64 = 0;
        let mut acc = String::new();
        for c in rest.chars() {
            if c.is_ascii_digit() {
                acc.push(c);
            } else {
                let n: u64 = acc.parse().map_err(|_| {
                    format!("invalid number in ISO duration: '{input}'")
                })?;
                acc.clear();
                match c.to_ascii_uppercase() {
                    'H' => total = total.saturating_add(n.saturating_mul(3600)),
                    'M' => total = total.saturating_add(n.saturating_mul(60)),
                    'S' => total = total.saturating_add(n),
                    other => return Err(format!(
                        "unknown ISO duration unit '{other}' in '{input}'"
                    )),
                }
            }
        }
        if !acc.is_empty() {
            return Err(format!("trailing digits without unit in '{input}'"));
        }
        return Ok(total);
    }

    // Suffix form: <number><unit>
    let (num_part, unit) = match s.chars().last() {
        Some(c) if c.is_ascii_alphabetic() => {
            let unit = c.to_ascii_lowercase();
            (&s[..s.len() - c.len_utf8()], Some(unit))
        }
        _ => (s, None),
    };
    let n: u64 = num_part.trim().parse().map_err(|_| {
        format!("invalid duration '{input}' — expected number with optional s/m/h/d suffix")
    })?;
    let secs = match unit {
        None | Some('s') => n,
        Some('m')        => n.saturating_mul(60),
        Some('h')        => n.saturating_mul(3600),
        Some('d')        => n.saturating_mul(86_400),
        Some(other)      => return Err(format!(
            "unknown duration unit '{other}' in '{input}' (expected s/m/h/d)"
        )),
    };
    Ok(secs)
}
