//! One run's side of the conversation: the [`Session`] that streams the message to the panel and asks the
//! user, and the [`ImberHost`] that plugs the Corvus tools and the approval rule into the shared agent loop
//! (`imber_core::agent`).
//!
//! The loop decides nothing about git. For each call the model makes it asks the host `gate` (may this run?)
//! and then `run`. `gate` is where manual mode lives: a write is shown as a card with the exact command
//! and the run parks until the user answers or cancels.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use corvus_core::prelude::CorvusState;
use imber_core::prelude::{detail_of, AgentEvent, Gate, Host, ToolCall, ToolSpec, Usage, View};
use serde_json::{json, Value};

use super::permission::{classify, needs_approval_for, preview, unattended_allows, Mode, Risk};
use super::{plan_tools, runs, tools};

pub const TOPIC: &str = "arbor://corvus/imber";
/// Streamed text is pushed at most this often; a card or a receipt is pushed at once.
const TEXT_PUSH_EVERY: Duration = Duration::from_millis(60);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// What a run shares between the loop's thread and the handlers that answer or cancel it.
pub struct Session {
    pub run_id: String,
    pub conversation_id: String,
    sink: Arc<dyn EventSink>,
    pub cancel: Arc<AtomicBool>,
    view: Mutex<View>,
    usage: Mutex<Usage>,
    last_push: Mutex<Instant>,
    prompt_seq: std::sync::atomic::AtomicU64,
}

impl Session {
    pub fn new(run_id: String, conversation_id: String, sink: Arc<dyn EventSink>, cancel: Arc<AtomicBool>, view: View) -> Self {
        Self {
            run_id,
            conversation_id,
            sink,
            cancel,
            view: Mutex::new(view),
            usage: Mutex::new(Usage::default()),
            last_push: Mutex::new(Instant::now() - TEXT_PUSH_EVERY),
            prompt_seq: std::sync::atomic::AtomicU64::new(1),
        }
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn view(&self) -> MutexGuard<'_, View> {
        lock(&self.view)
    }

    pub fn add_usage(&self, u: Usage) -> Usage {
        let mut total = lock(&self.usage);
        *total = total.merge(u);
        *total
    }

