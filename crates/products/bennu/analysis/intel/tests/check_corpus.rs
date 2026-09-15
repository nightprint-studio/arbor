//! Differential test: `bennu-check` against javac, over the Maven corpus in
//! `crates/products/bennu/analysis/check/tests/corpus` (its README explains the corpus, the markers and how
//! to refresh the golden files).
//!
//! * **False positive** — a Bennu *error* on a line (widened to its statement) where javac reports no
//!   error at all. The test FAILS on any: the validator's contract is to stay silent when unsure.
//! * **Miss** — a javac error Bennu does not report with a mapped check. Scored, never a failure.
//! * **Mislabeled** — both complain there, but Bennu's check is not one the javac key maps to.
//! * **Clean modules** (`clean8`, `clean21`) — realistic legal code javac compiles silently: EVERY
//!   Bennu diagnostic there, error or warning, is a false positive and fails the test.
//!
//! The javac-key → Bennu-check mapping is `bennu_check::engine::javac` — the same table the langtools and
//! `javac_diff` harnesses score through, so the three can never disagree about what "covered" means.
//!
//! Why here and not in `bennu-check/tests`: validating against the REAL JDK needs the classpath
//! reader, the persisted project index, `IndexResolver` and this crate's project indexer. Those are
//! dependencies of `bennu-intel`, not of `bennu-check` (which `bennu-intel` itself depends on).
//!
//! Skips (prints why, passes) when no JDK resolves or a golden file has not been generated yet.
//!
//! Run: `cargo test -p bennu-intel --test check_corpus -- --nocapture`

mod check_corpus_support;

use std::path::{Path, PathBuf};

use check_corpus_support::clean::run_clean_module;
use check_corpus_support::module::{run_module, CorpusModule};
use check_corpus_support::report;

/// The corpus modules and the language level each is compiled at.
const MODULES: &[CorpusModule] = &[
    CorpusModule { name: "java8", release: "8", major: 8 },
    CorpusModule { name: "java21", release: "21", major: 21 },
];

/// Realistic legal code that must draw no diagnostic at all, error or warning.
const CLEAN_MODULES: &[CorpusModule] = &[
    CorpusModule { name: "clean8", release: "8", major: 8 },
    CorpusModule { name: "clean21", release: "21", major: 21 },
];

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../check/tests/corpus")
}

#[test]
fn bennu_check_reports_no_error_javac_does_not() {
    let root = corpus_root();
    let results: Vec<_> =
        MODULES.iter().filter_map(|module| ran(module, run_module(&root, module))).collect();
    let clean: Vec<_> =
        CLEAN_MODULES.iter().filter_map(|module| ran(module, run_clean_module(&root, module))).collect();
    if results.is_empty() && clean.is_empty() {
        eprintln!("check_corpus: no module ran, nothing was scored");
        return;
    }

    let report_path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bennu-check-corpus.md");
    match std::fs::write(&report_path, report::render(&results, &clean)) {
        Ok(()) => eprintln!("check_corpus: report at {}", report_path.display()),
        Err(why) => eprintln!("check_corpus: could not write {}: {why}", report_path.display()),
    }
    for result in &results {
        eprintln!("check_corpus: {}", report::summary_line(result));
    }
    for result in &clean {
        eprintln!("check_corpus: {}", report::clean_summary_line(result));
    }

    let false_positives = report::false_positive_listing(&results, &clean);
    assert!(
        false_positives.is_empty(),
        "bennu-check reports errors where javac reports none, or any diagnostic on a clean module:\n{false_positives}"
    );
}

/// The module's outcome, or `None` after saying why it was skipped.
fn ran<T>(module: &CorpusModule, outcome: Result<T, String>) -> Option<T> {
    outcome.map_err(|why| eprintln!("check_corpus: SKIPPED {}: {why}", module.name)).ok()
}
