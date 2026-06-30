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
//! encoding-aware decode ([`encoding`]), the **recovery** snapshot journal
//! ([`recovery`]), and the File-Explorer git awareness ([`explorer`] — overlay
//! badges + light inline actions, shared with `sitta-be`).
//!
//! ## Public API: use the [`prelude`]

pub mod bisect;
pub mod bisect_sessions;
pub mod branch;
pub mod cli;
pub mod diff;
// Encoding-aware decode/encode lives in the foundation crate `arbor-fs` (it is a
// pure file-content concern, not git-specific). Re-exported here so this crate's
// `crate::encoding::…` call sites and external `corvus_git::encoding::…` paths
// keep resolving to the single canonical implementation.
pub use arbor_fs::prelude::encoding;
pub mod error;
pub mod explorer;
pub mod gitflow;
pub mod graph;
pub mod graph_svg;
pub mod init;
pub mod merge;
pub mod notes;
pub mod prelude;
pub mod rebase;
pub mod recovery;
pub mod reflog;
pub mod remote;
pub mod repo;
pub mod reset;
pub mod search;
pub mod stats;
pub mod status;
pub mod stash;
pub mod submodule;
pub mod tickets;
pub mod worktree;
