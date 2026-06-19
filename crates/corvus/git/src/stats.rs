//! `stats` domain — pure git logic, Tauri-free.
//!
//! A single `git2` revwalk that aggregates contributor / temporal / file
//! statistics into a [`RepoStats`], plus the self-contained JSON / HTML
//! exporters (inline CSS + SVG charts, no external dependencies).
//!
//! Lifted verbatim from the shell `crate::git::stats` + `crate::git::stats_export`;
//! the only coupling the crate refuses is the per-repo config type. The shell's
//! `StatsExcludeConfig` (a serde TOML struct living in `crate::config`) is
//! **injected** as the crate-owned [`StatsExclude`] value-struct, so the walk
//! stays Tauri-/config-free. `compute_stats` is pure `git2` — it takes no
//! [`crate::cli::GitCli`] and records no recovery snapshot.
//!
//! NOT moved (stays shell-side): the background-job orchestration, the stats
//! cache, and the plugin-logo branding snapshot — all Tauri-coupled and reached
//! by the broker handler in `ipc/corvus/stats.rs`.

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use git2::{Repository, Sort};
use serde::{Deserialize, Serialize};

use crate::error::{GitError, Result};

// ---------------------------------------------------------------------------
// Injected config
// ---------------------------------------------------------------------------

/// Paths and extensions to exclude from repository statistics.
///
/// The crate-owned mirror of the shell's `StatsExcludeConfig` (which is a
/// serde TOML struct in `crate::config::repo_config`). The shell adapts its
/// config into this value-struct so the walk depends on nothing config-shaped.
#[derive(Debug, Clone, Default)]
pub struct StatsExclude {
    /// File extensions to exclude, e.g. [".ron", ".lock"].
    /// Leading dot is optional — both ".ron" and "ron" are accepted.
    pub extensions: Vec<String>,
    /// Folder prefixes to exclude, e.g. ["assets/generated", "vendor"].
    pub folders: Vec<String>,
    /// Exact file names or relative paths to exclude, e.g. ["Cargo.lock"].
    pub files: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributorStat {
    pub name: String,
    pub email: String,
    pub commit_count: usize,
    pub percentage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeStat {
    pub path: String,
    pub change_count: usize,
}

/// Per-author line-change statistics (computed from first FILE_COMMIT_LIMIT commits).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorLineStat {
    pub name: String,
    pub email: String,
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub total_changes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStats {
    pub total_commits: usize,
    pub total_contributors: usize,
    pub first_commit_time: i64,
    pub last_commit_time: i64,
    pub active_days: usize,
    /// Last 365 days: Vec of (iso_date "YYYY-MM-DD", count).
    /// Only dates that have at least one commit are included.
    pub commits_by_day: Vec<(String, usize)>,
    /// Top 10 contributors by commit count.
    pub top_contributors: Vec<ContributorStat>,
    /// Distribution by hour of day (index = hour 0–23).
    pub commits_by_hour: Vec<usize>,
    /// Distribution by weekday (0 = Mon … 6 = Sun).
    pub commits_by_weekday: Vec<usize>,
    /// Most changed files — only the first 500 commits are scanned for performance.
    pub most_changed_files: Vec<FileChangeStat>,
    /// Breakdown by file extension: top 10 (ext, cumulative change count).
    pub file_type_breakdown: Vec<(String, usize)>,
    /// Top contributor in the last 7 days.
    pub top_contributor_week: Option<ContributorStat>,
    /// Top contributor in the last 30 days.
    pub top_contributor_month: Option<ContributorStat>,
    /// Author with most total lines changed (first FILE_COMMIT_LIMIT commits).
    pub top_changer: Option<AuthorLineStat>,
    /// Top authors by lines changed, sorted desc — up to 10.
    pub top_changers: Vec<AuthorLineStat>,
    /// Calendar date with the highest single-day commit count.
    pub busiest_day: Option<(String, usize)>,
    /// Average commits per calendar week over the full project lifetime.
    pub avg_commits_per_week: f32,
    /// Longest consecutive-day streak with at least one commit.
    pub longest_streak: usize,
    /// Average lines changed per commit (insertions + deletions, first FILE_COMMIT_LIMIT commits).
    pub avg_commit_size: f32,
}

// ---------------------------------------------------------------------------
// Date/time helpers — no external dependencies
// ---------------------------------------------------------------------------

/// Civil date algorithm by Howard Hinnant (public domain).
/// Returns (year, month 1–12, day 1–31) for a UTC Unix timestamp.
fn unix_to_ymd(ts: i64) -> (i32, u32, u32) {
    let z = ts.div_euclid(86400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn unix_to_iso_date(ts: i64) -> String {
    let (y, m, d) = unix_to_ymd(ts);
    format!("{y:04}-{m:02}-{d:02}")
}

fn unix_to_hour(ts: i64) -> usize {
    (ts.rem_euclid(86400) / 3600) as usize
}

/// Weekday: 0 = Monday … 6 = Sunday.
/// 1970-01-01 was a Thursday → index 3 in Mon-based weekday.
fn unix_to_weekday(ts: i64) -> usize {
    ((ts.div_euclid(86400) + 3).rem_euclid(7)) as usize
}

// ---------------------------------------------------------------------------
// Core computation
// ---------------------------------------------------------------------------

const FILE_COMMIT_LIMIT: usize = 500;

/// Returns `true` if this file path should be skipped based on the exclusion config.
fn is_excluded(path: &str, exclude: &StatsExclude) -> bool {
    // Check extensions
    if !exclude.extensions.is_empty() {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        for excl_ext in &exclude.extensions {
            let normalized = excl_ext.trim_start_matches('.');
            if ext == normalized.to_lowercase() {
                return true;
            }
        }
    }
    // Check folders (path prefix match)
    for folder in &exclude.folders {
        let prefix = folder.trim_end_matches('/');
        if path.starts_with(&format!("{prefix}/")) || path == prefix {
            return true;
        }
    }
    // Check exact filenames or relative paths
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");
    for file in &exclude.files {
        if file == path || file == filename {
            return true;
        }
    }
    false
}

pub fn compute_stats(repo: &Repository, exclude: &StatsExclude) -> Result<RepoStats> {
    let mut walk = repo.revwalk().map_err(GitError::Git)?;

    // Empty repo: push_head fails — return zero stats gracefully.
    if walk.push_head().is_err() {
        return Ok(empty_stats());
    }
    walk.set_sorting(Sort::TIME).map_err(GitError::Git)?;

    // Accumulators
    let mut total_commits: usize = 0;
    // email → (display_name, count)
    let mut contributors: HashMap<String, (String, usize)> = HashMap::new();
    // all-time YYYY-MM-DD → count (for active_days + busiest day)
    let mut all_days: HashMap<String, usize> = HashMap::new();
    // last-365-days YYYY-MM-DD → count (for heatmap)
    let mut recent_days: HashMap<String, usize> = HashMap::new();
    // week / month contributors
    let mut week_contrib:  HashMap<String, (String, usize)> = HashMap::new();
    let mut month_contrib: HashMap<String, (String, usize)> = HashMap::new();
    let mut hours = vec![0usize; 24];
    let mut weekdays = vec![0usize; 7];
    let mut first_ts = i64::MAX;
    let mut last_ts = i64::MIN;

    // File change + line change tracking — only first FILE_COMMIT_LIMIT commits
    let mut file_counts:  HashMap<String, usize> = HashMap::new();
    // email → (name, lines_added, lines_deleted)
    let mut line_stats: HashMap<String, (String, usize, usize)> = HashMap::new();
    let mut file_idx: usize = 0;

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let cutoff_365  = now_secs - 365 * 86400;
    let cutoff_week = now_secs - 7  * 86400;
    let cutoff_month= now_secs - 30 * 86400;

    for oid_result in walk {
        let oid = match oid_result {
            Ok(o) => o,
            Err(_) => continue,
        };
        let commit = match repo.find_commit(oid) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let ts = commit.author().when().seconds();
        total_commits += 1;

        // Contributor stats
        let email = commit.author().email().unwrap_or("").to_string();
        let name = commit.author().name().unwrap_or("").to_string();
        let e = contributors.entry(email.clone()).or_insert_with(|| (name.clone(), 0));
        if e.0.is_empty() { e.0 = name.clone(); }
        e.1 += 1;
        if ts >= cutoff_week {
            let ew = week_contrib.entry(email.clone()).or_insert_with(|| (name.clone(), 0));
            if ew.0.is_empty() { ew.0 = name.clone(); }
            ew.1 += 1;
        }
        if ts >= cutoff_month {
            let em = month_contrib.entry(email.clone()).or_insert_with(|| (name.clone(), 0));
            if em.0.is_empty() { em.0 = name.clone(); }
            em.1 += 1;
        }

        // Temporal stats
        if ts < first_ts { first_ts = ts; }
        if ts > last_ts  { last_ts  = ts; }

        let iso = unix_to_iso_date(ts);
        *all_days.entry(iso.clone()).or_insert(0) += 1;
        if ts >= cutoff_365 {
            *recent_days.entry(iso).or_insert(0) += 1;
        }

        let h = unix_to_hour(ts);
        if h < 24 { hours[h] += 1; }
        let wd = unix_to_weekday(ts);
        if wd < 7 { weekdays[wd] += 1; }

        // File change + line stats (limited to first FILE_COMMIT_LIMIT commits)
        if file_idx < FILE_COMMIT_LIMIT {
            file_idx += 1;
            if let Ok(tree) = commit.tree() {
                let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
                if let Ok(diff) = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None) {
                    let _ = diff.foreach(
                        &mut |delta, _| {
                            let path = delta
                                .new_file().path()
                                .or_else(|| delta.old_file().path())
                                .and_then(|p| p.to_str())
                                .map(|s| s.to_string());
                            if let Some(p) = path {
                                if !is_excluded(&p, exclude) {
                                    *file_counts.entry(p).or_insert(0) += 1;
                                }
                            }
                            true
                        },
                        None, None, None,
                    );
                    // Line-level stats per author
                    if let Ok(stats) = diff.stats() {
                        let ls = line_stats.entry(email.clone())
                            .or_insert_with(|| (name.clone(), 0, 0));
                        if ls.0.is_empty() { ls.0 = name.clone(); }
                        ls.1 += stats.insertions();
                        ls.2 += stats.deletions();
                    }
                }
            }
        }
    }

    let total_contributors = contributors.len();

    // commits_by_day: sorted chronologically
    let mut commits_by_day: Vec<(String, usize)> = recent_days.into_iter().collect();
    commits_by_day.sort_by(|a, b| a.0.cmp(&b.0));

    // ── Week / Month top contributors ────────────────────────────────────────
    let make_contrib = |map: HashMap<String, (String, usize)>, total: usize| -> Option<ContributorStat> {
        map.into_iter()
            .max_by_key(|(_, (_, c))| *c)
            .map(|(email, (name, count))| ContributorStat {
                name, email, commit_count: count,
                percentage: if total > 0 { count as f32 / total as f32 * 100.0 } else { 0.0 },
            })
    };
    let week_total:  usize = week_contrib.values().map(|(_, c)| c).sum();
    let month_total: usize = month_contrib.values().map(|(_, c)| c).sum();
    let top_contributor_week  = make_contrib(week_contrib,  week_total);
    let top_contributor_month = make_contrib(month_contrib, month_total);

    // ── Top changers by lines ────────────────────────────────────────────────
    let mut changers_vec: Vec<AuthorLineStat> = line_stats
        .into_iter()
        .map(|(email, (name, added, deleted))| AuthorLineStat {
            name, email,
            lines_added:   added,
            lines_deleted: deleted,
            total_changes: added + deleted,
        })
        .collect();
    changers_vec.sort_by(|a, b| b.total_changes.cmp(&a.total_changes));
    let top_changer = changers_vec.first().cloned();
    changers_vec.truncate(10);
    let avg_commit_size = if file_idx > 0 {
        let total_lines: usize = changers_vec.iter().map(|c| c.total_changes).sum();
        total_lines as f32 / file_idx as f32
    } else { 0.0 };
    let top_changers = changers_vec;

    // ── Busiest day ──────────────────────────────────────────────────────────
    let busiest_day = all_days.iter()
        .max_by_key(|(_, &c)| c)
        .map(|(d, &c)| (d.clone(), c));

    // ── Average commits per week ─────────────────────────────────────────────
    let avg_commits_per_week = if first_ts != i64::MAX && last_ts != i64::MIN {
        let span_days = ((last_ts - first_ts).max(0) as f64 / 86400.0).max(1.0);
        (total_commits as f64 / (span_days / 7.0)) as f32
    } else { 0.0 };

    // ── Longest streak ───────────────────────────────────────────────────────
    let longest_streak = {
        let mut days_sorted: Vec<&String> = all_days.keys().collect();
        days_sorted.sort();
        let mut streak = 0usize;
        let mut best   = 0usize;
        let mut prev_epoch: i64 = -2;
        for d in days_sorted {
            // Parse YYYY-MM-DD to a day-epoch (days since some reference)
            let parts: Vec<&str> = d.split('-').collect();
            if parts.len() == 3 {
                if let (Ok(y), Ok(m), Ok(day)) = (
                    parts[0].parse::<i32>(),
                    parts[1].parse::<i64>(),
                    parts[2].parse::<i64>(),
                ) {
                    // Simplified day epoch: rough approximation
                    let epoch = y as i64 * 365 + m * 30 + day;
                    if epoch == prev_epoch + 1 {
                        streak += 1;
                    } else {
                        streak = 1;
                    }
                    prev_epoch = epoch;
                    if streak > best { best = streak; }
                }
            }
        }
        best
    };

    // top_contributors: sorted desc by count, top 10
    let total_f = total_commits as f32;
    let mut contrib_vec: Vec<ContributorStat> = contributors
        .into_iter()
        .map(|(email, (name, count))| ContributorStat {
            name,
            email,
            commit_count: count,
            percentage: if total_f > 0.0 { count as f32 / total_f * 100.0 } else { 0.0 },
        })
        .collect();
    contrib_vec.sort_by(|a, b| b.commit_count.cmp(&a.commit_count));
    contrib_vec.truncate(10);

    // file_type_breakdown from the full file_counts (before truncating to 20)
    let mut ext_counts: HashMap<String, usize> = HashMap::new();
    for (path, &count) in &file_counts {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()))
            .unwrap_or_else(|| "(none)".into());
        *ext_counts.entry(ext).or_insert(0) += count;
    }
    let mut ext_vec: Vec<(String, usize)> = ext_counts.into_iter().collect();
    ext_vec.sort_by(|a, b| b.1.cmp(&a.1));
    ext_vec.truncate(10);

    // most_changed_files: top 20
    let mut files_vec: Vec<FileChangeStat> = file_counts
        .into_iter()
        .map(|(path, change_count)| FileChangeStat { path, change_count })
        .collect();
    files_vec.sort_by(|a, b| b.change_count.cmp(&a.change_count));
    files_vec.truncate(20);

    Ok(RepoStats {
        total_commits,
        total_contributors,
        first_commit_time: if first_ts == i64::MAX { 0 } else { first_ts },
        last_commit_time:  if last_ts  == i64::MIN { 0 } else { last_ts  },
        active_days: all_days.len(),
        commits_by_day,
        top_contributors: contrib_vec,
        commits_by_hour: hours,
        commits_by_weekday: weekdays,
        most_changed_files: files_vec,
        file_type_breakdown: ext_vec,
        top_contributor_week,
        top_contributor_month,
        top_changer,
        top_changers,
        busiest_day,
        avg_commits_per_week,
        longest_streak,
        avg_commit_size,
    })
}

fn empty_stats() -> RepoStats {
    RepoStats {
        total_commits: 0,
        total_contributors: 0,
        first_commit_time: 0,
        last_commit_time: 0,
        active_days: 0,
        commits_by_day: vec![],
        top_contributors: vec![],
        commits_by_hour: vec![0; 24],
        commits_by_weekday: vec![0; 7],
        most_changed_files: vec![],
        file_type_breakdown: vec![],
        top_contributor_week: None,
        top_contributor_month: None,
        top_changer: None,
        top_changers: vec![],
        busiest_day: None,
        avg_commits_per_week: 0.0,
        longest_streak: 0,
        avg_commit_size: 0.0,
    }
}

// ===========================================================================
// Export — JSON or a self-contained HTML report.
//
// HTML output is a single file with inline CSS, inline SVG charts, the Arbor
// logo, and a JS tooltip layer — no external dependencies, opens in any browser.
// JSON output is pretty-printed and mirrors the `RepoStats` struct exactly.
// ===========================================================================

// ---------------------------------------------------------------------------
// Arbor logo — inline SVG, IDs prefixed with "al-" to avoid collisions when
// embedded alongside other SVG charts in the same HTML document.
// ---------------------------------------------------------------------------

pub const LOGO_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="38" height="38"><defs><linearGradient id="al-bg" x1="256" y1="0" x2="256" y2="512" gradientUnits="userSpaceOnUse"><stop offset="0%" stop-color="#1b2847"/><stop offset="55%" stop-color="#121a33"/><stop offset="100%" stop-color="#0a1020"/></linearGradient><linearGradient id="al-bevel" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#ffffff" stop-opacity="0.12"/><stop offset="40%" stop-color="#ffffff" stop-opacity="0"/></linearGradient><linearGradient id="al-legL" x1="96" y1="440" x2="256" y2="72" gradientUnits="userSpaceOnUse"><stop offset="0%" stop-color="#2f6de8"/><stop offset="55%" stop-color="#4f9bff"/><stop offset="100%" stop-color="#9cd6ff"/></linearGradient><linearGradient id="al-legR" x1="416" y1="440" x2="256" y2="72" gradientUnits="userSpaceOnUse"><stop offset="0%" stop-color="#1aa79a"/><stop offset="55%" stop-color="#3fd6c9"/><stop offset="100%" stop-color="#88f0e2"/></linearGradient><linearGradient id="al-bar" x1="176" y1="300" x2="336" y2="300" gradientUnits="userSpaceOnUse"><stop offset="0%" stop-color="#27b05a"/><stop offset="50%" stop-color="#65f18a"/><stop offset="100%" stop-color="#2bb878"/></linearGradient><radialGradient id="al-apexCore" cx="0.5" cy="0.5" r="0.5"><stop offset="0%" stop-color="#ffffff"/><stop offset="45%" stop-color="#d8e8ff"/><stop offset="100%" stop-color="#6ea8ff"/></radialGradient><filter id="al-soft" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="8" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge></filter><filter id="al-apex" x="-80%" y="-80%" width="260%" height="260%"><feGaussianBlur stdDeviation="14" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge></filter><filter id="al-shadow" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur in="SourceAlpha" stdDeviation="6"/><feOffset dx="0" dy="6" result="o"/><feComponentTransfer><feFuncA type="linear" slope="0.45"/></feComponentTransfer><feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter></defs><rect x="0" y="0" width="512" height="512" rx="112" ry="112" fill="url(#al-bg)"/><rect x="4" y="4" width="504" height="504" rx="108" ry="108" fill="none" stroke="#ffffff" stroke-opacity="0.06" stroke-width="2"/><rect x="0" y="0" width="512" height="220" rx="112" ry="112" fill="url(#al-bevel)"/><g filter="url(#al-shadow)"><line x1="96" y1="440" x2="256" y2="72" stroke="url(#al-legL)" stroke-width="76" stroke-linecap="round"/><line x1="416" y1="440" x2="256" y2="72" stroke="url(#al-legR)" stroke-width="76" stroke-linecap="round"/><path d="M 176 316 Q 256 338 336 316" fill="none" stroke="url(#al-bar)" stroke-width="48" stroke-linecap="round"/></g><circle cx="96" cy="440" r="22" fill="#0a1020" stroke="#6fb3ff" stroke-width="7" filter="url(#al-soft)"/><circle cx="416" cy="440" r="22" fill="#0a1020" stroke="#4fe0d0" stroke-width="7" filter="url(#al-soft)"/><circle cx="176" cy="316" r="13" fill="#0a1020" stroke="#7cf59e" stroke-width="5" filter="url(#al-soft)"/><circle cx="336" cy="316" r="13" fill="#0a1020" stroke="#7cf59e" stroke-width="5" filter="url(#al-soft)"/><circle cx="256" cy="332" r="9" fill="#b8f3c8" opacity="0.9"/><g filter="url(#al-apex)"><circle cx="256" cy="72" r="34" fill="#1a2a55" stroke="#b8d0ff" stroke-width="9"/><circle cx="256" cy="72" r="17" fill="url(#al-apexCore)"/></g></svg>"##;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn export_to_file(
    stats: &RepoStats,
    path: &Path,
    format: &str,
    repo_name: &str,
    logo_override: Option<&str>,
) -> std::result::Result<(), String> {
    match format {
        "json" => {
            let file = std::fs::File::create(path)
                .map_err(|e| format!("Cannot create '{}': {e}", path.display()))?;
            serde_json::to_writer_pretty(file, stats)
                .map_err(|e| format!("JSON serialisation error: {e}"))
        }
        "html" => {
            let html = generate_html(stats, repo_name, logo_override);
            std::fs::write(path, html)
                .map_err(|e| format!("Cannot write '{}': {e}", path.display()))
        }
        other => Err(format!("Unknown format '{other}'. Expected 'json' or 'html'.")),
    }
}

