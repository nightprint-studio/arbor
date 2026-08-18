//! Reading a `cargo test` run back — the two streams, and why they are read separately.
//!
//! Cargo produces no report file. Everything a panel can know comes from the output, and that
//! output arrives on **two pipes that mean different things**:
//!
//! - **stderr** is cargo's: `Compiling …`, then one `Running <desc> (<exe>)` line per test binary
//!   it is about to start. This is the only place the *target* of a test is named.
//! - **stdout** is the test binary's (libtest's): `running N tests`, one line per case as it
//!   finishes, the captured output of each failure, and a `test result:` summary.
//!
//! Neither stream can be read alone. stdout says `test util::tests::works ... ok` — and in a
//! twenty-crate workspace, `util::tests::works` exists in four of them. stderr says which binary
//! is running but nothing about what happened inside it.
//!
//! ## How the halves are matched
//!
//! **By index, not by time.** Cargo runs test binaries one at a time and prints its `Running`
//! line before each; libtest prints `running N tests` as each binary starts. So the k-th
//! `running N tests` on stdout belongs to the k-th `Running` on stderr, whatever order the two
//! pipes happen to be scheduled in. Pairing on arrival order across pipes would be a race; pairing
//! on the count is exact.
//!
//! The pairing itself is the caller's (it owns both reader threads) — this module provides the two
//! halves as pure line readers: [`running_target`] for stderr and [`LibtestParser`] for stdout.
//!
//! ## Per-case timings
//!
//! libtest does not report them without an unstable flag, so a case carries none. The block's
//! `finished in` is real and is on [`LibtestResult`]; inventing a per-case number by dividing it
//! would be a fabrication the panel would then sort by.

use serde::Serialize;

use crate::surefire::TestStatus;

/// A test binary cargo is about to run, from one `Running …` line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunningTarget {
    /// Cargo's own words for it: `unittests src/lib.rs`, `tests/api.rs`, `foo` for doc-tests.
    pub desc: String,
    /// The target's root source file, when the line names one. This is what identifies *which*
    /// target it is — `unittests src/lib.rs` is the lib, `tests/api.rs` is integration test `api`.
    pub src: Option<String>,
    /// The built executable. Absent for doc-tests, which rustdoc runs itself.
    pub exe: Option<String>,
    /// True for the `Doc-tests <crate>` announcement, whose `desc` is a crate name rather than a
    /// path — the one case where the target kind is known from the line alone.
    pub doc: bool,
}

/// The target announced by a cargo status line, if it is one.
///
/// Two spellings, plus doc-tests:
/// - `     Running unittests src/lib.rs (target/debug/deps/demo-1a2b3c)`
/// - `     Running tests/api.rs (target/debug/deps/api-4d5e6f)`
/// - `   Doc-tests demo`
///
/// Older cargo wrote `Running target/debug/deps/demo-1a2b3c` with no description at all; that
/// form yields the executable and no `src`, which still pairs and still names a row.
pub fn running_target(line: &str) -> Option<RunningTarget> {
    let body = line.trim();
    if let Some(rest) = body.strip_prefix("Doc-tests ") {
        let name = rest.trim();
        return (!name.is_empty()).then(|| RunningTarget {
            desc: name.to_string(),
            src: None,
            exe: None,
            doc: true,
        });
    }
    let rest = body.strip_prefix("Running ")?.trim();
    if rest.is_empty() {
        return None;
    }
    // The executable is the parenthesised tail, when there is one.
    let (desc, exe) = match (rest.rfind(" ("), rest.ends_with(')')) {
        (Some(i), true) => (rest[..i].trim(), Some(rest[i + 2..rest.len() - 1].to_string())),
        _ => (rest, None),
    };
    // `unittests <path>` is cargo's phrasing for a lib/bin target; the bare form is an
    // integration test named by its own file.
    let src = desc
        .strip_prefix("unittests ")
        .map(str::to_string)
        .or_else(|| desc.ends_with(".rs").then(|| desc.to_string()));
    Some(RunningTarget { desc: desc.to_string(), src, exe, doc: false })
}

