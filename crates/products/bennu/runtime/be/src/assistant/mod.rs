//! `assistant` domain — Bennu's built-in AI assistant: the handlers the panel calls, and the run that
//! answers them.
//!
//! The pure parts (the provider, the loop, the access policy) are the `bennu-assistant` crate and are
//! tested without a network. What lives here is the glue that needs the editor: the HTTP transport, the
//! tools backed by Bennu's own handlers, the conversations on disk, and the thread a run lives on.
//!
//! A send returns at once with a run id. The run then streams its progress as `arbor://bennu/assistant`
//! events — the message as it stands, usage, questions that need an answer — and ends with `done`. It
//! lives on its own thread because it waits on the user, and a serve-loop worker is not for waiting.

mod compact;
mod git;
mod help;
mod instructions;
mod ledger;
mod models;
mod projects;
mod prompt;
mod pseudonyms;
mod runs;
mod session;
mod store;
mod text_tools;
mod tools;
mod transport;
mod view;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use bennu_assistant::prelude::{
    self as assistant, find_model, mask_key, preview, Anthropic, Block, Effort, Ended, Message, ModelInfo, Policy, Request,
    Level, Limits, Role, Scope, Spent, Usage,
};
use bennu_core::prelude::BennuState;
use serde::Deserialize;
use serde_json::{json, Value};

use self::compact::tokens;
use self::session::Session;
use self::store::{Context, Conversation};
use self::tools::ToolHost;
use self::transport::HttpTransport;
use self::view::View;

/// The provider this build talks to. The vault keeps one key per provider id.
const PROVIDER: &str = "anthropic";
/// Requests one run may make before it is stopped.
const MAX_STEPS: u32 = 40;
const DEFAULT_MAX_TOKENS: u32 = 16_000;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

// ── Pure helpers ─────────────────────────────────────────────────────────────────────────────────

fn parse_scope(word: &str) -> Scope {
    match word {
        "file" => Scope::File,
        "workspace" => Scope::Workspace,
        "disk" => Scope::Disk,
        _ => Scope::Project,
    }
}

/// The user's message as the model receives it: what they typed, preceded by what they were looking at.
fn with_context(text: &str, file: Option<&str>, selection: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(f) = file.filter(|f| !f.is_empty()) {
        out.push_str(&format!("[Open file: {f}]\n"));
    }
    if let Some(sel) = selection.filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("[Selected text]\n```\n{sel}\n```\n"));
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(text);
    out
}

/// Where the tokens of a conversation go, for the context bar: the standing part (instructions and
/// tool definitions), what tools returned, and the rest. An estimate, as the bar says.
fn estimate_context(system: &str, tools: &[bennu_assistant::prelude::ToolSpec], transcript: &[Message]) -> Context {
    let mut project = 0u64;
    let mut conversation = 0u64;
    for m in transcript {
        for b in &m.blocks {
            match b {
                Block::ToolResult { content, .. } => project += tokens(content),
                Block::Text { text } => conversation += tokens(text),
                Block::ToolUse { input, .. } => conversation += tokens(&input.to_string()),
            }
        }
    }
    let tools_json = serde_json::to_string(tools).unwrap_or_default();
    Context { system: tokens(system) + tokens(&tools_json), project, conversation }
}

fn title_from(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let short: String = line.chars().take(60).collect();
    if short.is_empty() { "New conversation".into() } else { short }
}

fn new_id() -> String {
    format!("c-{:x}", now_ms())
}

