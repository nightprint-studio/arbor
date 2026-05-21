use std::collections::HashMap;
use git2::{Repository, Sort};
use serde::{Deserialize, Serialize};
use crate::config::repo_config::StatsExcludeConfig;
use crate::error::{AppError, Result};

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
fn is_excluded(path: &str, exclude: &StatsExcludeConfig) -> bool {
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

pub fn compute_stats(repo: &Repository, exclude: &StatsExcludeConfig) -> Result<RepoStats> {
    let mut walk = repo.revwalk().map_err(AppError::Git)?;

    // Empty repo: push_head fails — return zero stats gracefully.
    if walk.push_head().is_err() {
        return Ok(empty_stats());
    }
    walk.set_sorting(Sort::TIME).map_err(AppError::Git)?;

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
