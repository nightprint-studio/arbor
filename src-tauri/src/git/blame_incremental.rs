//! Streaming git blame via the system `git` binary (`blame --incremental`).
//!
//! libgit2's `blame_file` is all-or-nothing: it walks the entire history and
//! only then hands back the hunks, so on a large file with deep history the UI
//! can only show an indeterminate spinner for several seconds.  `git blame
//! --incremental` instead emits one block per commit *as the walk reaches it*,
//! and every block states how many final lines that commit owns — which is
//! exactly the signal we need to drive a *determinate* progress bar
//! (`attributed / total`).
//!
//! Total wall-clock is ≈ the same as the libgit2 path (the cost is the history
//! walk, not the transfer); the win is first feedback + a real percentage.
//!
//! This path is only taken when a `git` binary is available
//! ([`crate::git_cli::snapshot`]).  When it isn't, the caller falls back to the
//! libgit2 [`crate::git::diff::get_file_blame`] (no progress).
//!
//! ## `--incremental` format (per `git-blame(1)`)
//!
//! Each entry begins with a header line:
//! ```text
//! <40-hex-sha> <orig-line> <final-line> <num-lines>
//! ```
//! The *first* time a commit appears it is followed by tagged metadata lines
//! (`author`, `author-mail`, `author-time`, `summary`, …); repeated commits
//! carry only the header.  Every entry is terminated by a `filename ` line.
//! We don't pattern-match the header by shape — we track an `expect_header`
//! flag (true at start and after each `filename` line) so an author whose name
//! happens to look like a header can't fool the parser.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Stdio;

use crate::error::{AppError, Result};
use crate::git::diff::BlameLine;
use crate::git::encoding::decode_bytes;

/// Progress tick streamed to the frontend over the streaming seam (one
/// `arbor://blame-stream-chunk` event per tick) while the blame walk runs.
/// The producer here is agnostic to the egress: `run_incremental_blame` just
/// invokes an `Fn(BlameProgress)` callback, which the handler backs with
/// `Stream::chunk`.  `done` counts final-file lines attributed so far out of
/// `total`; the `current_*` fields describe the commit the walk is on (so the
/// spinner can show "risalendo la storia").
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameProgress {
    /// Lines attributed so far.
    pub done: usize,
    /// Total attributable lines in the file at HEAD.
    pub total: usize,
    /// Short OID of the commit just attributed (always present once the walk
    /// has produced at least one block).
    pub current_short: Option<String>,
    /// Author timestamp (unix seconds) of that commit, when known.
    pub current_date: Option<i64>,
    /// First line of that commit's message, when known.
    pub current_summary: Option<String>,
}

#[derive(Default, Clone)]
struct CommitMeta {
    author_name: String,
    author_email: String,
    timestamp: i64,
    summary: String,
}

/// Read the file content at HEAD (for line text + the attributable line count)
/// — mirrors what the libgit2 path does so both produce identical `content`.
fn read_head_content(repo: &git2::Repository, path: &str) -> String {
    repo.revparse_single("HEAD")
        .and_then(|h| repo.find_commit(h.id()))
        .and_then(|c| c.tree())
        .ok()
        .and_then(|tree| tree.get_path(Path::new(path)).ok())
        .and_then(|e| repo.find_blob(e.id()).ok())
        .map(|b| decode_bytes(b.content()).0)
        .unwrap_or_default()
}

