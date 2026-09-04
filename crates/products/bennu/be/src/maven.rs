//! `maven` domain — the local repository, as something the editor and a model can ask about, and
//! the one action that changes it.
//!
//! ## Why the reporting half exists at all
//!
//! The symptom of a dependency that was never downloaded is not "a dependency is missing". It is
//! every type from that jar reading as unresolvable, in source files that are correct, with nothing
//! anywhere naming the cause. [`bennu_maven_status`] is that name: which repository was consulted,
//! how much is in it, which coordinates this project needs and does not have.
//!
//! Everything here except [`bennu_maven_download`] is read-only and touches no process: the
//! resolution is the same one the index tier runs, and it reads poms and `stat`s files.
//!
//! ## And why the acting half is one verb
//!
//! Downloading is the only thing that can fix a missing artifact, it needs the network, and it is
//! slow — so it is a deliberate action with a job behind it, never something a status call does on
//! the way past.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arbor_process_ext::prelude::NoWindowExt;
use bennu_core::prelude::BennuState;
use bennu_maven::prelude::{Catalog, LocalRepo, Resolution};
use serde::{Deserialize, Serialize};

/// Args naming a project root.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MavenRootArgs {
    /// Absolute path to the project root (the directory holding the root `pom.xml`).
    pub root: String,
}

/// One artifact this project needs and the local repository does not have.
#[derive(Serialize, Default)]
pub struct MissingArtifact {
    /// `groupId:artifactId:version`, as a person reads it.
    pub coord: String,
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    /// Where it was looked for — the path a download would create.
    pub path: String,
    /// Whether the repository holds *some* version of it, which separates a wrong version from a
    /// coordinate nobody has ever fetched.
    pub other_versions: Vec<String>,
}

/// What the dependency tier is actually standing on.
#[derive(Serialize, Default)]
pub struct MavenStatus {
    /// The local repository in use — resolved from `settings.xml` / `-Dmaven.repo.local`, not
    /// assumed. The first thing to check when nothing resolves on a machine that builds fine.
    pub repository: String,
    pub repository_exists: bool,
    /// Distinct `groupId:artifactId` in it, and installed versions across them. Zero means the
    /// catalog has not been scanned yet, not that the repository is empty.
    pub artifacts: usize,
    pub versions: usize,
    /// Whether a Maven launcher was found, and which one.
    pub maven: String,
    /// Jars the offline resolve produced — what the index would get with no Maven run at all.
    pub resolved_jars: usize,
    /// The project's own modules, which are built from source and never looked for in a repository.
    pub modules: Vec<String>,
    pub missing: Vec<MissingArtifact>,
    /// Declared dependencies whose version nothing on disk answers — an undefined `${property}`, a
    /// version range, a BOM that is itself missing. A download will not help these.
    pub unversioned: Vec<String>,
    /// One line summarising the shortfall, empty when everything resolved.
    pub shortfall: String,
}

/// Where this project's dependencies come from, and which of them are not there.
///
/// Runs no build tool and touches no network: it reads the project's poms and looks in the local
/// repository, which is the same reading the index tier does.
#[arbor_rpc::handler(mcp(
    name = "bennu_maven_status",
    title = "See which dependencies are missing from the local repository",
    safety = read,
    description = "Report a Maven project's dependency resolution against the local repository \
(~/.m2, or wherever settings.xml points): which repository is in use, how many artifacts it holds, \
how many jars this project resolves to, and — the reason to call it — exactly which coordinates the \
project needs and the repository does not have. Call this when Java types from a library read as \
unresolvable, or when a project reports errors that look unrelated to the code in front of you: a \
dependency that was never downloaded makes every type in that jar unresolvable at once, and this \
names it. `unversioned` is the separate case where no version can be determined at all (an \
undefined ${property}, a version range) — downloading will not fix those. Nothing here downloads \
anything.",
))]
pub(crate) fn bennu_maven_status(_ctx: &BennuState, args: MavenRootArgs) -> Result<MavenStatus, String> {
    let root = PathBuf::from(&args.root);
    if !root.join("pom.xml").is_file() {
        return Err(format!("{} has no pom.xml — it is not a Maven project", args.root));
    }
    let repo = LocalRepo::discover();
    let resolution = bennu_maven::prelude::resolve_offline(&root, &repo);
    let catalog = Catalog::cached(&repo).unwrap_or_default();
    Ok(MavenStatus {
        repository: repo.root().to_string_lossy().replace('\\', "/"),
        repository_exists: repo.exists(),
        artifacts: catalog.len(),
        versions: catalog.version_count(),
        maven: crate::dep_classpath::find_mvn_launcher(&root),
        resolved_jars: resolution.jars.len(),
        modules: resolution.reactor.clone(),
        missing: missing_rows(&repo, &resolution),
        unversioned: resolution.unversioned.iter().map(|c| c.gav()).collect(),
        shortfall: resolution.shortfall().unwrap_or_default(),
    })
}

