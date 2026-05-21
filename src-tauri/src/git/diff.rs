use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use git2::{BlameOptions, DiffOptions, Oid, Patch, Repository};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::git::encoding::{decode_bytes, decode_with};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiffStats {
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFile {
    pub path: String,
    /// Populated on renames/copies.
    pub old_path: Option<String>,
    pub status: DiffStatus,
    pub hunks: Vec<DiffHunk>,
    pub stats: DiffStats,
    pub is_binary: bool,
    /// base64-encoded bytes for image files (old/new side).
    pub image_old: Option<String>,
    pub image_new: Option<String>,
    /// Encoding label used to decode the hunk lines. Detected from the
    /// new-side blob (or old-side for deletions). `None` for binary /
    /// image / untracked deltas where no decoding happened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    /// e.g. `@@ -10,6 +10,8 @@`
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "tiff"];

fn is_image_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    IMAGE_EXTS.iter().any(|e| lower.ends_with(e))
}

/// Mapping of delta path → encoding label (e.g. `"windows-1252"`). Frontend
/// supplies this to override auto-detection on a per-file basis. The
/// preferred lookup key is `delta.new_file().path()`; for deletions we
/// fall back to `old_file().path()`.
pub type EncodingOverrides = std::collections::HashMap<String, String>;

/// Resolve the encoding for a delta. Order:
///   1. Explicit override from the per-path map (user pinned via the pill).
///   2. Auto-detection on the new-side blob.
///   3. Auto-detection on the old-side blob (used for deletions).
///   4. UTF-8.
fn resolve_delta_encoding(
    repo: &Repository,
    delta: &git2::DiffDelta,
    overrides: Option<&EncodingOverrides>,
) -> &'static encoding_rs::Encoding {
    if let Some(map) = overrides {
        let path = delta.new_file().path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string());
        if let Some(p) = path.as_deref() {
            if let Some(label) = map.get(p) {
                return crate::git::encoding::encoding_for_label(label);
            }
        }
    }

    let try_blob = |oid: Oid| -> Option<&'static encoding_rs::Encoding> {
        if oid.is_zero() { return None; }
        let blob = repo.find_blob(oid).ok()?;
        Some(crate::git::encoding::detect(blob.content()))
    };
    try_blob(delta.new_file().id())
        .or_else(|| try_blob(delta.old_file().id()))
        .unwrap_or(encoding_rs::UTF_8)
}

fn delta_status(s: git2::Delta) -> DiffStatus {
    match s {
        git2::Delta::Added     => DiffStatus::Added,
        git2::Delta::Deleted   => DiffStatus::Deleted,
        git2::Delta::Modified  => DiffStatus::Modified,
        git2::Delta::Renamed   => DiffStatus::Renamed,
        git2::Delta::Copied    => DiffStatus::Copied,
        git2::Delta::Untracked => DiffStatus::Untracked,
        // Conflicted, Ignored, Typechange, Unreadable, Unmodified — treat as Modified
        // and log so we notice any unexpected delta types during development.
        other => {
            tracing::debug!("unhandled git2::Delta variant {other:?}, mapping to Modified");
            DiffStatus::Modified
        }
    }
}

// ---------------------------------------------------------------------------
// Core parser — works on any git2::Diff
// ---------------------------------------------------------------------------

/// Extract per-delta path/status metadata WITHOUT parsing hunks.
/// Used by streaming diff loaders to emit a fast "file list" phase before the
/// expensive hunk-parse phase.
pub fn parse_diff_meta(diff: &git2::Diff) -> Vec<DiffFile> {
    let num = diff.deltas().count();
    let mut files: Vec<DiffFile> = Vec::with_capacity(num);
    for i in 0..num {
        let Some(delta) = diff.get_delta(i) else { continue };
        let new_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let old_path_raw = delta
            .old_file()
            .path()
            .map(|p| p.to_string_lossy().to_string());
        let old_path = match delta.status() {
            git2::Delta::Renamed | git2::Delta::Copied => old_path_raw,
            _ => None,
        };
        let status = delta_status(delta.status());
        let is_binary = delta.new_file().is_binary() || delta.old_file().is_binary();
        files.push(DiffFile {
            path: new_path,
            old_path,
            status,
            hunks: Vec::new(),
            stats: DiffStats::default(),
            is_binary,
            image_old: None,
            image_new: None,
            // Meta phase doesn't read content — encoding is filled in by
            // the hunk-parse phase.
            encoding: None,
        });
    }
    files
}

