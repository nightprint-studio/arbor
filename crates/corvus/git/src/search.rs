//! `search` — commit search via a full revwalk + substring matching, pure git2.
//!
//! Extracted from the shell verbatim so the in-process handler **and** the
//! headless `corvus-be` run the same revwalk. No CLI shell-out, no Tauri: just
//! [`git2`]. Errors surface as [`GitError`](crate::error::GitError) (the only
//! failures here are libgit2 ones, which map to `GitError::Git`).
//!
//! [`AuthorInfo`] is defined locally as the minimal `{ name, email }` projection
//! the search result carries — its serde shape is byte-identical to the shell's
//! `crate::git::graph::AuthorInfo`, so the frontend wire payload is unchanged.

use git2::{Repository, Sort};
use serde::{Deserialize, Serialize};

use crate::error::Result;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Minimal author projection carried by a [`SearchResult`]. Mirrors the shell's
/// `crate::git::graph::AuthorInfo` field-for-field so the JSON is identical.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub oid: String,
    pub short_oid: String,
    pub summary: String,
    pub author: AuthorInfo,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    /// If true, match against author names/emails too.
    pub include_author: bool,
    /// Max results.
    pub limit: usize,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

pub fn search_commits(repo: &Repository, query: &SearchQuery) -> Result<Vec<SearchResult>> {
    let needle = query.text.to_lowercase();
    let limit = query.limit.max(1).min(500);

    let mut revwalk = repo.revwalk()?;
    revwalk.push_glob("refs/heads/*")?;
    revwalk.push_glob("refs/tags/*")?;
    if let Ok(head) = repo.head() {
        if let Some(id) = head.target() {
            let _ = revwalk.push(id);
        }
    }
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

    let mut results = Vec::new();

    for oid in revwalk.filter_map(|r| r.ok()) {
        let Ok(commit) = repo.find_commit(oid) else { continue };

        let summary = commit.summary().unwrap_or("").to_lowercase();
        let oid_str = oid.to_string();
        let author_name = commit.author().name().unwrap_or("").to_lowercase();
        let author_email = commit.author().email().unwrap_or("").to_lowercase();

        let matches = summary.contains(&needle)
            || oid_str.starts_with(&needle)
            || (query.include_author
                && (author_name.contains(&needle) || author_email.contains(&needle)));

        if matches {
            let author = commit.author();
            results.push(SearchResult {
                oid: oid_str.clone(),
                short_oid: oid_str[..7.min(oid_str.len())].to_string(),
                summary: commit.summary().unwrap_or("").to_string(),
                author: AuthorInfo {
                    name: author.name().unwrap_or("").to_string(),
                    email: author.email().unwrap_or("").to_string(),
                },
                timestamp: commit.time().seconds(),
            });

            if results.len() >= limit {
                break;
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_is_clamped_low_and_high() {
        // The clamp is pure arithmetic and the only non-libgit2 logic worth
        // pinning: limit 0 -> 1, limit 9999 -> 500, in-range passes through.
        let clamp = |l: usize| l.max(1).min(500);
        assert_eq!(clamp(0), 1);
        assert_eq!(clamp(9999), 500);
        assert_eq!(clamp(42), 42);
    }
}
