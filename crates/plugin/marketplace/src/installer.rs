//! Zipball installer for marketplace plugins + theme writer for themes.
//!
//! Plugins:
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
    archive_url, client, parse_github_repo, raw_url, resolve_ref_sha,
};
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

    // Download the zipball.
    let zip_url = archive_url(&owner, &repo, &r#ref);
    tracing::info!("marketplace: downloading {zip_url}");
    let http = client()?;
    let bytes = http.get(&zip_url).send().await
        .map_err(|e| MarketplaceError::Other(format!("GET {zip_url}: {e}")))?
        .error_for_status()
        .map_err(|e| MarketplaceError::Other(format!("HTTP {zip_url}: {e}")))?
        .bytes().await
        .map_err(|e| MarketplaceError::Other(format!("body {zip_url}: {e}")))?;

    // GitHub archive rooting:
    //   `archive/{ref}.zip` → `{repo}-{ref}/...`
    //   But branch names with `/` flatten to `-`, and refs containing `v`
    //   prefixes are kept verbatim. The safest thing is to discover the
    //   single archive root at read time rather than guessing.
    let target = paths::plugins_dir().join(&plugin.name);
    if target.exists() {
        std::fs::remove_dir_all(&target)?;
    }
    std::fs::create_dir_all(&target)?;

    let written = extract_subpath_into(&bytes, &subpath, &target, &plugin.name)?;
    tracing::info!(
        "marketplace: extracted {} files to {target:?} ({} bytes archive)",
        written, bytes.len()
    );

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

pub fn uninstall_plugin(name: &str) -> Result<()> {
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
