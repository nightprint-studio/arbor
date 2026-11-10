//! Imber's schedules in Bennu: "every weekday at 09:00 ask Imber what changed on `main`" — a run that happens
//! while nobody is looking.
//!
//! * [`action`] — what a Bennu job does (wake a conversation, or start one) and how Bennu words the tools.
//! * [`unattended`] — the table of what a run with nobody there may do (pure, tested).
//! * [`request`] — the arguments and the words of a scheduled run (pure, tested).
//! * [`budget`] — the cap on one run and on how many start in a day (pure parts tested).
//! * this file — the handlers the Schedules window calls, the MCP pair, and what happens when a job fires.
//!
//! The job as data, its store and the calendar are `arbor-scheduler`; the `schedule_*` tools, the window helpers
//! and the loop are `imber-schedule`. A schedule is *data* the window can list and edit; one small loop looks at
//! the list every tick and starts what is due, only while Bennu runs (no daemon).
//!
//! **Autonomy.** The run is built exactly like a panel send — the user's model, instructions, plan and skills —
//! but in manual mode, inside the project, with no git, sub-agents or isolated copy, and with [`unattended`]
//! deciding every tool call: it reads, searches and *proposes*; the proposals wait as cards in the run's
//! conversation. Nothing is written to the live project, no command or process starts, nothing is deleted, no
//! schedule is created, and a question for the user ends the run as incomplete.
//!
//! **Cost.** One run is capped ([`budget::run_cap_usd`]), so is the number of runs a day, and the ordinary
//! day/week/month limits apply on top. A job never starts while its previous run is going.

mod action;
pub(super) mod budget;
pub(super) mod request;
pub(super) mod unattended;

use std::collections::HashSet;
use std::sync::{Arc, Mutex, Once, OnceLock};

use arbor_scheduler::prelude::{now_ms, period_starts, JobAction, JobInput as GenericJobInput, Store};
use bennu_core::prelude::BennuState;
use bennu_assistant::prelude::ToolSpec;
use imber_schedule::prelude::{failed_event, record, run_tool as run_schedule_tool, save, set_enabled, specs as schedule_specs, start_loop, started_event, ToolCtx};
use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) use self::action::Action;
use self::action::BennuFlavour;
use self::request::Wake;

pub type Job = arbor_scheduler::prelude::Job<Action>;
pub type JobInput = GenericJobInput<Action>;

/// Where the frontend hears about runs starting and failing.
const TOPIC: &str = "arbor://bennu/imber-schedule";

/// Told once, at the very end of a scheduled run: whether it ended well, and a line about it.
pub(super) type DoneHook = Box<dyn FnOnce(bool, String) + Send>;

/// What a scheduled run adds to a send.
pub(super) struct Scheduled {
    pub on_done: DoneHook,
}

/// How a run ended as the line the schedule keeps: whether it counts as done, and why not when it does not.
/// `end_note` is a reason the run itself gave (it needed a person, it reached its cost cap) and wins.
pub(super) fn outcome_line(ended: &str, error: Option<String>, last_said: Option<String>, end_note: Option<String>) -> (bool, String) {
    if let Some(note) = end_note {
        return (false, format!("Incomplete. {note}"));
    }
    if let Some(e) = error {
        return (false, e);
    }
    let said = last_said.unwrap_or_default();
    match ended {
        "done" => (true, if said.trim().is_empty() { "Done.".into() } else { said }),
        "cancelled" => (false, "Stopped before it finished.".into()),
        "too_many_steps" => (false, "Incomplete. It stopped after too many steps.".into()),
        "incomplete" => (false, "Incomplete. The answer was cut short.".into()),
        other => (false, format!("The run ended as {other}.")),
    }
}

// ── The list ─────────────────────────────────────────────────────────────────────────────────────

/// The list, in `<bennu config dir>/imber-schedules.json`, loaded on first use. Not in the project: a job's next
/// and last run change on every fire and its prompt is personal, so a file under the repository would be noise in
/// `git status` and leak into the team.
fn store() -> &'static Store<Action> {
    static STORE: OnceLock<Store<Action>> = OnceLock::new();
    STORE.get_or_init(|| Store::new(arbor_core::prelude::bennu_config_dir().join("imber-schedules.json")))
}

// ── Running a job ────────────────────────────────────────────────────────────────────────────────

