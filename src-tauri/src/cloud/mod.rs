//! Host-side wiring for the `arbor-cloud` crate.
//!
//! All cloud logic now lives in `crates/arbor-cloud`. This module:
//!   * re-exports the cloud crate's modules so existing call sites
//!     (`crate::cloud::types::CloudConnection`, etc.) keep compiling;
//!   * defines [`ArborCloudHost`], the `arbor_cloud::CloudHost` impl that
//!     bridges into `AppState`'s `JobRegistry` / `PluginHost` / Tauri
//!     events / cancellation maps;
//!   * exposes [`install`] which the Tauri `setup()` calls once to (a)
//!     register the Google OAuth refresher and (b) construct + manage the
//!     `Arc<dyn CloudHost>` that command + plugin-namespace layers pull
//!     out of Tauri State.
//!
//! Earmarked for deletion when the cloud-storage plugin moves to a
//! subprocess runtime. See `crates/arbor-cloud/src/lib.rs` for the broader
//! note on the migration path.

// Re-export migrated modules so external paths keep working without
// touching every consumer.
pub use arbor_cloud::{oauth_google, ops, secrets, transfer, types};

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};

use arbor_cloud::host::{CloudHost, CloudJobInfo, CloudJobStatus};
use crate::AppState;
use crate::jobs::{JobInfo, JobRegistry, JobStatus};
use crate::plugin::runtime::PluginHost;

/// Type aliases re-exported at module root for backward compat with the
/// pre-split `crate::cloud::CloudCancellations` / `CloudPendingOps` paths
/// (used by `AppState`'s field declarations and a few consumers).
pub use arbor_cloud::host::{CloudCancellations, CloudPendingOps};

// ── CloudHost impl ─────────────────────────────────────────────────────────

/// Bridges `arbor_cloud::CloudHost` onto the host's `AppState` registries
/// + the Tauri event bus. Constructed once at startup and managed as
/// `Arc<dyn CloudHost>` so the command + plugin-namespace layers can pull
/// it back out of Tauri State without knowing the concrete type.
pub struct ArborCloudHost {
    cancellations: Arc<CloudCancellations>,
    pending_ops:   Arc<CloudPendingOps>,
    jobs:          Arc<Mutex<JobRegistry>>,
    plugin_host:   Arc<Mutex<PluginHost>>,
    app:           AppHandle,
}

impl ArborCloudHost {
    pub fn new(
        app:           AppHandle,
        cancellations: Arc<CloudCancellations>,
        pending_ops:   Arc<CloudPendingOps>,
        jobs:          Arc<Mutex<JobRegistry>>,
        plugin_host:   Arc<Mutex<PluginHost>>,
    ) -> Self {
        Self { cancellations, pending_ops, jobs, plugin_host, app }
    }

    /// Construct from a managed `AppState` + an `AppHandle` (typically
    /// inside Tauri's `setup()`).
    pub fn from_state(state: &AppState, app: AppHandle) -> Self {
        Self::new(
            app,
            state.cloud_cancellations.clone(),
            state.cloud_pending_ops.clone(),
            state.jobs.clone(),
            state.plugin_host.clone(),
        )
    }
}

impl CloudHost for ArborCloudHost {
    fn cancellations(&self) -> &CloudCancellations { &self.cancellations }
    fn pending_ops(&self)    -> &CloudPendingOps   { &self.pending_ops }

    fn fire_plugin_hook(&self, plugin: &str, hook: &str, payload_json: &str) {
        match self.plugin_host.lock() {
            Ok(host) => { let _ = host.fire_hook_on(plugin, hook, payload_json); }
            Err(e)   => tracing::warn!("plugin_host poisoned, dropping hook {plugin}:{hook}: {e}"),
        }
    }

    fn emit_event(&self, topic: &str, payload: serde_json::Value) {
        if let Err(e) = self.app.emit(topic, payload) {
            tracing::warn!("emit {topic} failed: {e}");
        }
    }

    fn job_new_id(&self) -> String {
        match self.jobs.lock() {
            Ok(mut jobs) => jobs.new_id(),
            Err(e) => {
                tracing::error!("jobs mutex poisoned in job_new_id: {e}");
                String::new()
            }
        }
    }

    fn job_register(&self, info: CloudJobInfo) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.register(JobInfo {
                id:              info.id,
                name:            info.name,
                plugin_name:     info.plugin_name,
                command:         info.command,
                started_at:      info.started_at,
                status:          cloud_to_jobs_status(info.status),
                category:        info.category,
                non_cancellable: info.non_cancellable,
                hidden:          info.hidden,
                is_system:       info.is_system,
                finished_at:     None,
            });
        }
    }

    fn job_append_output(&self, job_id: &str, line: String) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.append_output(job_id, line);
        }
    }

    fn job_set_status(&self, job_id: &str, status: CloudJobStatus) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.set_status(job_id, cloud_to_jobs_status(status));
        }
    }
}

fn cloud_to_jobs_status(s: CloudJobStatus) -> JobStatus {
    match s {
        CloudJobStatus::Running                   => JobStatus::Running,
        CloudJobStatus::Completed { exit_code }   => JobStatus::Completed { exit_code },
        CloudJobStatus::Failed    { error }       => JobStatus::Failed { error },
        CloudJobStatus::Cancelled                 => JobStatus::Cancelled,
    }
}

// ── Startup wiring ─────────────────────────────────────────────────────────

/// Register the OAuth refresher AND publish the `Arc<dyn CloudHost>` into
/// Tauri's managed state. Call once from `setup()` after `AppState` is
/// managed and the `AppHandle` is available.
pub fn install(app: &AppHandle) {
    // 1. OAuth refresher — let arbor-cloud wire its own internal
    //    `refresh_with` against the auth_gcs OnceLock.
    arbor_cloud::oauth_google::install_refresher();

    // 2. CloudHost — built once, shared via Arc so spawned tokio tasks
    //    inside arbor-cloud can clone cheaply.
    let state: tauri::State<'_, AppState> = app.state();
    let host = ArborCloudHost::from_state(&*state, app.clone());
    let host_arc: Arc<dyn CloudHost> = Arc::new(host);
    app.manage(host_arc);
}
