//! Debugging a cargo target: build it, find what was built, run that under the adapter.
//!
//! ## Why this is not "cargo run, with a debugger"
//!
//! A JVM is debugged by *launching it differently* — an agent argument, and the VM connects back. A
//! native binary cannot be: the debugger has to be the thing that starts the process, so the binary
//! must exist first and its path must be known.
//!
//! `cargo run` gives neither. It builds and then execs the binary itself, so there is no moment at
//! which Bennu holds a path and no process to hand to an adapter. So the sequence is:
//!
//! 1. `cargo build …  --message-format=json-render-diagnostics`
//! 2. read the **`compiler-artifact`** messages and take the `executable` of the target that was asked
//!    for — see [`executable_of`], which is where all the subtlety is;
//! 3. hand that path to [`crate::debug_dap`].
//!
//! ## Why the path is read from cargo and not composed
//!
//! `target/debug/<name>` is wrong often enough to matter, and every way it is wrong is silent: a
//! `--profile` other than dev or release puts it under `target/<profile>/`, a `[[bin]]` with a `name`
//! different from the package's changes the file name, a workspace with a `target-dir` in
//! `.cargo/config.toml` moves the whole tree, a cross-compilation target adds a triple directory, and
//! a **test** binary has a hash suffix that is not predictable at all. Cargo already computes this and
//! says so in its JSON; guessing it instead would produce "no such file" on exactly the projects that
//! are configured rather than default.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use arbor_process_ext::NoWindowExt;
use bennu_cargo::prelude::{Invocation, TargetSelector};
use serde::Deserialize;

/// The cargo executable, as everywhere else in this crate.
// Resolved, not taken from `PATH`: see `cargo_cmd::cargo_launcher`.
use crate::cargo_cmd::cargo_launcher;

