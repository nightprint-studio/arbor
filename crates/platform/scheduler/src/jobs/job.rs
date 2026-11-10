//! A scheduled job as data, and the rules that move it forward. Pure: the clock is a parameter, so the whole
//! life of a job — created, due, run, missed, finished — is testable without waiting for anything.
//!
//! The job is generic over *what it does* ([`JobAction`]): Imber's schedules run a prompt, Hirundo's run a
//! collection. Everything else — when, the missed-run policy, the history of the last run — is the same.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::calendar::{due, next_after, validate, Due, MissedPolicy, Recurrence};

pub const MAX_PROMPT_CHARS: usize = 4_000;
pub const MAX_NAME_CHARS: usize = 80;
/// The part of a run's answer kept as its outcome line.
pub const MAX_DETAIL_CHARS: usize = 300;

/// What a job does when it fires. Implemented by each product's action enum.
pub trait JobAction: Clone + PartialEq + Serialize + DeserializeOwned + Send + 'static {
    /// The text sent to a model, for the actions that have one (checked for emptiness and length on save).
    fn prompt(&self) -> Option<&str> {
        None
    }

    /// A short lowercase word naming the action, for tools and logs.
    fn word(&self) -> &'static str;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastRun {
    pub at_ms: i64,
    pub ok: bool,
    /// One line: what the run answered, or why it failed or was not run.
    pub detail: String,
    /// The run was never started: its time passed while nothing was running and the policy dropped it.
    #[serde(default)]
    pub skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job<A> {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub recurrence: Recurrence,
    #[serde(default)]
    pub missed: MissedPolicy,
    pub action: A,
    /// The repository or project the job works on.
    pub root: String,
    /// What the job may do with nobody there to approve. The words are the product's (`manual` / `auto`).
    #[serde(default = "default_mode")]
    pub mode: String,
    pub created_ms: i64,
    pub next_run_ms: Option<i64>,
    #[serde(default)]
    pub last_run: Option<LastRun>,
    #[serde(default)]
    pub runs: u32,
}

fn default_mode() -> String {
    "manual".into()
}

/// What a tick finds to do with a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Nothing,
    Run,
}

impl<A: JobAction> Job<A> {
    /// Whether the job may be saved, with the reason when it may not.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the schedule needs a name".into());
        }
        if self.name.chars().count() > MAX_NAME_CHARS {
            return Err(format!("the name is longer than {MAX_NAME_CHARS} characters"));
        }
        if self.root.trim().is_empty() {
            return Err("the schedule needs a repository".into());
        }
        if let Some(p) = self.action.prompt() {
            if p.trim().is_empty() {
                return Err("the saved prompt is empty".into());
            }
            if p.chars().count() > MAX_PROMPT_CHARS {
                return Err(format!("the prompt is longer than {MAX_PROMPT_CHARS} characters"));
            }
        }
        validate(&self.recurrence)
    }

    /// Work out `next_run_ms` from `now` (after creating or editing). A disabled job, or a one-time run that is
    /// already past, has none.
    pub fn schedule_from(&mut self, now_ms: i64) -> Result<(), String> {
        self.next_run_ms = if self.enabled { next_after(&self.recurrence, now_ms)? } else { None };
        Ok(())
    }

    /// Look at the job at `now_ms`: run it if it is due; drop a missed run the policy says to drop.
    pub fn tick(&mut self, now_ms: i64) -> Tick {
        let Some(next) = self.next_run_ms.filter(|_| self.enabled) else { return Tick::Nothing };
        match due(next, now_ms, self.missed) {
            Due::Wait => Tick::Nothing,
            Due::Run => Tick::Run,
            Due::Skip => {
                self.last_run = Some(LastRun { at_ms: now_ms, ok: false, detail: "Missed: Arbor was not running at the time, and the schedule skips missed runs.".into(), skipped: true });
                if matches!(self.recurrence, Recurrence::Once { .. }) {
                    self.finish_once();
                } else {
                    self.next_run_ms = next_after(&self.recurrence, now_ms).ok().flatten();
                }
                Tick::Nothing
            }
        }
    }

    /// Record that the job started now: the next run is worked out at once, so a slow run cannot be started
    /// twice by the next tick.
    pub fn started(&mut self, now_ms: i64) {
        self.runs += 1;
        if matches!(self.recurrence, Recurrence::Once { .. }) {
            self.finish_once();
        } else {
            self.next_run_ms = next_after(&self.recurrence, now_ms).ok().flatten();
        }
    }

    pub fn finished(&mut self, at_ms: i64, ok: bool, detail: &str) {
        self.last_run = Some(LastRun { at_ms, ok, detail: clip(detail), skipped: false });
    }

    fn finish_once(&mut self) {
        self.next_run_ms = None;
        self.enabled = false;
    }
}

