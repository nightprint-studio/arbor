//! Installer for marketplace plugins + theme writer for themes.
//!
//! ## Two channels, and the entry decides
//!
//! A package that carries only Lua installs from the **source archive** of a git ref, which
//! is what every package does today and what the rest of this comment describes. A package
//! that carries a **build artifact** — a `.wasm` implementing a host interface — installs
//! from the **release** for its tag instead, and every file is checked against the digest
//! the registry recorded for it.
//!
//! The split is not fussiness. GitHub generates `archive/{ref}.zip` on demand and its bytes
//! have changed across GitHub's own tooling upgrades for untouched repositories, so a digest
//! pinned to one would turn a working install into a mysterious failure on somebody else's
//! schedule. A source archive's integrity is `pinned_sha` — git is content-addressed, so the
//! commit pins the source. A release asset is a file an author uploaded and GitHub stores
//! verbatim, so it can be hashed, and it has to be: nobody can read a `.wasm`.
//!
//! Plugins (source-archive channel):
//!   * download `https://github.com/{owner}/{repo}/archive/{ref}.zip`
//!   * the archive is rooted at `{repo}-{ref}/`; the actual plugin files
//!     live under `{archive_root}/{subpath}/`.
//!   * we strip both prefixes and dump the contents into
//!     [`crate::paths::plugins_dir`]`/{name}/`.
//!   * Refuses to overwrite a non-empty dev plugin folder with the same
//!     name (collision policy: dev wins; see [`crate::host::MarketplaceHost`]).
//!
//! Themes:
//!   * the entry's `subpath` points at a single JSON file in the registry
//!     repo. We fetch it raw and drop it into [`crate::paths::themes_dir`]
//!     `/{id}.json` (the same dir the host's theme loader scans).

use std::io::Cursor;
use std::path::Path;

use crate::error::{MarketplaceError, Result};
use crate::github_api::{
    archive_url, client, parse_github_repo, raw_url, release_asset_url, resolve_ref_sha,
};
use crate::integrity;
use crate::host::MarketplaceHost;
use crate::index::REGISTRY_REF;
use crate::installs::{self, InstalledPlugin, InstalledTheme};
use crate::paths;
use crate::types::{MarketplacePlugin, MarketplaceTheme};

// ---------------------------------------------------------------------------
// Plugin install
// ---------------------------------------------------------------------------

