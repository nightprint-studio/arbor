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

mod apply;
#[cfg(test)]
mod cache_audit;
mod compact;
mod decisions;
mod digest_store;
mod git;
mod help;
mod instructions;
mod ledger;
mod live_log;
mod models;
mod outline;
mod projects;
mod prompt;
mod prompt_agents;
mod pseudonyms;
mod read_memory;
mod runs;
mod session;
mod side_jobs;
mod skills;
mod store;
pub(crate) mod stop_watch;
mod undo;
mod subagents;
mod text_tools;
mod token_report;
mod tools;
mod transport;
mod view;
mod worktree;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use bennu_assistant::prelude::{
    self as assistant, find_model, AccessMode, mask_key, preview, Anthropic, Block, Effort, Ended, Message, ModelInfo, Policy, Request,
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
                Block::ToolImage { tokens, .. } => project += u64::from(*tokens),
                Block::Text { text } => conversation += tokens(text),
                Block::ToolUse { input, .. } => conversation += tokens(&input.to_string()),
            }
        }
    }
    let tools_json = serde_json::to_string(tools).unwrap_or_default();
    Context { system: tokens(system) + tokens(&tools_json), project, conversation, ..Default::default() }
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
    /// Send the conversation again as it stands — a run that failed is picked up where it stopped, with no new
    /// message from the user.
    #[serde(default)]
    pub retry: bool,
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
    /// `manual`, `accept_edits` or `autonomous` (the old `auto` reads as `accept_edits`).
    #[serde(default)]
    pub mode: String,
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
    /// `shared` (default: builds queue in the project's target) or `separate` (the conversation compiles in a
    /// target of its own). Honoured only on a conversation's first message; after that its record decides.
    #[serde(default)]
    pub build_isolation: String,
    /// The kind of isolated copy to work in (`copy` or `git`), honoured only on a conversation's first message: after
    /// that the conversation's own record decides, like `chat`.
    #[serde(default)]
    pub worktree: Option<String>,
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
    /// Let the assistant change a running Bevy game (BRP writes). Off unless the user turned it on.
    #[serde(default)]
    pub bevy_write: bool,
    /// Let the assistant change the program it debugs (throw, force a return, drop a frame, write a value). Off unless
    /// the user turned it on.
    #[serde(default)]
    pub debug_mutate: bool,
    /// The skills the user enabled, as `<scope>/<name>` ids. None until they choose: the allow-list is the
    /// only way a skill reaches the model.
    #[serde(default)]
    pub skills_on: Vec<String>,
    /// Where today, this week and this month began, in the user's calendar — for the spending limits.
    #[serde(default)]
    pub periods: ledger::Periods,
    /// Build the request it would make instead of sending it.
    #[serde(default)]
    pub dry_run: bool,
    /// The user's default decision level and notes (settings), and this conversation's own (empty: inherit).
    /// The conversation's are kept with it; they apply from this message on.
    #[serde(default)]
    pub decision_level: String,
    #[serde(default)]
    pub decision_notes: String,
    #[serde(default)]
    pub chat_decision_level: String,
    #[serde(default)]
    pub chat_decision_notes: String,
    /// Minutes an unused debug session of the assistant lives (0: the default).
    #[serde(default)]
    pub debug_idle_minutes: u32,
    /// Sub-agents: the user's default (settings), and this conversation's own word (`on`, `off`; empty follows the setting).
    #[serde(default)]
    pub subagents_enabled: bool,
    #[serde(default)]
    pub chat_subagents: String,
}

fn yes() -> bool {
    true
}

impl SendArgs {
    fn access_mode(&self) -> AccessMode {
        AccessMode::parse(&self.mode)
    }

    /// Sub-agents are offered to this run: switched on (the conversation's choice, else the setting) and not a chat.
    /// Once the run's tool families are settled the answer is read off them, so the prompt and the tools agree.
    fn agents_on(&self) -> bool {
        !self.chat && !self.families_off.iter().any(|f| f == "agents")
    }

    /// The decision level this run follows: the conversation's own when it has one, else the setting.
    fn decision_level_effective(&self) -> bennu_assistant::prelude::DecisionLevel {
        let word = if self.chat_decision_level.trim().is_empty() { &self.decision_level } else { &self.chat_decision_level };
        bennu_assistant::prelude::DecisionLevel::parse(word)
    }

