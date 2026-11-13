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
    /// The text the edit was made against, so it can be made again on the file as it is later.
    pub base: String,
    /// How the card names the file (`src/A.java`, `A.java → B.java`): the key the screen knows it by.
    pub label: String,
}

#[derive(Default)]
struct Registry {
    cancel: HashMap<String, Arc<AtomicBool>>,
    prompts: HashMap<String, Sender<Value>>,
    grants: HashMap<String, Vec<Grant>>,
    /// Projects Imber brought into a conversation during a run, by root: its later runs work in them too.
    opened: HashMap<String, Vec<String>>,
    proposals: HashMap<String, Vec<PendingEdit>>,
    revokers: HashMap<String, Revoker>,
    /// Set by "retry now" while a run waits to send a failed request again.
    skip_wait: HashMap<String, Arc<AtomicBool>>,
    /// What the user wrote while the run worked, by run, not yet read: delivered before its next request. The
    /// entry exists from the run's start until [`close_notes`], so a note can never land in a run that has
    /// already read its last one.
    notes: HashMap<String, Vec<UserNote>>,
}

/// A message the user wrote while a run was working.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct UserNote {
    /// The panel's id for it, so the panel can tell when it has been read.
    pub id: String,
    pub text: String,
}

/// Takes back one of the files a run opened for itself; says whether there was such a grant.
pub(super) type Revoker = Arc<dyn Fn(&str) -> bool + Send + Sync>;

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

fn reg() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|p| p.into_inner())
}

/// Start tracking a run; the returned flag is what cancelling sets.
pub(super) fn begin(run_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut r = reg();
    r.cancel.insert(run_id.to_string(), Arc::clone(&flag));
    r.skip_wait.insert(run_id.to_string(), Arc::new(AtomicBool::new(false)));
    r.notes.insert(run_id.to_string(), Vec::new());
    flag
}

/// Leave a message for a run that is working. False when it is not (any more) taking them: the panel then sends
/// it as an ordinary message.
pub(super) fn queue_note(run_id: &str, note: UserNote) -> bool {
    match reg().notes.get_mut(run_id) {
        Some(list) => {
            list.push(note);
            true
        }
        None => false,
    }
}

/// The messages waiting for a run, taken: it reads them now.
pub(super) fn take_notes(run_id: &str) -> Vec<UserNote> {
    reg().notes.get_mut(run_id).map(std::mem::take).unwrap_or_default()
}

/// The run reads no more messages: what is still waiting comes back, for the panel to put back in the box.
pub(super) fn close_notes(run_id: &str) -> Vec<UserNote> {
    reg().notes.remove(run_id).unwrap_or_default()
}

/// The user asked a run that is waiting to retry to try now. False when the run is not going.
pub(super) fn retry_now(run_id: &str) -> bool {
    match reg().skip_wait.get(run_id) {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            true
        }
        None => false,
    }
}

/// Whether "retry now" was asked since the last time; asking clears it.
pub(super) fn take_retry_now(run_id: &str) -> bool {
    reg().skip_wait.get(run_id).is_some_and(|flag| flag.swap(false, Ordering::SeqCst))
}

pub(super) fn finish(run_id: &str) {
    let mut r = reg();
    r.cancel.remove(run_id);
    r.revokers.remove(run_id);
    r.skip_wait.remove(run_id);
    r.notes.remove(run_id);
}

/// Let the panel take back a file the run opened for itself, while the run goes on.
pub(super) fn set_revoker(run_id: &str, revoker: Revoker) {
    reg().revokers.insert(run_id.to_string(), revoker);
}

/// Revoke a grant of a running run. False when the run is over (its grants ended with it) or has no such grant.
pub(super) fn revoke(run_id: &str, grant_id: &str) -> bool {
    // Called outside the registry lock: revoking pushes a message to the panel.
    let revoker = reg().revokers.get(run_id).cloned();
    revoker.is_some_and(|r| r(grant_id))
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
    wait_within(rx, cancelled, None)
}

/// [`wait`], also giving up once `limit` has passed — for an answer that only the panel can give and that
/// a closed or busy panel might never send.
pub(super) fn wait_within(rx: &Receiver<Value>, cancelled: &AtomicBool, limit: Option<Duration>) -> Option<Value> {
    let deadline = limit.map(|l| std::time::Instant::now() + l);
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(v) => return Some(v),
            Err(RecvTimeoutError::Timeout) => {
                if cancelled.load(Ordering::SeqCst) || deadline.is_some_and(|d| std::time::Instant::now() >= d) {
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

/// The projects Imber brought into this conversation in earlier runs (while Arbor stays open, like its grants).
pub(super) fn opened_projects(conversation: &str) -> Vec<String> {
    reg().opened.get(conversation).cloned().unwrap_or_default()
}

pub(super) fn remember_opened_project(conversation: &str, root: &str) {
    let mut r = reg();
    let list = r.opened.entry(conversation.to_string()).or_default();
    if !list.iter().any(|x| x == root) {
        list.push(root.to_string());
    }
}

pub(super) fn store_proposal(proposal_id: &str, edits: Vec<PendingEdit>) {
    reg().proposals.insert(proposal_id.to_string(), edits);
}

/// Take a proposal out for deciding. Taken, not peeked: applying twice would write twice.
pub(super) fn take_proposal(proposal_id: &str) -> Option<Vec<PendingEdit>> {
    reg().proposals.remove(proposal_id)
}

/// Whether a proposal other than `except` is still waiting on `file`. Whichever of the two is applied
/// first makes the other outdated, and the one proposing second should know.
pub(super) fn pending_elsewhere(file: &str, except: &str) -> bool {
    reg().proposals.iter().any(|(id, edits)| id != except && edits.iter().any(|e| e.file == file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_reaches_a_working_run_once_and_none_after_it_stopped_reading() {
        let run = "r-notes-test";
        begin(run);
        let note = |id: &str| UserNote { id: id.into(), text: "use the other module".into() };
        assert!(queue_note(run, note("n1")));
        assert_eq!(take_notes(run), vec![note("n1")]);
        assert!(take_notes(run).is_empty(), "read once");
        assert!(queue_note(run, note("n2")));
        assert_eq!(close_notes(run), vec![note("n2")], "what was not read comes back");
        assert!(!queue_note(run, note("n3")), "a run that stopped reading takes no more");
        finish(run);
    }

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
        store_proposal("prop", vec![PendingEdit { root: "r".into(), file: "f".into(), new_text: "t".into(), stamp: None, move_to: None, base: String::new(), label: "f".into() }]);
        assert_eq!(take_proposal("prop").map(|e| e.len()), Some(1));
        assert!(take_proposal("prop").is_none());
    }

    #[test]
    fn a_running_run_can_be_asked_to_revoke_and_a_finished_one_cannot() {
        set_revoker("run-rv", Arc::new(|id: &str| id == "g1"));
        assert!(revoke("run-rv", "g1"));
        assert!(!revoke("run-rv", "g2"));
        finish("run-rv");
        assert!(!revoke("run-rv", "g1"), "its grants ended with it");
    }

    #[test]
    fn grants_are_remembered_per_conversation() {
        remember_grants("conv-x", vec![Grant { root: "/a".into(), write: true }]);
        assert_eq!(grants("conv-x").len(), 1);
        assert!(grants("conv-y").is_empty());
    }
}
