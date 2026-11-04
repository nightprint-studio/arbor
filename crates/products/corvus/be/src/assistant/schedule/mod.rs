//! Imber's schedules: do something at a time, once or on a repeat — wake a conversation with a saved prompt
//! ("awake"), start a fresh one, or run a plain `git fetch`.
//!
//! * [`model`] — a job as data, and the rules that move it forward (pure, tested).
//! * [`store`] — the list, mirrored to `imber-schedules.json`.
//! * this file — the handlers the Schedules window calls, the tools Imber and MCP clients use, and the loop
//!   that fires what is due.
//!
//! The calendar arithmetic (next run, missed runs) is `arbor-scheduler`'s `calendar` module. A job is *data*
//! the window can list, edit and show the next run of, so it is not registered as an in-memory task in the
//! engine; one small loop here looks at the list every [`TICK`].
//!
//! **Autonomy.** A scheduled run has nobody to approve anything. What it may do is the job's `mode`, set when
//! the job was created (and approved when Imber created it): `manual` is read-only, `auto` lets writes run,
//! and destructive actions never run unattended (`permission::unattended_allows`). A job Imber creates can be
//! no freer than the conversation that created it.
//!
//! **Missed runs.** The loop only runs while Corvus does. A run whose time passed while it was closed is
//! dropped (`skip`, the default) or run once on the next start (`run_once`) — per job.

mod model;
mod store;

use std::sync::{Arc, Once};
use std::time::Duration;

use arbor_scheduler::prelude::{format_local, parse_clock, parse_local_datetime, parse_weekday, MissedPolicy, Recurrence};
use corvus_core::prelude::CorvusState;
use imber_core::prelude::ToolSpec;
use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) use self::model::{Action, Job};
use self::store::now_ms;
use super::permission::Mode;

/// How often the loop looks at the list. A run starts at most this late.
const TICK: Duration = Duration::from_secs(20);

// ── The `when` of a tool call → a recurrence ─────────────────────────────────────────────────────

/// The friendly form Imber writes (`{"kind":"weekly","days":["mon","fri"],"time":"09:00"}`) as a [`Recurrence`].
/// `now_ms` anchors an interval: it first runs one interval from now.
pub fn when_to_recurrence(when: &Value, now_ms: i64) -> Result<Recurrence, String> {
    let text = |k: &str| when.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let clock = || text("time").ok_or("give the time as HH:MM").and_then(|t| parse_clock(t).ok_or("the time must look like 09:30")).map_err(String::from);
    match text("kind").unwrap_or("") {
        "once" => {
            let at = text("at").ok_or("give the date and time as YYYY-MM-DDTHH:MM (local time)")?;
            let at_ms = parse_local_datetime(at).ok_or_else(|| format!("'{at}' is not a local date and time like 2026-11-05T09:00"))?;
            Ok(Recurrence::Once { at_ms })
        }
        "daily" => clock().map(|(hour, minute)| Recurrence::Daily { hour, minute }),
        "weekly" => {
            let names = when.get("days").and_then(Value::as_array).ok_or("give the weekdays, e.g. [\"mon\", \"fri\"]")?;
            let mut days = Vec::new();
            for n in names.iter().filter_map(Value::as_str) {
                let d = parse_weekday(n).ok_or_else(|| format!("'{n}' is not a weekday"))?;
                if !days.contains(&d) {
                    days.push(d);
                }
            }
            let (hour, minute) = clock()?;
            Ok(Recurrence::Weekly { days, hour, minute })
        }
        "every" => {
            let minutes = when.get("minutes").and_then(Value::as_u64).ok_or("give the interval as `minutes`")?;
            Ok(Recurrence::Interval { secs: minutes.saturating_mul(60), anchor_ms: now_ms })
        }
        "cron" => Ok(Recurrence::Cron { expr: text("expr").ok_or("give the 6-field cron expression as `expr`")?.to_string() }),
        other => Err(format!("unknown kind '{other}' (once, daily, weekly, every, cron)")),
    }
}

