//! Turning "run these tests" into a Maven command line.
//!
//! ## Why simple class names
//!
//! Surefire's `-Dtest` has grown several dialects. Modern ones accept fully-qualified names
//! and `com.acme.**` package globs; **2.x does not**, and its failure mode is the dangerous
//! one — an expression it doesn't understand doesn't error, it just fails to match, and with
//! `failIfNoTests=false` (which we must pass, or a multi-module build fails in every module
//! that has no matching test) the run reports success having executed nothing. "All green,
//! zero tests" is the single worst thing a test runner can say.
//!
//! So a selection is expressed the one way every Surefire since 2.x has understood: the
//! **simple class name**, comma-separated, with `Class#method` for individual cases. The
//! cost is that two identically-named classes in different packages both run. That is a
//! visible, harmless over-run — the panel shows what actually ran — and it is the right side
//! of the trade against silently running nothing.
//!
//! ## Why a package is a list of classes
//!
//! There is no portable `-Dtest` spelling for "this package and below". Since discovery
//! already knows every test class in the project, a package, a directory or a
//! multi-selection all reduce to the same thing: an explicit list of class names. One
//! mechanism, and it cannot silently match the wrong set.
//!
//! The list does have a limit — a command line is finite, and Windows' is about 32k — so
//! [`plan`] widens an oversized selection to the whole module or project rather than
//! truncating it. A truncated selection would run *some* of what you asked for while
//! reporting completion, which is the same lie in a smaller size.

use serde::{Deserialize, Serialize};

/// One test case: a class (by [selector name](crate::discover::TestClass::selector)) and one
/// of its methods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestCaseRef {
    /// The class's selector name (`OrderTest`, `OuterTest$Inner`).
    pub class: String,
    /// The method name, without parentheses.
    pub method: String,
}

/// What to run. The four shapes the UI can ask for — everything, a Maven module, a set of
/// classes (which is also how a package, a folder or a multi-selection arrives), or
/// individual cases (a single method, or "rerun failed").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TestScope {
    /// Every test in the project. Deliberately expressed as *no* `-Dtest` filter at all, so
    /// it also runs the tests discovery never found — a runner that only runs what it
    /// recognises is a runner that hides tests.
    All,
    /// Every test in one Maven module, by its path relative to the project root.
    Module { module: String },
    /// An explicit set of classes.
    Classes { classes: Vec<String> },
    /// Individual cases.
    Cases { cases: Vec<TestCaseRef> },
}

/// The command line to run, plus what to tell the user about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenPlan {
    /// Arguments for the `mvn` launcher, in order.
    pub args: Vec<String>,
    /// A short human description of the selection — the panel's title and the log's header
    /// line (`OrderTest.computesTotal`, `12 classes`, `all tests`).
    pub label: String,
    /// Set when the selection could not be expressed on one command line and was widened.
    /// The caller must surface it: the user asked for a subset and is getting a superset,
    /// and finding that out from the runtime is not acceptable.
    pub widened: Option<String>,
}

/// The longest `-Dtest=` value we will build. Well under the ~32k Windows command-line
/// limit, with room for the rest of the line — the point is to stay far away from it, not
/// to find its edge.
const MAX_FILTER_CHARS: usize = 6000;

/// Build the Maven invocation for `scope`.
///
/// `offline` mirrors the Build button's `-o`: resolve from the local `~/.m2` only, so a
/// repository that is slow or unreachable cannot turn a test run into a five-minute stall.
pub fn plan(scope: &TestScope, offline: bool) -> MavenPlan {
    let mut args = vec!["test".to_string(), "--batch-mode".to_string()];
    if offline {
        args.push("-o".to_string());
    }
    // Both spellings: the first is honoured by Surefire 2.x, the second by 3.x. Without them
    // a filtered run FAILS in every module that has no matching test, which on a multi-module
    // project is most of them.
    args.push("-DfailIfNoTests=false".to_string());
    args.push("-Dsurefire.failIfNoSpecifiedTests=false".to_string());

    let mut widened = None;
    let label;

    match scope {
        TestScope::All => {
            label = "all tests".to_string();
        }
        TestScope::Module { module } => {
            args.push("-pl".to_string());
            args.push(module.clone());
            label = format!("module {module}");
        }
        TestScope::Classes { classes } => {
            let filter = classes.join(",");
            label = class_label(classes);
            if filter.len() > MAX_FILTER_CHARS {
                widened = Some(format!(
                    "{} classes are too many to name on one command line — running every test instead",
                    classes.len()
                ));
            } else if !classes.is_empty() {
                args.push(format!("-Dtest={filter}"));
            }
        }
        TestScope::Cases { cases } => {
            let filter = case_filter(cases);
            label = case_label(cases);
            if filter.len() > MAX_FILTER_CHARS {
                widened = Some(format!(
                    "{} cases are too many to name on one command line — running every test instead",
                    cases.len()
                ));
            } else if !cases.is_empty() {
                args.push(format!("-Dtest={filter}"));
            }
        }
    }

    MavenPlan { args, label, widened }
}