/// The crate cargo has started compiling, from a `Compiling foo v0.1.0 (…)` line.
///
/// Worth reading for one reason: on a cold workspace the first useful second of a test run is
/// spent compiling, and a panel that shows nothing until the first test lands looks hung.
pub fn compiling_crate(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix("Compiling ")?;
    let name = rest.split_whitespace().next()?;
    (!name.is_empty()).then_some(name)
}

/// libtest's summary of one binary's run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct LibtestResult {
    pub passed: u32,
    pub failed: u32,
    pub ignored: u32,
    pub measured: u32,
    pub filtered_out: u32,
    pub time_ms: u64,
    /// libtest's own verdict (`test result: ok.` / `FAILED.`).
    pub ok: bool,
}

/// What one line of libtest output meant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum LibtestEvent {
    /// `running 12 tests` — a new binary's block has started.
    Start { count: u32 },
    /// One case finished.
    Case {
        /// The libtest path: `util::tests::works`, or `src/lib.rs - add (line 5)` for a doc test.
        path: String,
        status: TestStatus,
        /// A skip's reason (`#[ignore = "needs the network"]`) or a bench's timing — whatever the
        /// outcome carried beyond the verdict itself.
        note: Option<String>,
    },
    /// The captured output of a failed case — the panic message and where it came from.
    Failure { path: String, output: String },
    /// `test result: …` — the block is over.
    Result(LibtestResult),
}

/// A cap on one failure's captured output. Panic messages are read, so they stay generous — but a
/// test that printed a megabyte before failing must not send all of it across the IPC seam and
/// into the renderer.
const MAX_OUTPUT_CHARS: usize = 20_000;

/// Reads libtest's stdout line by line.
///
/// Stateful for one reason: a failure's output is a **block**, opened by a
/// `---- <path> stdout ----` header and closed by the next header or by the end of the section.
/// Everything else is a single line.
#[derive(Debug, Default)]
pub struct LibtestParser {
    /// The failure block being captured: its path and the lines so far.
    capturing: Option<(String, String)>,
}

impl LibtestParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Interpret one line. Returns what it meant — usually nothing, since most of a run's output
    /// is the code under test talking.
    pub fn line(&mut self, line: &str) -> Vec<LibtestEvent> {
        let mut out = Vec::new();
        let trimmed = line.trim_end();

        // ── inside a failure block ────────────────────────────────────────────────
        if let Some(path) = failure_header(trimmed) {
            // A new header closes the previous block.
            if let Some(done) = self.flush() {
                out.push(done);
            }
            self.capturing = Some((path, String::new()));
            return out;
        }
        if self.capturing.is_some() {
            // The section ends at the `failures:` list or at the summary; either way the block
            // is complete and must be emitted before the line is read as anything else.
            let ends = trimmed.trim_start() == "failures:" || trimmed.starts_with("test result:");
            if ends {
                if let Some(done) = self.flush() {
                    out.push(done);
                }
                // Fall through: `test result:` is still a summary line worth reading.
            } else {
                if let Some((_, buf)) = self.capturing.as_mut() {
                    if buf.len() < MAX_OUTPUT_CHARS {
                        buf.push_str(trimmed);
                        buf.push('\n');
                    }
                }
                return out;
            }
        }

        // ── the ordinary lines ────────────────────────────────────────────────────
        if let Some(count) = block_start(trimmed) {
            out.push(LibtestEvent::Start { count });
            return out;
        }
        if let Some(result) = block_result(trimmed) {
            out.push(LibtestEvent::Result(result));
            return out;
        }
        if let Some(ev) = case_line(trimmed) {
            out.push(ev);
        }
        out
    }

    /// Close the failure block being captured, if any.
    ///
    /// Called at the end of the stream too: a run killed mid-failure still has a message worth
    /// showing, and dropping it would leave a red row with no reason on it.
    pub fn flush(&mut self) -> Option<LibtestEvent> {
        let (path, output) = self.capturing.take()?;
        Some(LibtestEvent::Failure { path, output: output.trim_end().to_string() })
    }
}

