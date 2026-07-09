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
use bennu_proto::prelude::{FileContents, ProjectInfo, TreeNode, WriteResult};
use bennu_project::prelude::{build_tree, open_project, read_file, write_file, OpenOptions};
use serde::{Deserialize, Serialize};

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
fn bennu_open_project(ctx: &BennuState, args: OpenProjectArgs) -> Result<ProjectInfo, String> {
    let cfg = bennu_core::config::load();
    let jdk_override = cfg.jdk_overrides.get(&args.root).map(|s| s.as_str());
    let opts = OpenOptions { default_encoding: &cfg.default_encoding, jdk_override };
    let info = open_project(Path::new(&args.root), &opts).map_err(String::from)?;

    // Kick off the symbol-index build off the IPC thread (async, non-blocking). The
    // completion provider goes live when it finishes; until then `bennu_completion`
    // serves the empty list. Resolve the JDK level from the project (else the target
    // stack's JDK 8). The build emits `arbor://bennu/index-progress` events on the event
    // sink so the FE can show a live "Indexing…" status.
    let jdk_version =
        info.jdk.as_ref().map(|j| j.version.clone()).unwrap_or_else(|| DEFAULT_JDK.to_string());
    // Index sources in the project's declared encoding (per-project override → pom
    // `sourceEncoding` → config default) so a legacy Cp1252 tree is indexed in its real
    // encoding; a mislabelled file is recovered + reported, not dropped.
    let encoding_label = crate::index_service::resolve_index_encoding(&args.root);
    // Wire the reverse channel so the background analysis warm-up can register a tracked job.
    IndexService::global().set_host(ctx.host_caller());
    IndexService::global().open(&args.root, &jdk_version, &encoding_label, ctx.event_sink());

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

/// Args for [`bennu_write_file`].
#[derive(Deserialize)]
pub struct WriteFileArgs {
    /// Absolute path to the project root (used to resolve the pom-declared encoding).
    pub root: String,
    /// Absolute path to the file to write.
    pub file: String,
    /// The buffer text to save (always valid UTF-8 on the wire).
    pub text: String,
}

/// Write a file encoded in the project's resolved encoding — the round-trip inverse of
/// [`bennu_read_file`] (per-file/per-project override → pom-declared → config default). A
/// char the declared encoding can't represent falls back to UTF-8 and still succeeds.
/// Returns the encoding that actually applied.
#[arbor_rpc::handler]
fn bennu_write_file(_ctx: &BennuState, args: WriteFileArgs) -> Result<WriteResult, String> {
    let cfg = bennu_core::config::load();
    // A per-file override wins over a per-project one (both keyed by absolute path) — the
    // same resolution `bennu_read_file` uses, so a read and its save agree on encoding.
    let override_label = cfg
        .encoding_overrides
        .get(&args.file)
        .or_else(|| cfg.encoding_overrides.get(&args.root))
        .map(|s| s.as_str());
    write_file(
        Path::new(&args.root),
        Path::new(&args.file),
        &args.text,
        &cfg.default_encoding,
        override_label,
    )
    .map_err(Into::into)
}

/// Args for [`bennu_move_to_package`].
#[derive(Deserialize)]
pub struct MoveToPackageArgs {
    /// Absolute path of the `.java` file to move.
    pub file: String,
    /// The buffer text — its declared `package` determines the destination folder.
    pub source: String,
}

/// Where the file ended up after moving it to match its declared package.
#[derive(Serialize)]
pub struct MoveResult {
    /// The new absolute path.
    pub new_path: String,
}

/// Move a `.java` file into the folder that matches the `package` it declares — the filesystem
/// counterpart of the `change-package` quick-fix (which instead rewrites the declaration). The
/// destination is the file's source root (`src/main/java` …) joined with the declared package path.
///
/// The caller must save the buffer first: this renames the on-disk file as-is. Errors (never panics)
/// when the source root can't be determined, the file is already in place, or a file already exists
/// at the destination.
#[arbor_rpc::handler]
fn bennu_move_to_package(_ctx: &BennuState, args: MoveToPackageArgs) -> Result<MoveResult, String> {
    let src_path = Path::new(&args.file);
    let parent = src_path.parent().ok_or("file has no parent directory")?;
    let package = bennu_java::prelude::extract_symbols(&args.source).package.unwrap_or_default();
    let target_dir = bennu_java::prelude::package_dir(parent, &package)
        .ok_or("cannot determine the project source root for this file")?;
    let file_name = src_path.file_name().ok_or("file has no name")?;
    let target = target_dir.join(file_name);

    if target == src_path {
        return Err("The file is already in the folder matching its package".to_string());
    }
    if target.exists() {
        return Err(format!("A file already exists at {}", target.display()));
    }
    std::fs::create_dir_all(&target_dir).map_err(|e| format!("create target dir: {e}"))?;
    std::fs::rename(src_path, &target).map_err(|e| format!("move file: {e}"))?;

    Ok(MoveResult { new_path: target.to_string_lossy().replace('\\', "/") })
}