pub async fn install_plugin(
    host:   &dyn MarketplaceHost,
    plugin: &MarketplacePlugin,
) -> Result<InstalledPlugin> {
    let (owner, repo) = parse_github_repo(&plugin.entry.repo)
        .ok_or_else(|| MarketplaceError::InvalidUrl(plugin.entry.repo.clone()))?;
    let r#ref = plugin.entry.r#ref.clone().unwrap_or_else(|| REGISTRY_REF.to_string());
    let subpath = plugin.entry.subpath.clone().unwrap_or_default();

    // Collision guard: refuse to install on top of a dev plugin folder.
    let dev_target = host.dev_plugin_dir().join(&plugin.name);
    if dev_target.exists() {
        return Err(MarketplaceError::InstallCollision(format!(
            "a dev/local plugin named '{}' already exists at {dev_target:?}; \
             marketplace install would be shadowed by it. Remove or rename \
             the local folder first.",
            plugin.name
        )));
    }

    let http = client()?;
    let target = paths::plugins_dir().join(&plugin.name);

    // Each channel fetches and verifies EVERYTHING before it touches `target` — see
    // `reset_dir`. Until then the version already on disk is the one that keeps working.
    if plugin.entry.artifacts.is_empty() {
        install_from_source_archive(&http, plugin, &owner, &repo, &r#ref, &subpath, &target)
            .await?;
    } else {
        install_from_release(&http, plugin, &owner, &repo, &r#ref, &target).await?;
    }

    // Resolve SHA — best-effort fingerprint, not load-bearing.
    let resolved_sha = resolve_ref_sha(&http, &owner, &repo, &r#ref).await.ok();

    Ok(InstalledPlugin {
        name:         plugin.name.clone(),
        version:      plugin.version.clone(),
        entry:        plugin.entry.clone(),
        resolved_sha,
        install_path: target.to_string_lossy().to_string(),
        installed_at: installs::now_secs(),
        // Convention: marketplace installs land disabled. The user opts in
        // once they've reviewed the plugin from the detail pane.
        enabled:      false,
    })
}

/// Empty the install directory, immediately before writing the new version into it.
///
/// Called only once a channel holds everything it is going to write. The ordering is the
/// whole point: a download that fails, or an artifact that fails its integrity check, must
/// leave the installed version exactly as it was. Wiping first would turn "the network was
/// down" into "the plugin is gone".
fn reset_dir(target: &Path) -> Result<()> {
    if target.exists() {
        std::fs::remove_dir_all(target)?;
    }
    std::fs::create_dir_all(target)?;
    Ok(())
}

/// Download the repo's source archive for a ref and extract the package's subpath out of it.
///
/// The channel every package uses today. Unchanged behaviour: GitHub roots
/// `archive/{ref}.zip` at `{repo}-{ref}/`, but branch names containing `/` flatten to `-`
/// and `v` prefixes are kept verbatim, so the root is discovered at read time rather than
/// reconstructed.
async fn install_from_source_archive(
    http:    &reqwest::Client,
    plugin:  &MarketplacePlugin,
    owner:   &str,
    repo:    &str,
    r#ref:   &str,
    subpath: &str,
    target:  &Path,
) -> Result<()> {
    let zip_url = archive_url(owner, repo, r#ref);
    tracing::info!("marketplace: downloading {zip_url}");
    let bytes = fetch_bytes(http, &zip_url).await?;
    reset_dir(target)?;
    let written = extract_subpath_into(&bytes, subpath, target, &plugin.name)?;
    tracing::info!(
        "marketplace: extracted {written} files to {target:?} ({} bytes archive)",
        bytes.len()
    );
    Ok(())
}

/// Download the release assets the registry approved, verify each, and lay them out.
///
/// Every asset named in the entry is fetched and hashed **before anything is written**. An
/// artifact that fails the check aborts the whole install rather than the one file: a package
/// is only meaningful whole, and a directory holding three of its four approved files is a
/// worse outcome than no directory at all.
///
/// A `.zip` asset carries the package's readable half — `plugin.toml`, the Lua, the docs —
/// and is extracted. Everything else is written beside it under its own name, which is how a
/// `[[provides]]` entry's `module` finds its file.
async fn install_from_release(
    http:   &reqwest::Client,
    plugin: &MarketplacePlugin,
    owner:  &str,
    repo:   &str,
    tag:    &str,
    target: &Path,
) -> Result<()> {
    let mut verified: Vec<(String, Vec<u8>)> = Vec::new();
    for (asset, digest) in &plugin.entry.artifacts {
        if asset.contains('/') || asset.contains("..") {
            return Err(MarketplaceError::InvalidEntry(format!(
                "'{}': asset name '{asset}' is a path, not a file name", plugin.name
            )));
        }
        let url = release_asset_url(owner, repo, tag, asset);
        tracing::info!("marketplace: downloading {url}");
        let body = fetch_bytes(http, &url).await?;
        integrity::verify(asset, &body, digest)?;
        verified.push((asset.clone(), body));
    }

    reset_dir(target)?;
    let mut zips = 0usize;
    for (asset, body) in &verified {
        if asset.ends_with(".zip") {
            zips += 1;
            let written = extract_package_zip(body, target, &plugin.name)?;
            tracing::info!("marketplace: extracted {written} files from {asset}");
        } else {
            std::fs::write(target.join(asset), body)?;
        }
    }

    // A package installed from a release still has to look like a package. Without the zip
    // there is no `plugin.toml`, and the host would discover a directory of modules it has
    // no manifest for — which reads as a corrupt install rather than as a missing asset.
    if zips == 0 || !target.join("plugin.toml").exists() {
        return Err(MarketplaceError::InvalidEntry(format!(
            "'{}': the release for {tag} carries no .zip with a plugin.toml. A package's \
             readable half ships as a zip asset beside its modules.",
            plugin.name
        )));
    }
    Ok(())
}

/// GET a URL and return its body, with the URL in every error.
///
/// `Vec<u8>` rather than `bytes::Bytes`: the caller hashes and writes it once, so the
/// cheap-clone the latter buys is worth less than not taking a dependency for a type that
/// only ever appears inside this file.
async fn fetch_bytes(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let body = http
        .get(url)
        .send()
        .await
        .map_err(|e| MarketplaceError::Other(format!("GET {url}: {e}")))?
        .error_for_status()
        .map_err(|e| MarketplaceError::Other(format!("HTTP {url}: {e}")))?
        .bytes()
        .await
        .map_err(|e| MarketplaceError::Other(format!("body {url}: {e}")))?;
    Ok(body.to_vec())
}

/// Extract an author-made package zip into `target`.
///
/// Unlike a GitHub source archive this one is the author's own, so it may be rooted either at
/// the package files directly or inside a single wrapping folder. Both are accepted — the
/// same latitude the sideload-a-zip path already gives — because which one you get depends on
/// how somebody's release job invoked `zip`, and that is not worth a failed install.
fn extract_package_zip(bytes: &[u8], target: &Path, label: &str) -> Result<usize> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| MarketplaceError::InvalidArchive(format!("'{label}': {e}")))?;

    // A wrapping folder only counts as one if every entry shares it AND the manifest is not
    // already at the root — otherwise a package whose only content happens to sit in one
    // directory would have that directory stripped.
    let mut names = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let f = archive
            .by_index(i)
            .map_err(|e| MarketplaceError::InvalidArchive(format!("zip read: {e}")))?;
        let name = f.name().replace('\\', "/");
        if name.contains("..") || name.starts_with('/') {
            return Err(MarketplaceError::InvalidArchive(format!("unsafe path: {name}")));
        }
        names.push(name);
    }
    let manifest_at_root = names.iter().any(|n| n == "plugin.toml");
    let roots: std::collections::BTreeSet<&str> = names
        .iter()
        .filter_map(|n| n.split('/').next())
        .filter(|s| !s.is_empty())
        .collect();
    let strip = (!manifest_at_root && roots.len() == 1)
        .then(|| format!("{}/", roots.iter().next().copied().unwrap_or_default()));

    let mut written = 0usize;
    for i in 0..archive.len() {
        let mut f = archive
            .by_index(i)
            .map_err(|e| MarketplaceError::InvalidArchive(format!("zip read: {e}")))?;
        let name = f.name().replace('\\', "/");
        let rel = match &strip {
            Some(prefix) => match name.strip_prefix(prefix.as_str()) {
                Some(r) => r,
                None => continue,
            },
            None => name.as_str(),
        };
        if rel.is_empty() {
            continue;
        }
        let mut out_path = target.to_path_buf();
        for part in rel.split('/').filter(|p| !p.is_empty()) {
            out_path.push(part);
        }
        if f.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = std::fs::File::create(&out_path)?;
        std::io::copy(&mut f, &mut out_file)?;
        written += 1;
    }
    Ok(written)
}

/// Walk the zip archive, find the single top-level folder, then extract
/// everything underneath `{root}/{subpath}/` into `target` with the subpath
/// stripped. Returns the number of files written.
fn extract_subpath_into(
    bytes:   &[u8],
    subpath: &str,
    target:  &Path,
    label:   &str,
) -> Result<usize> {
    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| MarketplaceError::InvalidArchive(format!("'{label}': {e}")))?;

    // Pass 1: discover the (single) archive root and verify our subpath
    // actually exists.
    let mut archive_root: Option<String> = None;
    let mut subpath_present = false;
    let subpath_clean = subpath.trim_matches('/').to_string();

    for i in 0..archive.len() {
        let f = archive.by_index(i)
            .map_err(|e| MarketplaceError::InvalidArchive(format!("zip read: {e}")))?;
        let name = f.name().replace('\\', "/");
        if name.contains("..") || name.starts_with('/') {
            return Err(MarketplaceError::InvalidArchive(format!("unsafe path: {name}")));
        }
        let first = name.split('/').next().unwrap_or("");
        if first.is_empty() { continue; }
        if archive_root.is_none() {
            archive_root = Some(first.to_string());
        } else if archive_root.as_deref() != Some(first) {
            return Err(MarketplaceError::InvalidArchive(format!(
                "'{label}' has multiple top-level folders"
            )));
        }
        // Does this entry sit under `{archive_root}/{subpath}/...`?
        let prefix = if subpath_clean.is_empty() {
            format!("{first}/")
        } else {
            format!("{first}/{subpath_clean}/")
        };
        if name.starts_with(&prefix) || name == prefix.trim_end_matches('/') {
            subpath_present = true;
        }
    }
    let root = archive_root.ok_or_else(|| MarketplaceError::InvalidArchive(format!(
        "'{label}' is empty"
    )))?;
    if !subpath_present {
        return Err(MarketplaceError::InvalidArchive(format!(
            "subpath '{subpath_clean}' not found inside archive for '{label}'"
        )));
    }

    let extract_prefix = if subpath_clean.is_empty() {
        format!("{root}/")
    } else {
        format!("{root}/{subpath_clean}/")
    };

    // Pass 2: extract.
    let mut written = 0usize;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)
            .map_err(|e| MarketplaceError::InvalidArchive(format!("zip read: {e}")))?;
        let name = f.name().replace('\\', "/");
        if name.contains("..") || name.starts_with('/') {
            return Err(MarketplaceError::InvalidArchive(format!("unsafe path: {name}")));
        }
        let Some(rel) = name.strip_prefix(&extract_prefix) else { continue; };
        if rel.is_empty() { continue; }

        let mut out_path = target.to_path_buf();
        for part in rel.split('/').filter(|p| !p.is_empty()) {
            out_path.push(part);
        }

        if f.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = std::fs::File::create(&out_path)?;
        std::io::copy(&mut f, &mut out_file)?;
        written += 1;
    }

    // Sanity check — plugin folders MUST carry a plugin.toml at the root.
    if !target.join("plugin.toml").exists() {
        return Err(MarketplaceError::InvalidArchive(format!(
            "extracted archive for '{label}' is missing plugin.toml"
        )));
    }
    Ok(written)
}

