//! Turning "run these tests" into a `cargo test` command line.
//!
//! Cargo splits the job in two, and the split is the whole design here: **cargo** chooses which
//! test binaries to build and run (`-p`, `--lib`, `--test api`), and **libtest** — inside each of
//! those binaries, after the `--` — chooses which of its own tests to run (a filter, optionally
//! `--exact`). Getting the halves the wrong way round is the classic mistake: `cargo test
//! util::tests::works` in a workspace passes the filter to *every* binary of *every* crate, which
//! both wastes the run and can match a same-named test somewhere else entirely.
//!
//! ## Why a selection is narrowed on both sides
//!
//! Narrowing with cargo alone (`-p foo --lib`) runs every test in that target. Narrowing with
//! libtest alone runs the right names in every target. Doing both is what makes "run this one
//! test" mean one test: cargo builds and starts one binary, and that binary runs one case.
//!
//! ## `--no-fail-fast`, always
//!
//! Without it, the first target that fails ends the run and every later crate reports nothing —
//! on a twenty-crate workspace that turns one broken crate into nineteen crates of silence. The
//! panel's whole value is the shape of the failure across the workspace, so the run always goes
//! all the way through.
//!
//! ## A selection that cannot be spelled is widened, never truncated
//!
//! Same doctrine as the Maven half ([`crate::selector`]): a command line is finite, so an
//! oversized set of case filters widens to the enclosing target — running *some* of what was
//! asked for while reporting completion is the same lie in a smaller size. The caller must
//! surface [`CargoPlan::widened`].

use serde::{Deserialize, Serialize};

use crate::cargo_discover::TestTarget;

/// One test case to run: where it lives, and how its name must be matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RustCaseRef {
    pub package: String,
    pub target: TestTarget,
    /// The libtest path (`util::tests::works`).
    pub path: String,
    /// Whether `path` names the case exactly.
    ///
    /// `false` for a parameterized function, whose real cases are `path::case_1`, `path::case_2`
    /// … — there `path` is a **prefix**, and asking libtest for it exactly would run nothing.
    #[serde(default = "yes")]
    pub exact: bool,
}

fn yes() -> bool {
    true
}

/// What to run.
///
/// Five shapes, which are the five levels the panel's tree has: the workspace, a crate, one of
/// that crate's targets, a module inside it, or individual cases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CargoTestScope {
    /// Every test in the workspace. Expressed as `--workspace` with **no filter at all**, so it
    /// also runs the tests discovery never found — a runner that only runs what it recognises is
    /// a runner that hides tests.
    Workspace,
    Package { package: String },
    Target { package: String, target: TestTarget },
    /// Every test under a module path — a substring filter, so submodules come with it.
    Module { package: String, target: TestTarget, module: String },
    Cases { cases: Vec<RustCaseRef> },
}

/// The command line to run, plus what to tell the user about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoPlan {
    /// Arguments for `cargo`, in order, including the `--` and everything after it.
    pub args: Vec<String>,
    /// A short human description of the selection — the tab's title and the log's header.
    pub label: String,
    /// Set when the selection could not be expressed and was widened. The caller MUST surface it.
    pub widened: Option<String>,
}

/// The longest run of filter arguments we will build. Far below any command-line limit — the
/// point is to stay away from the edge, not to find it.
const MAX_FILTER_CHARS: usize = 6000;

