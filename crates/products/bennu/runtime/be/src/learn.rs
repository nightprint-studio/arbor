//! `learn` — what became of each ghost text once the user was done with it, handed to `bennu-learn`.
//!
//! The editor watches a suggestion after it is taken and, when the user has moved on, says what stood in its place.
//! This module is the thin door to the learner: it reads the two switches from the config on every call (so turning
//! them off takes effect at once and creates no file), keeps one learner for the process, and writes its counters at
//! most every [`WRITE_EVERY`] or when the editor says it is going away.
//!
//! Nothing here is on the keystroke path: batches arrive every few seconds, and the text in them is classified and
//! dropped unless the user also asked for the raw log.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bennu_core::prelude::BennuState;
use bennu_learn::prelude::{classify, Event, Learner, Outcome, Settings};
use serde::Deserialize;

/// Counters are written at most this often while events keep arriving.
const WRITE_EVERY: Duration = Duration::from_secs(30);
/// Longest origin tag taken: the tags are a short fixed vocabulary, and anything else is not one.
const MAX_ORIGIN: usize = 24;

struct Held {
    learner: Learner,
    last_write: Instant,
}

fn held() -> &'static Mutex<Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(|| {
        let dir = arbor_core::prelude::bennu_config_path("learn");
        Mutex::new(Held { learner: Learner::open(dir, Settings::default()), last_write: Instant::now() })
    })
}

fn settings() -> Settings {
    let cfg = bennu_core::config::load();
    Settings { enabled: cfg.flc_learn, keep_text: cfg.flc_learn && cfg.flc_learn_keep_text }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn is_origin(tag: &str) -> bool {
    !tag.is_empty() && tag.len() <= MAX_ORIGIN && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The share of one kind of suggestion that was useful as far as it went (kept, written on, or at least its start kept),
/// once enough have been seen; `None` while *Learn from corrections* is off or there is too little to say.
pub(crate) fn useful_rate(origin: &str, context: &str) -> Option<f32> {
    if !settings().enabled {
        return None;
    }
    let h = held().lock().unwrap_or_else(|p| p.into_inner());
    h.learner.useful_rate(origin, context)
}

/// What the corrections counted, for the page that shows it: `(origin, context, events, useful share)`.
pub(crate) fn corrections() -> Vec<(String, String, f32, Option<f32>, String)> {
    let h = held().lock().unwrap_or_else(|p| p.into_inner());
    h.learner.entries().map(|(o, c, t)| (o.to_string(), c.to_string(), t.total(), t.useful_rate(), breakdown(t))).collect()
}

/// What became of the suggestions of one kind in one place, as a line: `kept 3 · cut short 1 · ignored 4`.
fn breakdown(t: &bennu_learn::prelude::Tally) -> String {
    [
        ("kept", t.kept), ("written on", t.extended), ("cut short", t.shortened), ("re-laid out", t.reformatted),
        ("changed in the middle", t.replaced), ("taken back", t.undone), ("ignored", t.ignored),
    ]
    .iter()
    .filter(|(_, n)| n.round() >= 1.0)
    .map(|(label, n)| format!("{label} {:.0}", n))
    .collect::<Vec<_>>()
    .join(" · ")
}

/// What the counters of corrections take: how many (origin, context) pairs, and their estimated size.
pub(crate) fn memory_item() -> arbor_be::prelude::MemoryItem {
    use crate::flc_memory::text;
    let h = held().lock().unwrap_or_else(|p| p.into_inner());
    let (n, bytes) = h
        .learner
        .entries()
        .fold((0usize, 0usize), |(n, b), (o, c, _)| (n + 1, b + text(o) + text(c) + std::mem::size_of::<bennu_learn::prelude::Tally>() + 48));
    arbor_be::prelude::MemoryItem::estimate("process", "Learned · what you did with suggestions (counters)", n, bytes)
}

/// Forgets what was counted for one origin in one context; written out at once, since the page that asked is showing it.
pub(crate) fn forget_correction(origin: &str, context: &str) {
    let mut h = held().lock().unwrap_or_else(|p| p.into_inner());
    if h.learner.forget(origin, context) {
        let _ = h.learner.flush();
    }
}

/// Whether *Learn from corrections* is on.
pub(crate) fn enabled() -> bool {
    settings().enabled
}

/// What a suggestion at `offset` was for, in a form that can be counted: the call whose argument it is
/// (`filter#0`, the method name and the argument's index), or `-` anywhere else. A name a project writes, never a path
/// or a line of code.
pub(crate) fn context_of(source: &str, offset: usize) -> String {
    match bennu_query::prelude::call_site_name_and_index(source, offset) {
        Some((callee, index, _)) => {
            let callee: String = callee.chars().filter(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | ' ')).take(48).collect();
            if callee.is_empty() { "-".to_string() } else { format!("{callee}#{index}") }
        }
        None => "-".to_string(),
    }
}

#[derive(Deserialize)]
pub struct SettledEvent {
    pub origin: String,
    pub ghost: String,
    /// What the suggestion was for, as the backend gave it (see [`context_of`]).
    #[serde(default)]
    pub context: String,
    /// What stood where the ghost was drawn; `None` when it was never taken.
    pub after: Option<String>,
}

#[derive(Deserialize)]
pub struct SettledArgs {
    pub events: Vec<SettledEvent>,
    /// Write to disk now (the editor is going away), instead of waiting for the next interval.
    #[serde(default)]
    pub flush: bool,
}

/// How an outcome reads in a notification.
fn outcome_label(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Ignored => "ignored",
        Outcome::Kept => "kept",
        Outcome::Reformatted { .. } => "re-laid out",
        Outcome::Shortened { .. } => "cut short",
        Outcome::Extended { .. } => "written on",
        Outcome::Replaced { .. } => "changed in the middle",
        Outcome::Undone => "taken back",
    }
}

