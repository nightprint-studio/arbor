//! What the tests share: running the extension over a buffer the way the host does, and applying the
//! edits it offers the way the editor does.

use std::path::Path;

use bennu_ext::prelude::{ExtEdit, ExtIntention, ExtProblem, FileCtx, FrameworkExtension};
use bennu_proto::prelude::Diagnostic;

use crate::ext::AssertJExtension;

const PATH: &str = "/p/src/test/java/com/acme/OrderTest.java";

fn ctx(source: &str) -> FileCtx<'_> {
    FileCtx { path: Path::new(PATH), source }
}

pub(crate) fn diagnostics(source: &str) -> Vec<Diagnostic> {
    AssertJExtension::new().diagnostics(&ctx(source))
}

/// The text under each squiggle of `code`.
pub(crate) fn squiggled<'s>(source: &'s str, code: &str) -> Vec<&'s str> {
    diagnostics(source).into_iter().filter(|d| d.code == code).map(|d| &source[d.start..d.end]).collect()
}

/// The offer `id` at `offset`, with every finding in the file handed back as the editor would.
pub(crate) fn offered(source: &str, id: &str, offset: usize) -> Option<ExtIntention> {
    let problems: Vec<ExtProblem> = diagnostics(source)
        .into_iter()
        .map(|d| ExtProblem { code: d.code, start: d.start, end: d.end })
        .collect();
    AssertJExtension::new().intentions(&ctx(source), offset, &problems).into_iter().find(|i| i.id == id)
}

/// Apply one offer's edits in a single transaction — every offset into the original text, as the
/// contract says — after checking that none of them overlap.
pub(crate) fn apply(source: &str, edits: &[ExtEdit]) -> String {
    let mut ordered: Vec<&ExtEdit> = edits.iter().collect();
    ordered.sort_by_key(|e| std::cmp::Reverse(e.start));
    for pair in ordered.windows(2) {
        assert!(pair[1].end <= pair[0].start, "overlapping edits: {edits:?}");
    }
    let mut out = source.to_string();
    for edit in ordered {
        out.replace_range(edit.start..edit.end, &edit.text);
    }
    out
}