// ── Saving from the window ───────────────────────────────────────────────────────────────────────

/// What the Schedules window sends to save a job.
#[derive(Debug, Deserialize)]
pub struct JobInput {
    /// `None` creates.
    pub id: Option<String>,
    pub name: String,
    pub enabled: bool,
    pub recurrence: Recurrence,
    #[serde(default)]
    pub missed: MissedPolicy,
    pub action: Action,
    pub root: String,
    #[serde(default)]
    pub mode: String,
}

/// The job a save produces: new, or the stored one with the edits, keeping its history. Pure over the stored
/// job and the clock. An interval is anchored to now whenever it is new or its cadence changed.
fn apply_input(existing: Option<Job>, input: JobInput, now_ms: i64) -> Result<Job, String> {
    let mut recurrence = input.recurrence;
    if let Recurrence::Interval { secs, anchor_ms } = &mut recurrence {
        let unchanged = matches!(existing.as_ref().map(|j| &j.recurrence), Some(Recurrence::Interval { secs: s, .. }) if s == secs);
        *anchor_ms = if unchanged { existing.as_ref().and_then(|j| if let Recurrence::Interval { anchor_ms, .. } = j.recurrence { Some(anchor_ms) } else { None }).unwrap_or(now_ms) } else { now_ms };
    }
    let mut job = Job {
        id: input.id.clone().filter(|i| !i.is_empty()).unwrap_or_else(store::new_id),
        name: input.name.trim().to_string(),
        enabled: input.enabled,
        recurrence,
        missed: input.missed,
        action: input.action,
        root: input.root,
        mode: Mode::parse(&input.mode).word().to_string(),
        created_ms: existing.as_ref().map_or(now_ms, |j| j.created_ms),
        next_run_ms: None,
        last_run: existing.as_ref().and_then(|j| j.last_run.clone()),
        runs: existing.as_ref().map_or(0, |j| j.runs),
    };
    job.validate()?;
    job.schedule_from(now_ms)?;
    Ok(job)
}

// ── Running a job ────────────────────────────────────────────────────────────────────────────────

fn record(state: &CorvusState, id: &str, ok: bool, detail: &str) {
    let _ = store::mutate(state, id, |j| {
        j.finished(now_ms(), ok, detail);
        Ok(())
    });
}