/// Counts what became of a batch of ghost texts. A no-op while *Learn from corrections* is off.
///
/// Answers with one line per event counted when the debugging switch `flc_learn_notify` is on (the origin, the
/// context, what happened, how many of that kind have been seen) and with nothing otherwise. Names only, never the
/// text of a suggestion.
#[arbor_rpc::handler]
fn bennu_learn_settled(_ctx: &BennuState, args: SettledArgs) -> Result<Vec<String>, String> {
    let wanted = settings();
    if !wanted.enabled {
        return Ok(Vec::new());
    }
    let notify = bennu_core::config::load().flc_learn_notify;
    let mut notes = Vec::new();
    let mut h = held().lock().unwrap_or_else(|p| p.into_inner());
    h.learner.set_settings(wanted);
    let at = now();
    for e in args.events.iter().filter(|e| is_origin(&e.origin)) {
        let counted = h.learner.record(&Event { origin: &e.origin, context: &e.context, ghost: &e.ghost, after: e.after.as_deref() }, at);
        if notify && counted {
            let seen = h.learner.tally(&e.origin, &e.context).map_or(0.0, |t| t.total());
            let outcome = outcome_label(&classify(&e.ghost, e.after.as_deref()));
            notes.push(format!("{} · {} · {} (seen {:.0})", e.origin, e.context, outcome, seen));
        }
    }
    if args.flush || h.last_write.elapsed() >= WRITE_EVERY {
        // a counter file that cannot be written costs the counters, never the editor
        let _ = h.learner.flush();
        h.last_write = Instant::now();
    }
    Ok(notes)
}

/// Forgets everything learned from corrections, counters and raw log, on disk too.
#[arbor_rpc::handler]
fn bennu_learn_reset(_ctx: &BennuState) -> Result<(), String> {
    let mut h = held().lock().unwrap_or_else(|p| p.into_inner());
    h.learner.reset().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::is_origin;

    #[test]
    fn a_call_argument_is_counted_by_method_and_position_and_anything_else_by_a_dash() {
        let src = "class A { void f() { list.filter(x, ";
        assert_eq!(super::context_of(src, src.len()), "filter#1");
        assert_eq!(super::context_of("class A { int x", 15), "-");
    }

    #[test]
    fn only_short_plain_tags_are_origins() {
        assert!(is_origin("model"));
        assert!(is_origin("array_constructor"));
        assert!(!is_origin(""));
        assert!(!is_origin("a\tb"));
        assert!(!is_origin(&"x".repeat(25)));
    }
}
