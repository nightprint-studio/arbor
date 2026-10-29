//! The assistant's git tools: Corvus's own, reached through the shell. Off until the user turns them on.
//!
//! Corvus already serves git as tools for AI clients — status, diff, log, blame, stage, unstage, commit —
//! with descriptions written for a model and schemas to match. The assistant does not rebuild any of that:
//! it asks the shell for those descriptors (the shell lists them from Corvus, narrowed to an allowlist) and
//! calls them back through the same relay. `push` is not among them and never will be: publishing is the
//! user's to do.
//!
//! Two things make this opt-in rather than ambient, and they are the reason it is a Settings choice and off
//! by default: git results carry **people** (authors, in names and addresses, and in commit messages) which
//! would otherwise go to the provider — see `pseudonyms` — and `commit` changes the repository.

use arbor_rpc::tool::ToolDescriptor;
use bennu_core::prelude::BennuState;
use serde_json::Value;

/// The Corvus tools the assistant may be given. The shell enforces the same list; this is the second lock.
pub(super) const ALLOWED: [&str; 7] = [
    "corvus_status", "corvus_diff", "corvus_log", "corvus_blame", "corvus_stage", "corvus_unstage", "corvus_commit",
];

/// Keep only what the allowlist names, whatever the shell sent.
pub(super) fn keep_allowed(tools: Vec<ToolDescriptor>) -> Vec<ToolDescriptor> {
    tools.into_iter().filter(|t| ALLOWED.contains(&t.method.as_str())).collect()
}

/// The git tools available now. Empty when Corvus is not there — which a run treats as "no git", quietly.
pub(super) fn load(ctx: &BennuState) -> Vec<ToolDescriptor> {
    let Ok(value) = ctx.host_call("__assistant_git_tools", Value::Null) else { return Vec::new() };
    keep_allowed(serde_json::from_value(value).unwrap_or_default())
}

/// A tool that changes the repository for good enough that it is asked about even in Auto: a commit is
/// a record, and what goes in it is the user's to see first.
pub(super) fn always_asks(name: &str) -> bool {
    name == "corvus_commit"
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_rpc::tool::{Safety, ToolOutput};
    use serde_json::json;

    fn tool(method: &str) -> ToolDescriptor {
        ToolDescriptor {
            name: method.into(), method: method.into(), title: method.into(), description: String::new(),
            input_schema: json!({}), safety: Safety::Read, idempotent: true, open_world: false,
            wrap_in: Some("args".into()), output: ToolOutput::Json,
        }
    }

    #[test]
    fn only_the_listed_tools_survive_and_push_is_never_among_them() {
        let kept = keep_allowed(vec![tool("corvus_status"), tool("corvus_push"), tool("corvus_commit"), tool("something_else")]);
        let names: Vec<&str> = kept.iter().map(|t| t.method.as_str()).collect();
        assert_eq!(names, ["corvus_status", "corvus_commit"]);
        assert!(!ALLOWED.contains(&"corvus_push"));
    }

    #[test]
    fn a_commit_is_asked_about_even_when_edits_are_automatic() {
        assert!(always_asks("corvus_commit"));
        assert!(!always_asks("corvus_stage") && !always_asks("corvus_log"));
    }
}
