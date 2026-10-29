//! The tools the model is given, and the rules about when they may run.
//!
//! Two kinds, one list. **Bennu's own tools** — the `#[handler(mcp(...))]` catalogue the AI clients
//! already use: usages, symbols, endpoints, beans, problems, the rename plan — are the whole of Bennu's
//! intelligence, called in-process; nothing is re-implemented for the assistant. **The assistant's own**
//! are the few things those do not cover: a text search, a file finder, an exact edit, create and
//! delete, and a way to ask the user. There is no shell and no way to build one.
//!
//! Every call crosses [`Host::gate`] first, which is where the user's limits live: the reach (file,
//! project, every project, anywhere), the one prompt that decides read-only or read-and-write when a
//! path is outside it, and the confirmation that deletions and destructive tools always need.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arbor_history::prelude::compare_text;
use arbor_rpc::{async_registry_for, registry_for, tools_for};
use arbor_rpc::tool::{Safety, ToolDescriptor, ToolOutput};
use bennu_assistant::prelude::{
    sleep_interruptible, AgentEvent, Decision, ErrorKind, Gate, Host, Limits, Message, ModelInfo, Op, Pause, Request, Scope, ToolCall, ToolSpec, Usage,
};
use serde_json::{json, Value};

use super::compact::{self, tokens};
use super::git;
use super::projects;
use super::pseudonyms::{restore_value, scrub_value};
use super::runs::{self, PendingEdit};
use super::session::Session;
use super::skills::Skill;
use super::text_tools::{self, FindArgs, SearchArgs, Span};
use super::transport::HttpTransport;
use super::view::detail_of;

mod autonomy;
mod batch;
mod brp;
mod debug;
mod debug_bp;
mod debug_capture;
mod debug_consoles;
mod debug_keep;
mod debug_launch;
mod debug_text;
mod diagram;
mod diagram_context;
mod edit;
mod families;
mod gate;
mod digests;
mod history;
mod live;
mod own_specs;
mod reads;
pub(super) mod reserve;

/// End the debug session a conversation left open (it is being deleted).
pub(super) fn release_debug(conversation: &str) {
    debug_keep::release(conversation, "the conversation was deleted");
}

/// End the kept debug sessions of a project (it closed), or all of them (`None`: Arbor is exiting).
pub(super) fn release_debug_matching(root: Option<&str>, reason: &str) {
    debug_keep::release_matching(root, reason);
}
mod screenshot;
mod skills;
mod spawn;
mod structural;
mod ui;

pub(super) use families::Off;
pub(super) use spawn::AgentSetup;

/// A tool definition: its name, what it is for, and the shape of its arguments.
fn spec(name: &str, description: &str, input_schema: Value) -> ToolSpec {
    ToolSpec { name: name.into(), description: description.into(), input_schema }
}

/// The most text one tool result carries back to the model. A result past this is cut with a note
/// saying so — a silent cut would read as a complete answer.
const MAX_RESULT_CHARS: usize = 60_000;

/// Every tool the model is offered: the assistant's own, then Bennu's catalogue. Free of any run, so a
/// dry run can show exactly the list a real one would send.
pub(super) fn all_specs() -> Vec<ToolSpec> {
    let mut specs = ToolHost::own_specs();
    specs.extend(batch::specs());
    specs.extend(brp::specs());
    specs.extend(debug::specs());
    specs.extend(diagram::specs());
    specs.extend(history::specs());
    specs.extend(live::specs());
    specs.extend(reads::specs());
    specs.extend(screenshot::specs());
    specs.extend(skills::specs());
    specs.extend(spawn::specs());
    specs.extend(structural::specs());
    specs.extend(ui::specs());
    specs.extend(tools_for("").into_iter().filter(offered).map(|d| ToolSpec {
        name: d.name,
        description: d.description,
        // The model sends flat arguments; the host re-wraps them for the handler.
        input_schema: d.input_schema,
    }));
    specs
}

/// The list a run is offered: everything, less the step plan when it is off, `load_skill` when no skill is
/// enabled (a tool with nothing to load is only noise) and every family the user has switched off, plus git
/// when it is on. The one place that decides, so a dry run shows what a run sends.
pub(super) fn offered_specs(plan: bool, skills: bool, off: &Off, git: &[ToolDescriptor]) -> Vec<ToolSpec> {
    let mut specs = all_specs();
    specs.retain(|t| (plan || t.name != "update_plan") && (skills || t.name != "load_skill") && !off.hides(&t.name));
    specs.extend(git.iter().map(|d| ToolSpec { name: d.name.clone(), description: d.description.clone(), input_schema: d.input_schema.clone() }));
    specs
}

