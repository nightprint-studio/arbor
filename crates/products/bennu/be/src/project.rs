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
use std::sync::{Mutex, OnceLock};

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{
    FileContents, FileStamp, ProjectInfo, SourceEdit, TreeNode, WriteResult,
};
use bennu_project::prelude::{
    build_tree, file_stamp, open_project, read_file, rename_path, write_file, OpenOptions,
};
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;
use crate::lsp_registry::SessionOrigin;

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
#[derive(Deserialize, schemars::JsonSchema)]
pub struct OpenProjectArgs {
    /// Absolute path to the project root (the dir holding the root `pom.xml`).
    pub root: String,
    /// Whether this project is the one about to be **on screen**. Absent means yes.
    ///
    /// Only a workspace restore says otherwise, and it is the reason this field exists: it opens
    /// every member of the workspace to read its manifest and stash its session, and exactly one
    /// of them is then activated. Warm-starting a language server for each was a rust-analyzer
    /// per Cargo project — five projects, five indexing runs, several gigabytes — for four
    /// projects nobody was looking at.
    #[serde(default)]
    pub active: Option<bool>,
}

/// Open a project (Maven or Cargo — the leaf dispatches on the manifest): parse the
/// manifest, detect capabilities / JDK / encoding where they apply, and return the
/// [`ProjectInfo`]. The default encoding + per-project JDK override come from the
/// backend-owned config.
///
/// Two engines start here, and which one depends on the root:
///
/// * the Java **symbol index**, for a Maven root only. For a Cargo one there is nothing it
///   could index — the sources aren't Java and the classpath doesn't exist — so starting it
///   would spend a full tree walk to produce an empty index and light the FE's "Indexing…"
///   status for a result no feature reads.
/// * a **language server**, for any root carrying a manifest one of them claims (a
///   `Cargo.toml` → rust-analyzer). Warm-started rather than started on the first request,
///   because rust-analyzer needs tens of seconds to index and a user who opens a `.rs` file
///   and gets nothing reads that as "Bennu has no Rust support" rather than as "the server is
///   warming up". Starting at open moves the wait to before the first question.
///
/// Both are off-thread and neither blocks the other; a polyglot root gets both.
#[arbor_rpc::handler]
fn bennu_open_project(ctx: &BennuState, args: OpenProjectArgs) -> Result<ProjectInfo, String> {
    let warm = args.active.unwrap_or(true);
    if warm {
        set_active_root(&args.root);
    }
    open_and_start(ctx, &args.root, SessionOrigin::Window, warm)
}

// ── The project on screen ────────────────────────────────────────────────────────
//
// Kept here because this module owns both events that change it: opening a project as the
// active one, and switching to one already open. A plugin asking "what is open?" is asking
// this, and until now nothing in the backend could answer — the root travelled as an argument
// on every call and was never remembered.

static ACTIVE_ROOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn active_slot() -> &'static Mutex<Option<String>> {
    ACTIVE_ROOT.get_or_init(|| Mutex::new(None))
}

fn set_active_root(root: &str) {
    if let Ok(mut slot) = active_slot().lock() {
        *slot = Some(root.to_string());
    }
}

/// The project currently on screen, or `None` before one is opened.
pub fn active_root() -> Option<String> {
    active_slot().lock().ok().and_then(|s| s.clone())
}

/// Args for [`bennu_activate_project`].
#[derive(Deserialize)]
pub struct ActivateProjectArgs {
    /// Absolute path to the project root now on screen.
    pub root: String,
}

/// Say that an already-open project is now the one on screen.
///
/// The other half of `active: false`. Opening a project and *looking at* one are two different
/// events, and only the second is a reason to spend a language server: a workspace restore does
/// the first for every member and the second for exactly one, and switching between members
/// afterwards does the second again with no re-open.
///
/// Idempotent and cheap — a server already up is claimed rather than restarted, which is what
/// stops it being reclaimed as idle while somebody is reading it.
#[arbor_rpc::handler]
fn bennu_activate_project(ctx: &BennuState, args: ActivateProjectArgs) -> Result<(), String> {
    set_active_root(&args.root);
    crate::lsp_registry::LspRegistry::global().set_sink(ctx.event_sink());
    crate::lsp_registry::LspRegistry::global().warm_start(&args.root, SessionOrigin::Window);
    Ok(())
}

