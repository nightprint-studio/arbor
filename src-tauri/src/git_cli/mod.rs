//! Shell-facing facade over the Tauri-free [`corvus_git_cli`] crate.
//!
//! Detection, the resolved-`git` `Command` builder, and the PortableGit download
//! moved into the crate (so the headless `corvus-be` shares them). The shell now
//! only re-exports the handful of items its remaining in-process call sites use
//! (`detect` for boot/config, `command` for the few shell-side git invocations,
//! `portable_dir` for the config snapshot). The fallible wrappers and the
//! keyring-coupled HTTP auth-arg helpers are gone — their callers moved to
//! `corvus-be`.

// Infallible items re-exported verbatim (the crate owns the global detection
// state — in-process there is one instance, so behaviour is identical).
pub use corvus_git_cli::{command, detect, portable_dir};
