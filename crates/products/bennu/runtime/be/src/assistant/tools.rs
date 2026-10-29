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
    AgentEvent, Decision, Gate, Host, Limits, Message, ModelInfo, Op, Request, Scope, Spent, ToolCall, ToolSpec, Usage,
};
use serde_json::{json, Value};

use super::compact::{self, tokens};
use super::git;
use super::projects;
use super::pseudonyms::{restore_value, scrub_value};
use super::runs::{self, PendingEdit};
use super::session::Session;
use super::text_tools::{self, FindArgs, SearchArgs, Span};
use super::transport::HttpTransport;
use super::view::detail_of;

mod batch;
mod families;
mod history;
mod structural;
mod ui;

pub(super) use families::Off;

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
    specs.extend(history::specs());
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

/// The list a run is offered: everything, less the step plan when it is off and every family the user
/// has switched off, plus git when it is on. The one place that decides, so a dry run shows what a run sends.
pub(super) fn offered_specs(plan: bool, off: &Off, git: &[ToolDescriptor]) -> Vec<ToolSpec> {
    let mut specs = all_specs();
    specs.retain(|t| (plan || t.name != "update_plan") && !off.hides(&t.name));
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
    /// Spent in each window before this run (the session's own cost included).
    pub baseline: Spent,
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
    /// The last thing the panel was told the run was doing, so a change is reported and a repeat is not.
    last_status: std::sync::Mutex<String>,
}

impl ToolHost {
    pub(super) fn new(session: Arc<Session>, rt: tokio::runtime::Handle, compaction: Option<Compaction>, guard: Option<Guard>, chat: bool, git: Vec<ToolDescriptor>, plan: bool, off: Off) -> Self {
        let catalog = tools_for("").into_iter().filter(offered).collect();
        Self { session, catalog, rt, compaction, guard, run_spend: Default::default(), chat, git, plan, off, last_status: Default::default() }
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
        let size = |m: &[Message]| overhead + tokens(&serde_json::to_string(m).unwrap_or_default());

        match compact::compact(&request.messages, &transport, &c.key, &c.model, None, false, &size, c.threshold) {
            Ok(done) => {
                if done.summarized {
                    self.session.extra_requests.lock().unwrap_or_else(|p| p.into_inner()).push(done.usage);
                }
                request.messages = done.transcript;
                self.note(
                    "info",
                    if done.summarized {
                        "The conversation reached its window limit and was compacted: the earlier part is now a summary."
                    } else {
                        "The conversation was nearing its window limit: older tool output was cleared to make room."
                    },
                );
            }
            Err(e) => self.note("warning", &format!("The conversation is near its window limit and could not be compacted: {e}")),
        }
    }

    /// Which request of the run is in progress: the finished ones, plus this.
    fn iteration(&self) -> u32 {
        self.run_spend.lock().unwrap_or_else(|p| p.into_inner()).finished.len() as u32 + 1
    }