/// One `compiler-artifact` line, reduced to what matters.
#[derive(Debug, Clone, Deserialize)]
struct Artifact {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    target: ArtifactTarget,
    /// The built file, when this artifact is runnable. `None` for a library, and for the many
    /// artifacts of a build that are not the thing asked for.
    #[serde(default)]
    executable: Option<String>,
    /// Whether it was built with `--test` — which is what distinguishes a test binary from the
    /// ordinary one built from the same source file.
    #[serde(default)]
    profile: ArtifactProfile,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ArtifactTarget {
    #[serde(default)]
    name: String,
    /// `bin` · `lib` · `test` · `example` · `bench`. A file can produce several.
    #[serde(default)]
    kind: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ArtifactProfile {
    #[serde(default)]
    test: bool,
}

/// What was asked to be debugged, in cargo's own terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wanted {
    /// The package's default binary, or a named `[[bin]]`.
    Bin(String),
    /// A named `[[example]]`.
    Example(String),
    /// A test binary — the harness, which is then given a filter.
    Test(String),
    /// Whatever the build produced, when nothing was named. Only unambiguous when there is one.
    Any,
}

impl Wanted {
    /// What the selector in a run configuration means here.
    ///
    /// The plural kinds (`bins`, `tests`, `all-targets`) become [`Wanted::Any`]: they build several
    /// things, and which of them to debug is a question the artifact stream answers or refuses.
    pub fn from_selector(selector: &TargetSelector) -> Wanted {
        let name = selector.name.trim();
        if name.is_empty() {
            return Wanted::Any;
        }
        match selector.kind.trim() {
            "bin" => Wanted::Bin(name.to_string()),
            "example" => Wanted::Example(name.to_string()),
            // A bench is a test binary as far as building and running one goes.
            "test" | "bench" => Wanted::Test(name.to_string()),
            _ => Wanted::Any,
        }
    }
}

/// Pick the executable to debug out of a build's artifact stream.
///
/// The rules, in the order they matter:
///
/// * an artifact with no `executable` is not a candidate — that is every library, and every
///   dependency;
/// * a **test** artifact and a non-test one can come from the same source file with the same target
///   name, so `profile.test` is what tells them apart. Asking for a test and getting the binary means
///   debugging a program instead of its tests, which does not fail — it runs the wrong thing;
/// * the **last** matching artifact wins, because a rebuild of the same target appears again and the
///   newest line describes the file that is now on disk;
/// * with nothing named, exactly one candidate is an answer and several is not. Choosing one of four
///   silently is how you debug a different binary from the one you meant.
pub fn executable_of(stream: &str, wanted: &Wanted) -> Result<String, String> {
    let artifacts: Vec<Artifact> = stream
        .lines()
        .filter_map(|line| serde_json::from_str::<Artifact>(line).ok())
        .filter(|a| a.reason == "compiler-artifact")
        .filter(|a| a.executable.is_some())
        .collect();

    let matches: Vec<&Artifact> = artifacts
        .iter()
        .filter(|a| match wanted {
            Wanted::Bin(name) => {
                !a.profile.test && a.target.name == *name && a.target.kind.iter().any(|k| k == "bin")
            }
            Wanted::Example(name) => {
                !a.profile.test
                    && a.target.name == *name
                    && a.target.kind.iter().any(|k| k == "example")
            }
            // A test binary is one built with `--test`, whatever kind its target claims: a `#[test]`
            // in `main.rs` produces a `bin` target with `profile.test` set.
            Wanted::Test(name) => a.profile.test && a.target.name == *name,
            Wanted::Any => true,
        })
        .collect();

    match matches.as_slice() {
        [] => Err(match wanted {
            Wanted::Bin(name) => format!(
                "the build produced no binary called `{name}`. A `[[bin]]` target's name is what to debug, not the package's."
            ),
            Wanted::Example(name) => format!("the build produced no example called `{name}`."),
            Wanted::Test(name) => format!(
                "the build produced no test binary called `{name}`. A test target has to be built with `--test` to be one."
            ),
            Wanted::Any => {
                "the build produced nothing runnable. A library has no binary to debug — pick a bin, an example, or a test.".to_string()
            }
        }),
        // The newest line for that target: a rebuild reports it again, and the last one describes what
        // is on disk now.
        [..] if !matches!(wanted, Wanted::Any) => {
            Ok(matches.last().unwrap().executable.clone().unwrap_or_default())
        }
        [one] => Ok(one.executable.clone().unwrap_or_default()),
        several => {
            let mut names: Vec<&str> = several.iter().map(|a| a.target.name.as_str()).collect();
            names.sort_unstable();
            names.dedup();
            if names.len() == 1 {
                return Ok(several.last().unwrap().executable.clone().unwrap_or_default());
            }
            Err(format!(
                "this build produced {} runnable targets ({}) — name which one to debug in the run configuration.",
                names.len(),
                names.join(", ")
            ))
        }
    }
}

/// The build command for a debug launch.
///
/// `build` rather than the invocation's own command, and `--test --no-run` for a test target: what is
/// wanted is the artifact, not a run. `--message-format=json-render-diagnostics` is the load-bearing
/// flag — the JSON is how the path is learnt, and `render-diagnostics` keeps the human-readable errors
/// on stderr so a build failure still reads like one in the console.
pub fn build_argv(invocation: &Invocation, wanted: &Wanted) -> Vec<String> {
    let mut argv: Vec<String> = Vec::new();
    match wanted {
        Wanted::Test(name) => {
            argv.push("test".to_string());
            argv.push("--no-run".to_string());
            argv.push("--test".to_string());
            argv.push(name.clone());
        }
        Wanted::Bin(name) => {
            argv.push("build".to_string());
            argv.push("--bin".to_string());
            argv.push(name.clone());
        }
        Wanted::Example(name) => {
            argv.push("build".to_string());
            argv.push("--example".to_string());
            argv.push(name.clone());
        }
        Wanted::Any => argv.push("build".to_string()),
    }
    if !invocation.package.is_empty() {
        argv.push("-p".to_string());
        argv.push(invocation.package.clone());
    }
    if !invocation.profile.is_empty() {
        argv.push("--profile".to_string());
        argv.push(invocation.profile.clone());
    } else if invocation.release {
        // Allowed, and worth allowing — an optimised build is sometimes the only one that reproduces
        // the bug — but the frontend says that the line numbers will be approximate.
        argv.push("--release".to_string());
    }
    if invocation.all_features {
        argv.push("--all-features".to_string());
    }
    if invocation.no_default_features {
        argv.push("--no-default-features".to_string());
    }
    if !invocation.features.is_empty() {
        argv.push("--features".to_string());
        argv.push(invocation.features.join(","));
    }
    argv.extend(invocation.extra.iter().cloned());
    argv.push("--message-format=json-render-diagnostics".to_string());
    argv
}

/// Build, and return the executable to debug.
///
/// `on_line` receives the build's human-readable output — the diagnostics on stderr — so the console
/// shows a failing build as a failing build rather than as "the debugger would not start".
pub fn build_and_locate(
    root: &str,
    cwd: &str,
    invocation: &Invocation,
    wanted: &Wanted,
    mut on_line: impl FnMut(&str),
) -> Result<String, String> {
    let argv = build_argv(invocation, wanted);
    let launcher = cargo_launcher();
    let mut cmd = Command::new(&launcher);
    cmd.current_dir(cwd);
    for a in &argv {
        cmd.arg(a);
    }
    cmd.env("CARGO_TERM_COLOR", "never");
    cmd.env("CARGO_TERM_PROGRESS_WHEN", "never");
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    cmd.no_window();

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn cargo ({}): {e}", launcher.to_string_lossy()))?;
    // stderr on a thread, because the two pipes fill independently and reading one to the end while
    // the other is full deadlocks a verbose build.
    let stderr = child.stderr.take();
    let errors = std::thread::spawn(move || {
        let mut lines = Vec::new();
        if let Some(stderr) = stderr {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                lines.push(line);
            }
        }
        lines
    });