/// The first line of `text`, bounded.
pub fn clip(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut out: String = line.chars().take(MAX_DETAIL_CHARS).collect();
    if line.chars().count() > MAX_DETAIL_CHARS {
        out.push('…');
    }
    out
}

/// What a window sends to save a job (`id` absent creates).
#[derive(Debug, Deserialize)]
pub struct JobInput<A> {
    pub id: Option<String>,
    pub name: String,
    pub enabled: bool,
    pub recurrence: Recurrence,
    #[serde(default)]
    pub missed: MissedPolicy,
    pub action: A,
    pub root: String,
    #[serde(default)]
    pub mode: String,
}

impl<A: JobAction> Job<A> {
    /// The job a save produces: new, or the stored one with the edits, keeping its history. Pure over the stored
    /// job and the clock. An interval is anchored to now whenever it is new or its cadence changed.
    /// `mode_word` turns whatever the caller sent into one of the product's mode words.
    pub fn from_input(existing: Option<Job<A>>, input: JobInput<A>, now_ms: i64, new_id: impl FnOnce() -> String, mode_word: impl FnOnce(&str) -> String) -> Result<Job<A>, String> {
        let mut recurrence = input.recurrence;
        if let Recurrence::Interval { secs, anchor_ms } = &mut recurrence {
            let kept = match existing.as_ref().map(|j| &j.recurrence) {
                Some(Recurrence::Interval { secs: stored, anchor_ms: a }) if stored == secs => Some(*a),
                _ => None,
            };
            *anchor_ms = kept.unwrap_or(now_ms);
        }
        let mut job = Job {
            id: input.id.filter(|i| !i.is_empty()).unwrap_or_else(new_id),
            name: input.name.trim().to_string(),
            enabled: input.enabled,
            recurrence,
            missed: input.missed,
            action: input.action,
            root: input.root,
            mode: mode_word(&input.mode),
            created_ms: existing.as_ref().map_or(now_ms, |j| j.created_ms),
            next_run_ms: None,
            last_run: existing.as_ref().and_then(|j| j.last_run.clone()),
            runs: existing.as_ref().map_or(0, |j| j.runs),
        };
        job.validate()?;
        job.schedule_from(now_ms)?;
        Ok(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::GRACE_MS;

    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum Act {
        Awake { conversation_id: Option<String>, prompt: String },
        Prompt { prompt: String },
        Fetch,
    }

    impl JobAction for Act {
        fn prompt(&self) -> Option<&str> {
            match self {
                Act::Awake { prompt, .. } | Act::Prompt { prompt } => Some(prompt),
                Act::Fetch => None,
            }
        }
        fn word(&self) -> &'static str {
            "act"
        }
    }

    fn job(recurrence: Recurrence, missed: MissedPolicy) -> Job<Act> {
        Job { id: "j1".into(), name: "Morning fetch".into(), enabled: true, recurrence, missed, action: Act::Fetch, root: "/work/shop".into(), mode: "manual".into(), created_ms: 0, next_run_ms: None, last_run: None, runs: 0 }
    }

    fn input(recurrence: Recurrence) -> JobInput<Act> {
        JobInput { id: None, name: " Morning check ".into(), enabled: true, recurrence, missed: MissedPolicy::Skip, action: Act::Prompt { prompt: "status?".into() }, root: "/r".into(), mode: "auto".into() }
    }

    fn make(existing: Option<Job<Act>>, i: JobInput<Act>, now: i64) -> Result<Job<Act>, String> {
        Job::from_input(existing, i, now, || "sch-1".into(), |m| if m == "auto" { "auto".into() } else { "manual".into() })
    }

    #[test]
    fn a_job_needs_a_name_a_repository_a_prompt_and_a_valid_recurrence() {
        let ok = job(Recurrence::Interval { secs: 3600, anchor_ms: 0 }, MissedPolicy::Skip);
        assert!(ok.validate().is_ok());
        let mut bad = ok.clone();
        bad.name = "  ".into();
        assert!(bad.validate().unwrap_err().contains("name"));
        let mut bad = ok.clone();
        bad.root.clear();
        assert!(bad.validate().unwrap_err().contains("repository"));
        let mut bad = ok.clone();
        bad.action = Act::Prompt { prompt: " ".into() };
        assert!(bad.validate().unwrap_err().contains("prompt is empty"));
        bad.action = Act::Awake { conversation_id: None, prompt: "x".repeat(MAX_PROMPT_CHARS + 1) };
        assert!(bad.validate().unwrap_err().contains("longer"));
        let mut bad = ok;
        bad.recurrence = Recurrence::Interval { secs: 5, anchor_ms: 0 };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn scheduling_sets_the_next_run_and_a_disabled_job_has_none() {
        let mut j = job(Recurrence::Interval { secs: 3600, anchor_ms: 0 }, MissedPolicy::Skip);
        j.schedule_from(10 * MIN).unwrap();
        assert_eq!(j.next_run_ms, Some(HOUR));
        j.enabled = false;
        j.schedule_from(10 * MIN).unwrap();
        assert_eq!(j.next_run_ms, None);
    }

    #[test]
    fn a_due_job_runs_once_and_its_next_run_moves_on_immediately() {
        let mut j = job(Recurrence::Interval { secs: 3600, anchor_ms: 0 }, MissedPolicy::Skip);
        j.schedule_from(0).unwrap();
        assert_eq!(j.tick(HOUR - MIN), Tick::Nothing);
        assert_eq!(j.tick(HOUR + 5_000), Tick::Run);
        j.started(HOUR + 5_000);
        assert_eq!(j.next_run_ms, Some(2 * HOUR));
        assert_eq!(j.tick(HOUR + 20_000), Tick::Nothing, "the next tick does not start it again");
        assert_eq!(j.runs, 1);
    }

    #[test]
    fn a_missed_run_is_dropped_or_caught_up_by_policy() {
        let mut skip = job(Recurrence::Interval { secs: 3600, anchor_ms: 0 }, MissedPolicy::Skip);
        skip.schedule_from(0).unwrap();
        let late = HOUR + GRACE_MS + 1;
        assert_eq!(skip.tick(late), Tick::Nothing);
        assert_eq!(skip.next_run_ms, Some(2 * HOUR), "it waits for the next one");
        assert!(skip.last_run.as_ref().is_some_and(|l| l.skipped && !l.ok), "and says it dropped one");

        let mut catch_up = job(Recurrence::Interval { secs: 3600, anchor_ms: 0 }, MissedPolicy::RunOnce);
        catch_up.schedule_from(0).unwrap();
        assert_eq!(catch_up.tick(5 * HOUR), Tick::Run, "run once however many were missed");
        catch_up.started(5 * HOUR);
        assert_eq!(catch_up.next_run_ms, Some(6 * HOUR));
    }

    #[test]
    fn a_one_time_job_ends_after_it_runs_and_says_so_when_it_was_missed() {
        let mut j = job(Recurrence::Once { at_ms: HOUR }, MissedPolicy::Skip);
        j.schedule_from(0).unwrap();
        assert_eq!(j.next_run_ms, Some(HOUR));
        assert_eq!(j.tick(HOUR), Tick::Run);
        j.started(HOUR);
        assert!(!j.enabled && j.next_run_ms.is_none());

        let mut missed = job(Recurrence::Once { at_ms: HOUR }, MissedPolicy::Skip);
        missed.schedule_from(0).unwrap();
        assert_eq!(missed.tick(10 * HOUR), Tick::Nothing);
        assert!(!missed.enabled);
        let last = missed.last_run.unwrap();
        assert!(!last.ok && last.skipped && last.detail.starts_with("Missed"), "{last:?}");
    }

    #[test]
    fn a_disabled_job_never_ticks() {
        let mut j = job(Recurrence::Interval { secs: 3600, anchor_ms: 0 }, MissedPolicy::RunOnce);
        j.schedule_from(0).unwrap();
        j.enabled = false;
        assert_eq!(j.tick(10 * HOUR), Tick::Nothing);
    }

    #[test]
    fn the_outcome_line_is_the_first_line_bounded() {
        assert_eq!(clip("\n  Fetched 2 branches\nmore"), "Fetched 2 branches");
        let long = clip(&"x".repeat(MAX_DETAIL_CHARS + 50));
        assert_eq!(long.chars().count(), MAX_DETAIL_CHARS + 1);
        assert!(long.ends_with('…'));
        assert_eq!(clip(""), "");
    }

    #[test]
    fn a_job_survives_a_round_trip_through_json() {
        let mut j = job(Recurrence::Weekly { days: vec![0, 4], hour: 9, minute: 0 }, MissedPolicy::RunOnce);
        j.action = Act::Awake { conversation_id: Some("c-1".into()), prompt: "status?".into() };
        let text = serde_json::to_string(&j).unwrap();
        assert!(text.contains("\"kind\":\"awake\"") && text.contains("\"kind\":\"weekly\""), "{text}");
        assert_eq!(serde_json::from_str::<Job<Act>>(&text).unwrap(), j);
    }

    #[test]
    fn a_file_from_before_the_skipped_flag_still_reads() {
        let text = r#"{"id":"a","name":"n","enabled":true,"recurrence":{"kind":"daily","hour":9,"minute":0},"action":{"kind":"fetch"},"root":"/r","created_ms":0,"next_run_ms":null,"last_run":{"at_ms":5,"ok":true,"detail":"Fetched origin."}}"#;
        let j: Job<Act> = serde_json::from_str(text).unwrap();
        assert!(!j.last_run.unwrap().skipped);
        assert_eq!(j.mode, "manual");
    }

    #[test]
    fn a_new_job_is_validated_and_scheduled_and_keeps_its_cleaned_name() {
        let job = make(None, input(Recurrence::Interval { secs: 600, anchor_ms: 0 }), 10 * MIN).unwrap();
        assert_eq!(job.name, "Morning check");
        assert_eq!(job.mode, "auto");
        assert_eq!(job.next_run_ms, Some(20 * MIN), "an interval is anchored to now and first runs one interval on");
        assert_eq!(job.id, "sch-1");
        assert!(make(None, input(Recurrence::Interval { secs: 5, anchor_ms: 0 }), 0).is_err());
    }

    #[test]
    fn editing_keeps_history_and_the_interval_anchor_unless_the_cadence_changed() {
        let mut stored = make(None, input(Recurrence::Interval { secs: 600, anchor_ms: 0 }), 10 * MIN).unwrap();
        stored.runs = 4;
        stored.finished(15 * MIN, true, "ok");
        let mut same = input(Recurrence::Interval { secs: 600, anchor_ms: 0 });
        same.id = Some(stored.id.clone());
        let edited = make(Some(stored.clone()), same, 100 * MIN).unwrap();
        assert_eq!((edited.runs, edited.created_ms), (4, stored.created_ms));
        assert_eq!(edited.last_run, stored.last_run);
        assert_eq!(edited.recurrence, stored.recurrence, "same cadence: same anchor");
        let mut changed = input(Recurrence::Interval { secs: 1200, anchor_ms: 0 });
        changed.id = Some(stored.id.clone());
        let moved = make(Some(stored), changed, 100 * MIN).unwrap();
        assert_eq!(moved.recurrence, Recurrence::Interval { secs: 1200, anchor_ms: 100 * MIN });
    }

    #[test]
    fn a_disabled_job_has_no_next_run() {
        let mut i = input(Recurrence::Daily { hour: 9, minute: 0 });
        i.enabled = false;
        assert_eq!(make(None, i, 0).unwrap().next_run_ms, None);
    }
}
