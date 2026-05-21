use git2::{FetchOptions, FetchPrune, PushOptions, RemoteCallbacks, Repository};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::process_ext::NoWindowExt;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    pub remote: String,
    pub received_objects: usize,
    pub indexed_objects: usize,
    pub received_bytes: usize,
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

pub fn list_remotes(repo: &Repository) -> Result<Vec<RemoteInfo>> {
    let names = repo.remotes()?;
    let mut out = Vec::new();
    for name in names.iter().flatten() {
        let remote = repo.find_remote(name)?;
        out.push(RemoteInfo {
            name: name.to_string(),
            url: remote.url().unwrap_or("").to_string(),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Credential resolution
//
// Returns a `move` closure suitable for `RemoteCallbacks::credentials`.
//
// Priority:
//   1. Stored credential (OS keychain via save_for_host) — tried once.
//   2. System git credential helper (`Cred::default`) — fallback when no
//      stored cred is found (e.g. credential.helper=manager on Windows).
//   3. SSH agent — for SSH remotes.
//
// The `tried_*` flags ensure that a wrong-credentials 401 from the server
// does not cause libgit2 to retry infinitely (it calls the callback again
// after each failed attempt).
// ---------------------------------------------------------------------------

fn make_credentials_cb(
    stored: Option<(String, String)>,
) -> impl FnMut(&str, Option<&str>, git2::CredentialType)
       -> std::result::Result<git2::Cred, git2::Error>
{
    let mut tried_userpass = false;
    let mut tried_ssh      = false;
    let mut tried_default  = false;

    move |url, username_from_url, allowed| {
        // ── HTTPS / token auth ──────────────────────────────────────────
        // IMPORTANT: do NOT fall back to Cred::default() for HTTPS — that
        // flag means NTLM/Kerberos, which GitHub/GitLab never accept.
        // Doing so produces a silent "no credentials" failure deep in libgit2
        // instead of a clear actionable message.
        if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            if tried_userpass {
                return Err(git2::Error::from_str(
                    "HTTP auth failed — server rejected the token. Verify scopes/expiry in Settings → Credentials.",
                ));
            }
            tried_userpass = true;
            return match &stored {
                Some((user, pass)) => git2::Cred::userpass_plaintext(user, pass),
                None => Err(git2::Error::from_str(&format!(
                    "No token stored for '{url}'. Add a Personal Access Token in Settings → Credentials.",
                ))),
            };
        }

        // ── SSH agent ───────────────────────────────────────────────────
        if allowed.contains(git2::CredentialType::SSH_KEY) {
            if tried_ssh {
                return Err(git2::Error::from_str(
                    "SSH auth failed — check that ssh-agent is running with a key for this host.",
                ));
            }
            tried_ssh = true;
            return git2::Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"));
        }

        // ── NTLM / Kerberos (corporate on-prem Git servers) ────────────
        if allowed.contains(git2::CredentialType::DEFAULT) {
            if tried_default {
                return Err(git2::Error::from_str("NTLM/Kerberos auth failed."));
            }
            tried_default = true;
            return git2::Cred::default();
        }

        Err(git2::Error::from_str(&format!(
            "Unsupported auth type for '{url}' (flags: {allowed:?}). For HTTPS add a token in Settings → Credentials.",
        )))
    }
}

// ---------------------------------------------------------------------------
// Fetch (synchronous — async wrapper lives in commands)
// ---------------------------------------------------------------------------

pub fn fetch(repo: &Repository, remote_name: &str) -> Result<FetchResult> {
    let mut remote = repo.find_remote(remote_name)?;
    let url = remote.url().unwrap_or("").to_string();
    let stored = match crate::auth::credential_store::resolve_credentials(&url) {
        Ok(creds) => creds,
        Err(e) => {
            tracing::warn!("keyring lookup failed for '{url}': {e}");
            None
        }
    };

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(make_credentials_cb(stored));

    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);
    fetch_opts.download_tags(git2::AutotagOption::Auto);
    fetch_opts.prune(FetchPrune::On);

    remote.fetch(&[] as &[&str], Some(&mut fetch_opts), None)?;

    let stats = remote.stats();
    Ok(FetchResult {
        remote: remote_name.to_string(),
        received_objects: stats.received_objects(),
        indexed_objects: stats.indexed_objects(),
        received_bytes: stats.received_bytes(),
    })
}

// ---------------------------------------------------------------------------
// Push
// ---------------------------------------------------------------------------

pub fn push(repo: &Repository, remote_name: &str, refspec: &str, force: bool) -> Result<()> {
    let mut remote = repo.find_remote(remote_name)?;
    let url = remote.url().unwrap_or("").to_string();
    let stored = match crate::auth::credential_store::resolve_credentials(&url) {
        Ok(creds) => creds,
        Err(e) => {
            tracing::warn!("keyring lookup failed for '{url}': {e}");
            None
        }
    };

    let spec = if force { format!("+{refspec}") } else { refspec.to_string() };

    // Capture server-side per-ref rejections. libgit2's `remote.push()` returns
    // Ok even when the server rejects the push for an individual ref (e.g.
    // protected branch, refusing to delete the default branch). The only way
    // to surface those is the push_update_reference callback: it fires once
    // per pushed ref with status=Some(msg) when the server rejected it.
    let rejection: std::sync::Arc<std::sync::Mutex<Option<(String, String)>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let rejection_cb = rejection.clone();

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(make_credentials_cb(stored));
    callbacks.push_update_reference(move |refname, status| {
        if let Some(msg) = status {
            if let Ok(mut slot) = rejection_cb.lock() {
                if slot.is_none() {
                    *slot = Some((refname.to_string(), msg.to_string()));
                }
            }
        }
        Ok(())
    });

    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);

    remote.push(&[spec.as_str()], Some(&mut push_opts))?;

    let rejection_taken = rejection.lock().ok().and_then(|mut g| g.take());
    if let Some((refname, msg)) = rejection_taken {
        return Err(crate::error::AppError::Other(format!(
            "remote rejected push of '{refname}': {msg}"
        )));
    }

    // libgit2 does not always update refs/remotes/* after a push, so do it manually.
    // This ensures the sidebar immediately reflects the push (removes the "local" badge).
    let src = refspec.trim_start_matches('+');
    if let Some(branch_name) = src.strip_prefix("refs/heads/") {
        if let Ok(local_ref) = repo.find_reference(src) {
            if let Some(oid) = local_ref.target() {
                let tracking = format!("refs/remotes/{remote_name}/{branch_name}");
                let _ = repo.reference(&tracking, oid, true, "push: update remote-tracking ref");
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Pull (fetch + fast-forward or merge)
// ---------------------------------------------------------------------------

pub fn pull(repo: &Repository, remote_name: &str) -> Result<()> {
    fetch(repo, remote_name)?;

    let head = repo.head()?;
    let branch_name = head
        .shorthand()
        .ok_or_else(|| crate::error::AppError::Other("detached HEAD".into()))?;

    // Prefer the configured upstream tracking ref (branch.<name>.merge + remote).
    // Fall back to the conventional refs/remotes/<remote>/<branch> pattern.
    let fetch_head_id = {
        let from_config = repo
            .find_branch(branch_name, git2::BranchType::Local)
            .ok()
            .and_then(|b| b.upstream().ok())
            .and_then(|u| u.get().target());

        match from_config {
            Some(id) => id,
            None => repo
                .refname_to_id(&format!("refs/remotes/{remote_name}/{branch_name}"))
                .map_err(|_| crate::error::AppError::Other(format!(
                    "No upstream tracking branch found for '{branch_name}'. \
                     Push the branch first or set tracking with: \
                     git branch --set-upstream-to={remote_name}/{branch_name}"
                )))?,
        }
    };

    let fetch_commit = repo.find_annotated_commit(fetch_head_id)?;

    let (merge_analysis, _) = repo.merge_analysis(&[&fetch_commit])?;

    if merge_analysis.is_up_to_date() {
        return Ok(());
    }

    if merge_analysis.is_fast_forward() {
        // Delegate the fast-forward to the git CLI (same pattern as the
        // normal-merge branch below). libgit2's `checkout_tree(SAFE)` has
        // a rougher edge-case handling of untracked/ignored-yet-colliding
        // files than the CLI does — spent a full debugging session
        // chasing false "1 conflict prevents checkout" errors where git
        // CLI would happily fast-forward. The CLI also produces readable
        // error messages ("untracked working tree files would be
        // overwritten") when a real conflict exists, which we propagate
        // unchanged up the PullResult chain.
        //
        // We still avoid `--force` semantics: `--ff-only` refuses the
        // operation when the merge isn't purely a fast-forward, giving us
        // the same safety net against silent data loss that drove the
        // removal of libgit2's `.force()` checkout.
        let workdir = repo.workdir().ok_or_else(|| {
            crate::error::AppError::Other("bare repository has no working directory".into())
        })?;

        let out = crate::git_cli::command()
            .args(["merge", "--ff-only", &fetch_head_id.to_string()])
            .current_dir(workdir)
            .no_window()
            .output()
            .map_err(crate::error::AppError::Io)?;

        if out.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let msg = if !stderr.trim().is_empty() { stderr.to_string() } else { stdout.to_string() };
        return Err(crate::error::AppError::Other(format!(
            "fast-forward failed: {msg}"
        )));
    }

    // Non-fast-forward: delegate to the git CLI for the merge so git's own
    // three-way merge strategy, conflict handling, and MERGE_MSG/MERGE_HEAD
    // state files are all handled correctly.
    // --no-edit: keep the auto-generated merge commit message without opening
    // an interactive editor (which would block a GUI subprocess).
    if merge_analysis.is_normal() {
        let workdir = repo.workdir().ok_or_else(|| {
            crate::error::AppError::Other("bare repository has no working directory".into())
        })?;

        let out = crate::git_cli::command()
            .args(["merge", "--no-edit", &fetch_head_id.to_string()])
            .current_dir(workdir)
            .no_window()
            .output()
            .map_err(crate::error::AppError::Io)?;

        if out.status.success() {
            return Ok(());
        }

        // Non-zero exit can mean either merge conflicts or a hard error.
        // Conflicts leave MERGE_HEAD in the git dir — use that to distinguish.
        if repo.path().join("MERGE_HEAD").exists() {
            return Err(crate::error::AppError::Other(
                "Pull produced merge conflicts. Resolve conflicts in the working tree, \
                 then commit to complete the merge."
                    .into(),
            ));
        }

        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let msg = if !stderr.trim().is_empty() {
            stderr.to_string()
        } else {
            stdout.to_string()
        };
        return Err(crate::error::AppError::Other(format!("git merge failed: {msg}")));
    }

    Err(crate::error::AppError::Other(
        "Cannot determine merge strategy for this branch state.".into(),
    ))
}
