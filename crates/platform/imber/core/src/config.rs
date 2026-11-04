//! The assistant's settings: one `[assistant]` table of the Bennu config file, read by every product that hosts
//! Imber, so the model, the effort, the spending limits and the rest are chosen once. The API key is **not**
//! here — it lives in the credential vault — and neither is any path: where the file is and how it is read
//! belong to the host (this crate has no filesystem).

use serde::{Deserialize, Serialize};

/// The assistant's defaults — chosen in Settings › Assistant, and changeable per conversation from the
/// composer. The API key is **not** here: it lives in the credential vault, never in a config file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssistantConfig {
    /// Model id, e.g. `claude-opus-5-5`.
    pub model: String,
    /// `low` … `max`; ignored by a model with no effort setting.
    pub effort: String,
    /// `manual` (edits are proposed for review), `accept_edits` (applied as made, inside the scope) or
    /// `autonomous` (that, and it may open the files the build names). An older `auto` reads as `accept_edits`.
    #[serde(deserialize_with = "assistant_mode_word")]
    pub mode: String,
    /// `file`, `project`, `workspace` or `disk` — how far it may reach without asking.
    pub scope: String,
    /// The largest context window it may use, in tokens.
    pub max_context: u32,
    pub allow_edits: bool,
    /// May use the system Temp folders as scratch space.
    pub allow_temp: bool,
    /// Folders it may work in besides the project, in any scope.
    pub extra_folders: Vec<String>,
    /// How much it may spend.
    pub limits: LimitsConfig,
    /// A conversation with no tools at all: it cannot read, search or change anything, only talk.
    pub chat_only: bool,
    /// Read the project's `CLAUDE.md` / `AGENTS.md` (and the user's own instructions file) into the
    /// assistant's instructions.
    pub use_instructions: bool,
    /// Let the assistant use git — status, diff, log, blame, stage, commit. Off until the user turns it on:
    /// it sends commit messages and author names to the provider, and commit changes the repository.
    pub git_enabled: bool,
    /// With git on: replace the people in git results (authors, addresses) with made-up ids before the model
    /// sees them, and turn the ids back into names for the user. On by default.
    pub hide_authors: bool,
    /// Let the assistant keep a visible step plan (with a percentage) for work of several steps. On by
    /// default; off removes the tool and the instruction, which also saves their tokens.
    pub plan_enabled: bool,
    /// How many open projects one conversation may work across (the one the user is in included), when the
    /// reach is the workspace. Each is a line in the instructions and a place a search may go.
    pub max_projects: u32,
    /// Tool families switched off (`read`, `project`, `edit`, `refactor`, `run`, `history`, `structural`): not
    /// offered to the model, so it cannot call them and their definitions cost nothing.
    pub tool_families_off: Vec<String>,
    /// Let the assistant change a running Bevy game over BRP (mutate, insert, remove, spawn, despawn). Off until
    /// the user turns it on: the game is live state with no proposal and no undo.
    pub bevy_write_enabled: bool,
    /// Let the assistant change the program it is debugging (throw an exception, return early, drop a frame, write a
    /// value). Off until the user turns it on: the program changes and there is no undo.
    pub debug_mutate_enabled: bool,
    /// Skills the user enabled, as `<scope>/<name>` ids (scope `user` or `project`). An allow-list: a skill
    /// found on disk is not shown to the model until it is ticked in the skills manager.
    pub skills_on: Vec<String>,
    /// How much it decides alone: `ask_forks` (default), `ask_big` or `decide`.
    pub decision_level: String,
    /// The user's own words about decisions, for every conversation (a conversation can add its own).
    pub decision_notes: String,
    /// Ask before a change of model or context size that makes a long conversation re-read its history at full price.
    pub confirm_cost_changes: bool,
    /// Offer the assistant sub-agents (`spawn_agent`): clean-context helpers with a subset of its permissions.
    pub subagents_enabled: bool,
    /// Minutes a debug session the assistant started may sit unused before it is ended.
    pub debug_idle_minutes: u32,
    /// When the debugger stops while the assistant is idle, start a run by itself to tell it. Spends money; off by default.
    pub auto_continue_on_stop: bool,
    /// Keep the instructions and tool definitions in the provider's one-hour cache instead of the five-minute one.
    /// Pays off when you pause between messages (or open several conversations in a project): the standing part
    /// is then read at a fraction instead of being written again. On by default.
    pub long_cache: bool,
    /// The cheaper model for side jobs — compacting a conversation, and (when sub-agents exist) read-only
    /// exploration. Empty: the cheapest Haiku-class model in the price list.
    pub side_model: String,
    /// Code pasted into the composer becomes a fenced block with its language (Ctrl+Shift+V pastes plain). On by default.
    pub wrap_pasted_code: bool,
    /// The user's standing context for every conversation — who they are, how they like to work, what Imber
    /// should always know — in the manner of a chat assistant's custom instructions. Empty: nothing is added.
    /// Cut at [`MAX_GENERAL_CONTEXT_CHARS`] when it is put in the instructions.
    pub general_context: String,
}

/// The most characters of [`AssistantConfig::general_context`] that reach the instructions. The text is sent
/// (and cached) with every request, so the cap is what a runaway paste costs on every turn.
pub const MAX_GENERAL_CONTEXT_CHARS: usize = 4000;

/// The saved assistant mode, with the word the two-way switch used (`auto`) read as the mode it became.
/// The file is rewritten with the new word the next time the choice is saved.
fn assistant_mode_word<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let word = String::deserialize(d)?;
    Ok(if word.trim().eq_ignore_ascii_case("auto") { "accept_edits".to_string() } else { word })
}

/// Spending limits, in euros; `0` switches one off. Prices are in dollars, so `eur_per_usd` is the rate
/// they are converted at — kept by hand, because a rate that moved on its own would move the limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    /// Per conversation.
    pub session_eur: f64,
    pub day_eur: f64,
    pub week_eur: f64,
    pub month_eur: f64,
    /// Euros per dollar.
    pub eur_per_usd: f64,
    /// From this share of a limit, in percent, the meter warns.
    pub warn_percent: f64,
}

impl Default for LimitsConfig {
    // On by default, at amounts that a normal day does not reach: the brakes are for the day something
    // loops, and nobody turns them on after that day.
    fn default() -> Self {
        Self { session_eur: 5.0, day_eur: 10.0, week_eur: 0.0, month_eur: 50.0, eur_per_usd: 0.92, warn_percent: 80.0 }
    }
}

impl Default for AssistantConfig {
    fn default() -> Self {
        Self {
            model: "claude-opus-5-5".to_string(),
            effort: "high".to_string(),
            mode: "manual".to_string(),
            scope: "project".to_string(),
            max_context: 200_000,
            allow_edits: true,
            allow_temp: true,
            extra_folders: Vec::new(),
            limits: LimitsConfig::default(),
            chat_only: false,
            use_instructions: true,
            git_enabled: false,
            hide_authors: true,
            plan_enabled: true,
            max_projects: 6,
            tool_families_off: Vec::new(),
            bevy_write_enabled: false,
            debug_mutate_enabled: false,
            skills_on: Vec::new(),
            decision_level: "ask_forks".to_string(),
            decision_notes: String::new(),
            confirm_cost_changes: true,
            subagents_enabled: false,
            debug_idle_minutes: 30,
            auto_continue_on_stop: false,
            long_cache: true,
            side_model: String::new(),
            wrap_pasted_code: true,
            general_context: String::new(),
        }
    }
}

