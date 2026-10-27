//! False-positive sweep over real, compiling code.
//!
//! The corpus measures Bennu against cases written to probe it. This is the other measure: library
//! sources that javac builds without a diagnostic, so **every error Bennu reports on them is a false
//! positive**, whatever the check. It is how the bugs a hand-written case never thinks of get found —
//! a nested type sharing a simple name with another, `T[]` read through a type argument, a `final`
//! declared inside a loop.
//!
//! Opt-in, because it needs sources this repository does not carry. Point `BENNU_SWEEP_ROOT` at a
//! directory holding one folder per project, each the unpacked sources of a library that needs only the
//! JDK (a `*-sources.jar` from `~/.m2`: commons-lang3, commons-collections4 …):
//!
//! ```text
//! <root>/lang3/src/main/java/org/apache/…
//! <root>/lang3/level            optional: the Java major the project targets (default 8)
//! <root>/lang3/expected.txt     optional: any header line
//! ```
//!
//! Run: `BENNU_SWEEP_ROOT=<root> cargo test -p bennu-intel --test real_code_sweep -- --nocapture`
//!
//! The test never fails — it prints, per project, the false-positive counts by check and the first
//! errors. Lint warnings (`unused-member`, `switch-fallthrough`) are listed in the counts and left out of
//! the errors: they are opinions, not compile errors. A project that needs a library the sweep does not
//! have shows `unresolved-import` for it; that is a missing classpath, not a false positive.

#[allow(dead_code)]
mod check_corpus_support;

use std::collections::BTreeMap;
use std::path::PathBuf;

use check_corpus_support::clean::run_clean_module;
use check_corpus_support::module::CorpusModule;
use check_corpus_support::report;

const LINTS: &[&str] = &["unused-member", "unused-import", "switch-fallthrough", "dead-store"];

/// What a project that needs libraries the sweep does not carry reports for them.
const MISSING_CLASSPATH: &[&str] = &["unresolved-import", "unresolved-type"];

#[test]
fn bennu_stays_silent_on_real_code() {
    let Ok(root) = std::env::var("BENNU_SWEEP_ROOT") else {
        eprintln!("real_code_sweep: BENNU_SWEEP_ROOT is not set, nothing swept (see the file header)");
        return;
    };
    let root = PathBuf::from(root);
    let Ok(entries) = std::fs::read_dir(&root) else {
        eprintln!("real_code_sweep: cannot read {}", root.display());
        return;
    };
    let mut projects: Vec<String> =
        entries.flatten().filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().into_string().ok()).collect();
    projects.sort();
    // `BENNU_SWEEP_ONLY=guava,spring` limits the run to those folders.
    if let Ok(only) = std::env::var("BENNU_SWEEP_ONLY") {
        let wanted: Vec<&str> = only.split(',').map(str::trim).collect();
        projects.retain(|p| wanted.contains(&p.as_str()));
    }
    for name in projects {
        let major = std::fs::read_to_string(root.join(&name).join("level"))
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .unwrap_or(8);
        // The module name is borrowed for the run; leaking it is fine for a test process.
        let leaked: &'static str = Box::leak(name.clone().into_boxed_str());
        let module = CorpusModule { name: leaked, release: "17", major, libs: &[] };
        match run_clean_module(&root, &module) {
            Err(why) => eprintln!("real_code_sweep: {name} skipped: {why}"),
            Ok(result) => {
                eprintln!("real_code_sweep: {}", report::clean_summary_line(&result));
                let mut by_code: BTreeMap<&str, usize> = BTreeMap::new();
                for fp in &result.false_positives {
                    *by_code.entry(fp.code.as_str()).or_default() += 1;
                }
                eprintln!("real_code_sweep: {name} by check {by_code:?}");
                // Missing-classpath noise is counted above but not listed: it hides the real ones.
                let listed = result
                    .false_positives
                    .iter()
                    .filter(|f| !LINTS.contains(&f.code.as_str()) && !MISSING_CLASSPATH.contains(&f.code.as_str()));
                for fp in listed.take(60) {
                    eprintln!("  [{}] {}:{} {}", fp.code, fp.file, fp.first_line, fp.message);
                }
            }
        }
    }
}