    /// Stop the run, before the next request, if it has taken the spending to a limit.
    fn enforce_limits(&self) {
        let Some(Guard { limits, baseline, model: Some(model) }) = &self.guard else { return };
        let run_usd = {
            let spend = self.run_spend.lock().unwrap_or_else(|p| p.into_inner());
            let extra = self.session.extra_requests.lock().unwrap_or_else(|p| p.into_inner());
            model.cost_of_all(&spend.finished) + model.cost(&spend.current) + model.cost_of_all(&extra)
        };
        if let Some((_, message)) = limits.breach(&baseline.plus(run_usd)) {
            self.note("warning", &format!("Stopped: {message}. Raise the limit in Settings › Assistant › Limits to go on."));
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

    // ── Specs ────────────────────────────────────────────────────────────────────────────────────

    fn own_specs() -> Vec<ToolSpec> {
        let spec = |name: &str, description: &str, schema: Value| ToolSpec {
            name: name.into(),
            description: description.into(),
            input_schema: schema,
        };
        vec![
            spec(
                "search_text",
                "Search the text of files for a string or a regular expression, like grep. Returns matching lines with their file and line number, optionally with context. Skips build output, version-control folders and binaries. Prefer Bennu's symbol tools (usages, symbols) when the question is about code structure; use this for strings, comments, configuration and anything the index does not model.",
                json!({"type":"object","properties":{
                    "pattern":{"type":"string","description":"Text or regular expression to look for."},
                    "path":{"type":"string","description":"Folder or file to search; absolute or relative to the project. Defaults to the project root."},
                    "regex":{"type":"boolean","description":"Treat pattern as a regular expression. Default false (plain text)."},
                    "ignore_case":{"type":"boolean"},
                    "extensions":{"type":"array","items":{"type":"string"},"description":"Only files with these extensions, without the dot."},
                    "context":{"type":"integer","description":"Lines of context around each match, 0-5."},
                    "max_results":{"type":"integer","description":"Stop after this many matching lines. Default 100."},
                    "files_only":{"type":"boolean","description":"Only list the files that match, with counts."},
                    "project":{"type":"string","description":"With several projects open: a project's name, or \"all\", to search there instead of in the one the user is in."}
                },"required":["pattern"]}),
            ),
            spec(
                "find_files",
                "Find files by name, like find. `name` is part of the file name, or `*.ext` for a suffix. Skips build output and version-control folders.",
                json!({"type":"object","properties":{
                    "name":{"type":"string"},
                    "path":{"type":"string","description":"Folder to search; defaults to the project root."},
                    "max_results":{"type":"integer"},
                    "project":{"type":"string","description":"With several projects open: a project's name, or \"all\"."}
                },"required":["name"]}),
            ),
            spec(
                "edit_file",
                "Replace exact text in a file. `old_text` must appear in the file exactly once (include enough surrounding lines to make it unique) unless `expected_count` says how many to replace. The file is read and written in its own encoding. Depending on the user's mode the edit is applied at once or proposed for the user to review; the result says which. Read the file first.",
                json!({"type":"object","properties":{
                    "file":{"type":"string","description":"Absolute path, or relative to the project."},
                    "old_text":{"type":"string"},
                    "new_text":{"type":"string"},
                    "expected_count":{"type":"integer","description":"How many occurrences to replace. Default 1."}
                },"required":["file","old_text","new_text"]}),
            ),
            spec(
                "create_file",
                "Create a new file with the given text. Fails if the file already exists — use edit_file to change one.",
                json!({"type":"object","properties":{
                    "file":{"type":"string","description":"Absolute path, or relative to the project."},
                    "text":{"type":"string"}
                },"required":["file","text"]}),
            ),
            spec(
                "rename_symbol",
                "Rename a symbol — a class, method, field, variable or parameter — and everything that refers to it, across the whole project, using Bennu's reference index (not text replacement). Point at the symbol with the file and the line and column of its name (a declaration or any use). A public type's file is renamed too. Proposed or applied like any edit, as one change. Prefer this to editing names by hand.",
                json!({"type":"object","properties":{
                    "file":{"type":"string","description":"Absolute path, or relative to the project, of a file containing the symbol."},
                    "line":{"type":"integer","description":"1-based line of the symbol's name."},
                    "column":{"type":"integer","description":"1-based column within the line. Default 1; give it when the line has several names."},
                    "new_name":{"type":"string"}
                },"required":["file","line","new_name"]}),
            ),
            spec(
                "add_parameter",
                "Add a parameter to a Java method and update the method, its overriders and every call, using Bennu's reference index. Point at the method's name (its declaration or a call) with file, line and column. Callers receive `default_value` as the argument. Refused, with the reason, when the change is not safe (a method reference, a library override). Proposed or applied as one change.",
                json!({"type":"object","properties":{
                    "file":{"type":"string"},
                    "line":{"type":"integer","description":"1-based line of the method's name."},
                    "column":{"type":"integer","description":"1-based column of the method's name. Default 1."},
                    "type":{"type":"string","description":"The parameter's type as written, e.g. `Currency` or `List<String>`."},
                    "name":{"type":"string"},
                    "default_value":{"type":"string","description":"What existing calls pass. Defaults to the type's zero value (0, false, null)."},
                    "before":{"type":"integer","description":"Index of the parameter it goes before; absent means last."}
                },"required":["file","line","type","name"]}),
            ),
            spec(
                "delete_file",
                "Delete files or folders. The user is always asked first, and what is deleted is kept in Local History so it can be restored.",
                json!({"type":"object","properties":{
                    "paths":{"type":"array","items":{"type":"string"},"description":"Absolute paths, or relative to the project."}
                },"required":["paths"]}),
            ),
            spec(
                "update_plan",
                "Keep a visible plan for work that has three or more steps. Call it at the start with every step as `pending` and the first as `in_progress`, then call it again as you go: mark a step `completed` as soon as it is done and the next `in_progress`. Exactly one step is in progress at a time. Send the whole list each time. Add steps if you find more to do. Call it in the same turn as the other tools you are using, never on its own. Do not use it for a question or a one-step change — the user reads this as progress, so keep each step short and true.",
                json!({"type":"object","properties":{"steps":{"type":"array","minItems":1,"maxItems":12,"items":{"type":"object","properties":{
                    "text":{"type":"string","description":"The step, in a few words."},
                    "status":{"type":"string","enum":["pending","in_progress","completed"]}
                },"required":["text","status"]}}},"required":["steps"]}),
            ),
            spec(
                "bennu_help",
                "Search Bennu's own documentation. Use it whenever the user asks how to do something in Bennu itself — a feature, a shortcut, a setting, where a panel is, how a refactoring or a run configuration works. Pass a few words for what they want to do. It returns the matching sections with the page each is from; answer from them, and if nothing matches say you could not find it rather than guessing how Bennu works.",
                json!({"type":"object","properties":{
                    "query":{"type":"string","description":"A few words: what the user wants to do, e.g. \"rename a symbol\", \"run configuration\", \"split the editor\"."},
                    "limit":{"type":"integer","description":"How many sections, 1-6. Default 4."}
                },"required":["query"]}),
            ),
            spec(
                "ask_user",
                "Ask the user one to four multiple-choice questions when a decision is theirs to make and guessing would be wrong. Each question has 2-4 options; the user can always type their own answer instead. Returns their answers.",
                json!({"type":"object","properties":{"questions":{"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","properties":{
                    "header":{"type":"string","description":"One or two words naming the decision."},
                    "text":{"type":"string","description":"The question."},
                    "options":{"type":"array","items":{"type":"object","properties":{"label":{"type":"string"},"description":{"type":"string"}},"required":["label"]}},
                    "multi":{"type":"boolean","description":"Allow choosing several."}
                },"required":["header","text","options"]}}},"required":["questions"]}),
            ),
        ]
    }

    // ── Gate ─────────────────────────────────────────────────────────────────────────────────────

    /// The paths a call touches and how, for the access policy.
    fn touches(&self, call: &ToolCall) -> Vec<(PathBuf, Op)> {
        let input = &call.input;
        let mut out: Vec<(PathBuf, Op)> = Vec::new();
        let str_of = |k: &str| input.get(k).and_then(Value::as_str).map(|s| self.resolve(s));
        match call.name.as_str() {
            "search_text" | "find_files" => match self.text_targets(input, input.get("path").and_then(Value::as_str)) {
                Ok(targets) => out.extend(targets.into_iter().map(|(_, _, base)| (base, Op::List))),
                // A name that is not a project: the call will say so; here it is held to the project folder.
                Err(_) => out.push((self.project().to_path_buf(), Op::List)),
            },
            "edit_file" | "rename_symbol" | "add_parameter" => out.extend(str_of("file").map(|p| (p, Op::Write))),
            "create_file" => out.extend(str_of("file").map(|p| (p, Op::Create))),
            "delete_file" => {
                for p in input.get("paths").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    out.push((self.resolve(p), Op::Delete));
                }
            }
            "edit_files" => out.extend(self.edit_files_touched(input).into_iter().map(|p| (p, Op::Write))),
            // A tree-wide change needs the right to write across the folder it works in.
            "replace_in_files" => out.push((str_of("path").unwrap_or_else(|| self.project().to_path_buf()), Op::Write)),
            "structural_replace" => out.push((self.project().to_path_buf(), Op::Write)),
            "local_history" => out.extend(self.history_touched(input).into_iter().map(|(p, write)| (p, if write { Op::Write } else { Op::Read }))),
            "ask_user" | "bennu_help" | "update_plan" | "open_ui" => {}
            _ => {
                let op = match self.descriptor(&call.name).map(|d| d.safety) {
                    Some(Safety::Read) | None => Op::Read,
                    Some(_) => Op::Write,
                };
                for (path, role) in catalog_path_args(input) {
                    out.push((self.resolve(&path), if role == PathRole::Folder { Op::List } else { op }));
                }
                // The policy sees what a call is given, not what comes back. A tool that answers about the
                // whole project (usages, callers, the framework catalogue) hands back other files' paths and
                // lines even when it was pointed at one file — so under "this file" it is held to the
                // project folder as well, and asks.
                if self.session.policy().scope == Scope::File && reaches_beyond_its_file(&call.name) {
                    out.push((self.project().to_path_buf(), Op::List));
                }
            }
        }
        out
    }

