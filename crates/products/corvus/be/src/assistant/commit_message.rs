//! The request that turns a staged diff into a commit message, and the reading of its answer.
//!
//! The model is not asked for "a good message" in the abstract: it is told how *this repository* writes
//! them — Conventional Commits, a ticket prefix, or plain sentences — measured from the recent history,
//! plus the `commit.template` when there is one and the ticket the branch name carries. Pure functions
//! over strings, so the conventions the prompt claims are the ones the history actually shows.

use regex::Regex;

/// How the recent history writes its subject lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Convention {
    /// `type(scope): subject`.
    Conventional,
    /// `ABC-123 subject` or `[ABC-123] subject`.
    TicketPrefix,
    Plain,
}

/// What the history says about style, measured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    pub convention: Convention,
    /// The subject length the history keeps to, clamped to what git tooling shows comfortably.
    pub subject_max: usize,
}

const MIN_SUBJECT: usize = 50;
const MAX_SUBJECT: usize = 72;
/// A convention is claimed when this share of the subjects follows it.
const MAJORITY_PERCENT: usize = 60;

/// Read the style off the recent subjects (newest first). Merge commits and reverts say nothing about
/// how people write, so they are skipped.
pub fn detect_style(subjects: &[String]) -> Style {
    let own: Vec<&str> = subjects.iter().map(String::as_str).filter(|s| !s.starts_with("Merge ") && !s.starts_with("Revert ")).collect();
    let conventional = Regex::new(r"^[a-z]+(\([^)]+\))?!?: \S").expect("static pattern");
    let ticket = Regex::new(r"^\[?[A-Z][A-Z0-9]+-\d+\]?[: ]").expect("static pattern");
    let share = |re: &Regex| own.iter().filter(|s| re.is_match(s)).count() * 100;
    let convention = if !own.is_empty() && share(&conventional) >= MAJORITY_PERCENT * own.len() {
        Convention::Conventional
    } else if !own.is_empty() && share(&ticket) >= MAJORITY_PERCENT * own.len() {
        Convention::TicketPrefix
    } else {
        Convention::Plain
    };
    let mut lengths: Vec<usize> = own.iter().map(|s| s.chars().count()).collect();
    lengths.sort_unstable();
    // The 90th percentile: what the history stays under, not what its one long outlier reached.
    let subject_max = lengths.get(lengths.len().saturating_sub(1) * 9 / 10).copied().unwrap_or(MAX_SUBJECT).clamp(MIN_SUBJECT, MAX_SUBJECT);
    Style { convention, subject_max }
}

/// A ticket key in the branch name (`feature/ABC-123-cart` → `ABC-123`), if there is one.
pub fn ticket_in_branch(branch: &str) -> Option<String> {
    Regex::new(r"[A-Za-z][A-Za-z0-9]+-\d+").expect("static pattern").find(branch).map(|m| m.as_str().to_uppercase())
}

/// Everything the request is built from.
pub struct Inputs<'a> {
    /// The staged patch, already within its budget.
    pub diff: &'a str,
    /// Staged paths, for when the patch was cut.
    pub files: &'a [String],
    /// The latest subjects of this repository, newest first.
    pub recent_subjects: &'a [String],
    /// The text of `commit.template`, when set.
    pub template: Option<&'a str>,
    pub branch: &'a str,
    /// What the user typed in the box already, or a hint such as "mention the migration".
    pub hint: &'a str,
}

pub const SYSTEM: &str = "You write git commit messages. Reply with the message only: no code fences, no quotes, \
no preamble, no explanation. The subject line says what the change does, in the imperative mood, without a trailing period. \
When the change is more than trivial, add a blank line and a short body that says why, wrapped at about 72 columns. \
Describe only what the diff shows; never invent a ticket, a reason or a side effect that is not in it. \
Write in the language the repository's recent messages are written in.";

/// The user turn: the repository's habits first, then the change.
pub fn user_prompt(inputs: &Inputs<'_>) -> String {
    let style = detect_style(inputs.recent_subjects);
    let mut p = String::new();
    p.push_str("Write the commit message for the staged change below.\n\n## How this repository writes commits\n");
    match style.convention {
        Convention::Conventional => p.push_str("- Conventional Commits: `type(scope): subject` (feat, fix, docs, refactor, test, chore, …).\n"),
        Convention::TicketPrefix => p.push_str("- A ticket key opens the subject, as in the examples below.\n"),
        Convention::Plain => p.push_str("- Plain sentences, no prefix convention.\n"),
    }
    p.push_str(&format!("- Subject line: at most {} characters.\n", style.subject_max));
    if let Some(ticket) = ticket_in_branch(inputs.branch) {
        p.push_str(&format!("- The branch `{}` carries the ticket {ticket}; use it if this repository's messages do.\n", inputs.branch));
    }
    if !inputs.recent_subjects.is_empty() {
        p.push_str("\nRecent subjects, newest first:\n");
        for s in inputs.recent_subjects.iter().take(12) {
            p.push_str(&format!("- {s}\n"));
        }
    }
    if let Some(t) = inputs.template.map(str::trim).filter(|t| !t.is_empty()) {
        p.push_str(&format!("\nThe repository's commit template (follow its structure; lines starting with # are comments):\n```\n{t}\n```\n"));
    }
    if !inputs.hint.trim().is_empty() {
        p.push_str(&format!("\nThe user wrote this in the message box; keep its intent and improve it:\n```\n{}\n```\n", inputs.hint.trim()));
    }
    p.push_str(&format!("\n## Staged files\n{}\n\n## Staged change\n```diff\n{}\n```\n", inputs.files.join("\n"), inputs.diff.trim_end()));
    p
}

