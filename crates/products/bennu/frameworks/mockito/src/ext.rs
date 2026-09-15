//! The extension — what a host registers.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use bennu_ext::prelude::{
    ExtIntention, ExtProblem, ExtStat, FileCtx, FrameworkExtension, ProjectScan,
};
use bennu_facts::prelude::scan_java;
use bennu_proto::prelude::{severity, CapabilitySet, Diagnostic};

use crate::eq_fix::wrap_in_eq;
use crate::file::JavaFile;
use crate::init::{add_extension, mocked_field_count, uninitialised_classes, Runner};
use crate::matchers::mixed_calls;
use crate::unfinished::{unfinished_statements, Unfinished};

pub const CODE_UNFINISHED_STUBBING: &str = "mockito.unfinished-stubbing";
pub const CODE_UNFINISHED_VERIFICATION: &str = "mockito.unfinished-verification";
pub const CODE_MIXED_MATCHERS: &str = "mockito.mixed-matchers";
pub const CODE_NOT_INITIALISED: &str = "mockito.mocks-not-initialised";

pub const INTENTION_WRAP_IN_EQ: &str = "mockito.wrap-in-eq";
pub const INTENTION_ADD_EXTENSION: &str = "mockito.add-extension";

/// The pre-filter. Every Mockito name a check resolves is imported from here, so a file without it
/// has nothing any check could certainly recognise.
const MARKER: &str = "org.mockito";

#[derive(Default)]
pub struct MockitoExtension {
    scanned: AtomicBool,
    /// `@Mock` and `@Spy` fields across the project, for the overview.
    mocked_fields: AtomicUsize,
}

impl MockitoExtension {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FrameworkExtension for MockitoExtension {
    fn id(&self) -> &'static str {
        "mockito"
    }

    fn display_name(&self) -> &'static str {
        "Mockito"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.mockito
    }

    /// Only a count. Every check is about one test file on its own, so there is no project model to
    /// build — which is also why the diagnostics do not wait for this.
    fn reindex(&self, scan: &ProjectScan<'_>) {
        let count = scan
            .java
            .iter()
            .filter(|f| f.text.contains(MARKER))
            .filter_map(|f| scan_java(&f.path.to_string_lossy(), &f.text))
            .map(|facts| mocked_field_count(&facts))
            .sum();
        self.mocked_fields.store(count, Ordering::Release);
        self.scanned.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Acquire)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        let Some(file) = parse(ctx) else { return Vec::new() };
        let mut out = Vec::new();
        for statement in unfinished_statements(&file) {
            let (code, message) = unfinished_message(&statement.kind);
            out.push(diagnostic(code, severity::ERROR, message, statement.start, statement.end));
        }
        for call in mixed_calls(&file) {
            for &(start, end) in &call.raw {
                out.push(diagnostic(CODE_MIXED_MATCHERS, severity::ERROR, MIXED_MESSAGE.to_string(), start, end));
            }
        }
        for class in uninitialised_classes(&file) {
            let initialiser = match class.runner {
                Runner::Jupiter => "@ExtendWith(MockitoExtension.class)",
                Runner::JUnit4 => "@RunWith(MockitoJUnitRunner.class)",
            };
            for field in &class.fields {
                let message = format!(
                    "`{}` is never initialised — nothing in {} runs MockitoExtension, \
                     MockitoJUnitRunner or MockitoAnnotations.openMocks, so the @{} does nothing: \
                     the field stays null and the first stubbing throws a NullPointerException. \
                     Add {initialiser}",
                    field.field, class.class, field.annotation
                );
                out.push(diagnostic(CODE_NOT_INITIALISED, severity::WARNING, message, field.start, field.end));
            }
        }
        out.sort_by_key(|d| d.start);
        out
    }

    /// The two mechanical fixes. Both recompute from the buffer and offer nothing when the analysis
    /// no longer finds the problem the editor drew.
    fn intentions(&self, ctx: &FileCtx<'_>, _offset: usize, problems: &[ExtProblem]) -> Vec<ExtIntention> {
        let drawn = |code: &str, start: usize, end: usize| {
            problems.iter().any(|p| p.code == code && p.start == start && p.end == end)
        };
        let ours = problems.iter().any(|p| p.code == CODE_MIXED_MATCHERS || p.code == CODE_NOT_INITIALISED);
        if !ours {
            return Vec::new();
        }
        let Some(file) = parse(ctx) else { return Vec::new() };
        let mut out = Vec::new();
        for call in mixed_calls(&file) {
            if !call.raw.iter().any(|&(start, end)| drawn(CODE_MIXED_MATCHERS, start, end)) {
                continue;
            }
            if let Some(edits) = wrap_in_eq(&file, &call) {
                out.push(ExtIntention {
                    id: INTENTION_WRAP_IN_EQ.to_string(),
                    label: "Wrap plain arguments in eq(…)".to_string(),
                    edits,
                });
            }
        }
        for class in uninitialised_classes(&file) {
            if !class.fields.iter().any(|f| drawn(CODE_NOT_INITIALISED, f.start, f.end)) {
                continue;
            }
            let label = match class.runner {
                Runner::Jupiter => "Add @ExtendWith(MockitoExtension.class)",
                Runner::JUnit4 => "Add @RunWith(MockitoJUnitRunner.class)",
            };
            out.push(ExtIntention {
                id: INTENTION_ADD_EXTENSION.to_string(),
                label: label.to_string(),
                edits: add_extension(&file, &class),
            });
        }
        out
    }

    fn stats(&self) -> Vec<ExtStat> {
        vec![ExtStat {
            label: "Mocked fields".to_string(),
            value: self.mocked_fields.load(Ordering::Acquire),
            catalog: None,
        }]
    }
}