/// The families named in settings, as the run's filter.
pub(super) fn families_off(ids: &[String]) -> Off {
    Off::from_ids(ids)
}

/// Tools that exist for the editor's own use or for diagnosis, not for a conversation.
fn offered(d: &ToolDescriptor) -> bool {
    let n = d.name.as_str();
    // The assistant has its own, encoding-safe edit tools; a raw write would bypass the proposal step.
    !(n == "bennu_write_file" || n.starts_with("bennu_shader") || n.contains("wgsl") || n.contains("dtolab"))
}

/// When and how a conversation that has grown too large is made smaller on its own.
pub(super) struct Compaction {
    pub key: String,
    pub model: String,
    /// Estimated input tokens at which compacting starts: 90% of the user's largest window.
    pub threshold: u64,
}

/// The spending limits a run is held to, and what had been spent when it began.
pub(super) struct Guard {
    pub limits: Limits,
    /// Where the day, week and month began: the ledger is summed afresh at each request, so what other
    /// conversations finished since this run began counts.
    pub periods: super::ledger::Periods,
    /// What the conversation had cost before this run.
    pub session_base: f64,
    /// The model's prices; without them the run's cost is unknown and nothing is enforced mid-run.
    pub model: Option<ModelInfo>,
}

/// What the run has used so far: the requests finished, and the one in progress.
#[derive(Default)]
struct RunSpend {
    finished: Vec<Usage>,
    current: Usage,
}

pub(super) struct ToolHost {
    session: Arc<Session>,
    catalog: Vec<ToolDescriptor>,
    rt: tokio::runtime::Handle,
    compaction: Option<Compaction>,
    guard: Option<Guard>,
    run_spend: std::sync::Mutex<RunSpend>,
    /// A conversation with no tools: none are offered, and none would run.
    chat: bool,
    /// Corvus's git tools, when the user has turned git on.
    git: Vec<ToolDescriptor>,
    /// The step plan is on: `update_plan` is offered.
    plan: bool,
    /// Tool families the user has switched off: not offered, and refused if called.
    off: Off,
    /// The skills the user enabled, read when the run began: what `load_skill` can return.
    skills: Vec<Skill>,
    /// The last thing the panel was told the run was doing, so a change is reported and a repeat is not.
    last_status: std::sync::Mutex<String>,
    /// How many `show_diagram` sources failed to parse in this run: past a few, the tool gives up.
    diagram_failures: std::sync::Mutex<u32>,
    /// The debug session this run opened, if any; released when the run ends.
    debug: debug::DebugRun,
    /// The sub-agent tools: what this run may start, and the agents it is waiting for.
    agents: spawn::Agents,
    /// The panel was told the run is waiting to retry, and has not been told it resumed.
    retrying: std::sync::atomic::AtomicBool,
}

impl ToolHost {
    pub(super) fn new(session: Arc<Session>, rt: tokio::runtime::Handle, compaction: Option<Compaction>, guard: Option<Guard>, chat: bool, git: Vec<ToolDescriptor>, plan: bool, off: Off, skills: Vec<Skill>) -> Self {
        let catalog = tools_for("").into_iter().filter(offered).collect();
        let host = Self { session, catalog, rt, compaction, guard, run_spend: Default::default(), chat, git, plan, off, skills, last_status: Default::default(), diagram_failures: Default::default(), debug: Default::default(), agents: Default::default(), retrying: Default::default() };
        // A debug session the conversation left open is this run's again.
        host.debug.adopt(&host.session);
        host
    }

    /// Make the conversation smaller when it has reached the window the user allows.
    fn compact_if_needed(&self, request: &mut Request) {
        let Some(c) = &self.compaction else { return };
        if compact::estimate_request(request) < c.threshold {
            return;
        }
        let Ok(transport) = HttpTransport::new() else { return };
        self.status("compacting", None, self.iteration());
        // The part of the request that is not the conversation: it stays, so it counts against the limit.
        let tools_json = serde_json::to_string(&request.tools).unwrap_or_default();
        let overhead = tokens(&request.system) + tokens(&tools_json);
        let size = |m: &[Message]| overhead + tokens(&serde_json::to_string(m).unwrap_or_default()) + bennu_assistant::prelude::image_tokens(m);

        match compact::compact(&request.messages, &transport, &c.key, &c.model, None, false, &size, c.threshold) {
            Ok(done) => {
                if done.summarized {
                    // Written by the cheaper side model, so priced at its rates (not with the main model's requests).
                    super::side_jobs::record(&self.session.run_id, &c.model, &done.usage);
                }
                request.messages = done.transcript;
                let text = if done.summarized {
                    format!("The conversation reached its window limit. {}", done.dropped)
                } else {
                    "The conversation was nearing its window limit: older tool output was cleared to make room.".to_string()
                };
                self.note("info", &text);
            }
            Err(e) => self.note("warning", &format!("The conversation is near its window limit and could not be compacted ({}). It was left as it was.", e.kind.status_text().to_lowercase())),
        }
    }

