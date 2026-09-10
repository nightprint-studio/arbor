//! `maven` build domain — the reactor as the tool window lists it, and running a goal in it.
//!
//! Two handlers, and the split between them is the point:
//!
//! | Handler | What it costs |
//! |---|---|
//! | `bennu_maven_model` | a parse of every pom in the reactor — no Maven, no network |
//! | `bennu_maven_goal` | one Maven, streaming into the Run console |
//!
//! The model is cheap on purpose. A tool window that had to start Maven to find out what it could
//! offer would take seconds to draw its first row, on a project that has never been built, over
//! and over — so the panel is built out of what the poms *say*, and Maven is started only when
//! somebody presses something.
//!
//! ## Where a goal runs
//!
//! In the module's own directory. `mvn install` pressed on a module means that module, and
//! invoking it at the root with `-pl` would be a different build with a different reactor — one
//! that also rebuilds the module's dependencies, which is a decision the panel has no business
//! taking on somebody's behalf. Pressing it on the root row runs the whole reactor, because that
//! is what the root pom's build is.
//!
//! Nothing here is offline by default, and that is the opposite of [`crate::build`]'s compile:
//! that one exists to be instant and runs a goal Maven can always satisfy from `~/.m2`, while
//! `install` on a cold repository legitimately needs to fetch the plugin that performs it. A
//! deliberate press may use the network; the button that runs behind your typing may not.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use arbor_process_ext::prelude::NoWindowExt;
use bennu_core::prelude::BennuState;
use bennu_maven::prelude::{
    pom_plugins, pom_profiles, reactor, Phase, PomPlugin, PomProfile, LIFECYCLE,
};
use bennu_proto::prelude::RunHandle;
use serde::{Deserialize, Serialize};

use crate::build::spawn_streamed;

/// Args naming a project root.
#[derive(Deserialize)]
pub struct MavenModelArgs {
    /// Absolute path to the project root — the directory holding the root `pom.xml`.
    pub root: String,
}

/// One module of the reactor, as a row of the tool window.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MavenModule {
    /// Directory relative to the project root, forward-slashed. Empty for the root module itself.
    pub dir: String,
    pub artifact_id: String,
    /// `<name>` when the pom gives one — what a person calls this module, which is often not what
    /// the build calls it. Empty otherwise, and then the artifactId is the name.
    pub name: String,
    /// `jar` / `war` / `pom` / …, as written. Empty when the pom does not say, which means `jar`.
    pub packaging: String,
    /// Absolute path of its pom, forward-slashed.
    pub pom: String,
    /// The plugins this pom configures — see `bennu_maven::goals`.
    pub plugins: Vec<PomPlugin>,
}

/// What the Maven tool window is built from.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MavenModel {
    /// The Maven launcher that would be used, or empty when none was found — which is the panel's
    /// cue to say so once, instead of every press failing to spawn with no explanation.
    pub launcher: String,
    /// The reactor, root first, in the order the poms declare their modules.
    pub modules: Vec<MavenModule>,
    /// Every profile declared anywhere in the reactor, deduplicated by id.
    pub profiles: Vec<PomProfile>,
    /// Maven's lifecycle phases, in run order. Served from here rather than hard-coded in the
    /// panel so the words and the command line can never disagree.
    pub lifecycle: Vec<Phase>,
}

