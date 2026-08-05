//! Reading Surefire's XML reports — the only per-case truth a Maven test run produces.
//!
//! The console tells you `Tests run: 12, Failures: 1` and prints one stack trace. It does
//! not tell you *which* of the twelve failed, how long each took, or which were skipped and
//! why. All of that is in `target/surefire-reports/TEST-<class>.xml`, one file per test
//! class, written **as each class finishes** — which is what makes a live tree possible:
//! the backend watches the directory and a class lands in the panel the moment it is done,
//! rather than everything appearing at the end.
//!
//! Two things here are less obvious than they look:
//!
//! - **`time` is locale-formatted.** Surefire 2.x wrote durations with the default JVM
//!   locale, so on an Italian or German machine the attribute reads `0,123` and a plain
//!   `parse::<f64>()` returns nothing — every test silently reported as taking 0ms. Both
//!   separators are accepted.
//! - **A rerun failure is a pass.** With `rerunFailingTestsCount` set, a case that failed
//!   and then succeeded carries a `<flakyFailure>` and no `<failure>`. It passed; calling it
//!   a failure would make a green build look red. It is flagged as flaky instead, because
//!   silently passing it hides the thing worth knowing.

use serde::Serialize;

/// How one case ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    /// An assertion did not hold — the test ran and disagreed with the code.
    Failed,
    /// The test threw before it could assert. Kept distinct from `Failed` because the two
    /// mean different things to whoever reads the panel: one is a wrong answer, the other is
    /// a broken run.
    Error,
    Skipped,
}

impl TestStatus {
    /// Whether this status should turn the run red.
    pub fn is_bad(self) -> bool {
        matches!(self, TestStatus::Failed | TestStatus::Error)
    }
}

/// One executed test case. For a `@ParameterizedTest` there is one of these per invocation,
/// so several may share a `name` root — the report's `name` carries the invocation suffix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestCaseResult {
    pub name: String,
    /// The declaring class, fully qualified as the report writes it.
    pub classname: String,
    pub status: TestStatus,
    pub time_ms: u64,
    /// The failure's message (`expected:<1> but was:<2>`), or a skip's reason.
    pub message: Option<String>,
    /// The exception type (`java.lang.AssertionError`).
    pub kind: Option<String>,
    /// The stack trace, as written.
    pub trace: Option<String>,
    /// It failed at least once and then passed on a rerun.
    pub flaky: bool,
}

/// One test class's report — the contents of a single `TEST-*.xml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestClassResult {
    /// Fully-qualified class name, from the suite's `name`.
    pub classname: String,
    pub total: u32,
    pub failures: u32,
    pub errors: u32,
    pub skipped: u32,
    pub time_ms: u64,
    pub cases: Vec<TestCaseResult>,
    /// Anything the class printed to stdout during the run, when Surefire captured it.
    pub system_out: Option<String>,
    pub system_err: Option<String>,
}

impl TestClassResult {
    /// Whether anything in this class failed or errored.
    pub fn is_bad(&self) -> bool {
        self.failures > 0 || self.errors > 0
    }
}

/// A failure message is a sentence; anything longer is a stack trace that escaped into the
/// wrong field, and the panel has a place for those.
const MAX_MESSAGE_CHARS: usize = 2_000;
/// Traces are read, so they stay long — but not unbounded: a deep recursion produces
/// megabytes, and every byte of it crosses the IPC seam and lands in the renderer.
const MAX_TRACE_CHARS: usize = 20_000;
/// Same reasoning for captured stdout, which on a chatty legacy test is the larger of the two.
const MAX_OUTPUT_CHARS: usize = 20_000;

/// Parse one `TEST-*.xml`. `None` when the document isn't parseable — which during a live
/// run is the *expected* answer for a file caught mid-write, and the reason the caller
/// simply tries again on the next tick rather than reporting an error.
pub fn parse_report(xml: &str) -> Option<TestClassResult> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let suite = doc.root_element();
    if suite.tag_name().name() != "testsuite" {
        return None;
    }

    let classname = suite.attribute("name").unwrap_or_default().to_string();
    let mut cases = Vec::new();
    let mut system_out = None;
    let mut system_err = None;

    for child in suite.children().filter(|c| c.is_element()) {
        match child.tag_name().name() {
            "testcase" => cases.push(parse_case(child, &classname)),
            "system-out" => system_out = clip_opt(&all_text(child), MAX_OUTPUT_CHARS),
            "system-err" => system_err = clip_opt(&all_text(child), MAX_OUTPUT_CHARS),
            _ => {}
        }
    }

    // The counters are recomputed from the cases rather than read from the suite's
    // attributes: the two disagree in the rerun case (the suite counts the first, failing
    // attempt; the cases record that it ultimately passed), and the cases are what the panel
    // displays. A total that doesn't match the rows under it is a bug report waiting to happen.
    let failures = cases.iter().filter(|c| c.status == TestStatus::Failed).count() as u32;
    let errors = cases.iter().filter(|c| c.status == TestStatus::Error).count() as u32;
    let skipped = cases.iter().filter(|c| c.status == TestStatus::Skipped).count() as u32;
    let time_ms = suite
        .attribute("time")
        .and_then(parse_seconds)
        .unwrap_or_else(|| cases.iter().map(|c| c.time_ms).sum());

    Some(TestClassResult {
        classname,
        total: cases.len() as u32,
        failures,
        errors,
        skipped,
        time_ms,
        cases,
        system_out,
        system_err,
    })
}