/// Parse a single delta (by index) into a fully-populated DiffFile (with hunks,
/// stats, and image content for binary image files).  Complements
/// `parse_diff_meta` for the streaming parse path.
pub fn parse_diff_one(
    repo: &Repository,
    diff: &git2::Diff,
    i: usize,
    overrides: Option<&EncodingOverrides>,
) -> Result<DiffFile> {
    let delta = diff
        .get_delta(i)
        .ok_or_else(|| AppError::Other(format!("delta {i} missing")))?;

    let new_path = delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let old_path_raw = delta
        .old_file()
        .path()
        .map(|p| p.to_string_lossy().to_string());
    let old_path = match delta.status() {
        git2::Delta::Renamed | git2::Delta::Copied => old_path_raw,
        _ => None,
    };

    let status = delta_status(delta.status());
    let is_binary = delta.new_file().is_binary() || delta.old_file().is_binary();

    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut stats = DiffStats::default();
    let mut image_old: Option<String> = None;
    let mut image_new: Option<String> = None;
    let mut encoding_label: Option<String> = None;

    if is_binary && is_image_path(&new_path) {
        let read_blob_b64 = |oid: Oid| -> Option<String> {
            if oid.is_zero() { return None; }
            repo.find_blob(oid).ok().map(|b| B64.encode(b.content()))
        };
        image_old = read_blob_b64(delta.old_file().id());
        image_new = read_blob_b64(delta.new_file().id());
    } else if !is_binary {
        let enc = resolve_delta_encoding(repo, &delta, overrides);
        encoding_label = Some(enc.name().to_string());
        if let Ok(Some(patch)) = Patch::from_diff(diff, i) {
            for h in 0..patch.num_hunks() {
                if let Ok((hunk, _num_lines)) = patch.hunk(h) {
                    let num_lines = patch.num_lines_in_hunk(h).unwrap_or(0);
                    let mut lines: Vec<DiffLine> = Vec::with_capacity(num_lines);
                    for l in 0..num_lines {
                        if let Ok(line) = patch.line_in_hunk(h, l) {
                            let (kind, old_lineno, new_lineno) = match line.origin_value() {
                                git2::DiffLineType::Addition => {
                                    stats.additions += 1;
                                    (LineKind::Added, None, line.new_lineno())
                                }
                                git2::DiffLineType::Deletion => {
                                    stats.deletions += 1;
                                    (LineKind::Removed, line.old_lineno(), None)
                                }
                                // Skip the synthetic "\ No newline at end of file" markers
                                // and any other pseudo-lines: they are NOT real file
                                // content and including them as context breaks
                                // partial-stage patch application (the exact marker text
                                // doesn't exist in the index).
                                git2::DiffLineType::ContextEOFNL
                                | git2::DiffLineType::AddEOFNL
                                | git2::DiffLineType::DeleteEOFNL => continue,
                                _ => (LineKind::Context, line.old_lineno(), line.new_lineno()),
                            };
                            lines.push(DiffLine {
                                kind,
                                old_lineno,
                                new_lineno,
                                content: decode_with(line.content(), enc),
                            });
                        }
                    }
                    hunks.push(DiffHunk {
                        // Headers are always ASCII (`@@ -1,5 +1,5 @@`) — the
                        // file encoding doesn't apply, so plain UTF-8 lossy
                        // is fine here.
                        header: String::from_utf8_lossy(hunk.header()).into_owned(),
                        old_start: hunk.old_start(),
                        old_lines: hunk.old_lines(),
                        new_start: hunk.new_start(),
                        new_lines: hunk.new_lines(),
                        lines,
                    });
                }
            }
        }
    }

    Ok(DiffFile {
        path: new_path,
        old_path,
        status,
        hunks,
        stats,
        is_binary,
        image_old,
        image_new,
        encoding: encoding_label,
    })
}

