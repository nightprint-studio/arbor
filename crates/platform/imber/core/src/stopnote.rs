//! Telling the assistant that the program it is debugging stopped — the pure rules.
//!
//! The snapshot of a stop is taken by Bennu the moment it happens, so nothing depends on the model being awake:
//! where it stopped, the top of the stack, the values the agent asked to be captured, and the top frame's
//! locals — all capped, and with the values of secret-looking names masked. How it reaches the agent depends on
//! what the owning conversation is doing ([`decide_delivery`]); whether a stop may *start* a run by itself is
//! opt-in and rate-limited ([`auto_continue_allowed`], [`RateLimiter`]).

use std::collections::HashMap;

use crate::attribution::Note;

pub const MAX_FRAMES: usize = 8;
pub const MAX_LOCALS: usize = 20;
pub const MAX_CAPTURES: usize = 10;
pub const MAX_VALUE_CHARS: usize = 200;
/// Automatic runs one conversation may start per window.
pub const MAX_AUTO_CONTINUES: usize = 3;
pub const AUTO_WINDOW_MS: u64 = 60 * 60 * 1000;

/// What the stop looked like when it happened.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StopSnapshot {
    /// `Order.java:42 in OrderService.total`.
    pub location: String,
    /// Breakpoint, step, exception.
    pub reason: String,
    pub frames: Vec<String>,
    /// The values the breakpoint asked for: (expression, rendered value).
    pub captured: Vec<(String, String)>,
    /// The top frame's visible locals.
    pub locals: Vec<(String, String)>,
}

/// A name whose value should not be shown (the same idea as the debug tools' masking).
pub fn is_secret_name(name: &str) -> bool {
    let n = name.to_lowercase();
    ["password", "passwd", "secret", "token", "apikey", "api_key", "credential", "privatekey", "private_key"].iter().any(|s| n.contains(s))
}

fn cap(value: &str) -> String {
    let flat: String = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX_VALUE_CHARS {
        return flat;
    }
    let mut cut: String = flat.chars().take(MAX_VALUE_CHARS).collect();
    cut.push('…');
    cut
}

fn shown(name: &str, value: &str) -> String {
    if is_secret_name(name) { "***".to_string() } else { cap(value) }
}

/// The snapshot as a system note: capped and masked.
pub fn format_snapshot(s: &StopSnapshot) -> String {
    let mut out = format!("Bennu note from the debugger: the program stopped at {} ({}).\n", s.location, s.reason);
    if !s.frames.is_empty() {
        out.push_str("Stack:\n");
        for f in s.frames.iter().take(MAX_FRAMES) {
            out.push_str(&format!("  {}\n", cap(f)));
        }
        if s.frames.len() > MAX_FRAMES {
            out.push_str(&format!("  … {} more frames (debug_stack)\n", s.frames.len() - MAX_FRAMES));
        }
    }
    out.push_str(&format_values(s));
    out.push_str("The program is suspended; debug_variables, debug_evaluate, debug_step and debug_resume work on it.");
    out
}

/// Only the values of a snapshot — what the breakpoint asked to capture, then the top frame's locals — for a
/// reader that already has the stack (the wait that returns the stop). Empty when there are none.
pub fn format_values(s: &StopSnapshot) -> String {
    let mut out = String::new();
    if !s.captured.is_empty() {
        out.push_str("Captured:\n");
        for (e, v) in s.captured.iter().take(MAX_CAPTURES) {
            out.push_str(&format!("  {e} = {}\n", shown(e, v)));
        }
    }
    if !s.locals.is_empty() {
        out.push_str("Locals of the top frame:\n");
        for (n, v) in s.locals.iter().take(MAX_LOCALS) {
            out.push_str(&format!("  {n} = {}\n", shown(n, v)));
        }
        if s.locals.len() > MAX_LOCALS {
            out.push_str(&format!("  … {} more (debug_variables)\n", s.locals.len() - MAX_LOCALS));
        }
    }
    out
}

/// A one-line card text for the thread: `Stopped at Order.java:42 in OrderService.total`.
pub fn card_title(s: &StopSnapshot) -> String {
    format!("Stopped at {}", s.location)
}

/// The note for the inbox, hashed on the place and the hit number so one hit is delivered once.
pub fn stop_note(s: &StopSnapshot, hit: u64) -> Note {
    let hash = crate::worktree::fnv64(format!("{}|{}|{}", s.location, s.reason, hit).as_bytes());
    Note { text: format_snapshot(s), hash }
}

/// What the owning conversation is doing when the stop arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerState {
    /// A run is active and the agent is blocked in `debug_wait_stop`.
    WaitingForStop,
    /// A run is active and the agent is busy with something else.
    RunBusy,
    /// No run is active.
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Returned by the wait that is blocked, as today.
    ReturnFromWait,
    /// Appended to the next tool result as a system note.
    AppendToNextToolResult,
    /// Queued for the next prompt, with a visible notice in the thread and a "send now" button.
    QueueAndNotify,
}

