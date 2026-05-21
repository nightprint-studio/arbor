//! Cloud object-storage layer — backs the `cloud-storage` plugin.
//!
//! All entry points are async. The host is **stateless**: every call takes
//! a full [`CloudConnection`] from the plugin (which keeps the list of
//! connections in its own settings). The only durable state we touch is
//! the OS keyring for secrets that can't safely live in plain plugin
//! settings (inline service-account JSON, OAuth refresh tokens).
//!
//! Earmarked for WASM-migration alongside `json_studio`. When the WASM
//! plugin runtime lands, this entire module + `plugin/api/ns/cloud.rs` +
//! `commands/cloud_commands.rs` + the `opendal` Cargo dependency are
//! deleted — the plugin gains its own WASM crate that pulls opendal
//! directly.  See: memory/project_cloud_storage_plugin.md

pub mod types;
pub mod secrets;
pub mod auth_gcs;
pub mod operator;
pub mod ops;
pub mod transfer;
pub mod oauth_google;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

/// Per-job cancellation flags for in-process tokio tasks (download/upload/sync).
///
/// Subprocess-backed jobs (build runners, etc.) are cancelled by killing the
/// PID — opendal-based jobs have no PID, so we poll a cooperative flag
/// between chunks instead. The map is owned by `AppState.cloud_cancellations`
/// and read by `cancel_job` to flip the right flag before falling through to
/// the standard PID-kill path.
pub type CloudCancellations = Mutex<HashMap<String, Arc<AtomicBool>>>;

/// Stream-id → job-id map for download_many calls that defer their final
/// status report to a follow-up phase (chunk-merge). The download task
/// inserts here when `keep_open=true`; `cloud_report_done` reads + removes
/// the entry to finalize the JobRegistry state once the merge phase ends.
pub type CloudPendingOps = Mutex<HashMap<String, String>>;
