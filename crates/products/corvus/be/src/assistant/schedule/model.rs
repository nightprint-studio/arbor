//! A scheduled Imber job as data, and the rules that move it forward. Pure: the clock is a parameter, so the
//! whole life of a job — created, due, run, missed, finished — is testable without waiting for anything.

use arbor_scheduler::prelude::{due, next_after, validate_recurrence, Due, MissedPolicy, Recurrence};
use serde::{Deserialize, Serialize};

use crate::assistant::permission::Mode;

pub const MAX_PROMPT_CHARS: usize = 4_000;
const MAX_NAME_CHARS: usize = 80;
/// The part of a run's answer kept as its outcome line.
pub const MAX_DETAIL_CHARS: usize = 300;

/// What a job does when it fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// "Awake": wake a conversation with a saved prompt. The conversation keeps its history, so Imber picks up
    /// where it stopped. With no conversation (it is gone after a restart) it starts a new one.
    Awake { conversation_id: Option<String>, prompt: String },
    /// A fresh conversation each time, with this prompt.
    Prompt { prompt: String },
    /// `git fetch` — no model, no cost.
    Fetch { remote: Option<String> },
}

impl Action {
    pub fn prompt(&self) -> Option<&str> {
        match self {
            Action::Awake { prompt, .. } | Action::Prompt { prompt } => Some(prompt),
            Action::Fetch { .. } => None,
        }
    }

    pub fn word(&self) -> &'static str {
        match self {
            Action::Awake { .. } => "awake",
            Action::Prompt { .. } => "prompt",
            Action::Fetch { .. } => "fetch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastRun {
    pub at_ms: i64,
    pub ok: bool,
    /// One line: what the run answered, or why it failed or was not run.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub recurrence: Recurrence,
    #[serde(default)]
    pub missed: MissedPolicy,
    pub action: Action,
    /// The repository the job works on.
    pub root: String,
    /// What the job may do with nobody there to approve: `auto` lets writes run, `manual` (the default) leaves
    /// it read-only. Destructive actions never run unattended.
    #[serde(default = "manual")]
    pub mode: String,
    pub created_ms: i64,
    pub next_run_ms: Option<i64>,
    #[serde(default)]
    pub last_run: Option<LastRun>,
    #[serde(default)]
    pub runs: u32,
}

fn manual() -> String {
    "manual".into()
}

/// What a tick finds to do with a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Nothing,
    Run,
}

impl Job {
    pub fn mode(&self) -> Mode {
        Mode::parse(&self.mode)
    }

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
        validate_recurrence(&self.recurrence)
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
                if matches!(self.recurrence, Recurrence::Once { .. }) {
                    self.last_run = Some(LastRun { at_ms: now_ms, ok: false, detail: "Missed: Corvus was not running at the time, and the schedule skips missed runs.".into() });
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
        self.last_run = Some(LastRun { at_ms, ok, detail: clip(detail) });
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

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_scheduler::prelude::GRACE_MS;

    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;

    fn job(recurrence: Recurrence, missed: MissedPolicy) -> Job {
        Job {
            id: "j1".into(),
            name: "Morning fetch".into(),
            enabled: true,
            recurrence,
            missed,
            action: Action::Fetch { remote: None },
            root: "/work/shop".into(),
            mode: "manual".into(),
            created_ms: 0,
            next_run_ms: None,
            last_run: None,
            runs: 0,
        }
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
        bad.action = Action::Prompt { prompt: " ".into() };
        assert!(bad.validate().unwrap_err().contains("prompt is empty"));
        bad.action = Action::Awake { conversation_id: None, prompt: "x".repeat(MAX_PROMPT_CHARS + 1) };
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
        assert!(!last.ok && last.detail.starts_with("Missed"), "{last:?}");
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
        j.action = Action::Awake { conversation_id: Some("c-1".into()), prompt: "status?".into() };
        let text = serde_json::to_string(&j).unwrap();
        assert!(text.contains("\"kind\":\"awake\"") && text.contains("\"kind\":\"weekly\""), "{text}");
        assert_eq!(serde_json::from_str::<Job>(&text).unwrap(), j);
    }
}