// ── Handlers ─────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendArgs {
    /// The conversation to continue; absent starts a new one.
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub text: String,
    /// Absolute path of the project the user is working in.
    pub root: String,
    /// The file the user is in, and the text they have selected.
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub selection: Option<String>,
    pub model: String,
    /// `low` … `max`; absent for a model with no such setting.
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Apply edits as they are made, rather than proposing them.
    #[serde(default)]
    pub auto: bool,
    /// `file`, `project`, `workspace` or `disk`.
    #[serde(default)]
    pub scope: String,
    #[serde(default = "yes")]
    pub allow_edits: bool,
    #[serde(default = "yes")]
    pub allow_temp: bool,
    #[serde(default)]
    pub extra_folders: Vec<String>,
    #[serde(default)]
    pub workspace_roots: Vec<String>,
    /// How many open projects one conversation may work across (the one the user is in included); 0 for the default.
    #[serde(default)]
    pub max_projects: usize,
    /// The largest window the user allows. A conversation that reaches 90% of it is compacted
    /// automatically; 0 turns that off.
    #[serde(default)]
    pub max_context: u32,
    /// A chat with no tools: nothing is read, searched or changed.
    #[serde(default)]
    pub chat: bool,
    /// Read the project's CLAUDE.md / AGENTS.md into the instructions.
    #[serde(default = "yes")]
    pub instructions: bool,
    /// Give the assistant the git tools (Corvus's): off unless the user turned it on.
    #[serde(default)]
    pub git: bool,
    /// With git on, show the model made-up ids instead of the people in git results.
    #[serde(default = "yes")]
    pub hide_authors: bool,
    /// Offer the step plan (`update_plan`) and ask for it.
    #[serde(default = "yes")]
    pub plan: bool,
    /// Tool families the user has switched off (`read`, `project`, `edit`, `refactor`, `run`, `history`, `structural`).
    #[serde(default)]
    pub families_off: Vec<String>,
    /// Where today, this week and this month began, in the user's calendar — for the spending limits.
    #[serde(default)]
    pub periods: ledger::Periods,
    /// Build the request it would make instead of sending it.
    #[serde(default)]
    pub dry_run: bool,
}

fn yes() -> bool {
    true
}

fn api_key(ctx: &BennuState) -> Result<String, String> {
    let key = ctx.host_call("__assistant_key", json!(PROVIDER))?;
    match key.as_str().map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => Ok(k.to_string()),
        None => Err("No API key is set. Add one in Settings › Assistant.".into()),
    }
}

/// The projects this run works across: several only when the reach is the workspace or wider; otherwise the
/// project the user is in, and nothing is named or reachable beside it.
fn working_set(args: &SendArgs) -> Vec<projects::Project> {
    let wide = matches!(parse_scope(&args.scope), Scope::Workspace | Scope::Disk);
    let open: &[String] = if wide { &args.workspace_roots } else { &[] };
    projects::working_set(&args.root, open, if wide { args.max_projects } else { 1 })
}

fn build_policy(args: &SendArgs, conversation: &str, projects: &[projects::Project]) -> Policy {
    let path = |s: &String| PathBuf::from(s);
    Policy {
        scope: parse_scope(&args.scope),
        file: args.file.as_ref().map(path),
        project_root: Some(PathBuf::from(&args.root)),
        workspace_roots: projects.iter().map(|p| PathBuf::from(&p.root)).collect(),
        extra_folders: args.extra_folders.iter().map(path).collect(),
        temp_dirs: vec![std::env::temp_dir()],
        allow_temp: args.allow_temp,
        allow_edits: args.allow_edits,
        grants: runs::grants(conversation),
    }
}

fn build_request(args: &SendArgs, transcript: Vec<Message>, git_tools: &[arbor_rpc::tool::ToolDescriptor], projects: &[projects::Project]) -> Request {
    let rendered = if args.instructions { instructions::render(&instructions::load(std::path::Path::new(&args.root))) } else { String::new() };
    let system = prompt::system(&prompt::Context {
        root: &args.root,
        file: args.file.as_deref(),
        auto: args.auto,
        allow_edits: args.allow_edits,
        chat: args.chat,
        instructions: &rendered,
        git: !git_tools.is_empty(),
        hide_authors: args.hide_authors,
        plan: args.plan,
        projects,
    });
    Request {
        model: args.model.clone(),
        system,
        messages: transcript,
        // A chat has only the help about Bennu itself; an agent has everything.
        tools: if args.chat {
            tools::all_specs().into_iter().filter(|t| t.name == "bennu_help").collect()
        } else {
            tools::offered_specs(args.plan, &tools::families_off(&args.families_off), git_tools)
        },
        max_tokens: args.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        effort: args.effort.as_deref().and_then(Effort::parse),
    }
}