pub fn decide_delivery(state: OwnerState) -> Delivery {
    match state {
        OwnerState::WaitingForStop => Delivery::ReturnFromWait,
        OwnerState::RunBusy => Delivery::AppendToNextToolResult,
        OwnerState::Idle => Delivery::QueueAndNotify,
    }
}

/// Whether a stop that arrives while idle may start a run by itself: only when the user turned it on (the global
/// setting, overridable per conversation) and the conversation is idle.
pub fn auto_continue_allowed(global: bool, chat_override: Option<bool>, idle: bool) -> bool {
    idle && chat_override.unwrap_or(global)
}

/// At most `max` events per `window_ms` per key, with the clock passed in.
#[derive(Debug, Default)]
pub struct RateLimiter {
    seen: HashMap<String, Vec<u64>>,
}

impl RateLimiter {
    /// Record an event for `key` at `now_ms` if allowed; false when the window is full.
    pub fn try_acquire(&mut self, key: &str, now_ms: u64, max: usize, window_ms: u64) -> bool {
        let times = self.seen.entry(key.to_string()).or_default();
        times.retain(|t| now_ms.saturating_sub(*t) < window_ms);
        if times.len() >= max {
            return false;
        }
        times.push(now_ms);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> StopSnapshot {
        StopSnapshot {
            location: "Order.java:42 in OrderService.total".into(),
            reason: "breakpoint".into(),
            frames: (0..12).map(|i| format!("frame{i}")).collect(),
            captured: vec![("order.id".into(), "17".into()), ("user.password".into(), "hunter2".into())],
            locals: (0..25).map(|i| (format!("v{i}"), format!("{i}"))).collect(),
        }
    }

    #[test]
    fn the_snapshot_is_capped_in_frames_locals_and_value_length() {
        let text = format_snapshot(&snap());
        assert!(text.contains("frame7") && !text.contains("frame8"));
        assert!(text.contains("4 more frames"));
        assert!(text.contains("v19") && !text.contains("v20 ="));
        assert!(text.contains("5 more"));
        let mut long = snap();
        long.captured = vec![("blob".into(), "x".repeat(MAX_VALUE_CHARS * 3))];
        let t = format_snapshot(&long);
        assert!(t.contains('…') && !t.contains(&"x".repeat(MAX_VALUE_CHARS + 1)));
    }

    #[test]
    fn secret_looking_names_are_masked_wherever_they_appear() {
        let mut s = snap();
        s.locals.push(("apiKey".into(), "sk-live-123".into()));
        s.locals.truncate(MAX_LOCALS - 1);
        s.locals.push(("apiKey".into(), "sk-live-123".into()));
        let text = format_snapshot(&s);
        assert!(!text.contains("hunter2") && !text.contains("sk-live-123"));
        assert!(text.contains("user.password = ***"));
    }

    #[test]
    fn the_values_alone_are_capped_and_masked_like_the_whole_note() {
        let text = format_values(&snap());
        assert!(text.starts_with("Captured:") && text.contains("order.id = 17") && text.contains("user.password = ***"));
        assert!(!text.contains("Stack:") && !text.contains("suspended"));
        assert_eq!(format_values(&StopSnapshot::default()), "");
        assert_eq!(card_title(&snap()), "Stopped at Order.java:42 in OrderService.total");
    }

    #[test]
    fn a_hit_is_delivered_once() {
        let inbox = crate::attribution::Inbox::default();
        assert!(inbox.push("c", stop_note(&snap(), 1)));
        assert!(!inbox.push("c", stop_note(&snap(), 1)), "same hit");
        assert!(inbox.push("c", stop_note(&snap(), 2)), "next hit");
    }

    #[test]
    fn delivery_follows_what_the_owner_is_doing() {
        assert_eq!(decide_delivery(OwnerState::WaitingForStop), Delivery::ReturnFromWait);
        assert_eq!(decide_delivery(OwnerState::RunBusy), Delivery::AppendToNextToolResult);
        assert_eq!(decide_delivery(OwnerState::Idle), Delivery::QueueAndNotify);
    }

    #[test]
    fn auto_continue_is_off_unless_asked_for_and_only_when_idle() {
        assert!(!auto_continue_allowed(false, None, true), "off by default");
        assert!(auto_continue_allowed(true, None, true));
        assert!(!auto_continue_allowed(true, None, false), "a run is active");
        assert!(!auto_continue_allowed(true, Some(false), true), "the chat opted out");
        assert!(auto_continue_allowed(false, Some(true), true), "the chat opted in");
    }

    #[test]
    fn at_most_three_automatic_runs_an_hour_per_conversation() {
        let mut rl = RateLimiter::default();
        let h = AUTO_WINDOW_MS;
        for i in 0..MAX_AUTO_CONTINUES as u64 {
            assert!(rl.try_acquire("c1", i * 1000, MAX_AUTO_CONTINUES, h));
        }
        assert!(!rl.try_acquire("c1", 10_000, MAX_AUTO_CONTINUES, h));
        assert!(rl.try_acquire("c2", 10_000, MAX_AUTO_CONTINUES, h), "another conversation has its own");
        assert!(rl.try_acquire("c1", h + 1, MAX_AUTO_CONTINUES, h), "the window moved on");
    }
}