    let mut stream = String::new();
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            stream.push_str(&line);
            stream.push('\n');
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    for line in errors.join().unwrap_or_default() {
        on_line(&line);
    }
    if !status.success() {
        return Err("the build failed — nothing to debug yet.".to_string());
    }
    let exe = executable_of(&stream, wanted)?;
    // Cargo said it built this; if it is not there, something removed it between the build and now,
    // and reporting that is better than handing an adapter a path it will fail on obscurely.
    if !PathBuf::from(&exe).is_file() {
        return Err(format!("cargo reported building `{exe}`, but it is not there."));
    }
    let _ = root;
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic artifact stream: a dependency, the package's library, its binary, and a test binary
    /// built from the same source — which is the case that has to be told apart.
    const STREAM: &str = r#"
{"reason":"compiler-artifact","target":{"name":"serde","kind":["lib"]},"executable":null,"profile":{"test":false}}
{"reason":"compiler-artifact","target":{"name":"geode","kind":["lib"]},"executable":null,"profile":{"test":false}}
{"reason":"compiler-artifact","target":{"name":"geode","kind":["bin"]},"executable":"/p/target/debug/geode","profile":{"test":false}}
{"reason":"compiler-artifact","target":{"name":"geode","kind":["bin"]},"executable":"/p/target/debug/deps/geode-9a1f","profile":{"test":true}}
{"reason":"build-finished","success":true}
"#;

    #[test]
    fn a_named_binary_is_found_by_name_and_kind() {
        assert_eq!(
            executable_of(STREAM, &Wanted::Bin("geode".into())).unwrap(),
            "/p/target/debug/geode"
        );
    }

