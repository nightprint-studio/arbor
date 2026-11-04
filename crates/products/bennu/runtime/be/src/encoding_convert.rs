//! `encoding_convert` domain — rewrite files from one encoding to another.
//!
//! Three handlers, in the order a person uses them:
//!
//! * [`bennu_encoding_convert_plan`] — the dry run. Reads every file in scope, converts it **in
//!   memory** and reports what would happen: which files change, which are skipped and why, and
//!   which characters the target cannot hold (with where they are). Writes nothing.
//! * [`bennu_encoding_convert_apply`] — does it. Every file is copied to a backup folder before it is
//!   touched, the writes run on the shared worker pool as a Job with progress, and the run is
//!   recorded in a manifest so [`bennu_encoding_convert_undo`] can put everything back as ONE step.
//! * [`bennu_encoding_convert_undo`] — restores the latest run (or a named one), file by file, and
//!   leaves alone any file somebody changed after the conversion.
//!
//! The byte-level work is `bennu_project::convert` — pure and unit-tested. What lives here is
//! everything around it: which files are in scope, the backups, the project's declared encoding, the
//! per-file overrides, local history and the Jobs panel.
//!
//! ## Why the config is rewritten
//!
//! Bennu decodes a file by *override → pom → default*. Convert a file to UTF-8 and leave that rule
//! alone and the next read decodes the new bytes as the old encoding: every accent turns into
//! mojibake in the editor, and a save writes the garbage back. So a conversion also moves the rule —
//! the project's declared encoding when the whole project was converted, a per-file override when
//! only part of it was — and records what it changed so an undo moves the rule back with the bytes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arbor_history::prelude::RecordCtx;
use bennu_core::prelude::BennuState;
use bennu_project::prelude::{
    convert_encoding, detect_encoding, file_stamp, parse_pom, BomPolicy, Codec, ConvertError, ConvertOptions,
    Converted, EncodingPlan, EscapeStyle, Fallback,
};
use serde::{Deserialize, Serialize};

use crate::project::normalise_key;
use crate::task::{Task, TaskSpec};

/// The property Maven reads the source encoding from.
const SOURCE_ENCODING_KEY: &str = "project.build.sourceEncoding";

/// Rows a preview carries. A conversion of a huge tree is still applied in full; this only bounds
/// how much the dialog is asked to draw.
const MAX_PLAN_ROWS: usize = 3000;

/// How long a finished run's backup is kept.
const BACKUP_KEEP: Duration = Duration::from_secs(14 * 24 * 3600);

/// Extensions that are never text — a short binary can be NUL-free.
const BINARY_EXTS: [&str; 18] = [
    "class", "jar", "war", "ear", "zip", "gz", "png", "jpg", "jpeg", "gif", "ico", "pdf", "exe", "dll",
    "so", "bin", "o", "wasm",
];

// ── wire types ───────────────────────────────────────────────────────────────────────────────

/// What a conversion is asked to do. Shared by the preview and the real thing, so the preview
/// cannot describe something the apply will not do.
#[derive(Deserialize, Clone)]
pub struct ConvertRequest {
    /// Absolute path of the project root.
    pub root: String,
    /// Files and/or directories to convert, absolute and inside the project. The root itself means
    /// the whole project.
    pub targets: Vec<String>,
    /// The encoding the files are in now. Absent: each file's own (override → pom → default).
    #[serde(default)]
    pub from: Option<String>,
    /// The encoding to write.
    pub to: String,
    /// `keep` | `add` | `remove`.
    #[serde(default = "default_keep")]
    pub bom: String,
    /// `abort` | `replace` | `escape` — what to do with a character `to` cannot hold.
    #[serde(default = "default_abort")]
    pub fallback: String,
    /// Rewrite the encoding a file declares about itself (XML prolog, JSP directive).
    #[serde(default = "default_true")]
    pub rewrite_declarations: bool,
}

fn default_keep() -> String {
    "keep".into()
}
fn default_abort() -> String {
    "abort".into()
}
fn default_true() -> bool {
    true
}

/// Args for [`bennu_encoding_convert_apply`].
#[derive(Deserialize)]
pub struct ApplyArgs {
    #[serde(flatten)]
    pub request: ConvertRequest,
    /// Also write the new encoding into the pom(s) — `project.build.sourceEncoding`. Only honoured
    /// when the whole project was converted, because a project-wide declaration over a partly
    /// converted tree would be false for the part that was not.
    #[serde(default)]
    pub declare_project: bool,
}

/// Args for [`bennu_encoding_convert_undo`].
#[derive(Deserialize)]
pub struct UndoArgs {
    pub root: String,
    /// The run to undo. Absent: the latest one that has not been undone.
    #[serde(default)]
    pub run_id: Option<String>,
}

