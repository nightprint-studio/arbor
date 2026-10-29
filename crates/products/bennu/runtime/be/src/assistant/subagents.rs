//! Sub-agents, the host side: the registry of children, the handlers the panel calls to list, read and stop
//! them, and what ends with a run or a conversation.
//!
//! The rules (what a child may do, what it costs, how a spawn is checked) are `bennu_assistant::children` and
//! `subagent`; the tools and the child's loop are `tools::spawn`. This is the process-wide state between them:
//! the children's records, their live sessions (for the transcript and for stopping), and the figures the
//! other conversations' agents can read about a conversation (its current plan step).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use bennu_assistant::prelude::{child_card_block, ChildInfo, ChildRegistry};
use bennu_core::prelude::BennuState;
use serde_json::{json, Value};

use super::runs;
use super::session::Session;

/// Where a sub-agent's own session reports to: nowhere. Its thread is not a message of the conversation; what the
/// user sees of it is the card in the parent's thread and the transcript the panel asks for.
pub(super) struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _topic: &str, _payload: Value) {}
}

static CHILDREN: LazyLock<ChildRegistry> = LazyLock::new(ChildRegistry::default);
/// The sessions of children that are running: their thread so far, and the way to stop them.
static LIVE: LazyLock<Mutex<HashMap<String, Arc<Session>>>> = LazyLock::new(Default::default);
/// The thread a child ended with, for the transcript view.
static ENDED: LazyLock<Mutex<HashMap<String, Value>>> = LazyLock::new(Default::default);
/// What each conversation is working on right now (its plan's current step), for `file_owner`.
static PLAN_STEPS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub(super) fn registry() -> &'static ChildRegistry {
    &CHILDREN
}

pub(super) fn register_live(id: &str, session: Arc<Session>) {
    lock(&LIVE).insert(id.to_string(), session);
}

/// The child is over: keep the thread it ended with, drop the live session.
pub(super) fn end_live(id: &str) {
    if let Some(session) = lock(&LIVE).remove(id) {
        lock(&ENDED).insert(id.to_string(), session.display_snapshot());
    }
}

/// Tell the panel about a child (cost, status) and refresh its card in the parent's thread.
pub(super) fn announce(parent: &Session, info: &ChildInfo) {
    parent.emit("agent", json!({ "agent": info }));
    parent.view().update("/agent/id", &info.id, |b| *b = child_card_block(info));
    parent.push_message();
}

/// Remember what a conversation's plan is on, or forget it when the plan has nothing open.
pub(super) fn note_plan_step(conversation: &str, steps: &[Value]) {
    let text_of = |status: &str| steps.iter().find(|s| s["status"] == status).and_then(|s| s["text"].as_str()).map(str::to_string);
    let mut map = lock(&PLAN_STEPS);
    match text_of("in_progress").or_else(|| text_of("pending")) {
        Some(step) => map.insert(conversation.to_string(), step),
        None => map.remove(conversation),
    };
}

pub(super) fn plan_step_of(conversation: &str) -> Option<String> {
    lock(&PLAN_STEPS).get(conversation).cloned()
}

/// Ask a child to stop. False when it is not running.
pub(super) fn cancel_child(id: &str) -> bool {
    runs::cancel(id)
}

/// Stop every running child a run started.
pub(super) fn stop_run(run_id: &str) -> usize {
    registry().running_of_run(run_id).iter().filter(|id| cancel_child(id)).count()
}

fn stop_conversation(conversation: &str) -> usize {
    registry().running_of_conversation(conversation).iter().filter(|id| cancel_child(id)).count()
}