    /// Ask the access policy about one path, putting the user's answer to any prompt it raises.
    fn authorize(&self, path: &Path, op: Op) -> Result<(), String> {
        // A grant or a yes changes the answer at most twice (read → write upgrade), so the loop is bounded.
        for _ in 0..3 {
            let decision = self.session.policy().check(path, op);
            match decision {
                Decision::Allow => return Ok(()),
                Decision::Deny(why) => return Err(why),
                Decision::Ask(ask) => match ask.grant {
                    Some(folder) => self.ask_access(&folder, path, &ask.message)?,
                    // A confirmation applies to this call only and is not recorded in the policy, so a
                    // "yes" is the answer — looping back would ask the same question again.
                    None => return self.confirm("Allow this?", &ask.message, true),
                },
            }
        }
        Err("access could not be settled".into())
    }

    /// The one prompt for a path outside the allowed scope: no, reading, or reading and writing.
    fn ask_access(&self, folder: &Path, file: &Path, reason: &str) -> Result<(), String> {
        let request_id = format!("ax-{}", folder.display());
        let block = json!({
            "type": "access",
            "request": {
                "id": request_id,
                "folder": folder.display().to_string(),
                "file": file.file_name().map(|n| n.to_string_lossy().to_string()),
                "reason": reason,
            },
        });
        let answer = self.session.ask(block, "/request/id", &request_id, |b, a| {
            b["request"]["answer"] = a.get("answer").cloned().unwrap_or(json!("denied"));
        });
        match answer.as_ref().and_then(|a| a.get("answer")).and_then(Value::as_str) {
            Some("read") => {
                self.session.policy().grant(folder.to_path_buf(), false);
                runs::remember_grants(&self.session.conversation_id, self.session.policy().grants.clone());
                Ok(())
            }
            Some("write") => {
                self.session.policy().grant(folder.to_path_buf(), true);
                runs::remember_grants(&self.session.conversation_id, self.session.policy().grants.clone());
                Ok(())
            }
            _ => Err(format!("the user did not allow access to {}", folder.display())),
        }
    }