    /// The distinction that matters most: a test binary and the ordinary one share a target name, and
    /// picking the wrong one does not fail — it debugs a program instead of its tests.
    #[test]
    fn a_test_binary_is_not_confused_with_the_binary_of_the_same_name() {
        assert_eq!(
            executable_of(STREAM, &Wanted::Test("geode".into())).unwrap(),
            "/p/target/debug/deps/geode-9a1f",
        );
        assert_eq!(
            executable_of(STREAM, &Wanted::Bin("geode".into())).unwrap(),
            "/p/target/debug/geode",
            "asking for the bin must not answer with the test harness",
        );
    }

    #[test]
    fn a_library_has_nothing_to_debug_and_says_so() {
        let lib_only = r#"{"reason":"compiler-artifact","target":{"name":"geode","kind":["lib"]},"executable":null,"profile":{"test":false}}"#;
        let err = executable_of(lib_only, &Wanted::Any).unwrap_err();
        assert!(err.contains("library"), "{err}");
    }

    #[test]
    fn asking_for_a_target_the_build_did_not_produce_names_it() {
        let err = executable_of(STREAM, &Wanted::Bin("nope".into())).unwrap_err();
        assert!(err.contains("nope"), "{err}");
        let err = executable_of(STREAM, &Wanted::Example("demo".into())).unwrap_err();
        assert!(err.contains("demo"), "{err}");
    }

    /// Choosing one of several silently is how you debug a different binary from the one you meant.
    #[test]
    fn several_runnable_targets_with_nothing_named_is_a_question_not_a_guess() {
        let two = r#"
{"reason":"compiler-artifact","target":{"name":"geode","kind":["bin"]},"executable":"/p/target/debug/geode","profile":{"test":false}}
{"reason":"compiler-artifact","target":{"name":"digger","kind":["bin"]},"executable":"/p/target/debug/digger","profile":{"test":false}}
"#;
        let err = executable_of(two, &Wanted::Any).unwrap_err();
        assert!(err.contains("geode") && err.contains("digger"), "{err}");
        assert!(err.contains("name which one"), "the message has to say what to do: {err}");
    }

    #[test]
    fn one_runnable_target_with_nothing_named_is_unambiguous() {
        let one = r#"{"reason":"compiler-artifact","target":{"name":"geode","kind":["bin"]},"executable":"/p/target/debug/geode","profile":{"test":false}}"#;
        assert_eq!(executable_of(one, &Wanted::Any).unwrap(), "/p/target/debug/geode");
    }

    /// A rebuild reports the same target again, and the newest line is the file that is on disk.
    #[test]
    fn the_last_artifact_for_a_target_wins() {
        let rebuilt = r#"
{"reason":"compiler-artifact","target":{"name":"geode","kind":["bin"]},"executable":"/p/target/debug/geode-old","profile":{"test":false}}
{"reason":"compiler-artifact","target":{"name":"geode","kind":["bin"]},"executable":"/p/target/debug/geode","profile":{"test":false}}
"#;
        assert_eq!(
            executable_of(rebuilt, &Wanted::Bin("geode".into())).unwrap(),
            "/p/target/debug/geode"
        );
    }

    #[test]
    fn junk_in_the_stream_is_skipped_rather_than_fatal() {
        // cargo interleaves other reasons, and a nightly may add fields or a stray line.
        let messy = format!("not json\n{{}}\n{STREAM}");
        assert_eq!(
            executable_of(&messy, &Wanted::Bin("geode".into())).unwrap(),
            "/p/target/debug/geode"
        );
    }

    // ── the command line ──────────────────────────────────────────────────────

    fn invocation() -> Invocation {
        Invocation {
            command: "run".into(),
            package: String::new(),
            workspace: false,
            target: TargetSelector { kind: String::new(), name: String::new() },
            release: false,
            profile: String::new(),
            features: Vec::new(),
            all_features: false,
            no_default_features: false,
            extra: Vec::new(),
            args: Vec::new(),
        }
    }

