//! The children of a conversation — pure: what the registry holds, the rules for starting one, and the small
//! texts around them.
//!
//! [`crate::subagent`] settles *what a child may do* (permissions, money, when to suggest one). This is the
//! bookkeeping a host needs to run them: a record per child that the panel can list, the check that a
//! `spawn_agent` call is allowed *now*, the summary handed back to the parent, the one-line status of who owns
//! a file, and the inputs of the nudge (how full the window is, which files were worked on).
//!
//! Nothing here starts a thread or touches the network; the host (`bennu-be`) owns the loop and the tools.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::{json, Value};

use crate::autonomy::AccessMode;
use crate::error::ProviderError;
use crate::message::{Block, Message, Role};
use crate::subagent::{ChildTools, NudgeEffort, MAX_PARALLEL_CHILDREN};

/// The longest a child's title is, in characters.
pub const MAX_TITLE_CHARS: usize = 60;
/// The longest a task is: it is the child's whole context, so it is bounded but generous.
pub const MAX_TASK_CHARS: usize = 8_000;
/// The longest summary handed back to the parent. A child that wrote more is cut: the parent asked for a
/// summary, and a long one would put back into the parent's context what the child was started to keep out of it.
pub const MAX_SUMMARY_CHARS: usize = 6_000;
/// Requests a child may make before it is stopped.
pub const CHILD_MAX_STEPS: u32 = 25;
/// Finished children kept per conversation; the oldest are forgotten first.
pub const KEEP_FINISHED: usize = 30;
/// The cap on one child's spending when no session limit says otherwise, in euros.
pub const DEFAULT_CHILD_CAP_EUR: f64 = 1.0;

/// Where a child is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChildStatus {
    Running,
    Done,
    /// Stopped by the user, or because its parent was.
    Stopped,
    /// Ended by an error or by running out of steps or money.
    Failed,
}

impl ChildStatus {
    pub fn is_running(self) -> bool {
        self == ChildStatus::Running
    }
}

/// One child, as the panel lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildInfo {
    pub id: String,
    pub conversation_id: String,
    /// The run of the parent that started it.
    pub run_id: String,
    pub title: String,
    pub task: String,
    /// `read` or `read+edit`.
    pub tools: String,
    pub model: String,
    pub status: ChildStatus,
    /// Dollars, like the conversation's own cost.
    pub cost: f64,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    /// What the child answered, once it has.
    pub summary: Option<String>,
    pub error: Option<String>,
    /// Set while the agent waits to send a failed request again ("waiting to retry (2/5): No connection").
    pub waiting: Option<String>,
}

/// The children of every conversation, in memory: a child does not outlive the process, and the parent's thread
/// keeps a card of each (see [`card_block`]).
#[derive(Default)]
pub struct ChildRegistry {
    items: Mutex<Vec<ChildInfo>>,
    seq: AtomicU64,
}

impl ChildRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<ChildInfo>> {
        self.items.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// An id for the next child of `run_id`: it is also the child's run id, so it never collides with a parent's.
    pub fn next_id(&self, run_id: &str) -> String {
        format!("{run_id}.a{}", self.seq.fetch_add(1, Ordering::SeqCst) + 1)
    }

    pub fn add(&self, info: ChildInfo) {
        let mut items = self.lock();
        let conversation = info.conversation_id.clone();
        items.push(info);
        // Bound memory: past the limit the oldest finished children of the conversation go.
        let finished: Vec<usize> = items.iter().enumerate().filter(|(_, c)| c.conversation_id == conversation && !c.status.is_running()).map(|(i, _)| i).collect();
        if finished.len() > KEEP_FINISHED {
            let drop_count = finished.len() - KEEP_FINISHED;
            let doomed: Vec<usize> = finished.into_iter().take(drop_count).collect();
            let mut i = 0;
            items.retain(|_| {
                let keep = !doomed.contains(&i);
                i += 1;
                keep
            });
        }
    }