// ---------------------------------------------------------------------------
// Date helpers (self-contained, no external deps)
// ---------------------------------------------------------------------------

fn unix_to_iso(ts: i64) -> String {
    let (y, m, d) = unix_to_ymd(ts);
    format!("{y:04}-{m:02}-{d:02}")
}

fn today_days() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        / 86_400
}

fn days_to_iso(days: i64) -> String {
    unix_to_iso(days * 86_400)
}

/// Weekday for a days-since-epoch value. 0 = Mon … 6 = Sun.
fn days_to_weekday(days: i64) -> usize {
    ((days + 3).rem_euclid(7)) as usize
}

fn format_age(first: i64, last: i64) -> String {
    let days = ((last - first).unsigned_abs() / 86_400) as usize;
    if days < 30        { format!("{days}d") }
    else if days < 365  { format!("{}mo", days / 30) }
    else                { format!("{:.1}y", days as f64 / 365.0) }
}

fn fmt_num(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { out.push(','); }
        out.push(c);
    }
    out.chars().rev().collect()
}

fn current_date_str() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    unix_to_iso(now)
}

// ---------------------------------------------------------------------------
// XML / SVG helpers
// ---------------------------------------------------------------------------

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

// Single-quote–safe escape for SVG attribute values (no &apos; needed for
// data-tip since we always wrap those in single quotes).
fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     // single quotes inside a single-quoted SVG attribute must be escaped
     .replace('\'', "&apos;")
}