/// One character the target cannot hold.
#[derive(Serialize)]
pub struct Sample {
    pub line: u32,
    pub column: u32,
    pub ch: String,
    /// `U+20AC`.
    pub code: String,
}

/// What would happen to one file.
#[derive(Serialize)]
pub struct FilePlan {
    /// Absolute path, forward slashes.
    pub file: String,
    /// Project-relative path, forward slashes.
    pub rel: String,
    /// `convert` | `unchanged` | `binary` | `invalid` | `blocked` | `unreadable` | `unknown-encoding`.
    pub status: &'static str,
    /// The encoding it is read as.
    pub from: String,
    /// The encoding a byte-level guess says it is in — a hint when `from` looks wrong.
    pub detected: String,
    pub size_before: u64,
    pub size_after: u64,
    pub bom_before: bool,
    pub bom_after: bool,
    pub unrepresentable_total: usize,
    pub samples: Vec<Sample>,
    /// The encoding the file declares about itself, when it is not the target.
    pub declaration: Option<String>,
    pub declaration_rewritten: bool,
    /// Why it is skipped / blocked, in words.
    pub message: Option<String>,
}

/// What the project says about its own encoding, for the dialog's "declare it" offer.
#[derive(Serialize)]
pub struct ProjectFacts {
    /// What Bennu reads the project in today.
    pub declared: String,
    /// A pom exists to write `sourceEncoding` into.
    pub can_declare: bool,
    /// The targets cover the whole project.
    pub whole_project: bool,
    /// The target's canonical name.
    pub to: String,
}

/// The dry run.
#[derive(Serialize)]
pub struct ConvertPlan {
    /// Every file that is not `unchanged`, problems first, capped at [`MAX_PLAN_ROWS`].
    pub files: Vec<FilePlan>,
    pub truncated: bool,
    pub total_files: usize,
    pub to_convert: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub blocked: usize,
    pub unrepresentable_total: usize,
    pub project: ProjectFacts,
}

/// A file a conversion could not write.
#[derive(Serialize)]
pub struct Failure {
    pub file: String,
    pub error: String,
}

/// What a conversion did.
#[derive(Serialize)]
pub struct ApplyResult {
    pub run_id: String,
    pub converted: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub failed: Vec<Failure>,
    pub cancelled: bool,
    /// Every file that was rewritten (absolute, forward slashes) — the frontend reloads the open ones.
    pub converted_files: Vec<String>,
    /// How many poms now declare the target as their source encoding.
    pub poms_declared: usize,
    /// Why `declare_project` was not honoured, when it was asked and not done.
    pub declare_note: Option<String>,
    pub backup_dir: String,
}

/// What an undo did.
#[derive(Serialize)]
pub struct UndoResult {
    pub run_id: String,
    pub restored: Vec<String>,
    /// Files changed after the conversion, left as they are.
    pub skipped: Vec<String>,
}

// ── the run record ───────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct Manifest {
    run_id: String,
    root: String,
    to: String,
    created_ms: u64,
    files: Vec<ManifestFile>,
    config_before: Vec<ConfigBefore>,
    #[serde(default)]
    undone: bool,
}

#[derive(Serialize, Deserialize)]
struct ManifestFile {
    file: String,
    rel: String,
    /// The file's stamp right after the conversion; an undo refuses a file whose stamp moved.
    stamp_after: String,
}

/// An `encoding_overrides` entry as it was before the run (`None`: it did not exist).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct ConfigBefore {
    key: String,
    value: Option<String>,
}

// ── resolving a request ──────────────────────────────────────────────────────────────────────

struct Resolved {
    root: PathBuf,
    files: Vec<PathBuf>,
    /// Forced source encoding, or `None` to read each file as Bennu does.
    from: Option<Codec>,
    to: Codec,
    bom: BomPolicy,
    fallback: Fallback,
    rewrite: bool,
    plan: EncodingPlan,
    whole_project: bool,
    has_pom: bool,
}

