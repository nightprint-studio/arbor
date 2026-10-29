//! The sub-agent tools: `spawn_agent` and `await_agents`, which run a child loop; `file_owner` and
//! `request_change`, which talk about the other conversations.
//!
//! A child is the same loop as the conversation's own (`bennu_assistant::run`) with a host of its own: a fresh
//! context (the system prompt and the task), a narrower set of tools, and the parent's access policy narrowed
//! by `restrict_policy` — never wider. What it needs beyond that is asked of the user, in the parent's thread,
//! labelled with the agent: a child does not widen its own reach, even under an autonomous parent.
//!
//! The child runs on a thread of its own so the parent can wait on it, stop it, or (for read-only children) go on
//! and collect it later with `await_agents`. Only the child's final summary goes back into the parent's context.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::time::Duration;

use bennu_assistant::prelude::{
    self as assistant, SpawnPlan, change_request_note, child_card_block, child_cap_eur, child_mode, describe_owner, final_answer, find_model, plan_spawn, report_for_parent, restrict_policy,
    child_failure_note, child_waiting_text, Anthropic, ChildInfo, ChildStatus, ChildTools, Effort, Ended, Outcome, ProviderError, SideJob, SubagentBudget, WorktreeMode, CHILD_MAX_STEPS, MAX_CHANGE_REQUESTS_PER_RUN,
};
use bennu_core::prelude::BennuState;

use super::families::Family;
use super::*;
use crate::assistant::view::View;
use crate::assistant::{digest_store, models, prompt, read_memory, side_jobs, subagents};

pub(super) const SPAWN: &str = "spawn_agent";
pub(super) const AWAIT: &str = "await_agents";
pub(super) const OWNER: &str = "file_owner";
pub(super) const REQUEST: &str = "request_change";
pub(super) const RESUME: &str = "resume_agent";

pub(super) fn is_agent_tool(name: &str) -> bool {
    matches!(name, SPAWN | AWAIT | OWNER | REQUEST | RESUME)
}

/// What an agent is never given, whatever it was started with: it cannot put questions to the user (its access
/// prompts are the host's), draw into the conversation, open panels, keep a plan or load skills.
const CHILD_NEVER: [&str; 5] = ["ask_user", "show_diagram", "open_ui", "update_plan", "load_skill"];

/// The families an agent is given; every other one is off, so a family added later is closed to agents until someone
/// decides otherwise. A reader gets what looks; one that edits also gets changing and building. Starting agents,
/// talking to the other conversations, driving processes, the debugger and the screenshots are never an agent's.
fn child_families(tools: ChildTools) -> &'static [Family] {
    match tools {
        ChildTools::Read => &[Family::Read, Family::Project, Family::Bevy],
        ChildTools::ReadEdit => &[Family::Read, Family::Project, Family::Bevy, Family::Edit, Family::Refactor, Family::Run, Family::History, Family::Structural],
    }
}

/// The ids of the families an agent does without.
pub(super) fn child_off_ids(tools: ChildTools) -> Vec<&'static str> {
    let given = child_families(tools);
    Family::ALL.into_iter().filter(|f| !given.contains(f)).map(Family::id).collect()
}