/// `running 12 tests` — and `running 0 tests`, which is what a filtered-out binary says and must
/// still open a block, or the pairing with cargo's `Running` lines would drift by one.
fn block_start(line: &str) -> Option<u32> {
    let rest = line.trim().strip_prefix("running ")?;
    let n = rest.split_whitespace().next()?;
    let count = n.parse::<u32>().ok()?;
    (rest.contains("test")).then_some(count)
}

/// `---- util::tests::fails stdout ----` — the header of a failure's captured output.
fn failure_header(line: &str) -> Option<String> {
    let body = line.trim().strip_prefix("---- ")?.strip_suffix(" ----")?;
    // The trailing word is which stream was captured; the rest is the path, which for a doc test
    // contains spaces.
    let (path, stream) = body.rsplit_once(' ')?;
    matches!(stream, "stdout" | "stderr").then(|| path.to_string())
}

/// `test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s`
fn block_result(line: &str) -> Option<LibtestResult> {
    let rest = line.trim().strip_prefix("test result:")?.trim();
    let mut r = LibtestResult { ok: rest.starts_with("ok"), ..Default::default() };
    for part in rest.split(';') {
        let part = part.trim();
        if let Some(t) = part.strip_prefix("finished in ") {
            r.time_ms = parse_seconds_ms(t);
            continue;
        }
        // The count is not always the first token: the leading segment carries the verdict too
        // (`ok. 2 passed`), and reading its first word as the number lost every count on a
        // passing run.
        let toks: Vec<&str> = part.split_whitespace().collect();
        let Some(i) = toks.iter().position(|t| t.parse::<u32>().is_ok()) else { continue };
        let Ok(n) = toks[i].parse::<u32>() else { continue };
        match toks[i + 1..].join(" ").as_str() {
            "passed" => r.passed = n,
            "failed" => r.failed = n,
            "ignored" => r.ignored = n,
            "measured" => r.measured = n,
            "filtered out" => r.filtered_out = n,
            _ => {}
        }
    }
    Some(r)
}

/// `1.23s` → milliseconds. Tolerates a comma decimal separator, for the same reason the Surefire
/// reader does: a locale-formatted number that silently parses as zero makes every duration 0ms.
fn parse_seconds_ms(text: &str) -> u64 {
    let cleaned: String = text
        .trim()
        .trim_end_matches('s')
        .chars()
        .map(|c| if c == ',' { '.' } else { c })
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    cleaned.parse::<f64>().map(|s| (s * 1000.0).round() as u64).unwrap_or(0)
}