fn forward(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn resolve(req: &ConvertRequest) -> Result<Resolved, String> {
    let root = PathBuf::from(&req.root);
    if !root.is_dir() {
        return Err(format!("{} is not a directory", req.root));
    }
    let to = Codec::parse(&req.to).ok_or_else(|| format!("`{}` is not an encoding Bennu can write", req.to))?;
    let from = match req.from.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(label) => Some(Codec::parse(label).ok_or_else(|| format!("`{label}` is not an encoding Bennu can read"))?),
        None => None,
    };
    let bom = BomPolicy::parse(&req.bom).ok_or_else(|| format!("unknown BOM policy `{}`", req.bom))?;
    let fallback =
        Fallback::parse(&req.fallback).ok_or_else(|| format!("unknown fallback `{}`", req.fallback))?;

    let has_pom = root.join("pom.xml").is_file();
    // A Cargo root is UTF-8 by the language's definition — no pom to read a declared encoding from.
    let from = from.or_else(|| (!has_pom && root.join("Cargo.toml").is_file()).then_some(Codec::Utf8));

    let root_key = normalise_key(&req.root);
    let mut whole_project = false;
    let mut files: Vec<PathBuf> = Vec::new();
    for target in &req.targets {
        let key = normalise_key(target);
        if !(key == root_key || key.starts_with(&format!("{}/", root_key.trim_end_matches('/')))) {
            return Err(format!("{target} is outside the project"));
        }
        let path = PathBuf::from(target);
        if path.is_dir() {
            whole_project |= key == root_key;
            files.extend(crate::find::collect_text_paths(&path));
        } else if path.is_file() {
            files.push(path);
        } else {
            return Err(format!("{target} does not exist"));
        }
    }
    files.sort();
    files.dedup();

    Ok(Resolved {
        root,
        files,
        from,
        to,
        bom,
        fallback,
        rewrite: req.rewrite_declarations,
        plan: crate::index_service::encoding_plan(&req.root),
        whole_project,
        has_pom,
    })
}

fn rel_of(root: &Path, path: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(root) {
        return forward(rel);
    }
    let (r, p) = (forward(root), forward(path));
    let (rl, pl) = (r.to_lowercase(), p.to_lowercase());
    match pl.strip_prefix(&format!("{}/", rl.trim_end_matches('/'))) {
        Some(_) => p[r.trim_end_matches('/').len() + 1..].to_string(),
        None => path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(p),
    }
}

// ── analysing one file ───────────────────────────────────────────────────────────────────────

struct Analysis {
    row: FilePlan,
    /// `(before, after)` bytes, kept only for the apply and only for a file that will be written.
    bytes: Option<(Vec<u8>, Vec<u8>)>,
}

fn sample_of(u: &bennu_project::prelude::Unrepresentable) -> Sample {
    Sample { line: u.line, column: u.column, ch: u.ch.to_string(), code: format!("U+{:04X}", u.ch as u32) }
}

fn analyse(path: &Path, r: &Resolved, keep_bytes: bool) -> Analysis {
    let mut row = FilePlan {
        file: forward(path),
        rel: rel_of(&r.root, path),
        status: "unreadable",
        from: String::new(),
        detected: String::new(),
        size_before: 0,
        size_after: 0,
        bom_before: false,
        bom_after: false,
        unrepresentable_total: 0,
        samples: Vec::new(),
        declaration: None,
        declaration_rewritten: false,
        message: None,
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            row.message = Some(e.to_string());
            return Analysis { row, bytes: None };
        }
    };
    row.size_before = bytes.len() as u64;
    row.detected = detect_encoding(&bytes).codec.name().to_string();

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if BINARY_EXTS.iter().any(|b| b.eq_ignore_ascii_case(ext)) {
        row.status = "binary";
        row.message = Some("binary file".into());
        return Analysis { row, bytes: None };
    }
    let from = match r.from {
        Some(c) => c,
        None => {
            let label = r.plan.label_for(path);
            match Codec::parse(label) {
                Some(c) => c,
                None => {
                    row.status = "unknown-encoding";
                    row.from = label.to_string();
                    row.message = Some(format!("`{label}` is not an encoding Bennu can read; give the source encoding explicitly"));
                    return Analysis { row, bytes: None };
                }
            }
        }
    };
    row.from = from.name().to_string();

    let opts = ConvertOptions {
        from,
        to: r.to,
        bom: r.bom,
        fallback: r.fallback,
        escape: EscapeStyle::for_extension(ext),
        rewrite_declaration: r.rewrite,
    };
    match convert_encoding(&bytes, &opts) {
        Ok(done) => finish_ok(row, bytes, done, keep_bytes),
        Err(ConvertError::Binary) => {
            row.status = "binary";
            row.message = Some("binary file".into());
            Analysis { row, bytes: None }
        }
        Err(e @ ConvertError::InvalidSource { .. }) => {
            row.status = "invalid";
            row.message = Some(format!("{e} — read as {}", from.name()));
            Analysis { row, bytes: None }
        }
        Err(ConvertError::Unrepresentable { total, samples }) => {
            row.status = "blocked";
            row.unrepresentable_total = total;
            row.samples = samples.iter().map(sample_of).collect();
            row.message = Some(format!("{total} character{} {} cannot hold", if total == 1 { "" } else { "s" }, r.to.name()));
            Analysis { row, bytes: None }
        }
    }
}

