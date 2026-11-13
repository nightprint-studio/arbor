//! Reading a `cargo nextest run` back.
//!
//! Where `cargo test` splits its answer across two pipes (see [`crate::cargo_report`]), nextest says everything
//! on one, and says it per test: each verdict line names the test **and its binary**, so there is no pairing to
//! do. The run is started with the flags [`crate::cargo_selector::cargo_plan_with`] gives it, and this reads what
//! those flags produce:
//!
//! ```text
//!     Starting 3 tests across 2 binaries
//!         PASS [   0.004s] demo cli::tests::parses
//!         SKIP [   0.000s] demo::api slow_one
//!         FAIL [   0.010s] demo::bin/tool tests::breaks
//! ──── STDERR:             demo::bin/tool tests::breaks
//! thread 'tests::breaks' panicked at src/main.rs:9:5:
//! ...
//!      Summary [   0.512s] 3 tests run: 1 passed, 1 failed, 1 skipped
//! ```
//!
//! The binary id is `crate` for the library, `crate::bin/name`, `crate::bench/name`, `crate::example/name`, and
//! `crate::name` for an integration test — which is enough to place the test in the same tree the cargo runner
//! fills. Newer versions put a `(3/120)` counter after the time, and a failure's output under `──── STDOUT:`
//! rather than `--- STDOUT: … ---`; both spellings are read.
//!
//! A retried test prints `TRY 1 FAIL`, `TRY 2 FAIL`, … and the line alone cannot say which try is the last. So a
//! failed try is held back: a later try of the same test that passes drops it, and whatever is still held when the
//! run ends is that test's failure. Each test gets one verdict — a second line about it (`FLAKY`) is not counted
//! again. `SLOW` and `START` lines are progress, not verdicts. Anything else nextest prints is passed through as log, unread.

use std::collections::HashSet;

use serde::Serialize;

use crate::cargo_discover::TestTarget;
use crate::surefire::TestStatus;

/// A cap on one failure's captured output, as for libtest's.
const MAX_OUTPUT_CHARS: usize = 20_000;

/// Which package and target a nextest binary id names.
pub fn binary_target(id: &str) -> (String, TestTarget) {
    match id.split_once("::") {
        None => (id.to_string(), TestTarget::Lib),
        Some((package, rest)) => {
            let target = match rest.split_once('/') {
                Some(("bin", name)) => TestTarget::Bin { name: name.to_string() },
                Some(("bench", name)) => TestTarget::Bench { name: name.to_string() },
                Some(("example", name)) => TestTarget::Example { name: name.to_string() },
                Some(("test", name)) => TestTarget::Test { name: name.to_string() },
                _ => TestTarget::Test { name: rest.to_string() },
            };
            (package.to_string(), target)
        }
    }
}

/// What one line of nextest's output meant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NextestEvent {
    /// A test's final verdict.
    Case { binary: String, path: String, status: TestStatus },
    /// The captured output of a failed test (stdout and stderr, in the order printed).
    Failure { binary: String, path: String, output: String },
    /// `Summary …` — the run is over.
    Summary { passed: u32, failed: u32, skipped: u32 },
}

/// Reads nextest's output line by line. Stateful for the failure output, which is a block under a header.
#[derive(Debug, Default)]
pub struct NextestParser {
    /// The failure block being captured: its binary, its test, and the lines so far.
    capturing: Option<(String, String, String)>,
    /// Past the summary nothing is a verdict any more.
    done: bool,
    /// Tests already given their verdict, by (binary, path).
    decided: HashSet<(String, String)>,
    /// Tests whose last try so far failed, in the order they did: a failure unless a later try passes.
    held: Vec<(String, String)>,
}