/// The model's answer as a message: fences, quotes and a "Commit message:" lead-in removed, template
/// comments dropped, blank-line runs collapsed.
pub fn clean_reply(raw: &str) -> String {
    let mut text = raw.trim();
    // A fenced block: take what is inside the first fence.
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let body = after.split_once('\n').map_or(after, |(_lang, rest)| rest);
        text = body.split("```").next().unwrap_or(body).trim();
    }
    let lines: Vec<&str> = text.lines().map(str::trim_end).filter(|l| !l.starts_with('#')).collect();
    let mut joined = lines.join("\n");
    for lead in ["commit message:", "message:", "subject:"] {
        if joined.to_lowercase().starts_with(lead) {
            joined = joined[lead.len()..].trim_start().to_string();
            break;
        }
    }
    let unquoted = joined.trim().trim_matches(|c| c == '"' || c == '`').trim();
    let mut out = String::new();
    let mut blank_run = 0;
    for line in unquoted.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subjects(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_history_of_typed_subjects_is_conventional_commits() {
        let s = detect_style(&subjects(&["feat(cart): add coupons", "fix: round totals", "docs: update readme", "Merge branch 'x'", "wip"]));
        assert_eq!(s.convention, Convention::Conventional, "merges are ignored and 3 of 4 follow it");
    }

    #[test]
    fn a_history_of_ticket_keys_is_a_ticket_prefix() {
        let s = detect_style(&subjects(&["ABC-12 fix totals", "[ABC-13] add coupons", "ABC-14: rename", "cleanup"]));
        assert_eq!(s.convention, Convention::TicketPrefix);
    }

    #[test]
    fn mixed_or_empty_history_claims_no_convention() {
        assert_eq!(detect_style(&subjects(&["feat: a", "Update thing", "Fix other", "More work"])).convention, Convention::Plain);
        assert_eq!(detect_style(&[]).convention, Convention::Plain);
    }

    #[test]
    fn the_subject_length_follows_the_history_within_bounds() {
        let short: Vec<String> = (0..10).map(|i| format!("fix: {i}")).collect();
        assert_eq!(detect_style(&short).subject_max, MIN_SUBJECT, "never tighter than 50");
        let long: Vec<String> = (0..10).map(|_| "x".repeat(120)).collect();
        assert_eq!(detect_style(&long).subject_max, MAX_SUBJECT, "never looser than 72");
    }

    #[test]
    fn the_ticket_is_read_from_the_branch_name() {
        assert_eq!(ticket_in_branch("feature/abc-123-cart"), Some("ABC-123".into()));
        assert_eq!(ticket_in_branch("PROJ-9"), Some("PROJ-9".into()));
        assert_eq!(ticket_in_branch("main"), None);
    }

    #[test]
    fn the_prompt_carries_the_style_the_template_and_the_change() {
        let recent = subjects(&["feat: add a", "fix: b"]);
        let files = vec!["src/a.rs".to_string()];
        let p = user_prompt(&Inputs { diff: "--- a\n+++ a\n+x\n", files: &files, recent_subjects: &recent, template: Some("type(scope): subject\n\n# why"), branch: "feature/ABC-7-x", hint: "mention coupons" });
        assert!(p.contains("Conventional Commits"));
        assert!(p.contains("ABC-7"));
        assert!(p.contains("- feat: add a"));
        assert!(p.contains("commit template"));
        assert!(p.contains("mention coupons"));
        assert!(p.contains("src/a.rs") && p.contains("+x"));
    }

    #[test]
    fn a_prompt_without_template_or_hint_does_not_mention_them() {
        let p = user_prompt(&Inputs { diff: "d", files: &[], recent_subjects: &[], template: None, branch: "main", hint: "  " });
        assert!(!p.contains("commit template") && !p.contains("message box"));
    }

    #[test]
    fn the_answer_is_cleaned_of_fences_quotes_and_lead_ins() {
        assert_eq!(clean_reply("```\nfeat: add coupons\n\nWhy.\n```"), "feat: add coupons\n\nWhy.");
        assert_eq!(clean_reply("```text\nfix: x\n```"), "fix: x");
        assert_eq!(clean_reply("\"fix: quoted\""), "fix: quoted");
        assert_eq!(clean_reply("Commit message: fix: lead"), "fix: lead");
        assert_eq!(clean_reply("  fix: spaced  \n\n\n\nbody\n"), "fix: spaced\n\nbody");
    }

    #[test]
    fn template_comment_lines_are_dropped_from_the_answer() {
        assert_eq!(clean_reply("fix: a\n\n# Please enter the message\nbody"), "fix: a\n\nbody");
    }
}