    /// Send an event about this run to the panel.
    pub fn emit(&self, kind: &str, extra: Value) {
        let mut payload = json!({ "run_id": self.run_id, "conversation_id": self.conversation_id, "kind": kind });
        if let (Some(obj), Some(more)) = (payload.as_object_mut(), extra.as_object()) {
            obj.extend(more.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        self.sink.emit(TOPIC, payload);
    }

    /// The message as it stands, pushed to the panel.
    pub fn push_message(&self) {
        let message = self.view().snapshot();
        self.emit("message", json!({ "message": message }));
        *lock(&self.last_push) = Instant::now();
    }

    fn push_message_soon(&self) {
        if lock(&self.last_push).elapsed() >= TEXT_PUSH_EVERY {
            self.push_message();
        }
    }

    /// Put a question to the user and wait. `block` joins the message with a fresh `promptId`; the panel
    /// answers with `imber_reply`. `None` comes back when the run is cancelled while waiting. `answered`
    /// turns the answer into the change that marks the card decided, so it stays in the thread as a record.
    pub fn ask(&self, mut block: Value, id_pointer: &str, id: &str, answered: impl FnOnce(&mut Value, &Value)) -> Option<Value> {
        let prompt_id = format!("p{}-{}", self.prompt_seq.fetch_add(1, Ordering::SeqCst), self.run_id);
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

/// The agent loop's host for a Corvus conversation.
pub struct ImberHost {
    pub session: Arc<Session>,
    pub state: Arc<CorvusState>,
    pub handle: tokio::runtime::Handle,
    pub root: String,
    pub conversation_id: String,
    pub mode: Mode,
    /// A chat offers no tools.
    pub chat: bool,
    /// A scheduled run: nobody is there to approve, so [`unattended_allows`] decides instead of a card.
    pub unattended: bool,
    /// Calls shown as an approval card: their result goes on the card instead of a second receipt.
    carded: Mutex<HashSet<String>>,
    retrying: AtomicBool,
    /// The model's context window, when known: what each request's `max_tokens` is cut to fit.
    pub window: Option<u64>,
}

impl ImberHost {
    #[allow(clippy::too_many_arguments)]
    pub fn new(session: Arc<Session>, state: Arc<CorvusState>, handle: tokio::runtime::Handle, root: String, conversation_id: String, mode: Mode, chat: bool, unattended: bool) -> Self {
        Self { session, state, handle, root, conversation_id, mode, chat, unattended, carded: Mutex::new(HashSet::new()), retrying: AtomicBool::new(false), window: None }
    }

    /// The plan a plan tool is about, in full, for the card — `None` for any other tool or an unknown plan.
    fn plan_of(&self, call: &ToolCall) -> Value {
        if !plan_tools::PLAN_TOOLS.contains(&call.name.as_str()) {
            return Value::Null;
        }
        call.input
            .get("plan_id")
            .and_then(Value::as_str)
            .and_then(|id| crate::plan::plan_get(&self.state, id.to_string()).ok())
            .and_then(|p| serde_json::to_value(p).ok())
            .unwrap_or(Value::Null)
    }

    /// After a plan tool ran: put the plan in the chat as a card (or refresh the one already there). The card
    /// keeps a snapshot; the panel shows the live plan when it has it, so each step's outcome appears as it happens.
    fn attach_plan(&self, call: &ToolCall, result: &str) {
        if !plan_tools::PLAN_TOOLS.contains(&call.name.as_str()) {
            return;
        }
        let id = call
            .input
            .get("plan_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| serde_json::from_str::<Value>(result).ok().and_then(|v| v.get("plan_id").and_then(Value::as_str).map(str::to_string)));
        let Some(plan) = id.and_then(|id| crate::plan::plan_get(&self.state, id).ok()).and_then(|p| serde_json::to_value(p).ok()) else { return };
        let plan_id = plan["id"].as_str().unwrap_or_default().to_string();
        let updated = self.session.view().update("/plan/id", &plan_id, |b| b["plan"] = plan.clone());
        if !updated {
            self.session.view().push(json!({ "type": "plan", "plan": plan }));
        }
        self.session.push_message();
    }

    /// Show the exact action and wait for the user. `true` only on an explicit approval.
    fn approved(&self, call: &ToolCall, risk: Risk) -> bool {
        let p = preview(&call.name, &call.input);
        lock(&self.carded).insert(call.id.clone());
        let block = json!({
            "type": "approval",
            "approval": {
                "id": call.id, "tool": call.name, "title": p.title, "commands": p.commands, "effect": p.effect,
                "risk": risk.word(), "input": call.input, "plan": self.plan_of(call), "answer": Value::Null, "result": Value::Null,
            },
        });
        let answer = self.session.ask(block, "/approval/id", &call.id, |b, a| {
            b["approval"]["answer"] = a.get("answer").cloned().unwrap_or(json!("reject"));
        });
        answer.as_ref().and_then(|a| a.get("answer")).and_then(Value::as_str) == Some("approve")
    }
}

impl Host for ImberHost {
    fn tools(&self) -> Vec<ToolSpec> {
        if self.chat { Vec::new() } else { tools::specs() }
    }

    fn context_window(&self) -> Option<u64> {
        self.window
    }

    fn gate(&self, call: &ToolCall) -> Gate {
        let risk = classify(&call.name, &call.input);
        if self.unattended {
            return match unattended_allows(self.mode, risk, &call.name) {
                Ok(()) => Gate::Run,
                Err(why) => Gate::Refuse(why),
            };
        }
        if !needs_approval_for(&call.name, self.mode, risk) {
            return Gate::Run;
        }
        if self.approved(call, risk) {
            Gate::Run
        } else {
            Gate::Refuse("The user declined this action, so it was not run. Do not retry it; ask what they would like instead.".into())
        }
    }

    fn run(&self, call: &ToolCall) -> Result<String, String> {
        let ctx = tools::Ctx { state: &self.state, handle: &self.handle, root: &self.root, conversation_id: &self.conversation_id, mode: self.mode, unattended: self.unattended };
        let result = tools::run(&ctx, &call.name, &call.input);
        if let Ok(text) = &result {
            self.attach_plan(call, text);
        }
        result
    }

    fn summarize(&self, call: &ToolCall, result: &Result<String, String>) -> String {
        receipt_detail(&call.name, result)
    }

    fn event(&self, event: AgentEvent) {
        let s = &self.session;
        // Data from the provider after a wait: the connection is back, the status line goes.
        if matches!(event, AgentEvent::Text(_) | AgentEvent::Thinking(_) | AgentEvent::ToolStart { .. } | AgentEvent::Usage(_)) && self.retrying.swap(false, Ordering::SeqCst) {
            s.emit("retry_end", json!({}));
        }
        match event {
            AgentEvent::Retry { attempt, max, wait_secs, kind, detail } => {
                self.retrying.store(true, Ordering::SeqCst);
                s.emit("retry", json!({ "attempt": attempt, "max": max, "wait_secs": wait_secs, "kind": kind.code(), "detail": detail }));
            }
            AgentEvent::RetryDiscard => {
                s.view().discard_attempt();
                s.push_message();
            }
            AgentEvent::Text(t) => {
                s.view().text(&t);
                s.push_message_soon();
            }
            AgentEvent::ToolDone { id, name, input, summary, ok, timing } => {
                if lock(&self.carded).contains(&id) {
                    s.view().update("/approval/id", &id, |b| b["approval"]["result"] = json!({ "ok": ok, "detail": summary }));
                } else {
                    s.view().tool(&id, &name, &input, &summary, ok, timing);
                }
                s.push_message();
            }
            AgentEvent::Usage(u) => {
                let total = s.add_usage(u);
                s.emit("usage", json!({ "input": total.input_tokens, "output": total.output_tokens, "cache_read": total.cache_read_tokens, "cache_write": total.cache_write_tokens }));
            }
            AgentEvent::Request { .. } => {
                s.view().begin_attempt();
                s.emit("status", json!({ "status": "thinking" }));
            }
            AgentEvent::ToolStart { name, .. } => {
                s.emit("status", json!({ "status": "tool", "tool": name }));
                // Flush the text streamed since the last throttled push: the tool may take long.
                s.push_message();
            }
            AgentEvent::Thinking(_) => s.emit("status", json!({ "status": "thinking" })),
            // Corvus's thread has no row for a model step: the receipts carry their own times.
            AgentEvent::Responded { .. } => {}
        }
    }

    fn cancelled(&self) -> bool {
        self.session.cancelled()
    }
}

/// The line a finished call leaves in the thread: what it made, not the JSON it returned.
pub fn receipt_detail(name: &str, result: &Result<String, String>) -> String {
    let Ok(text) = result else { return detail_of(result) };
    let json: Option<Value> = serde_json::from_str(text).ok();
    let count = |noun: &str| json.as_ref().and_then(Value::as_array).map(|a| format!("{} {noun}{}", a.len(), if a.len() == 1 { "" } else { "s" }));
    let field = |k: &str| json.as_ref().and_then(|v| v.get(k)).and_then(Value::as_str).map(str::to_string);
    let line = match name {
        "git_commit" => field("short").zip(field("summary")).map(|(short, summary)| format!("{short} {summary}")),
        "git_log" => count("commit"),
        "git_branches" => count("branch"),
        "git_mr_list" => count("request"),
        "git_blame" => count("run"),
        "git_mr_create" => json.as_ref().and_then(|v| v.get("number")).and_then(Value::as_u64).map(|n| format!("#{n}")),
        _ => None,
    };
    line.unwrap_or_else(|| detail_of(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct Recorder(StdMutex<Vec<(String, Value)>>);

    impl EventSink for Recorder {
        fn emit(&self, topic: &str, payload: Value) {
            self.0.lock().unwrap().push((topic.to_string(), payload));
        }
    }

    fn session(rec: Arc<Recorder>, run: &str) -> Arc<Session> {
        Arc::new(Session::new(run.into(), "c1".into(), rec, runs::begin(run), View::new("a1".into(), "m".into(), 0)))
    }

    #[test]
    fn a_question_blocks_until_the_answer_arrives_and_the_card_records_it() {
        let rec = Arc::new(Recorder::default());
        let s = session(rec.clone(), "host-test-1");
        let asker = {
            let s = Arc::clone(&s);
            std::thread::spawn(move || s.ask(json!({"type": "approval", "approval": {"id": "t1", "answer": null}}), "/approval/id", "t1", |b, a| b["approval"]["answer"] = a["answer"].clone()))
        };
        // The card reaches the panel with its prompt id; answer it as the panel would.
        let prompt_id = loop {
            let seen = rec.0.lock().unwrap().iter().find_map(|(_, p)| p["message"]["blocks"][0]["promptId"].as_str().map(str::to_string));
            if let Some(id) = seen {
                break id;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(runs::reply(&prompt_id, json!({"answer": "approve"})));
        assert_eq!(asker.join().unwrap(), Some(json!({"answer": "approve"})));
        assert_eq!(s.view().snapshot()["blocks"][0]["approval"]["answer"], "approve");
        runs::finish("host-test-1");
    }

    #[test]
    fn cancelling_while_parked_on_a_question_releases_the_run() {
        let rec = Arc::new(Recorder::default());
        let s = session(rec, "host-test-2");
        let asker = {
            let s = Arc::clone(&s);
            std::thread::spawn(move || s.ask(json!({"type": "approval", "approval": {"id": "t2"}}), "/approval/id", "t2", |_, _| {}))
        };
        std::thread::sleep(Duration::from_millis(50));
        assert!(runs::cancel("host-test-2"));
        assert_eq!(asker.join().unwrap(), None);
        runs::finish("host-test-2");
    }

    #[test]
    fn events_carry_the_run_and_conversation_ids() {
        let rec = Arc::new(Recorder::default());
        let s = session(rec.clone(), "host-test-3");
        s.emit("done", json!({"ended": "done"}));
        let events = rec.0.lock().unwrap();
        assert_eq!(events[0].0, TOPIC);
        assert_eq!(events[0].1["run_id"], "host-test-3");
        assert_eq!(events[0].1["conversation_id"], "c1");
        assert_eq!(events[0].1["ended"], "done");
        runs::finish("host-test-3");
    }

    #[test]
    fn receipts_say_what_was_made_not_the_json() {
        let ok = |s: &str| Ok::<String, String>(s.to_string());
        assert_eq!(receipt_detail("git_commit", &ok(r#"{"commit":"abc123456","short":"abc12345","summary":"fix: totals","files":2}"#)), "abc12345 fix: totals");
        assert_eq!(receipt_detail("git_log", &ok(r#"[{"a":1},{"a":2}]"#)), "2 commits");
        assert_eq!(receipt_detail("git_log", &ok("[]")), "0 commits");
        assert_eq!(receipt_detail("git_branches", &ok(r#"[{"name":"main"}]"#)), "1 branch");
        assert_eq!(receipt_detail("git_mr_create", &ok(r#"{"number":42,"url":"u"}"#)), "#42");
        assert_eq!(receipt_detail("git_status", &ok("plain text\nsecond")), "plain text");
        assert_eq!(receipt_detail("git_push", &Err("rejected: behind\nmore".into())), "rejected: behind");
    }
}