/// What the loop needs to start a run: the state a handler would have been given, and a runtime to spawn on.
struct Runtime {
    state: Arc<BennuState>,
    handle: tokio::runtime::Handle,
}

static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();

/// The jobs whose run is going right now: a job never starts twice at once.
fn running() -> &'static Mutex<HashSet<String>> {
    static RUNNING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    RUNNING.get_or_init(Default::default)
}

/// Holds a job's place among the running ones until it is dropped, however the run ends.
struct Claim(String);

impl Claim {
    fn take(id: &str) -> Option<Claim> {
        running().lock().unwrap_or_else(|p| p.into_inner()).insert(id.to_string()).then(|| Claim(id.to_string()))
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        running().lock().unwrap_or_else(|p| p.into_inner()).remove(&self.0);
    }
}

/// Write down how a run ended, and tell the user when it is the first failure after a good one.
fn record_end(rt: &Runtime, id: &str, ok: bool, detail: &str) {
    if let Some(done) = record(store(), id, now_ms(), ok, detail) {
        if done.first_failure {
            rt.state.event_sink().emit(TOPIC, failed_event(&done.job, detail));
        }
    }
}

/// Run the hook in the slot, if nobody has yet: it can be reached from the run's own thread or, when the send fails
/// before there is a run, from the task that started it — whichever is first.
fn fire(slot: &Mutex<Option<DoneHook>>, ok: bool, detail: String) {
    let hook = slot.lock().unwrap_or_else(|p| p.into_inner()).take();
    if let Some(hook) = hook {
        hook(ok, detail);
    }
}

/// Start what the job does. Returns at once; the outcome is recorded on the job when the run ends.
fn run_job(rt: &Arc<Runtime>, job: Job) {
    let skip = |why: &str| record_end(rt, &job.id, false, &format!("Skipped: {why}"));
    let Some(claim) = Claim::take(&job.id) else { return skip("the previous run is still going.") };
    let cfg = bennu_core::config::load().assistant;
    let (day, week, month) = period_starts(now_ms());
    if !budget::within_daily_cap(budget::started_since(day), cfg.limits.schedule_runs_per_day) {
        return skip(&format!("the limit of {} scheduled runs a day was reached (Settings \u{203a} Imber \u{203a} Spending limits).", cfg.limits.schedule_runs_per_day));
    }
    let conversation = job.action.conversation();
    if conversation.is_some_and(super::conversation_active) {
        return skip("Imber is already working in that conversation.");
    }
    let wake = Wake { job_name: &job.name, conversation_id: conversation, prompt: job.action.prompt().unwrap_or(""), root: &job.root, periods: (day, week, month) };
    let args: super::SendArgs = match serde_json::from_value(request::send_args(&cfg, &wake)) {
        Ok(args) => args,
        Err(e) => return skip(&format!("the run could not be built ({e}).")),
    };

    rt.state.event_sink().emit(TOPIC, started_event(&job));
    budget::note_start(now_ms());

    let id = job.id.clone();
    let end_rt = Arc::clone(rt);
    // The claim goes with the hook: when the run ends — or the hook is dropped unfired, the run thread having died.
    let slot: Arc<Mutex<Option<DoneHook>>> = Arc::new(Mutex::new(Some(Box::new(move |ok, detail| {
        record_end(&end_rt, &id, ok, &detail);
        drop(claim);
    }))));
    let task_rt = Arc::clone(rt);
    rt.handle.spawn(async move {
        let run_slot = Arc::clone(&slot);
        let scheduled = Scheduled { on_done: Box::new(move |ok, detail| fire(&run_slot, ok, detail)) };
        if let Err(e) = super::send(&task_rt.state, args, Some(scheduled)).await {
            fire(&slot, false, e);
        }
    });
}

/// One look at the list: run what is due.
fn tick() {
    let Some(rt) = RUNTIME.get() else { return };
    for job in store().due_now(now_ms()) {
        run_job(rt, job);
    }
}

/// Remember how to start a run, and start the loop (once). The first look is immediate, which is where runs
/// missed while Bennu was closed are dropped or caught up.
fn ensure_driver(ctx: &BennuState, handle: tokio::runtime::Handle) -> Arc<Runtime> {
    static START: Once = Once::new();
    let rt = RUNTIME
        .get_or_init(|| {
            let mut state = BennuState::new(ctx.event_sink());
            if let Some(host) = ctx.host_caller() {
                state = state.with_host_caller(host);
            }
            Arc::new(Runtime { state: Arc::new(state), handle })
        })
        .clone();
    if let Err(e) = start_loop(&START, "imber-schedules", tick) {
        eprintln!("bennu-be: could not start the Imber schedule loop: {e}");
    }
    rt
}

