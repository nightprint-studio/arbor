//! The Node half of the Dependencies tool window.
//!
//! | Handler | Question |
//! |---|---|
//! | `bennu_node_dependencies` | what do the `package.json`s declare, and what did the lockfile resolve |
//! | `bennu_node_dependency_tree` | who requires what, and which packages are installed at several versions |
//! | `bennu_node_advisories` | which of the resolved packages have known vulnerabilities (OSV, `npm`) |
//! | `bennu_node_outdated` | which declared packages have a newer release |
//!
//! Separate handlers rather than a flag on the Maven ones: a root can legitimately be both a Maven
//! and a Node project (a frontend built by the Maven build), and the panel says which of the two it is
//! looking at — which also keeps every existing caller of `bennu_dependencies` exactly as it was.
//!
//! Only the last two touch the network, and both honour the settings the Maven side does: the
//! vulnerability check is `[maven] advisories`, the registry lookups are the switch and TTL that
//! `package.json` version hints already share with crates.io (see [`crate::npm`]).

use std::collections::BTreeMap;
use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_deps::prelude::{node_packages, node_tree, read_node, NodePackage, Report};
use bennu_npm::prelude::bump;
use serde::{Deserialize, Serialize};

use crate::dependency_tree::DependencyTree;
use crate::maven_advisories::{check_project, is_exact_version, Eco, ProjectAdvisories};
use crate::task::{Task, TaskSpec};

#[derive(Deserialize)]
pub struct NodeRootArgs {
    /// The folder holding the `package.json` (and, for a workspace, the lockfile).
    pub root: String,
}

/// The report for a Node root. Public to the crate for the advisories pass, which reads it again.
pub(crate) fn report_of_node(root: &str) -> Report {
    read_node(Path::new(root))
}

/// Every package's declared dependencies, with the versions the lockfile chose. Reads files only.
#[arbor_rpc::handler]
fn bennu_node_dependencies(_ctx: &BennuState, args: NodeRootArgs) -> Result<Report, String> {
    Ok(report_of_node(&args.root))
}

/// What the Node tool window lists: the project's packages and scripts, and the package manager.
#[derive(Debug, Clone, Serialize)]
pub struct NodeProject {
    /// `npm` · `pnpm` · `yarn` · `bun`, from the lockfile beside the root manifest.
    pub package_manager: String,
    /// The lockfile's file name, empty when there is none (not installed yet).
    pub lockfile: String,
    /// Whether `node_modules` exists at the root.
    pub installed: bool,
    pub packages: Vec<NodePackage>,
}