fn preview_json(ctx: &BennuState, request: &Request, model: Option<&ModelInfo>) -> Value {
    // The key is only looked at to be masked; a missing one just shows as not set.
    let key = api_key(ctx).unwrap_or_default();
    let p = preview(&Anthropic, request, &key);
    json!({
        "provider": p.provider,
        "method": p.method,
        "url": p.url,
        "headers": p.headers,
        "body": serde_json::to_string_pretty(&p.body).unwrap_or_default(),
        "breakdown": {
            "system": p.breakdown.system, "tools": p.breakdown.tools,
            "messages": p.breakdown.messages, "total": p.breakdown.total(),
        },
        "key": mask_key(&key),
        "estimate": model.map(|m| {
            let b = &p.breakdown;
            // The first request pays full price for the standing part; once it is cached, later ones read it.
            let first = m.cost(&Usage { input_tokens: b.total(), ..Default::default() });
            let next = m.cost(&Usage { input_tokens: b.messages, cache_read_tokens: b.system + b.tools, ..Default::default() });
            json!({ "first": first, "next": next })
        }),
    })
}

/// The spending limits in force, from Bennu's config.
fn limits_now() -> Limits {
    let l = bennu_core::config::load().assistant.limits;
    Limits {
        session_eur: l.session_eur,
        day_eur: l.day_eur,
        week_eur: l.week_eur,
        month_eur: l.month_eur,
        eur_per_usd: l.eur_per_usd,
        warn_percent: l.warn_percent,
    }
}

fn level_word(level: Level) -> &'static str {
    match level {
        Level::Ok => "ok",
        Level::Warn => "warn",
        Level::Reached => "reached",
    }
}

/// Spending against the limits as the panel shows it: a meter per window, and the warning worth saying.
fn spend_json(limits: &Limits, spent: &Spent) -> Value {
    json!({
        "meters": limits.meters(spent).iter().map(|m| json!({
            "period": m.period.word(),
            "spentEur": m.spent_eur,
            "limitEur": m.limit_eur,
            "level": level_word(m.level(limits.warn_percent)),
        })).collect::<Vec<_>>(),
        "warning": limits.warning(spent).map(|(_, text)| text),
        "rate": limits.eur_per_usd,
    })
}

