//! Live runs as tools: start the project's services, read what they print, call them over loopback
//! HTTP, follow one request through every log, and run scenarios — the model's way of *seeing the
//! program run* instead of guessing from the source.
//!
//! The machinery is the `bennu-live` crate and `crate::live` (the same state the Live panel shows).
//! This file is the model-facing layer: specs, the approval rules, and the text the model reads.
//!
//! ## What asks the user
//!
//! * `run_start` — always: it executes a command line.
//! * `run_recipe` / `run_scenario` — once per recipes-file *content*: approving covers the commands in
//!   it until the file changes (see `Approvals`).
//! * `http_request` — a write method (POST, PUT, ...) to a port none of our processes declared.
//! * Everything that only reads or stops what the run itself started does not ask.

use std::path::PathBuf;
use std::sync::Arc;

use bennu_assistant::prelude::{Op, ToolSpec};
use bennu_live::prelude::{
    parse_scenario, run_scenario, trace, Event, EventKind, Filter, HttpRequest, Outcome, Phase, ProcInfo, RawReady, Readiness, StartSpec,
};
use regex::Regex;
use serde_json::{json, Value};

use super::{clip, spec, ToolHost};
use crate::live::files::{read_recipes, read_scenario, read_scenarios, scenario_path, Recipes};
use crate::live::world::{approve_recipes_for, approved_recipe, foreign_holders, held_text, recipes_approved, run_recipe, stop_recipe, LiveWorld};
use crate::live::{conversation_key, live_for, owner_label, Live};

mod logs;

const NAMES: [&str; 13] = [
    "run_start",
    "run_status",
    "run_logs",
    "run_stop",
    "wait_ready",
    "list_recipes",
    "run_recipe",
    "http_request",
    "trace_request",
    "timeline",
    "list_scenarios",
    "run_scenario",
    "save_scenario",
];

/// Whether `name` is one of this family's tools.
pub(super) fn is_live_tool(name: &str) -> bool {
    NAMES.contains(&name) || logs::NAMES.contains(&name)
}

/// The most response body shown to the model.
const MAX_BODY_CHARS: usize = 12_000;