    /// Change a child in place; false when there is no such child.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut ChildInfo)) -> bool {
        match self.lock().iter_mut().find(|c| c.id == id) {
            Some(c) => {
                change(c);
                true
            }
            None => false,
        }
    }

    pub fn get(&self, id: &str) -> Option<ChildInfo> {
        self.lock().iter().find(|c| c.id == id).cloned()
    }

    /// The children of a conversation, oldest first.
    pub fn list(&self, conversation: &str) -> Vec<ChildInfo> {
        self.lock().iter().filter(|c| c.conversation_id == conversation).cloned().collect()
    }

    /// The running children a run started.
    pub fn running_of_run(&self, run_id: &str) -> Vec<String> {
        self.lock().iter().filter(|c| c.run_id == run_id && c.status.is_running()).map(|c| c.id.clone()).collect()
    }

    /// The running children of a conversation.
    pub fn running_of_conversation(&self, conversation: &str) -> Vec<String> {
        self.lock().iter().filter(|c| c.conversation_id == conversation && c.status.is_running()).map(|c| c.id.clone()).collect()
    }

    /// The conversation was deleted: its children go with it.
    pub fn forget_conversation(&self, conversation: &str) {
        self.lock().retain(|c| c.conversation_id != conversation);
    }
}

// ── Starting one ─────────────────────────────────────────────────────────────────────────────────

/// A `spawn_agent` call that is allowed, with the title and task tidied and what was changed on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnPlan {
    pub title: String,
    pub task: String,
    pub tools: ChildTools,
    /// The call returns at once with an id (read-only children only); otherwise the parent waits.
    pub background: bool,
    /// What the host tells the model about how the request was adjusted (empty when it was taken as asked).
    pub notes: Vec<String>,
}

fn clip_chars(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

/// Whether a `spawn_agent` call may go ahead, given how many children the run has going and what the parent may do.
///
/// A child that would edit while the parent only proposes (or has editing off) is started read-only and the
/// model is told so; a child that edits always runs in the foreground, so the parent is never writing at the
/// same time — one writer at a time follows from that, not from a lock.
pub fn plan_spawn(
    title: &str,
    task: &str,
    tools: &str,
    background: bool,
    running_in_run: usize,
    parent_mode: AccessMode,
    parent_allows_edits: bool,
) -> Result<SpawnPlan, String> {
    let title = clip_chars(title, MAX_TITLE_CHARS);
    if title.is_empty() {
        return Err("`title` is required: a few words the user will see in the Agents list".into());
    }
    let task = clip_chars(task, MAX_TASK_CHARS);
    if task.is_empty() {
        return Err("`task` is required: the agent starts with nothing but this text, so say what to find out or do, where to look and what to report".into());
    }
    let mut wanted = ChildTools::parse(tools).ok_or_else(|| format!("`tools` is `read` or `read+edit`, not `{}`", tools.trim()))?;
    if running_in_run >= MAX_PARALLEL_CHILDREN {
        return Err(format!("{running_in_run} agents are already running (the most is {MAX_PARALLEL_CHILDREN}): wait for one with `await_agents`"));
    }
    let mut notes = Vec::new();
    if wanted == ChildTools::ReadEdit && !(parent_mode.applies_edits() && parent_allows_edits) {
        wanted = ChildTools::Read;
        notes.push("The agent was started read-only: in this mode your own edits are only proposals (or editing is off), and an agent never does more than you can.".to_string());
    }
    let mut background = background;
    if wanted == ChildTools::ReadEdit && background {
        background = false;
        notes.push("An agent that edits runs in the foreground: you wait for it, so only one of you writes at a time.".to_string());
    }
    Ok(SpawnPlan { title, task, tools: wanted, background, notes })
}

/// The word a conversation stores for its own sub-agents switch: "on", "off", or empty to follow the setting.
pub fn normalize_agents_flag(word: &str) -> &'static str {
    match word.trim().to_lowercase().as_str() {
        "on" | "true" | "yes" => "on",
        "off" | "false" | "no" => "off",
        _ => "",
    }
}

/// Whether sub-agents are offered: the conversation's own choice when it has one, else the user's setting.
pub fn agents_enabled(setting: bool, conversation_word: &str) -> bool {
    match normalize_agents_flag(conversation_word) {
        "on" => true,
        "off" => false,
        _ => setting,
    }
}

/// The mode a child runs in: its parent's, except that Autonomous becomes Accept edits. Opening files a build named,
/// and running the project's build unasked, are things the user granted the *main* agent; a child never widens its
/// own reach, so it asks (the question goes to the user) where the parent would not have to.
pub fn child_mode(parent: AccessMode) -> AccessMode {
    if parent == AccessMode::Autonomous { AccessMode::AcceptEdits } else { parent }
}

/// The cap on one child's spending, in euros: a third of the session limit when there is one (so three
/// children cannot spend the whole of it), else [`DEFAULT_CHILD_CAP_EUR`].
pub fn child_cap_eur(session_limit_eur: f64) -> f64 {
    if session_limit_eur > 0.0 { (session_limit_eur / 3.0).min(DEFAULT_CHILD_CAP_EUR) } else { DEFAULT_CHILD_CAP_EUR }
}