fn missing_rows(repo: &LocalRepo, resolution: &Resolution) -> Vec<MissingArtifact> {
    resolution
        .missing
        .iter()
        .map(|c| MissingArtifact {
            coord: c.gav(),
            group_id: c.group_id.clone(),
            artifact_id: c.artifact_id.clone(),
            version: c.version.clone(),
            path: repo.artifact_file(c).to_string_lossy().replace('\\', "/"),
            other_versions: repo.versions(&c.group_id, &c.artifact_id).into_iter().take(6).collect(),
        })
        .collect()
}

/// Args for [`bennu_maven_search`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MavenSearchArgs {
    /// What to look for, matched against `groupId` and `artifactId`.
    pub query: String,
    /// `true` to search build plugins instead of libraries — only affects the built-in table, since
    /// the repository holds both.
    #[serde(default)]
    pub plugins: bool,
}

/// One coordinate a search turned up.
#[derive(Serialize, Default)]
pub struct MavenHit {
    pub group_id: String,
    pub artifact_id: String,
    /// Installed versions, newest first. Empty for a coordinate only the built-in table knows.
    pub versions: Vec<String>,
    /// What it is, when the built-in table says.
    pub description: String,
    /// Whether it is in the local repository — a coordinate that is not still resolves on a machine
    /// with a network, it just is not free.
    pub installed: bool,
}

/// Search for a dependency coordinate: the local repository first, then the built-in table.
///
/// The completion popup in a pom answers from the same two sources; this is the same question asked
/// without a caret, for an "add dependency" flow or a model that wants the coordinate before it
/// writes one.
#[arbor_rpc::handler]
pub(crate) fn bennu_maven_search(_ctx: &BennuState, args: MavenSearchArgs) -> Result<Vec<MavenHit>, String> {
    /// A search result list, not a database dump.
    const LIMIT: usize = 40;
    let repo = LocalRepo::discover();
    let catalog = Catalog::cached(&repo).unwrap_or_else(|| Catalog::ensure(&repo));
    let mut out: Vec<MavenHit> = catalog
        .search(&args.query, LIMIT)
        .into_iter()
        .map(|a| MavenHit {
            group_id: a.group_id.clone(),
            artifact_id: a.artifact_id.clone(),
            versions: a.versions.iter().take(8).cloned().collect(),
            description: bennu_maven::prelude::describe_coordinate(&a.group_id, &a.artifact_id)
                .unwrap_or_default()
                .to_string(),
            installed: true,
        })
        .collect();

    let query = args.query.to_ascii_lowercase();
    let table = if args.plugins { bennu_maven::prelude::PLUGINS } else { bennu_maven::prelude::LIBRARIES };
    for (group, artifact, description) in table {
        if out.len() >= LIMIT {
            break;
        }
        if !artifact.to_ascii_lowercase().contains(&query) && !group.to_ascii_lowercase().contains(&query) {
            continue;
        }
        if out.iter().any(|h| h.group_id == *group && h.artifact_id == *artifact) {
            continue;
        }
        out.push(MavenHit {
            group_id: group.to_string(),
            artifact_id: artifact.to_string(),
            versions: Vec::new(),
            description: description.to_string(),
            installed: false,
        });
    }
    Ok(out)
}

/// Rescan the local repository.
///
/// The catalog is rebuilt on a timer, which is right for a repository that grows by a jar a week
/// and wrong for the minute after a build downloads forty. This is the button for that minute.
#[arbor_rpc::handler]
pub(crate) fn bennu_maven_refresh(_ctx: &BennuState, _args: EmptyArgs) -> Result<usize, String> {
    let repo = LocalRepo::discover();
    let catalog = Catalog::scan(&repo);
    catalog.save();
    Ok(catalog.len())
}