pub(super) fn specs() -> Vec<ToolSpec> {
    let ready = json!({"type":"object","description":"When is it ready; every given check must hold.","properties":{
        "port":{"type":"integer","description":"A loopback port that accepts connections."},
        "log":{"type":"string","description":"Regex a log line must match, e.g. \"Started .*Application\"."},
        "http":{"type":"string","description":"A http://localhost URL that must answer."},
        "status":{"type":"integer","description":"With `http`: the exact status (default: any 2xx)."},
        "delay_ms":{"type":"integer"},
        "timeout_s":{"type":"integer","description":"Give up after this long. Default 60, at most 600."}
    }});
    vec![
        spec(
            "run_start",
            "Start a long-running process (a server, a watcher) on this machine and return its id at once. The user is asked to approve every start. Output is kept (ring buffer) and read with run_logs; `ready` says when it is up (see wait_ready); run_stop ends it with its whole process tree. The command is a shell line run in `cwd` (inside the project). Everything is stopped when the project closes. For a stack of several services prefer a recipe (list_recipes, run_recipe).",
            json!({"type":"object","properties":{
                "name":{"type":"string","description":"A short name, unique among running processes: `api`, `web`."},
                "command":{"type":"string","description":"The shell command line, e.g. `mvn -q spring-boot:run` or `npm run dev`."},
                "cwd":{"type":"string","description":"Folder relative to the project. Default: the project root."},
                "env":{"type":"object","additionalProperties":{"type":"string"}},
                "ports":{"type":"array","items":{"type":"integer"},"description":"Ports it will listen on: refused up front if already taken."},
                "ready":ready.clone(),
                "wait":{"type":"boolean","description":"Wait until ready before returning (needs `ready`). Default false."}
            },"required":["name","command"]}),
        ),
        spec(
            "run_status",
            "Status of the managed processes: running / ready / exited with a code / stopped, uptime, ports. Without `id`, all of them.",
            json!({"type":"object","properties":{"id":{"type":"string","description":"A process id (`p1`) or name."}}}),
        ),
        spec(
            "run_logs",
            "Read a managed process's output (stdout and stderr, timestamped, ANSI colours removed) as RECORDS: a log line with its stack trace attached. Start with `summary` (counts per level, the distinct errors, the last error) or log_facets, then narrow with `level`, `pattern`, `last`/`after`/`before` and `context`; identical consecutive records are folded (x12). Defaults to the newest 80 records. `since` takes the `next_since` of a previous call to get only what is new (or use wait_log to wait for a line). Every answer lists what it left out and how to ask for it.",
            logs::schema_with_filters(
                json!({
                    "id":{"type":"string","description":"A process id (`p1`) or name."},
                    "since":{"type":"integer","description":"Only lines after this sequence number (a previous `next_since`)."},
                    "stream":{"type":"string","enum":["stdout","stderr"]}
                }),
                &["id"],
            ),
        ),
        spec(
            "run_stop",
            "Stop a managed process (politely, then the whole process tree), all the processes of a recipe, or every process of yours. Needs no approval. It only ever stops what this conversation started: another conversation's or the user's services keep running and are refused.",
            json!({"type":"object","properties":{
                "id":{"type":"string","description":"A process id or name."},
                "recipe":{"type":"string","description":"Stop this recipe's services in its stop order."},
                "all":{"type":"boolean"}
            }}),
        ),
        spec(
            "wait_ready",
            "Wait until a managed process is ready: by the `ready` it was started with, or the checks given here (port open, log line, health URL, delay). Returns how long it took; if the process exits first or the timeout passes, returns the process's last output.",
            json!({"type":"object","properties":{
                "id":{"type":"string"},
                "port":ready["properties"]["port"].clone(),
                "log":ready["properties"]["log"].clone(),
                "http":ready["properties"]["http"].clone(),
                "status":ready["properties"]["status"].clone(),
                "timeout_s":ready["properties"]["timeout_s"].clone()
            },"required":["id"]}),
        ),
        spec(
            "list_recipes",
            "List the project's recipes: named stacks of services (command, cwd, ports, readiness, dependencies) from `.arbor/bennu/live/recipes.toml`, and whether the user has approved the file. A file with errors is reported with them.",
            json!({"type":"object","properties":{}}),
        ),
        spec(
            "run_recipe",
            "Start a recipe's services in dependency order, waiting for each to be ready before the next. The user approves a recipes file once; editing it asks again. Running it twice reuses services already up. If one fails to come up, the ones this call started are stopped and its last output is returned.",
            json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}),
        ),
        spec(
            "http_request",
            "Send an HTTP request to a service on THIS machine (localhost, 127.0.0.1 or [::1], plain http). An `X-Correlation-Id` header is added so the request can be followed through the services' logs with trace_request. Returns status, headers (secrets redacted), the body (cut when long) and the correlation id. A POST/PUT/PATCH/DELETE to a port none of the started processes declared asks the user first.",
            json!({"type":"object","properties":{
                "method":{"type":"string","description":"Default GET."},
                "url":{"type":"string","description":"http://localhost:8080/api/orders"},
                "headers":{"type":"object","additionalProperties":{"type":"string"}},
                "body":{"type":"string","description":"The raw request body."},
                "json":{"description":"A JSON value sent as the body (sets Content-Type)."},
                "timeout_ms":{"type":"integer","description":"Default 10000, at most 120000."}
            },"required":["url"]}),
        ),
        spec(
            "trace_request",
            "Everything one correlation id touched, in time order: the request and response, and every log line of every managed process that mentions the id (with the stack-trace lines that follow). Use it to explain a failure across services: which service saw the call, what it logged, what it sent on.",
            json!({"type":"object","properties":{
                "corr":{"type":"string","description":"The correlation id http_request returned."},
                "context":{"type":"integer","description":"Continuation lines kept after a matching log line (stack frames). Default 8."}
            },"required":["corr"]}),
        ),
        spec(
            "timeline",
            "The unified timeline of the live session: process output, HTTP requests and responses, process start/ready/exit — one chronological list. Filter by text, process, kind or correlation id; `since_seq` takes the `last_seq` of a previous call.",
            json!({"type":"object","properties":{
                "since_seq":{"type":"integer"},
                "limit":{"type":"integer","description":"The newest N. Default 60, at most 300."},
                "source":{"type":"string","description":"A process name, or `http`."},
                "text":{"type":"string","description":"A substring (case-insensitive)."},
                "kinds":{"type":"array","items":{"type":"string","enum":["log","http","marker"]}},
                "corr":{"type":"string"}
            }}),
        ),
        spec(
            "list_scenarios",
            "List the project's scenarios (`.arbor/bennu/live/scenarios/*.toml`): ordered steps with assertions — start a recipe, HTTP request with status/JSON/body/time checks, wait, log present/absent, process running. Shows the last result of each.",
            json!({"type":"object","properties":{}}),
        ),
        spec(
            "run_scenario",
            "Run a scenario and return per-step pass/fail with the failed assertions and, for a failing HTTP step, its correlation id (feed it to trace_request). The user approves a scenario once per content (and the recipes it starts). Stops at the first failing step unless the step says continue_on_fail.",
            json!({"type":"object","properties":{"name":{"type":"string","description":"The scenario's file name without .toml."}},"required":["name"]}),
        ),
        spec(
            "save_scenario",
            "Create or replace a scenario file from TOML text. The text is validated first; errors are returned so you can fix them. The file goes through the same review as any edit. Format: `name`, `description`, then `[[step]]` tables with `type` = start_recipe | stop_recipe | stop_all | http | wait | assert_log | assert_running. http steps: method, url, headers, body, expect_status, expect_body (regex), max_ms, `[[step.expect_json]]` {path, exists|equals|matches|gt|lt}, and later steps can use `${stepname:$.json.path}` of a saved response. assert_log: process, pattern, present, window_ms, timeout_ms.",
            json!({"type":"object","properties":{
                "name":{"type":"string","description":"File name without .toml: letters, digits, - _ ."},
                "content":{"type":"string","description":"The scenario TOML."}
            },"required":["name","content"]}),
        ),
    ]
    .into_iter()
    .chain(logs::specs())
    .collect()
}

