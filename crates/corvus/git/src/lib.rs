//! `corvus-git` — local-git logic for Corvus, Tauri-free.
//!
//! Extracted from the shell so the in-process handlers **and** the headless
//! `corvus-be` process run the exact same code (see `docs/corvus-be-bringup.md`).
//! It owns no global state: git invocation goes through an explicit
//! [`cli::GitCli`] (the program path), so the shell and `corvus-be` each build
//! their own — nothing to keep in sync across the process boundary.
//!
//! Errors are a local [`error::GitError`]; the shell maps it to its `AppError`
//! (preserving the wire string), `corvus-be` maps it to the IPC error string.
//!
//! Domains extracted so far: **bisect** ([`bisect`]) + saved sessions
//! ([`bisect_sessions`]), **stash** ([`stash`]), **reset** + tags ([`reset`]),
//! encoding-aware decode ([`encoding`]) and the **recovery** snapshot journal
//! ([`recovery`]).
//!
//! ## Public API: use the [`prelude`]

pub mod bisect;
pub mod bisect_sessions;
pub mod cli;
pub mod encoding;
pub mod error;
pub mod merge;
pub mod prelude;
pub mod recovery;
pub mod reset;
pub mod search;
pub mod stash;