fn finish_ok(mut row: FilePlan, before: Vec<u8>, done: Converted, keep_bytes: bool) -> Analysis {
    row.size_after = done.bytes.len() as u64;
    row.bom_before = done.bom_before;
    row.bom_after = done.bom_after;
    row.unrepresentable_total = done.unrepresentable_total;
    row.samples = done.samples.iter().map(sample_of).collect();
    row.declaration = done.declaration_before.clone();
    row.declaration_rewritten = done.declaration_rewritten;
    if done.bytes == before {
        row.status = "unchanged";
        return Analysis { row, bytes: None };
    }
    row.status = "convert";
    let bytes = keep_bytes.then_some((before, done.bytes));
    Analysis { row, bytes }
}

// ── the preview ──────────────────────────────────────────────────────────────────────────────

/// Preview a conversion: what would change, what would be skipped, what would not fit. Writes nothing.
#[arbor_rpc::handler]
async fn bennu_encoding_convert_plan(ctx: &BennuState, args: ConvertRequest) -> Result<ConvertPlan, String> {
    // Deferred: a preview of three files should not flash a row in the Jobs panel.
    let task = Task::start(
        ctx.host_caller().as_ref(),
        &ctx.event_sink(),
        TaskSpec::new("Preview encoding conversion", args.root.clone())
            .category("Encoding")
            .deferred(Duration::from_millis(600)),
    );
    tokio::task::spawn_blocking(move || {
        let result = plan_of(&args, &task);
        task.finish(result.as_ref().map(|_| ()).map_err(Clone::clone));
        result
    })
    .await
    .map_err(|e| format!("encoding preview: {e}"))?
}

fn plan_of(req: &ConvertRequest, task: &Task) -> Result<ConvertPlan, String> {
    let r = resolve(req)?;
    let done = AtomicUsize::new(0);
    let total = r.files.len();
    set_workers();
    let rows: Vec<FilePlan> = bennu_intel::prelude::parallel_map(&r.files, |p| {
        let row = analyse(p, &r, false).row;
        task.progress("Reading", done.fetch_add(1, Ordering::Relaxed) + 1, total);
        row
    });

    let count = |s: &str| rows.iter().filter(|f| f.status == s).count();
    let (to_convert, unchanged, blocked) = (count("convert"), count("unchanged"), count("blocked"));
    let skipped = rows.len() - to_convert - unchanged - blocked;
    let unrepresentable_total = rows.iter().map(|f| f.unrepresentable_total).sum();

    let mut shown: Vec<FilePlan> = rows.into_iter().filter(|f| f.status != "unchanged").collect();
    shown.sort_by_key(|f| status_rank(f.status));
    let truncated = shown.len() > MAX_PLAN_ROWS;
    shown.truncate(MAX_PLAN_ROWS);

    Ok(ConvertPlan {
        files: shown,
        truncated,
        total_files: total,
        to_convert,
        unchanged,
        skipped,
        blocked,
        unrepresentable_total,
        project: ProjectFacts {
            declared: r.plan.default_label().to_string(),
            can_declare: r.has_pom,
            whole_project: r.whole_project,
            to: r.to.name().to_string(),
        },
    })
}

/// Problems first: they are what the person has to look at.
fn status_rank(status: &str) -> u8 {
    match status {
        "blocked" => 0,
        "invalid" | "unknown-encoding" | "unreadable" => 1,
        "convert" => 2,
        _ => 3,
    }
}

fn set_workers() {
    bennu_intel::prelude::set_background_workers(bennu_core::config::load().index_threads);
}

// ── the conversion ───────────────────────────────────────────────────────────────────────────

enum Outcome {
    Written,
    Unchanged,
    Skipped,
    Failed(String),
    Cancelled,
}

/// Convert the files. Returns what was done; a file that could not be written is reported, not fatal.
#[arbor_rpc::handler]
async fn bennu_encoding_convert_apply(ctx: &BennuState, args: ApplyArgs) -> Result<ApplyResult, String> {
    let task = Task::start(
        ctx.host_caller().as_ref(),
        &ctx.event_sink(),
        TaskSpec::new("Convert encoding", format!("→ {} · {}", args.request.to, args.request.root))
            .category("Encoding")
            .cancellable(),
    );
    task.on_cancel(|| {});
    tokio::task::spawn_blocking(move || {
        let result = apply(&args, &task);
        task.finish(result.as_ref().map(|_| ()).map_err(Clone::clone));
        result
    })
    .await
    .map_err(|e| format!("encoding conversion: {e}"))?
}

