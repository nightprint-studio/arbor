//! One run's shared state: where it reports to, what it may touch, the message it is building, and the
//! way it asks the user.
//!
//! The loop runs on its own thread and the tool host runs inside it, but both — and the handlers that
//! answer a prompt or cancel — need the same few things. They live here, behind one `Arc`, so there is
//! one place that knows how a question reaches the screen and how its answer gets back.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use bennu_assistant::prelude::{grant_reason, AccessMode, Diagnostics, Policy, RunGrants, Usage};
use bennu_core::prelude::BennuState;
use serde_json::{json, Value};

use super::live_log::LiveRunLog;
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
    /// How much the run decides for itself: proposals only, edits applied inside the scope, or that plus
    /// opening the files the build names.
    pub mode: AccessMode,
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
    /// What the run did with live processes and which files it changed, for the closing summary card.
    pub live_log: Mutex<LiveRunLog>,
    /// The conversation's title, so a refusal elsewhere can say whose run holds a file.
    pub title: String,
    /// The files that builds and checks of this run reported an error in: the only ones Autonomous may open.
    pub diagnostics: Mutex<Diagnostics>,
    /// The files this run opened for itself, for the cap and for revoking.
    pub run_grants: Mutex<RunGrants>,
    /// The user already said yes, in this run, to the project's own build or tests (accept-edits asks once).
    own_commands_ok: AtomicBool,
    /// Whether this conversation's builds queue with the others or compile in a target of their own.
    pub build_isolation: crate::build_queue::Isolation,
    /// How many questions this run may put to the user (the "decide yourself" level caps them), and how many it has.
    pub ask_cap: Option<u32>,
    /// Minutes a debug session may sit unused (0: the default); from the user's setting.
    pub debug_idle_minutes: u32,
    /// For a sub-agent: the parent's session, whose thread the agent's questions go to (the user answers there),
    /// and the agent's title, which labels them. None for a conversation's own run.
    pub prompt_via: Option<(Arc<Session>, String)>,
    asked: std::sync::atomic::AtomicU32,
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
        mode: AccessMode,
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
            mode,
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
            live_log: Mutex::new(LiveRunLog::default()),
            title: String::new(),
            diagnostics: Mutex::new(Diagnostics::default()),
            run_grants: Mutex::new(RunGrants::default()),
            build_isolation: Default::default(),
            own_commands_ok: AtomicBool::new(false),
            ask_cap: None,
            debug_idle_minutes: 0,
            prompt_via: None,
            asked: std::sync::atomic::AtomicU32::new(0),
        }
    }

    /// The conversation the work belongs to: this run's own, or for a sub-agent its parent's. File reservations,
    /// the write journal and the build queue are kept per conversation, so an agent and the conversation that
    /// started it never lock each other out. (The agent's `conversation_id` is its own, so its read memory and
    /// digests stay apart from the parent's.)
    pub(super) fn owner_conversation(&self) -> &str {
        self.prompt_via.as_ref().map_or(&self.conversation_id, |(parent, _)| &parent.conversation_id)
    }

    /// Edits are written as they are made, rather than held for the user's review.
    pub(super) fn applies_edits(&self) -> bool {
        self.mode.applies_edits()
    }

    /// Open one file for the rest of the run, and leave a line in the thread saying so and why. `label` is
    /// how the thread names the file.
    pub(super) fn open_file(&self, file: &Path, label: &str) {
        self.policy().grant_file(file);
        let id = lock(&self.run_grants).add(file);
        self.view().push(json!({
            "type": "grant",
            "grant": { "id": format!("{}-{id}", self.run_id), "file": label, "reason": grant_reason(file), "state": "active" },
        }));
        self.push_message();
    }

    /// Take back a file opened with [`Self::open_file`] (`grant_id` is the id the thread carries). Edits
    /// already made stay; what ends is the right to touch the file again. False when there is no such grant.
    pub(super) fn revoke_grant(&self, grant_id: &str) -> bool {
        let Some(id) = grant_id.strip_prefix(&format!("{}-", self.run_id)) else { return false };
        let Some(file) = lock(&self.run_grants).revoke(id) else { return false };
        self.policy().revoke_file(&file);
        self.set_grant_state(grant_id, "revoked");
        true
    }

    /// The run is over, and so is every grant it still held: the thread says so rather than leaving them "active".
    pub(super) fn expire_grants(&self) {
        let ids = lock(&self.run_grants).active_ids();
        for id in ids {
            self.set_grant_state(&format!("{}-{id}", self.run_id), "expired");
        }
    }

    fn set_grant_state(&self, grant_id: &str, state: &str) {
        self.view().update("/grant/id", grant_id, |b| b["grant"]["state"] = Value::String(state.to_string()));
        self.push_message();
    }

    /// Count one question against the cap. False when the run has used them all.
    pub(super) fn may_ask_question(&self) -> bool {
        let n = self.asked.fetch_add(1, Ordering::SeqCst);
        self.ask_cap.map_or(true, |cap| n < cap)
    }

    pub(super) fn own_commands_approved(&self) -> bool {
        self.own_commands_ok.load(Ordering::SeqCst)
    }

    pub(super) fn approve_own_commands(&self) {
        self.own_commands_ok.store(true, Ordering::SeqCst);
    }

    pub(super) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub(super) fn view(&self) -> MutexGuard<'_, View> {
        lock(&self.view)
    }

    pub(super) fn live_log(&self) -> MutexGuard<'_, LiveRunLog> {
        lock(&self.live_log)
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
        block: Value,
        id_pointer: &str,
        id: &str,
        answered: impl FnOnce(&mut Value, &Value),
    ) -> Option<Value> {
        self.ask_within(block, id_pointer, id, None, answered)
    }

    /// [`Self::ask`] that also gives up (`None`) after `limit`: for a question only the panel's code can
    /// answer, such as whether a diagram parses. The caller tells a timeout from a cancel with `cancelled()`.
    pub(super) fn ask_within(
        &self,
        mut block: Value,
        id_pointer: &str,
        id: &str,
        limit: Option<std::time::Duration>,
        answered: impl FnOnce(&mut Value, &Value),
    ) -> Option<Value> {
        let prompt_id = format!("p{}-{}", PROMPT_SEQ.fetch_add(1, Ordering::SeqCst), self.run_id);
        block["promptId"] = Value::String(prompt_id.clone());
        // A sub-agent's question is shown in the parent's thread, labelled with the agent, under an id of its own
        // (the parent may have asked the same thing before).
        let mut id = id.to_string();
        if let Some((_, title)) = &self.prompt_via {
            bennu_assistant::prelude::label_prompt(&mut block, title);
            id = format!("{id}~{}", self.run_id);
            if let Some(v) = block.pointer_mut(id_pointer) {
                *v = Value::String(id.clone());
            }
        }
        let thread: &Session = self.prompt_via.as_ref().map_or(self, |(parent, _)| parent.as_ref());
        let rx = runs::open_prompt(&prompt_id);
        thread.view().push(block);
        thread.push_message();

        let answer = runs::wait_within(&rx, &self.cancel, limit);
        runs::close_prompt(&prompt_id);

        if let Some(a) = &answer {
            thread.view().update(id_pointer, &id, |b| answered(b, a));
            thread.push_message();
        }
        answer
    }
}
