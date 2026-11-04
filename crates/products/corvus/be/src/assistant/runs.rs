//! What is going on right now: runs that can be cancelled, approvals waiting for an answer, and the
//! conversations' transcripts.
//!
//! All process memory on purpose. A run does not survive a restart, an approval is meaningless without
//! the run parked on it, and a transcript is held so the next message continues the conversation.
//! (Persisting conversations to disk is not done yet — see the Imber docs.)

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::Duration;

use imber_core::prelude::Message;
use serde_json::Value;

#[derive(Default)]
struct Registry {
    cancel: HashMap<String, Arc<AtomicBool>>,
    prompts: HashMap<String, Sender<Value>>,
    transcripts: HashMap<String, Vec<Message>>,
    /// What each conversation has cost so far, in dollars.
    cost_usd: HashMap<String, f64>,
    /// Conversations with a run going.
    busy: std::collections::HashSet<String>,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

fn reg() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|p| p.into_inner())
}

/// Start tracking a run; the returned flag is what cancelling sets.
pub fn begin(run_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    reg().cancel.insert(run_id.to_string(), Arc::clone(&flag));
    flag
}

pub fn finish(run_id: &str) {
    reg().cancel.remove(run_id);
}

/// Returns whether such a run was going.
pub fn cancel(run_id: &str) -> bool {
    match reg().cancel.get(run_id) {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            true
        }
        None => false,
    }
}

/// Register a question and get the channel its answer arrives on.
pub fn open_prompt(prompt_id: &str) -> Receiver<Value> {
    let (tx, rx) = mpsc::channel();
    reg().prompts.insert(prompt_id.to_string(), tx);
    rx
}

pub fn close_prompt(prompt_id: &str) {
    reg().prompts.remove(prompt_id);
}

/// Deliver an answer. False when nothing is waiting for it (the run ended or was cancelled).
pub fn reply(prompt_id: &str, answer: Value) -> bool {
    let tx = reg().prompts.remove(prompt_id);
    tx.is_some_and(|tx| tx.send(answer).is_ok())
}

/// Wait for the answer, giving up when the run is cancelled. Polls rather than blocking forever: the
/// cancel flag is not a channel, and a user who closes the panel must not leave a thread parked.
pub fn wait(rx: &Receiver<Value>, cancelled: &AtomicBool) -> Option<Value> {
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

pub fn transcript(conversation: &str) -> Vec<Message> {
    reg().transcripts.get(conversation).cloned().unwrap_or_default()
}

pub fn save_transcript(conversation: &str, messages: Vec<Message>) {
    reg().transcripts.insert(conversation.to_string(), messages);
}

/// Forget a conversation (the user started a new one or closed it).
pub fn forget(conversation: &str) {
    let mut r = reg();
    r.transcripts.remove(conversation);
    r.cost_usd.remove(conversation);
}


/// Mark a conversation as having a run going. False when it already has one.
pub fn claim_conversation(conversation: &str) -> bool {
    reg().busy.insert(conversation.to_string())
}

pub fn release_conversation(conversation: &str) {
    reg().busy.remove(conversation);
}

pub fn conversation_busy(conversation: &str) -> bool {
    reg().busy.contains(conversation)
}
pub fn cost_usd(conversation: &str) -> f64 {
    reg().cost_usd.get(conversation).copied().unwrap_or(0.0)
}

pub fn add_cost(conversation: &str, usd: f64) -> f64 {
    let mut r = reg();
    let total = r.cost_usd.entry(conversation.to_string()).or_insert(0.0);
    *total += usd;
    *total
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cancelling_sets_the_flag_of_that_run_only() {
        let a = begin("t-run-a");
        let b = begin("t-run-b");
        assert!(cancel("t-run-a"));
        assert!(a.load(Ordering::SeqCst) && !b.load(Ordering::SeqCst));
        finish("t-run-a");
        finish("t-run-b");
        assert!(!cancel("t-run-a"), "a finished run is not found");
    }

    #[test]
    fn an_answer_reaches_the_run_waiting_for_it_once() {
        let flag = AtomicBool::new(false);
        let rx = open_prompt("t-p1");
        assert!(reply("t-p1", json!({"answer": "approve"})));
        assert_eq!(wait(&rx, &flag), Some(json!({"answer": "approve"})));
        assert!(!reply("t-p1", json!(1)), "a second answer has nowhere to go");
    }

    #[test]
    fn a_cancelled_run_stops_waiting() {
        let flag = AtomicBool::new(true);
        let rx = open_prompt("t-p2");
        assert_eq!(wait(&rx, &flag), None);
        close_prompt("t-p2");
    }

    #[test]
    fn a_conversation_keeps_its_transcript_and_its_cost_until_forgotten() {
        save_transcript("t-conv", vec![Message::user("hi")]);
        assert_eq!(transcript("t-conv").len(), 1);
        assert_eq!(add_cost("t-conv", 0.25), 0.25);
        assert_eq!(add_cost("t-conv", 0.25), 0.5);
        forget("t-conv");
        assert!(transcript("t-conv").is_empty());
        assert_eq!(cost_usd("t-conv"), 0.0);
    }
}
