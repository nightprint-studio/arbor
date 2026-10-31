//! `gradle` build domain — the build as the tool window lists it, and running a task in it.
//!
//! The Gradle counterpart of [`crate::maven_goals`] plus the three actions of [`crate::maven`], and
//! split the same way:
//!
//! | Handler | What it costs |
//! |---|---|
//! | `bennu_gradle_model` | a read of `settings.gradle` and every build script — no Gradle, no network |
//! | `bennu_gradle_task` | one Gradle, streaming into the Run console |
//! | `bennu_gradle_download` | one Gradle run that resolves every classpath (the only one that uses the network) |
//! | `bennu_gradle_reload` | drop the classpath cache and reindex |
//!
//! The model is cheap on purpose, for the reason the Maven one is: a window that had to start a JVM
//! to find out what it can offer would take seconds to draw its first row. What it shows is what the
//! build files *say*; Gradle is started when somebody presses something.
//!
//! ## Where a task runs
//!
//! Always from the build root, with the module as a project path (`:api:test`). Gradle is not Maven:
//! the directory is not what selects a project — the path is — and running `gradlew` from a
//! subdirectory finds the root's settings anyway. Pressing a task on the root row runs the bare task
//! name, which Gradle applies to every project that has it: exactly "the whole build".

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use arbor_process_ext::prelude::NoWindowExt;
use bennu_core::prelude::BennuState;
use bennu_gradle::prelude::{tasks_for, Task};
use bennu_proto::prelude::RunHandle;
use serde::{Deserialize, Serialize};

use crate::build::spawn_streamed;

/// Args naming a build root.
#[derive(Deserialize)]
pub struct GradleRootArgs {
    /// Absolute path to the build root — the directory holding `settings.gradle[.kts]`.
    pub root: String,
}

/// An applied or declared plugin, as a row.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GradlePluginRow {
    pub id: String,
    pub version: String,
    /// `false` for `apply false`.
    pub apply: bool,
    pub line: u32,
}

/// One project of the build, as a section of the tool window.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GradleModuleRow {
    /// `:` for the root, `:api`, `:web:client`.
    pub path: String,
    pub name: String,
    /// Directory relative to the build root, forward-slashed. Empty for the root.
    pub dir: String,
    /// Absolute path of its build script; empty for a project that has none (a container).
    pub build_file: String,
    /// `jar` / `war` / `platform`, or empty for a project that applies no JVM plugin.
    pub packaging: String,
    pub plugins: Vec<GradlePluginRow>,
    pub tasks: Vec<Task>,
}

/// A version catalog the build reads.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GradleCatalogRow {
    pub name: String,
    pub file: String,
    pub libraries: usize,
}

/// What the Gradle tool window is built from.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GradleModel {
    /// The launcher that would be used (the wrapper, else a `gradle` on the PATH), or empty when
    /// none was found — the panel's cue to say so once, instead of every press failing to spawn.
    pub launcher: String,
    /// The build uses its own `gradlew`.
    pub wrapper: bool,
    pub build_name: String,
    /// Root first, then the projects `settings.gradle` includes.
    pub modules: Vec<GradleModuleRow>,
    pub catalogs: Vec<GradleCatalogRow>,
    /// Absolute path of the settings script, when the build has one.
    pub settings_file: String,
}

