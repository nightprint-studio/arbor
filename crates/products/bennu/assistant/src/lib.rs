//! `bennu-assistant` — the pure core of Bennu's assistant.
//!
//! ## Why a crate with no network in it
//!
//! The assistant is three things that change at different speeds: the **provider** (Anthropic today,
//! others later), the **tools** it calls (Bennu's own, growing), and the **rules** about what a tool may
//! touch. This crate holds the part that must not be wrong and does not need the outside world to be
//! tested — the shapes, the translation to and from a provider's wire format, the effort levels and the
//! access policy — and leaves the transport (an HTTP client, the credential vault) and the tool
//! implementations to whoever hosts it.
//!
//! | | |
//! |---|---|
//! | [`message`] | the neutral conversation: roles, blocks (text, tool use, tool result), tools, a request |
//! | [`provider`] | [`Provider`](provider::Provider): how a request becomes an HTTP call, and a response stream becomes events |
//! | [`anthropic`] | the first provider — Messages API request body and its server-sent-event stream |
//! | [`dryrun`] | a request shown instead of sent: the body, the headers with the key masked, and where its size comes from |
//! | [`effort`] | how hard the model thinks, and which models have the setting |
//! | [`agent`] | the loop: ask, run the tools it calls, report back, until done — transport, tools and progress plugged in |
//! | [`error`] | a failed request as a kind (offline, rate limited, bad key, …), the provider's words, and what to tell the user |
//! | [`retry`] | which failures are sent again, the backoff schedule, the budgets, and the interruptible wait |
//! | [`access`] | what a tool may read, change, create and delete, and when it has to ask |
//! | [`autonomy`] | the three modes (manual, accept edits, autonomous) and the one rule by which autonomous may open a file the build named |
//! | [`attribution`] | who last wrote each file, the heuristic split of a failed build's errors between conversations, and the notes left for suspects |
//! | [`decision`] | the decision level (ask at forks, ask big ones, decide) and the user's notes about decisions, as a prompt paragraph |
//! | [`reservations`] | who is changing which file, across conversations running at once |
//! | [`images`] | pictures in the conversation: their token cost, which models read them, and keeping only the latest few |
//! | [`tokens`] | where a request's tokens go (tools, rules, instructions, history by kind), one estimate for every meter |
//! | [`stale`] | replacing old, bulky, re-fetchable tool results by stubs, in batches that pay for the cache rewrite |
//! | [`digest`] | the short answer a big build, test, problem, usage or search result deserves, with the detail pageable |
//! | [`summary`] | the compaction note: fixed sections, the files changed and the plan kept from the record, when to compact |
//! | [`sidejobs`] | which model does compaction and read-only helpers |
//! | [`worktree`] | an isolated copy of the project: its kinds, path mapping, and what applying it back means |
//!
//! ## Adding a provider
//!
//! Implement [`provider::Provider`]: say where requests go and how they are authenticated, build the body
//! from a [`message::Request`], and give back a decoder that turns the response's bytes into
//! [`provider::Event`]s. Nothing else in the assistant knows which provider it is talking to.
//!
//! ## Public API: use the [`prelude`]

pub mod access;
pub mod agent;
pub mod anthropic;
pub mod attribution;
pub mod autonomy;
pub mod children;
pub mod decision;
pub mod digest;
pub mod dryrun;
pub mod effort;
pub mod error;
pub mod images;
pub mod lifetime;
pub mod limits;
pub mod message;
pub mod models;
pub mod prelude;
pub mod provider;
pub mod reservations;
pub mod retry;
pub mod sidejobs;
pub mod stale;
pub mod stopnote;
pub mod subagent;
pub mod summary;
pub mod tokens;
pub mod worktree;
