//! `deep_link` domain — the `arbor://…` URL → local-repo lookup, served
//! out-of-process by corvus-be.
//!
//! Ported from the shell's `ipc::platform::deep_link::find_repo_by_remote_url`.
//! The launcher no longer resolves deep-links: it forwards the URL to the FE,
//! whose dispatcher (running in the Corvus window, so corvus-be is up) asks this
//! handler to match the URL against corvus-be's OWN registry + workspaces. The
//! window-open / event side stays shell-side (the launcher owns windows) — see
//! `commands/deep_link_commands.rs`. The `[deep_link]` config stays shell-side too
//! (it's an `AppConfig` slice the launcher owns).
//!
//! [`canonical_key`] is the fuzzy `host/owner/repo` matcher (scheme / credential /
//! `.git` / case-insensitive); the FE mirrors the same algorithm in
//! `utils/git-url.ts`.

use corvus_core::prelude::CorvusState;
use serde::Serialize;

use crate::workspace::{registry, store};

/// Outcome of matching a deep-link URL against local state. Byte-identical to the
/// shell's old `DeepLinkLookup`, so the FE `DeepLinkLookup` type decodes either.
#[derive(Debug, Serialize)]
pub struct DeepLinkLookup {
    pub repo_id:             Option<String>,
    pub repo_path:           Option<String>,
    pub display_name:        Option<String>,
    pub workspace_ids:       Vec<String>,
    pub in_active_workspace: bool,
    pub active_workspace_id: Option<String>,
}

#[arbor_rpc::handler]
fn find_repo_by_remote_url(state: &CorvusState, url: String) -> Result<DeepLinkLookup, String> {
    let key = canonical_key(&url);
    let active_workspace_id = store::store(state).active_workspace_id.clone();

    // Scan the registry for an entry whose remote_url canonicalises to the same
    // key as the incoming URL. Legacy entries with `remote_url = None` ("Open
    // folder…" before auto-probing) get a one-time backfill: probe `origin` from
    // disk so subsequent lookups skip it. Read-scan first (clone out, drop the
    // guard), then persist any backfills in a single `mutate` — so a lookup that
    // changes nothing never rewrites `repos.json`.
    let matched: Option<(String, String, String)> = match key.as_deref() {
        None => None,
        Some(key) => {
            let entries = registry::registry(state).list();
            let mut backfills: Vec<(String, String)> = Vec::new();
            let mut hit: Option<(String, String, String)> = None;
            for entry in entries {
                let url_to_check: Option<String> = match entry.remote_url.as_deref() {
                    Some(u) => Some(u.to_string()),
                    None => {
                        let probed = crate::repo::origin_url(&entry.path);
                        if let Some(ref u) = probed {
                            backfills.push((entry.id.clone(), u.clone()));
                        }
                        probed
                    }
                };
                if let Some(rurl) = url_to_check {
                    if canonical_key(&rurl).as_deref() == Some(key) {
                        hit = Some((entry.id, entry.path, entry.display_name));
                        break;
                    }
                }
            }
            if !backfills.is_empty() {
                let _ = registry::mutate(state, |reg| {
                    for (id, u) in &backfills {
                        let _ = reg.set_remote_url(id, Some(u.clone()));
                    }
                    Ok(())
                });
            }
            hit
        }
    };

    let Some((repo_id, repo_path, display_name)) = matched else {
        return Ok(DeepLinkLookup {
            repo_id:             None,
            repo_path:           None,
            display_name:        None,
            workspace_ids:       Vec::new(),
            in_active_workspace: false,
            active_workspace_id,
        });
    };

    // Every workspace listing this repo, in the user's order (Scratch always last
    // via `WorkspaceStore::ordered`).
    let workspace_ids: Vec<String> = store::store(state)
        .ordered()
        .into_iter()
        .filter(|w| w.repo_ids.iter().any(|id| id == &repo_id))
        .map(|w| w.id)
        .collect();

    let in_active_workspace = active_workspace_id
        .as_ref()
        .map(|aw| workspace_ids.iter().any(|w| w == aw))
        .unwrap_or(false);

    Ok(DeepLinkLookup {
        repo_id: Some(repo_id),
        repo_path: Some(repo_path),
        display_name: Some(display_name),
        workspace_ids,
        in_active_workspace,
        active_workspace_id,
    })
}

/// Reduce any git remote URL to a canonical `host/owner/repo` key for *fuzzy*
/// equality across schemes (https / ssh / scp-style), credentials, `.git` suffix,
/// trailing slashes and case. Verbatim port of the shell's `git::url::canonical_key`
/// (kept in sync with the FE `utils/git-url.ts`). `None` when the URL yields no
/// `(host, path)` pair.
fn canonical_key(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    // Drop scheme, userinfo, port — produce a "host/path" string.
    let host_path: String = if let Some(idx) = s.find("://") {
        let after_scheme = &s[idx + 3..];
        after_scheme
            .find('@')
            .map(|at| &after_scheme[at + 1..])
            .unwrap_or(after_scheme)
            .to_string()
    } else if let Some(at_idx) = s.find('@') {
        // scp-style: user@host:path
        let after_user = &s[at_idx + 1..];
        match after_user.find(':') {
            Some(col) => format!("{}/{}", &after_user[..col], &after_user[col + 1..]),
            None => after_user.to_string(),
        }
    } else {
        s.to_string()
    };

    let (host_with_port, path) = host_path.split_once('/')?;
    let host = host_with_port.split(':').next()?.trim().to_lowercase();
    if host.is_empty() {
        return None;
    }
    let path = path.trim_start_matches('/').trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if path.is_empty() {
        return None;
    }
    Some(format!("{host}/{}", path.to_lowercase()))
}