// ---------------------------------------------------------------------------
// Plugin uninstall
// ---------------------------------------------------------------------------

/// Remove a plugin: its files, and the secrets it owned.
///
/// The second half is easy to forget and the reason it is here: deleting a directory leaves
/// a plugin's credentials sitting in the OS keychain with nothing on disk to explain them.
/// The user asked for the plugin to be gone.
///
/// Credentials are cleared FIRST. If it went the other way round and the directory removal
/// failed, the plugin would still be installed but stripped of its tokens — a state where it
/// looks fine and silently cannot authenticate. Losing the files while the secrets linger is
/// the less confusing of the two failures, and the one the next uninstall fixes.
pub fn uninstall_plugin(host: &dyn MarketplaceHost, name: &str) -> Result<()> {
    host.forget_plugin_credentials(name);
    let target = paths::plugins_dir().join(name);
    if target.exists() {
        std::fs::remove_dir_all(&target)?;
        tracing::info!("marketplace: removed install dir {target:?}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Theme install / uninstall
// ---------------------------------------------------------------------------

pub async fn install_theme(theme: &MarketplaceTheme) -> Result<InstalledTheme> {
    let (owner, repo) = parse_github_repo(&theme.entry.repo)
        .ok_or_else(|| MarketplaceError::InvalidUrl(theme.entry.repo.clone()))?;
    let r#ref   = theme.entry.r#ref.clone().unwrap_or_else(|| REGISTRY_REF.to_string());
    let subpath = theme.entry.subpath.clone()
        .ok_or_else(|| MarketplaceError::Other(format!("theme '{}' has no subpath", theme.id)))?;

    // Themes live at a single JSON file — fetch raw and write to the user's
    // themes dir so the existing host loader picks it up.
    let url = raw_url(&owner, &repo, &r#ref, &subpath);
    let http = client()?;
    let body = http.get(&url).send().await
        .map_err(|e| MarketplaceError::Other(format!("GET {url}: {e}")))?
        .error_for_status()
        .map_err(|e| MarketplaceError::Other(format!("HTTP {url}: {e}")))?
        .text().await
        .map_err(|e| MarketplaceError::Other(format!("body {url}: {e}")))?;

    // Validate — make sure the JSON parses and the `id` matches.
    let parsed: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| MarketplaceError::Other(format!("theme JSON parse: {e}")))?;
    let file_id = parsed.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if file_id != theme.id {
        return Err(MarketplaceError::Other(format!(
            "theme id mismatch: registry says '{}', file says '{}'",
            theme.id, file_id
        )));
    }

    let dir = paths::themes_dir();
    std::fs::create_dir_all(&dir)?;
    let out_path = dir.join(format!("{}.json", theme.id));
    std::fs::write(&out_path, &body)?;

    Ok(InstalledTheme {
        id:           theme.id.clone(),
        name:         theme.name.clone(),
        entry:        theme.entry.clone(),
        install_path: out_path.to_string_lossy().to_string(),
        installed_at: installs::now_secs(),
    })
}

pub fn uninstall_theme(id: &str) -> Result<()> {
    let path = paths::themes_dir().join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path)?;
        tracing::info!("marketplace: removed theme file {path:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a zip in memory from `(path, contents)` pairs.
    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            for (name, body) in files {
                w.start_file(*name, opts).unwrap();
                w.write_all(body.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        buf
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir()
            .join(format!("arbor-mkt-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_zip_rooted_at_the_files_extracts_flat() {
        let dir = scratch("flat");
        let z = zip_of(&[("plugin.toml", "name = \"x\""), ("main.lua", "-- x")]);
        let n = extract_package_zip(&z, &dir, "x").unwrap();
        assert_eq!(n, 2);
        assert!(dir.join("plugin.toml").exists());
        assert!(dir.join("main.lua").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_single_wrapping_folder_is_stripped() {
        // Which one you get depends on how somebody's release job invoked `zip`, and that
        // is not worth a failed install.
        let dir = scratch("wrapped");
        let z = zip_of(&[("cloud/plugin.toml", "name = \"cloud\""), ("cloud/main.lua", "-- x")]);
        extract_package_zip(&z, &dir, "cloud").unwrap();
        assert!(dir.join("plugin.toml").exists(), "the wrapper should be gone");
        assert!(!dir.join("cloud").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_lone_subdirectory_is_not_mistaken_for_a_wrapper() {
        // The case the `manifest_at_root` check exists for: every entry shares the `lua/`
        // prefix except the manifest, and stripping it would throw the manifest away.
        let dir = scratch("subdir");
        let z = zip_of(&[("plugin.toml", "name = \"x\""), ("lua/util.lua", "-- u")]);
        extract_package_zip(&z, &dir, "x").unwrap();
        assert!(dir.join("plugin.toml").exists());
        assert!(dir.join("lua/util.lua").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_wrapper_whose_contents_are_all_in_one_folder_still_strips_once() {
        let dir = scratch("deep");
        let z = zip_of(&[("pkg/plugin.toml", "name = \"x\""), ("pkg/lua/util.lua", "-- u")]);
        extract_package_zip(&z, &dir, "x").unwrap();
        assert!(dir.join("plugin.toml").exists());
        assert!(dir.join("lua/util.lua").exists(), "only ONE level comes off");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resetting_creates_the_directory_and_clears_what_was_there() {
        let dir = scratch("reset");
        std::fs::write(dir.join("stale.lua"), "old").unwrap();
        reset_dir(&dir).unwrap();
        assert!(dir.exists());
        assert!(!dir.join("stale.lua").exists());
        // And it works on a path that does not exist yet — a first install.
        let fresh = dir.join("nested/deep");
        reset_dir(&fresh).unwrap();
        assert!(fresh.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_traversal_path_is_refused() {
        let dir = scratch("evil");
        let z = zip_of(&[("../../etc/passwd", "nope")]);
        let err = extract_package_zip(&z, &dir, "x").unwrap_err().to_string();
        assert!(err.contains("unsafe path"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
