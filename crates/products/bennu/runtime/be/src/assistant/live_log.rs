//! What a run did with live processes, kept so the thread can end with a card saying so.
//!
//! "I started the stack, sent three requests, two assertions failed" is what the user wants to see
//! after a run that drove running services, without reading every tool receipt. The log is filled by
//! the live tools as they run; [`LiveRunLog::card`] turns it into the block the panel draws.

use serde_json::{json, Value};

/// The most requests listed in the card; the count is always exact.
const MAX_LISTED_REQUESTS: usize = 30;

#[derive(Default)]
pub(super) struct LiveRunLog {
    used: bool,
    started: Vec<String>,
    stopped: Vec<String>,
    requests: Vec<Request>,
    scenarios: Vec<ScenarioNote>,
    assertions_passed: usize,
    assertions_failed: usize,
    files: Vec<String>,
    bevy_changes: Vec<String>,
}

struct Request {
    method: String,
    url: String,
    status: Option<u16>,
    corr: String,
    ms: Option<u64>,
}

struct ScenarioNote {
    name: String,
    passed: bool,
    steps: usize,
    failed: usize,
}

impl LiveRunLog {
    pub(super) fn started(&mut self, name: &str) {
        self.used = true;
        if !self.started.iter().any(|n| n == name) {
            self.started.push(name.to_string());
        }
    }

    pub(super) fn stopped(&mut self, name: &str) {
        self.used = true;
        if !self.stopped.iter().any(|n| n == name) {
            self.stopped.push(name.to_string());
        }
    }

    pub(super) fn request(&mut self, method: &str, url: &str, status: Option<u16>, corr: &str, ms: Option<u64>) {
        self.used = true;
        self.requests.push(Request { method: method.to_string(), url: url.to_string(), status, corr: corr.to_string(), ms });
    }

    pub(super) fn scenario(&mut self, name: &str, passed: bool, steps: usize, failed: usize, assertions_passed: usize, assertions_failed: usize) {
        self.used = true;
        self.scenarios.push(ScenarioNote { name: name.to_string(), passed, steps, failed });
        self.assertions_passed += assertions_passed;
        self.assertions_failed += assertions_failed;
    }

    /// A change made to the running Bevy game through BRP. The game has no undo and no review step, so
    /// every one is listed in the card: it is the user's only record of what the run did to live state.
    pub(super) fn bevy_change(&mut self, what: &str) {
        self.used = true;
        self.bevy_changes.push(what.to_string());
    }

    /// A file the run changed (applied or proposed). Recorded always; shown only if live tools were used.
    pub(super) fn file(&mut self, path: &str) {
        if !self.files.iter().any(|f| f == path) {
            self.files.push(path.to_string());
        }
    }

    /// The card block, or `None` when the run never touched live processes. `still_running` is read from
    /// the manager at the end of the run: what the user may want to stop.
    pub(super) fn card(&self, still_running: &[String], cost_usd: Option<f64>) -> Option<Value> {
        if !self.used {
            return None;
        }
        let ok = self.requests.iter().filter(|r| r.status.is_some_and(|s| s < 400)).count();
        let requests: Vec<Value> = self
            .requests
            .iter()
            .take(MAX_LISTED_REQUESTS)
            .map(|r| json!({ "method": r.method, "url": r.url, "status": r.status, "corr": r.corr, "ms": r.ms }))
            .collect();
        Some(json!({
            "type": "live_summary",
            "summary": {
                "started": self.started,
                "stopped": self.stopped,
                "still_running": still_running,
                "request_count": self.requests.len(),
                "requests_ok": ok,
                "requests": requests,
                "scenarios": self.scenarios.iter().map(|s| json!({ "name": s.name, "passed": s.passed, "steps": s.steps, "failed": s.failed })).collect::<Vec<_>>(),
                "assertions": { "passed": self.assertions_passed, "failed": self.assertions_failed },
                "files": self.files,
                "bevy_change_count": self.bevy_changes.len(),
                "bevy_changes": self.bevy_changes.iter().take(MAX_LISTED_REQUESTS).collect::<Vec<_>>(),
                "cost": cost_usd,
            },
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_that_never_touched_live_has_no_card() {
        let mut log = LiveRunLog::default();
        log.file("A.java");
        assert!(log.card(&[], Some(0.1)).is_none());
    }

    #[test]
    fn the_card_counts_what_the_run_did() {
        let mut log = LiveRunLog::default();
        log.started("api");
        log.started("api");
        log.stopped("api");
        log.request("GET", "http://localhost:8080/a", Some(200), "c1", Some(12));
        log.request("POST", "http://localhost:8080/b", Some(500), "c2", Some(30));
        log.request("GET", "http://localhost:1/", None, "c3", None);
        log.scenario("flow", false, 4, 1, 5, 2);
        log.file("src/A.java");
        log.file("src/A.java");
        let card = log.card(&["api".to_string()], Some(0.25)).unwrap();
        let s = &card["summary"];
        assert_eq!(card["type"], "live_summary");
        assert_eq!(s["started"].as_array().unwrap().len(), 1);
        assert_eq!((s["request_count"].as_u64(), s["requests_ok"].as_u64()), (Some(3), Some(1)));
        assert_eq!(s["assertions"]["failed"], 2);
        assert_eq!(s["files"].as_array().unwrap().len(), 1);
        assert_eq!(s["still_running"][0], "api");
        assert_eq!(s["cost"], 0.25);
    }

    #[test]
    fn changes_to_a_running_game_make_a_card_of_their_own_and_are_all_counted() {
        let mut log = LiveRunLog::default();
        for i in 0..40 {
            log.bevy_change(&format!("Health.current of entity {i} set to 0"));
        }
        let card = log.card(&[], None).expect("a mutation alone is worth a card");
        assert_eq!(card["summary"]["bevy_change_count"], 40);
        assert_eq!(card["summary"]["bevy_changes"].as_array().unwrap().len(), MAX_LISTED_REQUESTS);
    }

    #[test]
    fn listed_requests_are_capped_but_counted() {
        let mut log = LiveRunLog::default();
        for i in 0..50 {
            log.request("GET", "http://localhost/", Some(200), &format!("c{i}"), None);
        }
        let card = log.card(&[], None).unwrap();
        assert_eq!(card["summary"]["requests"].as_array().unwrap().len(), MAX_LISTED_REQUESTS);
        assert_eq!(card["summary"]["request_count"], 50);
    }
}
