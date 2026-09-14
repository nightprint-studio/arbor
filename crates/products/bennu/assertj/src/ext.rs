//! The extension — what a host registers.
//!
//! Everything here is a per-buffer question: whether an assertion can fail is written in the
//! statement, and which `assertThat` a name is comes from the file's own imports. So there is no
//! project model to build, and `reindex` only records that the host has been through once.

use std::sync::atomic::{AtomicBool, Ordering};

use bennu_ext::prelude::{ExtIntention, ExtProblem, FileCtx, FrameworkExtension, ProjectScan};
use bennu_facts::prelude::mentions_any;
use bennu_proto::prelude::{CapabilitySet, Diagnostic};

use crate::unit::Unit;
use crate::{dedicated, junit, nothing, soft};

pub const CODE_ASSERTS_NOTHING: &str = "assertj.asserts-nothing";
pub const CODE_SOFT_NEVER_ASSERTED: &str = "assertj.soft-assertions-never-asserted";
pub const CODE_DEDICATED: &str = "assertj.use-dedicated-assertion";

pub const INTENTION_ADD_ASSERT_ALL: &str = "assertj.add-assert-all";
pub const INTENTION_DEDICATED: &str = "assertj.use-dedicated-assertion";
pub const INTENTION_FROM_JUNIT: &str = "assertj.from-junit";
pub const INTENTION_FROM_JUNIT_FILE: &str = "assertj.from-junit-file";

/// A file that says neither is not parsed. `WithAssertions` is here because a class implementing it
/// through a wildcard import can use AssertJ without the word `org.assertj` anywhere but that line —
/// and a marker list is meant to be over-inclusive: a false hit costs one parse.
const ASSERTJ_MARKERS: &[&str] = &["org.assertj", "WithAssertions"];
const JUNIT_MARKERS: &[&str] = &["org.junit"];

#[derive(Default)]
pub struct AssertJExtension {
    scanned: AtomicBool,
}

impl AssertJExtension {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FrameworkExtension for AssertJExtension {
    fn id(&self) -> &'static str {
        "assertj"
    }

    fn display_name(&self) -> &'static str {
        "AssertJ"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.assertj
    }

    fn reindex(&self, _scan: &ProjectScan<'_>) {
        self.scanned.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Acquire)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        if ctx.extension() != "java" || !mentions_any(ctx.source, ASSERTJ_MARKERS) {
            return Vec::new();
        }
        let path = ctx.path_str();
        let Some(unit) = Unit::parse(&path, ctx.source) else { return Vec::new() };
        let mut out = nothing::diagnostics(&unit);
        out.extend(soft::diagnostics(&unit));
        out.extend(dedicated::diagnostics(&unit));
        out.sort_by_key(|d| d.start);
        out
    }

    /// The fixes for this extension's findings, and the JUnit rewrites — which need no finding, only
    /// AssertJ on the classpath, which `applies` has already established.
    fn intentions(&self, ctx: &FileCtx<'_>, offset: usize, problems: &[ExtProblem]) -> Vec<ExtIntention> {
        if ctx.extension() != "java" {
            return Vec::new();
        }
        let uses_assertj = mentions_any(ctx.source, ASSERTJ_MARKERS);
        let uses_junit = mentions_any(ctx.source, JUNIT_MARKERS);
        if !uses_assertj && !uses_junit {
            return Vec::new();
        }
        let path = ctx.path_str();
        let Some(unit) = Unit::parse(&path, ctx.source) else { return Vec::new() };
        let mut out = Vec::new();
        if uses_assertj {
            out.extend(soft::intentions(&unit, problems));
            out.extend(dedicated::intentions(&unit, offset, problems));
        }
        if uses_junit {
            out.extend(junit::intentions(&unit, offset));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn identity_and_gate() {
        let ext = AssertJExtension::new();
        assert_eq!(ext.id(), "assertj");
        assert_eq!(ext.display_name(), "AssertJ");
        assert!(ext.applies(&CapabilitySet { assertj: true, ..CapabilitySet::default() }));
        assert!(!ext.applies(&CapabilitySet::default()));
        assert!(!ext.is_ready());
        ext.reindex(&ProjectScan::empty(Path::new("/p")));
        assert!(ext.is_ready());
    }

    #[test]
    fn a_file_that_is_not_java_or_not_about_assertions_is_not_read() {
        let ext = AssertJExtension::new();
        let xml = FileCtx { path: Path::new("/p/pom.xml"), source: "<!-- org.assertj assertThat(x); -->" };
        assert!(ext.diagnostics(&xml).is_empty());
        assert!(ext.intentions(&xml, 0, &[]).is_empty());
        let plain = FileCtx { path: Path::new("/p/T.java"), source: "class T { void m() { assertThat(x); } }" };
        assert!(ext.diagnostics(&plain).is_empty(), "nothing imports AssertJ");
    }
}
