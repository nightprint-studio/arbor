//! What a test run looks like to a caller that **waited** for it.
//!
//! The two runners ([`crate::tests`], [`crate::cargo_tests`]) exist to fill a live tree:
//! they return the moment the child is up and everything after that arrives as events.
//! That is right for a panel and useless to anything that cannot listen — an AI client
//! gets a run id, reports the tests as run, and never learns that four of them failed.
//!
//! So this module is the other half of the same run: a [`Collector`] the pumps push into
//! while they emit, and a [`TestRunReport`] built from it once the child exits. Nothing
//! here runs anything. The run is the same run, the events are the same events, and the
//! panel fills in exactly as it would have — the caller simply also gets the answer.
//!
//! The two runners report **one** shape. libtest and Surefire disagree about almost
//! everything (a class versus a function, an "error" distinct from a "failure", where a
//! panic message lives), and a caller that had to branch on the build system before
//! reading a failure would be a caller that mostly does not.

use std::collections::HashMap;
use std::sync::Mutex;

use bennu_test::prelude::{TestClassResult, TestStatus};
use serde::Serialize;

/// One test that did not pass.
#[derive(Debug, Clone, Serialize)]
pub struct TestFailure {
    /// How the runner names it — `com.acme.OrderTest#computesTotal`, `orders::tests::total`.
    /// The same spelling [`crate::agent`]'s test catalogue reports, so it can be re-run
    /// directly.
    pub test: String,
    /// The exception type, when there is one (`java.lang.AssertionError`). Never set for a
    /// Rust panic, which has no type.
    pub kind: Option<String>,
    /// What it said. An assertion's message, or the panic's.
    pub message: String,
}

/// A finished run.
#[derive(Debug, Clone, Serialize)]
pub struct TestRunReport {
    /// `maven` or `cargo`.
    pub kind: String,
    /// What was run, in words.
    pub label: String,
    /// The command line, so a run that selected the wrong thing is diagnosable rather than
    /// merely surprising.
    pub command: String,
    /// The child's exit code. `None` when it was killed.
    pub exit_code: Option<i32>,
    /// True when the run was stopped rather than finishing.
    pub cancelled: bool,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    /// Every failure, with its message. Capped — see `note`.
    pub failures: Vec<TestFailure>,
    /// The one thing worth saying about this run that the numbers do not: that it never
    /// compiled, that failures were cut, that it was stopped.
    pub note: Option<String>,
}

/// How a run ended — everything [`TestRunReport`] needs that the [`Collector`] does not have.
///
/// Produced by both runners' `drive`, which is what makes the two reports one shape.
pub struct RunEnd {
    pub code: Option<i32>,
    pub cancelled: bool,
    /// The command line that ran.
    pub command: String,
    /// What was run, in words.
    pub label: String,
    /// The runner's own summary line, as `(run, failed, skipped)`, when it produced one.
    pub totals: Option<(u32, u32, u32)>,
}

/// A failure message is a sentence and a short trace; a megabyte of it is a stack that
/// escaped into the wrong field, and it would cross the seam and land in a model's context.
const MAX_MESSAGE_CHARS: usize = 4_000;
/// Past this many, a caller is not reading failures any more — it is reading a build that
/// broke. The note says how many were left out.
const MAX_FAILURES: usize = 50;

/// Accumulates a run's results while it streams.
///
/// Shared with the pump threads, so everything is behind one lock. Cheap: a failing test is
/// rare, and a passing one costs a counter.
#[derive(Default)]
pub struct Collector {
    inner: Mutex<State>,
}

#[derive(Default)]
struct State {
    passed: u32,
    failed: u32,
    skipped: u32,
    failures: Vec<TestFailure>,
    /// libtest reports a case's verdict and its panic output as two separate events, the
    /// message arriving after the row it belongs to. Keyed by case path until it lands.
    pending_messages: HashMap<String, String>,
}

