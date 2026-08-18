//! `bennu-toml` — TOML read **with positions**, which is the whole reason it exists.
//!
//! `toml::Value` answers "what does this file say". An editor needs the other half: *where* it
//! says it. Jumping to the declaration of a message key, rewriting a version requirement in place,
//! completing inside the value the caret is in, reporting a problem on the exact span that caused
//! it — none of those can be done from a parsed value tree, because the positions are gone by
//! then.
//!
//! So this is a **scanner**, not a parser. It walks the text once and records, for every table
//! header and every assignment, the dotted path and the byte range. It does not build a value
//! tree, does not resolve dotted keys into nested tables, and does not type anything: what a key
//! *means* belongs to whoever owns the schema.
//!
//! ## Tolerant by construction
//!
//! Every entry point takes text that may be **mid-keystroke** and must degrade to less
//! information rather than to an error. A file with an unclosed string still yields the tables
//! above it; a stray `]` costs one header, not the document. An editor that stops answering while
//! you type is worse than one that answers approximately.
//!
//! ## Who reads it
//!
//! Two callers with nothing else in common, which is why this is its own crate rather than a
//! module of either:
//!
//! - **`bennu-cargo`** — a `Cargo.toml`'s tables and keys, for validation, completion and
//!   rewriting a dependency's version.
//! - **`bennu-fulcrum-i18n`** — a message bundle's keys, so a label can be navigated to and a
//!   translation can be reported as missing on the line that should have held it.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_toml::prelude::...`.

pub mod manifest;
pub mod prelude;
