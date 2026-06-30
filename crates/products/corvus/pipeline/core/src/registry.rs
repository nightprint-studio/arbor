//! In-memory pipeline run registry — definitions, runs, concurrency locks,
//! cancel tokens, and the global running-count bookkeeping.
//!
//! Pure data structure (no Tauri): the host wraps it in a `Mutex` (paired
//! with a `Condvar` for the concurrency queue) and the orchestrator threads
//! mutate it under that lock. Persistence side effects (`remove_persisted_run`
//! on eviction / discard) go through [`crate::persist`].

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use corvus_pipeline_api::prelude::{PipelineDef, PipelineRun};

use crate::persist::remove_persisted_run;

#[derive(Default)]
pub struct PipelineRegistry {
    pub defs:          Vec<PipelineDef>,
    pub runs:          Vec<PipelineRun>,
    /// Cancel tokens keyed by run_id.
    pub cancel_tokens: HashMap<String, Arc<AtomicBool>>,
    /// lock_key -> run_id currently holding the lock (only set when the run
    /// is actively `Running`). `Failed` / `Paused` / `Success` / `Cancelled`
    /// runs DO NOT hold the lock — the lock is released the moment the run
    /// leaves `Running`.
    pub locks:         HashMap<String, String>,
    counter:           u64,
    /// Number of runs currently in `Running` state — bookkept by the
    /// orchestrator threads under the registry lock so `acquire_run_slot`
    /// / `release_run_slot` are race-free against the global concurrency
    /// cap. Always paired with the host's condvar for queue wake-ups.
    pub running_count: usize,
}

impl PipelineRegistry {
    /// Construct a registry pre-seeded with runs recovered from disk and the
    /// `counter` advanced past the highest recovered run id. Used by
    /// [`crate::persist::registry_from_disk`].
    pub fn from_recovered(runs: Vec<PipelineRun>, counter: u64) -> Self {
        Self { runs, counter, ..Default::default() }
    }

    pub fn new_run_id(&mut self) -> String {
        self.counter += 1;
        format!("pipe-run-{}", self.counter)
    }

    /// Register (or replace) a pipeline definition.
    pub fn register_def(&mut self, def: PipelineDef) {
        if let Some(existing) = self.defs.iter_mut()
            .find(|d| d.id == def.id && d.plugin == def.plugin)
        {
            *existing = def;
        } else {
            self.defs.push(def);
        }
    }

    /// Add a new run. Keeps only the last 50 runs. Does NOT acquire the
    /// lock — the orchestrator does that as soon as it transitions to
    /// Running, so queued runs don't block the lock_key unnecessarily.
    pub fn add_run(&mut self, run: PipelineRun, cancel: Arc<AtomicBool>) {
        self.cancel_tokens.insert(run.id.clone(), cancel);
        self.runs.push(run);
        if self.runs.len() > 50 {
            let old_id = self.runs.remove(0).id;
            self.cancel_tokens.remove(&old_id);
            remove_persisted_run(&old_id);
        }
    }

    /// Overwrite an existing run with an updated snapshot.
    pub fn update_run(&mut self, run: PipelineRun) {
        if let Some(slot) = self.runs.iter_mut().find(|r| r.id == run.id) {
            *slot = run;
        }
    }

    pub fn get_run(&self, run_id: &str) -> Option<&PipelineRun> {
        self.runs.iter().find(|r| r.id == run_id)
    }

    /// Signal the orchestrator for this run to stop after the current step.
    pub fn cancel(&mut self, run_id: &str) {
        if let Some(token) = self.cancel_tokens.get(run_id) {
            token.store(true, Ordering::Relaxed);
        }
    }

    /// Try to take the lock for `lock_key` on behalf of `run_id`.
    /// Returns Err(current_owner_run_id) when another run already owns it.
    /// Idempotent when `run_id` is already the owner.
    pub fn try_acquire_lock(&mut self, lock_key: &str, run_id: &str) -> std::result::Result<(), String> {
        if let Some(owner) = self.locks.get(lock_key) {
            if owner != run_id {
                return Err(owner.clone());
            }
            return Ok(());
        }
        self.locks.insert(lock_key.to_string(), run_id.to_string());
        Ok(())
    }

    /// Release any lock owned by `run_id` (no-op when it holds none).
    pub fn release_lock_of(&mut self, run_id: &str) {
        self.locks.retain(|_, owner| owner != run_id);
    }

    /// Returns the run_id currently holding `lock_key`, if any.
    pub fn locked_by(&self, lock_key: &str) -> Option<&str> {
        self.locks.get(lock_key).map(|s| s.as_str())
    }

    /// Drop a run (and its persisted file). Only call this on a terminal run
    /// that does NOT hold any lock — callers should ensure that themselves.
    pub fn discard(&mut self, run_id: &str) {
        self.cancel_tokens.remove(run_id);
        self.runs.retain(|r| r.id != run_id);
        remove_persisted_run(run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_run_id_is_monotonic() {
        let mut reg = PipelineRegistry::default();
        assert_eq!(reg.new_run_id(), "pipe-run-1");
        assert_eq!(reg.new_run_id(), "pipe-run-2");
    }

    #[test]
    fn from_recovered_continues_past_highest_id() {
        let mut reg = PipelineRegistry::from_recovered(Vec::new(), 7);
        assert_eq!(reg.new_run_id(), "pipe-run-8");
    }

    #[test]
    fn lock_is_exclusive_but_idempotent_for_owner() {
        let mut reg = PipelineRegistry::default();
        assert!(reg.try_acquire_lock("k", "run-1").is_ok());
        // Same owner re-acquires fine.
        assert!(reg.try_acquire_lock("k", "run-1").is_ok());
        // Different run is rejected with the current owner.
        assert_eq!(reg.try_acquire_lock("k", "run-2"), Err("run-1".to_string()));
        assert_eq!(reg.locked_by("k"), Some("run-1"));
    }

    #[test]
    fn release_lock_of_frees_the_key() {
        let mut reg = PipelineRegistry::default();
        reg.try_acquire_lock("k", "run-1").unwrap();
        reg.release_lock_of("run-1");
        assert_eq!(reg.locked_by("k"), None);
        // Now another run can take it.
        assert!(reg.try_acquire_lock("k", "run-2").is_ok());
    }
}
