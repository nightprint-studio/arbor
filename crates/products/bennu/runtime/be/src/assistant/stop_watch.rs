//! Telling a conversation that the program it is debugging stopped.
//!
//! The stop is noticed by the debugger itself (`debug.rs`), which takes the snapshot while the thread is still
//! suspended — nothing here depends on the model being awake. What this module decides is how it reaches the
//! owning conversation, by the pure rules in `bennu_assistant::stopnote`:
//!
//! * the agent is blocked in a wait → the wait returns the values itself ([`take_values`]); nothing is queued;
//! * a run is active but busy → a system note joins the inbox and is appended to its next tool result;
//! * no run → the same note is queued for the next prompt and the thread gets a card ("Stopped at …") with a
//!   button that starts a run, and — only when the user allowed it — a run may start by itself
//!   ([`assistant_stop_auto_ok`]: the setting, the per-chat override and the hourly limit).
//!
//! A stop also renews the idle lease of a parked session (see `tools::debug_keep`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use arbor_ipc::prelude::EventSink;
use bennu_assistant::prelude::{
    auto_continue_allowed, card_title, decide_delivery, format_snapshot, format_values, stop_note, Delivery, OwnerState, RateLimiter,
    StopSnapshot, AUTO_WINDOW_MS, MAX_AUTO_CONTINUES,
};
use bennu_core::prelude::BennuState;
use serde_json::{json, Value};

/// `arbor://bennu/assistant-stop`: the program of an idle conversation stopped (the thread shows a card).
pub(crate) const EVT_STOP: &str = "arbor://bennu/assistant-stop";

struct Watch {
    conversation: String,
    title: String,
    sink: Arc<dyn EventSink>,
}

#[derive(Default)]
struct State {
    watches: HashMap<String, Watch>,
    /// Waits in flight per conversation (`debug_wait_stop`, and the wait inside `debug_step`).
    waiting: HashMap<String, u32>,
    /// The latest snapshot per debug session, until a wait takes it.
    latest: HashMap<String, StopSnapshot>,
    last_stop_ms: HashMap<String, u64>,
    limiter: RateLimiter,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(Default::default);
static HITS: AtomicU64 = AtomicU64::new(0);

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|p| p.into_inner())
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Start following a debug session on behalf of a conversation.
pub(crate) fn watch(session_id: &str, conversation: &str, title: &str, sink: Arc<dyn EventSink>) {
    state().watches.insert(session_id.to_string(), Watch { conversation: conversation.to_string(), title: title.to_string(), sink });
}

/// The session ended.
pub(crate) fn unwatch(session_id: &str) {
    let mut st = state();
    st.watches.remove(session_id);
    st.latest.remove(session_id);
}

/// Whether a stop of this session is worth a snapshot (an assistant conversation owns it).
pub(crate) fn is_watched(session_id: &str) -> bool {
    state().watches.contains_key(session_id)
}

/// The agent is blocked waiting for a stop until the guard drops.
pub(crate) struct WaitGuard(String);

pub(crate) fn waiting(conversation: &str) -> WaitGuard {
    *state().waiting.entry(conversation.to_string()).or_insert(0) += 1;
    WaitGuard(conversation.to_string())
}

impl Drop for WaitGuard {
    fn drop(&mut self) {
        let mut st = state();
        if let Some(n) = st.waiting.get_mut(&self.0) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                st.waiting.remove(&self.0);
            }
        }
    }
}

/// When the conversation's session last stopped, for the idle lease.
pub(crate) fn last_stop_ms(conversation: &str) -> Option<u64> {
    state().last_stop_ms.get(conversation).copied()
}

/// The captured values and locals of the latest stop of a session, formatted, consumed once — appended to what
/// the wait returns so the model reads them without asking.
pub(crate) fn take_values(session_id: &str) -> String {
    state().latest.remove(session_id).map(|s| format_values(&s)).unwrap_or_default()
}

fn owner_state(st: &State, conversation: &str) -> OwnerState {
    if st.waiting.get(conversation).is_some_and(|n| *n > 0) {
        OwnerState::WaitingForStop
    } else if super::worktree::is_running(conversation) {
        OwnerState::RunBusy
    } else {
        OwnerState::Idle
    }
}

/// The program of `session_id` stopped and `snapshot` was taken. Delivers it as the owner's state asks.
pub(crate) fn on_stopped(session_id: &str, snapshot: StopSnapshot) {
    let hit = HITS.fetch_add(1, Ordering::SeqCst) + 1;
    let (conversation, title, sink, delivery) = {
        let mut st = state();
        let Some(w) = st.watches.get(session_id) else { return };
        let (conversation, title, sink) = (w.conversation.clone(), w.title.clone(), Arc::clone(&w.sink));
        let delivery = decide_delivery(owner_state(&st, &conversation));
        st.last_stop_ms.insert(conversation.clone(), now_ms());
        st.latest.insert(session_id.to_string(), snapshot.clone());
        (conversation, title, sink, delivery)
    };
    match delivery {
        Delivery::ReturnFromWait => {}
        Delivery::AppendToNextToolResult => {
            crate::build_queue::inbox().push(&conversation, stop_note(&snapshot, hit));
        }
        Delivery::QueueAndNotify => {
            // The wait that never came will not consume it.
            state().latest.remove(session_id);
            crate::build_queue::inbox().push(&conversation, stop_note(&snapshot, hit));
            sink.emit(
                EVT_STOP,
                json!({
                    "conversation_id": conversation, "title": title, "session_id": session_id, "hit": hit,
                    "card_title": card_title(&snapshot), "reason": snapshot.reason, "text": format_snapshot(&snapshot),
                }),
            );
        }
    }
}

/// Whether a stop that arrived while the conversation was idle may start a run by itself: the setting (global,
/// or this chat's override) says yes, nothing is running, and the hourly limit has room. Asking spends one of the
/// hour's automatic runs when the answer is yes.
#[arbor_rpc::handler]
fn assistant_stop_auto_ok(_ctx: &BennuState, conversation: String, global: bool, chat_override: Option<bool>) -> Result<Value, String> {
    let idle = !super::worktree::is_running(&conversation);
    if !auto_continue_allowed(global, chat_override, idle) {
        return Ok(json!(false));
    }
    let ok = state().limiter.try_acquire(&conversation, now_ms(), MAX_AUTO_CONTINUES, AUTO_WINDOW_MS);
    Ok(json!(ok))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wait_in_flight_is_seen_and_ends_with_its_guard() {
        let conv = "c-wait-test";
        {
            let _g = waiting(conv);
            assert_eq!(owner_state(&state(), conv), OwnerState::WaitingForStop);
            let _g2 = waiting(conv);
        }
        assert_ne!(owner_state(&state(), conv), OwnerState::WaitingForStop);
    }

    #[test]
    fn a_session_nobody_watches_is_not_snapshotted() {
        assert!(!is_watched("no-such-session"));
        assert_eq!(take_values("no-such-session"), "");
    }
}