// ── Formatting ───────────────────────────────────────────────────────────────────────────────────

/// `hh:mm:ss.mmm` (UTC) of an epoch-milliseconds time: one clock for every tool, so lines from
/// different tools can be ordered by eye.
fn clock(ms: u64) -> String {
    let day = ms % 86_400_000;
    format!("{:02}:{:02}:{:02}.{:03}", day / 3_600_000, (day / 60_000) % 60, (day / 1000) % 60, day % 1000)
}

fn duration_text(ms: u64) -> String {
    match ms {
        0..=999 => format!("{ms} ms"),
        1_000..=59_999 => format!("{:.1} s", ms as f64 / 1000.0),
        _ => format!("{} min {} s", ms / 60_000, (ms / 1000) % 60),
    }
}

fn state_word(p: &ProcInfo) -> String {
    match p.phase {
        Phase::Ready => "ready".into(),
        Phase::Running => "running (not ready yet)".into(),
        Phase::Exited => format!("exited with code {}", p.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "?".into())),
        Phase::Stopped => format!("stopped{}", p.note.as_deref().map(|n| format!(" — {n}")).unwrap_or_default()),
    }
}

fn proc_line(p: &ProcInfo, me: &str) -> String {
    let up = p.ended_ms.unwrap_or_else(bennu_live::prelude::now_ms).saturating_sub(p.started_ms);
    let ports = if p.ports.is_empty() { String::new() } else { format!(" ports {}", p.ports.iter().map(u16::to_string).collect::<Vec<_>>().join(",")) };
    let group = p.group.as_deref().map(|g| format!(" recipe {g}")).unwrap_or_default();
    format!("{} `{}` — {} — {}{}{} — {} log lines{}", p.id, p.name, state_word(p), duration_text(up), ports, group, p.log_lines, owner_note(p, me))
}

/// Whose process it is, from the point of view of conversation `me`: empty for its own, and for another
/// owner's a reminder that it can be read but not stopped.
fn owner_note(p: &ProcInfo, me: &str) -> String {
    if p.owner.as_deref() == Some(me) {
        return String::new();
    }
    format!(" — held by {} (read-only for you)", owner_label(p.owner.as_deref()))
}

fn event_line(e: &Event) -> String {
    let who = match e.kind {
        EventKind::Log => e.source.clone(),
        EventKind::HttpRequest => "http →".to_string(),
        EventKind::HttpResponse => "http ←".to_string(),
        _ => format!("{} •", e.source),
    };
    let corr = e.corr.as_deref().map(|c| format!(" [{c}]")).unwrap_or_default();
    format!("#{} {} {}{}: {}", e.seq, clock(e.ts_ms), who, corr, e.text)
}

fn events_text(events: &[Event]) -> String {
    events.iter().map(event_line).collect::<Vec<_>>().join("\n")
}

fn readiness_from(v: &Value) -> Result<Readiness, String> {
    let raw: RawReady = serde_json::from_value(v.clone()).map_err(|e| format!("`ready` is not valid: {e}"))?;
    let mut errors = Vec::new();
    raw.validate("this process", &mut errors);
    if errors.is_empty() { Ok(raw.to_readiness()) } else { Err(errors.join("; ")) }
}

fn str_arg<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
    input.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| format!("`{key}` is required"))
}

// ── The tools ────────────────────────────────────────────────────────────────────────────────────

impl ToolHost {
    fn live(&self) -> Arc<Live> {
        live_for(&self.session.root, &self.session.sink)
    }