/// What the parent reads when a child finishes: the child's answer, cut to a bounded length, with one line saying
/// how it ended. Only this goes back into the parent's context.
pub fn report_for_parent(title: &str, status: ChildStatus, answer: &str, cost_usd: f64, note: &str) -> String {
    let answer = answer.trim();
    let mut body: String = answer.chars().take(MAX_SUMMARY_CHARS).collect();
    if answer.chars().count() > MAX_SUMMARY_CHARS {
        body.push_str("\n… (cut: ask for a shorter summary next time)");
    }
    if body.is_empty() {
        body = "(the agent gave no answer)".into();
    }
    let end = match status {
        ChildStatus::Running => "still running",
        ChildStatus::Done => "finished",
        ChildStatus::Stopped => "stopped before it finished",
        ChildStatus::Failed => "ended early",
    };
    let note = if note.trim().is_empty() { String::new() } else { format!(" — {}", note.trim()) };
    format!("Agent “{title}” {end}{note} (${cost_usd:.3}).\n\n{body}")
}

/// How an agent that could not finish because a request failed is described to the conversation that started
/// it: what failed, whether trying again can help, and whether it had already changed anything — so the parent
/// neither takes the silence for "nothing found" nor repeats work that was done.
pub fn failure_note(error: &ProviderError, files_changed: usize) -> String {
    let hint = if error.retryable() { "This is usually temporary: starting the agent again may work." } else { "Starting it again will not help until this is fixed." };
    let changes = match files_changed {
        0 => "Nothing was changed.".to_string(),
        1 => "It had already changed 1 file before this.".to_string(),
        n => format!("It had already changed {n} files before this."),
    };
    let detail = error.detail.trim();
    let detail = if detail.is_empty() { String::new() } else { format!(" (provider: {detail})") };
    format!("it could not finish: {}{detail}. {hint} {changes}", error.kind.what_happened())
}

/// The status text on a card while the agent waits to send a request again.
pub fn waiting_text(attempt: u32, max: u32, wait_secs: u64, status: &str) -> String {
    format!("waiting to retry ({attempt}/{max}, in {wait_secs} s): {status}")
}

/// The text of the last assistant message that has some: a child's final answer.
pub fn final_answer(messages: &[Message]) -> String {
    messages.iter().rev().filter(|m| m.role == Role::Assistant).map(Message::text).find(|t| !t.trim().is_empty()).unwrap_or_default()
}

/// The card the parent's thread keeps for a child: compact, and updated in place as the child goes.
pub fn card_block(info: &ChildInfo) -> Value {
    let brief: String = info.summary.as_deref().unwrap_or("").chars().take(400).collect();
    json!({
        "type": "agent",
        "agent": {
            "id": info.id,
            "title": info.title,
            "status": info.status,
            "tools": info.tools,
            "model": info.model,
            "cost": info.cost,
            "startedAt": info.started_at,
            "endedAt": info.ended_at,
            "summary": brief,
            "error": info.error,
            "waiting": info.waiting,
        },
    })
}

// ── Asking the user on a child's behalf ──────────────────────────────────────────────────────────

/// Say, in a question that goes to the user, which agent is asking: an access prompt from a child must not look
/// like the parent's own. The block's shape is the panel's (`access`, `confirm`).
pub fn label_prompt(block: &mut Value, agent_title: &str) {
    let tag = format!("Agent “{agent_title}”");
    let prefix = |v: &mut Value, pointer: &str, sep: &str| {
        if let Some(Value::String(s)) = v.pointer_mut(pointer) {
            *s = format!("{tag}{sep}{s}");
        }
    };
    prefix(block, "/request/reason", ": ");
    prefix(block, "/confirm/title", " — ");
}

// ── Who owns a file ──────────────────────────────────────────────────────────────────────────────