fn parse_case(node: roxmltree::Node<'_, '_>, suite_class: &str) -> TestCaseResult {
    let name = node.attribute("name").unwrap_or_default().to_string();
    let classname = node.attribute("classname").unwrap_or(suite_class).to_string();
    let time_ms = node.attribute("time").and_then(parse_seconds).unwrap_or(0);

    let mut status = TestStatus::Passed;
    let mut message = None;
    let mut kind = None;
    let mut trace = None;
    let mut flaky = false;

    for child in node.children().filter(|c| c.is_element()) {
        match child.tag_name().name() {
            "failure" => {
                status = TestStatus::Failed;
                (message, kind, trace) = detail_of(child);
            }
            "error" => {
                status = TestStatus::Error;
                (message, kind, trace) = detail_of(child);
            }
            "skipped" => {
                status = TestStatus::Skipped;
                message = clip_opt(child.attribute("message").unwrap_or_default(), MAX_MESSAGE_CHARS);
            }
            // A failure that a rerun turned green. The case passed — but say so.
            "flakyFailure" | "flakyError" | "rerunFailure" | "rerunError" => {
                flaky = true;
                if status == TestStatus::Passed && trace.is_none() {
                    (message, kind, trace) = detail_of(child);
                }
            }
            _ => {}
        }
    }

    TestCaseResult { name, classname, status, time_ms, message, kind, trace, flaky }
}

/// The message / type / trace of a `<failure>`-shaped element.
fn detail_of(node: roxmltree::Node<'_, '_>) -> (Option<String>, Option<String>, Option<String>) {
    (
        clip_opt(node.attribute("message").unwrap_or_default(), MAX_MESSAGE_CHARS),
        clip_opt(node.attribute("type").unwrap_or_default(), MAX_MESSAGE_CHARS),
        clip_opt(&all_text(node), MAX_TRACE_CHARS),
    )
}

/// Every text and CDATA node under `node`, concatenated. `Node::text()` returns only the
/// first text child, which for a trace split across CDATA sections silently loses the rest.
fn all_text(node: roxmltree::Node<'_, '_>) -> String {
    node.descendants().filter_map(|n| n.text()).collect()
}

/// Seconds as Surefire writes them → milliseconds. Accepts BOTH decimal separators: 2.x
/// formatted with the JVM's default locale, so on a non-English machine this attribute is
/// `0,123` and a strict parse reports every test as instantaneous.
fn parse_seconds(raw: &str) -> Option<u64> {
    let normalized = raw.trim().replace(',', ".");
    let secs: f64 = normalized.parse().ok()?;
    if !secs.is_finite() || secs < 0.0 {
        return None;
    }
    Some((secs * 1000.0).round() as u64)
}

