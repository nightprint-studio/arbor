//! What the Schedules window's handlers do, once. The handlers themselves stay in each product (they bind to
//! its state type); each is one line over these.

use arbor_scheduler::prelude::{new_id, Job, JobAction, JobInput, Store};

use crate::flavour::Flavour;

/// Create or edit a job (`input.id` absent creates). Returns it as saved, with its next run.
pub fn save<F: Flavour>(f: &F, store: &Store<F::Action>, input: JobInput<F::Action>, now_ms: i64) -> Result<Job<F::Action>, String> {
    let existing = input.id.as_deref().and_then(|id| store.get(id));
    store.upsert(Job::from_input(existing, input, now_ms, new_id, |m| f.mode_word(m))?)
}

/// Turn a job on or off. Turning it on works out the next run from now.
pub fn set_enabled<A: JobAction + Send>(store: &Store<A>, id: &str, enabled: bool, now_ms: i64) -> Result<Job<A>, String> {
    store.mutate(id, |j| {
        j.enabled = enabled;
        j.schedule_from(now_ms)?;
        Ok(j.clone())
    })
}

/// A run's end as written on its job.
pub struct Recorded<A> {
    pub job: Job<A>,
    /// This failure starts an episode: the run before it went well, or there was none. Only that one is worth a
    /// toast — a schedule that keeps failing every hour must not shout every hour.
    pub first_failure: bool,
}

/// Write down how a run ended. `None` when the job was deleted meanwhile: there is nothing left to tell.
pub fn record<A: JobAction + Send>(store: &Store<A>, id: &str, now_ms: i64, ok: bool, detail: &str) -> Option<Recorded<A>> {
    store
        .mutate(id, |j| {
            let first_failure = !ok && j.last_run.as_ref().map_or(true, |l| l.ok || l.skipped);
            j.finished(now_ms, ok, detail);
            Ok(Recorded { job: j.clone(), first_failure })
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_scheduler::prelude::{MissedPolicy, Recurrence};
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Act;
    impl JobAction for Act {
        fn word(&self) -> &'static str {
            "act"
        }
    }

    struct F;
    impl Flavour for F {
        type Action = Act;
        fn app(&self) -> &'static str { "T" }
        fn unit(&self) -> &'static str { "repository" }
        fn actions(&self) -> &'static [&'static str] { &["act"] }
        fn modes(&self) -> &'static [&'static str] { &["manual"] }
        fn actions_doc(&self) -> &'static str { "" }
        fn modes_doc(&self) -> &'static str { "" }
        fn action_from_input(&self, _: Option<&str>, _: Option<String>, _: &Value, _: &str) -> Result<Act, String> { Ok(Act) }
        fn with_prompt(&self, current: &Act, _: String) -> Act { current.clone() }
        fn mode_word(&self, _: &str) -> String { "manual".into() }
        fn mode_for(&self, _: Option<&str>, _: &str) -> String { "manual".into() }
        fn lowered_note(&self) -> &'static str { "" }
    }

    fn input(id: Option<&str>) -> JobInput<Act> {
        JobInput { id: id.map(str::to_string), name: "Nightly".into(), enabled: true, recurrence: Recurrence::Daily { hour: 2, minute: 0 }, missed: MissedPolicy::Skip, action: Act, root: "/r".into(), mode: "auto".into() }
    }

    fn store(tag: &str) -> (Store<Act>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("imber-window-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (Store::new(dir.join("s.json")), dir)
    }

    #[test]
    fn saving_creates_then_edits_keeping_the_history() {
        let (s, dir) = store("save");
        let made = save(&F, &s, input(None), 1_000).unwrap();
        assert!(made.next_run_ms.is_some());
        record(&s, &made.id, 5_000, true, "done");
        let mut edit = input(Some(&made.id));
        edit.name = "Renamed".into();
        let edited = save(&F, &s, edit, 9_000).unwrap();
        assert_eq!((edited.name.as_str(), edited.created_ms), ("Renamed", 1_000));
        assert_eq!(edited.last_run.unwrap().detail, "done");
        assert_eq!(s.list().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn turning_a_job_off_clears_its_next_run_and_on_works_it_out_again() {
        let (s, dir) = store("toggle");
        let made = save(&F, &s, input(None), 1_000).unwrap();
        assert_eq!(set_enabled(&s, &made.id, false, 2_000).unwrap().next_run_ms, None);
        assert!(set_enabled(&s, &made.id, true, 2_000).unwrap().next_run_ms.is_some());
        assert!(set_enabled(&s, "nope", true, 0).is_err());
        assert!(record(&s, "nope", 0, true, "ignored").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_the_first_failure_of_an_episode_is_news() {
        let (s, dir) = store("episode");
        let id = save(&F, &s, input(None), 1_000).unwrap().id;
        assert!(record(&s, &id, 1, false, "boom").unwrap().first_failure, "no run before it");
        assert!(!record(&s, &id, 2, false, "boom again").unwrap().first_failure, "still the same episode");
        assert!(!record(&s, &id, 3, true, "fine").unwrap().first_failure);
        assert!(record(&s, &id, 4, false, "boom").unwrap().first_failure, "a new episode after a good run");
        let _ = std::fs::remove_dir_all(dir);
    }
}
