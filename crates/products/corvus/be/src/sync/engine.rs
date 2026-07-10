//! Push / fetch the bundle through the provider's raw file API.
//!
//! Push writes each bundle file (one commit per file — the bundle is a handful
//! of small files, and only when the fingerprint changed). Fetch reads back the
//! requested paths, skipping any the remote doesn't have yet (first-sync-safe).

use super::remote::SyncRemote;
use super::BundleFile;

const COMMIT_MSG: &str = "arbor: sync corvus settings";

/// Write the whole bundle to the remote in a **single commit**. Returns `true`
/// when a commit was made, `false` when nothing changed (no empty commit).
pub(crate) async fn push(remote: &SyncRemote, files: &[BundleFile]) -> Result<bool, String> {
    let provider = crate::provider::for_host(&remote.provider_key)?;
    let payload: Vec<(String, Vec<u8>)> =
        files.iter().map(|f| (f.path.clone(), f.bytes.clone())).collect();
    provider
        .put_repo_files(&remote.repo_ref, &remote.branch, &payload, COMMIT_MSG)
        .await
        .map_err(crate::provider::pe)
}

/// Fetch the given paths from the remote. Missing files are silently skipped, so
/// a partially-populated (or brand-new) sync repo reads cleanly.
pub(crate) async fn fetch(remote: &SyncRemote, paths: &[String]) -> Result<Vec<BundleFile>, String> {
    let provider = crate::provider::for_host(&remote.provider_key)?;
    let mut out = Vec::new();
    for p in paths {
        if let Some(bytes) = provider
            .get_repo_file(&remote.repo_ref, p, &remote.branch)
            .await
            .map_err(crate::provider::pe)?
        {
            out.push(BundleFile { path: p.clone(), bytes });
        }
    }
    Ok(out)
}
