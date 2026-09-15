//! `test_engines` domain — the test classes this project will **not run**, and the build is green.
//!
//! ## The defect
//!
//! The JUnit Platform runs nothing by itself. It runs *engines*, and each engine understands one
//! dialect: `junit-jupiter-engine` for JUnit 5's `@Test`, `junit-vintage-engine` for JUnit 4's. A
//! project migrated to Jupiter that forgot the vintage engine still **compiles** every JUnit 4 test
//! it has — the `junit:junit` jar is still on the test classpath, the annotations resolve, the
//! assertions resolve — and Surefire simply never executes them.
//!
//! What you see is a build that passes. What is actually true is that a hundred test classes did
//! not run. Nothing anywhere says so: Surefire reports on what it ran, and it ran the other ones.
//!
//! It goes the other way too, and is just as quiet: `junit-vintage-engine` without
//! `junit-jupiter-engine` means the JUnit 5 tests are the ones being skipped.
//!
//! ## Why this can be answered here and not by the build
//!
//! Because it needs two facts that live in different places: which dialect each test class is
//! written in (the sources — [`bennu_test`]'s discovery already records it) and which engines are
//! on the test classpath (the resolved dependencies). Neither half is suspicious on its own.
//!
//! ## What it refuses to claim
//!
//! Nothing at all until the classpath has actually been resolved: an empty jar list means *we have
//! not looked yet*, and reporting "the vintage engine is missing" from it would be reporting our
//! own cold start as the project's defect. Same for a project that does not use the Platform at all
//! — a plain JUnit 4 build with Surefire runs its tests exactly as it always did.

use std::path::PathBuf;

use bennu_core::prelude::BennuState;
use bennu_test::prelude::TestFramework;
use serde::{Deserialize, Serialize};

/// A dialect of test the project has, and cannot run.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EngineGap {
    /// `"JUnit 4"` / `"JUnit 5"` — how the classes are written.
    pub framework: String,
    /// The artifact that would run them (`junit-vintage-engine`).
    pub missing: String,
    /// How many test classes are affected.
    pub classes: usize,
    /// One of them, for a sentence that names something real.
    pub sample: String,
}

/// Args for [`bennu_test_engine_gap`].
#[derive(Deserialize)]
pub struct RootArgs {
    /// Absolute path to the project root.
    pub root: String,
}

/// Which of this project's test dialects have no engine to run them.
///
/// Empty for the overwhelming majority of projects, and empty — deliberately — whenever the answer
/// would be a guess: no resolved classpath, or a build that does not use the JUnit Platform.
#[arbor_rpc::handler]
fn bennu_test_engine_gap(_ctx: &BennuState, args: RootArgs) -> Result<Vec<EngineGap>, String> {
    let root = PathBuf::from(&args.root);
    let jars = crate::dep_classpath::cached_dep_jars(&root);
    Ok(gaps(&jars, &discovered(&args.root)))
}

/// The (framework, class name) of every test class the project declares.
fn discovered(root: &str) -> Vec<(TestFramework, String)> {
    crate::tests::cached_discovery(root)
        .iter()
        // Abstract bases hold shared methods and are never run by anybody — counting them as
        // "classes that will not run" would put a number on the warning that nobody can act on.
        .filter(|d| !d.class.is_abstract)
        .map(|d| (d.class.framework, d.class.selector.clone()))
        .collect()
}

/// One jar name, lowercased, without its version — `junit-jupiter-engine-5.10.2.jar` is looked at
/// as `junit-jupiter-engine`. A `contains` on the whole file name would be enough for these five
/// artifacts, and is what this does; the comment is here so nobody "improves" it into a parse.
fn has_jar(jars: &[PathBuf], artifact: &str) -> bool {
    jars.iter().any(|j| {
        j.file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase().contains(artifact))
            .unwrap_or(false)
    })
}

