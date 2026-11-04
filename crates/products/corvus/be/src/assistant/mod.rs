//! `assistant` domain — Imber in Corvus: the handlers the panel calls, and the run that answers them.
//!
//! Imber is the assistant of the whole suite. Its pure core (the provider, the agent loop, retry, the model
//! price list, the message view) is `imber-core`, its network is `imber-transport`, its settings are the
//! `[assistant]` table of Bennu's config and its API key is the one in the shell's vault — all shared, none
//! copied. What lives here is what is Corvus's own: the git tools ([`tools`]), the rule for what may run
//! unasked ([`permission`]), the instructions ([`prompt`]) and the host that plugs those into the loop
//! ([`host`]).
//!
//! A send returns at once with a run id; the run then streams `arbor://corvus/imber` events — the message as
//! it stands, usage, an approval waiting for an answer — and ends with `done`. It lives on its own thread
//! because it waits on the user, and a serve-loop worker is not for waiting.

mod commit_message;
mod diff_text;
mod history;
mod host;
mod permission;
mod plan_tools;
mod prompt;
mod runs;
mod schedule;
mod settings;
mod tools;
mod undo;

use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::get_workdir_diff;
use imber_core::prelude::{
    resolve_side_model, run as run_agent, Anthropic, Effort, Ended, Host, Message, ModelInfo, ProviderError, Request, SideJob, ToolSpec, View,
};
use imber_transport::prelude::HttpTransport;
use serde::Deserialize;
use serde_json::{json, Value};

use self::host::{ImberHost, Session};
use self::permission::Mode;
use crate::ephemeral::tab_for;
use crate::repo::{open, repo_path};

/// Requests one run may make before it is stopped.
const MAX_STEPS: u32 = 40;
const MAX_TOKENS: u32 = 16_000;
/// A commit message is a few lines.
const COMMIT_MESSAGE_TOKENS: u32 = 1_000;
/// Characters of the staged diff that reach the model when writing a commit message.
const COMMIT_DIFF_BUDGET: usize = 30_000;
/// Recent subjects read to learn the repository's commit style.
const STYLE_SAMPLE: usize = 30;

/// The process-wide state, handed in at startup: a run outlives the handler that started it, so it cannot
/// borrow the handler's `&CorvusState`.
static STATE: OnceLock<Arc<CorvusState>> = OnceLock::new();

pub(crate) fn init(state: Arc<CorvusState>) {
    let _ = STATE.set(state);
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn repo_name(root: &str) -> String {
    std::path::Path::new(root).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.to_string())
}

fn error_notice(e: &ProviderError) -> Value {
    json!({ "type": "notice", "level": "error", "text": e.friendly(), "detail": e.detail, "kind": e.kind.code(), "retryable": e.retryable(), "canRetry": true })
}

fn ended_word(ended: Ended) -> &'static str {
    match ended {
        Ended::Done => "done",
        Ended::Cancelled => "cancelled",
        Ended::TooManySteps => "too_many_steps",
        Ended::Incomplete(_) => "incomplete",
        Ended::Failed => "failed",
    }
}

// ── Conversation ─────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendArgs {
    /// `None` starts a conversation.
    pub conversation_id: Option<String>,
    pub text: String,
    /// The repository this conversation is about.
    pub root: String,
    /// A conversation with no tools.
    #[serde(default)]
    pub chat: bool,
    /// Send the conversation again as it stands (a run that failed), with no new message.
    #[serde(default)]
    pub retry: bool,
    /// `manual` or `auto`; the saved mode when absent.
    pub mode: Option<String>,
}

/// Start (or continue) a conversation: returns at once with the run and conversation ids.
#[arbor_rpc::handler]
async fn imber_send(state: &CorvusState, args: SendArgs) -> Result<Value, String> {
    let shared = STATE.get().cloned().ok_or("Imber is not ready yet")?;
    if args.root.trim().is_empty() {
        return Err("Open a repository first: Imber works on the repository in the active tab.".into());
    }
    let mode = args.mode.as_deref().map_or_else(|| settings::mode(state), Mode::parse);
    let start = Start { conversation_id: args.conversation_id, text: args.text, root: args.root, chat: args.chat, retry: args.retry, mode, unattended: false, announce: None };
    let (run_id, conversation_id) = begin_run(state, shared, tokio::runtime::Handle::current(), start, None)?;
    Ok(json!({ "run_id": run_id, "conversation_id": conversation_id }))
}