/// Why an agent's call is refused before it reaches the access policy, if it is.
pub(super) fn child_refuses(name: &str, safety: Option<Safety>, read_only: bool) -> Option<String> {
    if CHILD_NEVER.contains(&name) {
        return Some("a sub-agent cannot do that: decide for yourself, say what you assumed in your summary, and finish".into());
    }
    if is_agent_tool(name) {
        return Some("a sub-agent cannot start agents or write to other conversations".into());
    }
    if read_only && matches!(safety, Some(s) if s != Safety::Read) {
        return Some("this agent was started read-only: it cannot change or run anything".into());
    }
    None
}

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![
        spec(
            SPAWN,
            "Start a sub-agent: a helper with a clean context that knows only the task you give it, and sends back only a summary. Use it for an independent side task, for exploration that would fill your own context with file contents, or for work unrelated to what you were doing. Do not use it for something one or two calls would answer, or for work you must see step by step. `tools`: `read` (default; it searches and reads, on a cheaper model) or `read+edit` (it may change files — only when your own edits are applied rather than proposed; you wait for it). Put in `task` everything it needs: what to find or do, where to look, what to report. `background: true` (read only) returns at once with an id, and `await_agents` collects the summary. At most 3 run at once.",
            json!({"type":"object","properties":{
                "title":{"type":"string","description":"A few words for the user's Agents list."},
                "task":{"type":"string","description":"The whole brief: the agent has no other context."},
                "tools":{"type":"string","enum":["read","read+edit"]},
                "background":{"type":"boolean","description":"Return at once with an id instead of waiting (read-only agents)."},
                "effort":{"type":"string","enum":["low","medium","high"],"description":"How hard it thinks. Default medium."}
            },"required":["title","task"]}),
        ),
        spec(
            RESUME,
            "Continue a sub-agent that has finished (or was stopped, or was cut off by a restart) with a new instruction, instead of starting over. It picks up its own earlier thread: what it said, decided and concluded is kept; what it read is not kept in full (only a note that it read the file), so it reads again what it needs. Use it when an agent's summary was close and you need a follow-up. Same limits as spawn_agent. Its kept thread is deleted when the conversation is compacted or deleted, or when the user deletes the agent — then start a new one.",
            json!({"type":"object","properties":{
                "id":{"type":"string","description":"The agent's id (spawn_agent's answer, or the Agents list)."},
                "message":{"type":"string","description":"What it should do next, with whatever it needs that its earlier thread does not already say."},
                "background":{"type":"boolean","description":"Return at once with its id, as for spawn_agent (read-only agents)."}
            },"required":["id","message"]}),
        ),
        spec(
            AWAIT,
            "Wait for background agents and return their summaries. `ids` defaults to every agent you started in the background and have not collected.",
            json!({"type":"object","properties":{"ids":{"type":"array","items":{"type":"string"}}}}),
        ),
        spec(
            OWNER,
            "Who is working on a file: the conversation that has reserved it or last changed it, whether it is running now, and the step of its plan it is on. Read-only.",
            json!({"type":"object","properties":{"file":{"type":"string","description":"Absolute path, or relative to the project."}},"required":["file"]}),
        ),
        spec(
            REQUEST,
            "Ask the conversation that owns a file to change it, by leaving it a note it reads as coming from another agent (not from the user). Use it instead of editing a file another conversation holds. The same request is delivered once; a run may make a few.",
            json!({"type":"object","properties":{
                "file":{"type":"string","description":"The file the other conversation holds."},
                "description":{"type":"string","description":"The change you need, specific enough to act on."}
            },"required":["file","description"]}),
        ),
    ]
}

/// An agent being picked up again: its id, its first task, what it had cost, the thread it is sent and the blocks
/// the modal had drawn.
struct Resume {
    id: String,
    task: String,
    started_before: f64,
    messages: Vec<Message>,
    blocks: Vec<Value>,
}

/// What a run needs to start agents: the model and the settings its own requests use.
pub(in crate::assistant) struct AgentSetup {
    pub model: String,
    pub max_tokens: u32,
    pub long_cache: bool,
    /// The project's instructions, rendered: given to an agent that edits, not to a reader.
    pub instructions: String,
    pub worktree: Option<WorktreeMode>,
}

