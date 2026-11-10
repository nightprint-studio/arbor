//! Imber's schedules in Corvus: do something at a time, once or on a repeat — wake a conversation with a saved
//! prompt ("awake"), start a fresh one, or run a plain `git fetch`.
//!
//! * [`action`] — what a Corvus job does, and how Corvus words the schedule tools.
//! * this file — the handlers the Schedules window calls, the MCP tools, and what a run does when it fires.
//!
//! Everything neutral lives elsewhere: the job as data, its store and the calendar arithmetic are
//! `arbor-scheduler` (`jobs`, `calendar`); the `schedule_*` tools, the window helpers and the loop are
//! `imber-schedule`. A job is *data* the window can list, edit and show the next run of, so it is not
//! registered as an in-memory task in the engine; one small loop looks at the list every tick.
//!
//! **Autonomy.** A scheduled run has nobody to approve anything. What it may do is the job's `mode`, set when
//! the job was created (and approved when Imber created it): `manual` is read-only, `auto` lets writes run,
//! and destructive actions never run unattended (`permission::unattended_allows`). A job Imber creates can be
//! no freer than the conversation that created it.
//!
//! **Missed runs.** The loop only runs while Corvus does. A run whose time passed while it was closed is
//! dropped (`skip`, the default) or run once on the next start (`run_once`) — per job.
//!
//! **What the user is told.** A toast when a run starts and one when a run fails after a good one
//! (`arbor://corvus/imber-schedule`), whatever panel is open.

mod action;

use std::path::PathBuf;
use std::sync::{Arc, Once, OnceLock};

use arbor_scheduler::prelude::{now_ms, JobInput as GenericJobInput, Store};
use corvus_core::prelude::CorvusState;
use imber_core::prelude::ToolSpec;
use imber_schedule::prelude::{failed_event, record, run_tool as run_schedule_tool, save, set_enabled, specs as schedule_specs, start_loop, started_event, ToolCtx};
use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) use self::action::Action;
use self::action::CorvusFlavour;
use super::permission::Mode;

pub type Job = arbor_scheduler::prelude::Job<Action>;
pub type JobInput = GenericJobInput<Action>;

/// Where the frontend hears about runs starting and failing.
const TOPIC: &str = "arbor://corvus/imber-schedule";

/// The list, in `<corvus config dir>/imber-schedules.json`, loaded on first use.
fn store(state: &CorvusState) -> Result<&'static Store<Action>, String> {
    static STORE: OnceLock<Store<Action>> = OnceLock::new();
    if let Some(store) = STORE.get() {
        return Ok(store);
    }
    let path = PathBuf::from(crate::corvus_config::corvus_config_dir(state)?).join("imber-schedules.json");
    Ok(STORE.get_or_init(|| Store::new(path)))
}

// ── Running a job ────────────────────────────────────────────────────────────────────────────────

/// Write down how a run ended, and tell the user when it is the first failure after a good one.
fn record_end(state: &CorvusState, id: &str, ok: bool, detail: &str) {
    let Ok(store) = store(state) else { return };
    if let Some(done) = record(store, id, now_ms(), ok, detail) {
        if done.first_failure {
            state.event_sink().emit(TOPIC, failed_event(&done.job, detail));
        }
    }
}

/// Start what the job does. Returns at once; the outcome is recorded on the job when it ends.
fn run_job(state: &Arc<CorvusState>, handle: &tokio::runtime::Handle, job: Job) {
    state.event_sink().emit(TOPIC, started_event(&job));
    let id = job.id.clone();
    match &job.action {
        Action::Fetch { remote } => {
            let (st, rt, remote, root) = (Arc::clone(state), handle.clone(), remote.clone().unwrap_or_else(|| "origin".into()), job.root.clone());
            let spawned = std::thread::Builder::new().name(format!("imber-schedule-{id}")).spawn(move || {
                let outcome = crate::ephemeral::tab_for(&st, &root).and_then(|tab| rt.block_on(crate::remote::fetch_remote(&st, tab, remote.clone())));
                match outcome {
                    Ok(_) => record_end(&st, &id, true, &format!("Fetched {remote}.")),
                    Err(e) => record_end(&st, &id, false, &e),
                }
            });
            if let Err(e) = spawned {
                record_end(state, &job.id, false, &format!("could not start: {e}"));
            }
        }
        Action::Awake { conversation_id, prompt } => start_conversation(state, handle, &job, conversation_id.clone(), prompt),
        Action::Prompt { prompt } => start_conversation(state, handle, &job, None, prompt),
    }
}