/// Everything a run is started from. The panel's send and a scheduled job both build one.
pub(crate) struct Start {
    pub conversation_id: Option<String>,
    pub text: String,
    pub root: String,
    pub chat: bool,
    pub retry: bool,
    pub mode: Mode,
    /// Nobody is there to approve: a write runs only in auto mode and a destructive action never runs.
    pub unattended: bool,
    /// Shown in the thread before the run, as what woke it (`Scheduled: Morning check`).
    pub announce: Option<String>,
}

/// Called once when the run is over: whether it ended well, and a line about it.
pub(crate) type DoneHook = Box<dyn FnOnce(bool, String) + Send>;

/// Check, record and launch one run on its own thread.
pub(crate) fn begin_run(state: &CorvusState, shared: Arc<CorvusState>, handle: tokio::runtime::Handle, args: Start, on_done: Option<DoneHook>) -> Result<(String, String), String> {
    // Fails early, in words, when the folder is not a repository.
    // The repository's own spelling of its path: what the action log and the schedules key on.
    let root = repo_path(state, &tab_for(state, &args.root)?)?;

    let cfg = settings::assistant();
    let model = settings::model_info(&cfg.model);
    let conversation_id = args.conversation_id.clone().filter(|c| !c.is_empty()).unwrap_or_else(|| format!("c-{:x}", now_ms()));

    // The brake, before anything is spent: a conversation already at its limit does not go on.
    let spent_eur = runs::cost_usd(&conversation_id) * cfg.limits.eur_per_usd;
    if cfg.limits.session_eur > 0.0 && spent_eur >= cfg.limits.session_eur {
        return Err(format!("This conversation has reached its spending limit ({:.2} EUR). Raise it in Settings \u{203a} Imber \u{203a} Spending limits, or start a new conversation.", cfg.limits.session_eur));
    }
    let key = settings::api_key(state)?;

    let mut transcript = runs::transcript(&conversation_id);
    if args.retry {
        if transcript.is_empty() {
            return Err("There is nothing to retry.".into());
        }
    } else {
        if args.text.trim().is_empty() {
            return Err("The message is empty.".into());
        }
        transcript.push(Message::user(args.text.clone()));
    }

    // One run per conversation: two at once would interleave their messages in one transcript.
    if !runs::claim_conversation(&conversation_id) {
        return Err("Imber is already working in this conversation.".into());
    }

    let mode = args.mode;
    let system = prompt::system(&prompt::Context { repo_name: &repo_name(&root), root: &root, mode, chat: args.chat });
    let request = Request {
        model: cfg.model.clone(),
        system,
        messages: transcript,
        tools: Vec::<ToolSpec>::new(), // the loop asks the host for them before every request
        max_tokens: MAX_TOKENS,
        effort: Effort::parse(&cfg.effort),
        long_cache: cfg.long_cache,
    };

    let run_id = format!("r{:x}", now_ms());
    let cancel = runs::begin(&run_id);
    let session = Arc::new(Session::new(run_id.clone(), conversation_id.clone(), state.event_sink(), cancel, View::new(format!("a{}", now_ms()), cfg.model.clone(), now_ms())));
    if let Some(note) = &args.announce {
        session.emit("scheduled", json!({ "note": note, "text": args.text }));
    }
    let host = ImberHost::new(Arc::clone(&session), shared, handle, root.clone(), conversation_id.clone(), mode, args.chat, args.unattended);

    let claimed = conversation_id.clone();
    std::thread::Builder::new()
        .name(format!("imber-{run_id}"))
        .spawn(move || {
            execute(Arc::clone(&session), host, key, request, model, on_done);
            runs::release_conversation(&claimed);
        })
        .map_err(|e| {
            runs::finish(&run_id);
            runs::release_conversation(&conversation_id);
            format!("could not start the run: {e}")
        })?;
    Ok((run_id, conversation_id))
}