    /// A request failed in a way that can pass: close its accounting (what it billed is a request of its own, as
    /// the loop counts it) and tell the panel — one status line, no toast per attempt.
    fn report_retry(&self, attempt: u32, max: u32, wait_secs: u64, kind: ErrorKind, detail: &str) {
        {
            let mut spend = self.run_spend.lock().unwrap_or_else(|p| p.into_inner());
            let lost = std::mem::take(&mut spend.current);
            if lost != Usage::default() {
                spend.finished.push(lost);
            }
        }
        self.retrying.store(true, std::sync::atomic::Ordering::SeqCst);
        self.session.emit(
            "retry",
            json!({
                "attempt": attempt, "max": max, "wait_s": wait_secs, "until": super::now_ms() + wait_secs * 1000,
                "error_kind": kind.code(), "message": kind.status_text(), "detail": detail,
            }),
        );
    }

    /// Which request of the run is in progress: the finished ones, plus this.
    fn iteration(&self) -> u32 {
        self.run_spend.lock().unwrap_or_else(|p| p.into_inner()).finished.len() as u32 + 1
    }

    /// Stop the run, before the next request, if it has taken the spending to a limit.
    fn enforce_limits(&self) {
        let Some(Guard { limits, periods, session_base, model: Some(model) }) = &self.guard else { return };
        let run_usd = {
            let spend = self.run_spend.lock().unwrap_or_else(|p| p.into_inner());
            let extra = self.session.extra_requests.lock().unwrap_or_else(|p| p.into_inner());
            model.cost_of_all(&spend.finished) + model.cost(&spend.current) + model.cost_of_all(&extra) + super::side_jobs::cost_of(&self.session.run_id)
        };
        // This run's figure goes where the other runs can see it, and theirs come back: two conversations
        // running at once must not together pass a limit that neither passes alone.
        let others = super::ledger::in_flight().report(&self.session.run_id, run_usd);
        let spent = super::ledger::spent(periods, *session_base).plus_run(run_usd, others);
        if let Some((_, message)) = limits.breach(&spent) {
            self.note("warning", &format!("Stopped: {message}. Raise the limit in Settings › Imber › Spending limits to go on."));
            self.session.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Tell the panel what the run is doing — "thinking", "writing", a tool — when that has changed.
    fn status(&self, phase: &str, tool: Option<&str>, iteration: u32) {
        let key = format!("{phase}/{}/{iteration}", tool.unwrap_or(""));
        let mut last = self.last_status.lock().unwrap_or_else(|p| p.into_inner());
        if *last == key {
            return;
        }
        *last = key;
        self.session.emit("status", json!({ "phase": phase, "tool": tool, "iteration": iteration }));
    }

    /// Say, in the thread, that the conversation was made smaller and how.
    fn note(&self, level: &str, text: &str) {
        self.session.view().push(json!({ "type": "notice", "level": level, "text": text }));
        self.session.push_message();
    }

    fn descriptor(&self, name: &str) -> Option<&ToolDescriptor> {
        self.catalog.iter().chain(self.git.iter()).find(|d| d.name == name)
    }

    fn is_git(&self, name: &str) -> bool {
        self.git.iter().any(|d| d.name == name)
    }

    fn project(&self) -> &Path {
        Path::new(&self.session.root)
    }

    /// Absolute path for `raw`: a project's name (or `name:path` in one) first, then as given when absolute,
    /// otherwise under the project the user is in.
    fn resolve(&self, raw: &str) -> PathBuf {
        if let Some(real) = projects::expand(&self.session.projects, raw) {
            return PathBuf::from(real);
        }
        let p = Path::new(raw);
        if p.is_absolute() { p.to_path_buf() } else { self.project().join(p) }
    }

    // ── Running ──────────────────────────────────────────────────────────────────────────────────

    /// Who this run's tool calls are, for the build queue: the conversation, its cancel flag and where it builds.
    pub(super) fn caller_context(&self) -> crate::build_queue::CallContext {
        crate::build_queue::CallContext {
            owner: crate::build_queue::Owner { conversation: self.session.owner_conversation().to_string(), title: self.session.title.clone() },
            cancel: Arc::clone(&self.session.cancel),
            isolation: self.session.build_isolation,
        }
    }

    /// Call a handler of the catalogue in-process.
    fn call_handler(&self, method: &str, params: Value) -> Result<Value, String> {
        // The handler learns who is calling (for the build queue and the blame of a broken build) from the
        // thread, not from its arguments: the same handlers serve the panel and external clients.
        crate::build_queue::with_context(self.caller_context(), || {
            let ctx = &self.session.ctx as &dyn std::any::Any;
            if let Some(call) = registry_for("").get(method) {
                return call(ctx, params);
            }
            if let Some(call) = async_registry_for("").get(method) {
                return self.rt.block_on(call(ctx, params));
            }
            Err(format!("unknown tool `{method}`"))
        })
    }

    /// Append the system notes Bennu left for this conversation — a build another conversation ran failed in
    /// files this one may have changed — to a tool result. The model cannot be interrupted mid-stream; this is
    /// the next place it reads anything.
    fn with_notes(&self, result: Result<String, String>) -> Result<String, String> {
        let notes = crate::build_queue::inbox().take(&self.session.conversation_id);
        if notes.is_empty() {
            return result;
        }
        let text = bennu_assistant::prelude::render_notes(&notes);
        self.note("info", "Bennu passed Imber a system note (another conversation's failed build, or the debugged program stopping).");
        match result {
            Ok(out) => Ok(format!("{out}\n\n---\n{text}")),
            Err(e) => Err(format!("{e}\n\n---\n{text}")),
        }
    }

    fn run_catalog(&self, d: &ToolDescriptor, input: &Value) -> Result<String, String> {
        let value = self.catalog_call(d, input)?;
        // A big result is answered with a digest; the rest of it stays one `expand_result` away.
        Ok(self.digested(&d.name, Some(&value), render(d.output, &value)))
    }

    /// Call a catalogue tool with the project's root filled in and the names of projects turned into paths,
    /// and hand back its raw result. The pieces that want the value (digests, the read memory) start here.
    pub(super) fn catalog_call(&self, d: &ToolDescriptor, input: &Value) -> Result<Value, String> {
        let mut flat = input.clone();
        self.expand_names(&mut flat);
        if let Some(root) = flat.get("root").and_then(Value::as_str) {
            if root != self.session.root && projects::owner(&self.session.projects, Path::new(root)).is_none() {
                return Err(format!("`{root}` is not one of the projects in this conversation"));
            }
        }
        // Nearly every tool takes the project root; the model should not have to repeat it.
        let wants_root = d.input_schema.pointer("/properties/root").is_some();
        if wants_root && flat.get("root").is_none() {
            if let Some(obj) = flat.as_object_mut() {
                obj.insert("root".into(), Value::String(self.session.root.clone()));
            }
        }
        let root = flat.get("root").and_then(Value::as_str).unwrap_or(&self.session.root).to_string();
        let value = self.call_handler(&d.method, d.wrap_arguments(flat))?;
        self.note_diagnostics(&d.name, &root, &value);
        Ok(value)
    }

    /// Call one of Corvus's git tools through the shell. The people in the answer are replaced by ids on the way
    /// back (when that is on), and any id the model wrote in the arguments is turned back into the name.
    fn run_git(&self, d: &ToolDescriptor, input: &Value) -> Result<String, String> {
        let mut args = input.clone();
        if d.input_schema.pointer("/properties/root").is_some() && args.get("root").is_none() {
            if let Some(obj) = args.as_object_mut() {
                obj.insert("root".into(), Value::String(self.session.root.clone()));
            }
        }
        if self.session.hide_authors {
            restore_value(&self.session.authors.lock().unwrap_or_else(|p| p.into_inner()), &mut args);
        }
        let mut value = self.session.ctx.host_call("__assistant_git", json!({ "method": d.method, "args": args }))?;
        if self.session.hide_authors {
            let mut authors = self.session.authors.lock().unwrap_or_else(|p| p.into_inner());
            scrub_value(&mut authors, &mut value);
            authors.save();
        }
        Ok(render(d.output, &value))
    }

    /// The root of the project that owns `file`: where its encoding, index and history live. The project
    /// the user is in for anything outside the ones in play.
    fn root_of(&self, file: &Path) -> String {
        projects::owner(&self.session.projects, file).map(|p| p.root.clone()).unwrap_or_else(|| self.session.root.clone())
    }

    /// Turn a project's name, or `name:path`, in a tool's path arguments into the real path.
    fn expand_names(&self, args: &mut Value) {
        let Some(obj) = args.as_object_mut() else { return };
        let swap = |v: &mut Value| {
            if let Some(real) = v.as_str().and_then(|s| projects::expand(&self.session.projects, s)) {
                *v = Value::String(real);
            }
        };
        for key in ["root", "file", "path", "new_path", "dir"] {
            if let Some(v) = obj.get_mut(key) {
                swap(v);
            }
        }
        for key in ["files", "paths"] {
            if let Some(list) = obj.get_mut(key).and_then(Value::as_array_mut) {
                list.iter_mut().for_each(swap);
            }
        }
    }

    fn read_text(&self, file: &Path) -> Result<(String, String), String> {
        let v = self.call_handler(
            "bennu_read_file",
            json!({"args": {"root": self.root_of(file), "file": file.display().to_string()}}),
        )?;
        let text = v.get("text").and_then(Value::as_str).ok_or("the file could not be read as text")?.to_string();
        let stamp = v.get("stamp").and_then(Value::as_str).unwrap_or("").to_string();
        Ok((text, stamp))
    }

    fn write_text(&self, edit: &PendingEdit) -> Result<(), String> {
        self.call_handler(
            "bennu_write_file",
            json!({"args": {"root": edit.root, "file": edit.file, "text": edit.new_text, "expect_stamp": edit.stamp.clone().unwrap_or_default()}}),
        )
        .map(|_| ())
    }

    /// The projects a text tool is aimed at: `project: "all"`, one by name, or just the one the user is in.
    /// Each comes with the folder to look in.
    fn text_targets(&self, input: &Value, path: Option<&str>) -> Result<Vec<(String, PathBuf, PathBuf)>, String> {
        let which = input.get("project").and_then(Value::as_str).filter(|s| !s.is_empty());
        let chosen: Vec<&projects::Project> = match which {
            None => Vec::new(),
            Some(w) if w.eq_ignore_ascii_case("all") => self.session.projects.iter().collect(),
            Some(name) => vec![self
                .session
                .projects
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("`{name}` is not one of the projects in this conversation"))?],
        };
        if chosen.is_empty() {
            let base = path.map(|p| self.resolve(p)).unwrap_or_else(|| self.project().to_path_buf());
            return Ok(vec![(String::new(), self.project().to_path_buf(), base)]);
        }
        Ok(chosen
            .into_iter()
            .map(|p| {
                let root = PathBuf::from(&p.root);
                let base = match path {
                    Some(rel) if !Path::new(rel).is_absolute() => root.join(rel),
                    Some(abs) => PathBuf::from(abs),
                    None => root.clone(),
                };
                (p.name.clone(), root, base)
            })
            .collect())
    }

    fn run_search(&self, input: &Value) -> Result<String, String> {
        let args: SearchArgs = serde_json::from_value(input.clone()).map_err(|e| format!("bad arguments: {e}"))?;
        let targets = self.text_targets(input, args.path.as_deref())?;
        let mut parts = Vec::new();
        for (name, root, base) in &targets {
            let found = text_tools::search(root, base, &args)?;
            parts.push(if targets.len() > 1 || !name.is_empty() { format!("── {name} ──\n{found}") } else { found });
        }
        Ok(parts.join("\n\n"))
    }

    fn run_find(&self, input: &Value) -> Result<String, String> {
        let args: FindArgs = serde_json::from_value(input.clone()).map_err(|e| format!("bad arguments: {e}"))?;
        let targets = self.text_targets(input, args.path.as_deref())?;
        let mut parts = Vec::new();
        for (name, root, base) in &targets {
            let found = text_tools::find(root, base, &args)?;
            parts.push(if targets.len() > 1 || !name.is_empty() { format!("── {name} ──\n{found}") } else { found });
        }
        Ok(parts.join("\n\n"))
    }


    /// Record the plan: one card in the message, replaced whole each time the model sends the list.
    fn run_plan(&self, input: &Value) -> Result<String, String> {
        let steps = parse_plan(input)?;
        let (done, total) = (steps.iter().filter(|s| s["status"] == "completed").count(), steps.len());
        crate::assistant::subagents::note_plan_step(&self.session.conversation_id, &steps);
        let id = format!("plan-{}", self.session.run_id);
        let block = json!({ "type": "plan", "plan": { "id": id, "steps": steps } });
        let mut view = self.session.view();
        let replaced = view.update("/plan/id", &id, |b| *b = block.clone());
        if !replaced {
            view.push(block);
        }
        drop(view);
        self.session.push_message();
        Ok(format!("plan updated: {done} of {total} steps done"))
    }

    fn run_ask(&self, input: &Value) -> Result<String, String> {
        let questions: Vec<Value> = input.get("questions").and_then(Value::as_array).cloned().unwrap_or_default();
        if questions.is_empty() {
            return Err("no questions given".into());
        }
        if !self.session.may_ask_question() {
            return Err("the user asked you to decide for yourself and you have used the questions this run allows: choose, go on, and report the decision at the end".into());
        }
        let set_id = format!("qs-{}-{}", self.session.run_id, questions.len());
        let shaped: Vec<Value> = questions
            .iter()
            .enumerate()
            .map(|(i, q)| {
                json!({
                    "id": format!("q{}", i + 1),
                    "header": q.get("header").cloned().unwrap_or(json!("")),
                    "text": q.get("text").cloned().unwrap_or(json!("")),
                    "options": q.get("options").cloned().unwrap_or(json!([])),
                    "multi": q.get("multi").and_then(Value::as_bool).unwrap_or(false),
                })
            })
            .collect();
        let block = json!({ "type": "questions", "set": { "id": set_id, "questions": shaped } });
        let answer = self.session.ask(block, "/set/id", &set_id, |b, a| {
            b["set"]["answers"] = a.get("answers").cloned().unwrap_or(json!({}));
        });
        let answers = answer
            .as_ref()
            .and_then(|a| a.get("answers"))
            .and_then(Value::as_object)
            .ok_or("the user did not answer")?;
        let mut lines = Vec::new();
        for (i, q) in questions.iter().enumerate() {
            let key = format!("q{}", i + 1);
            let picked: Vec<String> = answers
                .get(&key)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            lines.push(format!("{}: {}", q.get("header").and_then(Value::as_str).unwrap_or(&key), picked.join(", ")));
        }
        Ok(lines.join("\n"))
    }
}

/// The steps of a plan, checked: one to twelve, each with text and a known status, at most one in progress.
fn parse_plan(input: &Value) -> Result<Vec<Value>, String> {
    let raw = input.get("steps").and_then(Value::as_array).ok_or("`steps` is required")?;
    if raw.is_empty() || raw.len() > 12 {
        return Err("a plan has one to twelve steps".into());
    }
    let mut steps = Vec::new();
    for s in raw {
        let text = s.get("text").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).ok_or("every step needs text")?;
        let status = s.get("status").and_then(Value::as_str).unwrap_or("pending");
        if !matches!(status, "pending" | "in_progress" | "completed") {
            return Err(format!("`{status}` is not a status: use pending, in_progress or completed"));
        }
        steps.push(json!({ "text": text.chars().take(140).collect::<String>(), "status": status }));
    }
    if steps.iter().filter(|s| s["status"] == "in_progress").count() > 1 {
        return Err("only one step can be in progress at a time".into());
    }
    Ok(steps)
}