fn forward(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn require_gradle_root(root: &Path, shown: &str) -> Result<(), String> {
    if bennu_gradle::prelude::is_gradle_root(root) {
        Ok(())
    } else {
        Err(format!("{shown} has no settings.gradle or build.gradle — it is not a Gradle project"))
    }
}

/// The build, its projects, plugins and tasks — everything the tool window draws.
///
/// Runs no build tool and touches no network. A root with no Gradle script is an error rather than
/// an empty model, because a Gradle panel open on a project that is not one is a question worth
/// answering out loud.
#[arbor_rpc::handler]
pub(crate) fn bennu_gradle_model(_ctx: &BennuState, args: GradleRootArgs) -> Result<GradleModel, String> {
    let root = PathBuf::from(&args.root);
    require_gradle_root(&root, &args.root)?;
    Ok(model_of(&root))
}

fn model_of(root: &Path) -> GradleModel {
    let project = bennu_gradle::prelude::read(root);
    let jvm_build = project.is_jvm();
    let modules = project
        .modules
        .iter()
        .map(|m| GradleModuleRow {
            path: m.path.clone(),
            name: m.name.clone(),
            dir: m.rel_dir.clone(),
            build_file: m.build_file.as_deref().map(forward).unwrap_or_default(),
            packaging: m.packaging().to_string(),
            plugins: m
                .info
                .plugins
                .iter()
                .map(|p| GradlePluginRow { id: p.id.clone(), version: p.version.clone(), apply: p.apply, line: p.line })
                .collect(),
            tasks: tasks_for(&m.info, m.is_jvm(), jvm_build),
        })
        .collect();
    let catalogs = project
        .catalogs
        .iter()
        .map(|c| GradleCatalogRow { name: c.name.clone(), file: c.file.clone(), libraries: c.libraries.len() })
        .collect();
    let launcher = find_gradle_launcher(root);
    GradleModel {
        launcher: if launcher_exists(&launcher) { launcher } else { String::new() },
        wrapper: project.has_wrapper,
        build_name: project.name.clone(),
        modules,
        catalogs,
        settings_file: project.settings_file.as_deref().map(forward).unwrap_or_default(),
    }
}

// ── launcher ───────────────────────────────────────────────────────────────────

/// The Gradle launcher for `root`, as an absolute path where one can be found.
///
/// Opposite precedence to [`crate::dep_classpath::find_mvn_launcher`], on purpose: a Gradle build
/// pins its own Gradle version in the wrapper and is only guaranteed to work with *that* one, while
/// a Maven build is written against whatever Maven is installed. So:
///   1. **the project's `gradlew`**, when it can be executed;
///   2. **`PATH`**;
///   3. **the usual install directories** — a desktop app launched from the Dock inherits a minimal
///      environment, not the shell profile, so a Homebrew or SDKMAN Gradle is invisible to `PATH`;
///   4. the bare `gradle`, letting the child process resolve it.
pub(crate) fn find_gradle_launcher(root: &Path) -> String {
    let wrapper = root.join(if cfg!(windows) { "gradlew.bat" } else { "gradlew" });
    if wrapper.is_file() && is_runnable(&wrapper) {
        return wrapper.display().to_string();
    }
    let names: &[&str] =
        if cfg!(windows) { &["gradle.bat", "gradle.cmd", "gradle.exe", "gradle"] } else { &["gradle"] };
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            if let Some(hit) = crate::dep_classpath::first_launcher(&dir, names) {
                return hit;
            }
        }
    }
    for dir in gradle_bin_dirs() {
        if let Some(hit) = crate::dep_classpath::first_launcher(&dir, names) {
            return hit;
        }
    }
    "gradle".to_string()
}

/// A wrapper that lost its executable bit (a zip checkout, a Windows-authored repo) cannot be
/// spawned directly; better to fall through to an installed Gradle than to fail on `EACCES`.
#[cfg(unix)]
fn is_runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(not(unix))]
fn is_runnable(_path: &Path) -> bool {
    true
}

/// Whether the launcher string names something that exists, rather than the bare fallback.
fn launcher_exists(launcher: &str) -> bool {
    launcher != "gradle" || which_on_path("gradle")
}

fn which_on_path(name: &str) -> bool {
    std::env::var("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file() || d.join(format!("{name}.bat")).is_file()))
        .unwrap_or(false)
}

fn gradle_bin_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Ok(v) = std::env::var("GRADLE_HOME") {
        if !v.is_empty() {
            out.push(PathBuf::from(v).join("bin"));
        }
    }
    out.push(PathBuf::from("/opt/homebrew/bin"));
    out.push(PathBuf::from("/usr/local/bin"));
    out.push(PathBuf::from("/opt/local/bin"));
    out.push(PathBuf::from("/usr/share/gradle/bin"));
    out.push(PathBuf::from("/opt/gradle/bin"));
    if let Some(home) = bennu_classpath::prelude::user_home() {
        out.push(home.join(".sdkman/candidates/gradle/current/bin"));
    }
    out
}

// ── bennu_gradle_task ──────────────────────────────────────────────────────────

/// Args for [`bennu_gradle_task`].
#[derive(Deserialize)]
pub struct GradleTaskArgs {
    /// Absolute path to the build root.
    pub root: String,
    /// The project to run in, as a Gradle path (`:api`). Empty or `:` = the root, i.e. the whole
    /// build.
    #[serde(default)]
    pub module: String,
    /// What to run, in order — task names exactly as they go on the command line. Everything that
    /// is *not* a task (offline, skipping tests) is a field of its own: see [`task_argv`].
    pub tasks: Vec<String>,
    /// `--offline`.
    #[serde(default)]
    pub offline: bool,
    /// `-x test`.
    #[serde(default)]
    pub skip_tests: bool,
}