/// Run `git blame --incremental` against HEAD and assemble `Vec<BlameLine>`,
/// invoking `on_progress` at every entry boundary (throttled to ~1% steps).
///
/// Blames the HEAD revision — not the working tree — to stay consistent with
/// the libgit2 path, whose content is read from the HEAD tree.
pub fn run_incremental_blame<F>(
    repo_path: &Path,
    path: &str,
    mut on_progress: F,
) -> Result<Vec<BlameLine>>
where
    F: FnMut(BlameProgress),
{
    // ── File content + attributable line count ────────────────────────────
    let repo = git2::Repository::open(repo_path)?;
    let content = read_head_content(&repo, path);
    let file_lines: Vec<&str> = content.split('\n').collect();
    // A file ending in '\n' yields a trailing empty element that git does NOT
    // count as a line; drop it from the denominator so `done == total` lands.
    let mut total = file_lines.len();
    if total > 0 && file_lines.last() == Some(&"") {
        total -= 1;
    }

    // ── Spawn the blame walk ──────────────────────────────────────────────
    let mut child = crate::git_cli::command()
        .current_dir(repo_path)
        .args(["blame", "--incremental", "HEAD", "--", path])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Other(format!("spawn git blame: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Other("git blame: no stdout pipe".into()))?;
    let reader = BufReader::new(stdout);

    // ── Parse state ───────────────────────────────────────────────────────
    // `owner[i]` = full OID attributed to final line i+1 (filled out of order,
    // since the walk emits newest-commit-first, not line order).
    let mut owner: Vec<Option<String>> = vec![None; total];
    let mut metas: HashMap<String, CommitMeta> = HashMap::new();
    let mut cur_oid: Option<String> = None;
    let mut expect_header = true;

    let mut done: usize = 0;
    let mut last_emit: usize = 0;
    let step = (total / 100).max(1); // ~1% granularity, never zero

    on_progress(BlameProgress {
        done: 0,
        total,
        current_short: None,
        current_date: None,
        current_summary: None,
    });

    for line in reader.lines() {
        let line = line.map_err(|e| AppError::Other(format!("git blame read: {e}")))?;

        if expect_header {
            // Header fields are [sha, orig-line, final-line, num-lines].
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 4 {
                if let (Ok(start), Ok(num)) =
                    (parts[2].parse::<usize>(), parts[3].parse::<usize>())
                {
                    let oid = parts[0].to_string();
                    metas.entry(oid.clone()).or_default();
                    for k in 0..num {
                        let idx = start + k; // final lines are 1-based
                        if idx >= 1 && idx <= total {
                            owner[idx - 1] = Some(oid.clone());
                        }
                    }
                    done = (done + num).min(total);
                    cur_oid = Some(oid);
                    expect_header = false;
                    continue;
                }
            }
            // Not a recognisable header (e.g. an early `git` warning line) —
            // stay in header mode and skip it.
            continue;
        }

        // Tagged metadata / entry terminator.
        if let Some(rest) = line.strip_prefix("filename ") {
            let _ = rest;
            // Entry complete → emit a throttled, fully-populated tick.
            if done.saturating_sub(last_emit) >= step || done >= total {
                let meta = cur_oid.as_ref().and_then(|o| metas.get(o));
                on_progress(BlameProgress {
                    done,
                    total,
                    current_short: cur_oid.as_ref().map(|o| short_oid(o)),
                    current_date: meta.map(|m| m.timestamp),
                    current_summary: meta.map(|m| m.summary.clone()),
                });
                last_emit = done;
            }
            expect_header = true;
            continue;
        }

        let Some(oid) = cur_oid.as_ref() else { continue };
        let meta = metas.entry(oid.clone()).or_default();
        if let Some(v) = line.strip_prefix("author ") {
            meta.author_name = v.to_string();
        } else if let Some(v) = line.strip_prefix("author-mail ") {
            meta.author_email = v.trim_start_matches('<').trim_end_matches('>').to_string();
        } else if let Some(v) = line.strip_prefix("author-time ") {
            meta.timestamp = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("summary ") {
            meta.summary = v.to_string();
        }
        // committer-*, author-tz, previous, boundary → ignored.
    }

    // ── Exit status ───────────────────────────────────────────────────────
    let status = child
        .wait()
        .map_err(|e| AppError::Other(format!("git blame wait: {e}")))?;
    if !status.success() {
        let mut err = String::new();
        if let Some(mut se) = child.stderr.take() {
            let _ = se.read_to_string(&mut err);
        }
        let err = err.trim();
        return Err(AppError::Other(if err.is_empty() {
            format!("git blame exited with {status}")
        } else {
            format!("git blame failed: {err}")
        }));
    }

    // Final tick so the bar always lands on 100% even if the last entry was
    // throttled out above.
    on_progress(BlameProgress {
        done: total,
        total,
        current_short: cur_oid.as_ref().map(|o| short_oid(o)),
        current_date: None,
        current_summary: None,
    });

    Ok(assemble(&owner, &file_lines, &metas, total))
}

/// First 7 chars of a full OID, or the zero sentinel for uncommitted lines.
fn short_oid(oid: &str) -> String {
    if oid.chars().all(|c| c == '0') {
        "0000000".to_string()
    } else {
        oid[..7.min(oid.len())].to_string()
    }
}

/// Turn the per-line owner map into `BlameLine`s in line order, computing
/// `is_group_start` on commit changes — identical shape to the libgit2 path.
fn assemble(
    owner: &[Option<String>],
    file_lines: &[&str],
    metas: &HashMap<String, CommitMeta>,
    total: usize,
) -> Vec<BlameLine> {
    let mut result = Vec::with_capacity(total);
    let mut prev_oid: Option<&str> = None;
    let empty = CommitMeta::default();

    for i in 0..total {
        let oid = owner[i].as_deref().unwrap_or("");
        let is_zero = oid.is_empty() || oid.chars().all(|c| c == '0');
        let meta = metas.get(oid).unwrap_or(&empty);

        let (commit_oid, summary) = if is_zero {
            ("0".repeat(40), "Uncommitted changes".to_string())
        } else {
            (oid.to_string(), meta.summary.clone())
        };

        let content = file_lines
            .get(i)
            .copied()
            .unwrap_or("")
            .trim_end_matches('\r')
            .to_string();

        let is_group_start = prev_oid != Some(oid);

        result.push(BlameLine {
            line_no: i + 1,
            content,
            commit_oid,
            short_oid: short_oid(oid),
            author_name: if is_zero || meta.author_name.is_empty() {
                "Unknown".to_string()
            } else {
                meta.author_name.clone()
            },
            author_email: meta.author_email.clone(),
            timestamp: meta.timestamp,
            summary,
            is_group_start,
        });

        prev_oid = Some(oid);
    }

    result
}