fn apply(args: &ApplyArgs, task: &Task) -> Result<ApplyResult, String> {
    let r = resolve(&args.request)?;
    let root_str = args.request.root.clone();
    set_workers();

    // Abort means abort: a tree half in one encoding and half in another is worse than either, so a
    // single file that cannot be converted stops the run before anything is written.
    if r.fallback == Fallback::Abort {
        task.line("Checking that every file fits…");
        let blocked = bennu_intel::prelude::parallel_map(&r.files, |p| analyse(p, &r, false).row.status == "blocked")
            .into_iter()
            .filter(|b| *b)
            .count();
        if blocked > 0 {
            return Err(format!(
                "{blocked} file{} contain characters {} cannot hold, so nothing was written. \
                 Preview the conversion to see them, or choose to replace or escape them.",
                if blocked == 1 { "" } else { "s" },
                r.to.name()
            ));
        }
    }

    let run_id = format!("{:013x}", now_ms());
    let run_dir = backups_dir(&root_str).join(&run_id);
    let total = r.files.len();
    let done = AtomicUsize::new(0);
    task.line(&format!("Converting {total} file{} to {}…", if total == 1 { "" } else { "s" }, r.to.name()));

    let outcomes: Vec<(PathBuf, Outcome)> = bennu_intel::prelude::parallel_map(&r.files, |path| {
        let outcome = convert_one(path, &r, &run_dir, task);
        task.progress("Converting", done.fetch_add(1, Ordering::Relaxed) + 1, total);
        (path.clone(), outcome)
    });

    let mut written: Vec<PathBuf> = Vec::new();
    let (mut unchanged, mut skipped, mut cancelled) = (0usize, 0usize, false);
    let mut failed: Vec<Failure> = Vec::new();
    let mut pom_candidates: Vec<PathBuf> = Vec::new();
    for (path, outcome) in outcomes {
        let is_pom = path.file_name().is_some_and(|n| n == "pom.xml");
        match outcome {
            Outcome::Written => {
                written.push(path.clone());
                if is_pom {
                    pom_candidates.push(path);
                }
            }
            Outcome::Unchanged => {
                unchanged += 1;
                if is_pom {
                    pom_candidates.push(path);
                }
            }
            Outcome::Skipped => skipped += 1,
            Outcome::Failed(error) => failed.push(Failure { file: forward(&path), error }),
            Outcome::Cancelled => cancelled = true,
        }
    }

    // Declaring the encoding in the pom(s). Only a pom that is itself in the target encoding now.
    let (mut declared_in, mut declare_note) = (Vec::new(), None);
    if args.declare_project {
        if !r.whole_project {
            declare_note = Some("The project's encoding is only declared when the whole project is converted.".to_string());
        } else if !r.has_pom {
            declare_note = Some("This project has no pom.xml to declare the encoding in.".to_string());
        } else if cancelled {
            declare_note = Some("The run was cancelled before the pom could be updated.".to_string());
        } else {
            for pom in &pom_candidates {
                let is_root_pom = pom.parent().is_some_and(|p| normalise_key(&forward(p)) == normalise_key(&root_str));
                match declare_in_pom(pom, &r, &run_dir, is_root_pom) {
                    Ok(true) => {
                        if !written.contains(pom) {
                            written.push(pom.clone());
                        }
                        declared_in.push(pom.clone());
                    }
                    Ok(false) => {}
                    Err(e) => failed.push(Failure { file: forward(pom), error: e }),
                }
            }
        }
    }
    let poms_declared = declared_in.len();

    let converted_files: Vec<String> = written.iter().map(|p| forward(p)).collect();
    let config_before = move_the_rule(&root_str, &converted_files, &r, poms_declared > 0 || project_already_declares(&r));
    write_manifest(&run_dir, &run_id, &root_str, &r, &written, config_before);
    record_history(&root_str, &run_dir, &written, &run_id, &r);
    prune_backups(&root_str);

    let converted = converted_files.len();
    task.line(&format!("Done: {converted} converted, {unchanged} already right, {skipped} skipped."));
    Ok(ApplyResult {
        run_id,
        converted,
        unchanged,
        skipped,
        failed,
        cancelled,
        converted_files,
        poms_declared,
        declare_note,
        backup_dir: forward(&run_dir),
    })
}

