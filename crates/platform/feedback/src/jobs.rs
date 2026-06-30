//! Window-agnostic job model: the in-memory [`JobRegistry`] plus the
//! [`JobInfo`] / [`JobStatus`] data types.
//!
//! This is the pure, Tauri-free half of the job system. The process-spawning
//! glue (`spawn_job`, the output `LineBatcher`) needs an `AppHandle`, the
//! `AppState` and the plugin host, so it stays in the shell crate (`src-tauri`).
//! Everything here is just data + bookkeeping and can be reused by any host.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Job types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum JobStatus {
    Running,
    Completed { exit_code: i32 },
    Failed { error: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobInfo {
    pub id: String,
    pub name: String,
    pub plugin_name: String,
    pub command: String,
    pub started_at: u64,
    pub status: JobStatus,
    /// Optional grouping category shown in the Jobs overlay (e.g. "Builds", "Services").
    pub category: Option<String>,
    /// When true the UI hides the cancel button — the job cannot be stopped by the user.
    #[serde(default)]
    pub non_cancellable: bool,
    /// When true the job is hidden from the default Jobs overlay / output panel
    /// listing and excluded from the status-bar running-count badge.  A "Show
    /// hidden" toggle on the Jobs panels reveals them as an escape hatch (e.g.
    /// when a domain-specific service-managed job becomes a zombie).
    #[serde(default)]
    pub hidden: bool,
    /// When true the job is internal (e.g. diff parsing, graph load) — it is
    /// purged from the registry a few seconds after completion so it does not
    /// clutter the Jobs overlay.  User-visible jobs (builds, plugin tasks) stay
    /// until the user clears them.
    #[serde(default)]
    pub is_system: bool,
    /// Unix timestamp at which the job entered a terminal state (Completed /
    /// Failed / Cancelled).  `None` while the job is still running.
    #[serde(default)]
    pub finished_at: Option<u64>,
    /// Optional window-routing target. `None` → the job belongs to the main
    /// window (the only host that accepts untagged items). A value (e.g.
    /// `"merula"`) routes the job's overlay/badge to the matching feedback host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// The fields a backend supplies when registering a job — the registry assigns
/// the `id` and stamps `started_at`. The reverse-channel wire shape for
/// `__job_register`: an out-of-process backend (e.g. `corvus-be`) sends a
/// `JobSpec`, the shell builds the [`JobInfo`]. Unlike `JobInfo` this is
/// `Deserialize` (the shell decodes it) — `JobInfo` stays serialize-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    pub name: String,
    pub plugin_name: String,
    pub command: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub non_cancellable: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub is_system: bool,
    #[serde(default)]
    pub target: Option<String>,
}

impl JobSpec {
    /// Build a `Running` [`JobInfo`] from this spec with the given id, stamping
    /// `started_at` now.
    pub fn into_info(self, id: String) -> JobInfo {
        JobInfo {
            id,
            name: self.name,
            plugin_name: self.plugin_name,
            command: self.command,
            started_at: JobRegistry::now_secs(),
            status: JobStatus::Running,
            category: self.category,
            non_cancellable: self.non_cancellable,
            hidden: self.hidden,
            is_system: self.is_system,
            finished_at: None,
            target: self.target,
        }
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct JobRegistry {
    jobs: HashMap<String, JobInfo>,
    /// Ring-buffer of output lines per job (max 2000 lines).
    outputs: HashMap<String, Vec<String>>,
    /// PID of the running process, if any — used for cancellation.
    pids: HashMap<String, u32>,
    counter: u64,
}

impl JobRegistry {
    pub fn new_id(&mut self) -> String {
        self.counter += 1;
        format!("job-{}", self.counter)
    }

    pub fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    pub fn register(&mut self, info: JobInfo) {
        self.outputs.insert(info.id.clone(), Vec::new());
        self.jobs.insert(info.id.clone(), info);
    }

    pub fn register_pid(&mut self, job_id: &str, pid: u32) {
        self.pids.insert(job_id.to_string(), pid);
    }

    pub fn append_output(&mut self, job_id: &str, line: String) {
        if let Some(lines) = self.outputs.get_mut(job_id) {
            lines.push(line);
            // Keep the last 2000 lines, dropping 200 at a time when full.
            if lines.len() > 2000 {
                lines.drain(0..200);
            }
        }
    }

    pub fn set_status(&mut self, job_id: &str, status: JobStatus) {
        let is_terminal = !matches!(status, JobStatus::Running);
        if let Some(info) = self.jobs.get_mut(job_id) {
            info.status = status;
            if is_terminal && info.finished_at.is_none() {
                info.finished_at = Some(Self::now_secs());
            }
        }
        // Clean up PID on terminal states.
        self.pids.remove(job_id);
        // Opportunistically purge old completed system jobs every time a
        // status transition occurs (keeps the overlay tidy with no timer).
        self.purge_stale_system_jobs();
    }

    /// Remove system jobs that finished more than `SYSTEM_JOB_TTL_SECS` ago.
    /// System jobs are short-lived internal tasks (diff parsing, graph loads)
    /// that should not accumulate in the Jobs overlay across a long session.
    pub fn purge_stale_system_jobs(&mut self) {
        const SYSTEM_JOB_TTL_SECS: u64 = 8;
        let now = Self::now_secs();
        let stale: Vec<String> = self.jobs.iter()
            .filter_map(|(id, info)| {
                if info.is_system {
                    if let Some(finished) = info.finished_at {
                        if now.saturating_sub(finished) >= SYSTEM_JOB_TTL_SECS {
                            return Some(id.clone());
                        }
                    }
                }
                None
            })
            .collect();
        for id in stale {
            self.jobs.remove(&id);
            self.outputs.remove(&id);
            self.pids.remove(&id);
        }
    }

    pub fn cancel(&mut self, job_id: &str) {
        if let Some(pid) = self.pids.remove(job_id) {
            kill_process(pid);
        }
        self.set_status(job_id, JobStatus::Cancelled);
    }

    /// Cancel all running jobs that belong to `plugin_name`.
    /// Pass `None` to cancel ALL running plugin jobs regardless of name.
    /// Jobs with `non_cancellable = true` (system jobs) are always skipped.
    pub fn cancel_by_plugin(&mut self, plugin_name: Option<&str>) {
        let ids: Vec<String> = self.jobs.values()
            .filter(|j| {
                j.status == JobStatus::Running
                    && !j.non_cancellable
                    && plugin_name.map_or(true, |p| j.plugin_name == p)
            })
            .map(|j| j.id.clone())
            .collect();
        for id in ids {
            self.cancel(&id);
        }
    }

    pub fn list(&self) -> Vec<JobInfo> {
        let mut v: Vec<JobInfo> = self.jobs.values().cloned().collect();
        // Most-recent first.
        v.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        v
    }

    /// Same as `list()` but also runs the stale-system-jobs purge first.
    /// Useful as a single entry point for the frontend listing command so the
    /// overlay never displays jobs that should have been cleaned up already.
    pub fn list_and_purge(&mut self) -> Vec<JobInfo> {
        self.purge_stale_system_jobs();
        self.list()
    }

    pub fn get_output(&self, job_id: &str) -> Vec<String> {
        self.outputs.get(job_id).cloned().unwrap_or_default()
    }

    pub fn running_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|j| j.status == JobStatus::Running)
            .count()
    }

    /// Remove a single job from the registry — only allowed for terminal jobs.
    /// Returns true when removed; false when missing or still running.
    pub fn dismiss(&mut self, job_id: &str) -> bool {
        let is_terminal = self.jobs.get(job_id)
            .map(|j| !matches!(j.status, JobStatus::Running))
            .unwrap_or(false);
        if !is_terminal { return false; }
        self.jobs.remove(job_id);
        self.outputs.remove(job_id);
        self.pids.remove(job_id);
        true
    }

    /// Remove every job in a terminal state. Returns the IDs that were
    /// removed so the caller can mirror the change in the frontend store.
    pub fn clear_finished(&mut self) -> Vec<String> {
        let to_remove: Vec<String> = self.jobs.values()
            .filter(|j| !matches!(j.status, JobStatus::Running))
            .map(|j| j.id.clone())
            .collect();
        for id in &to_remove {
            self.jobs.remove(id);
            self.outputs.remove(id);
            self.pids.remove(id);
        }
        to_remove
    }

    /// True when the job exists and is in Cancelled state.
    pub fn is_cancelled(&self, job_id: &str) -> bool {
        self.jobs
            .get(job_id)
            .map(|j| j.status == JobStatus::Cancelled)
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Process kill helpers
// ---------------------------------------------------------------------------

pub fn kill_process(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        use arbor_process_ext::prelude::NoWindowExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F", "/T"])
            .no_window()
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
}