    /// The closing prompt paragraph for the decision level and both notes.
    fn decision_paragraph(&self) -> String {
        let notes = bennu_assistant::prelude::merge_notes(&self.decision_notes, &self.chat_decision_notes);
        bennu_assistant::prelude::decision_paragraph(self.decision_level_effective(), &notes)
    }
}

fn api_key(ctx: &BennuState) -> Result<String, String> {
    let key = ctx.host_call("__assistant_key", json!(PROVIDER))?;
    match key.as_str().map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => Ok(k.to_string()),
        None => Err("No API key is set. Add one in Settings › Imber.".into()),
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
        delete_unasked: args.access_mode().applies_edits(),
        grants: runs::grants(conversation),
        run_files: Vec::new(),
    }
}

/// The skills this run may use: discovered under the projects in play and the user's folders, then narrowed to the
/// ones the user enabled. None in a chat, which has no `load_skill` to read them with.
fn enabled_skills(args: &SendArgs, projects: &[projects::Project]) -> Vec<skills::Skill> {
    if args.chat || args.skills_on.is_empty() {
        return Vec::new();
    }
    let roots: Vec<PathBuf> = projects.iter().map(|p| PathBuf::from(&p.root)).collect();
    let (user_bennu, user_claude) = skills::user_sources();
    let found = skills::discover(&skills::sources(&roots, user_bennu, user_claude));
    skills::enabled(&found.skills, &args.skills_on)
}

fn build_request(args: &SendArgs, transcript: Vec<Message>, git_tools: &[arbor_rpc::tool::ToolDescriptor], projects: &[projects::Project], skills: &[skills::Skill]) -> Request {
    let (rendered, skills_index) = token_report::standing_texts(args.instructions, &args.root, skills);
    let debug_on = !args.chat && !tools::families_off(&args.families_off).hides("debug_launch");
    let off = tools::families_off(&args.families_off);
    let (bevy_on, bevy_write_on, shots_on) = (!args.chat && !off.hides("brp_query"), !args.chat && !off.hides("brp_mutate"), !args.chat && !off.hides("screenshot_page"));
    let system = prompt_agents::with_agents(prompt::with_decisions(prompt::with_screenshots(prompt::with_bevy(prompt::with_debug(prompt::system(&prompt::Context {
        root: &args.root,
        mode: args.access_mode(),
        allow_edits: args.allow_edits,
        chat: args.chat,
        instructions: &rendered,
        skills: &skills_index,
        git: !git_tools.is_empty(),
        hide_authors: args.hide_authors,
        plan: args.plan,
        projects,
        live: !args.chat && !tools::families_off(&args.families_off).hides("run_start"),
        worktree: args.worktree.as_deref().and_then(bennu_assistant::prelude::WorktreeMode::parse),
    }), debug_on), bevy_on, bevy_write_on), shots_on), &if args.chat { String::new() } else { args.decision_paragraph() }), args.agents_on());
    Request {
        model: args.model.clone(),
        system,
        messages: transcript,
        // A chat has only the help about Bennu itself and opening a panel; an agent has everything.
        tools: if args.chat {
            tools::chat_specs()
        } else {
            tools::offered_specs(args.plan, !skills.is_empty(), &tools::families_off(&args.families_off), git_tools)
        },
        max_tokens: args.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        effort: args.effort.as_deref().and_then(Effort::parse),
        long_cache: bennu_core::config::load().assistant.long_cache,
    }
}

