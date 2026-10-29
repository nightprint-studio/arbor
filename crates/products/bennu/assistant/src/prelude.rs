//! Canonical entry point for `bennu-assistant`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_assistant::prelude::...`.

// What the assistant may touch.
pub use crate::access::{normalize, Ask, Decision, Grant, Op, Policy, Scope};

// The first provider, and what a host needs to know about its models.
pub use crate::anthropic::{efforts_for, wants_adaptive_thinking, Anthropic, AnthropicDecoder};

// How hard the model thinks.
pub use crate::effort::Effort;

// The neutral conversation.
pub use crate::message::{Block, Message, Request, Role, ToolSpec};

// The provider interface, and what a response stream says.
pub use crate::provider::{Decoder, Endpoint, Event, Provider, StopReason, Usage};

// The models on offer, and what a request to each one costs.
pub use crate::models::{find as find_model, merge as merge_models, ModelFile, ModelInfo, Rates, DEFAULT_TOML as DEFAULT_MODELS_TOML};

// Spending limits, in euros.
pub use crate::limits::{eur, Level, Limits, Meter, Period, Spent};

// A request shown instead of sent.
pub use crate::dryrun::{mask_key, preview, Breakdown, Preview};

// The loop, and what a host plugs into it.
pub use crate::agent::{run, AgentEvent, Ended, Gate, Host, HttpRequest, Outcome, ToolCall, Transport};
