//! `gitflow` domain — pure git logic, Tauri-free.
//!
//! Git Flow init/feature/release/hotfix start & finish, driven by the git CLI
//! (branch create/checkout/merge/tag/push) plus a few `git2`-based queries
//! (branch/remote existence, status classification). Lifted verbatim from the
//! shell `crate::git::gitflow`; only the couplings the crate refuses are
//! swapped — the git-program global (`crate::git_cli::command().no_window()`)
//! becomes an explicit [`GitCli`], and `AppError` becomes the crate's
//! [`GitError`]. The `GitFlowConfig` / `GitFlowStatus` / `FlowStartResult` /
//! `FlowFinishResult` DTOs move here and are re-exported by the shell wrapper.
//!
//! Nothing here snapshots: Git Flow finishes guard against an in-progress
//! merge/rebase/cherry-pick up front (`check_no_active_operation`) rather than
//! recording a recovery point, so no snapshot callback is injected.

use std::path::Path;
use git2::{BranchType, Repository};
use serde::{Deserialize, Serialize};

use crate::cli::GitCli;
use crate::error::{GitError, Result};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFlowConfig {
    #[serde(default = "default_main")]
    pub main_branch: String,
    #[serde(default = "default_develop")]
    pub develop_branch: String,
    #[serde(default)]
    pub prefixes: GitFlowPrefixes,
    #[serde(default)]
    pub finish: GitFlowFinishConfig,
    /// When starting a feature or bugfix, require the branch name to be derived
    /// from an issue tracker ticket (e.g. `feature/ABO-123`).
    #[serde(default)]
    pub require_ticket_branch: bool,
}

fn default_main()    -> String { "main".into() }
fn default_develop() -> String { "develop".into() }