    fn live_cancelled(&self) -> impl Fn() -> bool + '_ {
        move || self.session.cancelled()
    }

    /// This conversation's id: the owner recorded on every process it starts.
    fn me(&self) -> &str {
        &self.session.conversation_id
    }

    /// Ports declared by this conversation's own running processes: the only ones a state-changing request
    /// goes to without asking (another chat's service is not ours to modify unasked).
    fn own_ports(&self) -> Vec<u16> {
        let me = self.me();
        self.live().manager.list().into_iter().filter(|p| !p.phase.is_finished() && p.owner.as_deref() == Some(me)).flat_map(|p| p.ports).collect()
    }

    /// Refuse to take a name or a port another owner holds. A process of ours with that name is left to
    /// the manager's own "already running" message.
    fn refuse_if_held(&self, live: &Live, name: &str, ports: &[u16]) -> Result<(), String> {
        let me = Some(self.me());
        if let Some(h) = live.manager.holder_of_name(name).filter(|h| h.owner.as_deref() != me) {
            return Err(held_text(&h, &format!("the name `{name}`")));
        }
        for port in ports {
            if let Some(h) = live.manager.holder_of_port(*port).filter(|h| h.owner.as_deref() != me) {
                return Err(held_text(&h, &format!("port {port}")));
            }
        }
        Ok(())
    }

    /// The paths a live call touches, for the access policy: running things is held to the project folder.
    pub(super) fn live_touched(&self, tool: &str, input: &Value) -> Vec<(PathBuf, Op)> {
        match tool {
            "run_start" | "run_recipe" | "run_scenario" => vec![(self.project().to_path_buf(), Op::List)],
            n if logs::NAMES.contains(&n) => self.log_touched(n, input),
            "save_scenario" => match input.get("name").and_then(Value::as_str).map(|n| scenario_path(&self.session.root, n)) {
                Some(Ok(p)) => vec![(p, Op::Write)],
                _ => vec![(self.project().to_path_buf(), Op::Write)],
            },
            _ => Vec::new(),
        }
    }

    /// The questions a live call raises before it runs. `Err` carries the reason it was refused.
    pub(super) fn live_gate(&self, tool: &str, input: &Value) -> Result<(), String> {
        let root = &self.session.root;
        match tool {
            "run_start" => {
                let (name, command) = (str_arg(input, "name")?, str_arg(input, "command")?);
                let cwd = input.get("cwd").and_then(Value::as_str).unwrap_or(".");
                self.confirm("Start a process?", &format!("{name}\n$ {command}\nin {cwd}{}", ports_note(input)), true)
            }
            "run_recipe" => {
                let name = str_arg(input, "name")?;
                self.approve_recipes_if_needed(Some(name))
            }
            "run_scenario" => {
                let name = str_arg(input, "name")?;
                let entry = read_scenario(root, name)?;
                let Ok(scenario) = &entry.scenario else { return Ok(()) }; // run() reports the errors
                let recipes: Vec<&str> = scenario
                    .steps
                    .iter()
                    .filter_map(|s| match &s.kind {
                        bennu_live::prelude::StepKind::StartRecipe { recipe } => Some(recipe.as_str()),
                        _ => None,
                    })
                    .collect();
                if let Some(first) = recipes.first() {
                    self.approve_recipes_if_needed(Some(first))?;
                }
                let key = format!("{}#scenario:{name}", conversation_key(root, self.me()));
                if !crate::live::approvals().is_recipe_approved(&key, &entry.hash) {
                    let steps = scenario.steps.iter().enumerate().map(|(i, s)| format!("{}. {}", i + 1, s.label())).collect::<Vec<_>>().join("\n");
                    self.confirm(&format!("Run scenario `{name}`?"), &steps, false)?;
                    crate::live::approvals().approve_recipes(&key, &entry.hash);
                }
                Ok(())
            }
            "http_request" => {
                let url = str_arg(input, "url")?;
                let target = bennu_live::prelude::parse_local_url(url)?;
                let method = input.get("method").and_then(Value::as_str).unwrap_or("GET").to_ascii_uppercase();
                let safe = matches!(method.as_str(), "GET" | "HEAD" | "OPTIONS");
                if !safe && !self.own_ports().contains(&target.port) {
                    self.confirm("Send a request that changes things?", &format!("{method} {url}\nNo process started here declared port {}.", target.port), true)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Ask the user to approve the recipes file when its current content has not been approved yet.
    fn approve_recipes_if_needed(&self, which: Option<&str>) -> Result<(), String> {
        let root = &self.session.root;
        let Recipes::Valid(file) = read_recipes(root) else { return Ok(()) }; // run() explains a missing or invalid file
        if recipes_approved(root, Some(self.me()), &file.hash) {
            return Ok(());
        }
        let shown: Vec<&bennu_live::prelude::Recipe> = file.recipes.iter().filter(|r| which.is_none_or(|w| r.name.eq_ignore_ascii_case(w))).collect();
        let detail = shown
            .iter()
            .flat_map(|r| r.start_order().into_iter().map(|s| format!("{}: $ {}  (in {})", s.name, s.command, s.cwd)))
            .collect::<Vec<_>>()
            .join("\n");
        self.confirm("Run the commands of this recipes file?", &format!("{detail}\n\nApproving covers the whole recipes file until it changes."), true)?;
        approve_recipes_for(root, Some(self.me()), &file.hash);
        Ok(())
    }

    pub(super) fn run_live(&self, tool: &str, input: &Value) -> Result<String, String> {
        match tool {
            "run_start" => self.live_start(input),
            "run_status" => self.live_status(input),
            "run_logs" => self.live_logs(input),
            n if logs::NAMES.contains(&n) => self.run_log_tool(n, input),
            "run_stop" => self.live_stop(input),
            "wait_ready" => self.live_wait(input),
            "list_recipes" => self.live_list_recipes(),
            "run_recipe" => self.live_run_recipe(input),
            "http_request" => self.live_http(input),
            "trace_request" => self.live_trace(input),
            "timeline" => self.live_timeline(input),
            "list_scenarios" => self.live_list_scenarios(),
            "run_scenario" => self.live_run_scenario(input),
            "save_scenario" => self.live_save_scenario(input),
            other => Err(format!("`{other}` is not a live tool")),
        }
    }

    fn live_start(&self, input: &Value) -> Result<String, String> {
        let live = self.live();
        let (name, command) = (str_arg(input, "name")?.to_string(), str_arg(input, "command")?.to_string());
        let cwd = bennu_live::prelude::resolve_cwd(self.project(), input.get("cwd").and_then(Value::as_str).unwrap_or("."))?;
        let env: Vec<(String, String)> = input
            .get("env")
            .and_then(Value::as_object)
            .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect())
            .unwrap_or_default();
        let ports: Vec<u16> = input.get("ports").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).filter_map(|p| u16::try_from(p).ok()).collect()).unwrap_or_default();
        let ready = match input.get("ready") {
            Some(v) if v.is_object() => readiness_from(v)?,
            _ => Readiness::default(),
        };
        self.refuse_if_held(&live, &name, &ports)?;
        let owner = Some(self.me().to_string());
        let info = live.manager.start(StartSpec { name: name.clone(), command, cwd, env, ports, ready, group: None, lifetime: None, owner })?;
        self.session.live_log().started(&name);
        let mut out = format!("started {} `{}` (pid {}). Read its output with run_logs, stop it with run_stop.", info.id, info.name, info.pid);
        if input.get("wait").and_then(Value::as_bool).unwrap_or(false) {
            match live.manager.wait_ready(&info.id, None, &self.live_cancelled()) {
                Ok(ms) => out.push_str(&format!("\nready after {}.", duration_text(ms))),
                Err(e) => out.push_str(&format!("\nNOT ready: {e}\nThe process is still running (if it is): use run_logs, or run_stop.")),
            }
        }
        Ok(out)
    }

    fn live_status(&self, input: &Value) -> Result<String, String> {
        let live = self.live();
        let me = self.me();
        if let Some(id) = input.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            return Ok(proc_line(&live.manager.get(id)?, me));
        }
        let all = live.manager.list();
        if all.is_empty() {
            return Ok("no managed processes. Start one with run_start, or a stack with run_recipe.".into());
        }
        Ok(format!("{} running of {} (cap {}):\n{}", live.manager.running_count(), all.len(), live.manager.limits().max_running, all.iter().map(|p| proc_line(p, me)).collect::<Vec<_>>().join("\n")))
    }

    fn live_stop(&self, input: &Value) -> Result<String, String> {
        let live = self.live();
        let me = self.me();
        // Only what this conversation started: another chat's services, and the user's, are left running.
        if input.get("all").and_then(Value::as_bool).unwrap_or(false) {
            let names: Vec<String> = live.manager.list().into_iter().filter(|p| !p.phase.is_finished() && p.owner.as_deref() == Some(me)).map(|p| p.name).collect();
            let n = live.manager.stop_owned(Some(me));
            names.iter().for_each(|n| self.session.live_log().stopped(n));
            let others = live.manager.running_count();
            let left = if others > 0 { format!(" {others} process(es) started by others keep running (they are not yours to stop).") } else { String::new() };
            return Ok(format!("stopped {n} process(es) of yours.{left}"));
        }
        if let Some(recipe) = input.get("recipe").and_then(Value::as_str).filter(|r| !r.is_empty()) {
            let stopped = stop_recipe(&live, recipe, Some(Some(me)))?;
            stopped.iter().for_each(|p| self.session.live_log().stopped(&p.name));
            return Ok(format!("stopped {} service(s) of `{recipe}`: {}", stopped.len(), stopped.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")));
        }
        let id = str_arg(input, "id")?;
        let target = live.manager.get(id)?;
        if target.owner.as_deref() != Some(me) {
            return Err(format!(
                "`{}` ({}) was started by {}, not by this conversation, so it was not stopped. You can still read it (run_status, run_logs, timeline).",
                target.name,
                target.id,
                owner_label(target.owner.as_deref())
            ));
        }
        let info = live.manager.stop(&target.id)?;
        self.session.live_log().stopped(&info.name);
        Ok(format!("{} — {}", proc_line(&info, me), "its process tree is gone."))
    }

    fn live_wait(&self, input: &Value) -> Result<String, String> {
        let live = self.live();
        let id = str_arg(input, "id")?;
        let given = ["port", "log", "http", "status", "timeout_s"].iter().any(|k| input.get(*k).is_some());
        let readiness = if given { Some(readiness_from(&ready_keys(input))?) } else { None };
        let ms = live.manager.wait_ready(id, readiness.as_ref(), &self.live_cancelled())?;
        Ok(format!("`{id}` is ready after {}.", duration_text(ms)))
    }

    fn live_list_recipes(&self) -> Result<String, String> {
        let root = &self.session.root;
        match read_recipes(root) {
            Recipes::None => Ok("no recipes file. Create .arbor/bennu/live/recipes.toml with [[recipe]] and [[recipe.service]] tables (name, command, cwd, env, ports, depends_on, ready), or ask the user to use the Live panel's \"Create recipes file\".".into()),
            Recipes::Invalid { errors, .. } => Err(format!("the recipes file has errors:\n- {}", errors.join("\n- "))),
            Recipes::Valid(file) => {
                let approved = recipes_approved(root, Some(self.me()), &file.hash);
                let mut out = format!("recipes file: {} (the user has {}approved it)\n", if file.recipes.is_empty() { "empty" } else { "valid" }, if approved { "" } else { "not " });
                for r in &file.recipes {
                    out.push_str(&format!("\nrecipe `{}`{}\n", r.name, if r.description.is_empty() { String::new() } else { format!(" — {}", r.description) }));
                    for s in r.start_order() {
                        let deps = if s.depends_on.is_empty() { String::new() } else { format!(" after {}", s.depends_on.join(",")) };
                        out.push_str(&format!("  - {}{deps}: $ {}  (in {}, ports {:?}, ready {})\n", s.name, s.command, s.cwd, s.ports, ready_text(&s.ready)));
                    }
                }
                Ok(out)
            }
        }
    }

    fn live_run_recipe(&self, input: &Value) -> Result<String, String> {
        let live = self.live();
        let name = str_arg(input, "name")?;
        let me = self.me();
        // `approved_recipe` re-checks the approval the gate recorded, so a file edited in between is refused.
        let recipe = approved_recipe(&live.root, name, Some(me))?;
        // Services another owner already runs are neither duplicated nor taken over. When that owner has the
        // whole stack up, the model is attached to it read-only, which is usually what it wanted.
        let held = foreign_holders(&live, &recipe, Some(me));
        if !held.is_empty() {
            let all_up = held.len() == recipe.start_order().len() && held.iter().all(|(_, h)| h.ready && h.owner == held[0].1.owner);
            if !all_up {
                return Err(held_text(&held[0].1, &format!("service `{}` of recipe `{name}`", held[0].0)));
            }
            let lines = held.iter().map(|(_, h)| proc_line(h, me)).collect::<Vec<_>>().join("\n");
            return Ok(format!(
                "recipe `{name}` is already up and held by {}; attached read-only (nothing was started). You can call its services with http_request and read run_status, run_logs, timeline and trace_request, but you cannot stop it.\n{lines}",
                owner_label(held[0].1.owner.as_deref())
            ));
        }
        let infos = run_recipe(&live, name, Some(me), &self.live_cancelled())?;
        infos.iter().for_each(|p| self.session.live_log().started(&p.name));
        Ok(format!("recipe `{name}` is up:\n{}", infos.iter().map(|p| proc_line(p, me)).collect::<Vec<_>>().join("\n")))
    }

    fn live_http(&self, input: &Value) -> Result<String, String> {
        let live = self.live();
        let url = str_arg(input, "url")?.to_string();
        let method = input.get("method").and_then(Value::as_str).unwrap_or("GET").to_ascii_uppercase();
        let mut headers: Vec<(String, String)> = input
            .get("headers")
            .and_then(Value::as_object)
            .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect())
            .unwrap_or_default();
        let body = match (input.get("json"), input.get("body").and_then(Value::as_str)) {
            (Some(j), _) if !j.is_null() => {
                if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
                    headers.push(("Content-Type".into(), "application/json".into()));
                }
                Some(j.to_string())
            }
            (_, Some(b)) => Some(b.to_string()),
            _ => None,
        };
        let timeout_ms = input.get("timeout_ms").and_then(Value::as_u64).unwrap_or(10_000).clamp(100, 120_000);
        let req = HttpRequest { method: method.clone(), url: url.clone(), headers, body, timeout_ms };
        match live.http(req) {
            Ok((resp, corr)) => {
                self.session.live_log().request(&method, &url, Some(resp.status), &corr, Some(resp.elapsed_ms));
                let shown_headers = bennu_live::prelude::redact_headers(&resp.headers, &[]).iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n");
                let body: String = resp.body.chars().take(MAX_BODY_CHARS).collect();
                let cut = resp.truncated || resp.body.chars().count() > MAX_BODY_CHARS;
                Ok(format!(
                    "HTTP {} {} in {} ms — correlation id {corr}\n{shown_headers}\n\nbody{}:\n{body}",
                    resp.status, resp.reason, resp.elapsed_ms, if cut { " (cut)" } else { "" }
                ))
            }
            Err(e) => {
                let corr = e.rsplit("correlation id ").next().unwrap_or("").trim_end_matches(')').to_string();
                self.session.live_log().request(&method, &url, None, &corr, None);
                Err(e)
            }
        }
    }

    fn live_trace(&self, input: &Value) -> Result<String, String> {
        let live = self.live();
        let corr = str_arg(input, "corr")?;
        let context = input.get("context").and_then(Value::as_u64).unwrap_or(8).min(40) as usize;
        let t = trace(&live.timeline.query(&Filter::default()), corr, context);
        if t.events.is_empty() {
            return Ok(format!("nothing in the timeline mentions `{corr}`. The services only show up here if they log the X-Correlation-Id header (e.g. via MDC or a request filter), or if the request was sent with http_request in this session."));
        }
        Ok(clip(format!("{} event(s) across {}:\n{}", t.events.len(), t.sources.join(", "), events_text(&t.events))))
    }

    fn live_timeline(&self, input: &Value) -> Result<String, String> {
        let live = self.live();
        let text = match input.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) {
            Some(t) => Some(Regex::new(&format!("(?i){}", regex::escape(t))).map_err(|e| e.to_string())?),
            None => None,
        };
        let kinds: Vec<EventKind> = input
            .get("kinds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .flat_map(|k| match k {
                "log" => vec![EventKind::Log],
                "http" => vec![EventKind::HttpRequest, EventKind::HttpResponse],
                "marker" => vec![EventKind::ProcessStarted, EventKind::ProcessReady, EventKind::ProcessExited, EventKind::Marker],
                _ => vec![],
            })
            .collect();
        let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(60).clamp(1, 300) as usize;
        let events = live.timeline.query(&Filter {
            since_seq: input.get("since_seq").and_then(Value::as_u64),
            source: input.get("source").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string),
            corr: input.get("corr").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string),
            text,
            kinds,
            limit: Some(limit),
            ..Default::default()
        });
        Ok(clip(format!("{} event(s), last_seq={}\n{}", events.len(), live.timeline.last_seq(), events_text(&events))))
    }

    fn live_list_scenarios(&self) -> Result<String, String> {
        let live = self.live();
        let all = read_scenarios(&self.session.root);
        if all.is_empty() {
            return Ok("no scenarios. Write one with save_scenario (.arbor/bennu/live/scenarios/<name>.toml).".into());
        }
        let results = live.results.lock().unwrap_or_else(|p| p.into_inner());
        let lines: Vec<String> = all
            .iter()
            .map(|e| match &e.scenario {
                Ok(s) => {
                    let last = results.get(&s.name).map(|r| format!("last run: {} in {}", if r.passed { "PASSED" } else { "FAILED" }, duration_text(r.duration_ms))).unwrap_or_else(|| "not run yet".into());
                    format!("{} — {} step(s) — {last}{}", e.file, s.steps.len(), if s.description.is_empty() { String::new() } else { format!(" — {}", s.description) })
                }
                Err(errors) => format!("{} — INVALID: {}", e.file, errors.join("; ")),
            })
            .collect();
        Ok(lines.join("\n"))
    }

    fn live_run_scenario(&self, input: &Value) -> Result<String, String> {
        let live = self.live();
        let name = str_arg(input, "name")?;
        let entry = read_scenario(&self.session.root, name)?;
        let scenario = entry.scenario.map_err(|e| format!("the scenario is invalid:\n- {}", e.join("\n- ")))?;
        {
            let mut running = live.scenario_running.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(other) = running.as_ref() {
                return Err(format!("scenario `{other}` is already running (started by another conversation or from the Live panel); scenarios run one at a time per project. Try again when it has finished."));
            }
            *running = Some(scenario.name.clone());
        }
        let cancelled = self.live_cancelled();
        let result = {
            let mut world = LiveWorld { live: Arc::clone(&live), cancelled: &cancelled, owner: Some(self.me().to_string()) };
            run_scenario(&mut world, &scenario)
        };
        *live.scenario_running.lock().unwrap_or_else(|p| p.into_inner()) = None;

        let failed = result.steps.iter().filter(|s| s.outcome == Outcome::Failed).count();
        let (pass, fail) = result.steps.iter().flat_map(|s| s.assertions.iter()).fold((0, 0), |(p, f), a| if a.passed { (p + 1, f) } else { (p, f + 1) });
        {
            let mut log = self.session.live_log();
            log.scenario(&result.name, result.passed, result.steps.len(), failed, pass, fail);
            for s in result.steps.iter().filter(|s| s.kind == "http") {
                if let Some(c) = &s.corr {
                    log.request("HTTP", &s.name, None, c, Some(s.duration_ms));
                }
            }
        }
        let mut text = result.summary();
        // The slice that explains a failure: the timeline of the first failing step.
        if let Some(bad) = result.steps.iter().find(|s| s.outcome == Outcome::Failed) {
            if !bad.slice.is_empty() {
                text.push_str(&format!("\ntimeline during step {} ({}):\n{}", bad.index + 1, bad.name, events_text(&bad.slice)));
            }
        }
        live.store_result(result);
        Ok(clip(text))
    }

    fn live_save_scenario(&self, input: &Value) -> Result<String, String> {
        let name = str_arg(input, "name")?;
        let content = input.get("content").and_then(Value::as_str).ok_or("`content` is required")?;
        parse_scenario(content).map_err(|e| format!("the scenario is not valid, nothing was saved:\n- {}", e.join("\n- ")))?;
        let file = scenario_path(&self.session.root, name)?;
        let old = std::fs::read_to_string(&file).unwrap_or_default();
        let text = if content.ends_with('\n') { content.to_string() } else { format!("{content}\n") };
        if self.session.applies_edits() {
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let mut out = self.submit(&file, &old, text, None, None)?;
        out.push_str("\nRun it with run_scenario once the file is in place.");
        Ok(out)
    }
}