    #[test]
    fn the_build_always_asks_for_the_json_that_carries_the_path() {
        let argv = build_argv(&invocation(), &Wanted::Any);
        assert!(
            argv.contains(&"--message-format=json-render-diagnostics".to_string()),
            "the JSON is how the executable is learnt: {argv:?}"
        );
        assert_eq!(argv[0], "build");
    }

    /// A test target is built, not run: `--no-run` is what makes the artifact the product.
    #[test]
    fn a_test_target_is_built_and_not_run() {
        let argv = build_argv(&invocation(), &Wanted::Test("integration".into()));
        assert_eq!(argv[0], "test");
        assert!(argv.contains(&"--no-run".to_string()), "{argv:?}");
        assert!(argv.windows(2).any(|w| w == ["--test", "integration"]), "{argv:?}");
    }

    #[test]
    fn a_named_bin_and_example_reach_the_command_line() {
        let bin = build_argv(&invocation(), &Wanted::Bin("geode".into()));
        assert!(bin.windows(2).any(|w| w == ["--bin", "geode"]), "{bin:?}");
        let example = build_argv(&invocation(), &Wanted::Example("demo".into()));
        assert!(example.windows(2).any(|w| w == ["--example", "demo"]), "{example:?}");
    }

    #[test]
    fn the_invocations_package_profile_and_features_are_carried_through() {
        let mut inv = invocation();
        inv.package = "geode-core".into();
        inv.features = vec!["a".into(), "b".into()];
        inv.no_default_features = true;
        inv.extra = vec!["--locked".into()];
        let argv = build_argv(&inv, &Wanted::Any);
        assert!(argv.windows(2).any(|w| w == ["-p", "geode-core"]), "{argv:?}");
        assert!(argv.windows(2).any(|w| w == ["--features", "a,b"]), "{argv:?}");
        assert!(argv.contains(&"--no-default-features".to_string()));
        assert!(argv.contains(&"--locked".to_string()));
    }

    /// A named profile wins over `--release`, which is itself only `--profile release` spelled short —
    /// and building with both is how you get a binary in a directory nobody looked in.
    #[test]
    fn a_named_profile_wins_over_release() {
        let mut inv = invocation();
        inv.release = true;
        inv.profile = "bench".into();
        let argv = build_argv(&inv, &Wanted::Any);
        assert!(argv.windows(2).any(|w| w == ["--profile", "bench"]), "{argv:?}");
        assert!(!argv.contains(&"--release".to_string()), "{argv:?}");
    }

    #[test]
    fn release_alone_is_passed_as_release() {
        let mut inv = invocation();
        inv.release = true;
        assert!(build_argv(&inv, &Wanted::Any).contains(&"--release".to_string()));
    }

    #[test]
    fn a_selector_becomes_what_is_wanted() {
        let sel = |kind: &str, name: &str| TargetSelector {
            kind: kind.to_string(),
            name: name.to_string(),
        };
        assert_eq!(Wanted::from_selector(&sel("bin", "geode")), Wanted::Bin("geode".into()));
        assert_eq!(Wanted::from_selector(&sel("example", "demo")), Wanted::Example("demo".into()));
        assert_eq!(Wanted::from_selector(&sel("test", "it")), Wanted::Test("it".into()));
        // A bench is a test binary as far as building and running one goes.
        assert_eq!(Wanted::from_selector(&sel("bench", "b")), Wanted::Test("b".into()));
        // The plural kinds build several things: which one to debug is not settled here.
        assert_eq!(Wanted::from_selector(&sel("bins", "")), Wanted::Any);
        assert_eq!(Wanted::from_selector(&sel("", "")), Wanted::Any);
        // An empty name is not a name: it would build `--bin ` and cargo would refuse.
        assert_eq!(Wanted::from_selector(&sel("bin", "")), Wanted::Any);
    }
}

// ── the handler ────────────────────────────────────────────────────────────────