/// Run one or more Gradle tasks, streaming into the Run console.
///
/// Returns immediately with the [`RunHandle`] the console correlates by; Stop and stdin work on it
/// unchanged, because it goes through the same [`spawn_streamed`] a JVM launch does.
#[arbor_rpc::handler]
pub(crate) fn bennu_gradle_task(ctx: &BennuState, args: GradleTaskArgs) -> Result<RunHandle, String> {
    let root = PathBuf::from(&args.root);
    require_gradle_root(&root, &args.root)?;
    let argv = task_argv(&args)?;

    let launcher = find_gradle_launcher(&root);
    let mut cmd = Command::new(&launcher);
    cmd.current_dir(&root);
    for a in &argv {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::piped());
    cmd.no_window();

    let label = run_label(&args);
    let command = format!("{launcher} {}", argv.join(" "));
    spawn_streamed(cmd, label, command, forward(&root), &args.root, ctx.event_sink(), |_| {})
        .map_err(|e| format!("spawn gradle ({launcher}): {e}"))
}

/// The module as a project-path prefix, refusing anything that is not one.
///
/// The check is not ceremony: `module` arrives from a caller as a string and ends up in an argv
/// slot, where `--init-script` or `-p` would be a different build entirely.
pub(crate) fn module_prefix(module: &str) -> Result<String, String> {
    let m = module.trim();
    if m.is_empty() || m == ":" {
        return Ok(String::new());
    }
    let valid = m.starts_with(':')
        && m.split(':').skip(1).all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && seg.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
        });
    if valid {
        Ok(m.to_string())
    } else {
        Err(format!("`{m}` is not a project path of this build"))
    }
}

/// The whole command line after the launcher.
///
/// Flags are assembled **here** and nowhere else, which is why `tasks` refuses anything starting
/// with `-`: a caller that could smuggle a flag through the task list would be choosing this
/// build's options, and the panel would be showing one command while running another.
fn task_argv(args: &GradleTaskArgs) -> Result<Vec<String>, String> {
    let tasks: Vec<&str> = args.tasks.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).collect();
    if tasks.is_empty() {
        return Err("nothing to run — no task was named".to_string());
    }
    if let Some(flag) = tasks.iter().find(|t| t.starts_with('-')) {
        return Err(format!("`{flag}` is a Gradle option, not a task"));
    }
    let prefix = module_prefix(&args.module)?;

    let mut argv = vec!["--console=plain".to_string()];
    if args.offline {
        argv.push("--offline".to_string());
    }
    if args.skip_tests {
        argv.push("-x".to_string());
        argv.push("test".to_string());
    }
    for task in tasks {
        // A task that is already a path (`:other:build`, `--tests` never gets here) is left alone.
        if prefix.is_empty() || task.starts_with(':') {
            argv.push(task.to_string());
        } else {
            argv.push(format!("{prefix}:{task}"));
        }
    }
    Ok(argv)
}

/// The console tab's title: what was run, and where.
fn run_label(args: &GradleTaskArgs) -> String {
    let tasks = args.tasks.join(" ");
    match args.module.trim().trim_matches(':') {
        "" => tasks,
        module => {
            let leaf = module.rsplit(':').next().unwrap_or(module);
            format!("{tasks} · {leaf}")
        }
    }
}

// ── download / reload ──────────────────────────────────────────────────────────