/// `test util::tests::works ... ok`
///
/// The path is everything between `test ` and ` ... `, taken from the **right**, because a doc
/// test's name contains spaces (`src/lib.rs - add (line 5)`).
fn case_line(line: &str) -> Option<LibtestEvent> {
    let rest = line.trim().strip_prefix("test ")?;
    let (path, outcome) = rest.rsplit_once(" ... ")?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let outcome = outcome.trim();
    let (status, note) = match outcome {
        "ok" => (TestStatus::Passed, None),
        "FAILED" => (TestStatus::Failed, None),
        _ if outcome.starts_with("ignored") => (
            TestStatus::Skipped,
            // `ignored, needs the network` — the reason is the half worth keeping.
            outcome.strip_prefix("ignored").map(|r| r.trim_start_matches([',', ' ']).to_string()).filter(|r| !r.is_empty()),
        ),
        // `bench: 1,234 ns/iter (+/- 56)` — a benchmark that ran is a pass, and its number is
        // the only thing it has to say.
        _ if outcome.starts_with("bench:") => (
            TestStatus::Passed,
            Some(outcome.trim_start_matches("bench:").trim().to_string()),
        ),
        // A verdict we do not know is not a pass. Reporting it as one is how a runner comes to
        // say "all green" about something it did not understand.
        other => (TestStatus::Error, Some(other.to_string())),
    };
    Some(LibtestEvent::Case { path: path.to_string(), status, note })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── cargo's stderr ────────────────────────────────────────────────────────────

    #[test]
    fn reads_a_unit_test_binary() {
        let t = running_target("     Running unittests src/lib.rs (target/debug/deps/demo-1a2b3c)")
            .expect("a target");
        assert_eq!(t.src.as_deref(), Some("src/lib.rs"));
        assert_eq!(t.exe.as_deref(), Some("target/debug/deps/demo-1a2b3c"));
        assert!(!t.doc);
    }

    #[test]
    fn reads_an_integration_test_binary() {
        let t = running_target("     Running tests/api.rs (target/debug/deps/api-4d5e6f)")
            .expect("a target");
        assert_eq!(t.src.as_deref(), Some("tests/api.rs"));
    }

    /// An older cargo names only the executable. It still pairs, and still labels a row.
    #[test]
    fn tolerates_a_line_with_no_description() {
        let t = running_target("     Running target/debug/deps/demo-1a2b3c").expect("a target");
        assert_eq!(t.src, None);
        assert_eq!(t.desc, "target/debug/deps/demo-1a2b3c");
    }

    #[test]
    fn reads_the_doc_test_announcement() {
        let t = running_target("   Doc-tests demo").expect("a target");
        assert!(t.doc);
        assert_eq!(t.desc, "demo");
        assert_eq!(t.exe, None);
    }

    #[test]
    fn other_cargo_lines_are_not_targets() {
        assert!(running_target("   Compiling demo v0.1.0 (/p)").is_none());
        assert!(running_target("    Finished `test` profile [unoptimized] target(s) in 1.2s").is_none());
        assert!(running_target("Running").is_none());
    }

    #[test]
    fn reads_the_crate_being_compiled() {
        assert_eq!(compiling_crate("   Compiling demo v0.1.0 (/p)"), Some("demo"));
        assert_eq!(compiling_crate("[INFO] not cargo"), None);
    }

    // ── libtest's stdout ──────────────────────────────────────────────────────────

    fn parse(lines: &[&str]) -> Vec<LibtestEvent> {
        let mut p = LibtestParser::new();
        let mut out: Vec<LibtestEvent> = lines.iter().flat_map(|l| p.line(l)).collect();
        out.extend(p.flush());
        out
    }

    #[test]
    fn reads_a_block_of_passing_tests() {
        let events = parse(&[
            "",
            "running 2 tests",
            "test util::tests::works ... ok",
            "test other ... ok",
            "",
            "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s",
        ]);
        assert_eq!(events[0], LibtestEvent::Start { count: 2 });
        assert_eq!(
            events[1],
            LibtestEvent::Case {
                path: "util::tests::works".to_string(),
                status: TestStatus::Passed,
                note: None
            }
        );
        let LibtestEvent::Result(r) = events[3] else { panic!("a summary") };
        assert_eq!(r.passed, 2);
        assert!(r.ok);
        assert_eq!(r.time_ms, 10);
    }

    /// A filtered-out binary says `running 0 tests`, and that block MUST be reported: it is what
    /// keeps the k-th stdout block aligned with the k-th target cargo announced.
    #[test]
    fn an_empty_block_still_starts_one() {
        let events = parse(&["running 0 tests"]);
        assert_eq!(events, [LibtestEvent::Start { count: 0 }]);
    }

    #[test]
    fn an_ignored_test_keeps_its_reason() {
        let events = parse(&["test slow ... ignored, needs the network"]);
        assert_eq!(
            events[0],
            LibtestEvent::Case {
                path: "slow".to_string(),
                status: TestStatus::Skipped,
                note: Some("needs the network".to_string())
            }
        );
    }

    #[test]
    fn a_bare_ignored_test_has_no_reason() {
        let events = parse(&["test slow ... ignored"]);
        assert_eq!(
            events[0],
            LibtestEvent::Case {
                path: "slow".to_string(),
                status: TestStatus::Skipped,
                note: None
            }
        );
    }

    #[test]
    fn a_benchmark_carries_its_timing() {
        let events = parse(&["test speed ... bench:       1,234 ns/iter (+/- 56)"]);
        let LibtestEvent::Case { status, note, .. } = &events[0] else { panic!("a case") };
        assert_eq!(*status, TestStatus::Passed);
        assert_eq!(note.as_deref(), Some("1,234 ns/iter (+/- 56)"));
    }

    /// The panic message is the whole point of a failed row, and it arrives as a block that has
    /// to be closed by something.
    #[test]
    fn captures_a_failure_block() {
        let events = parse(&[
            "running 1 test",
            "test util::tests::fails ... FAILED",
            "",
            "failures:",
            "",
            "---- util::tests::fails stdout ----",
            "thread 'util::tests::fails' panicked at src/util.rs:42:9:",
            "assertion `left == right` failed",
            "  left: 1",
            " right: 2",
            "",
            "failures:",
            "    util::tests::fails",
            "",
            "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
        ]);
        let failure = events
            .iter()
            .find_map(|e| match e {
                LibtestEvent::Failure { path, output } => Some((path, output)),
                _ => None,
            })
            .expect("a failure block");
        assert_eq!(failure.0, "util::tests::fails");
        assert!(failure.1.contains("panicked at src/util.rs:42:9"));
        assert!(failure.1.contains("right: 2"));
        assert!(!failure.1.contains("failures:"), "the list section is not part of the message");
        // The summary after the block is still read.
        assert!(matches!(events.last(), Some(LibtestEvent::Result(r)) if r.failed == 1));
    }

    #[test]
    fn two_failure_blocks_do_not_bleed_into_each_other() {
        let events = parse(&[
            "---- a stdout ----",
            "first",
            "---- b stdout ----",
            "second",
            "test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
        ]);
        let blocks: Vec<(&str, &str)> = events
            .iter()
            .filter_map(|e| match e {
                LibtestEvent::Failure { path, output } => Some((path.as_str(), output.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(blocks, [("a", "first"), ("b", "second")]);
    }

    /// A doc test's name has spaces in it, so the path is taken from the right-hand separator.
    #[test]
    fn reads_a_doc_test_case() {
        let events = parse(&["test src/lib.rs - add (line 5) ... ok"]);
        assert_eq!(
            events[0],
            LibtestEvent::Case {
                path: "src/lib.rs - add (line 5)".to_string(),
                status: TestStatus::Passed,
                note: None
            }
        );
    }

    /// A verdict we do not recognise must not be silently green.
    #[test]
    fn an_unknown_outcome_is_an_error_not_a_pass() {
        let events = parse(&["test odd ... something new"]);
        let LibtestEvent::Case { status, note, .. } = &events[0] else { panic!("a case") };
        assert_eq!(*status, TestStatus::Error);
        assert_eq!(note.as_deref(), Some("something new"));
    }

    #[test]
    fn the_code_under_test_talking_is_not_a_case() {
        let events = parse(&[
            "testing the waters",
            "my test ... ok",
            "[INFO] test result: whatever",
        ]);
        // The third line DOES start with a summary once trimmed of its prefix — it must not, or
        // application logging would end a block.
        assert!(events.iter().all(|e| !matches!(e, LibtestEvent::Case { .. })));
    }

    #[test]
    fn a_run_killed_mid_failure_still_yields_the_message() {
        let mut p = LibtestParser::new();
        p.line("---- a stdout ----");
        p.line("panicked");
        let last = p.flush().expect("the partial block");
        assert_eq!(
            last,
            LibtestEvent::Failure { path: "a".to_string(), output: "panicked".to_string() }
        );
    }
}