/// Back up, then rewrite, one file.
fn convert_one(path: &Path, r: &Resolved, run_dir: &Path, task: &Task) -> Outcome {
    if task.is_cancelled() {
        return Outcome::Cancelled;
    }
    let analysis = analyse(path, r, true);
    match analysis.row.status {
        "convert" => {}
        "unchanged" => return Outcome::Unchanged,
        _ => return Outcome::Skipped,
    }
    let Some((before, after)) = analysis.bytes else { return Outcome::Skipped };
    // The copy comes first and a failure stops the file: a rewrite with no way back is not offered.
    if let Err(e) = backup_put(run_dir, &analysis.row.rel, &before) {
        return Outcome::Failed(format!("backup failed: {e}"));
    }
    match std::fs::write(path, &after) {
        Ok(()) => Outcome::Written,
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

/// Set (or, for a module pom that already declares one, update) `project.build.sourceEncoding` in
/// `pom` to the target. `Ok(true)` when the file changed.
fn declare_in_pom(pom: &Path, r: &Resolved, run_dir: &Path, add_if_missing: bool) -> Result<bool, String> {
    let bytes = std::fs::read(pom).map_err(|e| e.to_string())?;
    let had_bom = bennu_project::prelude::detect_bom(&bytes).map(|(c, _)| c) == Some(r.to);
    let as_utf8 = |from: Codec, to: Codec, bom: BomPolicy, data: &[u8]| {
        convert_encoding(
            data,
            &ConvertOptions {
                from,
                to,
                bom,
                fallback: Fallback::Abort,
                escape: EscapeStyle::None,
                rewrite_declaration: false,
            },
        )
        .map_err(|e| e.to_string())
    };
    let text = String::from_utf8(as_utf8(r.to, Codec::Utf8, BomPolicy::Remove, &bytes)?.bytes)
        .map_err(|e| e.to_string())?;
    if !add_if_missing && parse_pom(&text).property(SOURCE_ENCODING_KEY).is_none() {
        return Ok(false);
    }
    let Some(edit) = bennu_pomedit::prelude::set_property(&text, SOURCE_ENCODING_KEY, r.to.java_name()) else {
        return Ok(false);
    };
    let updated = bennu_pomedit::prelude::apply(&text, &[edit]);
    let bom = if had_bom { BomPolicy::Add } else { BomPolicy::Remove };
    let out = as_utf8(Codec::Utf8, r.to, bom, updated.as_bytes())?.bytes;
    backup_put_if_absent(run_dir, &rel_of(&r.root, pom), &bytes)?;
    std::fs::write(pom, out).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Whether the project already reads as the target through its pom (so no root override is needed).
fn project_already_declares(r: &Resolved) -> bool {
    Codec::parse(r.plan.default_label()) == Some(r.to) && r.has_pom
}

// ── the rule that decides how a file is read ─────────────────────────────────────────────────

/// Update `encoding_overrides` so the converted files are READ in their new encoding, and return
/// what each touched key was before. See the module docs for why a conversion has to do this.
fn move_the_rule(root: &str, converted: &[String], r: &Resolved, declared: bool) -> Vec<ConfigBefore> {
    let mut cfg = bennu_core::config::load();
    let project_label = r.plan.default_label().to_string();
    let before = reconcile_overrides(&mut cfg.encoding_overrides, root, converted, r.to, r.whole_project, declared, &project_label);
    if !before.is_empty() {
        if let Err(e) = bennu_core::config::save(&cfg) {
            eprintln!("[bennu] could not save encoding overrides after a conversion: {e}");
        }
    }
    before
}

/// The pure part of [`move_the_rule`].
///
/// * Whole project converted: the project's encoding becomes `to` — through the pom when it was
///   declared there (any project override is dropped so the pom shows through), else as a project
///   override — and the per-file overrides of the converted files, now redundant, go.
/// * Part of it: the project's encoding is untouched, so each converted file gets a per-file
///   override unless the project already reads as `to`.
fn reconcile_overrides(
    map: &mut BTreeMap<String, String>,
    root: &str,
    converted: &[String],
    to: Codec,
    whole_project: bool,
    declared: bool,
    project_label: &str,
) -> Vec<ConfigBefore> {
    let mut before = Vec::new();
    if whole_project {
        set_key(map, &mut before, root, (!declared).then(|| to.java_name()));
        for file in converted {
            set_key(map, &mut before, file, None);
        }
    } else {
        let project_is_target = Codec::parse(project_label) == Some(to);
        for file in converted {
            set_key(map, &mut before, file, (!project_is_target).then(|| to.java_name()));
        }
    }
    before
}

/// Set (`Some`) or remove (`None`) the entry for `key`, matching an existing one by its normalised
/// spelling, and remember what it was the first time it is touched.
fn set_key(map: &mut BTreeMap<String, String>, before: &mut Vec<ConfigBefore>, key: &str, value: Option<&str>) {
    let want = normalise_key(key);
    let existing = map.keys().find(|k| normalise_key(k) == want).cloned();
    let prior = existing.as_ref().and_then(|k| map.get(k)).cloned();
    if prior.as_deref() == value {
        return;
    }
    let actual = existing.unwrap_or_else(|| key.to_string());
    if !before.iter().any(|b| normalise_key(&b.key) == want) {
        before.push(ConfigBefore { key: actual.clone(), value: prior });
    }
    match value {
        Some(v) => {
            map.insert(actual, v.to_string());
        }
        None => {
            map.remove(&actual);
        }
    }
}

// ── backups, manifest, history ───────────────────────────────────────────────────────────────

fn fnv1a(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for b in text.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Where this project's conversion backups live — in the profile, not in the repository.
fn backups_dir(root: &str) -> PathBuf {
    arbor_core::prelude::bennu_data_dir()
        .join("encoding-backups")
        .join(format!("{:016x}", fnv1a(&normalise_key(root))))
}

fn backup_path(run_dir: &Path, rel: &str) -> PathBuf {
    run_dir.join("files").join(rel)
}

fn backup_put(run_dir: &Path, rel: &str, bytes: &[u8]) -> Result<(), String> {
    let dest = backup_path(run_dir, rel);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(dest, bytes).map_err(|e| e.to_string())
}

fn backup_put_if_absent(run_dir: &Path, rel: &str, bytes: &[u8]) -> Result<(), String> {
    if backup_path(run_dir, rel).is_file() {
        return Ok(());
    }
    backup_put(run_dir, rel, bytes)
}

fn write_manifest(
    run_dir: &Path,
    run_id: &str,
    root: &str,
    r: &Resolved,
    written: &[PathBuf],
    config_before: Vec<ConfigBefore>,
) {
    let manifest = Manifest {
        run_id: run_id.to_string(),
        root: root.to_string(),
        to: r.to.name().to_string(),
        created_ms: now_ms(),
        files: written
            .iter()
            .map(|p| ManifestFile { file: forward(p), rel: rel_of(&r.root, p), stamp_after: file_stamp(p) })
            .collect(),
        config_before,
        undone: false,
    };
    save_manifest(run_dir, &manifest);
}

fn save_manifest(run_dir: &Path, manifest: &Manifest) {
    let Ok(json) = serde_json::to_vec_pretty(manifest) else { return };
    let _ = std::fs::create_dir_all(run_dir);
    if let Err(e) = std::fs::write(run_dir.join("manifest.json"), json) {
        eprintln!("[bennu] could not write the encoding conversion manifest: {e}");
    }
}

fn load_manifest(run_dir: &Path) -> Option<Manifest> {
    serde_json::from_slice(&std::fs::read(run_dir.join("manifest.json")).ok()?).ok()
}

/// One local-history revision per file, all under the same change id, so the History view lists the
/// conversion as ONE operation. The `before` side is read back from the backup (a baseline for a file
/// history has never seen) rather than held in memory for the whole run.
fn record_history(root: &str, run_dir: &Path, written: &[PathBuf], run_id: &str, r: &Resolved) {
    let title = match r.from {
        Some(from) => format!("Convert encoding {} → {}", from.name(), r.to.name()),
        None => format!("Convert encoding → {}", r.to.name()),
    };
    let ctx = RecordCtx::titled(run_id.to_string(), title);
    for path in written {
        let before = std::fs::read(backup_path(run_dir, &rel_of(&r.root, path))).ok();
        crate::history::on_write(root, &forward(path), before, &ctx);
    }
}

/// Drop other runs' backups older than [`BACKUP_KEEP`].
fn prune_backups(root: &str) {
    let Ok(entries) = std::fs::read_dir(backups_dir(root)) else { return };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > BACKUP_KEEP);
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

// ── undo ─────────────────────────────────────────────────────────────────────────────────────

/// Put the files of a conversion back as they were, and the encoding rule with them.
#[arbor_rpc::handler]
async fn bennu_encoding_convert_undo(ctx: &BennuState, args: UndoArgs) -> Result<UndoResult, String> {
    let task = Task::start(
        ctx.host_caller().as_ref(),
        &ctx.event_sink(),
        TaskSpec::new("Undo encoding conversion", args.root.clone())
            .category("Encoding")
            .deferred(Duration::from_millis(600)),
    );
    tokio::task::spawn_blocking(move || {
        let result = undo(&args, &task);
        task.finish(result.as_ref().map(|_| ()).map_err(Clone::clone));
        result
    })
    .await
    .map_err(|e| format!("encoding undo: {e}"))?
}

fn undo(args: &UndoArgs, task: &Task) -> Result<UndoResult, String> {
    let (run_dir, mut manifest) = find_run(&args.root, args.run_id.as_deref())?;
    let total = manifest.files.len();
    let ctx = RecordCtx::titled(format!("{}-undo", manifest.run_id), format!("Undo encoding conversion → {}", manifest.to));
    let (mut restored, mut skipped) = (Vec::new(), Vec::new());
    for (i, entry) in manifest.files.iter().enumerate() {
        let path = Path::new(&entry.file);
        // Somebody wrote this file after the conversion: restoring would throw their work away.
        if file_stamp(path) != entry.stamp_after {
            skipped.push(entry.file.clone());
            continue;
        }
        let Ok(original) = std::fs::read(backup_path(&run_dir, &entry.rel)) else {
            skipped.push(entry.file.clone());
            continue;
        };
        match std::fs::write(path, &original) {
            Ok(()) => {
                crate::history::on_write(&args.root, &entry.file, None, &ctx);
                restored.push(entry.file.clone());
            }
            Err(_) => skipped.push(entry.file.clone()),
        }
        task.progress("Restoring", i + 1, total);
    }

    // The rule goes back with the bytes — but only for the files that did go back.
    let mut cfg = bennu_core::config::load();
    for before in &manifest.config_before {
        let belongs_to_skipped = skipped.iter().any(|s| normalise_key(s) == normalise_key(&before.key));
        if belongs_to_skipped {
            continue;
        }
        match &before.value {
            Some(v) => {
                cfg.encoding_overrides.insert(before.key.clone(), v.clone());
            }
            None => {
                let want = normalise_key(&before.key);
                cfg.encoding_overrides.retain(|k, _| normalise_key(k) != want);
            }
        }
    }
    if !manifest.config_before.is_empty() {
        bennu_core::config::save(&cfg)?;
    }

    manifest.undone = true;
    save_manifest(&run_dir, &manifest);
    Ok(UndoResult { run_id: manifest.run_id.clone(), restored, skipped })
}

/// The run to undo: the one named, else the newest that has not been undone.
fn find_run(root: &str, run_id: Option<&str>) -> Result<(PathBuf, Manifest), String> {
    let base = backups_dir(root);
    if let Some(id) = run_id {
        let dir = base.join(id);
        return load_manifest(&dir).map(|m| (dir, m)).ok_or_else(|| format!("No conversion {id} to undo."));
    }
    let mut runs: Vec<(PathBuf, Manifest)> = std::fs::read_dir(&base)
        .map_err(|_| "No encoding conversion to undo.".to_string())?
        .flatten()
        .filter_map(|e| load_manifest(&e.path()).map(|m| (e.path(), m)))
        .filter(|(_, m)| !m.undone)
        .collect();
    runs.sort_by_key(|(_, m)| m.created_ms);
    runs.pop().ok_or_else(|| "No encoding conversion to undo.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf8() -> Codec {
        Codec::Utf8
    }

    fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn a_partial_conversion_pins_each_file_to_its_new_encoding() {
        let mut m = map(&[]);
        let before = reconcile_overrides(&mut m, "C:/p", &["C:/p/src/A.java".into()], utf8(), false, false, "Cp1252");
        assert_eq!(m.get("C:/p/src/A.java").map(String::as_str), Some("UTF-8"));
        assert_eq!(before, vec![ConfigBefore { key: "C:/p/src/A.java".into(), value: None }]);
    }

    #[test]
    fn a_partial_conversion_to_the_projects_own_encoding_needs_no_pin() {
        let mut m = map(&[("C:/p/src/A.java", "Cp1252")]);
        let before = reconcile_overrides(&mut m, "C:/p", &["C:/p/src/A.java".into()], utf8(), false, false, "UTF-8");
        assert!(m.is_empty());
        assert_eq!(before[0].value.as_deref(), Some("Cp1252"));
    }

    #[test]
    fn converting_the_whole_project_declared_in_the_pom_drops_the_project_override() {
        let mut m = map(&[("C:/p", "Cp1252"), ("C:/p/src/A.java", "ISO-8859-1")]);
        reconcile_overrides(&mut m, "C:/p", &["C:/p/src/A.java".into()], utf8(), true, true, "Cp1252");
        assert!(m.is_empty());
    }

    #[test]
    fn converting_the_whole_project_undeclared_sets_the_project_override() {
        let mut m = map(&[]);
        reconcile_overrides(&mut m, "C:/p", &["C:/p/A.java".into()], utf8(), true, false, "Cp1252");
        assert_eq!(m.get("C:/p").map(String::as_str), Some("UTF-8"));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn keys_match_across_slash_styles_and_the_first_value_is_what_is_remembered() {
        let mut m = map(&[("C:\\p\\A.java", "Cp1252")]);
        let mut before = Vec::new();
        set_key(&mut m, &mut before, "C:/p/A.java", Some("UTF-8"));
        set_key(&mut m, &mut before, "C:/p/A.java", None);
        assert!(m.is_empty());
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].value.as_deref(), Some("Cp1252"));
    }

    #[test]
    fn relative_paths_survive_a_differently_cased_drive() {
        let rel = rel_of(Path::new("C:/proj"), Path::new("c:/proj/src/A.java"));
        assert_eq!(rel, "src/A.java");
        assert_eq!(rel_of(Path::new("/p"), Path::new("/p/src/A.java")), "src/A.java");
    }

    #[test]
    fn backup_folders_are_stable_per_project() {
        assert_eq!(fnv1a("abc"), fnv1a("abc"));
        assert_ne!(fnv1a("abc"), fnv1a("abd"));
    }

    #[test]
    fn problems_sort_before_files_that_will_simply_convert() {
        assert!(status_rank("blocked") < status_rank("convert"));
        assert!(status_rank("invalid") < status_rank("convert"));
        assert!(status_rank("convert") < status_rank("unchanged"));
    }
}