/// An id for a card that must not collide with another in the same message.
fn unique_id(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{prefix}-{nanos}")
}

/// A handler's result as the text the model reads.
fn render(output: ToolOutput, value: &Value) -> String {
    let text = match (output, value) {
        (ToolOutput::Text, Value::String(s)) => s.clone(),
        _ => value.to_string(),
    };
    clip(text)
}

fn clip(text: String) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text;
    }
    let kept: String = text.chars().take(MAX_RESULT_CHARS).collect();
    format!("{kept}\n… cut at {MAX_RESULT_CHARS} characters. Ask for less (a narrower path, a line range, fewer results).")
}

/// What a chat may call: it reads and changes nothing in the project, but it can look up how Bennu works and
/// open a panel or draw a diagram — showing the user a window or a picture touches neither files nor
/// processes, so it needs no access.
pub(super) const CHAT_TOOLS: [&str; 3] = ["bennu_help", "open_ui", diagram::NAME];

pub(super) fn chat_specs() -> Vec<ToolSpec> {
    all_specs().into_iter().filter(|t| CHAT_TOOLS.iter().any(|n| *n == t.name)).collect()
}

impl ToolHost {
    /// Run one tool call (the gate has already let it through).
    fn run_tool(&self, call: &ToolCall) -> Result<String, String> {
        match call.name.as_str() {
            "search_text" => self.run_search(&call.input).map(|found| self.digested("search_text", None, found)),
            "find_files" => self.run_find(&call.input),
            // Reading, without paying twice for what the conversation already holds (see `reads`).
            "bennu_read_file" => self.run_read_file(&call.id, &call.input),
            "read_files" => self.run_read_files(&call.id, &call.input),
            "file_outline" => self.run_outline(&call.input),
            "expand_result" => self.run_expand(&call.input),
            "edit_file" => self.run_edit(&call.input),
            "edit_files" => self.run_edit_files(&call.input),
            "replace_in_files" => self.run_replace_in_files(&call.input),
            "local_history" => self.run_history(&call.input),
            "structural_replace" => self.run_structural(&call.input),
            "open_ui" => self.run_open_ui(&call.input),
            "show_diagram" => self.run_show_diagram(&call.input),
            "load_skill" => self.run_load_skill(&call.input),
            n if spawn::is_agent_tool(n) => self.run_agent_tool(n, &call.input),
            n if live::is_live_tool(n) => self.run_live(n, &call.input),
            n if debug::is_debug_tool(n) => self.run_debug(n, &call.input),
            n if brp::is_brp_tool(n) => self.run_brp(n, &call.input),
            n if screenshot::is_screenshot_tool(n) => self.run_screenshot(&call.id, n, &call.input),
            "create_file" => self.run_create(&call.input),
            "delete_file" => self.run_delete(&call.input),
            "rename_symbol" => self.run_rename(&call.input),
            "add_parameter" => self.run_add_parameter(&call.input),
            "ask_user" => self.run_ask(&call.input),
            "update_plan" => self.run_plan(&call.input),
            "bennu_help" => {
                let query = call.input.get("query").and_then(Value::as_str).unwrap_or("");
                let limit = call.input.get("limit").and_then(Value::as_u64).unwrap_or(4) as usize;
                super::help::search(query, limit)
            }
            other => match self.descriptor(other) {
                Some(d) if self.is_git(other) => self.run_git(d, &call.input),
                Some(d) => self.run_catalog(d, &call.input),
                None => Err(format!("`{other}` is not a tool you have")),
            },
        }
    }
}