/// The run itself: the loop, then saving what it produced.
fn execute(session: Arc<Session>, host: ImberHost, key: String, request: Request, model: Option<ModelInfo>, on_done: Option<DoneHook>) {
    let outcome = match HttpTransport::new() {
        Ok(transport) => run_agent(&Anthropic, &transport, &host, &key, request.clone(), MAX_STEPS),
        Err(e) => imber_core::prelude::Outcome::failed_to_start(request.messages.clone(), ProviderError::local(e)),
    };

    // Whatever the loop left — after a failure, every turn that did complete — is the conversation now, so
    // "Retry" sends the same request again from there.
    runs::save_transcript(&session.conversation_id, outcome.messages);
    if let Some(e) = &outcome.error {
        session.view().push(error_notice(e));
    } else if outcome.ended == Ended::TooManySteps {
        session.view().push(json!({ "type": "notice", "level": "warning", "text": format!("Stopped after {MAX_STEPS} steps. Say \"continue\" to go on.") }));
    } else if matches!(outcome.ended, Ended::Incomplete(_)) {
        session.view().push(json!({ "type": "notice", "level": "warning", "text": "The answer was cut short — the model reached its length limit or declined." }));
    }

    // Priced request by request: a long prompt costs more per token, which a total cannot tell.
    let run_usd = model.as_ref().map_or(0.0, |m| m.cost_of_all(&outcome.requests));
    let conversation_usd = runs::add_cost(&session.conversation_id, run_usd);
    let empty = session.view().is_empty();
    if !empty {
        session.push_message();
    }
    session.emit("done", json!({ "ended": ended_word(outcome.ended), "cost_usd": run_usd, "conversation_cost_usd": conversation_usd, "priced": model.is_some() }));
    runs::finish(&session.run_id);
    if let Some(hook) = on_done {
        let ok = outcome.error.is_none() && outcome.ended == Ended::Done;
        let detail = match &outcome.error {
            Some(e) => e.friendly(),
            None => outcome.added.iter().rev().map(Message::text).find(|t| !t.trim().is_empty()).unwrap_or_else(|| ended_word(outcome.ended).to_string()),
        };
        hook(ok, detail);
    }
}

/// Stop a run. It ends at the next chunk or the next tool, and keeps what it had.
#[arbor_rpc::handler]
fn imber_cancel(_state: &CorvusState, run_id: String) -> Result<bool, String> {
    Ok(runs::cancel(&run_id))
}

/// Answer an approval card: `{ "answer": "approve" | "reject" }`.
#[arbor_rpc::handler]
fn imber_reply(_state: &CorvusState, prompt_id: String, answer: Value) -> Result<bool, String> {
    Ok(runs::reply(&prompt_id, answer))
}

/// Forget a conversation's transcript (the user started a new one).
#[arbor_rpc::handler]
fn imber_forget(_state: &CorvusState, conversation_id: String) -> Result<(), String> {
    runs::forget(&conversation_id);
    Ok(())
}

// ── Settings the panel shows ─────────────────────────────────────────────────────────────────────

/// What the panel needs to draw its header: the shared model and effort, whether a key is set, Corvus's mode.
#[arbor_rpc::handler]
fn imber_state(state: &CorvusState) -> Result<Value, String> {
    let cfg = settings::assistant();
    let model = settings::model_info(&cfg.model);
    Ok(json!({
        "hasKey": settings::api_key(state).is_ok(),
        "model": cfg.model,
        "modelName": model.map(|m| m.name).unwrap_or_else(|| cfg.model.clone()),
        "effort": cfg.effort,
        "mode": settings::mode(state).word(),
        "sessionLimitEur": cfg.limits.session_eur,
    }))
}

/// Remember the mode (`manual` or `auto`) for the next conversation.
#[arbor_rpc::handler]
fn imber_set_mode(state: &CorvusState, mode: String) -> Result<String, String> {
    let mode = Mode::parse(&mode);
    settings::set_mode(state, mode)?;
    Ok(mode.word().to_string())
}

// ── Commit message ───────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CommitMessageArgs {
    pub root: String,
    /// What is in the message box already, or a hint; empty for none.
    #[serde(default)]
    pub hint: String,
    /// Names this request so `imber_cancel` can stop it.
    pub request_id: String,
}

/// What a commit message is written from, read off the repository.
struct CommitInputs {
    diff: String,
    files: Vec<String>,
    recent_subjects: Vec<String>,
    template: Option<String>,
    branch: String,
}