/// Args for [`bennu_cargo_debug`].
#[derive(serde::Deserialize)]
pub struct CargoDebugArgs {
    pub root: String,
    /// The same invocation the Run button uses — so debugging is the run configuration you already
    /// have, with a debugger attached, rather than a second configuration to keep in step.
    pub invocation: Invocation,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,
    /// Stop before the first line of `main`. Off by default: the useful stop is the first breakpoint.
    #[serde(default)]
    pub stop_on_entry: bool,
}

/// Build a cargo target and debug it.
///
/// Blocking, and deliberately: the build is the slow part and the caller is a button that should stay
/// busy until there is either a session or a reason there is not. The frontend already shows a spinner
/// on the Debug action for the JVM launch, which is the same wait.
#[arbor_rpc::handler]
fn bennu_cargo_debug(
    ctx: &bennu_core::prelude::BennuState,
    args: CargoDebugArgs,
) -> Result<bennu_proto::prelude::RunHandle, String> {
    let cwd = match args.working_dir.as_deref() {
        Some(d) if !d.trim().is_empty() => d.to_string(),
        _ => args.root.clone(),
    };
    let wanted = Wanted::from_selector(&args.invocation.target);
    let sink = ctx.event_sink();

    // The build's diagnostics go to the console under the run id the session will carry, so a failing
    // build reads as a failing build in the place the user is already looking.
    let run_id = format!("dbg-{}", std::process::id() as u64 + rand_suffix());
    let build_sink = std::sync::Arc::clone(&sink);
    let id_for_lines = run_id.clone();
    let exe = build_and_locate(&args.root, &cwd, &args.invocation, &wanted, |line| {
        build_sink.emit(
            "arbor://bennu/debug-output",
            serde_json::json!({ "session_id": id_for_lines, "category": "stderr", "text": line }),
        );
    })?;

    // Kept for the console's first line, which is the command you would copy into a terminal.
    let exe_for_display = exe.clone();
    let env: Vec<(String, String)> = args.env.unwrap_or_default().into_iter().collect();
    // A test harness needs its own arguments (a filter, `--nocapture`); a program takes the
    // invocation's post-`--` arguments. Both are already in the same field.
    let program_args = args.invocation.args.clone();

    let cfg = bennu_core::config::load();
    let pinned = Some(cfg.debug_adapter.trim()).filter(|s| !s.is_empty());
    let adapter_path = Some(cfg.debug_adapter_path.trim()).filter(|s| !s.is_empty());

    crate::debug_dap::start(
        run_id.clone(),
        args.root.clone(),
        exe,
        program_args,
        env,
        args.stop_on_entry,
        pinned,
        adapter_path,
        sink,
    )?;

    // `main_class` is the JVM's word for "what is running"; for a native session it is the target's
    // name, which is what the console tab titles itself with either way.
    Ok(bennu_proto::prelude::RunHandle {
        run_id,
        main_class: label_of(&args.invocation, &wanted),
        command: display_command(&args.invocation, &wanted, &exe_for_display),
        working_dir: cwd,
    })
}

/// A run id that does not collide with the one the last debug session used.
///
/// The pid alone is not enough: two sessions in one backend would share it.
fn rand_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// The console's first line: what actually ran, and under which adapter.
///
/// From the backend rather than reassembled by the console, for the same reason the JVM launch's is:
/// only the backend knows which binary cargo produced and where.
fn display_command(invocation: &Invocation, wanted: &Wanted, exe: &str) -> String {
    let build = build_argv(invocation, wanted).join(" ");
    format!("cargo {build}\n{exe}")
}

/// The console tab's title.
fn label_of(invocation: &Invocation, wanted: &Wanted) -> String {
    let what = match wanted {
        Wanted::Bin(name) | Wanted::Example(name) | Wanted::Test(name) => name.clone(),
        Wanted::Any if !invocation.package.is_empty() => invocation.package.clone(),
        Wanted::Any => "debug".to_string(),
    };
    format!("Debug {what}")
}