/// No arguments. A named type rather than `()` so the wire shape stays an object.
#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct EmptyArgs {}

/// Download whatever this project's dependencies need, then rebuild the index.
///
/// The **only** thing here that uses the network, and the only thing that can fix a missing
/// artifact. `dependency:go-offline` is the goal that exists for exactly this — "fetch everything
/// this project would need to build with no network" — and `-U` makes it retry the artifacts a
/// previous failed attempt recorded, which is otherwise the reason a second run resolves nothing
/// and says nothing.
///
/// Returns immediately with the job id; the work runs on a background thread and reports through
/// the job panel, because it is minutes on a cold repository.
#[arbor_rpc::handler]
pub(crate) fn bennu_maven_download(ctx: &BennuState, args: MavenRootArgs) -> Result<String, String> {
    let root = PathBuf::from(&args.root);
    if !root.join("pom.xml").is_file() {
        return Err(format!("{} has no pom.xml — it is not a Maven project", args.root));
    }
    let host = ctx.host_caller().ok_or("no reverse channel to register the download job")?;
    let sink = ctx.event_sink();
    let root_str = args.root.clone();

    std::thread::spawn(move || {
        let job = crate::index_service::register_bennu_job(
            &host,
            &sink,
            "Download dependencies",
            "mvn -U dependency:go-offline",
            "Download",
            false,
        );
        let mvn = crate::dep_classpath::find_mvn_launcher(&root);
        let outcome = run_go_offline(&root, &mvn);
        match outcome {
            Ok((true, _)) => {
                // The list cache is keyed on what was missing; the artifacts have moved, so drop it
                // and let the index rebuild resolve from scratch.
                crate::dep_classpath::clear_list_cache(&root);
                Catalog::scan(&LocalRepo::discover()).save();
                crate::index_service::finish_bennu_job(&sink, job, true, None);
                crate::index_service::notify(
                    &sink,
                    "Dependencies downloaded",
                    "Rebuilding the index so library types resolve.",
                    "success",
                );
                crate::index_service::IndexService::global().reindex(&root_str, Arc::clone(&sink));
            }
            Ok((false, log)) => {
                let reason = go_offline_failure(&log);
                crate::index_service::finish_bennu_job(&sink, job, false, Some(reason.clone()));
                crate::index_service::notify(&sink, "Download failed", &reason, "error");
            }
            Err(e) => {
                crate::index_service::finish_bennu_job(&sink, job, false, Some(e.clone()));
                crate::index_service::notify(&sink, "Download failed", &e, "error");
            }
        }
    });
    Ok("started".to_string())
}

/// Re-resolve the project's dependencies from scratch, and rebuild the index behind them.
///
/// Two things that are always wanted together and were two separate gestures. Dropping the
/// classpath cache without reindexing leaves the editor answering from the old jars; reindexing
/// without dropping it faithfully re-serves the same wrong classpath, because the cache is keyed on
/// pom timestamps and nothing the user can do short of editing a pom moves those.
///
/// The repository catalogue is rescanned in the same breath: it is refreshed on a timer, which is
/// right for a repository that grows by a jar a week and wrong for the minute after a build
/// downloaded forty — which is exactly the minute somebody presses this.
#[arbor_rpc::handler]
pub(crate) fn bennu_maven_reload(ctx: &BennuState, args: MavenRootArgs) -> Result<String, String> {
    let root = PathBuf::from(&args.root);
    if !root.join("pom.xml").is_file() {
        return Err(format!("{} has no pom.xml — it is not a Maven project", args.root));
    }
    let sink = ctx.event_sink();
    let root_str = args.root.clone();
    // Off the dispatcher: the catalogue walk is seconds on a cold repository and the reindex is a
    // whole-project read. Neither belongs on the thread the editor is waiting on.
    std::thread::spawn(move || {
        crate::dep_classpath::clear_list_cache(&root);
        Catalog::scan(&LocalRepo::discover()).save();
        crate::index_service::IndexService::global().reindex(&root_str, Arc::clone(&sink));
    });
    Ok("started".to_string())
}