const MIXED_MESSAGE: &str = "a plain value beside argument matchers — when one argument of a \
    stubbed or verified call is a matcher, all of them must be, and Mockito throws \
    InvalidUseOfMatchersException. Wrap it in eq(…)";

fn parse<'s>(ctx: &FileCtx<'s>) -> Option<JavaFile<'s>> {
    if ctx.extension() != "java" || !ctx.source.contains(MARKER) {
        return None;
    }
    JavaFile::parse(ctx.source)
}

/// Every message says where the exception will really be thrown, because that is the part that costs
/// the afternoon: the stack trace points at the NEXT Mockito call, not at this line.
fn unfinished_message(kind: &Unfinished) -> (&'static str, String) {
    match kind {
        Unfinished::Answerless { call } => {
            let finishers = if call == "given" {
                "willReturn(…), willThrow(…) or willAnswer(…)"
            } else {
                "thenReturn(…), thenThrow(…) or thenAnswer(…)"
            };
            (
                CODE_UNFINISHED_STUBBING,
                format!(
                    "unfinished stubbing: `{call}(…)` never says what the call answers. Mockito \
                     throws UnfinishedStubbingException — not here, but at the next Mockito call, \
                     which may be in another test. Add {finishers}"
                ),
            )
        }
        Unfinished::Targetless => (
            CODE_UNFINISHED_STUBBING,
            "unfinished stubbing: `when(mock)` never names the method being stubbed. Mockito throws \
             UnfinishedStubbingException — not here, but at the next Mockito call. Call the method \
             on it: `when(mock).find(…)`"
                .to_string(),
        ),
        Unfinished::Unverified { call } => (
            CODE_UNFINISHED_VERIFICATION,
            format!(
                "unfinished verification: `{call}` never names the method to check, so this line \
                 verifies nothing. Mockito throws UnfinishedVerificationException at the next Mockito \
                 call. Call the method on it: `verify(mock).save(…)`"
            ),
        ),
    }
}

fn diagnostic(code: &str, severity: &str, message: String, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        message,
        severity: severity.to_string(),
        code: code.to_string(),
        start,
        end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edits::apply;
    use std::path::Path;

    const SRC: &str = "import static org.mockito.Mockito.*;\nimport org.junit.jupiter.api.Test;\nimport org.mockito.Mock;\n\nclass T {\n    @Mock Repo repo;\n\n    @Test void t() {\n        when(repo.find(1));\n        verify(repo);\n        verify(repo).save(any(), 5);\n    }\n}\n";

    fn ctx(source: &str) -> FileCtx<'_> {
        FileCtx { path: Path::new("/p/src/test/java/T.java"), source }
    }

    #[test]
    fn every_check_reports_through_the_seam_with_its_code_and_severity() {
        let found = MockitoExtension::new().diagnostics(&ctx(SRC));
        let codes: Vec<(&str, &str)> = found.iter().map(|d| (d.code.as_str(), d.severity.as_str())).collect();
        assert_eq!(
            codes,
            [
                (CODE_NOT_INITIALISED, "warning"),
                (CODE_UNFINISHED_STUBBING, "error"),
                (CODE_UNFINISHED_VERIFICATION, "error"),
                (CODE_MIXED_MATCHERS, "error"),
            ]
        );
    }

    #[test]
    fn each_fix_is_offered_for_its_own_problem_under_the_caret() {
        let ext = MockitoExtension::new();
        let diagnostics = ext.diagnostics(&ctx(SRC));
        let problem = |code: &str| {
            let d = diagnostics.iter().find(|d| d.code == code).unwrap();
            ExtProblem { code: d.code.clone(), start: d.start, end: d.end }
        };

        let wrap = ext.intentions(&ctx(SRC), 0, &[problem(CODE_MIXED_MATCHERS)]);
        assert_eq!(wrap.len(), 1);
        assert_eq!(wrap[0].id, INTENTION_WRAP_IN_EQ);
        assert!(apply(SRC, &wrap[0].edits).contains("save(any(), eq(5));"));

        let extension = ext.intentions(&ctx(SRC), 0, &[problem(CODE_NOT_INITIALISED)]);
        assert_eq!(extension.len(), 1);
        assert_eq!(extension[0].id, INTENTION_ADD_EXTENSION);
        assert!(apply(SRC, &extension[0].edits).contains("@ExtendWith(MockitoExtension.class)\nclass T {"));
    }

    /// The squiggle was drawn a keystroke ago; if the buffer no longer has the problem, no fix.
    #[test]
    fn a_stale_problem_gets_no_fix() {
        let fixed = SRC.replace("save(any(), 5)", "save(any(), eq(5))");
        let stale = ExtProblem { code: CODE_MIXED_MATCHERS.to_string(), start: 10, end: 11 };
        assert!(MockitoExtension::new().intentions(&ctx(&fixed), 0, &[stale]).is_empty());
    }

    #[test]
    fn a_file_that_does_not_mention_mockito_is_not_parsed() {
        let src = "class T { void t() { when(repo.find(1)); } }";
        assert!(MockitoExtension::new().diagnostics(&ctx(src)).is_empty());
    }
}