/// Who holds a file or wrote it last, as the model reads it. `holder` is (conversation, title, running now);
/// `writer` the last writer (conversation, title); `step` what the holder is working on.
pub fn describe_owner(file: &str, holder: Option<(&str, &str, bool)>, writer: Option<(&str, &str)>, step: Option<&str>) -> String {
    let name = |id: &str, title: &str| if title.trim().is_empty() { format!("conversation {id}") } else { format!("conversation {id} (\"{}\")", title.trim()) };
    let mut out = String::new();
    match (holder, writer) {
        (Some((id, title, running)), _) => {
            out.push_str(&format!("`{file}` is reserved by {}", name(id, title)));
            out.push_str(if running { ", which is running now." } else { ", which is idle." });
            if let Some(step) = step.filter(|s| !s.trim().is_empty()) {
                out.push_str(&format!(" Its current plan step: {}.", step.trim()));
            }
            out.push_str(" Use `request_change` to ask it for a change.");
        }
        (None, Some((id, title))) => {
            out.push_str(&format!("`{file}` is not reserved; it was last changed by {}. Use `request_change` to ask it for something.", name(id, title)));
        }
        (None, None) => out.push_str(&format!("`{file}` is not reserved and no conversation has changed it in this session: edit it yourself.")),
    }
    out
}

// ── The nudge's inputs ───────────────────────────────────────────────────────────────────────────

/// How hard the user asked it to think, as the nudge understands it: nothing or `low` never nudges.
pub fn nudge_effort(effort: Option<&str>) -> NudgeEffort {
    match effort.map(|e| e.trim().to_lowercase()).as_deref() {
        Some("medium") => NudgeEffort::Medium,
        Some("high") | Some("xhigh") | Some("max") => NudgeEffort::High,
        _ => NudgeEffort::Low,
    }
}

/// How full the window is: the conversation's tokens over the window the user allows (0 when there is no window).
pub fn fill_ratio(context_tokens: u64, window: u64) -> f64 {
    if window == 0 { 0.0 } else { context_tokens as f64 / window as f64 }
}