/// `A#one+two,B#three` — cases grouped by class, because that is the form Surefire has
/// accepted longest, and because repeating the class name for each method wastes the
/// command-line budget the grouping exists to protect.
fn case_filter(cases: &[TestCaseRef]) -> String {
    let mut classes: Vec<&str> = Vec::new();
    let mut methods: Vec<Vec<&str>> = Vec::new();
    for c in cases {
        match classes.iter().position(|k| *k == c.class) {
            Some(i) => {
                if !methods[i].contains(&c.method.as_str()) {
                    methods[i].push(&c.method);
                }
            }
            None => {
                classes.push(&c.class);
                methods.push(vec![&c.method]);
            }
        }
    }
    classes
        .iter()
        .zip(methods.iter())
        .map(|(c, ms)| format!("{c}#{}", ms.join("+")))
        .collect::<Vec<_>>()
        .join(",")
}

fn class_label(classes: &[String]) -> String {
    match classes {
        [] => "all tests".to_string(),
        [one] => one.replace('$', "."),
        many => format!("{} classes", many.len()),
    }
}

fn case_label(cases: &[TestCaseRef]) -> String {
    match cases {
        [] => "all tests".to_string(),
        [one] => format!("{}.{}", one.class.replace('$', "."), one.method),
        many => format!("{} tests", many.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(class: &str, method: &str) -> TestCaseRef {
        TestCaseRef { class: class.to_string(), method: method.to_string() }
    }

    fn filter_of(p: &MavenPlan) -> Option<&str> {
        p.args.iter().find_map(|a| a.strip_prefix("-Dtest="))
    }

    /// "Everything" must carry NO filter — a filter that matched nothing would report a
    /// green run of zero tests.
    #[test]
    fn all_passes_no_test_filter() {
        let p = plan(&TestScope::All, true);
        assert!(filter_of(&p).is_none());
        assert!(p.args.contains(&"test".to_string()));
        assert_eq!(p.label, "all tests");
    }

    #[test]
    fn offline_is_opt_in() {
        assert!(plan(&TestScope::All, true).args.contains(&"-o".to_string()));
        assert!(!plan(&TestScope::All, false).args.contains(&"-o".to_string()));
    }

    /// Both no-tests flags are always present: without them a filtered multi-module run
    /// fails in every module that has nothing to match.
    #[test]
    fn always_tolerates_modules_without_matching_tests() {
        let p = plan(&TestScope::Classes { classes: vec!["OrderTest".into()] }, true);
        assert!(p.args.contains(&"-DfailIfNoTests=false".to_string()));
        assert!(p.args.contains(&"-Dsurefire.failIfNoSpecifiedTests=false".to_string()));
    }

    #[test]
    fn a_single_class_is_named_by_its_simple_name() {
        let p = plan(&TestScope::Classes { classes: vec!["OrderTest".into()] }, true);
        assert_eq!(filter_of(&p), Some("OrderTest"));
        assert_eq!(p.label, "OrderTest");
    }

    #[test]
    fn several_classes_are_comma_separated() {
        let p = plan(
            &TestScope::Classes { classes: vec!["AT".into(), "BT".into(), "CT".into()] },
            true,
        );
        assert_eq!(filter_of(&p), Some("AT,BT,CT"));
        assert_eq!(p.label, "3 classes");
    }

    #[test]
    fn one_case_is_class_hash_method() {
        let p = plan(&TestScope::Cases { cases: vec![case("OrderTest", "computesTotal")] }, true);
        assert_eq!(filter_of(&p), Some("OrderTest#computesTotal"));
        assert_eq!(p.label, "OrderTest.computesTotal");
    }

    /// Cases of the same class share one entry — the form Surefire has accepted longest,
    /// and the one that fits most selection into the command line.
    #[test]
    fn cases_are_grouped_by_class() {
        let p = plan(
            &TestScope::Cases {
                cases: vec![case("AT", "one"), case("BT", "x"), case("AT", "two")],
            },
            true,
        );
        assert_eq!(filter_of(&p), Some("AT#one+two,BT#x"));
        assert_eq!(p.label, "3 tests");
    }

    #[test]
    fn duplicate_cases_collapse() {
        let p = plan(
            &TestScope::Cases { cases: vec![case("AT", "one"), case("AT", "one")] },
            true,
        );
        assert_eq!(filter_of(&p), Some("AT#one"));
    }

    #[test]
    fn a_module_run_uses_pl_and_no_filter() {
        let p = plan(&TestScope::Module { module: "core".into() }, true);
        assert!(filter_of(&p).is_none());
        let pl = p.args.iter().position(|a| a == "-pl").expect("-pl passed");
        assert_eq!(p.args[pl + 1], "core");
        assert_eq!(p.label, "module core");
    }

    /// An oversized selection is WIDENED, never truncated: running some of what was asked
    /// for and reporting completion is the same lie as running none of it.
    #[test]
    fn an_oversized_selection_widens_instead_of_truncating() {
        let classes: Vec<String> = (0..2000).map(|i| format!("SomeRatherLongTestName{i}")).collect();
        let p = plan(&TestScope::Classes { classes }, true);
        assert!(filter_of(&p).is_none(), "no partial filter");
        assert!(p.widened.is_some(), "the widening must be reported");
    }

    #[test]
    fn a_nested_class_label_reads_as_java_writes_it() {
        let p = plan(&TestScope::Classes { classes: vec!["OuterTest$Inner".into()] }, true);
        assert_eq!(filter_of(&p), Some("OuterTest$Inner"), "the filter keeps the $");
        assert_eq!(p.label, "OuterTest.Inner", "the label reads as source");
    }
}