fn start_conversation(state: &Arc<CorvusState>, handle: &tokio::runtime::Handle, job: &Job, conversation_id: Option<String>, prompt: &str) {
    let start = super::Start {
        conversation_id,
        text: format!("[Scheduled: {}] {prompt}", job.name),
        root: job.root.clone(),
        chat: false,
        retry: false,
        mode: Mode::parse(&job.mode),
        unattended: true,
        announce: Some(job.name.clone()),
    };
    let (hook_state, id) = (Arc::clone(state), job.id.clone());
    let hook: super::DoneHook = Box::new(move |ok, detail| record_end(&hook_state, &id, ok, &detail));
    if let Err(e) = super::begin_run(state, Arc::clone(state), handle.clone(), start, Some(hook)) {
        record_end(state, &job.id, false, &e);
    }
}

/// One look at the list: run what is due.
fn tick(handle: &tokio::runtime::Handle) {
    let Some(state) = super::STATE.get().cloned() else { return };
    let Ok(store) = store(&state) else { return };
    for job in store.due_now(now_ms()) {
        run_job(&state, handle, job);
    }
}

/// Start the loop, once, on the first call that has a runtime to give it.
fn ensure_driver(handle: tokio::runtime::Handle) {
    static START: Once = Once::new();
    if let Err(e) = start_loop(&START, "imber-schedules", move || tick(&handle)) {
        eprintln!("corvus-be: could not start the Imber schedule loop: {e}");
    }
}

// ── Handlers: the Schedules window ───────────────────────────────────────────────────────────────

/// Every schedule, soonest first. The first call also starts the loop that fires them.
#[arbor_rpc::handler]
async fn imber_schedule_list(state: &CorvusState) -> Result<Vec<Job>, String> {
    ensure_driver(tokio::runtime::Handle::current());
    Ok(store(state)?.list())
}

/// Create or edit a schedule (`id` absent creates). Returns it as saved, with its next run.
#[arbor_rpc::handler]
fn imber_schedule_save(state: &CorvusState, job: JobInput) -> Result<Job, String> {
    save(&CorvusFlavour, store(state)?, job, now_ms())
}

#[arbor_rpc::handler]
fn imber_schedule_delete(state: &CorvusState, id: String) -> Result<(), String> {
    store(state)?.remove(&id)
}

/// Turn a schedule on or off. Turning it on works out the next run from now.
#[arbor_rpc::handler]
fn imber_schedule_set_enabled(state: &CorvusState, id: String, enabled: bool) -> Result<Job, String> {
    set_enabled(store(state)?, &id, enabled, now_ms())
}

/// Run a schedule now, outside its times. Its next run is unchanged.
#[arbor_rpc::handler]
async fn imber_schedule_run_now(state: &CorvusState, id: String) -> Result<(), String> {
    let shared = super::STATE.get().cloned().ok_or("Imber is not ready yet")?;
    let job = store(state)?.get(&id).ok_or_else(|| format!("schedule '{id}' not found"))?;
    run_job(&shared, &tokio::runtime::Handle::current(), job);
    Ok(())
}

// ── Tools: Imber and MCP clients ─────────────────────────────────────────────────────────────────

pub fn specs() -> Vec<ToolSpec> {
    schedule_specs(&CorvusFlavour)
}

/// Run a schedule tool. `root` and `conversation_id` are this conversation's.
pub fn run_tool(state: &CorvusState, root: &str, conversation_id: &str, conversation_mode: Mode, name: &str, input: &Value) -> Result<String, String> {
    let ctx = ToolCtx { root, conversation_id, conversation_mode: conversation_mode.word(), now_ms: now_ms() };
    run_schedule_tool(&CorvusFlavour, store(state)?, &ctx, name, input)
}

// ── MCP: the same list and create, for AI clients ────────────────────────────────────────────────