/// The files a conversation has worked on, from the arguments of its tool calls: what `looks_unrelated` compares a
/// new request against. Newest last, without repeats.
pub fn touched_files(transcript: &[Message]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |p: &str| {
        if !p.is_empty() && !out.iter().any(|o| o == p) {
            out.push(p.to_string());
        }
    };
    for m in transcript {
        for b in &m.blocks {
            let Block::ToolUse { input, .. } = b else { continue };
            for key in ["file", "path"] {
                if let Some(p) = input.get(key).and_then(Value::as_str) {
                    push(p);
                }
            }
            for key in ["files", "paths"] {
                for p in input.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    push(p);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str, conv: &str, run: &str, status: ChildStatus) -> ChildInfo {
        ChildInfo {
            id: id.into(),
            conversation_id: conv.into(),
            run_id: run.into(),
            title: "t".into(),
            task: "task".into(),
            tools: "read".into(),
            model: "m".into(),
            status,
            cost: 0.0,
            started_at: 1,
            ended_at: None,
            summary: None,
            error: None,
            waiting: None,
        }
    }

    #[test]
    fn a_spawn_needs_a_title_a_task_and_a_known_tool_set() {
        let ok = plan_spawn("  Find the callers  ", "look for X", "read", false, 0, AccessMode::AcceptEdits, true).unwrap();
        assert_eq!((ok.title.as_str(), ok.tools, ok.background), ("Find the callers", ChildTools::Read, false));
        assert!(plan_spawn(" ", "x", "read", false, 0, AccessMode::Manual, true).is_err());
        assert!(plan_spawn("t", "  ", "read", false, 0, AccessMode::Manual, true).is_err());
        assert!(plan_spawn("t", "x", "shell", false, 0, AccessMode::Manual, true).unwrap_err().contains("read+edit"));
    }

    #[test]
    fn no_more_than_the_cap_run_at_once() {
        assert!(plan_spawn("t", "x", "read", true, MAX_PARALLEL_CHILDREN - 1, AccessMode::Manual, true).is_ok());
        assert!(plan_spawn("t", "x", "read", true, MAX_PARALLEL_CHILDREN, AccessMode::Manual, true).unwrap_err().contains("await_agents"));
    }

    #[test]
    fn an_editing_agent_needs_a_parent_that_edits_and_waits_for_it() {
        let p = plan_spawn("t", "x", "read+edit", true, 0, AccessMode::AcceptEdits, true).unwrap();
        assert_eq!((p.tools, p.background), (ChildTools::ReadEdit, false), "an editor is never in the background");
        assert_eq!(p.notes.len(), 1);
        let manual = plan_spawn("t", "x", "read+edit", false, 0, AccessMode::Manual, true).unwrap();
        assert_eq!(manual.tools, ChildTools::Read, "manual only proposes");
        let off = plan_spawn("t", "x", "read+edit", false, 0, AccessMode::Autonomous, false).unwrap();
        assert_eq!(off.tools, ChildTools::Read, "editing switched off");
        assert!(off.notes[0].contains("read-only"));
    }

    #[test]
    fn a_long_title_and_task_are_bounded() {
        let p = plan_spawn(&"a".repeat(200), &"b".repeat(MAX_TASK_CHARS + 50), "read", false, 0, AccessMode::Manual, true).unwrap();
        assert_eq!(p.title.chars().count(), MAX_TITLE_CHARS);
        assert_eq!(p.task.chars().count(), MAX_TASK_CHARS);
    }

    #[test]
    fn a_conversation_overrides_the_setting_only_when_it_says_so() {
        assert!(agents_enabled(false, "on") && !agents_enabled(true, "off"));
        assert!(agents_enabled(true, "") && !agents_enabled(false, "") && agents_enabled(true, "nonsense"));
        assert_eq!(normalize_agents_flag(" ON "), "on");
        assert_eq!(normalize_agents_flag("inherit"), "");
    }

    #[test]
    fn a_child_is_never_autonomous_even_under_an_autonomous_parent() {
        assert_eq!(child_mode(AccessMode::Autonomous), AccessMode::AcceptEdits);
        assert_eq!(child_mode(AccessMode::Manual), AccessMode::Manual);
        assert_eq!(child_mode(AccessMode::AcceptEdits), AccessMode::AcceptEdits);
    }

    #[test]
    fn the_cap_is_a_third_of_the_session_limit_up_to_a_ceiling() {
        assert!((child_cap_eur(0.0) - DEFAULT_CHILD_CAP_EUR).abs() < 1e-9);
        assert!((child_cap_eur(1.5) - 0.5).abs() < 1e-9);
        assert!((child_cap_eur(30.0) - DEFAULT_CHILD_CAP_EUR).abs() < 1e-9);
    }

    #[test]
    fn the_parent_reads_a_bounded_report_that_says_how_it_ended() {
        let r = report_for_parent("Callers", ChildStatus::Done, "Found 3.", 0.0123, "");
        assert!(r.starts_with("Agent “Callers” finished ($0.012)") && r.ends_with("Found 3."));
        let long = report_for_parent("t", ChildStatus::Done, &"x".repeat(MAX_SUMMARY_CHARS * 2), 0.0, "");
        assert!(long.chars().count() < MAX_SUMMARY_CHARS + 200 && long.contains("cut"));
        assert!(report_for_parent("t", ChildStatus::Stopped, "", 0.0, "the user stopped it").contains("stopped before it finished — the user stopped it"));
        assert!(report_for_parent("t", ChildStatus::Failed, "  ", 0.0, "").contains("no answer"));
    }

    #[test]
    fn a_failed_agent_says_what_failed_whether_it_can_be_retried_and_what_it_changed() {
        use crate::error::{ErrorKind, ProviderError};
        let down = failure_note(&ProviderError::offline("dns error"), 0);
        assert!(down.contains("could not reach the API") && down.contains("Nothing was changed") && down.contains("may work"), "{down}");
        let key = failure_note(&ProviderError::new(ErrorKind::Auth, "invalid x-api-key"), 2);
        assert!(key.contains("will not help") && key.contains("changed 2 files") && key.contains("invalid x-api-key"), "{key}");
        assert!(failure_note(&ProviderError::stream_broke(""), 1).contains("changed 1 file "));
        assert!(waiting_text(2, 5, 8, "No connection").contains("(2/5, in 8 s)"));
    }

    #[test]
    fn the_final_answer_is_the_last_assistant_text() {
        let msgs = vec![
            Message::user("task"),
            Message::assistant(vec![Block::Text { text: "first".into() }]),
            Message::tool_results(vec![]),
            Message::assistant(vec![Block::ToolUse { id: "1".into(), name: "x".into(), input: json!({}) }]),
            Message::assistant(vec![Block::Text { text: "the summary".into() }]),
            Message::assistant(vec![Block::ToolUse { id: "2".into(), name: "x".into(), input: json!({}) }]),
        ];
        assert_eq!(final_answer(&msgs), "the summary");
        assert_eq!(final_answer(&[Message::user("only")]), "");
    }

    #[test]
    fn the_registry_lists_by_conversation_and_finds_the_running_ones() {
        let r = ChildRegistry::default();
        let id = r.next_id("r1");
        assert!(id.starts_with("r1.a"));
        r.add(info(&id, "c-1", "r1", ChildStatus::Running));
        r.add(info("x", "c-2", "r2", ChildStatus::Running));
        assert_eq!(r.list("c-1").len(), 1);
        assert_eq!(r.running_of_run("r1"), vec![id.clone()]);
        assert!(r.update(&id, |c| c.status = ChildStatus::Done));
        assert!(r.running_of_run("r1").is_empty());
        assert_eq!(r.running_of_conversation("c-2"), vec!["x".to_string()]);
        r.forget_conversation("c-1");
        assert!(r.get(&id).is_none() && r.get("x").is_some());
        assert!(!r.update("nope", |_| {}));
    }

    #[test]
    fn the_oldest_finished_children_are_forgotten_past_the_limit() {
        let r = ChildRegistry::default();
        for i in 0..KEEP_FINISHED + 5 {
            r.add(info(&format!("a{i}"), "c", "r", ChildStatus::Done));
        }
        r.add(info("live", "c", "r", ChildStatus::Running));
        let ids: Vec<String> = r.list("c").into_iter().map(|c| c.id).collect();
        assert!(ids.contains(&"live".to_string()), "a running child is never dropped");
        assert!(!ids.contains(&"a0".to_string()) && ids.contains(&format!("a{}", KEEP_FINISHED + 4)));
        assert_eq!(ids.len(), KEEP_FINISHED + 1);
    }

    #[test]
    fn a_prompt_from_a_child_says_which_agent_asks() {
        let mut access = json!({"type":"access","request":{"id":"ax","reason":"It wants to read the folder."}});
        label_prompt(&mut access, "Callers");
        assert_eq!(access["request"]["reason"], "Agent “Callers”: It wants to read the folder.");
        let mut confirm = json!({"type":"confirm","confirm":{"id":"cf","title":"Run Build?","detail":"d"}});
        label_prompt(&mut confirm, "Fixer");
        assert_eq!(confirm["confirm"]["title"], "Agent “Fixer” — Run Build?");
        let mut other = json!({"type":"notice"});
        label_prompt(&mut other, "x");
        assert_eq!(other, json!({"type":"notice"}));
    }

    #[test]
    fn the_owner_of_a_file_is_described_with_what_it_is_doing() {
        let held = describe_owner("A.java", Some(("c-1", "Refactor cart", true)), None, Some("Rename total()"));
        assert!(held.contains("reserved by conversation c-1 (\"Refactor cart\")") && held.contains("running now") && held.contains("Rename total()") && held.contains("request_change"));
        assert!(describe_owner("A.java", Some(("c-1", "", false)), None, None).contains("idle"));
        assert!(describe_owner("A.java", None, Some(("c-2", "Old")), None).contains("last changed by conversation c-2"));
        assert!(describe_owner("A.java", None, None, None).contains("edit it yourself"));
    }

    #[test]
    fn effort_and_fill_feed_the_nudge() {
        assert_eq!(nudge_effort(Some("medium")), NudgeEffort::Medium);
        assert_eq!(nudge_effort(Some("xhigh")), NudgeEffort::High);
        assert_eq!(nudge_effort(Some("low")), NudgeEffort::Low);
        assert_eq!(nudge_effort(None), NudgeEffort::Low);
        assert!((fill_ratio(50, 200) - 0.25).abs() < 1e-9 && fill_ratio(5, 0) == 0.0);
    }

    #[test]
    fn touched_files_come_from_the_arguments_of_tool_calls() {
        let t = vec![
            Message::user("hi"),
            Message::assistant(vec![
                Block::ToolUse { id: "1".into(), name: "edit_file".into(), input: json!({"file": "/p/A.java"}) },
                Block::ToolUse { id: "2".into(), name: "read_files".into(), input: json!({"files": ["/p/B.java", "/p/A.java"]}) },
                Block::ToolUse { id: "3".into(), name: "search_text".into(), input: json!({"pattern": "x", "path": ""}) },
            ]),
        ];
        assert_eq!(touched_files(&t), vec!["/p/A.java", "/p/B.java"]);
    }

    #[test]
    fn a_card_carries_the_status_and_a_short_summary() {
        let mut i = info("r1.a1", "c", "r1", ChildStatus::Done);
        i.summary = Some("s".repeat(900));
        let card = card_block(&i);
        assert_eq!(card["type"], "agent");
        assert_eq!(card["agent"]["status"], "done");
        assert_eq!(card["agent"]["summary"].as_str().unwrap().chars().count(), 400);
    }
}