/// Download the `-sources.jar` of every dependency, so Ctrl+B into a library lands on real source
/// instead of a decompiled stub.
///
/// Project-wide and deliberate. The per-class download exists too — it is what the decompiled tab
/// offers — but doing it one class at a time is a Maven start per jar, and somebody who wants to
/// read a library wants the next one as well. Missing sources are not an error: plenty of artifacts
/// publish none, and `dependency:sources` reports that per artifact and carries on.
#[arbor_rpc::handler]
pub(crate) fn bennu_maven_download_sources(
    ctx: &BennuState,
    args: MavenRootArgs,
) -> Result<String, String> {
    let root = PathBuf::from(&args.root);
    if !root.join("pom.xml").is_file() {
        return Err(format!("{} has no pom.xml — it is not a Maven project", args.root));
    }
    let host = ctx.host_caller().ok_or("no reverse channel to register the download job")?;
    let sink = ctx.event_sink();
    let root_str = args.root.clone();

    std::thread::spawn(move || {
        let job = crate::index_service::register_bennu_job(
            &host,
            &sink,
            "Download dependency sources",
            "mvn dependency:sources",
            "Download",
            false,
        );
        let mvn = crate::dep_classpath::find_mvn_launcher(&root);
        match run_mvn(&root, &mvn, &["dependency:sources"]) {
            Ok((_, log)) => {
                // The exit code is not the answer here and `--fail-never` makes sure of it: an
                // artifact with no published sources is a normal outcome, not a failed run. What is
                // worth reporting is how many arrived.
                let attached = log.matches("Resolving: ").count();
                crate::index_service::finish_bennu_job(&sink, job, true, None);
                crate::index_service::notify(
                    &sink,
                    "Sources downloaded",
                    &match attached {
                        0 => "No new sources jars were available.".to_string(),
                        n => format!("{n} artifacts checked — sources attached where published."),
                    },
                    "success",
                );
                // The resolver caches which jars have sources beside them; without this the tabs
                // opened before the download keep showing their decompiled stubs.
                crate::index_service::IndexService::global().refresh_dep_sources(&root_str);
            }
            Err(e) => {
                crate::index_service::finish_bennu_job(&sink, job, false, Some(e.clone()));
                crate::index_service::notify(&sink, "Download sources failed", &e, "error");
            }
        }
    });
    Ok("started".to_string())
}

/// `mvn -U dependency:go-offline` in the project. Returns whether it got everything, plus its output.
fn run_go_offline(root: &Path, mvn: &str) -> Result<(bool, String), String> {
    let (ok, log) = run_mvn(root, mvn, &["-U", "dependency:go-offline"])?;
    // `--fail-never` means the exit code says nothing; the log does. An artifact that could not be
    // resolved is the failure worth reporting, and it is reported by name.
    let resolved_everything =
        !log.contains("Could not resolve") && !log.contains("Could not find artifact");
    Ok((ok && resolved_everything, log))
}

/// Run Maven in the project directory and merge its output.
///
/// Always `--batch-mode` and always `--fail-never`: one unresolvable artifact must not stop the
/// other thirty from being fetched, because the whole point of every goal reached from here is to
/// come back with as much as can be had. Which means the **exit code is not the answer** — each
/// caller reads the log for the one it is asking about.
fn run_mvn(root: &Path, mvn: &str, goals: &[&str]) -> Result<(bool, String), String> {
    let mut cmd = std::process::Command::new(mvn);
    cmd.current_dir(root).arg("--batch-mode").arg("--fail-never");
    for goal in goals {
        cmd.arg(goal);
    }
    let output = cmd.no_window().output().map_err(|e| format!("could not run `{mvn}`: {e}"))?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok((output.status.success(), log))
}

/// The line of a `go-offline` log that says what could not be had.
fn go_offline_failure(log: &str) -> String {
    for line in log.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("[ERROR]") {
            let rest = rest.trim();
            if !rest.is_empty() {
                return format!("Maven said: {rest}");
            }
        }
    }
    if log.contains("Could not find artifact") {
        return "Some artifacts are not published in the repositories this project is configured with."
            .to_string();
    }
    "Maven could not download everything (see the job output).".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--fail-never` makes the exit code meaningless, so the log is what decides. Reading only the
    /// status is how a download that fetched nothing reports success.
    #[test]
    fn an_unresolvable_artifact_is_read_off_the_log_and_named() {
        let log = "[ERROR] Failed to execute goal: Could not resolve dependencies for project x";
        assert!(go_offline_failure(log).contains("Could not resolve"));
    }
}
