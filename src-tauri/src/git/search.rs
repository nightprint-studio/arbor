use git2::{Repository, Sort};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::git::graph::AuthorInfo;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

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