/// Start what the job does. Returns at once; the outcome is recorded on the job when it ends.
fn run_job(state: &Arc<CorvusState>, handle: &tokio::runtime::Handle, job: Job) {
    let id = job.id.clone();
    match &job.action {
        Action::Fetch { remote } => {
            let (st, rt, remote, root) = (Arc::clone(state), handle.clone(), remote.clone().unwrap_or_else(|| "origin".into()), job.root.clone());
            let spawned = std::thread::Builder::new().name(format!("imber-schedule-{id}")).spawn(move || {
                let outcome = crate::ephemeral::tab_for(&st, &root).and_then(|tab| rt.block_on(crate::remote::fetch_remote(&st, tab, remote.clone())));
                match outcome {
                    Ok(_) => record(&st, &id, true, &format!("Fetched {remote}.")),
                    Err(e) => record(&st, &id, false, &e),
                }
            });
            if let Err(e) = spawned {
                record(state, &job.id, false, &format!("could not start: {e}"));
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
        mode: job.mode(),
        unattended: true,
        announce: Some(job.name.clone()),
    };
    let (hook_state, id) = (Arc::clone(state), job.id.clone());
    let hook: super::DoneHook = Box::new(move |ok, detail| record(&hook_state, &id, ok, &detail));
    if let Err(e) = super::begin_run(state, Arc::clone(state), handle.clone(), start, Some(hook)) {
        record(state, &job.id, false, &e);
    }
}

/// One look at the list: run what is due.
fn tick(handle: &tokio::runtime::Handle) {
    let Some(state) = super::STATE.get().cloned() else { return };
    for job in store::due_now(&state, now_ms()) {
        run_job(&state, handle, job);
    }
}

/// Start the loop, once, on the first call that has a runtime to give it. The first look is immediate, which is
/// where runs missed while Corvus was closed are dropped or caught up.
fn ensure_driver(handle: tokio::runtime::Handle) {
    static START: Once = Once::new();
    START.call_once(|| {
        let spawned = std::thread::Builder::new().name("imber-schedules".into()).spawn(move || loop {
            tick(&handle);
            std::thread::sleep(TICK);
        });
        if let Err(e) = spawned {
            eprintln!("corvus-be: could not start the Imber schedule loop: {e}");
        }
    });
}

// ── Handlers: the Schedules window ───────────────────────────────────────────────────────────────

/// Every schedule, soonest first. The first call also starts the loop that fires them.
#[arbor_rpc::handler]
async fn imber_schedule_list(state: &CorvusState) -> Result<Vec<Job>, String> {
    ensure_driver(tokio::runtime::Handle::current());
    Ok(store::list(state))
}

/// Create or edit a schedule (`id` absent creates). Returns it as saved, with its next run.
#[arbor_rpc::handler]
fn imber_schedule_save(state: &CorvusState, job: JobInput) -> Result<Job, String> {
    let existing = job.id.as_deref().and_then(|id| store::get(state, id));
    store::upsert(state, apply_input(existing, job, now_ms())?)
}

#[arbor_rpc::handler]
fn imber_schedule_delete(state: &CorvusState, id: String) -> Result<(), String> {
    store::remove(state, &id)
}

/// Turn a schedule on or off. Turning it on works out the next run from now.
#[arbor_rpc::handler]
fn imber_schedule_set_enabled(state: &CorvusState, id: String, enabled: bool) -> Result<Job, String> {
    store::mutate(state, &id, |j| {
        j.enabled = enabled;
        j.schedule_from(now_ms())?;
        Ok(j.clone())
    })
}

/// Run a schedule now, outside its times. Its next run is unchanged.
#[arbor_rpc::handler]
async fn imber_schedule_run_now(state: &CorvusState, id: String) -> Result<(), String> {
    let shared = super::STATE.get().cloned().ok_or("Imber is not ready yet")?;
    let job = store::get(state, &id).ok_or_else(|| format!("schedule '{id}' not found"))?;
    run_job(&shared, &tokio::runtime::Handle::current(), job);
    Ok(())
}

// ── Tools: Imber and MCP clients ─────────────────────────────────────────────────────────────────

fn spec(name: &str, description: &str, properties: Value, required: &[&str]) -> ToolSpec {
    ToolSpec { name: name.into(), description: description.into(), input_schema: json!({ "type": "object", "properties": properties, "required": required }) }
}

fn when_schema() -> Value {
    json!({ "type": "object", "description": "When it runs, in local time. {\"kind\":\"once\",\"at\":\"2026-11-05T09:00\"} | {\"kind\":\"daily\",\"time\":\"09:00\"} | {\"kind\":\"weekly\",\"days\":[\"mon\",\"fri\"],\"time\":\"09:00\"} | {\"kind\":\"every\",\"minutes\":30} | {\"kind\":\"cron\",\"expr\":\"0 0 9 * * 1-5\"} (6 fields: sec min hour day month weekday)." })
}

pub fn specs() -> Vec<ToolSpec> {
    let s = |d: &str| json!({ "type": "string", "description": d });
    vec![
        spec("schedule_list", "List the schedules: what runs when, whether it is on, its next run and how the last one ended.", json!({}), &[]),
        spec(
            "schedule_create",
            "Schedule something for later or on a repeat, for THIS repository. action `awake` wakes THIS conversation with `prompt` (it keeps its history); `prompt` starts a fresh conversation with it; `fetch` just fetches. A scheduled run has nobody to approve anything: with mode `manual` (default) it is read-only; `auto` lets it make changes, but never destructive ones, and no more than this conversation may do. Corvus must be open at the time; a run missed while it was closed is dropped unless missed=`run_once`.",
            json!({
                "name": s("A short name."),
                "action": json!({ "type": "string", "enum": ["awake", "prompt", "fetch"] }),
                "prompt": s("What to ask Imber at that time (awake / prompt). Self-contained: say what to check and what to report."),
                "when": when_schema(),
                "mode": json!({ "type": "string", "enum": ["manual", "auto"] }),
                "missed": json!({ "type": "string", "enum": ["skip", "run_once"] }),
                "remote": s("For fetch: the remote (default origin)."),
            }),
            &["name", "action", "when"],
        ),
        spec(
            "schedule_update",
            "Change a schedule: turn it on or off, or edit its name, prompt, time or mode.",
            json!({ "id": s("The schedule's id (from schedule_list)."), "enabled": json!({ "type": "boolean" }), "name": s("New name."), "prompt": s("New prompt."), "when": when_schema(), "mode": json!({ "type": "string", "enum": ["manual", "auto"] }) }),
            &["id"],
        ),
        spec("schedule_delete", "Delete a schedule.", json!({ "id": s("The schedule's id.") }), &["id"]),
    ]
}

/// The one-line view of a job a tool answers with.
fn job_json(j: &Job) -> Value {
    json!({
        "id": j.id, "name": j.name, "enabled": j.enabled, "action": j.action.word(), "mode": j.mode,
        "next_run": j.next_run_ms.map(format_local), "runs": j.runs,
        "last_run": j.last_run.as_ref().map(|l| json!({ "at": format_local(l.at_ms), "ok": l.ok, "detail": l.detail })),
    })
}

/// The mode a job created in a conversation of `conversation_mode` gets: what was asked, but no freer.
fn effective_mode(requested: Option<&str>, conversation_mode: Mode) -> Mode {
    match (Mode::parse(requested.unwrap_or("manual")), conversation_mode) {
        (Mode::Auto, Mode::Auto) => Mode::Auto,
        _ => Mode::Manual,
    }
}

/// Run a schedule tool. `root` and `conversation_id` are this conversation's.
pub fn run_tool(state: &CorvusState, root: &str, conversation_id: &str, conversation_mode: Mode, name: &str, input: &Value) -> Result<String, String> {
    let text = |k: &str| input.get(k).and_then(Value::as_str).map(str::to_string).filter(|s| !s.trim().is_empty());
    let now = now_ms();
    let out = match name {
        "schedule_list" => serde_json::to_string(&store::list(state).iter().filter(|j| j.root == root).map(job_json).collect::<Vec<_>>()),
        "schedule_create" => {
            let action = match text("action").as_deref() {
                Some("awake") => Action::Awake { conversation_id: Some(conversation_id.to_string()), prompt: text("prompt").ok_or("`prompt` is required for this action")? },
                Some("prompt") => Action::Prompt { prompt: text("prompt").ok_or("`prompt` is required for this action")? },
                Some("fetch") => Action::Fetch { remote: text("remote") },
                other => return Err(format!("unknown action {other:?} (awake, prompt, fetch)")),
            };
            let recurrence = when_to_recurrence(input.get("when").ok_or("`when` is required")?, now)?;
            let missed = if text("missed").as_deref() == Some("run_once") { MissedPolicy::RunOnce } else { MissedPolicy::Skip };
            let mode = effective_mode(text("mode").as_deref(), conversation_mode);
            let input = JobInput { id: None, name: text("name").ok_or("`name` is required")?, enabled: true, recurrence, missed, action, root: root.to_string(), mode: mode.word().into() };
            let job = store::upsert(state, apply_input(None, input, now)?)?;
            let note = if text("mode").as_deref() == Some("auto") && mode == Mode::Manual { " Created read-only: this conversation is in manual mode, so a schedule cannot be given more freedom than it has." } else { "" };
            return Ok(format!("{}{note}", serde_json::to_string(&job_json(&job)).map_err(|e| e.to_string())?));
        }
        "schedule_update" => {
            let id = text("id").ok_or("`id` is required")?;
            let current = store::get(state, &id).ok_or_else(|| format!("schedule '{id}' not found"))?;
            if current.root != root {
                return Err("That schedule belongs to another repository.".into());
            }
            let recurrence = match input.get("when") {
                Some(w) => when_to_recurrence(w, now)?,
                None => current.recurrence.clone(),
            };
            let action = match (&current.action, text("prompt")) {
                (Action::Awake { conversation_id, .. }, Some(prompt)) => Action::Awake { conversation_id: conversation_id.clone(), prompt },
                (Action::Prompt { .. }, Some(prompt)) => Action::Prompt { prompt },
                (other, _) => other.clone(),
            };
            let mode = match text("mode") {
                Some(m) => effective_mode(Some(&m), conversation_mode),
                None => current.mode(),
            };
            let edited = JobInput {
                id: Some(id),
                name: text("name").unwrap_or_else(|| current.name.clone()),
                enabled: input.get("enabled").and_then(Value::as_bool).unwrap_or(current.enabled),
                recurrence,
                missed: current.missed,
                action,
                root: current.root.clone(),
                mode: mode.word().into(),
            };
            let job = store::upsert(state, apply_input(Some(current), edited, now)?)?;
            return serde_json::to_string(&job_json(&job)).map_err(|e| e.to_string());
        }
        "schedule_delete" => {
            let id = text("id").ok_or("`id` is required")?;
            if store::get(state, &id).is_some_and(|j| j.root != root) {
                return Err("That schedule belongs to another repository.".into());
            }
            store::remove(state, &id)?;
            return Ok(format!("Deleted schedule {id}."));
        }
        other => return Err(format!("'{other}' is not a schedule tool.")),
    };
    out.map_err(|e| e.to_string())
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
    Ok(store::list(state).into_iter().filter(|j| j.root == root).collect())
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
    store::get(state, &id).ok_or_else(|| "the schedule was not created".into())
}

fn repo_root(state: &CorvusState, path: &str) -> Result<String, String> {
    crate::repo::repo_path(state, &crate::ephemeral::tab_for(state, path)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MIN: i64 = 60_000;

    fn input(recurrence: Recurrence) -> JobInput {
        JobInput { id: None, name: " Morning check ".into(), enabled: true, recurrence, missed: MissedPolicy::Skip, action: Action::Prompt { prompt: "status?".into() }, root: "/r".into(), mode: "auto".into() }
    }

    #[test]
    fn the_friendly_when_becomes_a_recurrence() {
        let now = 1_000;
        assert_eq!(when_to_recurrence(&json!({"kind": "daily", "time": "09:30"}), now), Ok(Recurrence::Daily { hour: 9, minute: 30 }));
        assert_eq!(
            when_to_recurrence(&json!({"kind": "weekly", "days": ["Mon", "fri", "lun"], "time": "08:00"}), now),
            Ok(Recurrence::Weekly { days: vec![0, 4], hour: 8, minute: 0 }),
            "names in either language, no duplicates"
        );
        assert_eq!(when_to_recurrence(&json!({"kind": "every", "minutes": 30}), now), Ok(Recurrence::Interval { secs: 1800, anchor_ms: now }));
        assert_eq!(when_to_recurrence(&json!({"kind": "cron", "expr": "0 0 9 * * 1-5"}), now), Ok(Recurrence::Cron { expr: "0 0 9 * * 1-5".into() }));
        match when_to_recurrence(&json!({"kind": "once", "at": "2026-11-05T09:00"}), now) {
            Ok(Recurrence::Once { at_ms }) => assert!((1_793_000_000_000..1_794_000_000_000).contains(&at_ms), "{at_ms}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_bad_when_says_what_is_wrong() {
        let err = |v: Value| when_to_recurrence(&v, 0).unwrap_err();
        assert!(err(json!({"kind": "daily"})).contains("HH:MM"));
        assert!(err(json!({"kind": "daily", "time": "9am"})).contains("09:30"));
        assert!(err(json!({"kind": "weekly", "days": ["funday"], "time": "09:00"})).contains("not a weekday"));
        assert!(err(json!({"kind": "weekly", "time": "09:00"})).contains("weekdays"));
        assert!(err(json!({"kind": "once", "at": "tomorrow"})).contains("not a local date"));
        assert!(err(json!({"kind": "every"})).contains("minutes"));
        assert!(err(json!({"kind": "sometimes"})).contains("unknown kind"));
        assert!(err(json!({})).contains("unknown kind"));
    }

    #[test]
    fn a_new_job_is_validated_and_scheduled_and_keeps_its_cleaned_name() {
        let job = apply_input(None, input(Recurrence::Interval { secs: 600, anchor_ms: 0 }), 10 * MIN).unwrap();
        assert_eq!(job.name, "Morning check");
        assert_eq!(job.mode, "auto");
        assert_eq!(job.next_run_ms, Some(20 * MIN), "an interval is anchored to now and first runs one interval on");
        assert!(job.id.starts_with("sch-"));
        assert!(apply_input(None, input(Recurrence::Interval { secs: 5, anchor_ms: 0 }), 0).is_err());
    }

    #[test]
    fn editing_keeps_history_and_the_interval_anchor_unless_the_cadence_changed() {
        let mut stored = apply_input(None, input(Recurrence::Interval { secs: 600, anchor_ms: 0 }), 10 * MIN).unwrap();
        stored.runs = 4;
        stored.finished(15 * MIN, true, "ok");
        let mut same = input(Recurrence::Interval { secs: 600, anchor_ms: 0 });
        same.id = Some(stored.id.clone());
        let edited = apply_input(Some(stored.clone()), same, 100 * MIN).unwrap();
        assert_eq!((edited.runs, edited.created_ms), (4, stored.created_ms));
        assert_eq!(edited.last_run, stored.last_run);
        assert_eq!(edited.recurrence, stored.recurrence, "same cadence: same anchor");
        let mut changed = input(Recurrence::Interval { secs: 1200, anchor_ms: 0 });
        changed.id = Some(stored.id.clone());
        let moved = apply_input(Some(stored), changed, 100 * MIN).unwrap();
        assert_eq!(moved.recurrence, Recurrence::Interval { secs: 1200, anchor_ms: 100 * MIN });
    }

    #[test]
    fn a_disabled_job_has_no_next_run() {
        let mut i = input(Recurrence::Daily { hour: 9, minute: 0 });
        i.enabled = false;
        assert_eq!(apply_input(None, i, 0).unwrap().next_run_ms, None);
    }

    #[test]
    fn an_unknown_mode_word_is_the_read_only_one() {
        let mut i = input(Recurrence::Daily { hour: 9, minute: 0 });
        i.mode = "autonomous".into();
        assert_eq!(apply_input(None, i, 0).unwrap().mode, "manual");
    }

    #[test]
    fn a_job_is_no_freer_than_the_conversation_that_made_it() {
        assert_eq!(effective_mode(Some("auto"), Mode::Auto), Mode::Auto);
        assert_eq!(effective_mode(Some("auto"), Mode::Manual), Mode::Manual);
        assert_eq!(effective_mode(None, Mode::Auto), Mode::Manual, "read-only unless asked");
        assert_eq!(effective_mode(Some("manual"), Mode::Auto), Mode::Manual);
    }

    #[test]
    fn the_tools_are_named_and_keyed_consistently() {
        let names: Vec<String> = specs().into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["schedule_list", "schedule_create", "schedule_update", "schedule_delete"]);
        for t in specs() {
            for r in t.input_schema["required"].as_array().unwrap() {
                assert!(t.input_schema["properties"].get(r.as_str().unwrap()).is_some(), "{}: {r}", t.name);
            }
        }
    }
}