/// Open `root` and start everything that opening it starts: the language-server warm-up,
/// the framework-extension registration, and the symbol-index build.
///
/// Shared because there is now more than one door into a project — the editor's
/// `bennu_open_project` and the agent surface's `bennu_project_summary` — and a second
/// door that starts only *some* of the engines is a project that behaves differently
/// depending on who opened it. Idempotent: re-opening re-reads the manifest and leaves
/// a running index alone.
///
/// `origin` is the one thing the two doors do NOT share, and it is about the language server
/// rather than the project: a window has somebody looking at it, so its server runs in full and is
/// never reclaimed; a request from an AI client has nothing on screen between one call and the
/// next, so its server runs lean and is stopped once it goes quiet. Both doors otherwise start
/// exactly the same engines, which is what this function is for.
pub(crate) fn open_and_start(
    ctx: &BennuState,
    root: &str,
    origin: SessionOrigin,
    warm: bool,
) -> Result<ProjectInfo, String> {
    let args = OpenProjectArgs { root: root.to_string(), active: Some(warm) };
    let cfg = bennu_core::config::load();
    let jdk_override = cfg.jdk_overrides.get(&args.root).map(|s| s.as_str());
    let opts = OpenOptions { default_encoding: &cfg.default_encoding, jdk_override };
    let info = open_project(Path::new(&args.root), &opts).map_err(String::from)?;

    // The registry needs the sink before it can report its own progress, and `warm_start`
    // itself only claims slots and spawns threads — the handshake happens on those, so this
    // never blocks the open.
    crate::lsp_registry::LspRegistry::global().set_sink(ctx.event_sink());
    // Not for a project being loaded as an inactive workspace member: see `OpenProjectArgs::active`.
    // Nothing is lost by waiting — the first request against one of its files starts the server on
    // its own, and switching to it says so through `bennu_activate_project`.
    if warm {
        crate::lsp_registry::LspRegistry::global().warm_start(&args.root, origin);
    }

    // Every project kind, and BEFORE the early return below: the framework-extension host resolves a
    // file's project through this, and a Cargo root that never registered made every caret-based
    // framework query on it answer "no project owns this file" — see `frameworks::register_root`.
    crate::frameworks::register_root(&args.root);

    // Retention, off-thread. Opening a project is the one moment that is already slow for
    // other reasons and happens once per session — which is exactly what a policy that
    // deletes things should be attached to, rather than to a timer nobody can predict.
    crate::history::purge_in_background(&args.root);

    if !info.kind.is_java() {
        return Ok(info);
    }

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
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ProjectTreeArgs {
    /// Absolute path to the directory to build the tree from (the project root, or a
    /// sub-directory the FE is lazily expanding).
    pub root: String,
    /// Optional depth override; defaults to [`TREE_DEPTH`].
    pub depth: Option<usize>,
}

/// List the files and directories under a project path, directories first, with build
/// output and VCS noise already excluded.
///
/// Use it to find your way around a project you have not read yet. `depth` bounds how
/// far it descends — keep it small on a large tree and call again on the sub-directory
/// you care about, rather than pulling the whole thing into context at once.
#[arbor_rpc::handler(mcp(
    title = "List a project's files",
    safety = read,
))]
fn bennu_project_tree(_ctx: &BennuState, args: ProjectTreeArgs) -> Result<TreeNode, String> {
    build_tree(Path::new(&args.root), args.depth.unwrap_or(TREE_DEPTH)).map_err(Into::into)
}

/// Args for [`bennu_read_file`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReadFileArgs {
    /// Absolute path to the project root (used to resolve the pom-declared encoding).
    pub root: String,
    /// Absolute path to the file to read.
    pub file: String,
}

/// Read a project file as text, decoded with the encoding that file is actually in.
///
/// Prefer this over a plain filesystem read for anything inside a Java project: legacy
/// sources are frequently Cp1252 or Latin-1 rather than UTF-8, and the encoding is
/// resolved from the build manifest and per-file overrides. Reading those bytes as UTF-8
/// silently mangles every accented character. The reply names the encoding that applied.
#[arbor_rpc::handler(mcp(
    title = "Read a project file",
    safety = read,
))]
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

/// Args for [`bennu_file_stamps`].
#[derive(Deserialize)]
pub struct FileStampsArgs {
    /// Absolute paths to stat. Typically the editor's open tabs — a handful, not a tree.
    pub files: Vec<String>,
}

/// Stat `files` and return each one's current on-disk [`FileStamp`].
///
/// The external-change poll: the FE holds the stamp each open buffer was read from and
/// compares. A stat per open tab is cheap enough to run whenever the window regains focus
/// (and on a slow tick while it has focus), which is what makes "somebody else changed this
/// file" a thing Bennu notices rather than something it discovers by overwriting it.
///
/// Never fails: an unreadable path comes back with an empty stamp and `exists: false`, and
/// the caller decides what that means (see `bennu_write_file`'s guard — a vanished file
/// must not block the save that recreates it).
#[arbor_rpc::handler]
fn bennu_file_stamps(_ctx: &BennuState, args: FileStampsArgs) -> Result<Vec<FileStamp>, String> {
    Ok(args
        .files
        .into_iter()
        .map(|file| {
            let stamp = file_stamp(Path::new(&file));
            FileStamp { exists: !stamp.is_empty(), file, stamp }
        })
        .collect())
}

/// Args for [`bennu_write_file`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct WriteFileArgs {
    /// Absolute path to the project root (used to resolve the pom-declared encoding).
    pub root: String,
    /// Absolute path to the file to write.
    pub file: String,
    /// The buffer text to save (always valid UTF-8 on the wire).
    pub text: String,
    /// The on-disk stamp the caller's buffer was read from. When present and still
    /// matching, the save proceeds; when the file has changed underneath, the save is
    /// **refused** (see [`bennu_project::prelude::write_file`]). Omitted / empty disables
    /// the check — `#[serde(default)]`, so a caller that never read the file still saves.
    #[serde(default)]
    pub expect_stamp: Option<String>,
}