/// The reactor, its plugins and its profiles — everything the tool window draws.
///
/// Runs no build tool and touches no network: it reads `pom.xml` files. A root with no pom is an
/// error rather than an empty model, because a Maven panel open on a project that is not one is a
/// question worth answering out loud.
#[arbor_rpc::handler]
pub(crate) fn bennu_maven_model(
    _ctx: &BennuState,
    args: MavenModelArgs,
) -> Result<MavenModel, String> {
    let root = PathBuf::from(&args.root);
    if !root.join("pom.xml").is_file() {
        return Err(format!("{} has no pom.xml — it is not a Maven project", args.root));
    }

    let mut modules = Vec::new();
    let mut profiles: Vec<PomProfile> = Vec::new();
    for (dir, pom) in reactor(&root) {
        let rel = relative(&root, &dir);
        let path = dir.join("pom.xml");
        let source = std::fs::read(&path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        for mut profile in pom_profiles(&source) {
            if profiles.iter().any(|p: &PomProfile| p.id == profile.id) {
                continue;
            }
            profile.module = rel.clone();
            profiles.push(profile);
        }
        modules.push(MavenModule {
            dir: rel,
            artifact_id: pom.artifact_id.clone(),
            name: pom.name.clone(),
            packaging: pom.packaging.clone(),
            pom: forward(&path),
            plugins: pom_plugins(&source),
        });
    }

    Ok(MavenModel {
        launcher: crate::build::resolve_mvn(&root),
        modules,
        profiles,
        lifecycle: LIFECYCLE.to_vec(),
    })
}

/// `dir` relative to `root`, forward-slashed. Empty for the root itself.
fn relative(root: &Path, dir: &Path) -> String {
    dir.strip_prefix(root).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default()
}

fn forward(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

// ── bennu_maven_goal ───────────────────────────────────────────────────────────

/// Args for [`bennu_maven_goal`].
#[derive(Deserialize)]
pub struct MavenGoalArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// The module to run in, relative to the root. Empty = the root, which is the whole reactor.
    #[serde(default)]
    pub module: String,
    /// What to run, in order — lifecycle phases (`clean`, `install`) and plugin goals
    /// (`compiler:compile`) alike, exactly as they go on the command line. Everything that is
    /// *not* a goal (profiles, offline, skipping tests) is a field of its own: see
    /// [`goal_argv`].
    pub goals: Vec<String>,
    /// Profile ids to activate — `-P a,b`.
    #[serde(default)]
    pub profiles: Vec<String>,
    /// `-DskipTests`. The one toggle worth a checkbox: it is what the same goal is run with and
    /// without, several times an hour.
    #[serde(default)]
    pub skip_tests: bool,
    /// `-o`. Off by default — see the module doc.
    #[serde(default)]
    pub offline: bool,
}

/// Run one or more Maven goals, streaming into the Run console.
///
/// Returns immediately with the [`RunHandle`] the console correlates by; the child runs on a
/// background thread, and Stop / stdin work on it unchanged, because it goes through the same
/// [`spawn_streamed`] a JVM launch does.
#[arbor_rpc::handler]
pub(crate) fn bennu_maven_goal(ctx: &BennuState, args: MavenGoalArgs) -> Result<RunHandle, String> {
    let root = PathBuf::from(&args.root);
    let cwd = module_dir(&root, &args.module)?;
    if !cwd.join("pom.xml").is_file() {
        return Err(format!("{} has no pom.xml", forward(&cwd)));
    }
    let argv = goal_argv(&args)?;

    let mvn = crate::build::resolve_mvn(&root);
    let mut cmd = Command::new(&mvn);
    cmd.current_dir(&cwd);
    for a in &argv {
        cmd.arg(a);
    }
    // The project's own JDK, resolved the same way the compile resolves it. `None` leaves the
    // ambient one, which is what running `mvn` in a terminal would have used.
    if let Some(home) = crate::build::resolve_java_home(&args.root) {
        cmd.env("JAVA_HOME", home);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::piped());
    cmd.no_window();

    let label = run_label(&args);
    let command = format!("{mvn} {}", argv.join(" "));
    spawn_streamed(
        cmd,
        label,
        command,
        forward(&cwd),
        &args.root,
        ctx.event_sink(),
        |_| {},
    )
    .map_err(|e| format!("spawn mvn ({mvn}): {e}"))
}

/// The directory to invoke Maven in, refusing anything that leaves the project.
///
/// The check is not ceremony: `module` arrives from a caller as a string, and a `..` in it would
/// run a build in a directory the user never opened, with whatever that pom's plugins do.
fn module_dir(root: &Path, module: &str) -> Result<PathBuf, String> {
    let module = module.trim().trim_matches('/');
    if module.is_empty() {
        return Ok(root.to_path_buf());
    }
    if module.split(['/', '\\']).any(|part| part == ".." || part.is_empty()) {
        return Err(format!("`{module}` is not a module of this project"));
    }
    Ok(root.join(module))
}

/// The whole command line after the launcher.
///
/// Flags are assembled **here** and nowhere else, which is why `goals` refuses anything starting
/// with `-`: a caller that could smuggle a flag through the goal list would be choosing this
/// build's `-D` properties, and the panel would be showing one command while running another.
fn goal_argv(args: &MavenGoalArgs) -> Result<Vec<String>, String> {
    let goals: Vec<&str> = args.goals.iter().map(|g| g.trim()).filter(|g| !g.is_empty()).collect();
    if goals.is_empty() {
        return Err("nothing to run — no goal was named".to_string());
    }
    if let Some(flag) = goals.iter().find(|g| g.starts_with('-')) {
        return Err(format!("`{flag}` is a Maven option, not a goal"));
    }

    let mut argv = vec!["--batch-mode".to_string()];
    if args.offline {
        argv.push("-o".to_string());
    }
    let profiles: Vec<&str> =
        args.profiles.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    if !profiles.is_empty() {
        argv.push("-P".to_string());
        argv.push(profiles.join(","));
    }
    if args.skip_tests {
        argv.push("-DskipTests".to_string());
    }
    argv.extend(goals.iter().map(|g| (*g).to_string()));
    Ok(argv)
}

/// The console tab's title: what was run, and where.
///
/// The module is in it because `install` on two modules is two different builds, and a tab strip
/// that called both "install" is unreadable from the second one on.
fn run_label(args: &MavenGoalArgs) -> String {
    let goals = args.goals.join(" ");
    match args.module.trim().trim_matches('/') {
        "" => goals,
        module => {
            let leaf = module.rsplit('/').next().unwrap_or(module);
            format!("{goals} · {leaf}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(goals: &[&str]) -> MavenGoalArgs {
        MavenGoalArgs {
            root: "/p".into(),
            module: String::new(),
            goals: goals.iter().map(|g| (*g).to_string()).collect(),
            profiles: Vec::new(),
            skip_tests: false,
            offline: false,
        }
    }

    #[test]
    fn the_plain_case_is_batch_mode_and_the_goals() {
        assert_eq!(goal_argv(&args(&["clean", "install"])).unwrap(), ["--batch-mode", "clean", "install"]);
    }

    #[test]
    fn the_toggles_land_before_the_goals_where_maven_wants_them() {
        let mut a = args(&["test"]);
        a.offline = true;
        a.skip_tests = true;
        a.profiles = vec!["ci".into(), " ".into(), "nightly".into()];
        assert_eq!(
            goal_argv(&a).unwrap(),
            ["--batch-mode", "-o", "-P", "ci,nightly", "-DskipTests", "test"]
        );
    }

    /// The one thing a caller must not be able to do: choose this build's options through the
    /// list of things to run.
    #[test]
    fn a_flag_smuggled_in_as_a_goal_is_refused() {
        assert!(goal_argv(&args(&["-Dmaven.repo.local=/tmp/evil"])).is_err());
        assert!(goal_argv(&args(&[])).is_err());
        assert!(goal_argv(&args(&["  "])).is_err());
    }

    #[test]
    fn a_module_that_climbs_out_of_the_project_is_refused() {
        let root = Path::new("/p");
        assert_eq!(module_dir(root, "").unwrap(), root);
        assert_eq!(module_dir(root, "core/api").unwrap(), root.join("core/api"));
        assert!(module_dir(root, "../elsewhere").is_err());
        assert!(module_dir(root, "core/../../elsewhere").is_err());
    }

    /// Two modules running the same phase must not produce two tabs with the same name.
    #[test]
    fn the_tab_says_which_module_it_was() {
        let mut a = args(&["install"]);
        assert_eq!(run_label(&a), "install");
        a.module = "services/orders".into();
        assert_eq!(run_label(&a), "install · orders");
    }
}