/// Start a run (or, with `dry_run`, show the request it would make).
#[arbor_rpc::handler]
async fn assistant_send(ctx: &BennuState, args: SendArgs) -> Result<Value, String> {
    if args.text.trim().is_empty() {
        return Err("the message is empty".into());
    }
    if args.root.trim().is_empty() {
        return Err("open a project first — the assistant works inside one".into());
    }
    let conversation_id = args.conversation_id.clone().unwrap_or_else(new_id);
    if !store::valid_id(&conversation_id) {
        return Err(format!("`{conversation_id}` is not a conversation id"));
    }
    let dir = store::dir();
    let mut conv = store::load_in(&dir, &conversation_id).unwrap_or_else(|_| Conversation {
        id: conversation_id.clone(),
        model: args.model.clone(),
        chat: args.chat,
        ..Default::default()
    });
    // What kind of conversation this is was decided when it began; the request follows it, whatever the
    // panel says now.
    let mut args = args;
    args.chat = conv.chat;

    // Git is opt-in, and so is hiding people: both only when asked for, and neither in a chat.
    let git_tools = if args.git && !args.chat { git::load(ctx) } else { Vec::new() };
    let hide = args.hide_authors && !git_tools.is_empty();

    let mut transcript = conv.transcript.clone();
    let mut user_text = with_context(&args.text, args.file.as_deref(), args.selection.as_deref());
    if hide {
        // A name the user types that is already a made-up id's owner goes to the provider as the id.
        user_text = pseudonyms::Authors::load().hide_known(&user_text);
    }
    transcript.push(Message::user(user_text));
    let set = working_set(&args);
    let request = build_request(&args, transcript.clone(), &git_tools, &set);

    let (table, _) = models::load();
    let model = find_model(&table, &args.model).cloned();

    if args.dry_run {
        return Ok(json!({ "preview": preview_json(ctx, &request, model.as_ref()) }));
    }

    // The brakes, before anything is spent: a conversation or a period already at its limit does not start.
    let limits = limits_now();
    let baseline = ledger::spent(&args.periods, conv.cost);
    if let Some((_, message)) = limits.breach(&baseline) {
        return Err(format!("{message}. Raise it in Settings › Assistant › Limits to go on."));
    }

    let key = api_key(ctx)?;
    let now = now_ms();
    let user_message = json!({
        "id": format!("u{now}"), "role": "user", "at": now,
        "blocks": [{ "type": "text", "text": args.text }],
    });
    conv.messages.push(user_message);
    conv.transcript = transcript;
    conv.model = args.model.clone();
    conv.updated_at = now;
    if conv.title.trim().is_empty() || conv.title == "New conversation" {
        conv.title = title_from(&args.text);
    }
    store::save_in(&dir, &conv)?;

    let run_id = format!("r{now:x}");
    let cancel = runs::begin(&run_id);
    let mut state = BennuState::new(ctx.event_sink());
    if let Some(host) = ctx.host_caller() {
        state = state.with_host_caller(host);
    }
    let mut session = Session::new(
        run_id.clone(),
        conversation_id.clone(),
        args.root.clone(),
        args.auto,
        ctx.event_sink(),
        state,
        cancel,
        build_policy(&args, &conversation_id, &set),
        View::new(format!("a{now}"), args.model.clone(), now),
    );
    session.hide_authors = hide;
    session.projects = set;
    let session = Arc::new(session);
    let handle = tokio::runtime::Handle::current();

    std::thread::Builder::new()
        .name(format!("assistant-{run_id}"))
        .spawn(move || {
            let guard = tools::Guard { limits, baseline, model: model.clone() };
            execute(session, handle, key, request, conv, RunOptions { model, max_context: args.max_context, periods: args.periods, guard, chat: args.chat, git: git_tools, plan: args.plan, off: tools::families_off(&args.families_off) });
        })
        .map_err(|e| format!("could not start the run: {e}"))?;

    Ok(json!({ "run_id": run_id, "conversation_id": conversation_id }))
}

/// What a run needs besides the conversation: the model's prices, the window it may fill, and the limits.
struct RunOptions {
    model: Option<ModelInfo>,
    max_context: u32,
    periods: ledger::Periods,
    guard: tools::Guard,
    chat: bool,
    git: Vec<arbor_rpc::tool::ToolDescriptor>,
    plan: bool,
    off: tools::Off,
}

