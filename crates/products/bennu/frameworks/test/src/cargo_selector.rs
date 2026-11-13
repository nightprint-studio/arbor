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

use crate::cargo_discover::{RustTest, RustTestKind, TestTarget};

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

/// Which program runs the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CargoRunner {
    /// `cargo test`: every binary in turn, libtest inside each.
    #[default]
    Cargo,
    /// `cargo nextest run`: one process per test, in parallel across binaries. It takes the same
    /// target flags and the same name filters after `--`; it does not run doc-tests.
    Nextest,
}

/// How a run is made, besides what it selects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CargoRunOptions {
    /// Also run the `#[ignore]`d tests — the only way to run one without editing the source.
    pub include_ignored: bool,
    /// Run **only** the `#[ignore]`d tests (libtest's `--ignored`): the slow or windowed ones kept out of the
    /// ordinary run. Wins over `include_ignored`.
    pub ignored_only: bool,
    /// nextest's filter expression (`-E`), e.g. `test(/^play::/) & package(vz-regression)`. nextest only.
    pub filter_expr: Option<String>,
    /// Cargo features to enable (`--features a,b`).
    pub features: Vec<String>,
    /// `--all-features`.
    pub all_features: bool,
    /// `--no-default-features`.
    pub no_default_features: bool,
    pub runner: CargoRunner,
}

impl CargoRunOptions {
    /// The feature flags, in cargo's spelling — the same for `test` and `nextest run`.
    fn feature_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        let features: Vec<&str> = self.features.iter().map(|f| f.trim()).filter(|f| !f.is_empty()).collect();
        if !features.is_empty() {
            args.push("--features".to_string());
            args.push(features.join(","));
        }
        if self.all_features {
            args.push("--all-features".to_string());
        }
        if self.no_default_features {
            args.push("--no-default-features".to_string());
        }
        args
    }
}

/// Build the `cargo test` invocation for `scope`.
///
/// `include_ignored` adds libtest's `--include-ignored`, which is the only way to run a
/// `#[ignore]`d test without editing the source.
pub fn cargo_plan(scope: &CargoTestScope, include_ignored: bool) -> CargoPlan {
    cargo_plan_with(scope, &CargoRunOptions { include_ignored, ..CargoRunOptions::default() })
}

/// Build the invocation for `scope` with `options`: `cargo test`, or `cargo nextest run`.
///
/// The two take the same selection — `-p`, the target flags, the features, and libtest-style name filters (with
/// `--exact` and `--include-ignored`) after the `--` — so one plan serves both; only the head of the line and the
/// output flags differ. nextest is told to print each test's verdict (skips included), the output of a failure
/// at once, nothing for a pass, and no final recap: the recap repeats every failure, which a reader of the stream
/// Past this many crates a run of several is spelled `--workspace` rather than one `-p` each.
const MAX_NAMED_PACKAGES: usize = 12;

