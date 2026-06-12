//! Shared user-feedback core for Arbor.
//!
//! The four systems that surface progress / status to the user — the job
//! registry, in-app notifications, the operations overlay, and (frontend-only)
//! toasts — used to live inside the main-window shell. This crate holds the
//! Tauri-agnostic pieces so any window (main, nemus, …) can host them:
//!
//! - [`jobs`]       — [`jobs::JobRegistry`] + [`jobs::JobInfo`] / [`jobs::JobStatus`],
//!                    the pure in-memory job model. The process-spawning glue
//!                    (which needs `AppHandle` + the plugin host) stays in the
//!                    shell; only the data lives here.
//! - [`notify`]     — the `plugin:notification` payload + emit helper.
//! - [`operations`] — the `arbor://plugin-operation-*` event-name contract.
//!
//! ## Routing (`target`)
//!
//! Backend events broadcast to every window. To let a notification / job land
//! in a *specific* window, payloads carry an optional `target` window id. Each
//! window mounts a feedback host with an id and filters incoming items by it;
//! the `main` host additionally accepts untagged items so existing call sites
//! (which pass no `target`) keep their original behavior. The frontend does the
//! filtering — this crate just makes `target` part of the contract.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — reach types through `arbor_feedback::prelude::…`
//! rather than the per-feature submodule paths.

pub mod jobs;
pub mod notify;
pub mod operations;
pub mod prelude;
