//! Canonical entry point for `bennu-assistant`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_assistant::prelude::...`.

// What the assistant may touch.
pub use crate::access::{normalize, Ask, Decision, Grant, Op, Policy, Scope, MAX_UNASKED_DELETE_FILES};

// How much the agent decides for itself, and the rule for opening a file the build named.
pub use crate::autonomy::{
    build_error_ask, canonical_mode_word, decide_grant, diagnostic_files, grant_reason, is_own_command, must_ask_own_command, AccessMode, Diagnostics, FileGrant,
    GrantQuery, Refusal, RunGrants, Verdict, MAX_AUTO_GRANTS, OWN_COMMANDS,
};

// The first provider, and what a host needs to know about its models.
pub use crate::anthropic::{efforts_for, wants_adaptive_thinking, Anthropic, AnthropicDecoder};

// How much it decides for itself, and the user's words about it.
pub use crate::decision::{merge_notes, paragraph as decision_paragraph, question_cap, DecisionLevel, DECIDE_QUESTION_CAP, MAX_NOTES_CHARS};

// How long what the assistant started may outlive its run.
pub use crate::lifetime::{clamp_idle_minutes, Lease, DEFAULT_IDLE_MINUTES, MAX_IDLE_MINUTES, MIN_IDLE_MINUTES};

// How hard the model thinks.
pub use crate::effort::Effort;

// The neutral conversation.
pub use crate::message::{Block, ImageRef, Message, Request, Role, ToolSpec};

// Pictures in the conversation: cost, vision support, keeping only the latest few.
pub use crate::images::{
    estimate_image_tokens, image_dimensions, image_tokens, keep_latest_images, load_images, png_dimensions, supports_images, without_images, KEEP_LATEST_IMAGES,
};

// The provider interface, and what a response stream says.
pub use crate::provider::{Decoder, Endpoint, Event, Provider, StopReason, Usage};

// The models on offer, and what a request to each one costs.
pub use crate::models::{find as find_model, merge as merge_models, ModelFile, ModelInfo, Rates, DEFAULT_TOML as DEFAULT_MODELS_TOML};

// Spending limits, in euros.
pub use crate::limits::{eur, InFlight, Level, Limits, Meter, Period, Spent};

// A request shown instead of sent.
pub use crate::dryrun::{mask_key, preview, Breakdown, Preview};

// The loop, and what a host plugs into it.
pub use crate::agent::{run, AgentEvent, Ended, Gate, Host, HttpRequest, Outcome, ToolCall, Transport};

// A failed request, as a kind and a sentence, and when it is sent again.
pub use crate::error::{ErrorKind, ProviderError};
pub use crate::retry::{entropy_jitter, sleep_interruptible, Backoff, Pause, Plan as RetryPlan, RetryPolicy, BASE_WAIT, MAX_RETRY_AFTER, MAX_WAIT, NETWORK_BUDGET, SERVER_ATTEMPTS};

// Who is editing which file, across conversations.
pub use crate::reservations::{key_of as reservation_key, Conflict as ReservationConflict, Registry as Reservations};

// An isolated copy of the project: its kinds, path mapping and what applying it back means.
pub use crate::worktree::{
    clash_summary, fnv64, fnv64_update, plan_apply, rel_of, to_copy, to_real, Base, Change, Clash, Kind as ChangeKind, Manifest, Mode as WorktreeMode,
    Observation, Plan as ApplyPlan, FNV_SEED,
};

// Who wrote what, who a broken build is blamed on, and the notes left for suspects.
pub use crate::attribution::{
    attribute, for_requester as attribution_for_requester, note_for as attribution_note_for, render_notes, Attribution, Elsewhere, Error as BuildError, Inbox, Journal, Note, Suspect, Writer,
    MAX_PENDING as MAX_PENDING_NOTES, MAX_PER_RUN as MAX_NOTES_PER_RUN, NOTE_ERRORS,
};

// Sub-agents: their permissions, money, the nudge to start one, and the note between conversations.
pub use crate::subagent::{
    change_request_note, files_named, looks_unrelated, nudge as subagent_nudge, nudge_threshold, restrict as restrict_policy, Budget as SubagentBudget, ChildTools, NudgeEffort,
    RequestLimiter, MAX_CHANGE_REQUESTS_PER_RUN, MAX_DEPTH as MAX_SUBAGENT_DEPTH, MAX_PARALLEL_CHILDREN,
};

// The children of a conversation: their record, the registry, the checks on starting one, and the texts around them.
pub use crate::children::{
    agents_enabled, card_block as child_card_block, child_cap_eur, normalize_agents_flag, child_mode, describe_owner, failure_note as child_failure_note, waiting_text as child_waiting_text, fill_ratio, final_answer, label_prompt, nudge_effort, plan_spawn, report_for_parent, touched_files, ChildInfo,
    ChildRegistry, ChildStatus, SpawnPlan, CHILD_MAX_STEPS, MAX_SUMMARY_CHARS,
};

// A stop of the debugged program, as a note for the assistant that owns the session.
pub use crate::stopnote::{
    auto_continue_allowed, card_title, decide_delivery, format_snapshot, format_values, is_secret_name, stop_note, Delivery, OwnerState, RateLimiter, StopSnapshot, AUTO_WINDOW_MS, MAX_AUTO_CONTINUES, MAX_CAPTURES,
};

// What a request costs and where: token estimates by part, the cache, old results, big results, compaction.
pub use crate::digest::{digest_text, digest_value, expand as expand_digest, Digest, MIN_CHARS as DIGEST_MIN_CHARS};
pub use crate::sidejobs::{resolve_side_model, SideJob, DEFAULT_SIDE_MODEL};
pub use crate::stale::{is_stub as is_stale_stub, maybe_stub as maybe_stub_stale, stale_tokens, stub_stale_results, StalePolicy, StaleReport, STUB_PREFIX};
pub use crate::summary::{
    compose_note as compose_summary_note, current_plan, dropped_report, due_at_boundary, files_touched, BOUNDARY_SHARE, MIN_TOKENS_TO_COMPACT, SYSTEM as SUMMARY_SYSTEM,
};
pub use crate::tokens::{
    cache_hit_share, estimate as estimate_tokens, history_tokens, parts as token_parts, standing as standing_tokens, tool_weights, tools_tokens, HistoryTokens, Part as TokenPart, Standing,
};