impl Default for GitFlowConfig {
    fn default() -> Self {
        Self {
            main_branch:           default_main(),
            develop_branch:        default_develop(),
            prefixes:              GitFlowPrefixes::default(),
            finish:                GitFlowFinishConfig::default(),
            require_ticket_branch: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFlowPrefixes {
    #[serde(default = "prefix_feature")] pub feature: String,
    #[serde(default = "prefix_release")] pub release: String,
    #[serde(default = "prefix_hotfix")]  pub hotfix:  String,
    #[serde(default = "prefix_bugfix")]  pub bugfix:  String,
    #[serde(default = "prefix_support")] pub support: String,
}

fn prefix_feature() -> String { "feature/".into() }
fn prefix_release() -> String { "release/".into() }
fn prefix_hotfix()  -> String { "hotfix/".into() }
fn prefix_bugfix()  -> String { "bugfix/".into() }
fn prefix_support() -> String { "support/".into() }

impl Default for GitFlowPrefixes {
    fn default() -> Self {
        Self {
            feature: prefix_feature(),
            release: prefix_release(),
            hotfix:  prefix_hotfix(),
            bugfix:  prefix_bugfix(),
            support: prefix_support(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFlowFinishConfig {
    #[serde(default = "bool_true")]    pub feature_delete_branch: bool,
    #[serde(default)]                  pub feature_squash:         bool,
    #[serde(default = "bool_true")]    pub release_tag:            bool,
    #[serde(default = "tag_prefix_v")] pub release_tag_prefix:    String,
    #[serde(default = "bool_true")]    pub hotfix_tag:             bool,
    /// Force a Pull Request / Merge Request when finishing a feature (mandatory — no local merge).
    #[serde(default)]
    pub feature_use_pr: bool,
    /// Force a Pull Request / Merge Request when finishing a release (mandatory — no local merge).
    #[serde(default)]
    pub release_use_pr: bool,
    /// Force a Pull Request / Merge Request when finishing a hotfix (mandatory — no local merge).
    #[serde(default)]
    pub hotfix_use_pr: bool,
    /// Default action for the Finish Feature button when PR/MR is not forced.
    /// `false` (default) = merge locally; `true` = open PR/MR.
    #[serde(default)]
    pub feature_pr_default: bool,
    /// Default action for the Finish Release button when PR/MR is not forced.
    #[serde(default)]
    pub release_pr_default: bool,
    /// Default action for the Finish Hotfix button when PR/MR is not forced.
    #[serde(default)]
    pub hotfix_pr_default: bool,
}

fn bool_true()     -> bool   { true }
fn tag_prefix_v()  -> String { "v".into() }

impl Default for GitFlowFinishConfig {
    fn default() -> Self {
        Self {
            feature_delete_branch: true,
            feature_squash:        false,
            release_tag:           true,
            release_tag_prefix:    "v".into(),
            hotfix_tag:            true,
            feature_use_pr:        false,
            release_use_pr:        false,
            hotfix_use_pr:         false,
            feature_pr_default:    false,
            release_pr_default:    false,
            hotfix_pr_default:     false,
        }
    }
}

/// Result of a flow finish operation.
/// When `*_use_pr` is enabled the backend does NOT merge locally — it pushes the
/// branch and returns `CreatePr` so the frontend can open the PR/MR creation form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FlowFinishResult {
    /// Branch was merged locally — no further action required.
    Merged,
    /// Frontend should open the PR/MR creation form pre-filled with these values.
    CreatePr {
        source_branch: String,
        target_branch: String,
    },
}

/// Result of a flow start operation — tells the caller which base branch was
/// used. When the configured base (e.g. develop) is missing, feature/release
/// starts silently fall back to `main`; the frontend uses this to surface a
/// one-time heads-up to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowStartResult {
    pub branch_name: String,
    pub base_branch: String,
    /// True when the configured develop branch was missing and we fell back
    /// to main (or any non-develop base).
    pub fell_back_to_main: bool,
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GitFlowBranchType {
    Main,
    Develop,
    Feature,
    Release,
    Hotfix,
    Bugfix,
    Support,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFlowStatus {
    /// True when both main and develop branches exist.
    pub initialized:         bool,
    pub current_branch:      String,
    pub current_branch_type: GitFlowBranchType,
    /// The name part only (e.g. "my-feature" from "feature/my-feature").
    pub current_flow_name:   Option<String>,
    pub active_features:     Vec<String>,
    pub active_releases:     Vec<String>,
    pub active_hotfixes:     Vec<String>,
    pub develop_exists:      bool,
    pub main_exists:         bool,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn run_git(git: &GitCli, workdir: &Path, args: &[&str]) -> Result<String> {
    let out = git.command()
        .args(args)
        .current_dir(workdir)
        .output()
        .map_err(|e| GitError::Other(format!("failed to spawn git: {e}")))?;

    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let msg = if stderr.is_empty() { stdout } else { stderr };
        Err(GitError::Other(msg))
    }
}

fn repo_workdir(repo: &Repository) -> Result<&Path> {
    repo.workdir()
        .ok_or_else(|| GitError::Other("bare repository not supported".into()))
}

fn branch_exists(repo: &Repository, name: &str) -> bool {
    repo.find_branch(name, BranchType::Local).is_ok()
}

/// Checkout a local branch (uses git CLI to update working tree properly).
fn checkout_branch_git(git: &GitCli, workdir: &Path, name: &str) -> Result<()> {
    run_git(git, workdir, &["checkout", name])?;
    Ok(())
}

/// Create a new branch from `base` and immediately check it out.
fn create_branch_from(git: &GitCli, workdir: &Path, branch_name: &str, base: &str) -> Result<()> {
    run_git(git, workdir, &["checkout", "-b", branch_name, base])?;
    Ok(())
}

/// Merge `from_branch` into `into_branch` (which is checked out first).
fn merge_branch(
    git: &GitCli,
    workdir: &Path,
    from_branch: &str,
    into_branch: &str,
    squash: bool,
    msg: &str,
) -> Result<()> {
    checkout_branch_git(git, workdir, into_branch)?;
    if squash {
        run_git(git, workdir, &["merge", "--squash", from_branch])?;
        run_git(git, workdir, &["commit", "-m", msg])?;
    } else {
        run_git(git, workdir, &["merge", "--no-ff", "-m", msg, from_branch])?;
    }
    Ok(())
}

fn create_tag_git(git: &GitCli, workdir: &Path, tag_name: &str, msg: &str) -> Result<()> {
    run_git(git, workdir, &["tag", "-a", tag_name, "-m", msg])?;
    Ok(())
}

fn push_branch_upstream(git: &GitCli, workdir: &Path, branch: &str) -> Result<()> {
    run_git(git, workdir, &["push", "-u", "origin", branch])?;
    Ok(())
}

fn has_remote(repo: &Repository, name: &str) -> bool {
    repo.find_remote(name).is_ok()
}

/// Abort with a clear error if any multi-step git operation is already active.
/// Running Git Flow finish steps on top of an in-progress merge/rebase/cherry-pick
/// would produce unpredictable partial state that is hard to recover from.
fn check_no_active_operation(repo: &Repository) -> Result<()> {
    let git_dir = repo.path();
    if git_dir.join("MERGE_HEAD").exists() {
        return Err(GitError::Other(
            "A merge is in progress. Complete or abort it (`git merge --abort`) \
             before running Git Flow operations.".into(),
        ));
    }
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        return Err(GitError::Other(
            "A rebase is in progress. Complete or abort it (`git rebase --abort`) \
             before running Git Flow operations.".into(),
        ));
    }
    if git_dir.join("CHERRY_PICK_HEAD").exists() {
        return Err(GitError::Other(
            "A cherry-pick is in progress. Complete or abort it (`git cherry-pick --abort`) \
             before running Git Flow operations.".into(),
        ));
    }
    Ok(())
}

fn classify_branch(
    branch_name: &str,
    config: &GitFlowConfig,
) -> (GitFlowBranchType, Option<String>) {
    if branch_name == config.main_branch    { return (GitFlowBranchType::Main,    None); }
    if branch_name == config.develop_branch { return (GitFlowBranchType::Develop, None); }

    let prefixed = [
        (config.prefixes.feature.as_str(), GitFlowBranchType::Feature),
        (config.prefixes.release.as_str(), GitFlowBranchType::Release),
        (config.prefixes.hotfix.as_str(),  GitFlowBranchType::Hotfix),
        (config.prefixes.bugfix.as_str(),  GitFlowBranchType::Bugfix),
        (config.prefixes.support.as_str(), GitFlowBranchType::Support),
    ];
    for (prefix, kind) in prefixed {
        if let Some(stripped) = branch_name.strip_prefix(prefix) {
            return (kind, Some(stripped.to_string()));
        }
    }
    (GitFlowBranchType::Other, None)
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

pub fn get_gitflow_status(repo: &Repository, config: &GitFlowConfig) -> Result<GitFlowStatus> {
    let head = repo.head().ok();
    let current_branch = head
        .as_ref()
        .and_then(|h| h.shorthand())
        .unwrap_or("HEAD")
        .to_string();

    let (current_branch_type, current_flow_name) = classify_branch(&current_branch, config);

    let develop_exists = branch_exists(repo, &config.develop_branch);
    let main_exists    = branch_exists(repo, &config.main_branch);
    let initialized    = develop_exists && main_exists;

    let mut active_features = Vec::new();
    let mut active_releases = Vec::new();
    let mut active_hotfixes = Vec::new();

    for b in repo.branches(Some(BranchType::Local))? {
        let (b, _) = b?;
        let name = b.name()?.unwrap_or("").to_string();
        if name.starts_with(config.prefixes.feature.as_str()) {
            active_features.push(name[config.prefixes.feature.len()..].to_string());
        } else if name.starts_with(config.prefixes.release.as_str()) {
            active_releases.push(name[config.prefixes.release.len()..].to_string());
        } else if name.starts_with(config.prefixes.hotfix.as_str()) {
            active_hotfixes.push(name[config.prefixes.hotfix.len()..].to_string());
        }
    }

    Ok(GitFlowStatus {
        initialized,
        current_branch,
        current_branch_type,
        current_flow_name,
        active_features,
        active_releases,
        active_hotfixes,
        develop_exists,
        main_exists,
    })
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

/// Ensure that both main and develop branches exist.
/// If develop doesn't exist it is created from main.
pub fn gitflow_init(git: &GitCli, repo: &Repository, config: &GitFlowConfig) -> Result<()> {
    let workdir = repo_workdir(repo)?;

    if !branch_exists(repo, &config.main_branch) {
        return Err(GitError::Other(format!(
            "main branch '{}' not found — cannot initialise Git Flow",
            config.main_branch
        )));
    }

    if !branch_exists(repo, &config.develop_branch) {
        create_branch_from(git, workdir, &config.develop_branch, &config.main_branch)?;
    } else {
        checkout_branch_git(git, workdir, &config.develop_branch)?;
    }

    if has_remote(repo, "origin") {
        push_branch_upstream(git, workdir, &config.main_branch)?;
        push_branch_upstream(git, workdir, &config.develop_branch)?;
    }

    Ok(())
}

/// Create the main branch (which doesn't exist yet) at either the latest commit
/// (HEAD) or the very first commit in history, then run the normal init to
/// create develop from it.
pub fn gitflow_init_create_main(
    git: &GitCli,
    repo: &Repository,
    config: &GitFlowConfig,
    from_initial: bool,
) -> Result<()> {
    let workdir = repo_workdir(repo)?;

    if repo.is_empty().unwrap_or(true) {
        return Err(GitError::Other(
            "repository has no commits yet — make an initial commit first".into(),
        ));
    }

    let target_sha = if from_initial {
        // Walk to the root commit(s); take the first one returned.
        run_git(git, workdir, &["rev-list", "--max-parents=0", "HEAD"])?
            .lines()
            .next()
            .unwrap_or("HEAD")
            .trim()
            .to_string()
    } else {
        run_git(git, workdir, &["rev-parse", "HEAD"])?
    };

    // Create the main branch at the chosen point WITHOUT checking it out.
    run_git(git, workdir, &["branch", &config.main_branch, &target_sha])?;

    // Now run the standard init (creates develop from main).
    gitflow_init(git, repo, config)
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

pub fn feature_start(git: &GitCli, repo: &Repository, config: &GitFlowConfig, name: &str) -> Result<FlowStartResult> {
    let workdir = repo_workdir(repo)?;
    let branch_name = format!("{}{}", config.prefixes.feature, name);

    if branch_exists(repo, &branch_name) {
        return Err(GitError::Other(format!("branch '{branch_name}' already exists")));
    }

    let (base, fell_back) = resolve_feature_base(repo, config)?;
    create_branch_from(git, workdir, &branch_name, &base)?;
    Ok(FlowStartResult { branch_name, base_branch: base, fell_back_to_main: fell_back })
}

/// Pick the base branch for feature/release starts: prefer the configured
/// develop branch; if it doesn't exist, fall back to main (non-standard flow).
/// Errors only when neither branch exists.
fn resolve_feature_base(repo: &Repository, config: &GitFlowConfig) -> Result<(String, bool)> {
    if branch_exists(repo, &config.develop_branch) {
        Ok((config.develop_branch.clone(), false))
    } else if branch_exists(repo, &config.main_branch) {
        Ok((config.main_branch.clone(), true))
    } else {
        Err(GitError::Other(format!(
            "neither develop ('{}') nor main ('{}') branch exists",
            config.develop_branch, config.main_branch
        )))
    }
}

pub fn feature_finish(git: &GitCli, repo: &Repository, config: &GitFlowConfig, name: &str) -> Result<()> {
    let workdir = repo_workdir(repo)?;
    let branch_name = format!("{}{}", config.prefixes.feature, name);

    if !branch_exists(repo, &branch_name) {
        return Err(GitError::Other(format!("branch '{branch_name}' not found")));
    }

    // Pick merge target: develop if it exists, otherwise main (non-standard flow).
    let target = if branch_exists(repo, &config.develop_branch) {
        &config.develop_branch
    } else {
        &config.main_branch
    };

    let msg = format!("Merge branch '{}' into {}", branch_name, target);
    merge_branch(git, workdir, &branch_name, target, config.finish.feature_squash, &msg)?;

    if config.finish.feature_delete_branch {
        // Force-delete: squash merges leave the branch "unmerged" per git's tracking.
        let _ = run_git(git, workdir, &["branch", "-D", &branch_name]);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Release
// ---------------------------------------------------------------------------

pub fn release_start(git: &GitCli, repo: &Repository, config: &GitFlowConfig, version: &str) -> Result<FlowStartResult> {
    let workdir = repo_workdir(repo)?;
    let branch_name = format!("{}{}", config.prefixes.release, version);

    if branch_exists(repo, &branch_name) {
        return Err(GitError::Other(format!("branch '{branch_name}' already exists")));
    }

    let (base, fell_back) = resolve_feature_base(repo, config)?;
    create_branch_from(git, workdir, &branch_name, &base)?;
    Ok(FlowStartResult { branch_name, base_branch: base, fell_back_to_main: fell_back })
}

pub fn release_finish(
    git: &GitCli,
    repo: &Repository,
    config: &GitFlowConfig,
    version: &str,
    tag_message: &str,
) -> Result<()> {
    let workdir = repo_workdir(repo)?;
    let branch_name = format!("{}{}", config.prefixes.release, version);

    // Pre-flight: refuse to start if another operation is already in progress.
    // If we started merging and then discovered an active rebase, recovery would
    // be very confusing.
    check_no_active_operation(repo)?;

    if !branch_exists(repo, &branch_name) {
        return Err(GitError::Other(format!("branch '{branch_name}' not found")));
    }

    // 1. Merge into main
    let main = &config.main_branch;
    let develop = &config.develop_branch;
    let main_msg = format!("Merge branch '{branch_name}' into {main}");
    merge_branch(git, workdir, &branch_name, main, false, &main_msg)
        .map_err(|e| GitError::Other(format!(
            "Release finish failed at step 1/3 (merge '{branch_name}' into '{main}'): {e}",
        )))?;

    // 2. Create annotated tag on main
    if config.finish.release_tag {
        let tag_name = format!("{}{}", config.finish.release_tag_prefix, version);
        let tag_msg = if tag_message.is_empty() {
            format!("Release {version}")
        } else {
            tag_message.to_string()
        };
        create_tag_git(git, workdir, &tag_name, &tag_msg).map_err(|e| GitError::Other(format!(
            "Release finish: merged '{branch_name}' into '{main}' (step 1/3 OK) but failed \
             to create tag '{tag_name}' (step 2/3): {e}. Complete manually: \
             git tag -a {tag_name} -m '...' \
             && git checkout {develop} && git merge {branch_name} \
             && git branch -D {branch_name}",
        )))?;
    }

    // 3. Merge into develop
    if branch_exists(repo, develop) {
        let dev_msg = format!("Merge branch '{branch_name}' into {develop}");
        merge_branch(git, workdir, &branch_name, develop, false, &dev_msg)
            .map_err(|e| GitError::Other(format!(
                "Release finish: steps 1–2 succeeded (merged into '{main}', tag created) \
                 but merge into '{develop}' failed (step 3/3): {e}. Resolve conflicts, then: \
                 git checkout {develop} && git merge {branch_name} \
                 && git branch -D {branch_name}",
            )))?;
    }

    // 4. Delete release branch
    let _ = run_git(git, workdir, &["branch", "-D", &branch_name]);
    Ok(())
}

// ---------------------------------------------------------------------------
// Hotfix
// ---------------------------------------------------------------------------

pub fn hotfix_start(git: &GitCli, repo: &Repository, config: &GitFlowConfig, name: &str) -> Result<FlowStartResult> {
    let workdir = repo_workdir(repo)?;
    let branch_name = format!("{}{}", config.prefixes.hotfix, name);

    if branch_exists(repo, &branch_name) {
        return Err(GitError::Other(format!("branch '{branch_name}' already exists")));
    }
    if !branch_exists(repo, &config.main_branch) {
        return Err(GitError::Other(format!(
            "main branch '{}' not found",
            config.main_branch
        )));
    }
    create_branch_from(git, workdir, &branch_name, &config.main_branch)?;
    Ok(FlowStartResult {
        branch_name,
        base_branch: config.main_branch.clone(),
        fell_back_to_main: false,
    })
}

// ---------------------------------------------------------------------------
// PR / MR finish variants
// ---------------------------------------------------------------------------

/// Finish a feature branch, respecting `config.finish.feature_use_pr`.
/// When `force_pr` or the config flag is set, the branch is pushed to `origin`
/// and a `CreatePr` result is returned so the frontend can open the PR/MR form.
pub fn feature_finish_or_pr(
    git: &GitCli,
    repo: &Repository,
    config: &GitFlowConfig,
    name: &str,
    force_pr: bool,
) -> Result<FlowFinishResult> {
    let branch_name = format!("{}{}", config.prefixes.feature, name);
    if config.finish.feature_use_pr || force_pr {
        if !branch_exists(repo, &branch_name) {
            return Err(GitError::Other(format!("branch '{branch_name}' not found")));
        }
        if has_remote(repo, "origin") {
            let workdir = repo_workdir(repo)?;
            push_branch_upstream(git, workdir, &branch_name)?;
        }
        // PR target: develop if present, else main (non-standard flow).
        let target = if branch_exists(repo, &config.develop_branch) {
            config.develop_branch.clone()
        } else {
            config.main_branch.clone()
        };
        return Ok(FlowFinishResult::CreatePr {
            source_branch: branch_name,
            target_branch: target,
        });
    }
    feature_finish(git, repo, config, name)?;
    Ok(FlowFinishResult::Merged)
}

/// Finish a release branch, respecting `config.finish.release_use_pr`.
pub fn release_finish_or_pr(
    git: &GitCli,
    repo: &Repository,
    config: &GitFlowConfig,
    version: &str,
    tag_message: &str,
    force_pr: bool,
) -> Result<FlowFinishResult> {
    let branch_name = format!("{}{}", config.prefixes.release, version);
    if config.finish.release_use_pr || force_pr {
        if !branch_exists(repo, &branch_name) {
            return Err(GitError::Other(format!("branch '{branch_name}' not found")));
        }
        if has_remote(repo, "origin") {
            let workdir = repo_workdir(repo)?;
            push_branch_upstream(git, workdir, &branch_name)?;
        }
        return Ok(FlowFinishResult::CreatePr {
            source_branch: branch_name,
            target_branch: config.main_branch.clone(),
        });
    }
    release_finish(git, repo, config, version, tag_message)?;
    Ok(FlowFinishResult::Merged)
}

/// Finish a hotfix branch, respecting `config.finish.hotfix_use_pr`.
pub fn hotfix_finish_or_pr(
    git: &GitCli,
    repo: &Repository,
    config: &GitFlowConfig,
    name: &str,
    tag_message: &str,
    force_pr: bool,
) -> Result<FlowFinishResult> {
    let branch_name = format!("{}{}", config.prefixes.hotfix, name);
    if config.finish.hotfix_use_pr || force_pr {
        if !branch_exists(repo, &branch_name) {
            return Err(GitError::Other(format!("branch '{branch_name}' not found")));
        }
        if has_remote(repo, "origin") {
            let workdir = repo_workdir(repo)?;
            push_branch_upstream(git, workdir, &branch_name)?;
        }
        return Ok(FlowFinishResult::CreatePr {
            source_branch: branch_name,
            target_branch: config.main_branch.clone(),
        });
    }
    hotfix_finish(git, repo, config, name, tag_message)?;
    Ok(FlowFinishResult::Merged)
}

pub fn hotfix_finish(
    git: &GitCli,
    repo: &Repository,
    config: &GitFlowConfig,
    name: &str,
    tag_message: &str,
) -> Result<()> {
    let workdir = repo_workdir(repo)?;
    let branch_name = format!("{}{}", config.prefixes.hotfix, name);

    // Pre-flight: refuse to start if another operation is already in progress.
    check_no_active_operation(repo)?;

    if !branch_exists(repo, &branch_name) {
        return Err(GitError::Other(format!("branch '{branch_name}' not found")));
    }

    let main    = &config.main_branch;
    let develop = &config.develop_branch;

    // 1. Merge into main
    let main_msg = format!("Merge branch '{branch_name}' into {main}");
    merge_branch(git, workdir, &branch_name, main, false, &main_msg)
        .map_err(|e| GitError::Other(format!(
            "Hotfix finish failed at step 1/3 (merge '{branch_name}' into '{main}'): {e}",
        )))?;

    // 2. Create annotated tag on main
    if config.finish.hotfix_tag {
        let tag_name = format!("{}{}", config.finish.release_tag_prefix, name);
        let tag_msg = if tag_message.is_empty() {
            format!("Hotfix {name}")
        } else {
            tag_message.to_string()
        };
        create_tag_git(git, workdir, &tag_name, &tag_msg).map_err(|e| GitError::Other(format!(
            "Hotfix finish: merged '{branch_name}' into '{main}' (step 1/3 OK) but failed \
             to create tag '{tag_name}' (step 2/3): {e}. Complete manually: \
             git tag -a {tag_name} -m '...' \
             && git checkout {develop} && git merge {branch_name} \
             && git branch -D {branch_name}",
        )))?;
    }

    // 3. Merge into develop (if present)
    if branch_exists(repo, develop) {
        let dev_msg = format!("Merge branch '{branch_name}' into {develop}");
        merge_branch(git, workdir, &branch_name, develop, false, &dev_msg)
            .map_err(|e| GitError::Other(format!(
                "Hotfix finish: steps 1–2 succeeded (merged into '{main}', tag created) \
                 but merge into '{develop}' failed (step 3/3): {e}. Resolve conflicts, then: \
                 git checkout {develop} && git merge {branch_name} \
                 && git branch -D {branch_name}",
            )))?;
    }

    // 4. Delete hotfix branch
    let _ = run_git(git, workdir, &["branch", "-D", &branch_name]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Pure DTO + classification logic (no git repo needed) ─────────────────

    #[test]
    fn default_config_uses_standard_branch_names_and_prefixes() {
        let cfg = GitFlowConfig::default();
        assert_eq!(cfg.main_branch, "main");
        assert_eq!(cfg.develop_branch, "develop");
        assert_eq!(cfg.prefixes.feature, "feature/");
        assert_eq!(cfg.prefixes.release, "release/");
        assert_eq!(cfg.prefixes.hotfix, "hotfix/");
        assert_eq!(cfg.prefixes.bugfix, "bugfix/");
        assert_eq!(cfg.prefixes.support, "support/");
        assert!(!cfg.require_ticket_branch);
        // Finish defaults: delete branch + tag releases/hotfixes on, no PR forcing.
        assert!(cfg.finish.feature_delete_branch);
        assert!(cfg.finish.release_tag);
        assert!(cfg.finish.hotfix_tag);
        assert_eq!(cfg.finish.release_tag_prefix, "v");
        assert!(!cfg.finish.feature_squash);
        assert!(!cfg.finish.feature_use_pr);
    }

    #[test]
    fn classify_branch_recognises_main_develop_and_prefixed() {
        let cfg = GitFlowConfig::default();

        assert_eq!(classify_branch("main", &cfg), (GitFlowBranchType::Main, None));
        assert_eq!(classify_branch("develop", &cfg), (GitFlowBranchType::Develop, None));
        assert_eq!(
            classify_branch("feature/my-feature", &cfg),
            (GitFlowBranchType::Feature, Some("my-feature".to_string())),
        );
        assert_eq!(
            classify_branch("release/1.2.0", &cfg),
            (GitFlowBranchType::Release, Some("1.2.0".to_string())),
        );
        assert_eq!(
            classify_branch("hotfix/urgent", &cfg),
            (GitFlowBranchType::Hotfix, Some("urgent".to_string())),
        );
        assert_eq!(
            classify_branch("bugfix/typo", &cfg),
            (GitFlowBranchType::Bugfix, Some("typo".to_string())),
        );
        assert_eq!(
            classify_branch("support/legacy", &cfg),
            (GitFlowBranchType::Support, Some("legacy".to_string())),
        );
    }

    #[test]
    fn classify_branch_unknown_is_other() {
        let cfg = GitFlowConfig::default();
        // A name that matches no configured branch / prefix is "Other".
        assert_eq!(classify_branch("random-topic", &cfg), (GitFlowBranchType::Other, None));
        // Edge: the prefix string itself with an empty remainder still classifies
        // as Feature with an empty flow name (starts_with("feature/") is true).
        assert_eq!(
            classify_branch("feature/", &cfg),
            (GitFlowBranchType::Feature, Some(String::new())),
        );
    }

    #[test]
    fn classify_branch_honours_custom_branch_names() {
        // Edge: non-default main/develop names must classify correctly and not
        // be mistaken for "Other".
        let cfg = GitFlowConfig {
            main_branch: "master".into(),
            develop_branch: "dev".into(),
            ..Default::default()
        };
        assert_eq!(classify_branch("master", &cfg), (GitFlowBranchType::Main, None));
        assert_eq!(classify_branch("dev", &cfg), (GitFlowBranchType::Develop, None));
        // "main" is no longer special once main_branch is "master".
        assert_eq!(classify_branch("main", &cfg), (GitFlowBranchType::Other, None));
    }

    #[test]
    fn flow_finish_result_serde_tags_snake_case() {
        // The frontend matches on the `action` discriminator — keep it stable.
        assert_eq!(
            serde_json::to_string(&FlowFinishResult::Merged).unwrap(),
            r#"{"action":"merged"}"#,
        );
        assert_eq!(
            serde_json::to_string(&FlowFinishResult::CreatePr {
                source_branch: "feature/x".into(),
                target_branch: "develop".into(),
            })
            .unwrap(),
            r#"{"action":"create_pr","source_branch":"feature/x","target_branch":"develop"}"#,
        );
    }

    #[test]
    fn gitflow_branch_type_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&GitFlowBranchType::Develop).unwrap(),
            "\"develop\"",
        );
        assert_eq!(
            serde_json::to_string(&GitFlowBranchType::Other).unwrap(),
            "\"other\"",
        );
    }
}