/// Write a file encoded in the project's resolved encoding — the round-trip inverse of
/// [`bennu_read_file`] (per-file/per-project override → pom-declared → config default). A
/// char the declared encoding can't represent falls back to UTF-8 and still succeeds.
/// Returns the encoding that actually applied + the new on-disk stamp.
///
/// Refuses with an [`ERR_EXTERNALLY_MODIFIED`](bennu_proto::prelude::ERR_EXTERNALLY_MODIFIED)-
/// prefixed error when `expect_stamp` says the file changed since it was read — the guard
/// that keeps autosave from throwing away an edit made outside Bennu.
#[arbor_rpc::handler(mcp(
    title = "Write a project file in its own encoding",
    safety = write,
    description = "Save text to a project file, encoded the way that file actually is. \
Use this instead of an ordinary file write for anything inside a Java project: legacy \
sources are frequently Cp1252 or Latin-1, and writing UTF-8 bytes over them silently \
mangles every accented character — a corruption that reads back fine to you, because you \
read through bennu_read_file, and is wrong for everyone else. Pass the `stamp` \
bennu_read_file gave you and the save is refused if the file changed underneath instead \
of overwriting someone's work.",
))]
fn bennu_write_file(_ctx: &BennuState, args: WriteFileArgs) -> Result<WriteResult, String> {
    let cfg = bennu_core::config::load();
    // A per-file override wins over a per-project one (both keyed by absolute path) — the
    // same resolution `bennu_read_file` uses, so a read and its save agree on encoding.
    let override_label = cfg
        .encoding_overrides
        .get(&args.file)
        .or_else(|| cfg.encoding_overrides.get(&args.root))
        .map(|s| s.as_str());
    // Read BEFORE the write: once it has happened, what the file used to be exists
    // nowhere else. Only needed for a file history has never heard of — after the first
    // revision, the previous save is the "before" — and `on_write` is what decides that,
    // so this is a read of a file that is about to be written anyway.
    let disk_before = std::fs::read(&args.file).ok();

    let result: WriteResult = write_file(
        Path::new(&args.root),
        Path::new(&args.file),
        &args.text,
        &cfg.default_encoding,
        override_label,
        args.expect_stamp.as_deref(),
    )
    .map_err(|e| -> String { e.into() })?;

    // After the write and after the guard: a refused save must not leave a revision
    // claiming something happened.
    crate::history::on_write(&args.root, &args.file, disk_before, &Default::default());

    // Tell a language server the file was saved. This is where the real diagnostics come from:
    // rust-analyzer runs `cargo check` on save, so a type or borrow error only exists after
    // this. Done here rather than in the frontend so autosave counts too — a user who never
    // presses Ctrl+S would otherwise only ever see parse errors.
    crate::lsp_route::did_save(&args.file, &args.text);

    Ok(result)
}

/// Args for [`bennu_rename_path`].
#[derive(Deserialize)]
pub struct RenamePathArgs {
    /// Absolute path to the project root — the history is keyed by it.
    pub root: String,
    /// Absolute path of the file to rename.
    pub file: String,
    /// Its absolute path afterwards. The caller builds it, so this is also how a file is *moved*.
    pub new_path: String,
}

/// A rename, and what it means for the code that referred to the file.
#[derive(Serialize)]
pub struct RenamePathResult {
    /// Where the file now is.
    pub new_path: String,
    /// The edits the rename implies — a Rust `mod` declaration that names the file and every `use`
    /// path through the module it declares. Empty when the language does not care (or no server
    /// does): a `.txt` rename implies nothing.
    ///
    /// Returned rather than applied: Bennu applies edits through the editor so they land in the undo
    /// history, and a backend that wrote them behind the frontend's back would leave open buffers
    /// disagreeing with their files.
    pub edits: Vec<SourceEdit>,
}

/// Rename a file, and answer with the code edits the rename implies.
///
/// The order is the whole subtlety, and it is dictated by the protocol: the server is asked
/// **before** the file moves (`workspace/willRenameFiles` — it answers about the tree as it stands,
/// and after the move the old path names nothing). Then the file moves. If the move fails, the edits
/// are dropped on the floor, which is the correct outcome — they described a rename that did not
/// happen.
///
/// Refuses rather than overwriting; see [`bennu_project::prelude::rename_path`] for which cases and
/// why.
#[arbor_rpc::handler]
fn bennu_rename_path(_ctx: &BennuState, args: RenamePathArgs) -> Result<RenamePathResult, String> {
    // Asked first: after `rename` the old path is gone, and a server asked about a file that no
    // longer exists has nothing to say.
    let edits = crate::lsp_route::will_rename(&args.file, &args.new_path);
    rename_path(Path::new(&args.file), Path::new(&args.new_path))
        .map_err(|e| -> String { e.into() })?;
    crate::history::on_rename(&args.root, &args.file, &args.new_path);
    Ok(RenamePathResult { new_path: args.new_path, edits })
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