/// The run itself: the loop, then saving what it produced.
fn execute(
    session: Arc<Session>,
    handle: tokio::runtime::Handle,
    key: String,
    request: Request,
    mut conv: Conversation,
    options: RunOptions,
) {
    let RunOptions { model, max_context, periods, guard, chat, git, plan, off } = options;
    let result = match HttpTransport::new() {
        Ok(transport) => {
            let compaction = (max_context > 0).then(|| tools::Compaction {
                key: key.clone(),
                model: request.model.clone(),
                threshold: u64::from(max_context) * 9 / 10,
            });
            let host = ToolHost::new(Arc::clone(&session), handle, compaction, Some(guard), chat, git, plan, off);
            assistant::run(&Anthropic, &transport, &host, &key, request.clone(), MAX_STEPS)
        }
        Err(e) => Err(e),
    };

    let mut error: Option<String> = None;
    let mut ended = "done";
    // Priced request by request: a long prompt costs more per token, which a total cannot tell.
    let mut requests: Vec<Usage> = Vec::new();
    match result {
        Ok(outcome) => {
            requests = outcome.requests;
            ended = match outcome.ended {
                Ended::Done => "done",
                Ended::Cancelled => "cancelled",
                Ended::TooManySteps => "too_many_steps",
                Ended::Incomplete(_) => "incomplete",
            };
            // The whole conversation as the run left it: it may have been compacted on the way.
            conv.transcript = outcome.messages;
        }
        Err(e) => error = Some(e),
    }
    if let Some(e) = &error {
        session.view().push(json!({ "type": "notice", "level": "error", "text": e }));
    } else if ended == "too_many_steps" {
        session.view().push(json!({ "type": "notice", "level": "warning", "text": format!("Stopped after {MAX_STEPS} steps. Say \"continue\" to go on.") }));
    } else if ended == "incomplete" {
        session.view().push(json!({ "type": "notice", "level": "warning", "text": "The answer was cut short — the model reached its length limit or declined." }));
    }

    if !session.view().is_empty() {
        conv.messages.push(session.display_snapshot());
    }
    // What compacting cost is part of what the conversation cost.
    requests.extend(session.extra_requests.lock().unwrap_or_else(|p| p.into_inner()).iter().copied());
    if let Some(m) = &model {
        let run_usd = m.cost_of_all(&requests);
        conv.cost += run_usd;
        // Into the ledger, so the day, week and month limits see it even after the conversation is gone.
        if run_usd > 0.0 {
            let entry = ledger::Entry { ts: now_ms(), conv: conv.id.clone(), model: m.id.clone(), usd: run_usd };
            if let Err(e) = ledger::append(&entry) {
                session.view().push(json!({ "type": "notice", "level": "warning", "text": format!("The cost could not be recorded for the spending limits: {e}") }));
            }
        }
    }
    let limits = limits_now();
    let spent_now = ledger::spent(&periods, conv.cost);
    conv.context = estimate_context(&request.system, &request.tools, &conv.transcript);
    conv.updated_at = now_ms();
    let saved = store::save_in(&store::dir(), &conv);

    session.push_message();
    session.emit(
        "done",
        json!({
            "ended": ended,
            "error": error,
            "save_error": saved.err(),
            "cost": conv.cost,
            "context": { "system": conv.context.system, "project": conv.context.project, "conversation": conv.context.conversation },
            "title": conv.title,
            "updated_at": conv.updated_at,
            "spend": spend_json(&limits, &spent_now),
        }),
    );
    runs::finish(&session.run_id);
}

/// Stop a run. It ends at the next chunk or the next tool, and keeps what it had.
#[arbor_rpc::handler]
fn assistant_cancel(_ctx: &BennuState, run_id: String) -> Result<bool, String> {
    Ok(runs::cancel(&run_id))
}

/// Answer a question a run is waiting on — a set of questions, an access request, a confirmation.
#[arbor_rpc::handler]
fn assistant_reply(_ctx: &BennuState, prompt_id: String, answer: Value) -> Result<bool, String> {
    Ok(runs::reply(&prompt_id, answer))
}

/// Apply or reject the edits a run proposed. Written through the same door as a save, so Local History
/// keeps what the files were.
#[arbor_rpc::handler]
fn assistant_decide(ctx: &BennuState, conversation_id: String, proposal_id: String, apply: bool) -> Result<Vec<String>, String> {
    let written = tools::decide(&proposal_id, apply, |method, params| {
        let call = arbor_rpc::registry_for("").get(method).copied().ok_or_else(|| format!("unknown method `{method}`"))?;
        call(ctx as &dyn std::any::Any, params)
    })?;

    // Record the decision in the saved conversation, so the card is right after a restart.
    let dir = store::dir();
    if let Ok(mut conv) = store::load_in(&dir, &conversation_id) {
        let status = if apply { "applied" } else { "rejected" };
        for m in &mut conv.messages {
            let Some(blocks) = m.get_mut("blocks").and_then(Value::as_array_mut) else { continue };
            for b in blocks {
                if b.pointer("/proposal/id").and_then(Value::as_str) == Some(proposal_id.as_str()) {
                    b["proposal"]["status"] = json!(status);
                }
            }
        }
        store::save_in(&dir, &conv)?;
    }
    Ok(written)
}

