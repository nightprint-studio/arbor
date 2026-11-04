//! The tools Imber has in Corvus: what the model is told it can call, and what calling one does.
//!
//! **No git is reimplemented here.** Every tool is a thin call into the handler the UI's own button
//! reaches (stage, commit, fetch, pull, branch, merge, MR, …) or into the read-only MCP tools, so a
//! commit made by Imber runs the same pre-commit hooks, recovery snapshots and credential flow as one
//! made by hand. What this file adds is the model-facing shape: a schema per tool, the repository
//! implied by the session (the model never types a path to switch repository), and results sized to be
//! read — a diff is text within a budget, not a tree of hunks.
//!
//! Which calls may run unasked is not decided here: that is [`super::permission`]'s table, consulted by
//! the host before [`run`] is reached.

use std::path::Path;

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{get_commit_diff, MergeStrategy};
use corvus_git_provider_api::prelude::CreateMrParams;
use imber_core::prelude::ToolSpec;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use super::diff_text;
use super::undo;
use super::history::{self, LogQuery};
use crate::ephemeral::tab_for;
use crate::mcp_tools::{self as mcp, relative_to, BlameArgs, CommitArgs, DiffArgs, PushArgs, RepoArgs, StageArgs};
use crate::repo::{open, repo_path};

/// Characters of a diff or a show that reach the model.
const DIFF_BUDGET: usize = 24_000;
/// The longest tool result that is sent whole.
const RESULT_MAX: usize = 30_000;

/// Everything a tool needs: the process state, a handle to block on the async handlers from the run's
/// own thread, and the repository this conversation is about.
pub struct Ctx<'a> {
    pub state: &'a CorvusState,
    pub handle: &'a tokio::runtime::Handle,
    pub root: &'a str,
    pub conversation_id: &'a str,
    pub mode: super::permission::Mode,
    /// A scheduled run: nobody is there to approve.
    pub unattended: bool,
}

fn spec(name: &str, description: &str, properties: Value, required: &[&str]) -> ToolSpec {
    ToolSpec { name: name.into(), description: description.into(), input_schema: json!({ "type": "object", "properties": properties, "required": required }) }
}