// ---------------------------------------------------------------------------
// Chart: horizontal bar (single colour)
// ---------------------------------------------------------------------------

fn bar_chart_h(
    items: &[(&str, usize, Option<f32>)],
    color: &str,
    max_val: usize,
) -> String {
    const LBL_W: i32 = 148;
    const BAR_A: i32 = 280;
    const VAL_W: i32 = 90;
    const ROW_H: i32 = 26;
    const BAR_H: i32 = 14;
    const PAD:   i32 = 6;

    let n       = items.len() as i32;
    let svg_w   = LBL_W + BAR_A + VAL_W;
    let svg_h   = n * ROW_H + PAD * 2;
    let max_val = max_val.max(1);

    let mut s = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' \
              width='{svg_w}' height='{svg_h}' \
              viewBox='0 0 {svg_w} {svg_h}'>"
    );

    for (i, (label, val, pct)) in items.iter().enumerate() {
        let iy    = PAD + i as i32 * ROW_H;
        let mid_y = iy + ROW_H / 2;
        let bar_y = iy + (ROW_H - BAR_H) / 2;
        let bar_w = (*val as f64 / max_val as f64 * BAR_A as f64) as i32;

        s.push_str(&format!(
            "<text x='{lx}' y='{mid_y}' text-anchor='end' \
                   font-size='11' fill='#abb2bf' font-family='system-ui,sans-serif' \
                   dominant-baseline='middle'>{lbl}</text>",
            lx  = LBL_W - 6,
            lbl = esc(label),
        ));

        s.push_str(&format!(
            "<rect x='{LBL_W}' y='{bar_y}' width='{BAR_A}' height='{BAR_H}' \
                   rx='3' fill='#2a2c30'/>"
        ));

        if bar_w > 0 {
            s.push_str(&format!(
                "<rect x='{LBL_W}' y='{bar_y}' width='{bar_w}' height='{BAR_H}' \
                       rx='3' fill='{color}' opacity='0.85'/>"
            ));
        }

        let val_str = match pct {
            Some(p) => format!("{} ({:.0}%)", fmt_num(*val), p),
            None    => fmt_num(*val),
        };
        s.push_str(&format!(
            "<text x='{vx}' y='{mid_y}' font-size='10' fill='#6e7e96' \
                   font-family='system-ui,sans-serif' dominant-baseline='middle'>{val_str}</text>",
            vx = LBL_W + bar_w + 6,
        ));
    }

    s.push_str("</svg>");
    s
}