/// A run's state for the agent tools.
#[derive(Default)]
pub(super) struct Agents {
    setup: Option<AgentSetup>,
    /// Agents started in the background, by id, with the channel their report arrives on.
    pending: Mutex<HashMap<String, Receiver<String>>>,
    limiter: Mutex<bennu_assistant::prelude::RequestLimiter>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl ToolHost {
    /// Let this run start agents.
    pub(in crate::assistant) fn with_agents(mut self, setup: AgentSetup) -> Self {
        self.agents.setup = Some(setup);
        self
    }

    pub(super) fn run_agent_tool(&self, name: &str, input: &Value) -> Result<String, String> {
        match name {
            SPAWN => self.run_spawn(input),
            AWAIT => self.run_await(input),
            OWNER => self.run_file_owner(input),
            REQUEST => self.run_request_change(input),
            RESUME => self.run_resume(input),
            other => Err(format!("`{other}` is not a tool you have")),
        }
    }

    // ── spawn_agent ──────────────────────────────────────────────────────────────────────────────

    fn run_spawn(&self, input: &Value) -> Result<String, String> {
        let setup = self.agents.setup.as_ref().ok_or("sub-agents are switched off for this conversation")?;
        let text = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
        let tools_word = if text("tools").trim().is_empty() { "read" } else { text("tools") };
        let background = input.get("background").and_then(Value::as_bool).unwrap_or(false);
        let running = subagents::registry().running_of_run(&self.session.run_id).len();
        let parent_allows_edits = self.session.policy().allow_edits;
        let plan = plan_spawn(text("title"), text("task"), tools_word, background, running, self.session.mode, parent_allows_edits)?;
        let effort = input.get("effort").and_then(Value::as_str).and_then(Effort::parse).unwrap_or(Effort::Medium);
        self.start_child(setup, plan, effort, None)
    }

    // ── resume_agent ─────────────────────────────────────────────────────────────────────────────

    /// Continue an agent that has ended: its kept thread plus a new instruction. What it read is not kept in full
    /// (see `agent_store`), so it knows what it had looked at and concluded, and reads again what it needs.
    fn run_resume(&self, input: &Value) -> Result<String, String> {
        let setup = self.agents.setup.as_ref().ok_or("sub-agents are switched off for this conversation")?;
        let id = input.get("id").and_then(Value::as_str).map(str::trim).filter(|i| !i.is_empty()).ok_or("`id` is required: the agent's id, as spawn_agent or the Agents list gave it")?;
        let message = input.get("message").and_then(Value::as_str).map(str::trim).filter(|m| !m.is_empty()).ok_or("`message` is required: what the agent should do next")?;
        let background = input.get("background").and_then(Value::as_bool).unwrap_or(false);

        let conversation = &self.session.conversation_id;
        subagents::hydrate(conversation);
        let info = subagents::registry().get(id).filter(|c| &c.conversation_id == conversation).ok_or_else(|| format!("`{id}` is not an agent of this conversation (or it was deleted, or the conversation was compacted)"))?;
        if info.status.is_running() {
            return Err(format!("`{id}` is still running: collect it with `await_agents`, or stop it first"));
        }
        let record = crate::assistant::agent_store::load(conversation, id).ok_or_else(|| format!("what `{id}` had worked out is no longer kept: start a new agent with `spawn_agent`"))?;

        let running = subagents::registry().running_of_run(&self.session.run_id).len();
        let plan = plan_spawn(&info.title, message, &info.tools, background, running, self.session.mode, self.session.policy().allow_edits)?;
        let resume = Resume {
            id: info.id.clone(),
            task: info.task.clone(),
            started_before: info.cost,
            messages: bennu_assistant::prelude::continue_with(record.messages, message),
            blocks: record.display.as_ref().and_then(|d| d["blocks"].as_array().cloned()).unwrap_or_default(),
        };
        self.start_child(setup, plan, Effort::Medium, Some(resume))
    }

    /// The part `spawn_agent` and `resume_agent` share: start the agent's thread with the messages it is to be sent,
    /// record it, put its card in this run's message, and wait for it or hand back its id.
    fn start_child(&self, setup: &AgentSetup, plan: SpawnPlan, effort: Effort, resume: Option<Resume>) -> Result<String, String> {
        let policy = restrict_policy(&self.session.policy(), self.session.mode, plan.tools);
        let (table, _) = models::load();
        let model_id = if plan.tools == ChildTools::Read { side_jobs::side_model(SideJob::Exploration, &setup.model) } else { setup.model.clone() };
        let model = find_model(&table, &model_id).cloned();
        let effort = model.as_ref().and_then(|m| effort.clamp_to(&m.efforts));
        let key = crate::assistant::api_key(&self.session.ctx)?;

        let id = resume.as_ref().map_or_else(|| subagents::registry().next_id(&self.session.run_id), |r| r.id.clone());
        let now = crate::assistant::now_ms();
        let mode = child_mode(self.session.mode);
        let read_only = plan.tools == ChildTools::Read;

        let mut ctx = BennuState::new(self.session.ctx.event_sink());
        if let Some(host) = self.session.ctx.host_caller() {
            ctx = ctx.with_host_caller(host);
        }
        let cancel = runs::begin(&id);
        let mut view = View::new(format!("g{id}"), model_id.clone(), now);
        // A resumed agent's thread goes on from where it was: what it did before, then why it is back.
        if let Some(r) = &resume {
            r.blocks.iter().for_each(|b| view.push(b.clone()));
            view.push(json!({ "type": "notice", "level": "info", "text": format!("Resumed by main: {}", plan.task.chars().take(300).collect::<String>()) }));
        }
        let mut session = Session::new(id.clone(), id.clone(), self.session.root.clone(), mode, std::sync::Arc::new(subagents::NullSink), ctx, cancel, policy, view);
        session.projects = self.session.projects.clone();
        session.title = self.session.title.clone();
        session.build_isolation = self.session.build_isolation;
        session.prompt_via = Some((Arc::clone(&self.session), plan.title.clone()));
        let session = Arc::new(session);

        let system = child_system(&session, setup, read_only);
        let messages = match &resume {
            Some(r) => r.messages.clone(),
            None => vec![Message::user(plan.task.clone())],
        };
        let request = Request {
            model: model_id.clone(),
            system,
            messages,
            tools: Vec::new(),
            max_tokens: setup.max_tokens,
            effort,
            long_cache: setup.long_cache,
        };
        let guard = self.guard.as_ref().map(|g| Guard { limits: g.limits, periods: g.periods, session_base: g.session_base, model: model.clone() });
        let (cap_eur, eur_per_usd) = self.guard.as_ref().map_or((child_cap_eur(0.0), 1.0), |g| (child_cap_eur(g.limits.session_eur), g.limits.eur_per_usd));

        let base_cost = resume.as_ref().map_or(0.0, |r| r.started_before);
        let info = ChildInfo {
            id: id.clone(),
            conversation_id: self.session.conversation_id.clone(),
            run_id: self.session.run_id.clone(),
            title: plan.title.clone(),
            task: resume.as_ref().map_or_else(|| plan.task.clone(), |r| r.task.clone()),
            tools: if read_only { "read" } else { "read+edit" }.into(),
            model: model_id.clone(),
            status: ChildStatus::Running,
            cost: base_cost,
            started_at: now,
            ended_at: None,
            summary: None,
            error: None,
            waiting: None,
        };
        if resume.is_some() {
            subagents::registry().update(&id, |c| *c = info.clone());
        } else {
            subagents::registry().add(info.clone());
        }
        subagents::register_live(&id, Arc::clone(&session));
        // Kept from the start, so an agent the app is closed on is found again, stopped, and not lost.
        subagents::keep(&info, Some(session.display_snapshot()), &request.messages);
        self.session.view().push(child_card_block(&info));
        self.session.push_message();
        self.session.emit("agent", json!({ "agent": info }));

        let (tx, rx) = mpsc::channel();
        let job = Job {
            session,
            parent: Arc::clone(&self.session),
            rt: self.rt.clone(),
            key,
            request,
            guard,
            off: self.off.with(&child_off_ids(plan.tools)),
            model,
            cap_eur,
            eur_per_usd,
            read_only,
            base_cost,
            tx,
        };
        std::thread::Builder::new()
            .name(format!("assistant-agent-{id}"))
            .spawn(move || run_child(job))
            .map_err(|e| {
                subagents::registry().update(&id, |c| c.status = ChildStatus::Failed);
                format!("could not start the agent: {e}")
            })?;

        let preface = plan.notes.join(" ");
        if plan.background {
            lock(&self.agents.pending).insert(id.clone(), rx);
            return Ok(format!("{preface} Agent “{}” started in the background as `{id}`. Go on with your own work; collect its summary with `await_agents`.", plan.title).trim().to_string());
        }
        let report = self.wait_report(&id, &rx);
        Ok(format!("{preface}\n{report}").trim().to_string())
    }

    /// Wait for a child's report, stopping it if this run is cancelled meanwhile.
    fn wait_report(&self, id: &str, rx: &Receiver<String>) -> String {
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(report) => return report,
                Err(RecvTimeoutError::Timeout) => {
                    if self.session.cancelled() {
                        subagents::cancel_child(id);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return lost_report(id),
            }
        }
    }

    // ── await_agents ─────────────────────────────────────────────────────────────────────────────

    fn run_await(&self, input: &Value) -> Result<String, String> {
        let wanted: Vec<String> = match input.get("ids").and_then(Value::as_array) {
            Some(list) if !list.is_empty() => list.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            _ => lock(&self.agents.pending).keys().cloned().collect(),
        };
        if wanted.is_empty() {
            return Err("no agent is waiting to be collected: start one with `spawn_agent` and `background: true`".into());
        }
        let mut reports = Vec::new();
        for id in wanted {
            let rx = lock(&self.agents.pending).remove(&id);
            reports.push(match rx {
                Some(rx) => self.wait_report(&id, &rx),
                None => match subagents::registry().get(&id) {
                    Some(c) if !c.status.is_running() => lost_report(&id),
                    _ => format!("`{id}` is not an agent you started in the background (or you already collected it)."),
                },
            });
        }
        Ok(reports.join("\n\n---\n\n"))
    }

    // ── file_owner and request_change ────────────────────────────────────────────────────────────

    fn run_file_owner(&self, input: &Value) -> Result<String, String> {
        let file = input.get("file").and_then(Value::as_str).map(str::trim).filter(|f| !f.is_empty()).ok_or("`file` is required")?;
        let key = self.resolve(file).display().to_string();
        let me = self.session.owner_conversation();
        let holder = reserve::holder_of(&key, me);
        let writer = crate::build_queue::journal().last_writer(&key).filter(|w| w.conversation != me);
        let step = holder.as_ref().and_then(|h| subagents::plan_step_of(&h.conversation));
        Ok(describe_owner(
            file,
            holder.as_ref().map(|h| (h.conversation.as_str(), h.title.as_str(), crate::assistant::conversation_active(&h.conversation))),
            writer.as_ref().map(|w| (w.conversation.as_str(), w.title.as_str())),
            step.as_deref(),
        ))
    }

    fn run_request_change(&self, input: &Value) -> Result<String, String> {
        let text = |k: &str| input.get(k).and_then(Value::as_str).map(str::trim).unwrap_or("");
        let (file, description) = (text("file"), text("description"));
        if file.is_empty() || description.is_empty() {
            return Err("`file` and `description` are required".into());
        }
        let description: String = description.chars().take(600).collect();
        let key = self.resolve(file).display().to_string();
        let me = self.session.owner_conversation();
        let owner = match reserve::holder_of(&key, me) {
            Some(c) => (c.conversation, c.title),
            None => match crate::build_queue::journal().last_writer(&key).filter(|w| w.conversation != me) {
                Some(w) => (w.conversation, w.title),
                None => return Err("no other conversation holds or last changed that file: edit it yourself".into()),
            },
        };
        if !lock(&self.agents.limiter).allow() {
            return Err(format!("you have made {MAX_CHANGE_REQUESTS_PER_RUN} change requests in this run: do the rest yourself, or tell the user what is waiting on the other conversation"));
        }
        let note = change_request_note(&self.session.title, file, &description);
        if !crate::build_queue::inbox().push(&owner.0, note) {
            return Ok("Not delivered: that conversation was already sent the same request, or it already has as many notes waiting as it takes. Do not insist.".into());
        }
        self.note("info", &format!("Asked conversation “{}” to change {file}.", owner.1.trim()));
        let running = crate::assistant::conversation_active(&owner.0);
        Ok(format!(
            "Left a note for conversation {} (\"{}\"), which {}. It reads the note with its next tool result or message, and decides for itself; do not wait for it.",
            owner.0,
            owner.1.trim(),
            if running { "is running now" } else { "is idle, so it will see the note when the user next writes to it" }
        ))
    }
}

fn lost_report(id: &str) -> String {
    match subagents::registry().get(id) {
        Some(c) => report_for_parent(&c.title, c.status, c.summary.as_deref().unwrap_or(""), c.cost, c.error.as_deref().unwrap_or("")),
        None => format!("The agent `{id}` is gone."),
    }
}

/// The system prompt of an agent: the conversation's own, as for a plain run, and what an agent is.
fn child_system(session: &Session, setup: &AgentSetup, read_only: bool) -> String {
    let instructions = if read_only { "" } else { setup.instructions.as_str() };
    let mut system = prompt::system(&prompt::Context {
        root: &session.root,
        mode: session.mode,
        allow_edits: !read_only && session.policy().allow_edits,
        chat: false,
        instructions,
        git: false,
        hide_authors: false,
        skills: "",
        plan: false,
        projects: &session.projects,
        live: false,
        worktree: setup.worktree,
    });
    system.push_str(CHILD_GUIDE);
    if read_only {
        system.push_str("You can only read and search: you cannot change anything, so do not try.\n");
    }
    system
}

const CHILD_GUIDE: &str = "\nYou are a sub-agent: the main assistant started you for one task, which is your first message. You know nothing of the conversation that started you. Do that task with the tools you have. You cannot ask the user questions (decide, and say what you assumed) and you cannot start other agents. Your final message is the only thing the main assistant receives, so make it a self-contained summary — what you found or did, file paths with line numbers, anything it must decide — short, without pasting whole files. If you cannot finish, say what is missing.\n";

// ── The child's loop ─────────────────────────────────────────────────────────────────────────────

struct Job {
    session: Arc<Session>,
    parent: Arc<Session>,
    rt: tokio::runtime::Handle,
    key: String,
    request: Request,
    guard: Option<Guard>,
    off: Off,
    model: Option<ModelInfo>,
    cap_eur: f64,
    eur_per_usd: f64,
    read_only: bool,
    /// What the agent had cost before this run of it (it was resumed).
    base_cost: f64,
    tx: mpsc::Sender<String>,
}

/// Whatever way the child's thread ends, what it held is let go and its record says it is over.
struct ChildEnd {
    id: String,
    parent: Arc<Session>,
}

impl Drop for ChildEnd {
    fn drop(&mut self) {
        runs::finish(&self.id);
        reserve::release_run(&self.id);
        read_memory::forget(&self.id);
        digest_store::forget(&self.id);
        crate::assistant::ledger::in_flight().finish(&self.id);
        subagents::end_live(&self.id);
        // A thread that panicked never wrote its ending: do it, so the list does not show it running forever.
        let mut crashed = false;
        subagents::registry().update(&self.id, |c| {
            if c.status.is_running() {
                c.status = ChildStatus::Failed;
                c.error = Some("the agent stopped unexpectedly".into());
                c.ended_at = Some(crate::assistant::now_ms());
                crashed = true;
            }
        });
        if crashed {
            if let Some(info) = subagents::registry().get(&self.id) {
                subagents::announce(&self.parent, &info);
            }
        }
    }
}

/// The child's host: the tool host with the agent's own rules on top. Its notes from the build queue are not
/// read (they are the conversation's), what it asks of the user goes to the parent's thread, and it stops when it
/// has spent its share.
struct ChildHost {
    inner: ToolHost,
    parent: Arc<Session>,
    id: String,
    model: Option<ModelInfo>,
    cap_eur: f64,
    eur_per_usd: f64,
    read_only: bool,
    capped: AtomicBool,
}

impl ChildHost {
    /// Every request so far, the one in progress included.
    fn requests(&self) -> Vec<Usage> {
        let spend = lock(&self.inner.run_spend);
        let mut all = spend.finished.clone();
        if spend.current != Usage::default() {
            all.push(spend.current);
        }
        all
    }

