//! The output of the programs the assistant launched, kept by **session id** and outliving the session.
//!
//! A `Capture` used to hang off the conversation's open session, so `debug_output` had nothing to read the
//! moment the program exited — exactly when its output is wanted — and two conversations could only ever see
//! their own. Here every launch registers its capture under the run id (the id the Run console and the Debug
//! panel already use for it), and the book keeps the last few after their sessions closed.
//!
//! Reading without an id means "the console of *this* conversation's latest launch", never the newest one in
//! the process: a second agent that starts its own debug run must not make the first one read the second
//! one's output. Reading with an id reads any console in the book — the user can see all of them in the Run
//! panel anyway — and says whose it was.

use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Mutex};

use super::debug_capture::Capture;

/// Consoles kept, ended ones included. Each is bounded by the capture's own caps.
const MAX_CONSOLES: usize = 8;

/// One launched program's output.
#[derive(Clone)]
pub(super) struct Console {
    /// The run id of the launch, which is also its debug session id.
    pub session_id: String,
    pub conversation: String,
    pub label: String,
    pub capture: Arc<Capture>,
}

static BOOK: LazyLock<Mutex<VecDeque<Console>>> = LazyLock::new(Default::default);

fn book() -> std::sync::MutexGuard<'static, VecDeque<Console>> {
    BOOK.lock().unwrap_or_else(|p| p.into_inner())
}

/// Keep the output of a launch. The oldest console goes when the book is full.
pub(super) fn register(console: Console) {
    let mut book = book();
    book.retain(|c| c.session_id != console.session_id);
    book.push_back(console);
    while book.len() > MAX_CONSOLES {
        book.pop_front();
    }
}

/// The console of a session id.
pub(super) fn find(session_id: &str) -> Option<Console> {
    book().iter().find(|c| c.session_id == session_id).cloned()
}

/// The newest console launched by `conversation`.
pub(super) fn latest_of(conversation: &str) -> Option<Console> {
    book().iter().rev().find(|c| c.conversation == conversation).cloned()
}

/// The consoles in the book, newest first, for an error that tells the model what it may ask for.
pub(super) fn list_text() -> String {
    let book = book();
    if book.is_empty() {
        return "no program has been launched under the debugger yet".to_string();
    }
    let rows: Vec<String> = book
        .iter()
        .rev()
        .map(|c| {
            let state = if c.capture.exit().is_some() { "exited" } else { "running" };
            format!("`{}` — {} ({state})", c.session_id, c.label)
        })
        .collect();
    format!("consoles available:\n{}", rows.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console(id: &str, conversation: &str) -> Console {
        Console { session_id: id.into(), conversation: conversation.into(), label: format!("run {id}"), capture: Arc::new(Capture::default()) }
    }

    #[test]
    fn a_conversation_reads_its_own_latest_launch_not_the_newest_one() {
        register(console("t-a1", "conv-a"));
        register(console("t-b1", "conv-b"));
        register(console("t-a2", "conv-a"));
        register(console("t-b2", "conv-b"));
        assert_eq!(latest_of("conv-a").unwrap().session_id, "t-a2");
        assert_eq!(latest_of("conv-b").unwrap().session_id, "t-b2");
        assert!(latest_of("conv-nobody").is_none());
        assert_eq!(find("t-a1").unwrap().conversation, "conv-a", "an older console is still there by id");
    }

    #[test]
    fn the_book_is_bounded_and_registering_twice_replaces() {
        for i in 0..(MAX_CONSOLES + 3) {
            register(console(&format!("t-bound-{i}"), "conv-bound"));
        }
        assert!(find("t-bound-0").is_none());
        assert!(find(&format!("t-bound-{}", MAX_CONSOLES + 2)).is_some());
        let before = book().len();
        register(console("t-bound-again", "conv-bound"));
        register(console("t-bound-again", "conv-bound"));
        assert_eq!(book().len(), before.min(MAX_CONSOLES), "the same id is not kept twice");
    }
}