fn gather_commit_inputs(state: &CorvusState, root: &str) -> Result<CommitInputs, String> {
    let tab = tab_for(state, root)?;
    let template = crate::stage::get_git_commit_template(state, tab.clone()).ok().flatten();
    let repo = open(state, &tab)?;
    let files = get_workdir_diff(&repo, true, 3, None, None).map_err(|e| e.to_string())?;
    if files.is_empty() {
        return Err("Nothing is staged, so there is nothing to describe. Stage the files first.".into());
    }
    let paths = files.iter().map(|f| f.path.clone()).collect();
    let recent_subjects = crate::mcp_tools::walk_log(&repo, None, STYLE_SAMPLE).map(|l| l.into_iter().map(|e| e.summary).collect()).unwrap_or_default();
    let branch = repo.head().ok().and_then(|h| h.shorthand().map(str::to_string)).unwrap_or_default();
    Ok(CommitInputs { diff: diff_text::render(&files, COMMIT_DIFF_BUDGET), files: paths, recent_subjects, template, branch })
}

/// A request with no tools, answered once: the host for jobs like the commit message.
struct OneShot {
    cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl Host for OneShot {
    fn tools(&self) -> Vec<ToolSpec> {
        Vec::new()
    }
    fn gate(&self, _call: &imber_core::prelude::ToolCall) -> imber_core::prelude::Gate {
        imber_core::prelude::Gate::Refuse("No tools are available for this request.".into())
    }
    fn run(&self, _call: &imber_core::prelude::ToolCall) -> Result<String, String> {
        Err("No tools are available for this request.".into())
    }
    fn event(&self, _event: imber_core::prelude::AgentEvent) {}
    fn cancelled(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Generate a commit message for what is staged. Fills the message box; **never commits**.
#[arbor_rpc::handler]
async fn imber_commit_message(state: &CorvusState, args: CommitMessageArgs) -> Result<String, String> {
    let cfg = settings::assistant();
    let key = settings::api_key(state)?;
    let inputs = gather_commit_inputs(state, &args.root)?;
    // A commit message does not need the conversation's large model: the side-job model (a cheap one by default).
    let model = resolve_side_model(SideJob::Compaction, &cfg.side_model, &settings::models(), &cfg.model);

    let user = commit_message::user_prompt(&commit_message::Inputs {
        diff: &inputs.diff,
        files: &inputs.files,
        recent_subjects: &inputs.recent_subjects,
        template: inputs.template.as_deref(),
        branch: &inputs.branch,
        hint: &args.hint,
    });
    let request = Request {
        model,
        system: commit_message::SYSTEM.to_string(),
        messages: vec![Message::user(user)],
        tools: Vec::new(),
        max_tokens: COMMIT_MESSAGE_TOKENS,
        effort: None,
        long_cache: false,
    };
    let cancel = runs::begin(&args.request_id);
    let request_id = args.request_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        let transport = HttpTransport::new()?;
        let outcome = run_agent(&Anthropic, &transport, &OneShot { cancel }, &key, request, 1);
        match (outcome.ended, outcome.error) {
            (Ended::Cancelled, _) => Err("cancelled".to_string()),
            (_, Some(e)) => Err(e.friendly()),
            _ => Ok(outcome.added.iter().map(Message::text).collect::<Vec<_>>().join("")),
        }
    })
    .await
    .map_err(|e| e.to_string());
    runs::finish(&request_id);
    let text = commit_message::clean_reply(&result??);
    if text.is_empty() { Err("The model returned no message. Try again.".into()) } else { Ok(text) }
}

// ── What Imber did, and taking it back ───────────────────────────────────────────────────────────

/// Imber's actions in a repository, newest first, each with how it can be undone — or why it cannot.
#[arbor_rpc::handler]
fn imber_actions(state: &CorvusState, root: String) -> Result<Vec<undo::Action>, String> {
    let root = repo_path(state, &tab_for(state, &root)?)?;
    Ok(undo::list(state, &root))
}

/// Undo Imber's last reversible action in a repository, or the one named. Returns what was done.
#[arbor_rpc::handler]
fn imber_undo(state: &CorvusState, root: String, action_id: Option<String>) -> Result<String, String> {
    let root = repo_path(state, &tab_for(state, &root)?)?;
    undo::undo(state, &root, action_id.as_deref())
}