// ---------------------------------------------------------------------------
// Chart: stacked horizontal bar (adds / deletes)
// ---------------------------------------------------------------------------

fn bar_chart_h_split(items: &[(&str, usize, usize)]) -> String {
    const LBL_W: i32 = 148;
    const BAR_A: i32 = 280;
    const VAL_W: i32 = 110;
    const ROW_H: i32 = 26;
    const BAR_H: i32 = 14;
    const PAD:   i32 = 6;

    let n     = items.len() as i32;
    let svg_w = LBL_W + BAR_A + VAL_W;
    let svg_h = n * ROW_H + PAD * 2;

    let max_total = items.iter()
        .map(|(_, a, d)| a + d)
        .max()
        .unwrap_or(1)
        .max(1);

    let mut s = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' \
              width='{svg_w}' height='{svg_h}' \
              viewBox='0 0 {svg_w} {svg_h}'>"
    );

    for (i, (label, adds, dels)) in items.iter().enumerate() {
        let iy    = PAD + i as i32 * ROW_H;
        let mid_y = iy + ROW_H / 2;
        let bar_y = iy + (ROW_H - BAR_H) / 2;
        let total = adds + dels;
        let total_w = (total as f64 / max_total as f64 * BAR_A as f64) as i32;
        let adds_w  = if total > 0 { (*adds as f64 / total as f64 * total_w as f64) as i32 } else { 0 };
        let dels_w  = total_w - adds_w;

        s.push_str(&format!(
            "<text x='{lx}' y='{mid_y}' text-anchor='end' \
                   font-size='11' fill='#abb2bf' font-family='system-ui,sans-serif' \
                   dominant-baseline='middle'>{lbl}</text>",
            lx  = LBL_W - 6,
            lbl = esc(label),
        ));

        s.push_str(&format!(
            "<rect x='{LBL_W}' y='{bar_y}' width='{BAR_A}' height='{BAR_H}' rx='3' fill='#2a2c30'/>"
        ));

        if adds_w > 0 {
            s.push_str(&format!(
                "<rect x='{LBL_W}' y='{bar_y}' width='{adds_w}' height='{BAR_H}' rx='3' fill='#4db84d' opacity='0.85'/>"
            ));
        }
        if dels_w > 0 {
            let dx = LBL_W + adds_w;
            s.push_str(&format!(
                "<rect x='{dx}' y='{bar_y}' width='{dels_w}' height='{BAR_H}' rx='3' fill='#ff4444' opacity='0.75'/>"
            ));
        }

        s.push_str(&format!(
            "<text x='{vx}' y='{mid_y}' font-size='10' fill='#6e7e96' \
                   font-family='system-ui,sans-serif' dominant-baseline='middle'>+{a} -{d}</text>",
            vx = LBL_W + total_w + 6,
            a  = fmt_num(*adds),
            d  = fmt_num(*dels),
        ));
    }

    s.push_str("</svg>");
    s
}