/// Args for [`corvus_imber_schedules`].
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpListArgs {
    /// Absolute path to the repository root, or to any folder or file inside it.
    pub root: String,
}

/// The Imber schedules of a repository: what runs when, whether it is on, the next run and how the last
/// one ended.
#[arbor_rpc::handler(mcp(
    title = "List Imber schedules",
    safety = read,
))]
fn corvus_imber_schedules(state: &CorvusState, args: McpListArgs) -> Result<Vec<Job>, String> {
    let root = repo_root(state, &args.root)?;
    Ok(store(state)?.list().into_iter().filter(|j| j.root == root).collect())
}

/// Args for [`corvus_imber_schedule_create`].
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpCreateArgs {
    /// Absolute path to the repository root, or to any folder or file inside it.
    pub root: String,
    pub name: String,
    /// `prompt` (a fresh Imber conversation with `prompt`), `awake` (the same, there being no conversation
    /// to wake) or `fetch`.
    pub action: String,
    /// What to ask Imber at that time. Self-contained.
    pub prompt: Option<String>,
    /// When, in local time: `{"kind":"once","at":"2026-11-05T09:00"}`, `{"kind":"daily","time":"09:00"}`,
    /// `{"kind":"weekly","days":["mon","fri"],"time":"09:00"}`, `{"kind":"every","minutes":30}` or
    /// `{"kind":"cron","expr":"0 0 9 * * 1-5"}`.
    pub when: Value,
    /// `skip` (default) or `run_once`: what to do with a run missed while Corvus was closed.
    pub missed: Option<String>,
    /// For `fetch`: the remote (default origin).
    pub remote: Option<String>,
}

/// Schedule an Imber run for a repository. The schedule is **read-only** (manual mode): a scheduled run has
/// nobody to approve changes, and an MCP client cannot grant it more; the user can change that in Imber ›
/// Schedules. Corvus must be open at the time.
#[arbor_rpc::handler(mcp(
    title = "Create an Imber schedule",
    safety = write,
))]
fn corvus_imber_schedule_create(state: &CorvusState, args: McpCreateArgs) -> Result<Job, String> {
    let root = repo_root(state, &args.root)?;
    let action = if args.action == "fetch" { "fetch" } else { "prompt" };
    let mut input = json!({ "name": args.name, "action": action, "when": args.when, "mode": "manual" });
    for (key, value) in [("prompt", args.prompt), ("missed", args.missed), ("remote", args.remote)] {
        if let Some(v) = value {
            input[key] = json!(v);
        }
    }
    let made = run_tool(state, &root, "", Mode::Manual, "schedule_create", &input)?;
    let id = serde_json::from_str::<Value>(&made).ok().and_then(|v| v["id"].as_str().map(str::to_string)).ok_or("the schedule was not created")?;
    store(state)?.get(&id).ok_or_else(|| "the schedule was not created".into())
}

fn repo_root(state: &CorvusState, path: &str) -> Result<String, String> {
    crate::repo::repo_path(state, &crate::ephemeral::tab_for(state, path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_scheduler::prelude::{MissedPolicy, Recurrence};

    #[test]
    fn the_tools_are_the_four_schedule_tools_with_fetch_on_offer() {
        let all = specs();
        let names: Vec<&str> = all.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["schedule_list", "schedule_create", "schedule_update", "schedule_delete"]);
        let create = &all[1];
        assert!(create.description.contains("THIS repository") && create.description.contains("Corvus must be open"));
        assert_eq!(create.input_schema["properties"]["action"]["enum"], json!(["awake", "prompt", "fetch"]));
        assert!(create.input_schema["properties"].get("remote").is_some());
    }

    #[test]
    fn a_save_from_the_window_reads_the_way_the_old_one_did() {
        let input: JobInput = serde_json::from_value(json!({
            "name": "Morning", "enabled": true, "recurrence": {"kind": "daily", "hour": 9, "minute": 0},
            "action": {"kind": "prompt", "prompt": "status?"}, "root": "/r", "mode": "auto"
        }))
        .unwrap();
        assert_eq!(input.missed, MissedPolicy::Skip);
        assert_eq!(input.recurrence, Recurrence::Daily { hour: 9, minute: 0 });
    }
}
