//! What the frontend is told about a schedule's runs, outside any conversation: a toast when one starts, and one
//! when a run fails after a good one. The payload is the same for every product; the topic is the product's.

use arbor_scheduler::prelude::{Job, JobAction};
use serde_json::{json, Value};

/// A run of `job` has just started.
pub fn started<A: JobAction>(job: &Job<A>) -> Value {
    json!({ "kind": "started", "id": job.id, "name": job.name, "root": job.root, "action": job.action.word() })
}

/// A run of `job` failed (the first failure of an episode); `detail` is its one-line outcome.
pub fn failed<A: JobAction>(job: &Job<A>, detail: &str) -> Value {
    json!({ "kind": "failed", "id": job.id, "name": job.name, "root": job.root, "action": job.action.word(), "detail": detail })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_scheduler::prelude::{MissedPolicy, Recurrence};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Act;
    impl JobAction for Act {
        fn word(&self) -> &'static str {
            "prompt"
        }
    }

    fn job() -> Job<Act> {
        Job { id: "j".into(), name: "Morning check".into(), enabled: true, recurrence: Recurrence::Daily { hour: 9, minute: 0 }, missed: MissedPolicy::Skip, action: Act, root: "/r".into(), mode: "manual".into(), created_ms: 0, next_run_ms: None, last_run: None, runs: 0 }
    }

    #[test]
    fn the_events_name_the_schedule() {
        let s = started(&job());
        assert_eq!((s["kind"].as_str(), s["name"].as_str(), s["action"].as_str()), (Some("started"), Some("Morning check"), Some("prompt")));
        let f = failed(&job(), "no API key");
        assert_eq!((f["kind"].as_str(), f["detail"].as_str()), (Some("failed"), Some("no API key")));
    }
}