/// would count twice.
pub fn cargo_plan_with(scope: &CargoTestScope, options: &CargoRunOptions) -> CargoPlan {
    let include_ignored = options.include_ignored;
    // `--color never` because the output is parsed: libtest already drops colour when its stdout
    // is a pipe, but cargo's own status lines are the ones that carry the target names, and a
    // stray escape sequence in one of those is a target the panel cannot name.
    let mut args = match options.runner {
        CargoRunner::Cargo => vec!["test".to_string()],
        CargoRunner::Nextest => vec!["nextest".to_string(), "run".to_string()],
    };
    args.extend(["--no-fail-fast", "--color", "never"].map(String::from));
    if options.runner == CargoRunner::Nextest {
        args.extend(
            ["--status-level", "skip", "--final-status-level", "none", "--failure-output", "immediate", "--success-output", "never"]
                .map(String::from),
        );
    }
    args.extend(options.feature_args());
    if options.runner == CargoRunner::Nextest {
        if let Some(expr) = options.filter_expr.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
            args.push("-E".to_string());
            args.push(expr.to_string());
        }
    }
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
                // Cases from several targets cannot be narrowed to one target: one invocation is one target
                // set. They are narrowed to their crates instead (`-p` for each), every binary of those is handed
                // the filters and reports only what it matches. Only across very many crates is it the workspace.
                let mut packages: Vec<&str> = cases.iter().map(|c| c.package.as_str()).collect();
                packages.sort_unstable();
                packages.dedup();
                if packages.len() <= MAX_NAMED_PACKAGES {
                    for p in packages {
                        args.push("-p".to_string());
                        args.push(p.to_string());
                    }
                } else {
                    args.push("--workspace".to_string());
                }
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
                    // A module prefix arrives already spelled `cli::`: one `::`, not two (`cli::::` matches nothing).
                    false => format!("{}::", c.path.trim_end_matches("::")),
                });
            }
            // One module prefix found in several binaries is still one filter.
            let mut seen = std::collections::HashSet::new();
            filters.retain(|f| seen.insert(f.clone()));
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

    // nextest takes both spellings after the `--` too, for compatibility with cargo test.
    if options.ignored_only {
        filters.push("--ignored".to_string());
    } else if include_ignored {
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

/// The cases a list of names asks for, against the tests that exist.
///
/// A name is either a test (`cli::tests::parses`) or a **module prefix** (`cli`, `cli::`, `cli::tests`): the
/// second runs every test under that module. A prefix becomes one libtest filter per binary that holds such
/// tests — `cli::`, not exact — so the command line stays one filter long however many tests are under it.
/// libtest matches a filter as a substring, so `cli::` also catches an `other::cli::x` in the same binary;
/// the report lists what ran, which is where an over-match shows. `Err` lists the names that are neither.
pub fn select_cases(found: &[RustTest], wanted: &[String]) -> Result<Vec<RustCaseRef>, Vec<String>> {
    let mut cases: Vec<RustCaseRef> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for raw in wanted {
        let name = raw.trim();
        if let Some(test) = found.iter().find(|t| t.path == name) {
            push_once(&mut cases, RustCaseRef {
                package: test.package.clone(),
                target: test.target.clone(),
                path: test.path.clone(),
                // A parameterised function's real cases are `path::case_1`, `case_2`, … — asking libtest for
                // `path` exactly would run none of them.
                exact: !matches!(test.kind, RustTestKind::Parameterized),
            });
            continue;
        }
        let module = name.trim_end_matches("::");
        let filter = format!("{module}::");
        let under: Vec<&RustTest> = if module.is_empty() { Vec::new() } else { found.iter().filter(|t| t.path.starts_with(&filter)).collect() };
        if under.is_empty() {
            missing.push(name.to_string());
            continue;
        }
        for test in under {
            push_once(&mut cases, RustCaseRef { package: test.package.clone(), target: test.target.clone(), path: filter.clone(), exact: false });
        }
    }
    if missing.is_empty() { Ok(cases) } else { Err(missing) }
}

fn push_once(cases: &mut Vec<RustCaseRef>, case: RustCaseRef) {
    if !cases.contains(&case) {
        cases.push(case);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(package: &str, target: TestTarget, path: &str) -> RustTest {
        RustTest {
            package: package.into(),
            target,
            module: path.rsplit_once("::").map(|(m, _)| m.to_string()).unwrap_or_default(),
            name: path.rsplit("::").next().unwrap_or(path).into(),
            path: path.into(),
            file: String::new(),
            line: 1,
            offset: 0,
            kind: RustTestKind::Test,
            ignored: false,
            should_panic: false,
        }
    }

    #[test]
    fn features_go_before_the_filters_for_either_runner() {
        let options = CargoRunOptions { features: vec!["serde".into(), " dev ".into(), "".into()], no_default_features: true, ..CargoRunOptions::default() };
        let p = cargo_plan_with(&CargoTestScope::Package { package: "a".into() }, &options);
        let at = p.args.iter().position(|a| a == "--features").unwrap();
        assert_eq!(p.args[at + 1], "serde,dev");
        assert!(p.args.contains(&"--no-default-features".to_string()) && !p.args.contains(&"--all-features".to_string()));
        assert!(p.args.iter().position(|a| a == "--").map_or(true, |dash| dash > at));
    }

    #[test]
    fn only_the_ignored_and_a_nextest_filter_reach_the_command_line() {
        let options = CargoRunOptions {
            runner: CargoRunner::Nextest,
            ignored_only: true,
            include_ignored: true,
            filter_expr: Some("test(/^play::/)".into()),
            ..CargoRunOptions::default()
        };
        let plan = cargo_plan_with(&CargoTestScope::Package { package: "vz-regression".into() }, &options);
        let line = plan.args.join(" ");
        assert!(line.contains("-E test(/^play::/)"), "{line}");
        assert!(line.ends_with("-- --ignored"), "{line}");
        // cargo test has no filter expression: it is left out rather than passed to a runner that refuses it.
        let cargo = cargo_plan_with(&CargoTestScope::Workspace, &CargoRunOptions { filter_expr: Some("x".into()), ..CargoRunOptions::default() });
        assert!(!cargo.args.iter().any(|a| a == "-E"));
    }

    #[test]
    fn nextest_takes_the_same_selection_with_its_own_head_and_output_flags() {
        let options = CargoRunOptions { runner: CargoRunner::Nextest, include_ignored: true, ..CargoRunOptions::default() };
        let scope = CargoTestScope::Cases { cases: vec![case("a", TestTarget::Lib, "cli::tests::x")] };
        let p = cargo_plan_with(&scope, &options);
        assert_eq!(&p.args[..2], ["nextest", "run"]);
        assert!(p.args.windows(2).any(|w| w == ["--final-status-level", "none"]));
        assert!(p.args.windows(2).any(|w| w == ["-p", "a"]) && p.args.contains(&"--lib".to_string()));
        assert_eq!(filters_of(&p), ["--exact", "cli::tests::x", "--include-ignored"]);
        assert_eq!(cargo_plan(&scope, true).args[0], "test", "the plain plan is still cargo test");
    }

    #[test]
    fn an_exact_name_is_one_exact_case() {
        let all = [found("a", TestTarget::Lib, "cli::tests::parses"), found("a", TestTarget::Lib, "cli::tests::fails")];
        let cases = select_cases(&all, &["cli::tests::parses".into()]).unwrap();
        assert_eq!(cases, [RustCaseRef { package: "a".into(), target: TestTarget::Lib, path: "cli::tests::parses".into(), exact: true }]);
    }

    #[test]
    fn a_module_prefix_runs_once_filtered_in_the_crates_that_have_it() {
        let all = [
            found("a", TestTarget::Lib, "cli::tests::parses"),
            found("b", TestTarget::Bin { name: "suite".into() }, "cli::main_works"),
            found("c", TestTarget::Lib, "other::tests::x"),
        ];
        let cases = select_cases(&all, &["cli::".into()]).unwrap();
        let p = cargo_plan_with(&CargoTestScope::Cases { cases }, &CargoRunOptions::default());
        let line = p.args.join(" ");
        assert!(line.contains("-p a -p b") && !line.contains("--workspace") && !line.contains("-p c"), "{line}");
        assert_eq!(filters_of(&p), ["cli::"]);
    }

    #[test]
    fn a_module_prefix_is_one_filter_per_binary_that_holds_its_tests() {
        let all = [
            found("a", TestTarget::Lib, "cli::tests::parses"),
            found("a", TestTarget::Lib, "cli::ptn::tests::runs"),
            found("a", TestTarget::Bin { name: "a".into() }, "cli::main_works"),
            found("a", TestTarget::Lib, "other::tests::x"),
        ];
        for spelled in ["cli", "cli::", " cli:: "] {
            let cases = select_cases(&all, &[spelled.into()]).unwrap();
            assert_eq!(cases.len(), 2, "{spelled}: {cases:?}");
            assert!(cases.iter().all(|c| c.path == "cli::" && !c.exact));
        }
    }

    #[test]
    fn a_name_that_is_neither_is_reported() {
        let all = [found("a", TestTarget::Lib, "cli::tests::parses")];
        assert_eq!(select_cases(&all, &["cl".into(), "nothing::".into(), "".into()]).unwrap_err(), ["cl", "nothing::", ""]);
    }

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
