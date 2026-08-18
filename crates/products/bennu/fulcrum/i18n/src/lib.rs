//! `bennu-fulcrum-i18n` — the fulcrum engine's i18n convention, as IDE tooling.
//!
//! A fulcrum project does not put its user-visible text in the code. A `.ron` content file declares
//! a **label** (`name: "tree:nodes.drill.name"`), the code asks for one (`t_key!("ui:effect.damage")`),
//! and the strings themselves live in `i18n/<lang>/<category>.toml` — one file per category, one
//! directory per language, with a stylesheet and a glossary beside them.
//!
//! That arrangement is better than inline strings and it moves the whole class of mistakes somewhere
//! no compiler looks:
//!
//! - a label read from a `.ron` that **no bundle declares** — and the engine renders the label
//!   itself when it cannot resolve one, so it survives QA until somebody sees
//!   `tree:nodes.drill.name` written on a hexagon;
//! - a label declared in one language and **forgotten in another**, which falls back silently and
//!   shows Italian inside an English interface;
//! - a label nothing reads any more, left behind by content that was deleted;
//! - a `$style` the stylesheet does not declare, which renders as the default and loses the
//!   emphasis it was written for.
//!
//! Four questions, none of them answerable from the file in front of you, all four answerable from
//! the project — which is what this crate is.
//!
//! ## The three problems, as three modules
//!
//! 1. [`markup`] — what a translation *is*: `{param}`, `$style{…}`, `@glossary{…}`, `~control{…}`.
//!    Parsed with byte spans and without failing, which is where it differs from the engine's own
//!    parser and why.
//! 2. [`catalog`] — what the `i18n/` trees declare: labels per language, styles, glossary entries,
//!    each with the position of its value.
//! 3. [`refs`] — where the project reads a label, in `.ron` and in `.rs`.
//!
//! 4. [`studio`] — the same three, read off the **live buffer** instead of the index: what the
//!    editor's i18n panel and its markup colouring are driven from.
//!
//! [`ext`] composes them into the one thing a host registers.
//!
//! ## Where this sits, and why it is its own crate
//!
//! Under `crates/products/bennu/fulcrum/`, which is the root for **one crate per fulcrum
//! subsystem**. i18n is the first; assets and content are the obvious next. Each registers itself
//! with the framework-extension seam under a namespaced id (`fulcrum.i18n`), so a sibling is a new
//! crate plus one registration line — not an edit to this one. No umbrella crate until two of them
//! have something to share, at which point `bennu-fulcrum-core` joins them in the same folder.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_fulcrum_i18n::prelude::...`.

pub mod catalog;
pub mod ext;
pub mod markup;
pub mod prelude;
pub mod refs;
pub mod studio;