impl Collector {
    /// A Surefire class report — every case in it at once.
    pub fn class(&self, result: &TestClassResult) {
        let mut state = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        for case in &result.cases {
            match case.status {
                TestStatus::Passed => state.passed += 1,
                TestStatus::Skipped => state.skipped += 1,
                _ => {
                    state.failed += 1;
                    let message = case
                        .message
                        .clone()
                        .or_else(|| case.trace.clone())
                        .unwrap_or_else(|| "failed with no message".to_string());
                    state.failures.push(TestFailure {
                        test: format!("{}#{}", case.classname, case.name),
                        kind: case.kind.clone(),
                        message: cap(message),
                    });
                }
            }
        }
    }

    /// One libtest case verdict.
    pub fn case(&self, path: &str, status: TestStatus) {
        let mut state = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match status {
            TestStatus::Passed => state.passed += 1,
            TestStatus::Skipped => state.skipped += 1,
            _ => {
                state.failed += 1;
                // The panic output may already be here (a run killed mid-failure flushes it
                // before the verdict), so take it rather than wait for it.
                let message = state
                    .pending_messages
                    .remove(path)
                    .unwrap_or_else(|| "failed with no output".to_string());
                state.failures.push(TestFailure {
                    test: path.to_string(),
                    kind: None,
                    message: cap(message),
                });
            }
        }
    }

    /// A libtest failure's output, which arrives after the verdict it belongs to.
    pub fn message(&self, path: &str, output: &str) {
        let mut state = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match state.failures.iter_mut().find(|f| f.test == path) {
            Some(failure) => failure.message = cap(output.to_string()),
            // Out of order: hold it for the verdict that has not arrived yet.
            None => {
                state.pending_messages.insert(path.to_string(), output.to_string());
            }
        }
    }

    /// Build the report. `totals` is the runner's own summary line when it produced one —
    /// authoritative where it disagrees with the count, because a class whose report never
    /// landed still appears in Maven's tally.
    pub fn finish(
        &self,
        kind: &str,
        label: String,
        command: String,
        exit_code: Option<i32>,
        cancelled: bool,
        totals: Option<(u32, u32, u32)>,
    ) -> TestRunReport {
        let mut state = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let (passed, failed, skipped) = match totals {
            Some((run, failures, skipped)) => {
                (run.saturating_sub(failures + skipped), failures, skipped)
            }
            None => (state.passed, state.failed, state.skipped),
        };

        let total_failures = state.failures.len();
        let mut failures = std::mem::take(&mut state.failures);
        failures.truncate(MAX_FAILURES);

        let note = if cancelled {
            Some("The run was stopped before it finished, so these numbers are partial.".into())
        } else if passed + failed + skipped == 0 {
            // The commonest way a run "passes" while proving nothing: it never got as far as
            // running a test. Saying so is the difference between a green answer and an honest
            // one.
            Some(format!(
                "No test ran. The build itself most likely failed{} — the runner's own output \
                 says why, and Bennu's Tests panel has it.",
                match exit_code {
                    Some(code) if code != 0 => format!(" (exit code {code})"),
                    _ => String::new(),
                }
            ))
        } else if total_failures > MAX_FAILURES {
            Some(format!(
                "Showing {MAX_FAILURES} of {total_failures} failures. Fix these and run again \
                 rather than asking for the rest — a run this broken usually has one cause."
            ))
        } else {
            None
        };

        TestRunReport {
            kind: kind.to_string(),
            label,
            command,
            exit_code,
            cancelled,
            passed,
            failed,
            skipped,
            failures,
            note,
        }
    }
}

