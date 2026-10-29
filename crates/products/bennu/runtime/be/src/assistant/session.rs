//! One run's shared state: where it reports to, what it may touch, the message it is building, and the
//! way it asks the user.
//!
//! The loop runs on its own thread and the tool host runs inside it, but both — and the handlers that
//! answer a prompt or cancel — need the same few things. They live here, behind one `Arc`, so there is
//! one place that knows how a question reaches the screen and how its answer gets back.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use bennu_assistant::prelude::{Policy, Usage};
use bennu_core::prelude::BennuState;
use serde_json::{json, Value};

use super::pseudonyms::{restore_value, Authors};
use super::runs;
use super::view::View;

pub(super) const TOPIC: &str = "arbor://bennu/assistant";

/// How often streamed text is pushed to the screen. Every delta would be a message per few characters;
/// this is smooth to read and cheap to send.
const TEXT_PUSH_EVERY: Duration = Duration::from_millis(60);

static PROMPT_SEQ: AtomicU64 = AtomicU64::new(1);

pub(super) struct Session {
    pub run_id: String,
    pub conversation_id: String,
    pub root: String,
    /// Edits are applied as they are proposed, rather than waiting for the user.
    pub auto: bool,
    pub sink: Arc<dyn EventSink>,
    /// The context the handlers are called with. Built from the same sink and reverse channel as the
    /// serve loop's: the state is transport-only, so a second one is the same thing.
    pub ctx: BennuState,
    pub cancel: Arc<AtomicBool>,
    pub policy: Mutex<Policy>,
    pub view: Mutex<View>,
    pub usage: Mutex<Usage>,
    /// What each compaction request used — requests of their own, outside the loop's, priced one by one.
    pub extra_requests: Mutex<Vec<Usage>>,
    last_push: Mutex<Instant>,
    /// The proposal the edits of this run are being gathered into, once there is one.
    pub proposal_id: Mutex<Option<String>>,
    /// The made-up ids standing for people in git results, and whether they are in use this run.
    pub authors: Mutex<Authors>,
    pub hide_authors: bool,
    /// The projects this conversation works across, the one the user is in first. One when the reach is narrower.
    pub projects: Vec<super::projects::Project>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        run_id: String,
        conversation_id: String,
        root: String,
        auto: bool,
        sink: Arc<dyn EventSink>,
        ctx: BennuState,
        cancel: Arc<AtomicBool>,
        policy: Policy,
        view: View,
    ) -> Self {
        Self {
            run_id,
            conversation_id,
            root,
            auto,
            sink,
            ctx,
            cancel,
            policy: Mutex::new(policy),
            view: Mutex::new(view),
            usage: Mutex::new(Usage::default()),
            extra_requests: Mutex::new(Vec::new()),
            last_push: Mutex::new(Instant::now() - TEXT_PUSH_EVERY),
            proposal_id: Mutex::new(None),
            authors: Mutex::new(Authors::load()),
            hide_authors: false,
            projects: Vec::new(),
        }
    }

    pub(super) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub(super) fn view(&self) -> MutexGuard<'_, View> {
        lock(&self.view)
    }

    pub(super) fn policy(&self) -> MutexGuard<'_, Policy> {
        lock(&self.policy)
    }

    pub(super) fn add_usage(&self, u: Usage) -> Usage {
        let mut total = lock(&self.usage);
        *total = total.merge(u);
        *total
    }

    /// Send an event about this run to the panel.
    pub(super) fn emit(&self, kind: &str, extra: Value) {
        let mut payload = json!({ "run_id": self.run_id, "conversation_id": self.conversation_id, "kind": kind });
        if let (Some(obj), Some(more)) = (payload.as_object_mut(), extra.as_object()) {
            for (k, v) in more {
                obj.insert(k.clone(), v.clone());
            }
        }
        self.sink.emit(TOPIC, payload);
    }

    /// The message as the panel shows it: with the made-up ids turned back into the names they stand for.
    /// What the model said stays in the transcript with the ids; only what the user reads is restored.
    pub(super) fn display_snapshot(&self) -> Value {
        let mut message = self.view().snapshot();
        if self.hide_authors {
            restore_value(&lock(&self.authors), &mut message);
        }
        message
    }

    /// Push the message as it stands now.
    pub(super) fn push_message(&self) {
        let message = self.display_snapshot();
        self.emit("message", json!({ "message": message }));
        *lock(&self.last_push) = Instant::now();
    }

    /// Push the message if it has been a moment since the last push — for streamed text.
    pub(super) fn push_message_soon(&self) {
        if lock(&self.last_push).elapsed() >= TEXT_PUSH_EVERY {
            self.push_message();
        }
    }

    /// Put a question to the user and wait for the answer.
    ///
    /// `block` is added to the message with a fresh `promptId`; the panel renders it and replies with
    /// that id. `None` comes back when the run is cancelled while waiting. `answered` turns the
    /// answer into the change that marks the block as decided, so it stays in the thread as a record.
    pub(super) fn ask(
        &self,
        mut block: Value,
        id_pointer: &str,
        id: &str,
        answered: impl FnOnce(&mut Value, &Value),
    ) -> Option<Value> {
        let prompt_id = format!("p{}-{}", PROMPT_SEQ.fetch_add(1, Ordering::SeqCst), self.run_id);
        block["promptId"] = Value::String(prompt_id.clone());
        let rx = runs::open_prompt(&prompt_id);
        self.view().push(block);
        self.push_message();

        let answer = runs::wait(&rx, &self.cancel);
        runs::close_prompt(&prompt_id);

        if let Some(a) = &answer {
            self.view().update(id_pointer, id, |b| answered(b, a));
            self.push_message();
        }
        answer
    }
}