// ── Handlers: the Schedules window ───────────────────────────────────────────────────────────────

/// Every schedule, soonest first. The first call also starts the loop that fires them, so the window of a project
/// asks for it as soon as it opens.
#[arbor_rpc::handler]
async fn imber_schedule_list(ctx: &BennuState) -> Result<Vec<Job>, String> {
    ensure_driver(ctx, tokio::runtime::Handle::current());
    Ok(store().list())
}

/// Create or edit a schedule (`id` absent creates). Returns it as saved, with its next run.
#[arbor_rpc::handler]
fn imber_schedule_save(_ctx: &BennuState, job: JobInput) -> Result<Job, String> {
    save(&BennuFlavour, store(), job, now_ms())
}

#[arbor_rpc::handler]
fn imber_schedule_delete(_ctx: &BennuState, id: String) -> Result<(), String> {
    store().remove(&id)
}

/// Turn a schedule on or off. Turning it on works out the next run from now.
#[arbor_rpc::handler]
fn imber_schedule_set_enabled(_ctx: &BennuState, id: String, enabled: bool) -> Result<Job, String> {
    set_enabled(store(), &id, enabled, now_ms())
}

/// Run a schedule now, outside its times. Its next run is unchanged.
#[arbor_rpc::handler]
async fn imber_schedule_run_now(ctx: &BennuState, id: String) -> Result<(), String> {
    let rt = ensure_driver(ctx, tokio::runtime::Handle::current());
    let job = store().get(&id).ok_or_else(|| format!("schedule '{id}' not found"))?;
    run_job(&rt, job);
    Ok(())
}

// ── Tools: Imber ─────────────────────────────────────────────────────────────────────────────────

pub fn specs() -> Vec<ToolSpec> {
    schedule_specs(&BennuFlavour)
}

/// Run a schedule tool for the conversation that called it. (The approval card was put by the gate.)
pub fn run_tool(root: &str, conversation_id: &str, name: &str, input: &Value) -> Result<String, String> {
    let ctx = ToolCtx { root, conversation_id, conversation_mode: "manual", now_ms: now_ms() };
    run_schedule_tool(&BennuFlavour, store(), &ctx, name, input)
}

// ── MCP: the same list and create, for AI clients ────────────────────────────────────────────────

/// Whether two project paths name the same folder, whatever the separators, a trailing one and (on Windows) the case.
fn same_root(a: &str, b: &str) -> bool {
    let clean = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_lowercase();
    clean(a) == clean(b)
}

/// Args for [`bennu_imber_schedules`].
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpListArgs {
    /// Absolute path to the project root.
    pub root: String,
}

/// The Imber schedules of a project: what runs when, whether it is on, the next run and how the last one ended.
#[arbor_rpc::handler(mcp(
    title = "List Imber schedules",
    safety = read,
))]
fn bennu_imber_schedules(_ctx: &BennuState, args: McpListArgs) -> Result<Vec<Job>, String> {
    Ok(store().list().into_iter().filter(|j| same_root(&j.root, &args.root)).collect())
}

/// Args for [`bennu_imber_schedule_create`].
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpCreateArgs {
    /// Absolute path to the project root.
    pub root: String,
    pub name: String,
    /// `prompt`: start a fresh Imber conversation with `prompt` at that time (`awake` does the same: there is no
    /// conversation to wake from here).
    pub action: Option<String>,
    /// What to ask Imber at that time. Self-contained: say what to check and what to report.
    pub prompt: String,
    /// When, in local time: `{"kind":"once","at":"2026-11-05T09:00"}`, `{"kind":"daily","time":"09:00"}`,
    /// `{"kind":"weekly","days":["mon","fri"],"time":"09:00"}`, `{"kind":"every","minutes":30}` or
    /// `{"kind":"cron","expr":"0 0 9 * * 1-5"}`.
    pub when: Value,
    /// `skip` (default) or `run_once`: what to do with a run missed while Bennu was closed.
    pub missed: Option<String>,
}