fn preview_json(ctx: &BennuState, request: &Request, model: Option<&ModelInfo>, standing_texts: &(String, String)) -> Value {
    // The key is only looked at to be masked; a missing one just shows as not set.
    let key = api_key(ctx).unwrap_or_default();
    let p = preview(&Anthropic, request, &key);
    json!({
        "parts": token_report::preview_parts(request, &standing_texts.0, &standing_texts.1),
        "cache": { "longStanding": request.long_cache, "marks": ["tools", "instructions", "conversation"] },
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
            // Cold: everything is written to the cache (the standing part to the long one when that is on, the
            // conversation to the short one). Warm: everything is read back, which is what each request after the
            // first costs apart from the few tokens it adds. Uncached: what it would cost with no cache at all.
            let standing = b.system + b.tools;
            let first = m.cost(&Usage { cache_write_tokens: b.total(), cache_write_1h_tokens: if request.long_cache { standing } else { 0 }, ..Default::default() });
            let next = m.cost(&Usage { cache_read_tokens: b.total(), ..Default::default() });
            let uncached = m.cost(&Usage { input_tokens: b.total(), ..Default::default() });
            json!({ "first": first, "next": next, "uncached": uncached })
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
    if args.text.trim().is_empty() && !args.retry {
        return Err("the message is empty".into());
    }
    if args.root.trim().is_empty() {
        return Err("open a project first — Imber works inside one".into());
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
        build_isolation: if args.build_isolation.trim().eq_ignore_ascii_case("separate") { "separate".into() } else { String::new() },
        ..Default::default()
    });
    // What kind of conversation this is was decided when it began; the request follows it, whatever the
    // panel says now.
    let mut args = args;
    conv.decision_level = args.chat_decision_level.trim().to_string();
    conv.decision_notes = args.chat_decision_notes.trim().chars().take(bennu_assistant::prelude::MAX_NOTES_CHARS).collect();
    conv.subagents = assistant::normalize_agents_flag(&args.chat_subagents).to_string();
    args.chat = conv.chat;
    args.build_isolation = conv.build_isolation.clone();

    // An isolated copy: made on the conversation's first message and kept until it is applied or discarded. From here
    // on the project the run works in IS the copy — its path is the root for the access policy, the tools and
    // the live processes, so none of them needs to know there is a copy at all.
    let mut copy_mode: Option<bennu_assistant::prelude::WorktreeMode> = None;
    if !args.chat && !args.dry_run {
        if let Some(meta) = worktree::prepare(ctx, &mut conv, args.worktree.as_deref(), &args.root, args.git).await? {
            args.file = args.file.as_deref().and_then(|f| bennu_assistant::prelude::to_copy(&meta.real_root, &meta.copy_root, f));
            args.root = meta.copy_root.clone();
            // The review of an isolated copy is "Apply to project", so edits land in the copy at once.
            if !args.access_mode().applies_edits() {
                args.mode = AccessMode::AcceptEdits.id().into();
            }
            if matches!(parse_scope(&args.scope), Scope::Workspace | Scope::Disk) {
                args.scope = "project".into();
            }
            args.workspace_roots.clear();
            args.extra_folders.clear();
            if !meta.mode.indexed() {
                // Nothing is indexed in a text-only copy: the tools that need an index are not offered.
                args.families_off.extend(["project", "refactor", "structural", "history"].map(String::from));
            }
            copy_mode = Some(meta.mode);
        }
    }
    args.worktree = copy_mode.map(|m| m.id().to_string());

    // Changing a running game is opt-in; and a project without a Cargo.toml has no Bevy game to look into, so
    // those tools (and their definitions) are not offered at all.
    if !args.bevy_write {
        args.families_off.push("bevy_write".into());
    }
    // Sub-agents are opt-in: the conversation's own switch, else the setting. Off, the tools are not even offered.
    if !assistant::agents_enabled(args.subagents_enabled, &conv.subagents) {
        args.families_off.push("agents".into());
    }
    // So is changing the program under the debugger.
    if !args.debug_mutate {
        args.families_off.push("debug_mutate".into());
    }
    if !std::path::Path::new(&args.root).join("Cargo.toml").is_file() {
        args.families_off.extend(["bevy", "bevy_write"].map(String::from));
    }

    // Git is opt-in, and so is hiding people: both only when asked for, and neither in a chat.
    let git_tools = if args.git && !args.chat { git::load(ctx) } else { Vec::new() };
    let hide = args.hide_authors && !git_tools.is_empty();

    let mut transcript = conv.transcript.clone();
    let set = working_set(&args);
    let enabled_skills = enabled_skills(&args, &set);
    let mut user_text = with_context(&args.text, args.file.as_deref(), args.selection.as_deref());
    // A message that starts with `/name` asks for that skill: the model is told so, and the user's own text
    // (shown in the thread) is left as typed.
    let note = skills::attachment_note(&skills::attached(&args.text, &enabled_skills));
    if !note.is_empty() {
        user_text = format!("{note}{user_text}");
    }
    if hide {
        // A name the user types that is already a made-up id's owner goes to the provider as the id.
        user_text = pseudonyms::Authors::load().hide_known(&user_text);
    }
    // What Bennu noted for this conversation while it was idle (a build failed in files it may have changed).
    // A dry run only looks: it must not use them up.
    if !args.dry_run {
        let inbox = crate::build_queue::inbox();
        inbox.begin_run(&conversation_id);
        let pending = inbox.take(&conversation_id);
        if !pending.is_empty() {
            user_text = format!("{}\n\n{user_text}", assistant::render_notes(&pending));
        }
    }
    // At the very end of the message — the one part that is new on every request — a one-line suggestion to hand an
    // unrelated task to a sub-agent when the window is filling up. The cached prefix is untouched.
    if args.agents_on() {
        let used = conv.context.system + conv.context.project + conv.context.conversation;
        let fill = assistant::fill_ratio(used, u64::from(args.max_context));
        let effort = assistant::nudge_effort(args.effort.as_deref());
        if let Some(note) = assistant::subagent_nudge(effort, fill, &args.text, &assistant::touched_files(&conv.transcript)) {
            user_text = format!("{user_text}\n\n{note}");
        }
    }
    // A retry sends what the conversation already holds: the user's message (or the tool results) is its last turn.
    if args.retry {
        if !matches!(transcript.last(), Some(m) if m.role == Role::User) {
            return Err("there is nothing to send again: the conversation does not end with your message".into());
        }
    } else {
        transcript.push(Message::user(user_text));
    }
    let request = build_request(&args, transcript.clone(), &git_tools, &set, &enabled_skills);

    let (table, _) = models::load();
    let model = find_model(&table, &args.model).cloned();

    if args.dry_run {
        let texts = token_report::standing_texts(args.instructions, &args.root, &enabled_skills);
        return Ok(json!({ "preview": preview_json(ctx, &request, model.as_ref(), &texts) }));
    }

    // The brakes, before anything is spent: a conversation or a period already at its limit does not start.
    let limits = limits_now();
    let baseline = ledger::spent(&args.periods, conv.cost);
    // Runs in flight in other conversations have spent money the ledger does not know yet.
    if let Some((_, message)) = limits.breach(&baseline.plus_run(0.0, ledger::in_flight().total())) {
        return Err(format!("{message}. Raise it in Settings › Imber › Spending limits to go on."));
    }

    let key = api_key(ctx)?;
    let now = now_ms();
    if !args.retry {
        let user_message = json!({
            "id": format!("u{now}"), "role": "user", "at": now,
            "blocks": [{ "type": "text", "text": args.text }],
        });
        conv.messages.push(user_message);
        if conv.title.trim().is_empty() || conv.title == "New conversation" {
            conv.title = title_from(&args.text);
        }
    }
    conv.transcript = transcript;
    conv.model = args.model.clone();
    conv.updated_at = now;
    store::save_in(&dir, &conv)?;

    let run_id = format!("r{now:x}");
    let session_base = conv.cost;
    let cancel = runs::begin(&run_id);
    worktree::mark_running(&conversation_id);
    let mut state = BennuState::new(ctx.event_sink());
    if let Some(host) = ctx.host_caller() {
        state = state.with_host_caller(host);
    }
    let mut session = Session::new(
        run_id.clone(),
        conversation_id.clone(),
        args.root.clone(),
        args.access_mode(),
        ctx.event_sink(),
        state,
        cancel,
        build_policy(&args, &conversation_id, &set),
        View::new(format!("a{now}"), args.model.clone(), now),
    );
    session.hide_authors = hide;
    session.ask_cap = bennu_assistant::prelude::question_cap(args.decision_level_effective());
    session.debug_idle_minutes = args.debug_idle_minutes;
    session.projects = set;
    session.title = conv.title.clone();
    session.build_isolation = crate::build_queue::Isolation::parse(&conv.build_isolation);
    // So a refusal in another conversation can say whose process or file it ran into.
    crate::live::note_owner(&conversation_id, &conv.title);
    let session = Arc::new(session);
    {
        // So the panel can take back a file the run opened for itself while it is still going.
        let weak = Arc::downgrade(&session);
        runs::set_revoker(&run_id, Arc::new(move |grant_id: &str| weak.upgrade().is_some_and(|s| s.revoke_grant(grant_id))));
    }
    let handle = tokio::runtime::Handle::current();
    // What the run needs to start sub-agents, when they are on: the model and settings of its own requests.
    let agent_setup = args.agents_on().then(|| tools::AgentSetup {
        model: args.model.clone(),
        max_tokens: request.max_tokens,
        long_cache: request.long_cache,
        instructions: token_report::standing_texts(args.instructions, &args.root, &enabled_skills).0,
        worktree: args.worktree.as_deref().and_then(bennu_assistant::prelude::WorktreeMode::parse),
    });

    std::thread::Builder::new()
        .name(format!("assistant-{run_id}"))
        .spawn(move || {
            let guard = tools::Guard { limits, periods: args.periods, session_base, model: model.clone() };
            execute(session, handle, key, request, conv, RunOptions { model, max_context: args.max_context, periods: args.periods, guard, chat: args.chat, git: git_tools, plan: args.plan, off: tools::families_off(&args.families_off), skills: enabled_skills, agents: agent_setup });
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
    skills: Vec<skills::Skill>,
    /// Present when the run may start sub-agents.
    agents: Option<tools::AgentSetup>,
}

/// Ends a run's bookkeeping when `execute` is left, however it is left.
struct RunCleanup(String, String);

impl Drop for RunCleanup {
    fn drop(&mut self) {
        // Agents it started and did not collect stop with it (on the normal path the run already settled them).
        subagents::stop_run(&self.0);
        tools::reserve::release_run(&self.0);
        side_jobs::take_cost(&self.0);
        // After the run's line is in the ledger (or it never made one): the money is counted in one place.
        ledger::in_flight().finish(&self.0);
        runs::finish(&self.0);
        worktree::unmark_running(&self.1);
    }
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
    let RunOptions { model, max_context, periods, guard, chat, git, plan, off, skills, agents } = options;
    let skills_index = skills::render_index(&skills);
    // Whatever way this function is left — a panic included — the run's reservations and cancel flag go.
    let _cleanup = RunCleanup(session.run_id.clone(), session.conversation_id.clone());
    let outcome = match HttpTransport::new() {
        Ok(transport) => {
            let compaction = (max_context > 0).then(|| tools::Compaction {
                key: key.clone(),
                // Folding an old conversation into a note does not need the conversation's own (large) model.
                model: side_jobs::side_model(bennu_assistant::prelude::SideJob::Compaction, &request.model),
                threshold: u64::from(max_context) * 9 / 10,
            });
            let host = ToolHost::new(Arc::clone(&session), handle, compaction, Some(guard), chat, git, plan, off, skills);
            let host = match agents {
                Some(setup) => host.with_agents(setup),
                None => host,
            };
            assistant::run(&Anthropic, &transport, &host, &key, request.clone(), MAX_STEPS)
        }
        Err(e) => assistant::Outcome::failed_to_start(request.messages.clone(), assistant::ProviderError::local(e)),
    };

    // Priced request by request: a long prompt costs more per token, which a total cannot tell. A request that
    // failed after being billed is one of them.
    let mut requests: Vec<Usage> = outcome.requests;
    let error: Option<assistant::ProviderError> = outcome.error;
    let ended = match outcome.ended {
        Ended::Done => "done",
        Ended::Cancelled => "cancelled",
        Ended::TooManySteps => "too_many_steps",
        Ended::Incomplete(_) => "incomplete",
        Ended::Failed => "failed",
    };
    // The whole conversation as the run left it: it may have been compacted on the way, and after a failure it is
    // every turn that did complete, so "Retry" sends the same request again from there.
    conv.transcript = outcome.messages;
    if let Some(e) = &error {
        session.view().push(error_notice(e));
    } else if ended == "too_many_steps" {
        session.view().push(json!({ "type": "notice", "level": "warning", "text": format!("Stopped after {MAX_STEPS} steps. Say \"continue\" to go on.") }));
    } else if ended == "incomplete" {
        session.view().push(json!({ "type": "notice", "level": "warning", "text": "The answer was cut short — the model reached its length limit or declined." }));
    }

    // The provider's own account of the last request, for the meter: what was read from the cache and what was not.
    let last_request = requests.last().copied();
    // At the end of a run that went well, a conversation past its share of the window is folded now: the user is
    // between messages, and the requests after this one stop carrying the bulk.
    if error.is_none() && ended == "done" && max_context > 0 {
        let overhead = tokens(&request.system) + tokens(&serde_json::to_string(&request.tools).unwrap_or_default());
        match side_jobs::compact_at_boundary(&session.run_id, &mut conv.transcript, overhead, max_context, &key, &request.model) {
            Ok(Some(folded)) => session.view().push(json!({ "type": "notice", "level": "info", "text": folded.report })),
            Ok(None) => {}
            // The conversation is exactly as it was; the next run tries again.
            Err(e) => session.view().push(json!({
                "type": "notice", "level": "warning",
                "text": format!("The conversation could not be compacted ({}). It was left as it was; Imber will try again after the next answer.", e.kind.status_text().to_lowercase()),
                "detail": e.detail,
            })),
        }
    }

    // What compacting cost is part of what the conversation cost (at the cheaper model's prices: see `side_jobs`).
    requests.extend(session.extra_requests.lock().unwrap_or_else(|p| p.into_inner()).iter().copied());
    let side_usd = side_jobs::take_cost(&session.run_id);
    let run_usd = model.as_ref().map(|m| m.cost_of_all(&requests) + side_usd);
    // Whatever the run opened for itself ends with it, and the thread says so.
    // Agents still going are stopped and given a moment to write their ending into the thread that is saved below.
    subagents::settle_run(&session.run_id);
    session.expire_grants();
    // A run that drove live processes ends with a card of what it started, sent, asserted and cost.
    let live_card = session.live_log().card(&crate::live::running_names_of(&session.root, Some(&session.conversation_id)), run_usd);
    if let Some(card) = live_card {
        session.view().push(card);
    }
    if !session.view().is_empty() {
        conv.messages.push(session.display_snapshot());
    }
    if let (Some(m), Some(run_usd)) = (&model, run_usd) {
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
    conv.context = token_report::context_of(&request.system, &session.root, &skills_index, &request.tools, &conv.transcript);
    conv.updated_at = now_ms();
    let saved = store::save_in(&store::dir(), &conv);

    session.push_message();
    session.emit(
        "done",
        json!({
            "ended": ended,
            "error": error.as_ref().map(|e| e.friendly()),
            "error_kind": error.as_ref().map(|e| e.kind.code()),
            "save_error": saved.err(),
            "cost": conv.cost,
            "context": token_report::context_json(&conv.context),
            "request": last_request.map(|u| token_report::usage_json(&u)),
            "title": conv.title,
            "updated_at": conv.updated_at,
            "spend": spend_json(&limits, &spent_now),
        }),
    );
    runs::finish(&session.run_id);
}

/// The notice a failed run leaves in the thread: what happened and what to do in plain words, the provider's own
/// wording kept apart for the expandable detail, and whether "Retry" is offered (it always is — fixing the key or
/// compacting first is the user's to do, and the request is then sent again as it stood).
fn error_notice(e: &assistant::ProviderError) -> Value {
    json!({
        "type": "notice", "level": "error", "text": e.friendly(), "detail": e.detail,
        "kind": e.kind.code(), "retryable": e.retryable(), "canRetry": true,
    })
}

/// Whether a conversation has a run going right now (for telling another conversation whether waiting for it makes sense).
pub(crate) fn conversation_active(conversation: &str) -> bool {
    worktree::is_running(conversation)
}

/// Stop a run. It ends at the next chunk or the next tool, and keeps what it had.
#[arbor_rpc::handler]
fn assistant_cancel(_ctx: &BennuState, run_id: String) -> Result<bool, String> {
    Ok(runs::cancel(&run_id))
}

/// Skip the wait before a failed request is sent again ("Retry now"). False when the run is not going.
#[arbor_rpc::handler]
fn assistant_retry_now(_ctx: &BennuState, run_id: String) -> Result<bool, String> {
    Ok(runs::retry_now(&run_id))
}

/// Take back a file a running agent opened for itself (Autonomous mode). False when the run is over or the
/// grant is already gone.
#[arbor_rpc::handler]
fn assistant_revoke_grant(_ctx: &BennuState, run_id: String, grant_id: String) -> Result<bool, String> {
    Ok(runs::revoke(&run_id, &grant_id))
}

/// Answer a question a run is waiting on — a set of questions, an access request, a confirmation.
#[arbor_rpc::handler]
fn assistant_reply(_ctx: &BennuState, prompt_id: String, answer: Value) -> Result<bool, String> {
    Ok(runs::reply(&prompt_id, answer))
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

    // The note is written by the cheaper side model (Settings › Assistant), not by the conversation's own.
    let side = side_jobs::side_model(bennu_assistant::prelude::SideJob::Compaction, &args.model);
    let done = compact::compact(&conv.transcript, &transport, &key, &side, args.instructions.as_deref(), true, &size, 0)
        .map_err(|e| format!("The conversation could not be compacted: {} It was left as it was.", e.friendly()))?;
    conv.transcript = done.transcript;
    let rebuilt = estimate_context("", &[], &conv.transcript);
    // The standing part (instructions and tools) is unchanged by this; only the rest is recounted.
    conv.context = Context { project: rebuilt.project, conversation: rebuilt.conversation, ..conv.context };
    let (table, _) = models::load();
    if done.summarized {
        if let Some(m) = find_model(&table, &side) {
            conv.cost += m.cost(&done.usage);
        }
    }
    let notice = if done.summarized { done.dropped.clone() } else { "Nothing earlier to summarise yet.".to_string() };
    conv.messages.push(json!({
        "id": format!("n{}", now_ms()), "role": "assistant", "at": now_ms(), "model": args.model,
        "blocks": [{ "type": "notice", "level": "info", "text": notice }],
    }));
    store::save_in(&dir, &conv)?;
    Ok(json!({
        "freed": before.saturating_sub(size(&conv.transcript)),
        "summarized": done.summarized,
        "cost": conv.cost,
        "context": token_report::context_json(&conv.context),
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

/// Every skill found under the given project roots and the user's folders, with the ones that could not be
/// used and why — for the manager. The body is included so the manager can preview it; which are enabled is
/// the panel's to say (`skills_on`), not decided here.
#[arbor_rpc::handler]
async fn assistant_skills_list(_ctx: &BennuState, roots: Vec<String>) -> Result<Value, String> {
    let found = tokio::task::spawn_blocking(move || {
        let roots: Vec<PathBuf> = roots.iter().filter(|r| !r.trim().is_empty()).map(PathBuf::from).collect();
        let (user_bennu, user_claude) = skills::user_sources();
        skills::discover(&skills::sources(&roots, user_bennu, user_claude))
    })
    .await
    .map_err(|e| format!("could not look for skills: {e}"))?;
    Ok(skills::wire(&found))
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
fn assistant_delete(ctx: &BennuState, id: String) -> Result<(), String> {
    // What the conversation still holds goes with it: its isolated copy, its processes, its file reservations.
    worktree::discard_for_deleted(ctx, &id);
    tools::reserve::release_conversation(&id);
    tools::release_debug(&id);
    read_memory::forget(&id);
    digest_store::forget(&id);
    subagents::forget_conversation(&id);
    crate::build_queue::journal().forget_conversation(&id);
    crate::build_queue::inbox().forget_conversation(&id);
    store::delete_in(&store::dir(), &id)
}

/// End the debug sessions the assistant left running: those of one project when it closes, all of them when
/// Arbor exits. A launched program is stopped, an attached one is detached and resumed.
pub(crate) fn release_debug_sessions(root: Option<&str>, reason: &str) {
    tools::release_debug_matching(root, reason);
}

/// Remove isolated copies a crash or an abandoned conversation left behind. Called once at startup, off the
/// main path: it can walk and delete a lot.
pub fn sweep_worktrees_in_background() {
    std::thread::Builder::new().name("assistant-worktree-sweep".into()).spawn(worktree::sweep).ok();
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
    fn a_send_reads_the_mode_by_its_new_and_its_old_word() {
        let args = |mode: &str| -> SendArgs {
            serde_json::from_value(json!({ "text": "t", "root": "r", "model": "m", "mode": mode })).unwrap()
        };
        assert_eq!(args("manual").access_mode(), AccessMode::Manual);
        assert_eq!(args("accept_edits").access_mode(), AccessMode::AcceptEdits);
        assert_eq!(args("auto").access_mode(), AccessMode::AcceptEdits, "what a saved config or an older panel sends");
        assert_eq!(args("autonomous").access_mode(), AccessMode::Autonomous);
        let absent: SendArgs = serde_json::from_value(json!({ "text": "t", "root": "r", "model": "m" })).unwrap();
        assert_eq!(absent.access_mode(), AccessMode::Manual, "no word is the cautious one");
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