impl NextestParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn line(&mut self, line: &str) -> Vec<NextestEvent> {
        let mut out = Vec::new();
        let trimmed = line.trim_end();
        if let Some((binary, path)) = output_header(trimmed) {
            // A second header for the same test (stdout, then stderr) continues the same block.
            if let Some((b, p, text)) = &mut self.capturing {
                if *b == binary && *p == path {
                    push_capped(text, trimmed);
                    return out;
                }
            }
            out.extend(self.flush());
            self.capturing = Some((binary, path, String::new()));
            return out;
        }
        if let Some(summary) = summary(trimmed) {
            out.extend(self.flush());
            out.extend(self.release_held());
            self.done = true;
            out.push(summary);
            return out;
        }
        if !self.done {
            if let Some((word, binary, path, retry)) = verdict_line(trimmed) {
                out.extend(self.flush());
                out.extend(self.verdict(word, binary, path, retry));
                return out;
            }
        }
        if let Some((_, _, text)) = &mut self.capturing {
            push_capped(text, trimmed);
        }
        out
    }

    /// The failure block still open. At the end of the stream call [`Self::finish`] instead, which also settles
    /// the tests whose last try failed.
    pub fn flush(&mut self) -> Option<NextestEvent> {
        let (binary, path, output) = self.capturing.take()?;
        Some(NextestEvent::Failure { binary, path, output: output.trim().to_string() })
    }

    /// Everything still pending when the stream ends: the open failure block, and the held failed tries.
    pub fn finish(&mut self) -> Vec<NextestEvent> {
        let mut out: Vec<NextestEvent> = self.flush().into_iter().collect();
        out.extend(self.release_held());
        out
    }

    fn verdict(&mut self, word: &str, binary: String, path: String, retry: bool) -> Option<NextestEvent> {
        let status = status_of(word)?;
        let key = (binary, path);
        if self.decided.contains(&key) {
            return None;
        }
        if retry && status == TestStatus::Failed {
            if !self.held.contains(&key) {
                self.held.push(key);
            }
            return None;
        }
        self.held.retain(|k| *k != key);
        self.decided.insert(key.clone());
        Some(NextestEvent::Case { binary: key.0, path: key.1, status })
    }

    /// The tests whose last try failed: failures now, since no later try passed.
    fn release_held(&mut self) -> Vec<NextestEvent> {
        std::mem::take(&mut self.held)
            .into_iter()
            .filter(|k| self.decided.insert(k.clone()))
            .map(|(binary, path)| NextestEvent::Case { binary, path, status: TestStatus::Failed })
            .collect()
    }
}

fn push_capped(text: &mut String, line: &str) {
    if text.chars().count() < MAX_OUTPUT_CHARS {
        text.push_str(line);
        text.push('\n');
    }
}

/// `──── STDOUT:   bin path` (newer) or `--- STDERR:   bin path ---` (older).
fn output_header(line: &str) -> Option<(String, String)> {
    let body = line.trim_start().trim_start_matches(['─', '-']).trim_start();
    let rest = ["STDOUT:", "STDERR:", "OUTPUT:"].iter().find_map(|h| body.strip_prefix(h))?;
    let rest = rest.trim().trim_end_matches(['─', '-']).trim();
    let mut parts = rest.split_whitespace();
    let binary = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    Some((binary, path))
}

/// `PASS [   0.004s] (3/12) binary path`, with an optional `TRY n` before the word (the last field says so).
/// `None` for any other line.
fn verdict_line(line: &str) -> Option<(&str, String, String, bool)> {
    let mut rest = line.trim_start();
    let mut retry = false;
    if let Some(after) = rest.strip_prefix("TRY ") {
        let (_, tail) = after.split_once(' ')?;
        rest = tail.trim_start();
        retry = true;
    }
    let (word, tail) = rest.split_once(' ')?;
    if word.is_empty() || !word.chars().all(|c| c.is_ascii_uppercase() || c == '-') {
        return None;
    }
    let tail = tail.trim_start().strip_prefix('[')?;
    let (_, tail) = tail.split_once(']')?;
    let mut tokens = tail.split_whitespace().peekable();
    if tokens.peek().is_some_and(|t| t.starts_with('(') && t.ends_with(')')) {
        tokens.next();
    }
    let binary = tokens.next()?.to_string();
    let path = tokens.next()?.to_string();
    Some((word, binary, path, retry))
}

/// The status a verdict word stands for. `None` for progress words.
fn status_of(word: &str) -> Option<TestStatus> {
    match word {
        "PASS" | "FLAKY" | "LEAK" => Some(TestStatus::Passed),
        "SKIP" => Some(TestStatus::Skipped),
        "SLOW" | "START" | "RETRY" | "SETUP" => None,
        // FAIL, SIGSEGV, SIGABRT, TIMEOUT, ABORT, LEAK-FAIL, …: the test did not pass.
        _ => Some(TestStatus::Failed),
    }
}

