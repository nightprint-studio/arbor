//! A JDBC driver the project does not depend on: fetched from Maven Central once, then kept.
//!
//! The jar is looked for on the project's classpath first (that is the version the application was
//! written against); only a driver that is not there comes here. What is downloaded is the newest
//! release of the artifact, kept in the Bennu profile beside the shared Tomcat so every project and
//! every configuration reuses it — and a jar already kept is used without asking the network
//! anything, so a launch works offline from the second one on. Deleting the file fetches it again.

use std::path::{Path, PathBuf};

use bennu_tomcat::prelude::{central_driver, parse_release, parse_sha512, CentralDriver};

use super::download;

const MAX_DRIVER_BYTES: u64 = 64 * 1024 * 1024;

/// What became of a driver that is not on the classpath.
pub(super) enum Fetch {
    /// Not a driver Maven Central can supply (Oracle, DB2, …): it has to be chosen by hand.
    Unknown,
    /// A copy kept from an earlier launch.
    Cached(PathBuf),
    /// Just downloaded.
    Downloaded(PathBuf),
    /// The driver is known but downloads are switched off.
    NotAllowed,
}

fn cache_dir() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("tools").join("jdbc")
}

/// The most recently written kept jar of `driver`'s artifact.
fn newest_kept(dir: &Path, driver: &CentralDriver) -> Option<PathBuf> {
    let prefix = format!("{}-", driver.artifact);
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with(&prefix) && name.ends_with(".jar")
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

/// The jar of `driver` (a class name), downloading it when `allow_download` and none is kept.
pub(super) fn ensure(driver: &str, allow_download: bool) -> Result<Fetch, String> {
    let Some(central) = central_driver(driver) else { return Ok(Fetch::Unknown) };
    let dir = cache_dir();
    if let Some(jar) = newest_kept(&dir, &central) {
        return Ok(Fetch::Cached(jar));
    }
    if !allow_download {
        return Ok(Fetch::NotAllowed);
    }
    download_newest(&dir, &central).map(Fetch::Downloaded)
}

fn download_newest(dir: &Path, central: &CentralDriver) -> Result<PathBuf, String> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let metadata = rt.block_on(download::get_text(&central.metadata_url(), 256 * 1024))?;
    let version = parse_release(&metadata)
        .ok_or_else(|| format!("Maven Central names no release of {}.", central.artifact))?;
    let url = central.jar_url(&version).ok_or("Maven Central named a version that cannot be fetched.")?;

    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let name = central.file_name(&version);
    let part = dir.join(format!(".{name}.part"));
    let actual = rt.block_on(download::download(&url, &part, MAX_DRIVER_BYTES, &|_, _| {}));
    let actual = match actual {
        Ok(hash) => hash,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
    };

    // Central publishes a SHA-512 beside most artifacts; when it does, the download has to match it.
    // Without one the transport is what vouches for the file: HTTPS, to a host the client is pinned to.
    let expected = rt.block_on(download::get_text(&format!("{url}.sha512"), 4096)).ok().and_then(|t| parse_sha512(&t));
    if expected.is_some_and(|e| e != actual) {
        let _ = std::fs::remove_file(&part);
        return Err("The download does not match the checksum Maven Central publishes, so it was discarded.".into());
    }

    let target = dir.join(&name);
    std::fs::rename(&part, &target).map_err(|e| format!("cannot keep {}: {e}", target.display()))?;
    Ok(target)
}
