//! Push / fetch the bundle through the provider's raw file API.
//!
//! Push writes each bundle file (one commit per file — the bundle is a handful
//! of small files, and only when the fingerprint changed). Fetch reads back the
//! requested paths, skipping any the remote doesn't have yet (first-sync-safe).

use super::remote::SyncRemote;
use super::BundleFile;

const COMMIT_MSG: &str = "arbor: sync corvus settings";

/// Write every bundle file to the remote on its branch.
pub(crate) async fn push(remote: &SyncRemote, files: &[BundleFile]) -> Result<(), String> {
    let provider = crate::provider::for_host(&remote.provider_key)?;
    for f in files {
        provider
            .put_repo_file(&remote.repo_ref, &f.path, &remote.branch, &f.bytes, COMMIT_MSG)
            .await
            .map_err(crate::provider::pe)?;
    }
    Ok(())
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