/// The readiness keys of a `wait_ready` call: its arguments less the process id (`RawReady` rejects unknown keys).
fn ready_keys(input: &Value) -> Value {
    Value::Object(input.as_object().map(|o| o.iter().filter(|(k, _)| k.as_str() != "id").map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default())
}

fn ports_note(input: &Value) -> String {
    match input.get("ports").and_then(Value::as_array).filter(|p| !p.is_empty()) {
        Some(p) => format!("\nports {}", p.iter().map(Value::to_string).collect::<Vec<_>>().join(", ")),
        None => String::new(),
    }
}

fn ready_text(r: &RawReady) -> String {
    let mut parts = Vec::new();
    if let Some(p) = r.port {
        parts.push(format!("port {p}"));
    }
    if let Some(l) = &r.log {
        parts.push(format!("log /{l}/"));
    }
    if let Some(h) = &r.http {
        parts.push(format!("GET {h}"));
    }
    if let Some(d) = r.delay_ms {
        parts.push(format!("after {d} ms"));
    }
    if parts.is_empty() { "immediately".into() } else { parts.join(" + ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_family_is_exactly_the_specs() {
        let specs = specs();
        assert_eq!(specs.len(), NAMES.len() + logs::NAMES.len());
        for s in &specs {
            assert!(is_live_tool(&s.name), "{}", s.name);
        }
        assert!(!is_live_tool("bennu_build"));
    }

    #[test]
    fn the_clock_is_a_utc_time_of_day() {
        assert_eq!(clock(0), "00:00:00.000");
        assert_eq!(clock(3_661_007), "01:01:01.007");
        assert_eq!(clock(86_400_000 + 5), "00:00:00.005");
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration_text(250), "250 ms");
        assert_eq!(duration_text(1_500), "1.5 s");
        assert_eq!(duration_text(125_000), "2 min 5 s");
    }

    #[test]
    fn a_ready_object_is_validated_like_a_recipes_one() {
        assert!(readiness_from(&json!({"port": 8080, "timeout_s": 5})).is_ok());
        assert!(readiness_from(&json!({"log": "("})).unwrap_err().contains("regex"));
        assert!(readiness_from(&json!({"http": "http://example.com/"})).unwrap_err().contains("loopback"));
        assert!(readiness_from(&json!({"bogus": 1})).is_err());
    }
}