impl Host for ToolHost {
    fn tools(&self) -> Vec<ToolSpec> {
        // A chat reads and changes nothing, but it can still look up how Bennu works: that is a built-in
        // text, not the project.
        if self.chat {
            return chat_specs();
        }
        offered_specs(self.plan, !self.skills.is_empty(), &self.off, &self.git)
    }

    fn prepare(&self, request: &mut Request) {
        // First, so the size the compaction measures is the size that would be sent.
        request.messages = diagram_context::prune_diagram_attempts(&request.messages);
        // Only the latest screenshots stay as pictures (the rest become a line), each loaded from disk now —
        // before compaction, so its estimate counts the pictures that would really be sent.
        request.messages = screenshot::prepare_messages(&request.messages, &request.model);
        // Old file reads and search results become one-line stubs — in a batch, when it pays for one rewrite of
        // the provider's cache (see `bennu_assistant::stale`), and before compaction so it measures the result.
        self.stub_stale_results(request);
        self.compact_if_needed(request);
        // Whatever the read memory called "above" must still be there: compaction and stubbing remove results.
        self.sync_read_memory(&request.messages);
        self.enforce_limits();
    }

    fn gate(&self, call: &ToolCall) -> Gate {
        self.gate_call(call)
    }

    fn run(&self, call: &ToolCall) -> Result<String, String> {
        let result = self.run_tool(call);
        self.with_notes(result)
    }