/// Trim to `max` characters (never mid-character), returning `None` for empty input.
fn clip_opt(s: &str, max: usize) -> Option<String> {
    let trimmed = s.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= max {
        return Some(trimmed.to_string());
    }
    let cut: String = trimmed.chars().take(max).collect();
    Some(format!("{cut}…"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSING: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="com.acme.OrderTest" time="0.213" tests="2" errors="0" skipped="0" failures="0">
  <testcase name="computesTotal" classname="com.acme.OrderTest" time="0.121"/>
  <testcase name="appliesDiscount" classname="com.acme.OrderTest" time="0.092"/>
</testsuite>"#;

    #[test]
    fn reads_a_passing_class() {
        let r = parse_report(PASSING).expect("parses");
        assert_eq!(r.classname, "com.acme.OrderTest");
        assert_eq!(r.total, 2);
        assert_eq!(r.failures, 0);
        assert!(!r.is_bad());
        assert_eq!(r.time_ms, 213);
        assert_eq!(r.cases[0].name, "computesTotal");
        assert_eq!(r.cases[0].status, TestStatus::Passed);
        assert_eq!(r.cases[0].time_ms, 121);
    }

    #[test]
    fn reads_a_failure_with_its_trace() {
        let xml = r#"<testsuite name="com.acme.OrderTest" time="0.1" tests="1" failures="1" errors="0" skipped="0">
  <testcase name="computesTotal" classname="com.acme.OrderTest" time="0.1">
    <failure message="expected:&lt;10&gt; but was:&lt;12&gt;" type="java.lang.AssertionError">at com.acme.OrderTest.computesTotal(OrderTest.java:42)</failure>
  </testcase>
</testsuite>"#;
        let r = parse_report(xml).expect("parses");
        assert!(r.is_bad());
        assert_eq!(r.failures, 1);
        let c = &r.cases[0];
        assert_eq!(c.status, TestStatus::Failed);
        assert_eq!(c.message.as_deref(), Some("expected:<10> but was:<12>"));
        assert_eq!(c.kind.as_deref(), Some("java.lang.AssertionError"));
        assert!(c.trace.as_deref().unwrap().contains("OrderTest.java:42"));
    }

    /// An exception is not an assertion — the panel says so, and the counters keep them apart.
    #[test]
    fn an_error_is_not_a_failure() {
        let xml = r#"<testsuite name="T" tests="1" time="0">
  <testcase name="boom" classname="T" time="0">
    <error message="/ by zero" type="java.lang.ArithmeticException">trace</error>
  </testcase>
</testsuite>"#;
        let r = parse_report(xml).expect("parses");
        assert_eq!(r.errors, 1);
        assert_eq!(r.failures, 0);
        assert_eq!(r.cases[0].status, TestStatus::Error);
    }

    #[test]
    fn reads_a_skip_with_its_reason() {
        let xml = r#"<testsuite name="T" tests="1" time="0">
  <testcase name="later" classname="T" time="0"><skipped message="not on CI"/></testcase>
</testsuite>"#;
        let r = parse_report(xml).expect("parses");
        assert_eq!(r.skipped, 1);
        assert_eq!(r.cases[0].status, TestStatus::Skipped);
        assert_eq!(r.cases[0].message.as_deref(), Some("not on CI"));
    }

    /// The bug this guards: on an Italian/German JVM Surefire 2.x writes `0,121`, and a
    /// strict parse reports every test as taking no time at all.
    #[test]
    fn accepts_a_comma_decimal_separator() {
        let xml = r#"<testsuite name="T" time="1,5" tests="1">
  <testcase name="a" classname="T" time="0,121"/>
</testsuite>"#;
        let r = parse_report(xml).expect("parses");
        assert_eq!(r.time_ms, 1500);
        assert_eq!(r.cases[0].time_ms, 121);
    }

    /// A case that failed and then passed on a rerun HAS passed. Counting it red would make
    /// a green build look broken.
    #[test]
    fn a_rerun_that_succeeded_is_a_pass_but_is_flagged_flaky() {
        let xml = r#"<testsuite name="T" tests="1" failures="1" time="0.2">
  <testcase name="racy" classname="T" time="0.2">
    <flakyFailure message="timing" type="java.lang.AssertionError">trace</flakyFailure>
  </testcase>
</testsuite>"#;
        let r = parse_report(xml).expect("parses");
        assert_eq!(r.cases[0].status, TestStatus::Passed);
        assert!(r.cases[0].flaky);
        assert_eq!(r.failures, 0, "counters follow the cases, not the suite attribute");
        assert!(!r.is_bad());
    }

    #[test]
    fn a_trace_split_across_cdata_is_kept_whole() {
        let xml = "<testsuite name=\"T\" tests=\"1\" time=\"0\">\
  <testcase name=\"a\" classname=\"T\" time=\"0\">\
    <failure message=\"m\" type=\"E\">head<![CDATA[ and tail]]></failure>\
  </testcase>\
</testsuite>";
        let r = parse_report(xml).expect("parses");
        assert_eq!(r.cases[0].trace.as_deref(), Some("head and tail"));
    }

    #[test]
    fn captures_system_out() {
        let xml = r#"<testsuite name="T" tests="1" time="0">
  <testcase name="a" classname="T" time="0"/>
  <system-out><![CDATA[hello from the test]]></system-out>
</testsuite>"#;
        let r = parse_report(xml).expect("parses");
        assert_eq!(r.system_out.as_deref(), Some("hello from the test"));
    }

    /// A file caught mid-write must not raise — the watcher simply reads it again next tick.
    #[test]
    fn a_truncated_file_is_none_not_a_panic() {
        assert!(parse_report("<testsuite name=\"T\"><testcase nam").is_none());
        assert!(parse_report("").is_none());
    }

    /// Some other XML that happens to be in the reports directory is not a report.
    #[test]
    fn a_foreign_root_element_is_rejected() {
        assert!(parse_report("<project><name>x</name></project>").is_none());
    }

    #[test]
    fn a_giant_trace_is_clipped() {
        let huge = "x".repeat(MAX_TRACE_CHARS * 2);
        let xml = format!(
            "<testsuite name=\"T\" tests=\"1\" time=\"0\"><testcase name=\"a\" classname=\"T\" time=\"0\"><failure message=\"m\" type=\"E\">{huge}</failure></testcase></testsuite>"
        );
        let r = parse_report(&xml).expect("parses");
        let trace = r.cases[0].trace.as_deref().unwrap();
        assert!(trace.chars().count() <= MAX_TRACE_CHARS + 1, "clipped, plus the ellipsis");
    }
}