/// Keep a failure readable without letting a deep recursion's trace cross the seam.
fn cap(mut message: String) -> String {
    if message.chars().count() <= MAX_MESSAGE_CHARS {
        return message;
    }
    let end = message
        .char_indices()
        .nth(MAX_MESSAGE_CHARS)
        .map(|(byte, _)| byte)
        .unwrap_or(message.len());
    message.truncate(end);
    message.push_str("\n… [truncated]");
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_test::prelude::TestCaseResult;

    fn case(name: &str, status: TestStatus, message: Option<&str>) -> TestCaseResult {
        TestCaseResult {
            name: name.to_string(),
            classname: "com.acme.OrderTest".to_string(),
            status,
            time_ms: 1,
            message: message.map(str::to_string),
            kind: Some("java.lang.AssertionError".to_string()),
            trace: None,
            flaky: false,
        }
    }

    fn class(cases: Vec<TestCaseResult>) -> TestClassResult {
        TestClassResult {
            classname: "com.acme.OrderTest".to_string(),
            total: cases.len() as u32,
            failures: 0,
            errors: 0,
            skipped: 0,
            time_ms: 1,
            cases,
            system_out: None,
            system_err: None,
        }
    }

    #[test]
    fn a_surefire_class_is_counted_and_its_failures_kept() {
        let collector = Collector::default();
        collector.class(&class(vec![
            case("ok", TestStatus::Passed, None),
            case("bad", TestStatus::Failed, Some("expected:<1> but was:<2>")),
            case("later", TestStatus::Skipped, None),
        ]));
        let report = collector.finish("maven", "l".into(), "c".into(), Some(1), false, None);
        assert_eq!((report.passed, report.failed, report.skipped), (1, 1, 1));
        assert_eq!(report.failures[0].test, "com.acme.OrderTest#bad");
        assert!(report.failures[0].message.contains("but was"));
    }

    #[test]
    fn a_libtest_message_finds_its_verdict_whichever_arrives_first() {
        // Verdict, then output — the normal order.
        let a = Collector::default();
        a.case("m::t::a", TestStatus::Failed);
        a.message("m::t::a", "panicked at 'boom'");
        assert!(a.finish("cargo", "l".into(), "c".into(), Some(101), false, None).failures[0]
            .message
            .contains("boom"));

        // Output, then verdict — what a run killed mid-failure produces.
        let b = Collector::default();
        b.message("m::t::b", "panicked at 'flushed'");
        b.case("m::t::b", TestStatus::Failed);
        assert!(b.finish("cargo", "l".into(), "c".into(), None, true, None).failures[0]
            .message
            .contains("flushed"));
    }

    #[test]
    fn the_runners_own_summary_wins_over_the_count() {
        // Maven tallies classes whose report never landed; its summary line is the truth.
        let collector = Collector::default();
        collector.class(&class(vec![case("ok", TestStatus::Passed, None)]));
        let report =
            collector.finish("maven", "l".into(), "c".into(), Some(0), false, Some((10, 2, 1)));
        assert_eq!((report.passed, report.failed, report.skipped), (7, 2, 1));
    }

    #[test]
    fn a_run_that_never_reached_a_test_says_so_instead_of_reading_as_green() {
        let report =
            Collector::default().finish("maven", "l".into(), "c".into(), Some(1), false, None);
        assert_eq!(report.failed, 0);
        let note = report.note.unwrap();
        assert!(note.contains("No test ran"), "{note}");
        assert!(note.contains("exit code 1"), "{note}");
    }

    #[test]
    fn a_stopped_run_says_its_numbers_are_partial() {
        let collector = Collector::default();
        collector.case("m::t::a", TestStatus::Passed);
        let report = collector.finish("cargo", "l".into(), "c".into(), None, true, None);
        assert!(report.note.unwrap().contains("stopped"));
    }

    #[test]
    fn a_trace_that_escaped_into_the_message_is_capped() {
        let collector = Collector::default();
        collector.case("m::t::a", TestStatus::Failed);
        collector.message("m::t::a", &"x".repeat(50_000));
        let report = collector.finish("cargo", "l".into(), "c".into(), Some(101), false, None);
        assert!(report.failures[0].message.ends_with("[truncated]"));
        assert!(report.failures[0].message.chars().count() < 5_000);
    }
}