    fn summarize(&self, _call: &ToolCall, result: &Result<String, String>) -> String {
        detail_of(result)
    }

    fn take_images(&self, call: &ToolCall) -> Vec<bennu_assistant::prelude::ImageRef> {
        if screenshot::is_screenshot_tool(&call.name) { self.shown_images(&call.id) } else { Vec::new() }
    }

    /// The wait before a retry ends early on Stop, and on "Retry now" in the panel.
    fn pause(&self, wait: std::time::Duration) -> Pause {
        sleep_interruptible(wait, &|| self.cancelled(), &|| runs::take_retry_now(&self.session.run_id))
    }

    /// A retry is a request like any other: the spending limits hold it too.
    fn before_retry(&self) {
        self.enforce_limits();
    }

    fn event(&self, event: AgentEvent) {
        let s = &self.session;
        // Data from the provider after a wait: the connection is back, the status line goes.
        if matches!(event, AgentEvent::Text(_) | AgentEvent::Thinking(_) | AgentEvent::ToolStart { .. } | AgentEvent::Usage(_)) && self.retrying.swap(false, std::sync::atomic::Ordering::SeqCst) {
            s.emit("retry_end", json!({}));
        }
        match event {
            AgentEvent::Retry { attempt, max, wait_secs, kind, detail } => self.report_retry(attempt, max, wait_secs, kind, &detail),
            AgentEvent::RetryDiscard => {
                s.view().discard_attempt();
                s.push_message();
            }
            AgentEvent::Text(t) => {
                self.status("writing", None, self.iteration());
                s.view().text(&t);
                s.push_message_soon();
            }
            AgentEvent::ToolDone { id, name, input, summary, ok } => {
                // The questions, the proposals and the access cards are the record of those calls.
                // A diagram that was drawn is its own record; one that failed leaves a receipt with the reason.
                if !(name == "ask_user" || name == "update_plan" || (name == diagram::NAME && ok)) {
                    s.view().tool(&id, &name, &input, &summary, ok);
                }
                s.push_message();
            }
            AgentEvent::Usage(u) => {
                {
                    let mut spend = self.run_spend.lock().unwrap_or_else(|p| p.into_inner());
                    spend.current = spend.current.merge(u);
                }
                let total = s.add_usage(u);
                s.emit("usage", json!({
                    "input": total.input_tokens, "output": total.output_tokens,
                    "cache_read": total.cache_read_tokens, "cache_write": total.cache_write_tokens,
                }));
            }
            AgentEvent::Request { iteration } => {
                s.view().begin_attempt();
                self.status("thinking", None, iteration);
                // A new request begins: the one that was in progress is finished and priced on its own.
                let mut spend = self.run_spend.lock().unwrap_or_else(|p| p.into_inner());
                let done = std::mem::take(&mut spend.current);
                if done != Usage::default() {
                    spend.finished.push(done);
                }
            }
            AgentEvent::Thinking(_) => self.status("thinking", None, self.iteration()),
            AgentEvent::ToolStart { name, .. } => self.status("tool", Some(&name), self.iteration()),
        }
    }