pub fn parse_diff(
    repo: &Repository,
    diff: &git2::Diff,
    overrides: Option<&EncodingOverrides>,
) -> Result<Vec<DiffFile>> {
    let num = diff.deltas().count();
    let mut files: Vec<DiffFile> = Vec::with_capacity(num);

    for i in 0..num {
        let delta = diff
            .get_delta(i)
            .ok_or_else(|| AppError::Other(format!("delta {i} missing")))?;

        // For deleted files the new_file path may be absent in some libgit2
        // versions; fall back to old_file path so the entry is never lost.
        let new_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let old_path_raw = delta
            .old_file()
            .path()
            .map(|p| p.to_string_lossy().to_string());

        let old_path = match delta.status() {
            git2::Delta::Renamed | git2::Delta::Copied => old_path_raw,
            _ => None,
        };

        let status = delta_status(delta.status());
        let is_binary = delta.new_file().is_binary() || delta.old_file().is_binary();

        let mut hunks: Vec<DiffHunk> = Vec::new();
        let mut stats = DiffStats::default();
        let mut image_old: Option<String> = None;
        let mut image_new: Option<String> = None;
        let mut encoding_label: Option<String> = None;

        if is_binary && is_image_path(&new_path) {
            // Try to read raw bytes from blob objects
            let read_blob_b64 = |oid: Oid| -> Option<String> {
                if oid.is_zero() { return None; }
                repo.find_blob(oid).ok().map(|b| B64.encode(b.content()))
            };
            image_old = read_blob_b64(delta.old_file().id());
            image_new = read_blob_b64(delta.new_file().id());
        } else if !is_binary {
            let enc = resolve_delta_encoding(repo, &delta, overrides);
            encoding_label = Some(enc.name().to_string());
            if let Ok(Some(patch)) = Patch::from_diff(diff, i) {
                for h in 0..patch.num_hunks() {
                    if let Ok((hunk, _num_lines)) = patch.hunk(h) {
                        let num_lines = patch.num_lines_in_hunk(h).unwrap_or(0);
                        let mut lines: Vec<DiffLine> = Vec::with_capacity(num_lines);

                        for l in 0..num_lines {
                            if let Ok(line) = patch.line_in_hunk(h, l) {
                                let (kind, old_lineno, new_lineno) = match line.origin_value() {
                                    git2::DiffLineType::Addition => {
                                        stats.additions += 1;
                                        (LineKind::Added, None, line.new_lineno())
                                    }
                                    git2::DiffLineType::Deletion => {
                                        stats.deletions += 1;
                                        (LineKind::Removed, line.old_lineno(), None)
                                    }
                                    // Skip the synthetic "\ No newline at end of file"
                                    // markers — they break partial-stage patch apply
                                    // because the marker text isn't real file content.
                                    git2::DiffLineType::ContextEOFNL
                                    | git2::DiffLineType::AddEOFNL
                                    | git2::DiffLineType::DeleteEOFNL => continue,
                                    _ => (LineKind::Context, line.old_lineno(), line.new_lineno()),
                                };
                                lines.push(DiffLine {
                                    kind,
                                    old_lineno,
                                    new_lineno,
                                    content: decode_with(line.content(), enc),
                                });
                            }
                        }

                        hunks.push(DiffHunk {
                            // Headers are always ASCII — UTF-8 lossy is fine.
                            header: String::from_utf8_lossy(hunk.header()).into_owned(),
                            old_start: hunk.old_start(),
                            old_lines: hunk.old_lines(),
                            new_start: hunk.new_start(),
                            new_lines: hunk.new_lines(),
                            lines,
                        });
                    }
                }
            }
        }

        files.push(DiffFile {
            path: new_path,
            old_path,
            status,
            hunks,
            stats,
            is_binary,
            image_old,
            image_new,
            encoding: encoding_label,
        });
    }

    Ok(files)
}