/// A run is ending: stop its children and give them a moment to write their last word into the parent's thread,
/// so the message saved with the conversation does not show an agent as running that is not.
pub(super) fn settle_run(run_id: &str) {
    if stop_run(run_id) == 0 {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !registry().running_of_run(run_id).is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The conversation was deleted: its agents go with it.
pub(super) fn forget_conversation(conversation: &str) {
    stop_conversation(conversation);
    let ids: Vec<String> = registry().list(conversation).into_iter().map(|c| c.id).collect();
    {
        let mut ended = lock(&ENDED);
        ids.iter().for_each(|id| {
            ended.remove(id);
        });
    }
    registry().forget_conversation(conversation);
    lock(&PLAN_STEPS).remove(conversation);
}

// ── Handlers ─────────────────────────────────────────────────────────────────────────────────────

/// The sub-agents of a conversation, oldest first: what the Agents list shows.
#[arbor_rpc::handler]
fn assistant_agents(_ctx: &BennuState, conversation_id: String) -> Result<Value, String> {
    serde_json::to_value(registry().list(&conversation_id)).map_err(|e| e.to_string())
}

/// One agent and its thread, read-only: live while it runs, as it ended otherwise.
#[arbor_rpc::handler]
fn assistant_agent_transcript(_ctx: &BennuState, id: String) -> Result<Value, String> {
    let info = registry().get(&id).ok_or("there is no such agent any more")?;
    let live = lock(&LIVE).get(&id).cloned();
    let message = match live {
        Some(session) => Some(session.display_snapshot()),
        None => lock(&ENDED).get(&id).cloned(),
    };
    Ok(json!({ "agent": info, "message": message }))
}

/// Stop one agent. It ends at its next chunk or tool and reports what it had.
#[arbor_rpc::handler]
fn assistant_agent_stop(_ctx: &BennuState, id: String) -> Result<bool, String> {
    Ok(cancel_child(&id))
}

/// Stop every running agent of a conversation; says how many were stopped.
#[arbor_rpc::handler]
fn assistant_agents_stop_all(_ctx: &BennuState, conversation_id: String) -> Result<usize, String> {
    Ok(stop_conversation(&conversation_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_is_known_by_the_step_it_is_on() {
        let steps = vec![json!({"text": "Read the cart", "status": "completed"}), json!({"text": "Rename total()", "status": "in_progress"}), json!({"text": "Add tests", "status": "pending"})];
        note_plan_step("c-plan", &steps);
        assert_eq!(plan_step_of("c-plan").as_deref(), Some("Rename total()"));
        let queued = vec![json!({"text": "Add tests", "status": "pending"})];
        note_plan_step("c-plan", &queued);
        assert_eq!(plan_step_of("c-plan").as_deref(), Some("Add tests"), "with nothing in progress, the next one");
        note_plan_step("c-plan", &[json!({"text": "x", "status": "completed"})]);
        assert_eq!(plan_step_of("c-plan"), None, "a finished plan is no step");
    }

    #[test]
    fn stopping_a_run_stops_only_its_running_children() {
        use bennu_assistant::prelude::ChildStatus;
        let info = |id: &str, run: &str, status| ChildInfo {
            id: id.into(),
            conversation_id: "c-stop".into(),
            run_id: run.into(),
            title: "t".into(),
            task: "x".into(),
            tools: "read".into(),
            model: "m".into(),
            status,
            cost: 0.0,
            started_at: 0,
            ended_at: None,
            summary: None,
            error: None,
            waiting: None,
        };
        let flag = runs::begin("stop-run.a1");
        registry().add(info("stop-run.a1", "stop-run", ChildStatus::Running));
        registry().add(info("stop-run.a2", "stop-run", ChildStatus::Done));
        let other = runs::begin("other-run.a1");
        registry().add(info("other-run.a1", "other-run", ChildStatus::Running));
        assert_eq!(stop_run("stop-run"), 1);
        assert!(flag.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!other.load(std::sync::atomic::Ordering::SeqCst), "another run's agent is left alone");
        forget_conversation("c-stop");
        assert!(registry().list("c-stop").is_empty());
        runs::finish("stop-run.a1");
        runs::finish("other-run.a1");
    }
}
