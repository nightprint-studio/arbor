use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::process_ext::NoWindowExt;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BisectMark {
    Good,
    Bad,
    Skip,
}

impl BisectMark {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Bad  => "bad",
            Self::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BisectState {
    pub active: bool,
    /// The commit currently being tested (HEAD during bisect).
    pub current_hash: Option<String>,
    /// All commits marked as "bad" during this session.
    pub bad_hashes: Vec<String>,
    pub good_hashes: Vec<String>,
    /// Approximate remaining steps, parsed from `git bisect` output.
    pub steps_remaining: Option<u32>,
    /// Set when bisect has found the first bad commit.
    pub result_hash: Option<String>,
    pub result_message: Option<String>,
    /// True when there is at least one mark that can be undone.
    pub can_undo: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Start a new bisect session in `--no-checkout` mode.
///
/// Git performs the binary search and updates `BISECT_HEAD` to point to the
/// next commit to test, but never modifies the working tree.  This means:
/// - A dirty working tree is never a problem.
/// - The user can mark commits based on historical knowledge without having
///   to physically check them out.
/// - A "Checkout" button in the UI lets the user switch to `BISECT_HEAD`
///   when they actually want to run tests against it.
pub fn bisect_start(repo_path: &str) -> Result<BisectState> {
    run_git(repo_path, &["bisect", "start", "--no-checkout"])?;
    get_bisect_state(repo_path)
}

/// Mark a commit as good, bad, or skip.
///
/// Returns the updated bisect state, including:
/// - `steps_remaining` if the session is still in progress.
/// - `result_hash` + `result_message` if bisect has found the culprit.
pub fn bisect_mark(repo_path: &str, hash: &str, mark: BisectMark) -> Result<BisectState> {
    let output = crate::git_cli::command()
        .args(["bisect", mark.as_str(), hash])
        .current_dir(repo_path)
        .no_window()
        .output()
        .map_err(AppError::Io)?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        // "git bisect good" with no remaining commits is not an error; ignore.
        if stdout.contains("first bad commit") {
            // fall through — will be parsed below as a result
        } else if stdout.contains("only 'skip'ped commits") || stdout.contains("We cannot bisect more") {
            return Err(AppError::Other(
                "All remaining commits have been skipped — cannot determine the first bad commit.".into(),
            ));
        } else if !stderr.is_empty() {
            return Err(AppError::Other(stderr.trim().to_string()));
        }
    }

    let mut state = get_bisect_state(repo_path)?;

    // Parse "Bisecting: N revisions left to test after this (roughly K steps)"
    if let Some(steps) = parse_steps_remaining(&stdout) {
        state.steps_remaining = Some(steps);
    }

    // Parse "<hash> is the first bad commit"
    if let Some((result_hash, result_msg)) = parse_result(&stdout) {
        state.result_hash    = Some(result_hash);
        state.result_message = Some(result_msg);
        state.active = true; // session still active until reset
    }

    Ok(state)
}

/// Abort the bisect session and restore the original HEAD.
pub fn bisect_reset(repo_path: &str) -> Result<()> {
    run_git(repo_path, &["bisect", "reset"])
}

/// Read the current bisect state from `.git/BISECT_HEAD` and `.git/BISECT_LOG`.
///
/// Note: `BISECT_HEAD` is only created after git has a full range (bad + good)
/// and checks out the midpoint. Before that (e.g. only bad is marked), only
/// `BISECT_LOG` exists — so we check both files to determine activity.
pub fn get_bisect_state(repo_path: &str) -> Result<BisectState> {
    let git_dir = Path::new(repo_path).join(".git");
    let head_file = git_dir.join("BISECT_HEAD");
    let log_file  = git_dir.join("BISECT_LOG");

    if !head_file.exists() && !log_file.exists() {
        return Ok(BisectState {
            active: false,
            current_hash: None,
            bad_hashes: Vec::new(),
            good_hashes: Vec::new(),
            steps_remaining: None,
            result_hash: None,
            result_message: None,
            can_undo: false,
        });
    }

    let current_hash = if head_file.exists() {
        std::fs::read_to_string(&head_file)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    let (bad_hashes, good_hashes, mark_count) = parse_bisect_log(&git_dir);

    // Detect the "result found" state on reload.
    //
    // After bisect terminates git leaves BISECT_HEAD pointing at the last
    // commit that was tested (the midpoint), which is now in either
    // bad_hashes (when the bad-side was confirmed) or good_hashes (when the
    // good-side was confirmed and git derived the adjacent bad commit).
    //
    // We require at least one good mark to avoid confusing the initial
    // "waiting for good" state (where BISECT_HEAD can also hold a bad commit).
    let result_hash = if good_hashes.is_empty() {
        None
    } else {
        match &current_hash {
            // BISECT_HEAD already points at the found bad commit.
            Some(ch) if bad_hashes.contains(ch) => Some(ch.clone()),
            // BISECT_HEAD points at the last tested commit (in good_hashes);
            // the actual first-bad commit is the most-recently-narrowed bad.
            Some(ch) if good_hashes.contains(ch) => bad_hashes.last().cloned(),
            _ => None,
        }
    };

    // When a result is known the midpoint is meaningless — clear it so the
    // banner doesn't ask the user to test it again.
    let effective_current = if result_hash.is_some() { None } else { current_hash };

    Ok(BisectState {
        active: true,
        current_hash: effective_current,
        bad_hashes,
        good_hashes,
        steps_remaining: None,
        result_hash,
        result_message: None,
        can_undo: mark_count > 0,
    })
}

/// Undo the last mark by replaying the bisect log without its final command.
///
/// Uses `git bisect log` → strips last mark line → `git bisect reset` →
/// `git bisect replay <edited-log>`.  If the only mark was the initial "bad",
/// resets fully and restarts from scratch (no marks left to replay).
pub fn bisect_undo_last_mark(repo_path: &str) -> Result<BisectState> {
    // 1. Capture the current session log.
    let log_out = crate::git_cli::command()
        .args(["bisect", "log"])
        .current_dir(repo_path)
        .no_window()
        .output()
        .map_err(AppError::Io)?;

    if !log_out.status.success() {
        return Err(AppError::Other("No active bisect session".into()));
    }

    let log = String::from_utf8_lossy(&log_out.stdout).to_string();
    let lines: Vec<&str> = log.lines().collect();

    // 2. Find the last mark command (good/bad/skip, not start).
    let last_mark_idx = lines.iter().rposition(|l| {
        l.starts_with("git bisect good ")
            || l.starts_with("git bisect bad ")
            || l.starts_with("git bisect skip ")
    });

    let Some(idx) = last_mark_idx else {
        return Err(AppError::Other("No marks to undo".into()));
    };

    // 3. Rebuild log: drop the mark line and its immediately preceding comment.
    let mut new_lines: Vec<&str> = Vec::with_capacity(lines.len());
    for (i, &line) in lines.iter().enumerate() {
        if i == idx {
            // Also remove the comment line just before this mark, if any.
            if new_lines.last().map(|l: &&str| l.starts_with('#')).unwrap_or(false) {
                new_lines.pop();
            }
            continue;
        }
        new_lines.push(line);
    }

    // 4. Reset the current session.
    run_git(repo_path, &["bisect", "reset"])?;

    // 5. If there are remaining marks, replay; otherwise just re-start.
    let has_remaining_marks = new_lines.iter().any(|l| {
        l.starts_with("git bisect good ")
            || l.starts_with("git bisect bad ")
            || l.starts_with("git bisect skip ")
    });

    if !has_remaining_marks {
        // Only "git bisect start" was left — restart a clean session.
        run_git(repo_path, &["bisect", "start"])?;
        return get_bisect_state(repo_path);
    }

    let tmp_path = std::env::temp_dir().join("arbor_bisect_replay.log");
    std::fs::write(&tmp_path, new_lines.join("\n") + "\n").map_err(AppError::Io)?;

    let replay_out = crate::git_cli::command()
        .args(["bisect", "replay", &tmp_path.to_string_lossy()])
        .current_dir(repo_path)
        .no_window()
        .output()
        .map_err(AppError::Io)?;

    if !replay_out.status.success() {
        return Err(AppError::Other(
            String::from_utf8_lossy(&replay_out.stderr).trim().to_string(),
        ));
    }

    let mut state = get_bisect_state(repo_path)?;

    // Propagate steps_remaining from replay output if present.
    let stdout = String::from_utf8_lossy(&replay_out.stdout).to_string();
    if let Some(steps) = parse_steps_remaining(&stdout) {
        state.steps_remaining = Some(steps);
    }

    Ok(state)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn run_git(repo_path: &str, args: &[&str]) -> Result<()> {
    let output = crate::git_cli::command()
        .args(args)
        .current_dir(repo_path)
        .no_window()
        .output()
        .map_err(AppError::Io)?;

    if !output.status.success() {
        return Err(AppError::Other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

/// Parse `.git/BISECT_LOG` to extract the initial bad hash, all good hashes,
/// and the total number of mark commands (for `can_undo`).
///
/// Log format (lines):
/// ```text
/// # bad: [<sha>] <summary>
/// # good: [<sha>] <summary>
/// git bisect bad <sha>
/// git bisect good <sha>
/// ```
fn parse_bisect_log(git_dir: &Path) -> (Vec<String>, Vec<String>, usize) {
    let log_path = git_dir.join("BISECT_LOG");
    let content = match std::fs::read_to_string(log_path) {
        Ok(c) => c,
        Err(_) => return (Vec::new(), Vec::new(), 0),
    };

    let mut bad_hashes: Vec<String>  = Vec::new();
    let mut good_hashes: Vec<String> = Vec::new();
    let mut mark_count: usize        = 0;

    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("git bisect bad ") {
            let sha = rest.trim().to_string();
            if !sha.is_empty() { bad_hashes.push(sha); mark_count += 1; }
        } else if let Some(rest) = line.strip_prefix("git bisect good ") {
            let sha = rest.trim().to_string();
            if !sha.is_empty() { good_hashes.push(sha); mark_count += 1; }
        } else if line.starts_with("git bisect skip ") {
            mark_count += 1;
        }
    }

    (bad_hashes, good_hashes, mark_count)
}

/// Parse "Bisecting: N revisions left …" → N
fn parse_steps_remaining(output: &str) -> Option<u32> {
    for line in output.lines() {
        if line.starts_with("Bisecting:") {
            // "Bisecting: 5 revisions left to test after this (roughly 3 steps)"
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if parts.len() >= 2 {
                if let Ok(n) = parts[1].trim_matches(',').parse::<u32>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Parse "<sha> is the first bad commit" → (sha, summary line)
fn parse_result(output: &str) -> Option<(String, String)> {
    for line in output.lines() {
        if let Some(pos) = line.find(" is the first bad commit") {
            let hash = line[..pos].trim().to_string();
            // Next non-empty line after "commit <sha>" is author/date; look for
            // the commit summary — it appears on a line starting with 4 spaces.
            let summary: String = output
                .lines()
                .skip_while(|l| !l.starts_with("    "))
                .take(1)
                .map(|l| l.trim().to_string())
                .next()
                .unwrap_or_default();
            return Some((hash, summary));
        }
    }
    None
}
