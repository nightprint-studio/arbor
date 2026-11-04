//! The history questions Imber asks beyond the plain `corvus_log`: filter by author, words in the message
//! and dates, walk a branch or a `a..b` range, and show one commit.
//!
//! The existing MCP log answers "what happened to this file"; "what did Marta change in March" and
//! "summarise everything on `dev` that is not on `main`" need the filters and the ranges. They are
//! read-only walks over libgit2, bounded so a huge repository cannot turn one question into a minute.

use git2::{Repository, Sort};

use crate::mcp_tools::{commit_touches, ymd, LogEntry};

pub const DEFAULT_LIMIT: usize = 30;
pub const MAX_LIMIT: usize = 200;
/// Commits examined before giving up on finding `limit` matches — bounds the walk on a huge history.
const MAX_SCAN: usize = 50_000;

#[derive(Debug, Default, Clone)]
pub struct LogQuery {
    pub limit: Option<usize>,
    /// Only commits that changed this path (repo-relative, forward slashes).
    pub path: Option<String>,
    /// Case-insensitive part of the author's name or email.
    pub author: Option<String>,
    /// Case-insensitive words the whole message must contain.
    pub search: Option<String>,
    /// `YYYY-MM-DD`, inclusive.
    pub since: Option<String>,
    pub until: Option<String>,
    /// A branch, tag, commit, or `a..b` range. HEAD when empty.
    pub rev: Option<String>,
}

/// Unix seconds at 00:00 UTC of a `YYYY-MM-DD` day (the inverse of the date the log prints).
pub fn day_start(day: &str) -> Option<i64> {
    let mut parts = day.trim().split('-');
    let (y, m, d): (i64, i64, i64) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from the civil calendar (Howard Hinnant's algorithm), the same arithmetic `ymd` undoes.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400)
}

pub fn log(repo: &Repository, query: &LogQuery) -> Result<Vec<LogEntry>, String> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let since = query.since.as_deref().map(|d| day_start(d).ok_or_else(|| format!("'{d}' is not a date; use YYYY-MM-DD"))).transpose()?;
    // Inclusive of the whole last day.
    let until = query.until.as_deref().map(|d| day_start(d).map(|s| s + 86_400 - 1).ok_or_else(|| format!("'{d}' is not a date; use YYYY-MM-DD"))).transpose()?;
    let author = query.author.as_deref().map(str::to_lowercase).filter(|a| !a.is_empty());
    let search = query.search.as_deref().map(str::to_lowercase).filter(|s| !s.is_empty());

    let mut walk = repo.revwalk().map_err(|e| e.to_string())?;
    match query.rev.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        Some(range) if range.contains("..") => walk.push_range(range).map_err(|e| format!("bad range '{range}': {e}"))?,
        Some(rev) => {
            let id = repo.revparse_single(rev).map_err(|e| format!("unknown revision '{rev}': {e}"))?.peel_to_commit().map_err(|e| e.to_string())?.id();
            walk.push(id).map_err(|e| e.to_string())?;
        }
        None => walk.push_head().map_err(|e| format!("no commits yet: {e}"))?,
    }
    walk.set_sorting(Sort::TIME).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for (scanned, oid) in walk.enumerate() {
        if scanned >= MAX_SCAN || out.len() >= limit {
            break;
        }
        let commit = repo.find_commit(oid.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let when = commit.author().when().seconds();
        if since.is_some_and(|s| when < s) || until.is_some_and(|u| when > u) {
            continue;
        }
        if let Some(a) = &author {
            let sig = commit.author();
            let who = format!("{} <{}>", sig.name().unwrap_or(""), sig.email().unwrap_or("")).to_lowercase();
            if !who.contains(a) {
                continue;
            }
        }
        if let Some(s) = &search {
            if !commit.message().unwrap_or("").to_lowercase().contains(s) {
                continue;
            }
        }
        if let Some(p) = &query.path {
            if !commit_touches(&commit, p) {
                continue;
            }
        }
        let id = commit.id().to_string();
        out.push(LogEntry {
            short: id.chars().take(8).collect(),
            commit: id,
            author: commit.author().name().unwrap_or("Unknown").to_string(),
            date: ymd(when),
            summary: commit.summary().unwrap_or("").to_string(),
        });
    }
    Ok(out)
}