// ---------------------------------------------------------------------------
// Commit diff
// ---------------------------------------------------------------------------

fn apply_algo(opts: &mut DiffOptions, algo: Option<&str>) {
    match algo {
        Some("patience") => { opts.patience(true); }
        Some("minimal")  => { opts.minimal(true); }
        _ => {} // myers is the default
    }
}

/// Lightweight per-file scan: produces `DiffFile` entries with status, paths,
/// `is_binary`, and **stats** populated, but no hunks. Used by the lazy
/// commit-diff loader so the UI can render the file list with +/- counters
/// without paying the cost of materialising hunks for every file (especially
/// painful when `full_file` is on).
///
/// The provided diff should have been built with `context_lines = 0` for
/// maximum speed — stats are computed by walking only the changed lines.
fn parse_diff_meta_stats(diff: &git2::Diff) -> Vec<DiffFile> {
    let num = diff.deltas().count();
    let mut files: Vec<DiffFile> = Vec::with_capacity(num);
    for i in 0..num {
        let Some(delta) = diff.get_delta(i) else { continue };
        let new_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let old_path_raw = delta
            .old_file()
            .path()
            .map(|p| p.to_string_lossy().to_string());
        let old_path = match delta.status() {
            git2::Delta::Renamed | git2::Delta::Copied => old_path_raw,
            _ => None,
        };
        let status = delta_status(delta.status());
        let is_binary = delta.new_file().is_binary() || delta.old_file().is_binary();

        // Patch::line_stats walks only the changed lines (we built the diff
        // with context=0). For binary deltas line_stats returns zeros, which
        // is what the UI expects (no per-line count shown for binaries).
        let mut stats = DiffStats::default();
        if !is_binary {
            if let Ok(Some(patch)) = Patch::from_diff(diff, i) {
                if let Ok((_ctx, adds, dels)) = patch.line_stats() {
                    stats.additions = adds;
                    stats.deletions = dels;
                }
            }
        }

        files.push(DiffFile {
            path: new_path,
            old_path,
            status,
            hunks: Vec::new(),
            stats,
            is_binary,
            image_old: None,
            image_new: None,
            encoding: None,
        });
    }
    files
}

/// Metadata-only commit diff (file list + stats, no hunks). Mirrors the
/// "lookup tree, diff against parent" logic of `get_commit_diff` but uses
/// `context_lines=0` internally so the patch walk is cheap regardless of
/// the user's `full_file` setting.
pub fn get_commit_diff_meta(
    repo: &Repository,
    oid_str: &str,
    algo: Option<&str>,
) -> Result<Vec<DiffFile>> {
    let oid =
        Oid::from_str(oid_str).map_err(|_| AppError::CommitNotFound(oid_str.to_string()))?;
    let commit = repo.find_commit(oid)?;
    let new_tree = commit.tree()?;

    let mut opts = DiffOptions::new();
    opts.context_lines(0);
    apply_algo(&mut opts, algo);

    let diff = if commit.parent_count() == 0 {
        repo.diff_tree_to_tree(None, Some(&new_tree), Some(&mut opts))?
    } else {
        let parent = commit.parent(0)?;
        let old_tree = parent.tree()?;
        repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts))?
    };

    let mut files = parse_diff_meta_stats(&diff);

    // Stash untracked commit (parent[2] when stashed with --include-untracked).
    if commit.parent_count() >= 3 {
        if let Ok(untracked_commit) = commit.parent(2) {
            if let Ok(untracked_tree) = untracked_commit.tree() {
                let mut u_opts = DiffOptions::new();
                u_opts.context_lines(0);
                apply_algo(&mut u_opts, algo);
                if let Ok(u_diff) = repo.diff_tree_to_tree(None, Some(&untracked_tree), Some(&mut u_opts)) {
                    files.extend(parse_diff_meta_stats(&u_diff));
                }
            }
        }
    }

    Ok(files)
}

