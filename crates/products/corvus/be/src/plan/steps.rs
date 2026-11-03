//! Running one step: each kind calls the same handler the UI's own button reaches, on a repo
//! addressed by path (the headless `path:<dir>` tab id), so a plan behaves exactly like the
//! manual action — same credentials path, same hooks, same error strings.

use corvus_core::prelude::CorvusState;
use corvus_git_provider_api::prelude::CreateMrParams;
use serde_json::{json, Value};

use super::model::{RepoRef, StepSpec};

const REMOTE: &str = "origin";

/// What a step needs from the plan around it, resolved before it runs.
pub struct StepCtx {
    pub spec: StepSpec,
    pub repo: RepoRef,
    /// The MR a `MergeMr` step refers to, read from the `CreateMr` step that opened it.
    pub mr_number: Option<u64>,
    /// That MR's source branch, for the post-merge branch cleanup.
    pub mr_source: Option<String>,
}

/// Run the step; the value it returns is stored as the step's `result`.
pub async fn execute(state: &CorvusState, ctx: StepCtx) -> Result<Option<Value>, String> {
    let tab = crate::ephemeral::tab_for(state, &ctx.repo.path)?;
    match ctx.spec {
        StepSpec::Push { branch } => {
            crate::remote::push_branch(state, tab, REMOTE.into(), format!("refs/heads/{branch}"), false).await?;
            Ok(None)
        }
        StepSpec::CreateMr { source, target, title, description, labels, draft, auto_merge, squash, delete_branch } => {
            let params = CreateMrParams {
                title,
                description: Some(description).filter(|d| !d.trim().is_empty()),
                source_branch: source,
                target_branch: target,
                is_draft: draft,
                labels,
                squash,
                delete_branch,
                auto_merge,
            };
            let mr = crate::mr::create_mr(state, tab, params).await?;
            Ok(Some(json!({ "number": mr.number, "url": mr.web_url })))
        }
        StepSpec::MergeMr { number, squash, delete_branch, .. } => {
            let number = ctx.mr_number.or(number).ok_or("the MR to merge has not been created")?;
            crate::mr::merge_mr(state, tab, number, None, Some(squash), Some(delete_branch), ctx.mr_source).await?;
            Ok(Some(json!({ "number": number })))
        }
        StepSpec::RunPipeline { branch, workflow_id, variables } => {
            let run = crate::ci::create_ci_pipeline(state, tab, branch, variables, workflow_id).await?;
            Ok(run.map(|r| json!({ "run": r })))
        }
        StepSpec::Tag { name, message, target } => {
            let oid = resolve_commit(&ctx.repo.path, target.as_deref().unwrap_or("HEAD"))?;
            crate::reset::create_tag(state, tab.clone(), name.clone(), oid, message)?;
            crate::remote::push_branch(state, tab, REMOTE.into(), format!("refs/tags/{name}"), false).await?;
            Ok(None)
        }
        StepSpec::Pull {} => {
            let out = crate::remote::pull_branch(state, tab, REMOTE.into(), None).await?;
            if let Some(e) = out.pull_error {
                return Err(e);
            }
            if !out.stash_conflicts.is_empty() {
                return Err(format!("pulled, but re-applying your stash conflicted in {} file(s)", out.stash_conflicts.len()));
            }
            Ok(None)
        }
    }
}

/// The commit `spec` (a branch, tag or oid) names in the repo at `path`. Scoped so the libgit2
/// handle is gone before the caller awaits.
fn resolve_commit(path: &str, spec: &str) -> Result<String, String> {
    let repo = git2::Repository::open(path).map_err(|e| e.to_string())?;
    let obj = repo.revparse_single(spec).map_err(|e| format!("cannot resolve '{spec}': {e}"))?;
    let commit = obj.peel_to_commit().map_err(|e| format!("'{spec}' is not a commit: {e}"))?;
    Ok(commit.id().to_string())
}