fn s(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn flag(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

fn strings(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": description })
}

/// The tools of a conversation about a repository. A chat has none.
pub fn specs() -> Vec<ToolSpec> {
    let mut all = vec![
        spec("git_status", "Branch, ahead/behind, and every changed file split into staged / unstaged / untracked / conflicted. Read this before committing.", json!({}), &[]),
        spec(
            "git_diff",
            "The patch of the uncommitted work. staged=true is exactly what a commit would contain; false (default) is what is not staged yet.",
            json!({ "staged": flag("Index against HEAD instead of working tree against index."), "path": s("Narrow to one file or folder, repo-relative."), "context_lines": json!({"type": "integer", "description": "Unchanged lines around each change (default 3)."}) }),
            &[],
        ),
        spec(
            "git_log",
            "Commit history, newest first. Filter by file, author, words in the message, dates, or give a branch / tag / `a..b` range (e.g. `main..feature/x` = what feature/x adds over main).",
            json!({
                "limit": json!({"type": "integer", "description": "Default 30, at most 200."}),
                "path": s("Only commits that changed this file or folder."),
                "author": s("Part of the author's name or email."),
                "search": s("Words the commit message must contain."),
                "since": s("YYYY-MM-DD, inclusive."),
                "until": s("YYYY-MM-DD, inclusive."),
                "rev": s("Branch, tag, commit, or a..b range. HEAD when omitted."),
            }),
            &[],
        ),
        spec(
            "git_show",
            "One commit: author, date, full message and its patch.",
            json!({ "commit": s("Commit hash, branch or tag."), "path": s("Only this file's part of the patch.") }),
            &["commit"],
        ),
        spec(
            "git_blame",
            "Who last changed each line of a file as of HEAD, folded into runs of lines that share a commit.",
            json!({ "file": s("File path, repo-relative or absolute."), "start_line": json!({"type": "integer"}), "end_line": json!({"type": "integer"}) }),
            &["file"],
        ),
        spec("git_branches", "Local branches with their upstream, ahead/behind and last commit.", json!({}), &[]),
        spec("git_mr_list", "Open (or closed / merged / all) merge or pull requests of this repository.", json!({ "state": s("open (default), closed, merged or all.") }), &[]),
        spec("git_stage", "Add files or folders to the index. Name what you have looked at; there is no 'everything'.", json!({ "paths": strings("Repo-relative files or folders.") }), &["paths"]),
        spec("git_unstage", "Take files or folders out of the index. The working tree is untouched.", json!({ "paths": strings("Repo-relative files or folders.") }), &["paths"]),
        spec(
            "git_commit",
            "Commit the staged changes (staging `paths` first when given). Never amends. Read git_diff with staged=true first so the message says what the commit contains.",
            json!({ "message": s("Subject line, blank line, body. Follow the repository's conventions."), "paths": strings("Files to stage before committing.") }),
            &["message"],
        ),
        spec("git_fetch", "Download the remote's new commits and branches. Changes nothing local.", json!({ "remote": s("Default origin.") }), &[]),
        spec("git_pull", "Fetch and merge the remote into the current branch (local changes are stashed and re-applied).", json!({ "remote": s("Default origin.") }), &[]),
        spec(
            "git_push",
            "Push the current branch. A plain push only fast-forwards. force=true overwrites the remote branch and always needs the user's approval.",
            json!({ "remote": s("Default origin."), "force": flag("Overwrite the remote branch.") }),
            &[],
        ),
        spec(
            "git_branch_create",
            "Create a branch at `from` (default HEAD), optionally switching to it.",
            json!({ "name": s("New branch name."), "from": s("Branch, tag or commit to start from."), "checkout": flag("Switch to it afterwards.") }),
            &["name"],
        ),
        spec("git_checkout", "Switch to an existing local branch (local changes are stashed and re-applied).", json!({ "branch": s("Branch name.") }), &["branch"]),
        spec(
            "git_merge",
            "Merge a branch into the current one. A conflict leaves the merge in progress for the user to resolve or abort.",
            json!({ "branch": s("Branch to merge in."), "strategy": json!({"type": "string", "enum": ["default", "no_ff", "ff_only", "squash"]}) }),
            &["branch"],
        ),
        spec(
            "git_mr_create",
            "Open a merge / pull request on the hosting provider. The source branch must already be pushed.",
            json!({
                "title": s("Request title."),
                "description": s("Markdown body: what changed and why."),
                "target_branch": s("Branch to merge into (see git_branches for the default)."),
                "source_branch": s("Default: the current branch."),
                "draft": flag("Open as a draft."),
                "labels": strings("Labels to apply."),
            }),
            &["title", "target_branch"],
        ),
        spec("git_branch_delete", "Delete a local branch. Always needs the user's approval.", json!({ "name": s("Branch name.") }), &["name"]),
    ];
    all.extend(super::schedule::specs());
    all.extend(super::plan_tools::specs());
    all.extend(undo_specs());
    all
}

fn arg<T: DeserializeOwned>(input: &Value) -> Result<T, String> {
    serde_json::from_value(input.clone()).map_err(|e| format!("the arguments were not valid: {e}"))
}

fn json_text(value: &impl serde::Serialize) -> Result<String, String> {
    serde_json::to_string(value).map(clip).map_err(|e| e.to_string())
}

/// A result bounded to what is worth sending: the head, and a line saying how much was left out.
fn clip(text: String) -> String {
    if text.len() <= RESULT_MAX {
        return text;
    }
    let mut end = RESULT_MAX;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[output cut: {} more characters]", &text[..end], text.len() - end)
}

fn current_branch(repo: &git2::Repository) -> Result<String, String> {
    let head = repo.head().map_err(|e| format!("Git error: {e}"))?;
    if !head.is_branch() {
        return Err("HEAD is detached, so there is no current branch.".into());
    }
    head.shorthand().map(str::to_string).ok_or_else(|| "the current branch has no printable name".into())
}

fn path_in(root: &str, file: &str) -> String {
    let p = Path::new(file);
    if p.is_absolute() { file.to_string() } else { Path::new(root).join(file).to_string_lossy().into_owned() }
}

#[derive(Deserialize)]
struct Paths {
    paths: Vec<String>,
}

#[derive(Deserialize, Default)]
struct Remote {
    remote: Option<String>,
    #[serde(default)]
    force: bool,
}

/// Run one tool. Whatever changes the repository (or leaves the machine) is written to the action log with how to
/// take it back, if git allows — see [`undo`].
pub fn run(ctx: &Ctx<'_>, name: &str, input: &Value) -> Result<String, String> {
    let tracked = undo::tracks(name);
    let before = if tracked { undo::facts(ctx.state, ctx.root).ok() } else { None };
    let out = run_inner(ctx, name, input)?;
    if tracked {
        let after = undo::facts(ctx.state, ctx.root).ok();
        let op = match (&before, &after) {
            (Some(b), Some(a)) => undo::op_for(name, input, b, a),
            _ => None,
        };
        undo::record(ctx.state, ctx.root, name, input, op);
    }
    Ok(out)
}

/// The tools that read and use the action log.
fn undo_specs() -> Vec<ToolSpec> {
    vec![
        spec("undo_list", "List what Imber did in this repository — and for each action whether it can be undone, how, or why not (a push, a merge request or a plan run left the machine).", json!({}), &[]),
        spec(
            "undo_last",
            "Undo Imber's last reversible action here (or the one named): a commit, a branch created / switched / deleted, a stage, a merge or pull. It cannot undo what left the machine. Always needs the user's approval.",
            json!({ "action_id": s("An id from undo_list. Omit for the most recent reversible action.") }),
            &[],
        ),
    ]
}

/// Run one tool. `Err` is what the model reads as a failed call.
fn run_inner(ctx: &Ctx<'_>, name: &str, input: &Value) -> Result<String, String> {
    let (state, root) = (ctx.state, ctx.root);
    match name {
        "git_status" => json_text(&mcp::corvus_status(state, RepoArgs { root: root.into() })?),
        "git_diff" => {
            #[derive(Deserialize, Default)]
            struct A { staged: Option<bool>, path: Option<String>, context_lines: Option<u32> }
            let a: A = arg(input)?;
            let r = mcp::corvus_diff(state, DiffArgs { root: root.into(), staged: a.staged, path: a.path, context_lines: a.context_lines })?;
            let head = format!("{} changes, {} file(s){}\n", if r.staged { "Staged" } else { "Unstaged" }, r.total_files, if r.truncated { " (only the first are listed; narrow with path)" } else { "" });
            Ok(head + &diff_text::render(&r.files, DIFF_BUDGET))
        }
        "git_log" => {
            #[derive(Deserialize, Default)]
            struct A { limit: Option<usize>, path: Option<String>, author: Option<String>, search: Option<String>, since: Option<String>, until: Option<String>, rev: Option<String> }
            let a: A = arg(input)?;
            let tab = tab_for(state, root)?;
            let workdir = repo_path(state, &tab)?;
            let repo = open(state, &tab)?;
            let query = LogQuery { limit: a.limit, path: a.path.map(|p| relative_to(&workdir, &p)), author: a.author, search: a.search, since: a.since, until: a.until, rev: a.rev };
            json_text(&history::log(&repo, &query)?)
        }
        "git_show" => {
            #[derive(Deserialize)]
            struct A { commit: String, path: Option<String> }
            let a: A = arg(input)?;
            let tab = tab_for(state, root)?;
            let workdir = repo_path(state, &tab)?;
            let repo = open(state, &tab)?;
            let (oid, header) = history::show_header(&repo, &a.commit)?;
            let mut files = get_commit_diff(&repo, &oid, 3, None, None).map_err(|e| e.to_string())?;
            if let Some(p) = a.path.map(|p| relative_to(&workdir, &p)).filter(|p| !p.is_empty()) {
                files.retain(|f| f.path == p || f.path.starts_with(&format!("{p}/")));
            }
            for f in &mut files {
                f.image_old = None;
                f.image_new = None;
            }
            Ok(format!("{header}\n\n{}", diff_text::render(&files, DIFF_BUDGET)))
        }
        "git_blame" => {
            #[derive(Deserialize)]
            struct A { file: String, start_line: Option<usize>, end_line: Option<usize> }
            let a: A = arg(input)?;
            let mut ranges = mcp::corvus_blame(state, BlameArgs { file: path_in(root, &a.file) })?;
            let (from, to) = (a.start_line.unwrap_or(1), a.end_line.unwrap_or(usize::MAX));
            ranges.retain(|r| r.end_line >= from && r.start_line <= to);
            json_text(&ranges)
        }
        "git_branches" => json_text(&crate::branch::list_local_branches(state, tab_for(state, root)?)?),
        "git_mr_list" => {
            #[derive(Deserialize, Default)]
            struct A { state: Option<String> }
            let a: A = arg(input)?;
            let tab = tab_for(state, root)?;
            let mrs = ctx.handle.block_on(crate::mr::list_mrs(state, tab, a.state))?;
            let rows: Vec<Value> = mrs
                .iter()
                .map(|m| json!({ "number": m.number, "title": m.title, "source": m.source_branch, "target": m.target_branch, "state": m.state, "draft": m.is_draft, "author": m.author.login, "checks": m.checks_status, "url": m.web_url }))
                .collect();
            json_text(&rows)
        }
        "git_stage" => {
            let a: Paths = arg(input)?;
            json_text(&mcp::corvus_stage(state, StageArgs { root: root.into(), paths: a.paths })?)
        }
        "git_unstage" => {
            let a: Paths = arg(input)?;
            json_text(&mcp::corvus_unstage(state, StageArgs { root: root.into(), paths: a.paths })?)
        }
        "git_commit" => {
            #[derive(Deserialize)]
            struct A { message: String, paths: Option<Vec<String>> }
            let a: A = arg(input)?;
            json_text(&mcp::corvus_commit(state, CommitArgs { root: root.into(), message: a.message, paths: a.paths })?)
        }
        "git_fetch" => {
            let a: Remote = arg(input).unwrap_or_default();
            let tab = tab_for(state, root)?;
            json_text(&ctx.handle.block_on(crate::remote::fetch_remote(state, tab, a.remote.unwrap_or_else(|| "origin".into())))?)
        }
        "git_pull" => {
            let a: Remote = arg(input).unwrap_or_default();
            let tab = tab_for(state, root)?;
            let result = ctx.handle.block_on(crate::remote::pull_branch(state, tab, a.remote.unwrap_or_else(|| "origin".into()), None))?;
            match &result.pull_error {
                Some(e) => Err(format!("The pull failed: {e}")),
                None => json_text(&result),
            }
        }
        "git_push" => {
            let a: Remote = arg(input).unwrap_or_default();
            let remote = a.remote.unwrap_or_else(|| "origin".into());
            if a.force {
                let tab = tab_for(state, root)?;
                let branch = current_branch(&open(state, &tab)?)?;
                ctx.handle.block_on(crate::remote::push_branch(state, tab, remote.clone(), format!("refs/heads/{branch}"), true))?;
                Ok(format!("Force-pushed {branch} to {remote}."))
            } else {
                json_text(&ctx.handle.block_on(mcp::corvus_push(state, PushArgs { root: root.into(), remote: Some(remote) }))?)
            }
        }
        "git_branch_create" => {
            #[derive(Deserialize)]
            struct A { name: String, from: Option<String>, #[serde(default)] checkout: bool }
            let a: A = arg(input)?;
            let tab = tab_for(state, root)?;
            let from_oid = {
                let repo = open(state, &tab)?;
                let rev = a.from.as_deref().filter(|f| !f.is_empty()).unwrap_or("HEAD");
                let oid = repo.revparse_single(rev).map_err(|e| format!("unknown start point '{rev}': {e}"))?.peel_to_commit().map_err(|e| e.to_string())?.id().to_string();
                oid
            };
            let info = crate::branch::create_branch(state, tab.clone(), a.name.clone(), from_oid)?;
            if a.checkout {
                let switched = crate::branch::checkout_branch_safe(state, tab, a.name)?;
                json_text(&json!({ "branch": info, "checkout": switched }))
            } else {
                json_text(&info)
            }
        }
        "git_checkout" => {
            #[derive(Deserialize)]
            struct A { branch: String }
            let a: A = arg(input)?;
            json_text(&crate::branch::checkout_branch_safe(state, tab_for(state, root)?, a.branch)?)
        }
        "git_merge" => {
            #[derive(Deserialize)]
            struct A { branch: String, strategy: Option<String> }
            let a: A = arg(input)?;
            let strategy = match a.strategy.as_deref() {
                None | Some("") => None,
                Some(word) => Some(serde_json::from_value::<MergeStrategy>(json!(word)).map_err(|_| format!("'{word}' is not a merge strategy (default, no_ff, ff_only, squash)"))?),
            };
            json_text(&json!({ "outcome": crate::merge::merge_branch(state, tab_for(state, root)?, a.branch, strategy)? }))
        }
        "git_mr_create" => {
            #[derive(Deserialize)]
            struct A { title: String, description: Option<String>, target_branch: String, source_branch: Option<String>, #[serde(default)] draft: bool, #[serde(default)] labels: Vec<String> }
            let a: A = arg(input)?;
            let tab = tab_for(state, root)?;
            let source = match a.source_branch.filter(|b| !b.is_empty()) {
                Some(b) => b,
                None => current_branch(&open(state, &tab)?)?,
            };
            let params = CreateMrParams { title: a.title, description: a.description, source_branch: source, target_branch: a.target_branch, is_draft: a.draft, labels: a.labels, squash: false, delete_branch: false, auto_merge: false };
            let mr = ctx.handle.block_on(crate::mr::create_mr(state, tab, params))?;
            json_text(&json!({ "number": mr.number, "title": mr.title, "url": mr.web_url, "source": mr.source_branch, "target": mr.target_branch }))
        }
        "git_branch_delete" => {
            #[derive(Deserialize)]
            struct A { name: String }
            let a: A = arg(input)?;
            crate::branch::delete_branch(state, tab_for(state, root)?, a.name.clone())?;
            Ok(format!("Deleted branch {}.", a.name))
        }
        n if n.starts_with("schedule_") => super::schedule::run_tool(state, root, ctx.conversation_id, ctx.mode, n, input),
        n if super::plan_tools::is_plan_tool(n) => super::plan_tools::run(&super::plan_tools::Ctx { state, handle: ctx.handle, root, unattended: ctx.unattended }, n, input),
        "undo_list" => {
            let rows: Vec<Value> = undo::list(state, root)
                .iter()
                .map(|a| json!({ "id": a.id, "at": a.at_ms, "action": a.summary, "undoable": a.undo.is_some() && a.undone_ms.is_none(), "undone": a.undone_ms.is_some(), "how": a.undo.as_ref().map(undo::describe), "why_not": a.irreversible }))
                .collect();
            json_text(&rows)
        }
        "undo_last" => undo::undo(state, root, input.get("action_id").and_then(Value::as_str).filter(|s| !s.is_empty())),
        other => Err(format!("'{other}' is not a tool Imber has in Corvus.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_the_model_is_offered_has_a_risk_the_permission_table_knows() {
        use super::super::permission::{classify, Risk};
        for tool in specs() {
            // The table's fallback is Destructive; a tool landing there by omission would always ask.
            let risk = classify(&tool.name, &json!({}));
            let deliberate = matches!(tool.name.as_str(), "git_branch_delete" | "undo_last") || risk != Risk::Destructive;
            assert!(deliberate, "{} is not in the permission table", tool.name);
        }
    }

    #[test]
    fn tool_names_are_unique_and_schemas_are_objects() {
        let all = specs();
        let mut names: Vec<&str> = all.iter().map(|t| t.name.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
        for t in &all {
            assert_eq!(t.input_schema["type"], "object", "{}", t.name);
        }
    }

    #[test]
    fn required_fields_are_declared_properties() {
        for t in specs() {
            for r in t.input_schema["required"].as_array().unwrap() {
                assert!(t.input_schema["properties"].get(r.as_str().unwrap()).is_some(), "{}: {r}", t.name);
            }
        }
    }

    #[test]
    fn a_relative_file_is_joined_to_the_root_and_an_absolute_one_is_kept() {
        assert!(path_in("/repo", "src/a.rs").replace('\\', "/").ends_with("repo/src/a.rs"));
        let abs = if cfg!(windows) { "C:\\x\\a.rs" } else { "/x/a.rs" };
        assert_eq!(path_in("/repo", abs), abs);
    }

    #[test]
    fn a_long_result_is_cut_and_says_how_much() {
        let cut = clip("é".repeat(RESULT_MAX));
        assert!(cut.contains("[output cut:"), "{}", &cut[cut.len() - 40..]);
        assert_eq!(clip("short".into()), "short");
    }

    #[test]
    fn a_tool_without_valid_arguments_says_so() {
        let e = arg::<Paths>(&json!({"paths": "not a list"})).err().unwrap();
        assert!(e.contains("not valid"), "{e}");
    }
}
