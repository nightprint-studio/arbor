//! `project` domain — `bennu_open_project` / `bennu_project_tree` / `bennu_read_file`.
//!
//! Thin wrappers over the leaf `bennu-project` crate: this module only marshals the
//! RPC args, reads the backend-owned config (default encoding + per-project/per-file
//! overrides), and forwards to `bennu_project::prelude`. The analysis logic lives in
//! the leaf; the be layer stays glue.
//!
//! The handler fn names ARE the wire method names (the `#[arbor_rpc::handler]`
//! contract), so they must read `bennu_open_project` / `bennu_project_tree` /
//! `bennu_read_file` exactly — the FE is built against them.

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{FileContents, ProjectInfo, TreeNode};
use bennu_project::prelude::{build_tree, open_project, read_file, OpenOptions};
use serde::Deserialize;

use crate::index_service::IndexService;

/// The JDK level used to resolve classpath sources when the project doesn't declare
/// (or we can't infer) one. JDK 8 is the target-stack default (Struts2/Entando).
const DEFAULT_JDK: &str = "1.8";

/// How deep the project tree is materialised. Legacy Java packages nest deep
/// (`src/main/java/<deep.pkg>/…`, easily 9+ levels), and the FE tree doesn't
/// lazy-fetch yet (and `TreeNode` has no "expandable" flag to distinguish a
/// truncated dir from an empty one), so a small depth cut off the `.java` leaves.
/// Materialise effectively the whole source tree in one shot — noise dirs
/// (`target`/`.git`/`node_modules`) are skipped in `bennu-project::tree`, so it stays
/// a fast single fs walk.
const TREE_DEPTH: usize = 64;

/// Args for [`bennu_open_project`].
#[derive(Deserialize)]
pub struct OpenProjectArgs {
    /// Absolute path to the project root (the dir holding the root `pom.xml`).
    pub root: String,
}

/// Open a Maven project: parse its pom, detect capabilities / JDK / encoding, and
/// return the [`ProjectInfo`]. The default encoding + per-project JDK override come
/// from the backend-owned config.
#[arbor_rpc::handler]
fn bennu_open_project(_ctx: &BennuState, args: OpenProjectArgs) -> Result<ProjectInfo, String> {
    let cfg = bennu_core::config::load();
    let jdk_override = cfg.jdk_overrides.get(&args.root).map(|s| s.as_str());
    let opts = OpenOptions { default_encoding: &cfg.default_encoding, jdk_override };
    let info = open_project(Path::new(&args.root), &opts).map_err(String::from)?;

    // Kick off the symbol-index build off the IPC thread (async, non-blocking). The
    // completion provider goes live when it finishes; until then `bennu_completion`
    // serves the empty list. Resolve the JDK level from the project (else the target
    // stack's JDK 8).
    let jdk_version =
        info.jdk.as_ref().map(|j| j.version.clone()).unwrap_or_else(|| DEFAULT_JDK.to_string());
    IndexService::global().open(&args.root, &jdk_version);

    Ok(info)
}

/// Args for [`bennu_project_tree`].
#[derive(Deserialize)]
pub struct ProjectTreeArgs {
    /// Absolute path to the directory to build the tree from (the project root, or a
    /// sub-directory the FE is lazily expanding).
    pub root: String,
    /// Optional depth override; defaults to [`TREE_DEPTH`].
    pub depth: Option<usize>,
}

/// Build the project file tree rooted at `root`, dirs-first, noise-dirs skipped.
#[arbor_rpc::handler]
fn bennu_project_tree(_ctx: &BennuState, args: ProjectTreeArgs) -> Result<TreeNode, String> {
    build_tree(Path::new(&args.root), args.depth.unwrap_or(TREE_DEPTH)).map_err(Into::into)
}

/// Args for [`bennu_read_file`].
#[derive(Deserialize)]
pub struct ReadFileArgs {
    /// Absolute path to the project root (used to resolve the pom-declared encoding).
    pub root: String,
    /// Absolute path to the file to read.
    pub file: String,
}

/// Read a file decoded in the project's resolved encoding (per-file/per-project
/// override → pom-declared → config default). Returns the text + the encoding that
/// applied.
#[arbor_rpc::handler]
fn bennu_read_file(_ctx: &BennuState, args: ReadFileArgs) -> Result<FileContents, String> {
    let cfg = bennu_core::config::load();
    // A per-file override wins over a per-project one (both keyed by absolute path).
    let override_label = cfg
        .encoding_overrides
        .get(&args.file)
        .or_else(|| cfg.encoding_overrides.get(&args.root))
        .map(|s| s.as_str());
    read_file(Path::new(&args.root), Path::new(&args.file), &cfg.default_encoding, override_label)
        .map_err(Into::into)
}