// ---------------------------------------------------------------------------
// Chart: vertical bars (hours / weekdays) — with hover tooltips
// ---------------------------------------------------------------------------

/// `tips[i]` is the tooltip label for bar i (e.g. "Hour 14" or "Monday").
/// A transparent full-height hit-area rect is layered on top of each column
/// so hover works even for very small bars.
fn bar_chart_v(values: &[usize], labels: &[&str], tips: &[&str], color: &str, svg_w: i32) -> String {
    const CHART_H: i32 = 90;
    const LBL_H:   i32 = 18;
    const MARGIN:  i32 = 12;

    let n       = values.len() as i32;
    let avail_w = svg_w - 2 * MARGIN;
    let step    = avail_w / n;
    let bar_w   = (step as f64 * 0.65) as i32;
    let bar_pad = (step - bar_w) / 2;
    let max_val = values.iter().copied().max().unwrap_or(1).max(1);
    let svg_h   = CHART_H + LBL_H + 4;

    let mut s = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' \
              width='{svg_w}' height='{svg_h}' \
              viewBox='0 0 {svg_w} {svg_h}'>"
    );

    // Baseline
    s.push_str(&format!(
        "<line x1='{MARGIN}' y1='{CHART_H}' x2='{xe}' y2='{CHART_H}' \
               stroke='#2e3035' stroke-width='1'/>",
        xe = svg_w - MARGIN
    ));

    for (i, (&val, (label, tip))) in values.iter()
        .zip(labels.iter().zip(tips.iter()))
        .enumerate()
    {
        let bar_h = ((val as f64 / max_val as f64) * CHART_H as f64) as i32;
        let x     = MARGIN + i as i32 * step + bar_pad;
        let y     = CHART_H - bar_h;
        let cx    = x + bar_w / 2;
        let tip_s = esc_attr(&format!("{tip} — {}", if val == 1 { "1 commit".to_string() } else { format!("{} commits", fmt_num(val)) }));

        // Bar fill
        if bar_h > 0 {
            s.push_str(&format!(
                "<rect x='{x}' y='{y}' width='{bar_w}' height='{bar_h}' \
                       rx='2' fill='{color}' opacity='0.82'/>"
            ));
        }

        // X-axis label
        s.push_str(&format!(
            "<text x='{cx}' y='{ly}' font-size='8' fill='#5e7288' \
                   text-anchor='middle' font-family='system-ui,sans-serif'>{lbl}</text>",
            ly  = CHART_H + 13,
            lbl = label,
        ));

        // Transparent hit-area covering the full column height — carries the tooltip
        s.push_str(&format!(
            "<rect x='{x}' y='0' width='{bar_w}' height='{CHART_H}' \
                   fill='transparent' data-tip='{tip_s}' style='cursor:default'/>"
        ));
    }

    s.push_str("</svg>");
    s
}

// ---------------------------------------------------------------------------
// Chart: commit heatmap (52 × 7 GitHub-style calendar) — with hover tooltips
// ---------------------------------------------------------------------------

