//! What is going on right now: runs that can be cancelled, questions waiting for an answer, the folders
//! the user has opened up, and the edits proposed and not yet decided.
//!
//! All of it is process memory on purpose. A run does not survive a restart (the conversation does —
//! see `store`), a pending question is meaningless without the run waiting on it, and a grant is
//! "for this session" by definition.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::Duration;

use bennu_assistant::prelude::Grant;
use serde_json::Value;

/// One edit waiting for the user: the file, the text it would have, and the stamp it was read at, so
/// applying it is refused if the file changed in between.
#[derive(Debug, Clone)]
pub(super) struct PendingEdit {
    pub root: String,
    pub file: String,
    pub new_text: String,
    pub stamp: Option<String>,
    /// A rename of a public type also moves its file; this is where it goes, done after the text is
    /// written (the edits are addressed to the old path).
    pub move_to: Option<String>,
}

#[derive(Default)]
struct Registry {
    cancel: HashMap<String, Arc<AtomicBool>>,
    prompts: HashMap<String, Sender<Value>>,
    grants: HashMap<String, Vec<Grant>>,
    proposals: HashMap<String, Vec<PendingEdit>>,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

fn reg() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|p| p.into_inner())
}

/// Start tracking a run; the returned flag is what cancelling sets.
pub(super) fn begin(run_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    reg().cancel.insert(run_id.to_string(), Arc::clone(&flag));
    flag
}

pub(super) fn finish(run_id: &str) {
    reg().cancel.remove(run_id);
}

/// Returns whether such a run was going.
pub(super) fn cancel(run_id: &str) -> bool {
    match reg().cancel.get(run_id) {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            true
        }
        None => false,
    }
}

/// Register a question and get the channel its answer arrives on.
pub(super) fn open_prompt(prompt_id: &str) -> Receiver<Value> {
    let (tx, rx) = mpsc::channel();
    reg().prompts.insert(prompt_id.to_string(), tx);
    rx
}

pub(super) fn close_prompt(prompt_id: &str) {
    reg().prompts.remove(prompt_id);
}

/// Deliver an answer. Returns false when nothing is waiting for it (the run ended, or was cancelled).
pub(super) fn reply(prompt_id: &str, answer: Value) -> bool {
    let tx = reg().prompts.remove(prompt_id);
    match tx {
        Some(tx) => tx.send(answer).is_ok(),
        None => false,
    }
}

/// Wait for the answer, giving up when the run is cancelled. Polls rather than blocking forever: the
/// cancel flag is not a channel, and a user who closes the panel must not leave a thread parked.
pub(super) fn wait(rx: &Receiver<Value>, cancelled: &AtomicBool) -> Option<Value> {
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(v) => return Some(v),
            Err(RecvTimeoutError::Timeout) => {
                if cancelled.load(Ordering::SeqCst) {
                    return None;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

pub(super) fn grants(conversation: &str) -> Vec<Grant> {
    reg().grants.get(conversation).cloned().unwrap_or_default()
}

pub(super) fn remember_grants(conversation: &str, grants: Vec<Grant>) {
    reg().grants.insert(conversation.to_string(), grants);
}

pub(super) fn store_proposal(proposal_id: &str, edits: Vec<PendingEdit>) {
    reg().proposals.insert(proposal_id.to_string(), edits);
}

/// Take a proposal out for deciding. Taken, not peeked: applying twice would write twice.
pub(super) fn take_proposal(proposal_id: &str) -> Option<Vec<PendingEdit>> {
    reg().proposals.remove(proposal_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_sets_the_flag_of_that_run_only() {
        let a = begin("run-a");
        let b = begin("run-b");
        assert!(cancel("run-a"));
        assert!(a.load(Ordering::SeqCst) && !b.load(Ordering::SeqCst));
        finish("run-a");
        finish("run-b");
        assert!(!cancel("run-a"), "a finished run is not found");
    }

    #[test]
    fn an_answer_reaches_the_run_waiting_for_it_once() {
        let flag = AtomicBool::new(false);
        let rx = open_prompt("p1");
        assert!(reply("p1", serde_json::json!({"answer": "yes"})));
        assert_eq!(wait(&rx, &flag), Some(serde_json::json!({"answer": "yes"})));
        assert!(!reply("p1", serde_json::json!(1)), "a second answer has nowhere to go");
    }

    #[test]
    fn a_cancelled_run_stops_waiting() {
        let flag = AtomicBool::new(true);
        let rx = open_prompt("p2");
        assert_eq!(wait(&rx, &flag), None);
        close_prompt("p2");
    }

    #[test]
    fn a_proposal_is_decided_once() {
        store_proposal("prop", vec![PendingEdit { root: "r".into(), file: "f".into(), new_text: "t".into(), stamp: None, move_to: None }]);
        assert_eq!(take_proposal("prop").map(|e| e.len()), Some(1));
        assert!(take_proposal("prop").is_none());
    }

    #[test]
    fn grants_are_remembered_per_conversation() {
        remember_grants("conv-x", vec![Grant { root: "/a".into(), write: true }]);
        assert_eq!(grants("conv-x").len(), 1);
        assert!(grants("conv-y").is_empty());
    }
}