/// Build the `cargo test` invocation for `scope`.
///
/// `include_ignored` adds libtest's `--include-ignored`, which is the only way to run a
/// `#[ignore]`d test without editing the source.
pub fn cargo_plan(scope: &CargoTestScope, include_ignored: bool) -> CargoPlan {
    // `--color never` because the output is parsed: libtest already drops colour when its stdout
    // is a pipe, but cargo's own status lines are the ones that carry the target names, and a
    // stray escape sequence in one of those is a target the panel cannot name.
    let mut args = vec![
        "test".to_string(),
        "--no-fail-fast".to_string(),
        "--color".to_string(),
        "never".to_string(),
    ];
    // Everything libtest gets, collected separately and appended after the `--`.
    let mut filters: Vec<String> = Vec::new();
    let mut widened = None;
    let label;

    match scope {
        CargoTestScope::Workspace => {
            args.push("--workspace".to_string());
            label = "all tests".to_string();
        }
        CargoTestScope::Package { package } => {
            args.push("-p".to_string());
            args.push(package.clone());
            label = format!("crate {package}");
        }
        CargoTestScope::Target { package, target } => {
            args.push("-p".to_string());
            args.push(package.clone());
            args.extend(target.selector_args());
            label = format!("{package} {}", target.label());
        }
        CargoTestScope::Module { package, target, module } => {
            args.push("-p".to_string());
            args.push(package.clone());
            args.extend(target.selector_args());
            // A trailing `::` so `util` does not also match `utilities` — and so the module's own
            // submodules do come with it.
            filters.push(format!("{module}::"));
            label = format!("{package} {module}");
        }
        CargoTestScope::Cases { cases } => {
            label = case_label(cases);
            let (narrow, kind) = common_home(cases);
            if let Some((package, target)) = narrow {
                args.push("-p".to_string());
                args.push(package);
                args.extend(target.selector_args());
            } else {
                // Cases from several crates or several targets cannot be narrowed on cargo's
                // side: one invocation is one target set. The whole workspace is built and every
                // binary is handed the filters — each reports only what it matches, so the result
                // is right, it just costs more to get.
                args.push("--workspace".to_string());
            }
            // `--exact` is a property of the whole run, not of one filter, so it is only safe when
            // every case wants it. With a parameterized case in the set the filters stay
            // substrings, which can over-run into a longer same-prefixed name — visible in the
            // panel (it shows what ran), and the right side of the trade against running nothing.
            if kind.all_exact {
                filters.push("--exact".to_string());
            }
            for c in cases {
                filters.push(match c.exact {
                    true => c.path.clone(),
                    // A parameterized function's cases are `path::case_1`; the prefix is what
                    // reaches all of them.
                    false => format!("{}::", c.path),
                });
            }
            let spelled: usize = filters.iter().map(|f| f.len() + 1).sum();
            if spelled > MAX_FILTER_CHARS {
                widened = Some(format!(
                    "{} tests are too many to name on one command line — running the whole selection instead",
                    cases.len()
                ));
                filters.clear();
            }
        }
    }

    if include_ignored {
        filters.push("--include-ignored".to_string());
    }
    if !filters.is_empty() {
        args.push("--".to_string());
        args.extend(filters);
    }

    CargoPlan { args, label, widened }
}

/// Facts about a set of cases that the plan branches on.
struct CaseKind {
    all_exact: bool,
}

/// The single `(package, target)` every case shares, when they share one.
///
/// `None` means the set spans more than one, which is the case that cannot be narrowed on cargo's
/// side — see [`cargo_plan`].
fn common_home(cases: &[RustCaseRef]) -> (Option<(String, TestTarget)>, CaseKind) {
    let kind = CaseKind { all_exact: cases.iter().all(|c| c.exact) };
    let Some(first) = cases.first() else { return (None, kind) };
    let shared = cases
        .iter()
        .all(|c| c.package == first.package && c.target == first.target)
        .then(|| (first.package.clone(), first.target.clone()));
    (shared, kind)
}