/// Resolve every classpath of the build with the network allowed, then rebuild the index.
///
/// The Gradle equivalent of `dependency:go-offline`: the classpath resolve *is* the download —
/// resolving a configuration is what fetches its artifacts. Returns immediately with `started`; the
/// work runs on a background thread and reports through the job panel.
#[arbor_rpc::handler]
pub(crate) fn bennu_gradle_download(ctx: &BennuState, args: GradleRootArgs) -> Result<String, String> {
    let root = PathBuf::from(&args.root);
    require_gradle_root(&root, &args.root)?;
    let host = ctx.host_caller().ok_or("no reverse channel to register the download job")?;
    let sink = ctx.event_sink();
    let root_str = args.root.clone();

    std::thread::spawn(move || {
        let job = crate::index_service::register_bennu_job(
            &host,
            &sink,
            "Download dependencies",
            "gradle --refresh-dependencies · resolve every classpath",
            "Download",
            false,
        );
        match crate::gradle_classpath::run_classpath(&root, true) {
            Ok(jars) => {
                crate::dep_classpath::clear_list_cache(&root);
                let indexed = crate::maven::reindex_and_wait(&root_str, &sink);
                crate::index_service::finish_bennu_job(
                    &sink,
                    job,
                    indexed,
                    (!indexed).then(|| "The index could not be rebuilt.".to_string()),
                );
                crate::index_service::notify(
                    &sink,
                    "Dependencies downloaded",
                    &format!(
                        "{} jar(s) on the classpath.{}",
                        jars.len(),
                        if indexed { " The index was rebuilt." } else { " The index could not be rebuilt." }
                    ),
                    if indexed { "success" } else { "warning" },
                );
            }
            Err(reason) => {
                crate::index_service::finish_bennu_job(&sink, job, false, Some(reason.clone()));
                crate::index_service::notify(&sink, "Download failed", &reason, "error");
            }
        }
    });
    Ok("started".to_string())
}

/// Drop the cached classpath and rebuild the index behind it.
///
/// The cache is keyed on build-script timestamps, so nothing the user can do short of editing a
/// script would otherwise make the index look again — and "the editor and my Gradle cache disagree"
/// is exactly when somebody reaches for this.
#[arbor_rpc::handler]
pub(crate) fn bennu_gradle_reload(ctx: &BennuState, args: GradleRootArgs) -> Result<String, String> {
    let root = PathBuf::from(&args.root);
    require_gradle_root(&root, &args.root)?;
    let host = ctx.host_caller();
    let sink = ctx.event_sink();
    let root_str = args.root.clone();
    std::thread::spawn(move || {
        let job = host.as_ref().and_then(|host| {
            crate::index_service::register_bennu_job(
                host,
                &sink,
                "Re-resolve dependencies",
                "drop the classpath cache · reindex",
                "Dependencies",
                false,
            )
        });
        crate::dep_classpath::clear_list_cache(&root);
        let indexed = crate::maven::reindex_and_wait(&root_str, &sink);
        crate::index_service::finish_bennu_job(
            &sink,
            job,
            indexed,
            (!indexed).then(|| "The index could not be rebuilt.".to_string()),
        );
        crate::index_service::notify(
            &sink,
            "Dependencies re-resolved",
            if indexed { "The index was rebuilt." } else { "The index could not be rebuilt." },
            if indexed { "success" } else { "warning" },
        );
    });
    Ok("started".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(tasks: &[&str]) -> GradleTaskArgs {
        GradleTaskArgs {
            root: "/p".into(),
            module: String::new(),
            tasks: tasks.iter().map(|t| (*t).to_string()).collect(),
            offline: false,
            skip_tests: false,
        }
    }

    #[test]
    fn the_plain_case_is_plain_console_and_the_tasks() {
        assert_eq!(task_argv(&args(&["clean", "build"])).unwrap(), ["--console=plain", "clean", "build"]);
    }

    #[test]
    fn a_module_qualifies_every_task_and_the_toggles_come_first() {
        let mut a = args(&["test", ":other:check"]);
        a.module = ":api".into();
        a.offline = true;
        a.skip_tests = true;
        assert_eq!(
            task_argv(&a).unwrap(),
            ["--console=plain", "--offline", "-x", "test", ":api:test", ":other:check"]
        );
    }

    /// The one thing a caller must not be able to do: choose this build's options through the task
    /// list or the module.
    #[test]
    fn a_flag_smuggled_in_as_a_task_or_module_is_refused() {
        assert!(task_argv(&args(&["--init-script=/tmp/evil.gradle"])).is_err());
        assert!(task_argv(&args(&[])).is_err());
        let mut a = args(&["build"]);
        a.module = "--init-script".into();
        assert!(task_argv(&a).is_err());
        a.module = ":api:../x".into();
        assert!(task_argv(&a).is_err());
    }

    #[test]
    fn the_tab_says_which_project_it_was() {
        let mut a = args(&["test"]);
        assert_eq!(run_label(&a), "test");
        a.module = ":services:orders".into();
        assert_eq!(run_label(&a), "test · orders");
    }
}
