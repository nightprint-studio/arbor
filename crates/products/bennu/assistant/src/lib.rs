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
//! | [`access`] | what a tool may read, change, create and delete, and when it has to ask |
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
pub mod dryrun;
pub mod effort;
pub mod limits;
pub mod message;
pub mod models;
pub mod prelude;
pub mod provider;
