//! `corvus-issue-tracker-api` — provider-agnostic issue-tracker contract.
//!
//! The [`types`] DTOs are the one shape Jira / Linear (today) and GitHub /
//! GitLab Issues (once they split out of the git-provider) normalise into, so
//! the frontend renders a single model regardless of the backing tracker, plus
//! provider-agnostic helpers like [`branch_name_for_issue`].
//!
//! This is the leaf `*-api` crate (pure data + pure helpers, `serde` only). A
//! `trait IssueTracker` will join it when the per-provider impls move into their
//! own crates and a second implementation justifies the abstraction — until
//! then the host calls the provider modules directly.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — reach this crate's surface through
//! `corvus_issue_tracker_api::prelude::...`.

pub mod prelude;
pub mod types;

use types::Issue;

/// Suggest a git branch name for an issue: `{lower-identifier}-{slugified-title}`.
///
/// The title is lower-cased, every non-alphanumeric run collapses to a single
/// `-`, and the slug is capped at 40 chars (trailing `-` trimmed).
pub fn branch_name_for_issue(issue: &Issue) -> String {
    let id_lower = issue.identifier.to_lowercase();
    let slug: String = issue
        .title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    // Cap by characters, not bytes: a non-ASCII title over 40 bytes must not
    // slice through a multi-byte code point (would panic).
    let slug: String = slug.chars().take(40).collect();
    let slug = slug.trim_end_matches('-').to_string();
    format!("{id_lower}-{slug}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{BodyFormat, IssueStatus};

    fn issue(identifier: &str, title: &str) -> Issue {
        Issue {
            id: "x".into(),
            identifier: identifier.into(),
            title: title.into(),
            description: None,
            description_format: BodyFormat::Markdown,
            status: IssueStatus {
                id: "s".into(),
                name: "Todo".into(),
                color: "#fff".into(),
                status_type: "unstarted".into(),
            },
            priority: 0,
            priority_label: "None".into(),
            assignee: None,
            labels: vec![],
            url: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            due_date: None,
            estimate: None,
            team: None,
            project: None,
            cycle: None,
            comments: vec![],
            comment_count: 0,
            attachments: vec![],
        }
    }

    #[test]
    fn branch_name_lowercases_id_and_slugifies_title() {
        assert_eq!(
            branch_name_for_issue(&issue("ENG-42", "Fix the Login Bug")),
            "eng-42-fix-the-login-bug"
        );
    }

    #[test]
    fn branch_name_collapses_punctuation_and_trims() {
        assert_eq!(
            branch_name_for_issue(&issue("ARB-1", "Hello, World!!!")),
            "arb-1-hello-world"
        );
    }

    #[test]
    fn branch_name_caps_long_slugs_without_trailing_dash() {
        let long = "word ".repeat(20); // many short words → slug well over 40 chars
        let name = branch_name_for_issue(&issue("ARB-2", &long));
        let slug = name.strip_prefix("arb-2-").unwrap();
        assert!(slug.chars().count() <= 40);
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn branch_name_caps_non_ascii_titles_on_a_char_boundary() {
        // 50 accented chars: a byte-cap at 40 would slice a multi-byte code
        // point and panic; the char-cap must keep it safe and ≤ 40 chars.
        let title = "à".repeat(50);
        let name = branch_name_for_issue(&issue("ARB-3", &title));
        let slug = name.strip_prefix("arb-3-").unwrap();
        assert!(slug.chars().count() <= 40);
    }
}