fn heatmap_svg(commits_by_day: &[(String, usize)]) -> String {
    let day_map: HashMap<&str, usize> =
        commits_by_day.iter().map(|(d, c)| (d.as_str(), *c)).collect();

    let max_count = day_map.values().copied().max().unwrap_or(1).max(1);

    const CELL: i32 = 11;
    const GAP:  i32 = 2;
    const STEP: i32 = CELL + GAP;
    const LBL_W: i32 = 28;
    const MON_H: i32 = 16;

    let n_weeks = 52i32;
    let n_days  = 7i32;
    let svg_w   = LBL_W + n_weeks * STEP;
    let svg_h   = MON_H + n_days * STEP;

    let today      = today_days();
    let today_wd   = days_to_weekday(today) as i64;
    let week_start = today - today_wd;
    let start_day  = week_start - 51 * 7;

    let mut s = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' \
              width='{svg_w}' height='{svg_h}' \
              viewBox='0 0 {svg_w} {svg_h}'>"
    );

    // Day labels (left): Mon, Wed, Fri only
    const DAY_LBLS: [&str; 7] = ["Mon","Tue","Wed","Thu","Fri","Sat","Sun"];
    for (d, lbl) in DAY_LBLS.iter().enumerate() {
        if d % 2 == 0 {
            let y = MON_H + d as i32 * STEP + CELL / 2 + 4;
            s.push_str(&format!(
                "<text x='{lx}' y='{y}' font-size='8' fill='#5e7288' \
                       text-anchor='end' font-family='system-ui,sans-serif'>{lbl}</text>",
                lx = LBL_W - 4,
            ));
        }
    }

    // Month labels (top)
    let mut prev_month = 0u32;
    const MONTH_NAMES: [&str; 12] =
        ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];

    for w in 0..n_weeks {
        let week_day0 = start_day + w as i64 * 7;
        let (_, m, _) = unix_to_ymd(week_day0 * 86_400);
        if m != prev_month {
            prev_month = m;
            let x = LBL_W + w * STEP;
            s.push_str(&format!(
                "<text x='{x}' y='11' font-size='8' fill='#5e7288' \
                       font-family='system-ui,sans-serif'>{lbl}</text>",
                lbl = MONTH_NAMES[(m - 1) as usize],
            ));
        }
    }

    // Cells
    for w in 0..n_weeks {
        for d in 0..n_days {
            let day       = start_day + w as i64 * 7 + d as i64;
            let is_future = day > today;

            let count = if is_future {
                0usize
            } else {
                let iso = days_to_iso(day);
                day_map.get(iso.as_str()).copied().unwrap_or(0)
            };

            let fill = if is_future {
                "#1a1b1e"
            } else {
                heatmap_color(count, max_count)
            };

            let x = LBL_W + w * STEP;
            let y = MON_H + d * STEP;

            if is_future {
                s.push_str(&format!(
                    "<rect x='{x}' y='{y}' width='{CELL}' height='{CELL}' rx='2' fill='{fill}'/>"
                ));
            } else {
                let iso = days_to_iso(day);
                let tip_text = if count == 0 {
                    format!("{iso} — no commits")
                } else if count == 1 {
                    format!("{iso} — 1 commit")
                } else {
                    format!("{iso} — {} commits", fmt_num(count))
                };
                let tip = esc_attr(&tip_text);
                s.push_str(&format!(
                    "<rect x='{x}' y='{y}' width='{CELL}' height='{CELL}' rx='2' \
                           fill='{fill}' data-tip='{tip}' style='cursor:default'/>"
                ));
            }
        }
    }

    s.push_str("</svg>");
    s
}

fn heatmap_color(count: usize, max: usize) -> &'static str {
    if count == 0 { return "#212820"; }
    let frac = count as f64 / max as f64;
    if      frac < 0.25 { "#1e3a1c" }
    else if frac < 0.50 { "#2a5c27" }
    else if frac < 0.75 { "#4db84d" }
    else                { "#7ed87e" }
}

// ---------------------------------------------------------------------------
// HTML stat cards
// ---------------------------------------------------------------------------

fn stat_card(label: &str, value: &str, sub: &str, accent: &str) -> String {
    format!(
        r#"<div class="card" style="border-top:3px solid {accent}">
  <div class="card-lbl">{label}</div>
  <div class="card-val">{value}</div>
  <div class="card-sub">{sub}</div>
</div>"#
    )
}

// ---------------------------------------------------------------------------
// Tooltip JavaScript — lightweight, no dependencies
// ---------------------------------------------------------------------------

const TOOLTIP_JS: &str = r#"
(function(){
  var tip = document.createElement('div');
  tip.style.cssText = [
    'position:fixed',
    'pointer-events:none',
    'background:#1c1e22',
    'border:1px solid #3a3d47',
    'border-radius:5px',
    'padding:5px 10px',
    'font-size:11px',
    'color:#c5cdd8',
    'white-space:nowrap',
    'opacity:0',
    'transition:opacity .1s',
    'z-index:9999',
    'font-family:system-ui,sans-serif',
    'box-shadow:0 4px 16px rgba(0,0,0,.55)',
    'line-height:1.5',
  ].join(';');
  document.body.appendChild(tip);

  function findTip(el) {
    while (el && el !== document.body) {
      if (el.getAttribute && el.getAttribute('data-tip')) return el;
      el = el.parentElement;
    }
    return null;
  }

  function move(e) {
    var x = e.clientX + 14, y = e.clientY - 38;
    if (x + tip.offsetWidth + 8 > window.innerWidth) x = e.clientX - tip.offsetWidth - 10;
    if (y < 8) y = e.clientY + 14;
    tip.style.left = x + 'px';
    tip.style.top  = y + 'px';
  }

  document.addEventListener('mouseover', function(e) {
    var el = findTip(e.target);
    if (el) {
      tip.textContent = el.getAttribute('data-tip');
      tip.style.opacity = '1';
      move(e);
    } else {
      tip.style.opacity = '0';
    }
  });

  document.addEventListener('mousemove', function(e) {
    if (tip.style.opacity !== '0') move(e);
  });

  document.addEventListener('mouseout', function(e) {
    if (!findTip(e.relatedTarget)) tip.style.opacity = '0';
  });
})();
"#;

// ---------------------------------------------------------------------------
// HTML generation
// ---------------------------------------------------------------------------

const CSS: &str = r##"
*{box-sizing:border-box;margin:0;padding:0}
body{background:#1e1f22;color:#abb2bf;font-family:-apple-system,'Segoe UI',system-ui,sans-serif;
     font-size:13px;line-height:1.5;padding:32px 28px}