#[derive(Debug, Deserialize)]
pub struct CompactArgs {
    pub id: String,
    /// What the summary must keep or stress — the text after /compact in Claude Code.
    #[serde(default)]
    pub instructions: Option<String>,
    pub model: String,
}

/// Fold the early part of a conversation into a note written by the model, after clearing old tool
/// output. Instructions say what the note must keep. Returns what was freed, the context afterwards,
/// the running cost and the conversation's messages (with a notice appended).
#[arbor_rpc::handler]
fn assistant_compact(ctx: &BennuState, args: CompactArgs) -> Result<Value, String> {
    let dir = store::dir();
    let mut conv = store::load_in(&dir, &args.id)?;
    let key = api_key(ctx)?;
    let transport = HttpTransport::new()?;
    let size = |m: &[Message]| tokens(&serde_json::to_string(m).unwrap_or_default());
    let before = size(&conv.transcript);

    let done = compact::compact(&conv.transcript, &transport, &key, &args.model, args.instructions.as_deref(), true, &size, 0)?;
    conv.transcript = done.transcript;
    let rebuilt = estimate_context("", &[], &conv.transcript);
    // The standing part (instructions and tools) is unchanged by this; only the rest is recounted.
    conv.context = Context { system: conv.context.system, project: rebuilt.project, conversation: rebuilt.conversation };
    let (table, _) = models::load();
    if done.summarized {
        if let Some(m) = find_model(&table, &args.model) {
            conv.cost += m.cost(&done.usage);
        }
    }
    conv.messages.push(json!({
        "id": format!("n{}", now_ms()), "role": "assistant", "at": now_ms(), "model": args.model,
        "blocks": [{ "type": "notice", "level": "info", "text": if done.summarized { "Conversation compacted: the earlier part is now a summary." } else { "Nothing earlier to summarise yet." } }],
    }));
    store::save_in(&dir, &conv)?;
    Ok(json!({
        "freed": before.saturating_sub(size(&conv.transcript)),
        "summarized": done.summarized,
        "cost": conv.cost,
        "context": { "system": conv.context.system, "project": conv.context.project, "conversation": conv.context.conversation },
        "messages": conv.messages,
    }))
}

/// The models on offer with their windows and base prices — the built-in table overlaid with the user's
/// `assistant-models.toml`. `file` is where that file goes; `error` says what is wrong with it, if it is.
/// Spending against the limits right now: for the Settings page and the panel's meter. `conversation_id`
/// is the one open, whose own cost is the session figure.
#[arbor_rpc::handler]
fn assistant_spend(_ctx: &BennuState, periods: ledger::Periods, conversation_id: Option<String>) -> Result<Value, String> {
    let session_usd = conversation_id
        .and_then(|id| store::load_in(&store::dir(), &id).ok())
        .map(|c| c.cost)
        .unwrap_or(0.0);
    let limits = limits_now();
    Ok(spend_json(&limits, &ledger::spent(&periods, session_usd)))
}

/// The instruction files that would be read for `root`, for the composer to show. The text itself is not
/// returned: the panel only needs to say which files are in play.
#[arbor_rpc::handler]
fn assistant_instructions(_ctx: &BennuState, root: String) -> Result<Value, String> {
    let list = instructions::load(std::path::Path::new(&root));
    Ok(json!(list.iter().map(|i| json!({ "name": i.name, "bytes": i.text.len(), "truncated": i.truncated })).collect::<Vec<_>>()))
}

#[arbor_rpc::handler]
fn assistant_models(_ctx: &BennuState) -> Result<Value, String> {
    let (table, error) = models::load();
    Ok(json!({
        "models": table.iter().map(models::wire).collect::<Vec<_>>(),
        "file": models::user_path().display().to_string(),
        "error": error,
    }))
}