/// Parse the hunks of a single file inside a commit. Returns the matching
/// `DiffFile` with hunks populated, or `Err` if no delta matches `path`.
/// Used by the lazy loader after `get_commit_diff_meta` has produced the
/// file list and the user picks which file to view.
pub fn get_commit_file_diff(
    repo: &Repository,
    oid_str: &str,
    path: &str,
    context_lines: u32,
    algo: Option<&str>,
    overrides: Option<&EncodingOverrides>,
) -> Result<DiffFile> {
    let oid =
        Oid::from_str(oid_str).map_err(|_| AppError::CommitNotFound(oid_str.to_string()))?;
    let commit = repo.find_commit(oid)?;
    let new_tree = commit.tree()?;

    let mut opts = DiffOptions::new();
    opts.context_lines(context_lines);
    opts.pathspec(path);
    apply_algo(&mut opts, algo);

    let diff = if commit.parent_count() == 0 {
        repo.diff_tree_to_tree(None, Some(&new_tree), Some(&mut opts))?
    } else {
        let parent = commit.parent(0)?;
        let old_tree = parent.tree()?;
        repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts))?
    };

    if let Some(i) = find_delta_index(&diff, path) {
        return parse_diff_one(repo, &diff, i, overrides);
    }

    // Fallback: maybe the path lives in the stash's untracked commit.
    if commit.parent_count() >= 3 {
        if let Ok(untracked_commit) = commit.parent(2) {
            if let Ok(untracked_tree) = untracked_commit.tree() {
                let mut u_opts = DiffOptions::new();
                u_opts.context_lines(context_lines);
                u_opts.pathspec(path);
                apply_algo(&mut u_opts, algo);
                let u_diff = repo.diff_tree_to_tree(None, Some(&untracked_tree), Some(&mut u_opts))?;
                if let Some(i) = find_delta_index(&u_diff, path) {
                    return parse_diff_one(repo, &u_diff, i, overrides);
                }
            }
        }
    }

    Err(AppError::Other(format!("file '{path}' not found in commit diff")))
}

/// Find the delta index in `diff` whose new- or old-file path matches `path`.
/// Used when a pathspec narrows the diff to one file but the index may not be 0
/// (e.g. when libgit2 still emits other paths in the deltas list).
fn find_delta_index(diff: &git2::Diff, path: &str) -> Option<usize> {
    let num = diff.deltas().count();
    for i in 0..num {
        let Some(delta) = diff.get_delta(i) else { continue };
        let np = delta.new_file().path().map(|p| p.to_string_lossy().to_string());
        let op = delta.old_file().path().map(|p| p.to_string_lossy().to_string());
        if np.as_deref() == Some(path) || op.as_deref() == Some(path) {
            return Some(i);
        }
    }
    None
}

pub fn get_commit_diff(
    repo: &Repository,
    oid_str: &str,
    context_lines: u32,
    algo: Option<&str>,
    overrides: Option<&EncodingOverrides>,
) -> Result<Vec<DiffFile>> {
    let oid =
        Oid::from_str(oid_str).map_err(|_| AppError::CommitNotFound(oid_str.to_string()))?;
    let commit = repo.find_commit(oid)?;
    let new_tree = commit.tree()?;

    let mut opts = DiffOptions::new();
    opts.ignore_whitespace_change(false)
        .context_lines(context_lines);
    apply_algo(&mut opts, algo);

    let diff = if commit.parent_count() == 0 {
        repo.diff_tree_to_tree(None, Some(&new_tree), Some(&mut opts))?
    } else {
        let parent = commit.parent(0)?;
        let old_tree = parent.tree()?;
        repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts))?
    };

    let mut files = parse_diff(repo, &diff, overrides)?;

    // Git stash with --include-untracked stores untracked files in a separate
    // "untracked" commit at parent[2].  Diff that commit from an empty tree so
    // those files appear as Added in the file list.
    if commit.parent_count() >= 3 {
        if let Ok(untracked_commit) = commit.parent(2) {
            if let Ok(untracked_tree) = untracked_commit.tree() {
                let mut u_opts = DiffOptions::new();
                u_opts.context_lines(context_lines);
                apply_algo(&mut u_opts, algo);
                if let Ok(u_diff) = repo.diff_tree_to_tree(None, Some(&untracked_tree), Some(&mut u_opts)) {
                    if let Ok(mut untracked_files) = parse_diff(repo, &u_diff, overrides) {
                        files.append(&mut untracked_files);
                    }
                }
            }
        }
    }

    Ok(files)
}