/// Schedule an Imber run for a project. The run is **read-only**: it reads and searches the project and leaves
/// proposed edits in its conversation for the user to review; it never changes the project, runs commands or asks
/// questions. Bennu must be open at the time. The user can see, edit and turn it off in Imber › Schedules.
#[arbor_rpc::handler(mcp(
    title = "Create an Imber schedule",
    safety = write,
))]
fn bennu_imber_schedule_create(_ctx: &BennuState, args: McpCreateArgs) -> Result<Job, String> {
    let mut input = json!({ "name": args.name, "action": "prompt", "prompt": args.prompt, "when": args.when });
    if let Some(missed) = args.missed {
        input["missed"] = json!(missed);
    }
    let made = run_tool(&args.root, "", "schedule_create", &input)?;
    let id = serde_json::from_str::<Value>(&made).ok().and_then(|v| v["id"].as_str().map(str::to_string)).ok_or("the schedule was not created")?;
    store().get(&id).ok_or_else(|| "the schedule was not created".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_that_went_well_keeps_what_it_said() {
        assert_eq!(outcome_line("done", None, Some("Two files changed on main.".into()), None), (true, "Two files changed on main.".into()));
        assert_eq!(outcome_line("done", None, None, None), (true, "Done.".into()));
        assert_eq!(outcome_line("done", None, Some("  ".into()), None), (true, "Done.".into()));
    }

    #[test]
    fn a_run_that_could_not_go_on_is_incomplete_and_says_why() {
        let needs = outcome_line("done", None, Some("I could not decide.".into()), Some("Needs a person: Rename only the public API?".into()));
        assert_eq!(needs, (false, "Incomplete. Needs a person: Rename only the public API?".into()), "the reason the run gave wins over a good ending");
        assert!(!outcome_line("cancelled", None, None, Some("Stopped: this scheduled run reached its cost cap (1.00 EUR).".into())).1.contains("Stopped before"));
        assert_eq!(outcome_line("failed", Some("No API key is set.".into()), None, None), (false, "No API key is set.".into()));
        assert!(outcome_line("too_many_steps", None, None, None).1.starts_with("Incomplete."));
        assert!(outcome_line("incomplete", None, None, None).1.starts_with("Incomplete."));
        assert_eq!(outcome_line("cancelled", None, None, None), (false, "Stopped before it finished.".into()));
    }

    #[test]
    fn a_job_is_never_claimed_twice_and_is_free_again_when_the_claim_goes() {
        let first = Claim::take("job-claim-test").expect("free");
        assert!(Claim::take("job-claim-test").is_none(), "still going");
        assert!(Claim::take("another-job").is_some(), "other jobs are not in its way");
        drop(first);
        assert!(Claim::take("job-claim-test").is_some(), "free after the run");
    }

    #[test]
    fn the_tools_are_the_four_schedule_tools_and_say_there_is_one_mode() {
        let all = specs();
        let names: Vec<&str> = all.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["schedule_list", "schedule_create", "schedule_update", "schedule_delete"]);
        let create = &all[1];
        assert!(create.description.contains("THIS project") && create.description.contains("Bennu must be open"));
        assert_eq!(create.input_schema["properties"]["action"]["enum"], json!(["awake", "prompt"]));
        assert_eq!(create.input_schema["properties"]["mode"]["enum"], json!(["manual"]));
    }

    #[test]
    fn project_paths_match_whatever_the_spelling() {
        assert!(same_root("C:\\Work\\Shop\\", "c:/work/shop"));
        assert!(!same_root("/a/shop", "/a/shop2"));
    }

    #[test]
    fn a_save_from_the_window_is_always_read_only() {
        let input: JobInput = serde_json::from_value(json!({
            "name": "Morning", "enabled": true, "recurrence": {"kind": "daily", "hour": 9, "minute": 0},
            "action": {"kind": "prompt", "prompt": "status?"}, "root": "/r", "mode": "autonomous"
        }))
        .unwrap();
        let dir = std::env::temp_dir().join(format!("bennu-imber-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::<Action>::new(dir.join("s.json"));
        let made = imber_schedule::prelude::save(&BennuFlavour, &store, input, 1_000).unwrap();
        assert_eq!(made.mode, "manual", "a schedule never gets a mode that writes to the project");
        let _ = std::fs::remove_dir_all(dir);
    }
}