/// Create the user's model file from the built-in table, if there is none, and say where it is.
#[arbor_rpc::handler]
fn assistant_models_template(_ctx: &BennuState) -> Result<String, String> {
    models::write_template().map(|p| p.display().to_string())
}

#[arbor_rpc::handler]
fn assistant_list(_ctx: &BennuState) -> Result<Vec<store::Summary>, String> {
    Ok(store::list_in(&store::dir()))
}

/// A conversation as the panel draws it (without the model-side transcript).
#[arbor_rpc::handler]
fn assistant_load(_ctx: &BennuState, id: String) -> Result<Value, String> {
    let mut c = serde_json::to_value(store::load_in(&store::dir(), &id)?).map_err(|e| e.to_string())?;
    if let Some(o) = c.as_object_mut() {
        o.remove("transcript");
    }
    Ok(c)
}

#[arbor_rpc::handler]
fn assistant_delete(_ctx: &BennuState, id: String) -> Result<(), String> {
    store::delete_in(&store::dir(), &id)
}

#[arbor_rpc::handler]
fn assistant_rename(_ctx: &BennuState, id: String, title: String) -> Result<(), String> {
    let dir = store::dir();
    let mut c = store::load_in(&dir, &id)?;
    let clean = title.trim();
    if clean.is_empty() {
        return Err("the title is empty".into());
    }
    c.title = clean.to_string();
    store::save_in(&dir, &c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_assistant::prelude::{Provider, ToolSpec};

    #[test]
    fn the_message_carries_what_the_user_was_looking_at() {
        let m = with_context("rename it", Some("src/A.java"), Some("int total;"));
        assert!(m.starts_with("[Open file: src/A.java]"));
        assert!(m.contains("```\nint total;\n```"));
        assert!(m.ends_with("rename it"));
        assert_eq!(with_context("hi", None, Some("   ")), "hi", "an empty selection adds nothing");
    }

    #[test]
    fn a_title_is_the_first_line_and_bounded() {
        assert_eq!(title_from("\n\n  Add a currency\nmore"), "Add a currency");
        assert_eq!(title_from(&"x".repeat(200)).chars().count(), 60);
        assert_eq!(title_from("   "), "New conversation");
    }

    #[test]
    fn the_context_estimate_separates_tool_output_from_talk() {
        let transcript = vec![
            Message::user("x".repeat(400)),
            Message { role: Role::User, blocks: vec![Block::ToolResult { tool_use_id: "t".into(), content: "y".repeat(800), is_error: false }] },
        ];
        let tools = vec![ToolSpec { name: "n".into(), description: "d".into(), input_schema: json!({}) }];
        let c = estimate_context("system", &tools, &transcript);
        assert_eq!(c.conversation, 100);
        assert_eq!(c.project, 200);
        assert!(c.system > 0);
    }

    #[test]
    fn scope_words_map_and_unknown_means_project() {
        assert_eq!(parse_scope("file"), Scope::File);
        assert_eq!(parse_scope("disk"), Scope::Disk);
        assert_eq!(parse_scope(""), Scope::Project);
        assert_eq!(parse_scope("nonsense"), Scope::Project);
    }

    #[test]
    fn the_offered_tools_are_the_same_list_every_time_and_have_no_shell() {
        let a: Vec<String> = tools::all_specs().into_iter().map(|t| t.name).collect();
        let b: Vec<String> = tools::all_specs().into_iter().map(|t| t.name).collect();
        assert_eq!(a, b);
        for needed in ["search_text", "find_files", "edit_file", "create_file", "delete_file", "ask_user"] {
            assert!(a.iter().any(|n| n == needed), "{needed}");
        }
        assert!(!a.iter().any(|n| n.contains("shell") || n.contains("bash") || n == "bennu_write_file"));
        let _ = Anthropic.id();
    }
}