fn case_label(cases: &[RustCaseRef]) -> String {
    match cases {
        [] => "all tests".to_string(),
        [one] => one.path.clone(),
        many => format!("{} tests", many.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(package: &str, target: TestTarget, path: &str) -> RustCaseRef {
        RustCaseRef { package: package.to_string(), target, path: path.to_string(), exact: true }
    }

    /// Everything after the `--`, which is libtest's half of the line.
    fn filters_of(p: &CargoPlan) -> Vec<&str> {
        match p.args.iter().position(|a| a == "--") {
            Some(i) => p.args[i + 1..].iter().map(String::as_str).collect(),
            None => Vec::new(),
        }
    }

    /// The one flag every run carries: without it, the first crate that fails silences every
    /// crate after it.
    #[test]
    fn every_run_goes_all_the_way_through() {
        for scope in [
            CargoTestScope::Workspace,
            CargoTestScope::Package { package: "a".into() },
            CargoTestScope::Cases { cases: vec![case("a", TestTarget::Lib, "x")] },
        ] {
            assert!(cargo_plan(&scope, false).args.contains(&"--no-fail-fast".to_string()));
        }
    }

    /// "Everything" must carry NO filter — one that matched nothing would report a green run of
    /// zero tests.
    #[test]
    fn the_whole_workspace_passes_no_filter() {
        let p = cargo_plan(&CargoTestScope::Workspace, false);
        assert!(p.args.contains(&"--workspace".to_string()));
        assert!(filters_of(&p).is_empty());
        assert_eq!(p.label, "all tests");
    }

    #[test]
    fn a_crate_is_narrowed_with_p() {
        let p = cargo_plan(&CargoTestScope::Package { package: "bennu-test".into() }, false);
        let at = p.args.iter().position(|a| a == "-p").expect("-p passed");
        assert_eq!(p.args[at + 1], "bennu-test");
        assert!(filters_of(&p).is_empty());
        assert_eq!(p.label, "crate bennu-test");
    }

    #[test]
    fn a_target_adds_its_own_flag() {
        let p = cargo_plan(
            &CargoTestScope::Target {
                package: "demo".into(),
                target: TestTarget::Test { name: "api".into() },
            },
            false,
        );
        assert!(p.args.windows(2).any(|w| w == ["--test", "api"]));
        assert_eq!(p.label, "demo test api");
    }

    /// A module is narrowed on BOTH sides: cargo starts one binary, libtest runs the subtree.
    #[test]
    fn a_module_narrows_cargo_and_libtest() {
        let p = cargo_plan(
            &CargoTestScope::Module {
                package: "demo".into(),
                target: TestTarget::Lib,
                module: "util::parse".into(),
            },
            false,
        );
        assert!(p.args.contains(&"--lib".to_string()));
        // The trailing `::` keeps `util` from also matching `utilities`.
        assert_eq!(filters_of(&p), ["util::parse::"]);
    }

    #[test]
    fn one_case_is_exact_and_narrowed_to_its_target() {
        let p = cargo_plan(
            &CargoTestScope::Cases {
                cases: vec![case("demo", TestTarget::Lib, "util::tests::works")],
            },
            false,
        );
        assert!(p.args.contains(&"--lib".to_string()));
        assert_eq!(filters_of(&p), ["--exact", "util::tests::works"]);
        assert_eq!(p.label, "util::tests::works");
    }

    /// Cases sharing a home keep the narrow; the shared `-p`/`--lib` is what makes "rerun the
    /// three that failed" one binary instead of the whole workspace.
    #[test]
    fn cases_from_one_target_stay_narrow() {
        let p = cargo_plan(
            &CargoTestScope::Cases {
                cases: vec![
                    case("demo", TestTarget::Lib, "a"),
                    case("demo", TestTarget::Lib, "b"),
                ],
            },
            false,
        );
        assert!(p.args.contains(&"--lib".to_string()));
        assert_eq!(filters_of(&p), ["--exact", "a", "b"]);
        assert_eq!(p.label, "2 tests");
    }

    /// One invocation is one target set, so a selection spanning crates has to be the workspace
    /// with filters — right, just dearer.
    #[test]
    fn cases_from_several_crates_widen_to_the_workspace() {
        let p = cargo_plan(
            &CargoTestScope::Cases {
                cases: vec![case("a", TestTarget::Lib, "x"), case("b", TestTarget::Lib, "y")],
            },
            false,
        );
        assert!(p.args.contains(&"--workspace".to_string()));
        assert!(!p.args.contains(&"-p".to_string()));
        assert_eq!(filters_of(&p), ["--exact", "x", "y"]);
    }

    /// A parameterized function's real cases are `path::case_1`; asking for `path` exactly would
    /// run nothing at all, so the whole run drops `--exact`.
    #[test]
    fn a_parameterized_case_is_a_prefix_and_drops_exact() {
        let mut c = case("demo", TestTarget::Lib, "adds");
        c.exact = false;
        let p = cargo_plan(&CargoTestScope::Cases { cases: vec![c] }, false);
        assert_eq!(filters_of(&p), ["adds::"]);
    }

    #[test]
    fn ignored_tests_are_opt_in() {
        let plain = cargo_plan(&CargoTestScope::Workspace, false);
        assert!(filters_of(&plain).is_empty());
        let with = cargo_plan(&CargoTestScope::Workspace, true);
        assert_eq!(filters_of(&with), ["--include-ignored"]);
    }

    /// An oversized selection is WIDENED, never truncated — running part of what was asked for
    /// and reporting completion is the same lie as running none of it.
    #[test]
    fn an_oversized_selection_widens_instead_of_truncating() {
        let cases: Vec<RustCaseRef> = (0..2000)
            .map(|i| case("demo", TestTarget::Lib, &format!("module::group::rather_long_test_name_{i}")))
            .collect();
        let p = cargo_plan(&CargoTestScope::Cases { cases }, false);
        assert!(p.widened.is_some(), "the widening must be reported");
        assert!(filters_of(&p).is_empty(), "no partial filter");
        // Still narrowed to the target they shared: widened does not mean "the whole workspace".
        assert!(p.args.contains(&"--lib".to_string()));
    }
}