// ---------------------------------------------------------------------------
// Working directory / index diffs
// ---------------------------------------------------------------------------

/// Build (but do not parse) the workdir-or-index diff.  Shared between the
/// synchronous `get_workdir_diff` and the streaming variant so both honor the
/// same options.
pub fn build_workdir_diff<'a>(repo: &'a Repository, staged: bool, context_lines: u32, algo: Option<&str>) -> Result<git2::Diff<'a>> {
    let diff = if staged {
        // staged = index vs HEAD
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        let mut opts = DiffOptions::new();
        opts.context_lines(context_lines);
        apply_algo(&mut opts, algo);
        repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
    } else {
        // unstaged = workdir vs index (include untracked files with full content)
        let mut opts = DiffOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .show_untracked_content(true)
            .context_lines(context_lines);
        apply_algo(&mut opts, algo);
        repo.diff_index_to_workdir(None, Some(&mut opts))?
    };
    Ok(diff)
}

pub fn get_workdir_diff(
    repo: &Repository,
    staged: bool,
    context_lines: u32,
    algo: Option<&str>,
    overrides: Option<&EncodingOverrides>,
) -> Result<Vec<DiffFile>> {
    let diff = build_workdir_diff(repo, staged, context_lines, algo)?;
    parse_diff(repo, &diff, overrides)
}

// ---------------------------------------------------------------------------
// Branch-to-branch diff
// ---------------------------------------------------------------------------

/// Diff the tips of two arbitrary refs (branch names, remote tracking branches,
/// tags, or any revspec accepted by `revparse_single`).
///
/// `from_ref` is the base (e.g. "main"), `to_ref` is the comparison side
/// (e.g. "feature" or "origin/feature").  The returned diff shows what
/// changed going *from* `from_ref` *to* `to_ref`.
pub fn get_branch_diff(
    repo: &Repository,
    from_ref: &str,
    to_ref: &str,
    context_lines: u32,
    algo: Option<&str>,
    overrides: Option<&EncodingOverrides>,
) -> Result<Vec<DiffFile>> {
    let from_obj = repo
        .revparse_single(from_ref)
        .map_err(|_| AppError::Other(format!("ref not found: {from_ref}")))?;
    let from_commit = repo
        .find_commit(from_obj.id())
        .map_err(|_| AppError::Other(format!("not a commit: {from_ref}")))?;
    let from_tree = from_commit.tree()?;

    let to_obj = repo
        .revparse_single(to_ref)
        .map_err(|_| AppError::Other(format!("ref not found: {to_ref}")))?;
    let to_commit = repo
        .find_commit(to_obj.id())
        .map_err(|_| AppError::Other(format!("not a commit: {to_ref}")))?;
    let to_tree = to_commit.tree()?;

    let mut opts = DiffOptions::new();
    opts.context_lines(context_lines);
    apply_algo(&mut opts, algo);

    let diff = repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut opts))?;
    parse_diff(repo, &diff, overrides)
}

// ---------------------------------------------------------------------------
// File content at a specific commit
// ---------------------------------------------------------------------------