    fn spent_usd(&self) -> f64 {
        self.model.as_ref().map_or(0.0, |m| m.cost_of_all(&self.requests()))
    }

    /// Stop the agent when it has spent its share: held at every request, and at every retry of one.
    fn check_cap(&self) {
        let mut budget = SubagentBudget::new(self.cap_eur);
        budget.record(&self.id, self.spent_usd() * self.eur_per_usd);
        if !budget.child_may_continue(&self.id) {
            self.capped.store(true, Ordering::SeqCst);
            self.inner.session.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Show on its card (and in the Agents list) that it is waiting to send a request again, or that it is not.
    fn set_waiting(&self, waiting: Option<String>) {
        subagents::registry().update(&self.id, |c| c.waiting = waiting);
        if let Some(info) = subagents::registry().get(&self.id) {
            subagents::announce(&self.parent, &info);
        }
    }

    fn publish_cost(&self) {
        let usd = self.spent_usd();
        subagents::registry().update(&self.id, |c| c.cost = usd);
        if let Some(info) = subagents::registry().get(&self.id) {
            subagents::announce(&self.parent, &info);
        }
    }
}

impl Host for ChildHost {
    fn tools(&self) -> Vec<ToolSpec> {
        self.inner.tools().into_iter().filter(|t| !CHILD_NEVER.contains(&t.name.as_str())).collect()
    }

    fn prepare(&self, request: &mut Request) {
        self.inner.prepare(request);
        self.check_cap();
    }

    fn before_retry(&self) {
        self.inner.before_retry();
        self.check_cap();
    }

    fn gate(&self, call: &ToolCall) -> Gate {
        let safety = self.inner.descriptor(&call.name).map(|d| d.safety);
        match child_refuses(&call.name, safety, self.read_only) {
            Some(why) => Gate::Refuse(why),
            None => self.inner.gate(call),
        }
    }

    fn run(&self, call: &ToolCall) -> Result<String, String> {
        // Not `Host::run`: that appends the conversation's build-queue notes, which are not for an agent.
        self.inner.run_tool(call)
    }

    fn summarize(&self, call: &ToolCall, result: &Result<String, String>) -> String {
        self.inner.summarize(call, result)
    }

    fn take_images(&self, call: &ToolCall) -> Vec<bennu_assistant::prelude::ImageRef> {
        self.inner.take_images(call)
    }

    fn event(&self, event: AgentEvent) {
        let request_started = matches!(event, AgentEvent::Request { .. });
        let waiting = match &event {
            AgentEvent::Retry { attempt, max, wait_secs, kind, .. } => Some(Some(child_waiting_text(*attempt, *max, *wait_secs, kind.status_text()))),
            // Data again after a wait: the agent is working.
            AgentEvent::Text(_) | AgentEvent::Thinking(_) | AgentEvent::ToolStart { .. } | AgentEvent::Usage(_) if self.inner.retrying.load(Ordering::SeqCst) => Some(None),
            _ => None,
        };
        self.inner.event(event);
        if let Some(waiting) = waiting {
            self.set_waiting(waiting);
        }
        if request_started {
            self.publish_cost();
        }
    }

    fn cancelled(&self) -> bool {
        self.inner.cancelled()
    }
}

/// How a child's run ended, as the status of its card and the sentence that goes with a failure. A request that
/// failed says what failed, whether trying again can help and what the agent had already changed — the parent must
/// not read the silence as "nothing found".
fn child_ending(outcome: &Outcome, capped: bool, cap_eur: f64) -> (ChildStatus, String) {
    match outcome.ended {
        Ended::Done => (ChildStatus::Done, String::new()),
        Ended::Cancelled if capped => (ChildStatus::Failed, format!("it reached its spending cap (€{cap_eur:.2})")),
        Ended::Cancelled => (ChildStatus::Stopped, String::new()),
        Ended::TooManySteps => (ChildStatus::Failed, format!("it used its {CHILD_MAX_STEPS} steps without finishing")),
        Ended::Incomplete(_) => (ChildStatus::Failed, "its answer was cut short".into()),
        Ended::Failed => {
            let error = outcome.error.clone().unwrap_or_else(|| ProviderError::local("the run failed"));
            let changed = bennu_assistant::prelude::files_touched(&outcome.messages).len();
            (ChildStatus::Failed, child_failure_note(&error, changed))
        }
    }
}

fn run_child(job: Job) {
    let Job { session, parent, rt, key, request, guard, off, model, cap_eur, eur_per_usd, read_only, base_cost, tx } = job;
    let id = session.run_id.clone();
    let _end = ChildEnd { id: id.clone(), parent: Arc::clone(&parent) };
    let host = ChildHost {
        inner: ToolHost::new(Arc::clone(&session), rt, None, guard, false, Vec::new(), false, off, Vec::new()),
        parent: Arc::clone(&parent),
        id: id.clone(),
        model: model.clone(),
        cap_eur,
        eur_per_usd,
        read_only,
        capped: AtomicBool::new(false),
    };
    let model_id = request.model.clone();
    let outcome = match HttpTransport::new() {
        Ok(transport) => assistant::run(&Anthropic, &transport, &host, &key, request, CHILD_MAX_STEPS),
        Err(e) => Outcome::failed_to_start(request.messages, ProviderError::local(e)),
    };

    let capped = host.capped.load(Ordering::SeqCst);
    let (status, note) = child_ending(&outcome, capped, cap_eur);
    let (answer, requests) = (final_answer(&outcome.messages), outcome.requests);

    let cost = base_cost + model.as_ref().map_or(0.0, |m| m.cost_of_all(&requests));
    // The money goes onto the parent's run, at the agent's own prices, where its limits see it.
    for usage in &requests {
        side_jobs::record(&parent.run_id, &model_id, usage);
    }
    let summary: String = answer.chars().take(bennu_assistant::prelude::MAX_SUMMARY_CHARS).collect();
    subagents::registry().update(&id, |c| {
        c.status = status;
        c.cost = cost;
        c.waiting = None;
        c.ended_at = Some(crate::assistant::now_ms());
        c.summary = Some(summary.clone());
        c.error = (!note.is_empty()).then(|| note.clone());
    });
    // The session's last state is kept for the transcript before the card says it is over.
    subagents::end_live(&id);
    if let Some(info) = subagents::registry().get(&id) {
        // And on disk, for as long as the conversation keeps it: the thread it drew and the thread it was sent.
        subagents::keep(&info, Some(session.display_snapshot()), &outcome.messages);
        subagents::announce(&parent, &info);
        let _ = tx.send(report_for_parent(&info.title, status, &answer, cost, &note));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_that_lost_its_connection_says_so_instead_of_returning_nothing() {
        let outcome = Outcome::failed_to_start(Vec::new(), ProviderError::offline("dns error"));
        let (status, note) = child_ending(&outcome, false, 1.0);
        assert_eq!(status, ChildStatus::Failed);
        assert!(note.contains("could not reach the API") && note.contains("Nothing was changed") && note.contains("may work"), "{note}");
        let report = report_for_parent("Callers", status, "", 0.0, &note);
        assert!(report.contains("ended early") && report.contains("could not reach the API"), "{report}");
        // A user's stop is not a failure.
        let stopped = Outcome { ended: Ended::Cancelled, ..outcome };
        assert_eq!(child_ending(&stopped, false, 1.0).0, ChildStatus::Stopped);
        assert_eq!(child_ending(&stopped, true, 1.0).0, ChildStatus::Failed);
    }

    #[test]
    fn the_agent_tools_are_recognised_and_sorted_into_families() {
        for t in [SPAWN, AWAIT, OWNER, REQUEST, RESUME] {
            assert!(is_agent_tool(t));
        }
        assert!(!is_agent_tool("edit_file"));
        assert_eq!(super::super::families::family_of(SPAWN), Some(super::super::families::Family::Agents));
        assert_eq!(super::super::families::family_of(REQUEST), Some(super::super::families::Family::Peers));
        let names: Vec<String> = specs().into_iter().map(|s| s.name).collect();
        assert_eq!(names, [SPAWN, RESUME, AWAIT, OWNER, REQUEST]);
    }

    #[test]
    fn a_read_only_agent_is_offered_nothing_that_changes_or_runs_things() {
        use super::super::families::{family_of, Family};
        let off = Off::from_ids(&[]).with(&child_off_ids(ChildTools::Read));
        let offered = offered_specs(false, false, &off, &[]);
        assert!(!offered.is_empty());
        for t in &offered {
            let family = family_of(&t.name);
            assert!(matches!(family, None | Some(Family::Read) | Some(Family::Project) | Some(Family::Bevy)), "{} ({family:?}) is offered to a reader", t.name);
        }
        for must_not in ["edit_file", "edit_files", "create_file", "delete_file", "rename_symbol", "replace_in_files", "structural_replace", "local_history", "bennu_build", "run_start", SPAWN, REQUEST, RESUME] {
            assert!(!offered.iter().any(|t| t.name == must_not), "{must_not} is offered to a reader");
        }
    }

    #[test]
    fn an_agent_that_edits_still_cannot_start_agents_or_write_to_other_conversations() {
        let off = Off::from_ids(&[]).with(&child_off_ids(ChildTools::ReadEdit));
        let offered = offered_specs(false, false, &off, &[]);
        assert!(offered.iter().any(|t| t.name == "edit_file"));
        for must_not in [SPAWN, AWAIT, OWNER, REQUEST, RESUME, "run_start", "debug_launch"] {
            assert!(!offered.iter().any(|t| t.name == must_not), "{must_not} is offered to an agent");
        }
    }

    #[test]
    fn a_reader_is_refused_every_catalogue_tool_that_is_not_a_read() {
        for d in tools_for("").into_iter().filter(offered) {
            let refused = child_refuses(&d.name, Some(d.safety), true).is_some();
            assert_eq!(refused, d.safety != Safety::Read, "{}", d.name);
            // An agent that edits is held to the policy, not to this list.
            assert!(child_refuses(&d.name, Some(d.safety), false).is_none(), "{}", d.name);
        }
    }

    #[test]
    fn an_agent_never_asks_the_user_nor_starts_agents_whatever_it_can_edit() {
        for read_only in [true, false] {
            for tool in ["ask_user", "show_diagram", "open_ui", "update_plan", "load_skill", SPAWN, AWAIT, OWNER, REQUEST, RESUME] {
                assert!(child_refuses(tool, None, read_only).is_some(), "{tool}");
            }
        }
    }
}