/// The project's packages (the root, then each workspace member) with their scripts. Reads
/// manifests only; the lockfile is looked at for its name, not its contents.
#[arbor_rpc::handler]
fn bennu_node_packages(_ctx: &BennuState, args: NodeRootArgs) -> Result<NodeProject, String> {
    let root = Path::new(&args.root);
    let lockfile = bennu_npm::prelude::lockfile_in(root)
        .and_then(|(_, path)| path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    Ok(NodeProject {
        package_manager: bennu_npm::prelude::package_manager_for(root).program().to_string(),
        lockfile,
        installed: root.join("node_modules").is_dir(),
        packages: node_packages(root),
    })
}

/// The resolved tree of every package, read out of the lockfile.
///
/// A row in the Jobs panel while it runs, like the Maven resolve it stands next to — the parse of a
/// very large lockfile is not instant, and the panel that asked keeps its own spinner too.
#[arbor_rpc::handler]
async fn bennu_node_dependency_tree(ctx: &BennuState, args: NodeRootArgs) -> Result<DependencyTree, String> {
    let task = Task::start(
        ctx.host_caller().as_ref(),
        &ctx.event_sink(),
        TaskSpec::new("Resolve dependency tree", args.root.clone()).category("Dependencies"),
    );
    let root = args.root;
    let result = tokio::task::spawn_blocking(move || node_tree(Path::new(&root)))
        .await
        .map_err(|e| format!("dependency tree: {e}"))
        .and_then(|r| r)
        .map(|modules| DependencyTree { modules, verbose: true });
    task.finish(result.as_ref().map(|_| ()).map_err(Clone::clone));
    result
}

/// Known vulnerabilities across every resolved package — declared and transitive. Same answer shape,
/// switch and window as Maven's.
#[arbor_rpc::handler]
async fn bennu_node_advisories(ctx: &BennuState, args: NodeRootArgs) -> Result<ProjectAdvisories, String> {
    check_project(ctx, args.root, Eco::Npm, report_of_node).await
}

// ── Newer versions ───────────────────────────────────────────────────────────

/// Registry lookups one request may make; the rest are answered on the next call, from the cache
/// the first ones filled. A cold project with a hundred dependencies would otherwise be a hundred
/// requests before the first badge appeared.
const MAX_FETCHES: usize = 40;
/// How many of them are in flight at once — a polite number for a public registry.
const CONCURRENCY: usize = 8;

/// A declared package with a newer release.
#[derive(Debug, Clone, Serialize)]
pub struct OutdatedItem {
    pub name: String,
    /// What the lockfile resolved — the version running, not the range written.
    pub current: String,
    /// The registry's `latest` dist-tag.
    pub latest: String,
    /// `major` · `minor` · `patch`.
    pub bump: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeOutdated {
    /// Whether registry lookups are switched on. `false` is a state the panel says nothing about —
    /// the setting is the user's, and no badge is the answer.
    pub enabled: bool,
    /// Packages not looked up yet because of [`MAX_FETCHES`]; calling again continues.
    pub pending: usize,
    pub items: Vec<OutdatedItem>,
}

/// Which declared packages are behind.
///
/// Only packages with a **resolved** version that come from the registry: a range cannot be "behind"
/// (it is a promise about a set), and a workspace sibling, a git URL or an alias has no registry
/// entry that means the same thing.
#[arbor_rpc::handler]
async fn bennu_node_outdated(_ctx: &BennuState, args: NodeRootArgs) -> Result<NodeOutdated, String> {
    let cfg = bennu_core::config::load().cargo;
    if !cfg.crates_io {
        return Ok(NodeOutdated { enabled: false, pending: 0, items: Vec::new() });
    }
    let root = args.root;
    let report = tokio::task::spawn_blocking(move || report_of_node(&root))
        .await
        .map_err(|e| format!("report: {e}"))?;

    // One lookup per package name, whichever modules declare it; the version is the first one seen,
    // which for a workspace is the same lockfile entry nearly always.
    let mut wanted: BTreeMap<String, String> = BTreeMap::new();
    for d in report.modules.iter().flat_map(|m| &m.dependencies) {
        if d.source == "npm" && d.variant.is_empty() && is_exact_version(&d.version) {
            wanted.entry(d.name.clone()).or_insert_with(|| d.version.clone());
        }
    }

    let mut budget = MAX_FETCHES;
    let mut plan: Vec<(String, String, bool)> = Vec::new();
    for (name, current) in wanted {
        let fresh = crate::npm::is_cache_fresh(&name, &cfg);
        let cached_only = !fresh && budget == 0;
        if !fresh && !cached_only {
            budget -= 1;
        }
        plan.push((name, current, cached_only));
    }

    let mut items = Vec::new();
    let mut pending = 0;
    for chunk in plan.chunks(CONCURRENCY) {
        let handles: Vec<_> = chunk
            .iter()
            .cloned()
            .map(|(name, current, cached_only)| {
                let cfg = cfg.clone();
                tokio::spawn(async move {
                    let latest = crate::npm::latest_of(&name, &cfg, cached_only).await;
                    (name, current, cached_only, latest)
                })
            })
            .collect();
        for handle in handles {
            let Ok((name, current, cached_only, latest)) = handle.await else { continue };
            match latest {
                Some(latest) => {
                    if let Some(b) = bump(&current, &latest) {
                        items.push(OutdatedItem { name, current, latest, bump: b.as_str().to_string() });
                    }
                }
                None if cached_only => pending += 1,
                None => {}
            }
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(NodeOutdated { enabled: true, pending, items })
}