.page{max-width:1100px;margin:0 auto}
.page-hdr{display:flex;align-items:center;gap:12px;margin-bottom:3px}
h1{color:#dce4f0;font-size:21px;font-weight:700}
.meta{color:#5e7288;font-size:11px;margin-bottom:32px;padding-left:50px}
h2{color:#c5cdd8;font-size:14px;font-weight:600;margin:32px 0 14px;
   border-bottom:1px solid #2e3035;padding-bottom:7px}
h3{color:#8b97a7;font-size:11px;font-weight:600;text-transform:uppercase;
   letter-spacing:.8px;margin:18px 0 8px}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-bottom:6px}
.card{background:#252629;border:1px solid #2e3035;border-radius:7px;padding:13px 15px}
.card-lbl{color:#5e7288;font-size:9px;text-transform:uppercase;letter-spacing:.9px;margin-bottom:5px}
.card-val{color:#dce4f0;font-size:20px;font-weight:700;letter-spacing:-.5px}
.card-sub{color:#5e7288;font-size:10px;margin-top:2px}
.box{background:#252629;border:1px solid #2e3035;border-radius:7px;
     padding:14px 16px;margin-bottom:14px;overflow-x:auto}
.two{display:grid;grid-template-columns:1fr 1fr;gap:16px}
footer{margin-top:48px;color:#333b45;font-size:11px;text-align:center;
       border-top:1px solid #2a2c30;padding-top:14px;display:flex;
       align-items:center;justify-content:center;gap:10px}
svg text{font-family:-apple-system,'Segoe UI',system-ui,sans-serif}
[data-tip]{cursor:default}
"##;

pub fn generate_html(stats: &RepoStats, repo_name: &str, logo_override: Option<&str>) -> String {
    let date = current_date_str();
    let age  = format_age(stats.first_commit_time, stats.last_commit_time);

    // ── Stat cards ───────────────────────────────────────────────────────────
    let cards = format!(
        "<div class=\"cards\">{}{}{}{}{}{}{}{}</div>",
        stat_card("Total Commits",    &fmt_num(stats.total_commits),       "lifetime",          "#3d7fff"),
        stat_card("Contributors",     &fmt_num(stats.total_contributors),  "unique authors",    "#b06fd4"),
        stat_card("Repository Age",   &age,                                "first→last commit", "#00d4cc"),
        stat_card("Active Days",      &fmt_num(stats.active_days),         "days with commits", "#ff8c00"),
        stat_card("Avg/Week",         &format!("{:.1}", stats.avg_commits_per_week), "commits per week",    "#4db84d"),
        stat_card("Longest Streak",   &format!("{} days", stats.longest_streak),    "consecutive days",    "#ffcc00"),
        stat_card("Avg Commit Size",  &format!("{:.0} lines", stats.avg_commit_size),"insertions+deletions","#ff7040"),
        stat_card("Busiest Day",
            stats.busiest_day.as_ref().map(|(d,_)| d.as_str()).unwrap_or("—"),
            &stats.busiest_day.as_ref().map(|(_, n)| format!("{n} commits")).unwrap_or_default(),
            "#9ab0cc"),
    );

    // ── Heatmap ──────────────────────────────────────────────────────────────
    let heatmap = heatmap_svg(&stats.commits_by_day);

    // ── Hour chart (with per-bar tooltip labels) ──────────────────────────────
    const HOUR_LBLS: [&str; 24] = [
        "0","","","","","","6","","","","","","12","","","","","","18","","","","","23",
    ];
    let hour_tips: Vec<String> = (0..24usize).map(|h| format!("{h:02}:00")).collect();
    let hour_tip_refs: Vec<&str> = hour_tips.iter().map(|s| s.as_str()).collect();
    let hour_chart = bar_chart_v(&stats.commits_by_hour, &HOUR_LBLS, &hour_tip_refs, "#3d7fff", 500);

    // ── Weekday chart (with per-bar tooltip labels) ───────────────────────────
    const WD_LBLS: [&str; 7]  = ["Mon","Tue","Wed","Thu","Fri","Sat","Sun"];
    const WD_TIPS: [&str; 7]  = ["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"];
    let weekday_chart = bar_chart_v(&stats.commits_by_weekday, &WD_LBLS, &WD_TIPS, "#b06fd4", 280);

    // ── Contributors by commits ───────────────────────────────────────────────
    let max_cc = stats.top_contributors.iter().map(|c| c.commit_count).max().unwrap_or(1);
    let contrib_items: Vec<(&str, usize, Option<f32>)> = stats.top_contributors.iter()
        .map(|c| (c.name.as_str(), c.commit_count, Some(c.percentage)))
        .collect();
    let contrib_chart = bar_chart_h(&contrib_items, "#3d7fff", max_cc);

    // ── Contributors by lines ─────────────────────────────────────────────────
    let changer_items: Vec<(&str, usize, usize)> = stats.top_changers.iter()
        .map(|c| (c.name.as_str(), c.lines_added, c.lines_deleted))
        .collect();
    let changers_chart = bar_chart_h_split(&changer_items);

    // ── File types ────────────────────────────────────────────────────────────
    let max_ft = stats.file_type_breakdown.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let ft_items: Vec<(&str, usize, Option<f32>)> = stats.file_type_breakdown.iter()
        .map(|(ext, c)| (ext.as_str(), *c, None))
        .collect();
    let ft_chart = bar_chart_h(&ft_items, "#00d4cc", max_ft);

    // ── Most changed files ────────────────────────────────────────────────────
    let max_mf = stats.most_changed_files.iter().map(|f| f.change_count).max().unwrap_or(1);
    let mf_items: Vec<(&str, usize, Option<f32>)> = stats.most_changed_files.iter()
        .take(20)
        .map(|f| {
            let short = f.path.rsplit('/').next()
                .or_else(|| f.path.rsplit('\\').next())
                .unwrap_or(f.path.as_str());
            (short, f.change_count, None)
        })
        .collect();
    let mf_chart = bar_chart_h(&mf_items, "#ff8c00", max_mf);

    // ── First / last commit timestamps ────────────────────────────────────────
    let first_date = unix_to_iso(stats.first_commit_time);
    let last_date  = unix_to_iso(stats.last_commit_time);

    let repo_esc = esc(repo_name);

    let busiest_html = stats.busiest_day.as_ref().map(|(d, n)| {
        format!("<span><strong style='color:#abb2bf'>Busiest day</strong><br>{d} ({n} commits)</span>")
    }).unwrap_or_default();

    // Plugin-supplied SVG wins; fall back to the bundled Arbor mark. We trust
    // the caller to have validated the override is well-formed SVG — it's
    // dropped straight into the document.
    let logo: &str = logo_override.unwrap_or(LOGO_SVG);
    let css   = CSS;
    let js    = TOOLTIP_JS;
    let nc    = stats.top_contributors.len();
    let nl    = stats.top_changers.len();
    let nft   = stats.file_type_breakdown.len();
    let nmf   = stats.most_changed_files.len().min(20);

    // ── Assemble HTML ─────────────────────────────────────────────────────────
    format!(
r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Statistics — {repo_esc}</title>
<style>{css}</style>
</head>
<body>
<div class="page">

<div class="page-hdr">
  {logo}
  <h1>{repo_esc}</h1>
</div>
<p class="meta">Repository Statistics · Exported {date} · Generated by Arbor</p>

<section>
<h2>Overview</h2>
{cards}

<h3>Commit Activity — Last 12 Months</h3>
<div class="box">{heatmap}</div>

<div class="two">
  <div>
    <h3>Commits by Hour of Day</h3>
    <div class="box">{hour_chart}</div>
  </div>
  <div>
    <h3>Commits by Day of Week</h3>
    <div class="box">{weekday_chart}</div>
  </div>
</div>

<h3>Timeline</h3>
<div class="box" style="font-size:12px;color:#8b97a7;display:flex;gap:40px">
  <span><strong style="color:#abb2bf">First commit</strong><br>{first_date}</span>
  <span><strong style="color:#abb2bf">Last commit</strong><br>{last_date}</span>
  {busiest_html}
</div>
</section>

<section>
<h2>Contributors</h2>
<div class="two">
  <div>
    <h3>By Commits (top {nc})</h3>
    <div class="box">{contrib_chart}</div>
  </div>
  <div>
    <h3>By Lines Changed (top {nl})</h3>
    <div class="box">{changers_chart}</div>
  </div>
</div>
</section>

<section>
<h2>Files</h2>
<div class="two">
  <div>
    <h3>By File Type (top {nft})</h3>
    <div class="box">{ft_chart}</div>
  </div>
  <div>
    <h3>Most Changed (top {nmf})</h3>
    <div class="box">{mf_chart}</div>
  </div>
</div>
</section>

<footer>
  {logo}
  <span>Generated by <strong>Arbor</strong> · {date}</span>
</footer>
</div>
<script>{js}</script>
</body>
</html>"##,
        repo_esc       = repo_esc,
        css            = css,
        logo           = logo,
        js             = js,
        date           = date,
        cards          = cards,
        heatmap        = heatmap,
        hour_chart     = hour_chart,
        weekday_chart  = weekday_chart,
        first_date     = first_date,
        last_date      = last_date,
        busiest_html   = busiest_html,
        nc             = nc,
        nl             = nl,
        nft            = nft,
        nmf            = nmf,
        contrib_chart  = contrib_chart,
        changers_chart = changers_chart,
        ft_chart       = ft_chart,
        mf_chart       = mf_chart,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_excluded: the pure path-matching predicate ──────────────────────

    #[test]
    fn is_excluded_matches_extension_with_or_without_leading_dot() {
        let excl = StatsExclude {
            extensions: vec![".lock".into(), "ron".into()],
            ..Default::default()
        };
        assert!(is_excluded("Cargo.lock", &excl));
        assert!(is_excluded("assets/data.ron", &excl));
        // Case-insensitive on the extension.
        assert!(is_excluded("UPPER.LOCK", &excl));
        // Unlisted extension passes through.
        assert!(!is_excluded("src/main.rs", &excl));
    }

    #[test]
    fn is_excluded_matches_folder_prefix_not_substring() {
        let excl = StatsExclude {
            folders: vec!["vendor".into(), "assets/generated/".into()],
            ..Default::default()
        };
        assert!(is_excluded("vendor/lib.rs", &excl));
        assert!(is_excluded("assets/generated/sprite.png", &excl));
        // Exact folder path itself (no trailing slash) also excluded.
        assert!(is_excluded("vendor", &excl));
        // A sibling that merely *starts with* the prefix text is NOT excluded.
        assert!(!is_excluded("vendored/lib.rs", &excl));
        assert!(!is_excluded("src/vendor.rs", &excl));
    }

    #[test]
    fn is_excluded_matches_exact_path_or_basename() {
        let excl = StatsExclude {
            files: vec!["Cargo.lock".into(), "docs/CHANGELOG.md".into()],
            ..Default::default()
        };
        // Basename match anywhere in the tree.
        assert!(is_excluded("crates/foo/Cargo.lock", &excl));
        // Exact relative-path match.
        assert!(is_excluded("docs/CHANGELOG.md", &excl));
        // Same basename but the rule was a full path → no basename match here.
        assert!(!is_excluded("src/CHANGELOG.md", &excl));
        // Unrelated file passes.
        assert!(!is_excluded("README.md", &excl));
    }

    #[test]
    fn empty_exclude_excludes_nothing() {
        let excl = StatsExclude::default();
        assert!(!is_excluded("anything/at/all.rs", &excl));
    }

    // ── empty_stats: shape invariants the frontend relies on ───────────────

    #[test]
    fn empty_stats_has_fixed_size_distributions() {
        let s = empty_stats();
        assert_eq!(s.total_commits, 0);
        // Heatmap/charts index these by fixed bucket — must stay 24 / 7 wide.
        assert_eq!(s.commits_by_hour.len(), 24);
        assert_eq!(s.commits_by_weekday.len(), 7);
        assert!(s.commits_by_hour.iter().all(|&c| c == 0));
        assert!(s.top_contributor_week.is_none());
        assert_eq!(s.avg_commit_size, 0.0);
    }

    // ── date helpers: Hinnant civil-date round-trip ────────────────────────

    #[test]
    fn unix_epoch_maps_to_thursday_1970() {
        // 1970-01-01 is the epoch and a Thursday → weekday index 3 (Mon-based).
        assert_eq!(unix_to_iso_date(0), "1970-01-01");
        assert_eq!(unix_to_weekday(0), 3);
        assert_eq!(unix_to_hour(0), 0);
    }

    #[test]
    fn known_timestamp_decodes_to_expected_civil_date() {
        // 2021-01-01 00:00:00 UTC = 1_609_459_200.
        assert_eq!(unix_to_iso_date(1_609_459_200), "2021-01-01");
        // 14:30 of that day → hour bucket 14.
        assert_eq!(unix_to_hour(1_609_459_200 + 14 * 3600 + 30 * 60), 14);
    }

    // ── export: format dispatch + error string ─────────────────────────────

    #[test]
    fn export_rejects_unknown_format() {
        let stats = empty_stats();
        let err = export_to_file(
            &stats,
            Path::new("/tmp/whatever"),
            "pdf",
            "demo",
            None,
        )
        .unwrap_err();
        assert_eq!(err, "Unknown format 'pdf'. Expected 'json' or 'html'.");
    }

    #[test]
    fn generate_html_embeds_repo_name_and_default_logo() {
        let stats = empty_stats();
        let html = generate_html(&stats, "my-repo", None);
        assert!(html.contains("<title>Statistics — my-repo</title>"));
        // Default Arbor mark is embedded when no override is supplied.
        assert!(html.contains("id=\"al-bg\""));
        // Override wins when present.
        let html2 = generate_html(&stats, "r", Some("<svg id='custom'></svg>"));
        assert!(html2.contains("<svg id='custom'></svg>"));
    }
}
