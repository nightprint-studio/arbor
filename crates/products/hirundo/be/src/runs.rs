//! The runner's history: the last runs of a project, newest first, kept in the profile (see
//! [`crate::project_state`]) so they survive the window and never reach a diff.

use hirundo_core::prelude::HirundoState;
use serde::{Deserialize, Serialize};

use crate::project_state::{load, save};

const FILE: &str = "runs";
/// Runs kept per project.
const MAX_RUNS: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRow {
    pub path: String,
    pub name: String,
    pub method: String,
    pub status: Option<u16>,
    pub time_ms: Option<u64>,
    pub passed: bool,
    /// Why nothing came back, or the failed checks, one line each.
    #[serde(default)]
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub at_ms: i64,
    /// The collection or folder that ran.
    pub label: String,
    pub env_name: String,
    /// `manual`, or the name of the schedule that started it.
    pub source: String,
    pub duration_ms: u64,
    pub rows: Vec<RunRow>,
}

fn push_capped(runs: &mut Vec<RunRecord>, record: RunRecord) {
    runs.insert(0, record);
    runs.truncate(MAX_RUNS);
}

#[arbor_rpc::handler]
fn runs_list(_state: &HirundoState, project: String) -> Result<Vec<RunRecord>, String> {
    Ok(load(&project, FILE))
}

#[arbor_rpc::handler]
fn run_record(_state: &HirundoState, project: String, record: RunRecord) -> Result<(), String> {
    let mut runs: Vec<RunRecord> = load(&project, FILE);
    push_capped(&mut runs, record);
    save(&project, FILE, &runs)
}

#[arbor_rpc::handler]
fn runs_clear(_state: &HirundoState, project: String) -> Result<(), String> {
    save(&project, FILE, &Vec::<RunRecord>::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(at: i64) -> RunRecord {
        RunRecord { at_ms: at, label: "bandi".into(), env_name: "Staging".into(), source: "manual".into(), duration_ms: 1, rows: vec![] }
    }

    #[test]
    fn the_newest_run_is_first_and_the_history_is_capped() {
        let mut runs = Vec::new();
        for i in 0..(MAX_RUNS as i64 + 10) {
            push_capped(&mut runs, record(i));
        }
        assert_eq!(runs.len(), MAX_RUNS);
        assert_eq!(runs[0].at_ms, MAX_RUNS as i64 + 9);
    }
}