pub fn get_file_at_commit(
    repo: &Repository,
    oid_str: &str,
    path: &str,
    encoding_override: Option<&str>,
) -> Result<String> {
    let oid =
        Oid::from_str(oid_str).map_err(|_| AppError::CommitNotFound(oid_str.to_string()))?;
    let commit = repo.find_commit(oid)?;
    let tree = commit.tree()?;
    let entry = tree
        .get_path(std::path::Path::new(path))
        .map_err(|_| AppError::Other(format!("file '{path}' not found in commit")))?;
    let blob = repo.find_blob(entry.id())?;
    let text = match encoding_override {
        Some(label) => decode_with(blob.content(), crate::git::encoding::encoding_for_label(label)),
        None        => decode_bytes(blob.content()).0,
    };
    Ok(text)
}

// ---------------------------------------------------------------------------
// Git blame
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct BlameLine {
    /// 1-based line number in the file.
    pub line_no: usize,
    /// Raw line content (trailing newline stripped).
    pub content: String,
    pub commit_oid: String,
    pub short_oid: String,
    pub author_name: String,
    pub author_email: String,
    /// Unix timestamp (seconds) of the commit author date.
    pub timestamp: i64,
    /// First line of the commit message.
    pub summary: String,
    /// True on the first line that belongs to this commit group, used for
    /// grouping in the UI (the gutter info is rendered only on the first line).
    pub is_group_start: bool,
}

pub fn get_file_blame(repo: &Repository, path: &str) -> Result<Vec<BlameLine>> {
    // Run blame against HEAD.
    let mut opts = BlameOptions::new();
    opts.track_copies_same_commit_moves(false);
    let blame = repo
        .blame_file(std::path::Path::new(path), Some(&mut opts))
        .map_err(|e| AppError::Other(format!("blame failed: {e}")))?;

    // Read file content at HEAD to get line text.
    let head = repo.revparse_single("HEAD")?;
    let commit = repo.find_commit(head.id())?;
    let tree = commit.tree()?;
    let content_str = tree
        .get_path(std::path::Path::new(path))
        .ok()
        .and_then(|e| repo.find_blob(e.id()).ok())
        .map(|b| decode_bytes(b.content()).0)
        .unwrap_or_default();

    let file_lines: Vec<&str> = content_str.split('\n').collect();

    let mut result: Vec<BlameLine> = Vec::new();
    let mut prev_oid: Option<Oid> = None;

    for hunk in blame.iter() {
        let oid = hunk.final_commit_id();
        let start = hunk.final_start_line(); // 1-based
        let count = hunk.lines_in_hunk();

        let sig = hunk.final_signature();
        let author_name = sig.name().unwrap_or("Unknown").to_string();
        let author_email = sig.email().unwrap_or("").to_string();
        let timestamp = sig.when().seconds();

        // Try to get the commit summary; fall back gracefully.
        let (commit_oid_str, short_oid, summary) = if oid.is_zero() {
            ("0000000".repeat(1), "0000000".to_string(), "Uncommitted changes".to_string())
        } else {
            let full = oid.to_string();
            let short = full[..7.min(full.len())].to_string();
            let summ = repo
                .find_commit(oid)
                .ok()
                .and_then(|c| c.summary().map(|s| s.to_string()))
                .unwrap_or_default();
            (full, short, summ)
        };

        for i in 0..count {
            let line_no = start + i;
            let content = file_lines
                .get(line_no.saturating_sub(1))
                .copied()
                .unwrap_or("")
                .trim_end_matches('\r')
                .to_string();

            let is_group_start = i == 0 && prev_oid.map_or(true, |p| p != oid);

            result.push(BlameLine {
                line_no,
                content,
                commit_oid: commit_oid_str.clone(),
                short_oid: short_oid.clone(),
                author_name: author_name.clone(),
                author_email: author_email.clone(),
                timestamp,
                summary: summary.clone(),
                is_group_start,
            });
        }

        prev_oid = Some(oid);
    }

    Ok(result)
}
