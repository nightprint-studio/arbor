//! `bennu_dependency_tree` — the resolved dependency graph, asked of Maven (or, for a Cargo
//! workspace, of `cargo tree`).
//!
//! [`crate::dependencies`] deliberately never runs the build tool: a panel that shells out when it
//! opens is a panel nobody opens twice. The questions *this* one answers — who brought a dependency
//! in, why there are two versions of it — cannot be answered from the poms (mediation, exclusions and
//! scopes depend on the whole reactor), so it is a separate call the panel makes **on request**, in
//! the background, and keeps the answer of until asked again.
//!
//! Runs `dependency:tree -Dverbose` first, because the nodes Maven drops are the other versions
//! somebody asked for; newer plugin versions dropped that flag's meaning, so an answer with no
//! modules is retried plain rather than reported as empty.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use bennu_core::prelude::BennuState;
use bennu_deps::prelude::{parse_tree_output, ModuleTree};
use serde::{Deserialize, Serialize};

use crate::dep_classpath::{auto_download, find_mvn_launcher};

/// A reactor resolve can take minutes on a cold cache; past this it is hung, not slow.
const TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Deserialize)]
pub struct DependencyTreeArgs {
    /// Absolute path to the project root (the dir holding the root `pom.xml`).
    pub root: String,
}

/// Every module's resolved tree.
#[derive(Debug, Clone, Serialize)]
pub struct DependencyTree {
    pub modules: Vec<ModuleTree>,
    /// Whether dropped nodes (other versions, duplicates) are in the answer.
    pub verbose: bool,
}

/// Run `mvn dependency:tree` for the project and return the resolved tree of each module.
///
/// `Err` carries Maven's own reason when it produced no tree at all — an offline resolve of
/// something never downloaded, a pom that does not build — because "empty" would be read as "no
/// dependencies".
#[arbor_rpc::handler]
async fn bennu_dependency_tree(_ctx: &BennuState, args: DependencyTreeArgs) -> Result<DependencyTree, String> {
    tokio::task::spawn_blocking(move || tree_of(Path::new(&args.root)))
        .await
        .map_err(|e| format!("dependency tree: {e}"))?
}

fn tree_of(root: &Path) -> Result<DependencyTree, String> {
    if !root.join("pom.xml").is_file() {
        if root.join("Cargo.toml").is_file() {
            return cargo_tree_of(root);
        }
        return Err("The dependency tree needs a Maven or Cargo project (no pom.xml or Cargo.toml at the root).".into());
    }
    let launcher = find_mvn_launcher(root);
    let (modules, log) = run_tree(root, &launcher, true)?;
    if !modules.is_empty() {
        return Ok(DependencyTree { modules, verbose: true });
    }
    let (modules, plain_log) = run_tree(root, &launcher, false)?;
    if modules.is_empty() {
        return Err(failure_reason(&plain_log, &log));
    }
    Ok(DependencyTree { modules, verbose: false })
}

/// The Cargo half: `cargo tree --workspace`, every member's tree. Cargo never drops a version, so
/// the answer is always complete (`verbose: true`) and the "conflicts" are crates built at two
/// incompatible versions.
fn cargo_tree_of(root: &Path) -> Result<DependencyTree, String> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(root)
        .args(["tree", "--workspace", "--prefix", "depth", "--charset", "ascii", "--color", "never"]);
    let output = bennu_classpath::prelude::run_bounded(cmd, TIMEOUT)
        .map_err(|e| format!("spawn cargo: {e} — is Cargo on PATH?"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let modules = bennu_deps::prelude::parse_cargo_tree(&stdout);
    if modules.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason: Vec<&str> = stderr.lines().filter(|l| l.starts_with("error")).take(3).collect();
        return Err(if reason.is_empty() {
            "Cargo produced no dependency tree. Run `cargo tree` in the project to see why.".into()
        } else {
            reason.join("\n")
        });
    }
    Ok(DependencyTree { modules, verbose: true })
}

fn run_tree(root: &Path, launcher: &str, verbose: bool) -> Result<(Vec<ModuleTree>, String), String> {
    let mut cmd = Command::new(launcher);
    cmd.current_dir(root).arg("--batch-mode").arg("dependency:tree").arg("-DoutputType=text");
    if verbose {
        cmd.arg("-Dverbose");
    }
    // Same switch as the classpath resolve: downloading is a setting, not a side effect.
    if !auto_download() {
        cmd.arg("-o");
    }
    // The JDK the project is indexed with, not whichever one the environment happens to name: a
    // pom targeting Java 8 does not always build under the newest JDK on the machine.
    let jdk = crate::index_service::IndexService::global()
        .jdk_version_of(&root.to_string_lossy())
        .unwrap_or_default();
    if let Some(home) = bennu_classpath::prelude::find_jdk_home(&jdk) {
        cmd.env("JAVA_HOME", home);
    }
    let output = bennu_classpath::prelude::run_bounded(cmd, TIMEOUT)
        .map_err(|e| format!("spawn mvn ({launcher}): {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok((parse_tree_output(&stdout), stdout))
}

/// The last few lines Maven called an error, or its tail when it called nothing one.
fn failure_reason(primary: &str, fallback: &str) -> String {
    let text = if primary.trim().is_empty() { fallback } else { primary };
    let errors: Vec<&str> = text.lines().filter(|l| l.contains("[ERROR]")).take(4).collect();
    if errors.is_empty() {
        "Maven produced no dependency tree. Run `mvn dependency:tree` in the project to see why.".into()
    } else {
        errors.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_names_what_maven_called_an_error() {
        let log = "[INFO] x\n[ERROR] Failed to execute goal\n[ERROR] Could not resolve foo\n";
        let r = failure_reason(log, "");
        assert!(r.contains("Could not resolve foo"));
        assert!(failure_reason("", "").contains("dependency:tree"));
    }
}