/// `Summary [   0.512s] 3 tests run: 1 passed, 1 failed, 1 skipped` (the parts after `run:` vary).
fn summary(line: &str) -> Option<NextestEvent> {
    let rest = line.trim_start().strip_prefix("Summary")?;
    let (_, counts) = rest.split_once("run:")?;
    let mut summary = (0u32, 0u32, 0u32);
    for part in counts.split(',') {
        let mut words = part.split_whitespace();
        let (Some(n), Some(what)) = (words.next(), words.next()) else { continue };
        let Ok(n) = n.parse::<u32>() else { continue };
        match what.trim_end_matches(';') {
            "passed" => summary.0 += n,
            "failed" | "timed" => summary.1 += n,
            "skipped" => summary.2 += n,
            _ => {}
        }
    }
    Some(NextestEvent::Summary { passed: summary.0, failed: summary.1, skipped: summary.2 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(lines: &str) -> Vec<NextestEvent> {
        let mut p = NextestParser::new();
        let mut out: Vec<NextestEvent> = lines.lines().flat_map(|l| p.line(l)).collect();
        out.extend(p.finish());
        out
    }

    #[test]
    fn binary_ids_name_the_package_and_the_target() {
        assert_eq!(binary_target("demo"), ("demo".into(), TestTarget::Lib));
        assert_eq!(binary_target("demo::bin/tool"), ("demo".into(), TestTarget::Bin { name: "tool".into() }));
        assert_eq!(binary_target("demo::api"), ("demo".into(), TestTarget::Test { name: "api".into() }));
        assert_eq!(binary_target("my-crate::bench/speed"), ("my-crate".into(), TestTarget::Bench { name: "speed".into() }));
    }

    #[test]
    fn verdicts_failures_and_the_summary_are_read() {
        let events = run("    Starting 3 tests across 2 binaries
        PASS [   0.004s] demo cli::tests::parses
        SKIP [   0.000s] demo::api slow_one
        FAIL [   0.010s] demo::bin/tool tests::breaks
──── STDOUT:             demo::bin/tool tests::breaks

running 1 test
──── STDERR:             demo::bin/tool tests::breaks
thread 'tests::breaks' panicked at src/main.rs:9:5:
boom
     Summary [   0.512s] 3 tests run: 1 passed, 1 failed, 1 skipped");
        assert_eq!(events[0], NextestEvent::Case { binary: "demo".into(), path: "cli::tests::parses".into(), status: TestStatus::Passed });
        assert_eq!(events[1], NextestEvent::Case { binary: "demo::api".into(), path: "slow_one".into(), status: TestStatus::Skipped });
        assert_eq!(events[2], NextestEvent::Case { binary: "demo::bin/tool".into(), path: "tests::breaks".into(), status: TestStatus::Failed });
        let NextestEvent::Failure { path, output, .. } = &events[3] else { panic!("{events:?}") };
        assert_eq!(path, "tests::breaks");
        assert!(output.contains("panicked at src/main.rs:9:5") && output.contains("boom"));
        assert_eq!(events[4], NextestEvent::Summary { passed: 1, failed: 1, skipped: 1 });
        assert_eq!(events.len(), 5);
    }

    #[test]
    fn the_older_spellings_and_the_counter_are_read_too() {
        let events = run("        PASS [   0.004s] (1/2) demo a::b
        FAIL [   0.010s] (2/2) demo a::c
--- STDERR:              demo a::c ---
panicked
");
        assert_eq!(events[0], NextestEvent::Case { binary: "demo".into(), path: "a::b".into(), status: TestStatus::Passed });
        assert!(matches!(&events[2], NextestEvent::Failure { output, .. } if output == "panicked"));
    }

    #[test]
    fn only_the_last_try_counts_and_progress_is_not_a_verdict() {
        let events = run("  TRY 1 FAIL [   0.010s] demo flaky
        SLOW [> 60.000s] demo long_one
  TRY 2 PASS [   0.011s] demo flaky
       FLAKY [   0.011s] demo flaky");
        let cases: Vec<_> = events.iter().filter(|e| matches!(e, NextestEvent::Case { .. })).collect();
        assert_eq!(cases, [&NextestEvent::Case { binary: "demo".into(), path: "flaky".into(), status: TestStatus::Passed }], "one verdict, the passing try");
    }

    #[test]
    fn a_test_whose_every_try_failed_is_one_failure_when_the_run_ends() {
        let events = run("  TRY 1 FAIL [   0.010s] demo stubborn
  TRY 2 FAIL [   0.010s] demo stubborn
     Summary [   0.100s] 1 test run: 0 passed, 1 failed, 0 skipped");
        assert_eq!(events[0], NextestEvent::Case { binary: "demo".into(), path: "stubborn".into(), status: TestStatus::Failed });
        assert!(matches!(events[1], NextestEvent::Summary { failed: 1, .. }));
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn signals_and_timeouts_are_failures() {
        let events = run("     SIGSEGV [   0.010s] demo crashes\n     TIMEOUT [  60.000s] demo hangs");
        assert!(events.iter().all(|e| matches!(e, NextestEvent::Case { status: TestStatus::Failed, .. })));
    }
}
