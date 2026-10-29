//! Debug sessions that outlive the run that opened them.
//!
//! A session belongs to the **conversation**: when a run ends it is *parked* here, still attached and — if the
//! program was stopped at a breakpoint — still suspended, so the user can inspect it in the Debug panel and the
//! next run of the same conversation picks it up again (`DebugRun::adopt`). What ends it: the agent's
//! `debug_detach`, the program exiting, the idle [`Lease`] running out (counted from the last debug tool of
//! the conversation), the conversation being deleted, its project closing, or Arbor exiting.
//!
//! A reaper thread (started with the first parked session) looks every few seconds; releasing is the same
//! `release_session` the run used to do at its end, so a launched program is stopped with its tree and an
//! attached JVM is detached and resumed.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arbor_ipc::prelude::EventSink;
use bennu_assistant::prelude::Note;
use bennu_core::prelude::BennuState;
use serde_json::{json, Value};

use super::debug::{release_session, remove_assistant_breakpoints, Hold};
use crate::debug_backend;

/// `arbor://bennu/assistant-debug`: a conversation's kept session started, was renewed, or ended.
pub(crate) const EVT_KEPT: &str = "arbor://bennu/assistant-debug";
/// How often the reaper looks.
const REAP_EVERY: Duration = Duration::from_secs(15);

pub(super) fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// One parked session.
pub(super) struct Kept {
    pub hold: Hold,
    pub root: String,
    pub sink: Arc<dyn EventSink>,
    pub title: String,
}

static KEPT: LazyLock<Mutex<HashMap<String, Kept>>> = LazyLock::new(Default::default);
static REAPER: std::sync::Once = std::sync::Once::new();

fn kept() -> std::sync::MutexGuard<'static, HashMap<String, Kept>> {
    KEPT.lock().unwrap_or_else(|p| p.into_inner())
}

fn describe(conversation: &str, k: &Kept, state: &str, reason: &str) -> Value {
    json!({
        "conversation": conversation, "title": k.title, "root": k.root, "label": k.hold.label, "sessionId": k.hold.session_id,
        "launched": k.hold.launched, "state": state, "reason": reason,
        "endsInMin": k.hold.lease.remaining_minutes(now_ms()),
    })
}

/// Park a session whose run ended. Replaces one already parked for the conversation (it cannot happen while
/// one-per-conversation holds, but a stale entry must not leak).
pub(super) fn park(conversation: &str, mut kept_session: Kept) {
    // A stop while the run was going is a use of the session as much as a tool call is.
    if let Some(t) = crate::assistant::stop_watch::last_stop_ms(conversation) {
        kept_session.hold.lease.touch(t);
    }
    kept_session.sink.emit(EVT_KEPT, describe(conversation, &kept_session, "kept", ""));
    if let Some(old) = kept().insert(conversation.to_string(), kept_session) {
        release_session(&old.hold, &old.sink);
    }
    start_reaper();
}

/// The next run of the conversation takes its session back.
pub(super) fn take(conversation: &str) -> Option<Kept> {
    kept().remove(conversation)
}

/// End a conversation's kept session now. Returns what was done.
pub(super) fn release(conversation: &str, reason: &str) -> Option<Vec<String>> {
    let k = kept().remove(conversation)?;
    Some(finish(conversation, &k, reason))
}

fn finish(conversation: &str, k: &Kept, reason: &str) -> Vec<String> {
    let mut done = release_session(&k.hold, &k.sink);
    done.extend(remove_assistant_breakpoints(&k.root, &k.sink));
    k.sink.emit(EVT_KEPT, describe(conversation, k, "ended", reason));
    // The agent finds out on its next message, not by guessing from a failing tool.
    if !reason.is_empty() {
        let note = format!("The debug session you left open (`{}`) was ended: {reason}. Its breakpoints were removed.", k.hold.label);
        let hash = note.bytes().fold(now_ms(), |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
        crate::build_queue::inbox().push(conversation, Note { text: note, hash });
    }
    done
}

/// Every kept session of a project (it closed), or all of them (Arbor exits) when `root` is `None`.
pub(crate) fn release_matching(root: Option<&str>, reason: &str) {
    let ids: Vec<String> = kept().iter().filter(|(_, k)| root.map_or(true, |r| same_root(&k.root, r))).map(|(id, _)| id.clone()).collect();
    for id in ids {
        release(&id, reason);
    }
}

fn same_root(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_lowercase();
    norm(a) == norm(b)
}

/// Release the sessions that are over: the program exited, or nobody used them for the idle time.
fn reap(now: u64) {
    let mut parked = kept();
    let due: Vec<(String, &'static str)> = parked
        .iter_mut()
        .filter_map(|(id, k)| {
            // A stop renews the lease: a program that keeps hitting its breakpoint is in use.
            if let Some(t) = crate::assistant::stop_watch::last_stop_ms(id) {
                k.hold.lease.touch(t);
            }
            if debug_backend::get(&k.hold.session_id).is_err() {
                Some((id.clone(), "the program had already ended"))
            } else if k.hold.lease.expired(now) {
                Some((id.clone(), "no debug tool was used for too long"))
            } else {
                None
            }
        })
        .collect();
    // `release` takes the same lock.
    drop(parked);
    for (id, why) in due {
        release(&id, why);
    }
}

fn start_reaper() {
    REAPER.call_once(|| {
        std::thread::Builder::new()
            .name("assistant-debug-reaper".into())
            .spawn(|| loop {
                std::thread::sleep(REAP_EVERY);
                reap(now_ms());
            })
            .ok();
    });
}

/// The kept sessions, for the panel opening after they were parked.
#[arbor_rpc::handler]
fn assistant_debug_kept(_ctx: &BennuState) -> Result<Value, String> {
    Ok(Value::Array(kept().iter().map(|(id, k)| describe(id, k, "kept", "")).collect()))
}

/// The user's Stop on a kept session.
#[arbor_rpc::handler]
fn assistant_debug_stop(_ctx: &BennuState, conversation: String) -> Result<String, String> {
    match release(&conversation, "") {
        Some(done) => Ok(done.join(", ")),
        None => Err("that debug session is already over".into()),
    }
}
