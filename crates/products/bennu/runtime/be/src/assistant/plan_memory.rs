//! Keeping the model aware of its own plan.
//!
//! The plan is in the conversation as `update_plan` calls, so the model *can* see it — but after a stop, a
//! "continue" or twenty tool calls, the last call to `update_plan` is far back and easy to stop following: steps
//! stay "in progress" long after they are done, and a new message starts over instead of picking up where the
//! plan was. Two nudges fix that, both as text **added** to what the model reads next (a note before the user's
//! message, a line after a tool result), never an edit of what it read before — so the cached prefix stays.
//!
//! - **On a new message**, an open plan (any step not completed) is restated: how far it got, the step in
//!   progress, and to carry on from it.
//! - **While working**, after [`REMIND_EVERY`] tool calls with no plan update, the next result says which step
//!   is in progress and to tick it when it is done.
//!
//! The plan of a conversation is read back from its transcript when a run begins, so the nudges survive a
//! restart of the app; during the run `update_plan` keeps it current.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use bennu_assistant::prelude::Message;
use serde_json::Value;

/// Tool calls without a plan update before the model is reminded of the step in progress.
pub(super) const REMIND_EVERY: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Step {
    pub text: String,
    /// `pending`, `in_progress` or `completed`.
    pub status: String,
}

#[derive(Debug, Default)]
struct PlanState {
    steps: Vec<Step>,
    /// Tool calls since the plan was last updated (or last recalled).
    calls_since: u32,
    /// This run already asked the model to close the plan before finishing: it is asked once, not in a loop.
    closing_asked: bool,
}

fn plans() -> &'static Mutex<HashMap<String, PlanState>> {
    static PLANS: OnceLock<Mutex<HashMap<String, PlanState>>> = OnceLock::new();
    PLANS.get_or_init(Default::default)
}

/// The steps of an `update_plan` input (or the plan's checked JSON).
pub(super) fn steps_of(value: &Value) -> Vec<Step> {
    let list = value.get("steps").and_then(Value::as_array).or_else(|| value.as_array());
    list.into_iter()
        .flatten()
        .filter_map(|s| {
            let text = s.get("text").and_then(Value::as_str)?.trim().to_string();
            let status = s.get("status").and_then(Value::as_str).unwrap_or("pending").to_string();
            (!text.is_empty()).then_some(Step { text, status })
        })
        .collect()
}

/// The last plan written in `messages`, if any.
pub(super) fn last_plan(messages: &[Message]) -> Option<Vec<Step>> {
    messages
        .iter()
        .flat_map(Message::tool_uses)
        .filter(|(_, name, _)| *name == "update_plan")
        .last()
        .map(|(_, _, input)| steps_of(input))
        .filter(|steps| !steps.is_empty())
}

fn is_open(steps: &[Step]) -> bool {
    steps.iter().any(|s| s.status != "completed")
}

/// "3 of 7 steps done; in progress: step 4, “…”" — or the next pending step when none is in progress.
fn where_it_is(steps: &[Step]) -> String {
    let done = steps.iter().filter(|s| s.status == "completed").count();
    let (word, at) = match steps.iter().position(|s| s.status == "in_progress") {
        Some(i) => ("in progress", Some(i)),
        None => ("next", steps.iter().position(|s| s.status != "completed")),
    };
    let current = at.map(|i| format!("; {word}: step {}, “{}”", i + 1, steps[i].text)).unwrap_or_default();
    format!("{done} of {} steps done{current}", steps.len())
}

/// The note put before a new message when the conversation has an open plan.
pub(super) fn resume_note(steps: &[Step]) -> Option<String> {
    is_open(steps).then(|| {
        format!(
            "[Your plan from earlier in this conversation is still open: {}. Carry on from it unless this message changes \
             the task — then rewrite it. Keep it current with update_plan as steps are done.]\n\n",
            where_it_is(steps)
        )
    })
}

/// The line added after a tool result when the plan has gone unattended for a while.
pub(super) fn reminder(steps: &[Step]) -> Option<String> {
    is_open(steps).then(|| {
        format!(
            "[Plan: {}. If that step is done, mark it completed (and the next one in progress) with update_plan in your \
             next tool calls; if the work has changed, update the plan to match.]",
            where_it_is(steps)
        )
    })
}

/// Where a run begins: the conversation's plan as its transcript last wrote it. Returns the note for the new
/// message, when the plan is still open.
pub(super) fn begin(conversation: &str, transcript: &[Message]) -> Option<String> {
    let steps = last_plan(transcript)?;
    let note = resume_note(&steps);
    if let Ok(mut map) = plans().lock() {
        map.insert(conversation.to_string(), PlanState { steps, ..PlanState::default() });
    }
    note
}