/// The gaps, given a resolved classpath and the discovered classes.
///
/// Split from the handler so every rule below is testable against a list of names — which is what
/// the shape of the answer actually depends on.
pub(crate) fn gaps(jars: &[PathBuf], classes: &[(TestFramework, String)]) -> Vec<EngineGap> {
    // Nothing resolved yet is not evidence of anything.
    if jars.is_empty() {
        return Vec::new();
    }
    // A build that never loads the Platform runs its JUnit 4 tests the way it always has.
    let platform = has_jar(jars, "junit-platform");
    if !platform {
        return Vec::new();
    }

    // `junit-jupiter` is the aggregate, and brings the engine with it — a project that declares it
    // has an engine even though no jar is named `junit-jupiter-engine` in its own pom.
    let jupiter = has_jar(jars, "junit-jupiter-engine") || has_jar(jars, "junit-jupiter-5");
    let vintage = has_jar(jars, "junit-vintage-engine");

    let mut out = Vec::new();
    for (framework, missing, present) in [
        (TestFramework::JUnit4, "junit-vintage-engine", vintage),
        (TestFramework::JUnit5, "junit-jupiter-engine", jupiter),
        // JUnit 3's `TestCase` shape is run by the vintage engine as well — same jar, same gap.
        (TestFramework::JUnit3, "junit-vintage-engine", vintage),
    ] {
        if present {
            continue;
        }
        let mut affected: Vec<&str> =
            classes.iter().filter(|(f, _)| *f == framework).map(|(_, n)| n.as_str()).collect();
        if affected.is_empty() {
            continue;
        }
        affected.sort_unstable();
        out.push(EngineGap {
            framework: framework.label().to_string(),
            missing: missing.to_string(),
            classes: affected.len(),
            sample: affected[0].to_string(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jars(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(|n| PathBuf::from(format!("/repo/{n}"))).collect()
    }

    fn four(name: &str) -> (TestFramework, String) {
        (TestFramework::JUnit4, name.to_string())
    }
    fn five(name: &str) -> (TestFramework, String) {
        (TestFramework::JUnit5, name.to_string())
    }

    /// The reported case: migrated to Jupiter, kept the old tests, forgot the vintage engine. They
    /// compile, they are never run, and the build is green.
    #[test]
    fn junit4_classes_with_no_vintage_engine_are_the_defect() {
        let found = gaps(
            &jars(&[
                "junit-platform-engine-1.10.2.jar",
                "junit-jupiter-engine-5.10.2.jar",
                "junit-4.13.2.jar",
            ]),
            &[four("OrderTest"), four("CartTest"), five("NuovoTest")],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].framework, "JUnit 4");
        assert_eq!(found[0].missing, "junit-vintage-engine");
        assert_eq!(found[0].classes, 2);
        assert_eq!(found[0].sample, "CartTest");
    }

    /// And the same defect the other way round, which is just as silent.
    #[test]
    fn junit5_classes_with_only_the_vintage_engine_are_the_same_defect() {
        let found = gaps(
            &jars(&["junit-platform-engine-1.10.2.jar", "junit-vintage-engine-5.10.2.jar"]),
            &[five("NuovoTest"), four("OrderTest")],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].framework, "JUnit 5");
        assert_eq!(found[0].missing, "junit-jupiter-engine");
    }

    /// The aggregate artifact brings the engine with it.
    #[test]
    fn the_jupiter_aggregate_counts_as_an_engine() {
        let found = gaps(
            &jars(&["junit-platform-engine-1.10.2.jar", "junit-jupiter-5.10.2.jar"]),
            &[five("NuovoTest")],
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// A plain JUnit 4 build runs its tests exactly as it always did — the Platform is not in it.
    #[test]
    fn a_project_without_the_platform_is_not_a_project_with_a_gap() {
        let found = gaps(&jars(&["junit-4.13.2.jar", "hamcrest-core-1.3.jar"]), &[four("OrderTest")]);
        assert!(found.is_empty(), "{found:?}");
    }

    /// An unresolved classpath is not evidence of a missing engine — it is evidence of not having
    /// looked, and reporting our own cold start as the project's defect is the worst kind of noise.
    #[test]
    fn nothing_is_claimed_before_the_classpath_resolves() {
        assert!(gaps(&[], &[four("OrderTest")]).is_empty());
    }

    /// A properly wired project says nothing, which is nearly all of them.
    #[test]
    fn a_project_with_both_engines_is_quiet() {
        let found = gaps(
            &jars(&[
                "junit-platform-engine-1.10.2.jar",
                "junit-jupiter-engine-5.10.2.jar",
                "junit-vintage-engine-5.10.2.jar",
            ]),
            &[four("OrderTest"), five("NuovoTest")],
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// No JUnit 4 class means nothing to report, however the engines are wired.
    #[test]
    fn a_missing_engine_nobody_needs_is_not_a_gap() {
        let found = gaps(
            &jars(&["junit-platform-engine-1.10.2.jar", "junit-jupiter-engine-5.10.2.jar"]),
            &[five("NuovoTest")],
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// JUnit 3's `TestCase` shape is the vintage engine's job too.
    #[test]
    fn a_junit3_test_case_needs_the_vintage_engine_as_well() {
        let found = gaps(
            &jars(&["junit-platform-engine-1.10.2.jar", "junit-jupiter-engine-5.10.2.jar"]),
            &[(TestFramework::JUnit3, "LegacyTest".to_string())],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].missing, "junit-vintage-engine");
    }
}