    /// A yes/no question. `Err` is the model-facing reason when the answer is no (or the run ended).
    fn confirm(&self, title: &str, detail: &str, danger: bool) -> Result<(), String> {
        let id = unique_id("cf");
        let block = json!({ "type": "confirm", "confirm": { "id": id, "title": title, "detail": detail, "danger": danger } });
        let answer = self.session.ask(block, "/confirm/id", &id, |b, a| {
            b["confirm"]["answer"] = a.get("answer").cloned().unwrap_or(json!("no"));
        });
        match answer.as_ref().and_then(|a| a.get("answer")).and_then(Value::as_str) {
            Some("yes") => Ok(()),
            _ => Err("the user declined".into()),
        }
    }

    // ── Running ──────────────────────────────────────────────────────────────────────────────────

    /// Call a handler of the catalogue in-process.
    fn call_handler(&self, method: &str, params: Value) -> Result<Value, String> {
        let ctx = &self.session.ctx as &dyn std::any::Any;
        if let Some(call) = registry_for("").get(method) {
            return call(ctx, params);
        }
        if let Some(call) = async_registry_for("").get(method) {
            return self.rt.block_on(call(ctx, params));
        }
        Err(format!("unknown tool `{method}`"))
    }

    fn run_catalog(&self, d: &ToolDescriptor, input: &Value) -> Result<String, String> {
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
        let value = self.call_handler(&d.method, d.wrap_arguments(flat))?;
        Ok(render(d.output, &value))
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

    fn run_edit(&self, input: &Value) -> Result<String, String> {
        let s = |k: &str| input.get(k).and_then(Value::as_str).ok_or_else(|| format!("`{k}` is required"));
        let file = self.resolve(s("file")?);
        let (old_text, new_text) = (s("old_text")?, s("new_text")?);
        let expected = input.get("expected_count").and_then(Value::as_u64).map(|n| n as usize);

        let (text, stamp) = self.read_text(&file)?;
        let next = text_tools::replace(&text, old_text, new_text, expected)?;
        self.submit(&file, &text, next, Some(stamp), None)
    }

    fn run_create(&self, input: &Value) -> Result<String, String> {
        let file = self.resolve(input.get("file").and_then(Value::as_str).ok_or("`file` is required")?);
        let text = input.get("text").and_then(Value::as_str).ok_or("`text` is required")?;
        if file.exists() {
            return Err(format!("{} already exists — use edit_file to change it", file.display()));
        }
        if let Some(parent) = file.parent() {
            // A proposal must not create folders before the user has agreed to the file.
            if self.session.auto {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        self.submit(&file, "", text.to_string(), None, None)
    }

    /// Apply an edit or hold it for review, by mode.
    fn submit(&self, file: &Path, old: &str, new: String, stamp: Option<String>, move_to: Option<String>) -> Result<String, String> {
        let edit = PendingEdit { root: self.root_of(file), file: file.display().to_string(), new_text: new.clone(), stamp, move_to };
        let delta = compare_text(old, &new, 3);
        if delta.identical {
            return Err("the edit changes nothing".into());
        }
        if self.session.auto {
            self.write_text(&edit)?;
            if let Some(to) = &edit.move_to {
                self.call_handler("bennu_rename_path", json!({"args": {"root": edit.root, "file": edit.file, "new_path": to}}))?;
            }
            self.record_applied(file, old, &new);
            return Ok(format!("applied: {} (+{} −{})", file.display(), delta.added, delta.removed));
        }
        self.propose(edit, old);
        Ok(format!(
            "proposed, not applied: {} (+{} −{}). The user reviews proposed changes themselves; do not assume this one is in the file until a later message says so.",
            file.display(), delta.added, delta.removed
        ))
    }

    /// How a file is named in a card or a receipt: relative to the project the user is in, `name:path` in
    /// another of the projects in play, as it is otherwise.
    fn display_path(&self, file: &str) -> String {
        if let Ok(rel) = Path::new(file).strip_prefix(self.project()) {
            return rel.to_string_lossy().replace('\\', "/");
        }
        projects::labelled(&self.session.projects, Path::new(file)).unwrap_or_else(|| file.to_string())
    }

    fn file_entry(&self, file: &str, old: &str, new: &str) -> Value {
        let delta = compare_text(old, new, 3);
        json!({ "path": self.display_path(file), "added": delta.added, "removed": delta.removed, "hunks": delta.hunks })
    }

    /// An edit applied in auto mode still leaves a card: what changed stays visible in the thread.
    fn record_applied(&self, file: &Path, old: &str, new: &str) {
        let id = unique_id("pa");
        let entry = self.file_entry(&file.display().to_string(), old, new);
        self.session.view().push(json!({ "type": "proposal", "proposal": { "id": id, "status": "applied", "files": [entry] } }));
        self.session.push_message();
    }

    /// Gather a proposed edit into this run's proposal card.
    fn propose(&self, edit: PendingEdit, old: &str) {
        let mut entry = self.file_entry(&edit.file, old, &edit.new_text);
        if let Some(to) = &edit.move_to {
            // The card says the file moves too, not only that its text changes.
            let name = Path::new(to).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| to.clone());
            entry["path"] = json!(format!("{} → {name}", self.display_path(&edit.file)));
        }
        let existing = self.session.proposal_id.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let id = match existing {
            Some(id) => id,
            None => {
                let id = format!("pr-{}", self.session.run_id);
                *self.session.proposal_id.lock().unwrap_or_else(|p| p.into_inner()) = Some(id.clone());
                self.session.view().push(json!({
                    "type": "proposal",
                    "proposal": { "id": id, "status": "pending", "files": [], "conversationId": self.session.conversation_id },
                }));
                id
            }
        };
        self.session.view().update("/proposal/id", &id, |b| {
            if let Some(files) = b["proposal"]["files"].as_array_mut() {
                // The same file edited twice replaces its entry: the card shows the file's final change.
                files.retain(|f| f["path"] != entry["path"]);
                files.push(entry.clone());
            }
        });
        let mut all = runs::take_proposal(&id).unwrap_or_default();
        all.retain(|e| e.file != edit.file);
        all.push(edit);
        runs::store_proposal(&id, all);
        self.session.push_message();
    }

    /// The position a refactoring tool was pointed at, as a byte offset into the file's text.
    fn position(&self, input: &Value) -> Result<(PathBuf, String, String, usize), String> {
        let raw = input.get("file").and_then(Value::as_str).ok_or("`file` is required")?;
        let file = self.resolve(raw);
        let line = input.get("line").and_then(Value::as_u64).ok_or("`line` is required")? as usize;
        let column = input.get("column").and_then(Value::as_u64).unwrap_or(1) as usize;
        let (text, stamp) = self.read_text(&file)?;
        let offset = text_tools::byte_offset(&text, line, column)?;
        Ok((file, text, stamp, offset))
    }

    fn run_rename(&self, input: &Value) -> Result<String, String> {
        let new_name = input.get("new_name").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or("`new_name` is required")?;
        let (file, text, _stamp, offset) = self.position(input)?;
        let applied = self.call_handler(
            "bennu_rename_apply",
            json!({"args": {"file": file.display().to_string(), "source": text, "offset": offset, "new_name": new_name}}),
        )?;
        let edits = edits_of(&applied);
        if edits.is_empty() {
            return Err("nothing to rename there — the position is not on a renameable name, or the project index is still building. Check the line and column against the file.".into());
        }
        let moved = match (applied.pointer("/file_rename/from").and_then(Value::as_str), applied.pointer("/file_rename/to").and_then(Value::as_str)) {
            (Some(from), Some(to)) => Some((from.to_string(), to.to_string())),
            _ => None,
        };
        self.apply_plan(edits, moved, &format!("rename to {new_name}"))
    }

    fn run_add_parameter(&self, input: &Value) -> Result<String, String> {
        let s = |k: &str| input.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).ok_or_else(|| format!("`{k}` is required"));
        let (type_text, name) = (s("type")?, s("name")?);
        let (file, text, _stamp, offset) = self.position(input)?;
        if file.extension().map(|e| !e.eq_ignore_ascii_case("java")).unwrap_or(true) {
            return Err("add_parameter works on Java methods only. For this file use edit_files, or rename_symbol to rename.".into());
        }
        let mut args = json!({
            "file": file.display().to_string(), "source": text, "offset": offset,
            "type_text": type_text, "name": name,
        });
        if let Some(v) = input.get("default_value").and_then(Value::as_str) {
            args["default_value"] = json!(v);
        }
        if let Some(b) = input.get("before").and_then(Value::as_u64) {
            args["before"] = json!(b);
        }
        let plan = self.call_handler("bennu_add_parameter_plan", json!({ "args": args }))?;
        if plan.is_null() {
            return Err("no method of this project is declared or called there, or the project index is still building. Check the line and column.".into());
        }
        if let Some(why) = plan.pointer("/preview/blocked").and_then(Value::as_str) {
            return Err(format!("not safe to do: {why}"));
        }
        let edits = edits_of(&plan["preview"]);
        if edits.is_empty() {
            return Err("the plan has no edits".into());
        }
        self.apply_plan(edits, None, &format!("add parameter {type_text} {name}"))
    }

    /// Apply a refactoring plan — spans grouped by file — as one change: every file the plan touches is
    /// checked against the access policy first, every file's new text is computed before any is
    /// written, and then each goes through the same door as an `edit_file` (proposed or applied by mode).
    fn apply_plan(&self, edits: Vec<(String, Span)>, file_move: Option<(String, String)>, what: &str) -> Result<String, String> {
        let mut by_file: Vec<(String, Vec<Span>)> = Vec::new();
        for (file, span) in edits {
            match by_file.iter_mut().find(|(f, _)| *f == file) {
                Some((_, spans)) => spans.push(span),
                None => by_file.push((file, vec![span])),
            }
        }
        // All of it or none of it: a refactoring that lands in half the files is a broken build.
        for (file, _) in &by_file {
            self.authorize(&self.resolve(file), Op::Write)?;
        }
        let mut prepared = Vec::new();
        for (file, spans) in &by_file {
            let (old, stamp) = self.read_text(&self.resolve(file))?;
            let new = text_tools::apply_spans(&old, spans).map_err(|e| format!("{file}: {e}"))?;
            prepared.push((file.clone(), old, new, stamp));
        }
        let mut lines = Vec::new();
        for (file, old, new, stamp) in prepared {
            // A public type's file moves with it: the move rides on the edit of the file it applies to.
            let moved = file_move.as_ref().filter(|(from, _)| same_path(from, &file)).map(|(_, to)| to.clone());
            lines.push(self.submit(&self.resolve(&file), &old, new, Some(stamp), moved)?);
        }
        Ok(format!("{what}: {} file(s).\n{}", lines.len(), lines.join("\n")))
    }

    fn run_delete(&self, input: &Value) -> Result<String, String> {
        let paths: Vec<String> = input
            .get("paths")
            .and_then(Value::as_array)
            .ok_or("`paths` is required")?
            .iter()
            .filter_map(Value::as_str)
            .map(|p| self.resolve(p).display().to_string())
            .collect();
        if paths.is_empty() {
            return Err("no paths given".into());
        }
        // Each project records its own history, so the paths go to the project that owns them.
        let mut by_root: Vec<(String, Vec<String>)> = Vec::new();
        for p in paths {
            let root = self.root_of(Path::new(&p));
            match by_root.iter_mut().find(|(r, _)| *r == root) {
                Some((_, list)) => list.push(p),
                None => by_root.push((root, vec![p])),
            }
        }
        let mut results = Vec::new();
        for (root, list) in by_root {
            results.push(self.call_handler("bennu_delete_paths", json!({"args": {"root": root, "paths": list}}))?.to_string());
        }
        Ok(format!("deleted (kept in Local History, restorable): {}", results.join(", ")))
    }

    /// Record the plan: one card in the message, replaced whole each time the model sends the list.
    fn run_plan(&self, input: &Value) -> Result<String, String> {
        let steps = parse_plan(input)?;
        let (done, total) = (steps.iter().filter(|s| s["status"] == "completed").count(), steps.len());
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

/// The `edits` array of a rename-apply or add-parameter reply, as spans grouped later by file.
fn edits_of(value: &Value) -> Vec<(String, Span)> {
    // A rename-apply reply has `edits`; an add-parameter plan has them nested per file.
    let flat: Vec<&Value> = match value.get("edits").and_then(Value::as_array) {
        Some(list) => list.iter().collect(),
        None => value
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|f| f.get("edits").and_then(Value::as_array).into_iter().flatten())
            .collect(),
    };
    flat.into_iter()
        .filter_map(|e| {
            Some((
                e.get("file")?.as_str()?.to_string(),
                Span {
                    start: e.get("start")?.as_u64()? as usize,
                    end: e.get("end")?.as_u64()? as usize,
                    old: e.get("old").and_then(Value::as_str).unwrap_or("").to_string(),
                    new_text: e.get("new_text")?.as_str()?.to_string(),
                },
            ))
        })
        .collect()
}

/// Whether two paths name the same file, whatever the separators and (on Windows) the case.
fn same_path(a: &str, b: &str) -> bool {
    a.replace('\\', "/").eq_ignore_ascii_case(&b.replace('\\', "/"))
}

/// The catalogue tools that look at one file and say nothing about the rest of the project.
const FILE_LOCAL_TOOLS: [&str; 4] = ["bennu_read_file", "bennu_outline", "bennu_check_file", "bennu_symbol_at"];

/// Whether a catalogue tool's answer can include things from outside the file it was pointed at.
fn reaches_beyond_its_file(tool: &str) -> bool {
    !FILE_LOCAL_TOOLS.contains(&tool)
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

/// What a path argument of a catalogue tool names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathRole {
    File,
    Folder,
}

/// The paths a catalogue tool's arguments name, for the access policy.
///
/// `root` is the project the tool works in — it is how a tool finds the project's encoding and index, not
/// something it reads — so it only counts when the call names nothing narrower. A call that reads one
/// allowed file must not be refused because that file lives in a project the user did not open up; a call
/// that has only a `root` is a project-wide question, and is held to the reach as one.
fn catalog_path_args(input: &Value) -> Vec<(String, PathRole)> {
    let mut out: Vec<(String, PathRole)> = Vec::new();
    let text = |k: &str| input.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    for key in ["file", "path", "new_path"] {
        out.extend(text(key).map(|p| (p, PathRole::File)));
    }
    out.extend(text("dir").map(|p| (p, PathRole::Folder)));
    for key in ["files", "paths"] {
        for p in input.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            out.push((p.to_string(), PathRole::File));
        }
    }
    if out.is_empty() {
        out.extend(text("root").map(|p| (p, PathRole::Folder)));
    }
    out
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

impl Host for ToolHost {
    fn tools(&self) -> Vec<ToolSpec> {
        // A chat reads and changes nothing, but it can still look up how Bennu works: that is a built-in
        // text, not the project.
        if self.chat {
            return all_specs().into_iter().filter(|t| t.name == "bennu_help").collect();
        }
        offered_specs(self.plan, &self.off, &self.git)
    }

    fn prepare(&self, request: &mut Request) {
        self.compact_if_needed(request);
        self.enforce_limits();
    }

    fn gate(&self, call: &ToolCall) -> Gate {
        if call.name == "update_plan" && !self.plan {
            return Gate::Refuse("the step plan is switched off".into());
        }
        if self.chat && call.name != "bennu_help" {
            return Gate::Refuse("this is a chat: the only tool is the help about Bennu".into());
        }
        if self.off.hides(&call.name) {
            return Gate::Refuse("the user has switched that kind of tool off".into());
        }
        let known = self.descriptor(&call.name).is_some()
            || matches!(
                call.name.as_str(),
                "search_text" | "find_files" | "edit_file" | "edit_files" | "replace_in_files" | "create_file" | "delete_file" | "ask_user" | "rename_symbol"
                    | "add_parameter" | "bennu_help" | "update_plan" | "local_history" | "structural_replace" | "open_ui"
            );
        if !known {
            return Gate::Refuse(format!("`{}` is not a tool you have", call.name));
        }
        for (path, op) in self.touches(call) {
            if let Err(why) = self.authorize(&path, op) {
                return Gate::Refuse(why);
            }
        }
        // A catalogue tool that changes things, or runs project code, asks first (writes only in manual mode).
        if let Some(d) = self.descriptor(&call.name) {
            let must_ask = match d.safety {
                Safety::Read => false,
                Safety::Write => !self.session.auto || git::always_asks(&d.name),
                Safety::Destructive => true,
            };
            if must_ask {
                if let Err(why) = self.confirm(&format!("Run {}?", d.title), &d.description.lines().next().unwrap_or(""), d.safety == Safety::Destructive) {
                    return Gate::Refuse(why);
                }
            }
        }
        Gate::Run
    }

    fn run(&self, call: &ToolCall) -> Result<String, String> {
        match call.name.as_str() {
            "search_text" => self.run_search(&call.input),
            "find_files" => self.run_find(&call.input),
            "edit_file" => self.run_edit(&call.input),
            "edit_files" => self.run_edit_files(&call.input),
            "replace_in_files" => self.run_replace_in_files(&call.input),
            "local_history" => self.run_history(&call.input),
            "structural_replace" => self.run_structural(&call.input),
            "open_ui" => self.run_open_ui(&call.input),
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

    fn summarize(&self, _call: &ToolCall, result: &Result<String, String>) -> String {
        detail_of(result)
    }

    fn event(&self, event: AgentEvent) {
        let s = &self.session;
        match event {
            AgentEvent::Text(t) => {
                self.status("writing", None, self.iteration());
                s.view().text(&t);
                s.push_message_soon();
            }
            AgentEvent::ToolDone { id, name, input, summary, ok } => {
                // The questions, the proposals and the access cards are the record of those calls.
                if !matches!(name.as_str(), "ask_user" | "update_plan") {
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

/// Apply or drop a proposal the user decided on. Returns the files written.
pub(super) fn decide(proposal_id: &str, apply: bool, call: impl Fn(&str, Value) -> Result<Value, String>) -> Result<Vec<String>, String> {
    let edits = runs::take_proposal(proposal_id).ok_or("this proposal was already decided, or its session ended")?;
    if !apply {
        return Ok(Vec::new());
    }
    let mut written = Vec::new();
    for e in &edits {
        call(
            "bennu_write_file",
            json!({"args": {"root": e.root, "file": e.file, "text": e.new_text, "expect_stamp": e.stamp.clone().unwrap_or_default()}}),
        )
        .map_err(|err| format!("{}: {err}", e.file))?;
        if let Some(to) = &e.move_to {
            call("bennu_rename_path", json!({"args": {"root": e.root, "file": e.file, "new_path": to}}))
                .map_err(|err| format!("{}: could not be moved to {to}: {err}", e.file))?;
        }
        written.push(e.file.clone());
    }
    Ok(written)
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
    fn under_this_file_only_the_file_local_tools_stay_inside_it() {
        for local in ["bennu_read_file", "bennu_outline", "bennu_check_file", "bennu_symbol_at"] {
            assert!(!reaches_beyond_its_file(local), "{local}");
        }
        for wide in ["bennu_references", "bennu_callers", "bennu_catalog", "bennu_rename_plan", "bennu_problems", "bennu_frameworks"] {
            assert!(reaches_beyond_its_file(wide), "{wide}");
        }
    }

    #[test]
    fn a_call_that_names_a_file_is_not_held_to_its_project_folder() {
        let got = catalog_path_args(&json!({"root": "C:/p", "file": "C:/p/src/A.java"}));
        assert_eq!(got, vec![("C:/p/src/A.java".to_string(), PathRole::File)]);
    }

    #[test]
    fn a_call_with_only_a_root_is_a_project_wide_question() {
        let got = catalog_path_args(&json!({"root": "C:/p", "symbol": "Order"}));
        assert_eq!(got, vec![("C:/p".to_string(), PathRole::Folder)]);
    }

    #[test]
    fn every_path_a_call_names_is_collected() {
        let got = catalog_path_args(&json!({"files": ["a", "b"], "dir": "d", "path": ""}));
        assert_eq!(got.len(), 3, "the empty path is ignored: {got:?}");
        assert!(got.contains(&("d".to_string(), PathRole::Folder)));
    }

    #[test]
    fn a_long_result_is_cut_and_says_so() {
        let long = "x".repeat(MAX_RESULT_CHARS + 10);
        let out = clip(long);
        assert!(out.contains("cut at") && out.chars().count() < MAX_RESULT_CHARS + 200);
        assert_eq!(clip("short".into()), "short");
    }

    #[test]
    fn a_text_result_is_not_wrapped_in_quotes() {
        assert_eq!(render(ToolOutput::Text, &json!("hello")), "hello");
        assert_eq!(render(ToolOutput::Json, &json!({"a": 1})), "{\"a\":1}");
    }
}