/// `update_plan` ran: this is the plan now.
pub(super) fn set(conversation: &str, steps: Vec<Step>) {
    if let Ok(mut map) = plans().lock() {
        let state = map.entry(conversation.to_string()).or_default();
        state.steps = steps;
        state.calls_since = 0;
    }
}

/// Another tool ran. Returns the reminder when the plan is open and has gone [`REMIND_EVERY`] calls untouched.
pub(super) fn tick(conversation: &str) -> Option<String> {
    let mut map = plans().lock().ok()?;
    let state = map.get_mut(conversation)?;
    state.calls_since += 1;
    if state.calls_since < REMIND_EVERY {
        return None;
    }
    state.calls_since = 0;
    reminder(&state.steps)
}

/// The model ended its turn. When the plan still has open steps, the note that asks it — once per run — to
/// close them or say what is left; the last step ("check", "run the tests") is the one most often left
/// ticking after the work is done.
pub(super) fn closing_note(conversation: &str) -> Option<String> {
    let mut map = plans().lock().ok()?;
    let state = map.get_mut(conversation)?;
    if state.closing_asked || !is_open(&state.steps) {
        return None;
    }
    state.closing_asked = true;
    let open: Vec<String> = state
        .steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.status != "completed")
        .map(|(i, s)| format!("{}. “{}”", i + 1, s.text))
        .collect();
    Some(format!(
        "[Before you finish: your plan still has open steps — {}. If they are done, mark them completed with \
         update_plan and end there, without repeating your answer. If they are not, leave them open and say in \
         one line what is left and why.]",
        open.join(", ")
    ))
}

/// The conversation is gone.
pub(super) fn forget(conversation: &str) {
    if let Ok(mut map) = plans().lock() {
        map.remove(conversation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_assistant::prelude::Block;
    use serde_json::json;

    fn plan_call(id: &str, steps: Value) -> Message {
        Message::assistant(vec![Block::ToolUse { id: id.into(), name: "update_plan".into(), input: json!({ "steps": steps }) }])
    }

    #[test]
    fn the_last_plan_of_a_transcript_is_read_back() {
        let t = vec![
            Message::user("go"),
            plan_call("1", json!([{"text": "Read", "status": "in_progress"}, {"text": "Fix", "status": "pending"}])),
            plan_call("2", json!([{"text": "Read", "status": "completed"}, {"text": "Fix", "status": "in_progress"}])),
        ];
        let steps = last_plan(&t).unwrap();
        assert_eq!(steps[0].status, "completed");
        assert_eq!(steps[1], Step { text: "Fix".into(), status: "in_progress".into() });
        assert!(last_plan(&[Message::user("hi")]).is_none());
    }

    #[test]
    fn an_open_plan_is_restated_and_a_finished_one_is_not() {
        let open = steps_of(&json!({"steps": [{"text": "Read", "status": "completed"}, {"text": "Fix `cli`", "status": "in_progress"}, {"text": "Test", "status": "pending"}]}));
        let note = resume_note(&open).unwrap();
        assert!(note.contains("1 of 3 steps done; in progress: step 2, “Fix `cli`”"), "{note}");
        let done = steps_of(&json!([{"text": "Read", "status": "completed"}]));
        assert!(resume_note(&done).is_none() && reminder(&done).is_none());
    }

    #[test]
    fn with_nothing_in_progress_the_next_pending_step_is_named() {
        let steps = steps_of(&json!([{"text": "Read", "status": "completed"}, {"text": "Fix", "status": "pending"}]));
        assert!(reminder(&steps).unwrap().contains("next: step 2, “Fix”"));
    }

    #[test]
    fn the_reminder_comes_after_enough_calls_without_an_update_and_an_update_resets_it() {
        let conv = "c-plan-memory-test";
        set(conv, steps_of(&json!([{"text": "Fix", "status": "in_progress"}])));
        for _ in 1..REMIND_EVERY {
            assert!(tick(conv).is_none());
        }
        assert!(tick(conv).unwrap().contains("update_plan"));
        set(conv, steps_of(&json!([{"text": "Fix", "status": "in_progress"}])));
        assert!(tick(conv).is_none(), "an update starts the count again");
        forget(conv);
        assert!(tick(conv).is_none(), "no plan, no reminder");
    }

    #[test]
    fn an_open_plan_is_closed_once_before_finishing() {
        let conv = "c-plan-closing-test";
        set(conv, steps_of(&json!([{"text": "Fix", "status": "completed"}, {"text": "Run the tests", "status": "in_progress"}])));
        let note = closing_note(conv).unwrap();
        assert!(note.contains("2. “Run the tests”") && !note.contains("Fix"), "{note}");
        assert!(closing_note(conv).is_none(), "asked once per run, not in a loop");
        set(conv, steps_of(&json!([{"text": "Fix", "status": "completed"}])));
        assert!(closing_note(conv).is_none(), "a finished plan lets the run end");
        forget(conv);
    }
}