    fn cancelled(&self) -> bool {
        self.session.cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_is_checked_before_it_is_shown() {
        let ok = parse_plan(&json!({"steps": [{"text": "Read it", "status": "completed"}, {"text": "Fix it", "status": "in_progress"}]})).unwrap();
        assert_eq!(ok.len(), 2);
        assert!(parse_plan(&json!({"steps": []})).is_err());
        assert!(parse_plan(&json!({"steps": [{"text": "a", "status": "doing"}]})).unwrap_err().contains("not a status"));
        let two = json!({"steps": [{"text": "a", "status": "in_progress"}, {"text": "b", "status": "in_progress"}]});
        assert!(parse_plan(&two).unwrap_err().contains("only one"));
        assert!(parse_plan(&json!({"steps": [{"text": "  ", "status": "pending"}]})).is_err());
        let many: Vec<Value> = (0..13).map(|i| json!({"text": format!("s{i}"), "status": "pending"})).collect();
        assert!(parse_plan(&json!({"steps": many})).is_err());
    }


    #[test]
    fn a_long_result_is_cut_and_says_so() {
        let long = "x".repeat(MAX_RESULT_CHARS + 10);
        let out = clip(long);
        assert!(out.contains("cut at") && out.chars().count() < MAX_RESULT_CHARS + 200);
        assert_eq!(clip("short".into()), "short");
    }

    #[test]
    fn show_diagram_is_offered_to_the_agent_and_to_a_chat() {
        assert!(all_specs().iter().any(|t| t.name == "show_diagram"));
        let chat: Vec<String> = chat_specs().into_iter().map(|t| t.name).collect();
        for tool in CHAT_TOOLS {
            assert!(chat.iter().any(|n| n == tool), "{tool} is not offered to a chat");
        }
        assert!(chat.iter().any(|n| n == "show_diagram"));
    }

    #[test]
    fn a_text_result_is_not_wrapped_in_quotes() {
        assert_eq!(render(ToolOutput::Text, &json!("hello")), "hello");
        assert_eq!(render(ToolOutput::Json, &json!({"a": 1})), "{\"a\":1}");
    }
}