/// One commit's header: who, when, the full message, and its parents — the part of `git show` above the patch.
pub fn show_header(repo: &Repository, rev: &str) -> Result<(String, String), String> {
    let commit = repo.revparse_single(rev).map_err(|e| format!("unknown revision '{rev}': {e}"))?.peel_to_commit().map_err(|e| e.to_string())?;
    let sig = commit.author();
    let parents: Vec<String> = commit.parent_ids().map(|p| p.to_string().chars().take(8).collect()).collect();
    let header = format!(
        "commit {}\nAuthor: {} <{}>\nDate:   {}\nParents: {}\n\n{}",
        commit.id(),
        sig.name().unwrap_or("Unknown"),
        sig.email().unwrap_or(""),
        ymd(sig.when().seconds()),
        if parents.is_empty() { "(root commit)".to_string() } else { parents.join(" ") },
        commit.message().unwrap_or("").trim_end()
    );
    Ok((commit.id().to_string(), header))
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Signature, Time};

    #[test]
    fn a_day_is_its_midnight_utc() {
        assert_eq!(day_start("1970-01-01"), Some(0));
        assert_eq!(day_start("1970-01-02"), Some(86_400));
        assert_eq!(day_start("2000-03-01"), Some(951_868_800));
        assert_eq!(day_start("2024-02-29"), Some(1_709_164_800), "leap day");
        assert_eq!(ymd(day_start("2026-11-03").unwrap()), "2026-11-03", "it undoes what the log prints");
    }

    #[test]
    fn a_malformed_day_is_refused() {
        for bad in ["", "2024", "2024-13-01", "2024-00-10", "2024-01-32", "2024-01-01-01", "yesterday"] {
            assert_eq!(day_start(bad), None, "{bad}");
        }
    }

    /// A throwaway repository with commits at known times: (author, message, unix time).
    fn repo_with(commits: &[(&str, &str, i64)]) -> (std::path::PathBuf, Repository) {
        let dir = std::env::temp_dir().join(format!("corvus-imber-history-{}-{}", std::process::id(), commits.len()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = Repository::init(&dir).unwrap();
        let mut parent: Option<git2::Oid> = None;
        for (i, (author, message, at)) in commits.iter().enumerate() {
            std::fs::write(dir.join("f.txt"), format!("{i}")).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("f.txt")).unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let sig = Signature::new(author, &format!("{}@x.io", author.to_lowercase()), &Time::new(*at, 0)).unwrap();
            let parents: Vec<git2::Commit> = parent.iter().map(|p| repo.find_commit(*p).unwrap()).collect();
            let refs: Vec<&git2::Commit> = parents.iter().collect();
            parent = Some(repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &refs).unwrap());
        }
        (dir, repo)
    }

    #[test]
    fn the_log_filters_by_author_words_and_dates() {
        let day = |d: &str| day_start(d).unwrap() + 3_600;
        let (dir, repo) = repo_with(&[("Marta", "feat: coupons", day("2026-03-01")), ("Luca", "fix: rounding bug", day("2026-03-15")), ("Marta", "fix: cart crash", day("2026-04-02"))]);
        let subjects = |q: LogQuery| log(&repo, &q).unwrap().into_iter().map(|e| e.summary).collect::<Vec<_>>();

        assert_eq!(subjects(LogQuery::default()), ["fix: cart crash", "fix: rounding bug", "feat: coupons"], "newest first");
        assert_eq!(subjects(LogQuery { author: Some("marta".into()), ..Default::default() }), ["fix: cart crash", "feat: coupons"]);
        assert_eq!(subjects(LogQuery { search: Some("FIX".into()), ..Default::default() }), ["fix: cart crash", "fix: rounding bug"]);
        assert_eq!(subjects(LogQuery { since: Some("2026-03-10".into()), until: Some("2026-03-31".into()), ..Default::default() }), ["fix: rounding bug"]);
        assert_eq!(subjects(LogQuery { limit: Some(1), ..Default::default() }), ["fix: cart crash"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_range_and_a_bad_revision_are_handled() {
        let (dir, repo) = repo_with(&[("A", "one", 1_000_000), ("A", "two", 2_000_000), ("A", "three", 3_000_000), ("A", "four", 4_000_000), ("A", "five", 5_000_000)]);
        let first = log(&repo, &LogQuery { limit: Some(10), ..Default::default() }).unwrap();
        let base = &first[3].commit; // "two"
        let range = log(&repo, &LogQuery { rev: Some(format!("{base}..HEAD")), ..Default::default() }).unwrap();
        assert_eq!(range.iter().map(|e| e.summary.as_str()).collect::<Vec<_>>(), ["five", "four", "three"]);
        assert!(log(&repo, &LogQuery { rev: Some("no-such-branch".into()), ..Default::default() }).unwrap_err().contains("unknown revision"));
        assert!(log(&repo, &LogQuery { since: Some("soon".into()), ..Default::default() }).unwrap_err().contains("not a date"));
        let (id, header) = show_header(&repo, "HEAD").unwrap();
        assert_eq!(id, first[0].commit);
        assert!(header.contains("five") && header.contains("Parents:"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
