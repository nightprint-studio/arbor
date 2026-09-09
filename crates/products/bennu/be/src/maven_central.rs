//! Maven Central — the one question about a pom that cannot be answered from this machine.
//!
//! | Handler | Question |
//! |---|---|
//! | `bennu_maven_version_hints` | which dependencies in this pom are behind |
//!
//! Everything else Bennu knows about a Maven project is read off the disk: the poms, `~/.m2`, the
//! resolved classpath. This is not, and that makes the same three things load-bearing here as on
//! the Rust side (see [`crate::crates_io`], which this is deliberately shaped like):
//!
//! 1. **A switch.** [`MavenConfig::central`] is on by default and turning it off makes the Java
//!    side entirely local again — the hints disappear, and nothing else changes.
//! 2. **A cache with a TTL.** One `maven-metadata.xml` per artifact per
//!    [`MavenConfig::metadata_ttl_hours`]. A library releases weekly at most.
//! 3. **Stale beats absent.** A failed fetch falls back to whatever is cached, however old.
//!
//! ## Only what the pom itself pins
//!
//! A dependency with no `<version>` is managed by a parent or a BOM, and the version that would
//! have to change is written somewhere else — telling you about it here would point at a line that
//! cannot be edited to fix it. A version written as `${a.property}` is skipped for the same reason
//! and one more: it is a *name*, so there is nothing to compare.
//!
//! The URL layout, the parsing, the version comparison and the cache live in
//! [`bennu_maven::central`], which never opens a socket; this module is the part that does.

use std::path::PathBuf;
use std::time::Duration;

use bennu_core::prelude::{BennuState, MavenConfig};
use bennu_maven::prelude::{
    cache_is_fresh, central_cache_path, central_latest_release, is_newer, metadata_url,
    parse_metadata, read_cache, write_cache, CENTRAL,
};
use serde::{Deserialize, Serialize};

/// Where cached metadata lives. Bennu's own data dir, never `~/.m2` — that belongs to Maven, and
/// writing into another tool's repository is how two tools start corrupting each other's state.
fn cache_dir() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("maven-metadata")
}

/// The configured freshness window. `0` reads as the default rather than as "always refetch".
fn ttl(cfg: &MavenConfig) -> Duration {
    let hours = if cfg.metadata_ttl_hours == 0 { 24 } else { cfg.metadata_ttl_hours };
    Duration::from_secs(u64::from(hours) * 3600)
}

/// How many artifacts one hints request may fetch.
///
/// A reactor pom with eighty dependencies and a cold cache would otherwise be eighty requests
/// before it could answer anything. The ones it does not reach stay uncached, so the next call
/// picks up where this one stopped and the pom converges over a few passes instead of blocking
/// once.
const MAX_FETCHES_PER_REQUEST: usize = 12;

/// Args for [`bennu_maven_version_hints`].
#[derive(Deserialize)]
pub struct PomHintsArgs {
    /// The pom's path. Unused today, and kept so the call is shaped like every other buffer
    /// request — and so a per-project mirror can key off it later without a wire change.
    pub file: String,
    /// The live buffer. The hints are drawn in the editor, so the spans must be in *this* text.
    pub source: String,
}

/// One dependency that has a newer release, on the wire.
#[derive(Debug, Clone, Serialize)]
pub struct PomVersionHint {
    /// `groupId:artifactId` — what the hint is about, for a tooltip that names it.
    pub coord: String,
    /// Byte offset of the `<dependency>` tag — where the hint row is drawn.
    pub offset: usize,
    /// 1-based line of that tag.
    pub line: u32,
    /// Byte span of the version **value**, quotes-free — what accepting the hint replaces.
    pub start: usize,
    pub end: usize,
    /// The version the pom writes.
    pub current: String,
    /// The newest release on Central.
    pub latest: String,
}

/// Which dependencies in the buffer have a newer release on Maven Central.
///
/// Empty — never an error — when the switch is off, when the buffer is not a pom, or when nothing
/// is behind. A hint that failed loudly would be a dialog about a grey line above a line of XML.
#[arbor_rpc::handler]
async fn bennu_maven_version_hints(
    _ctx: &BennuState,
    args: PomHintsArgs,
) -> Result<Vec<PomVersionHint>, String> {
    let cfg = bennu_core::config::load().maven;
    if !cfg.central {
        return Ok(Vec::new());
    }
    let _ = &args.file;
    Ok(hints_for(&cfg, &args.source, MAX_FETCHES_PER_REQUEST).await)
}

/// The dependencies of one pom's text that are behind, under a bounded fetch budget.
async fn hints_for(cfg: &MavenConfig, source: &str, mut budget: usize) -> Vec<PomVersionHint> {
    let pom = bennu_deps::prelude::parse_pom(source);
    let mut out = Vec::new();
    for dep in candidates(&pom) {
        let Some((start, end)) = dep.version_span else { continue };
        let cached_only = budget == 0;
        let path = central_cache_path(&cache_dir(), &dep.group_id, &dep.artifact_id);
        if !cache_is_fresh(&path, ttl(cfg)) {
            if cached_only {
                continue;
            }
            budget -= 1;
        }
        let xml = match cached_only {
            true => read_cache(&cache_dir(), &dep.group_id, &dep.artifact_id),
            false => metadata_of(&dep.group_id, &dep.artifact_id, cfg).await,
        };
        let Some(latest) = xml.map(|x| parse_metadata(&x)).and_then(|m| central_latest_release(&m))
        else {
            continue;
        };
        if !is_newer(&dep.version, &latest) {
            continue;
        }
        out.push(PomVersionHint {
            coord: dep.coord(),
            offset: dep.offset,
            line: dep.line,
            start,
            end,
            current: dep.version.clone(),
            latest,
        });
    }
    out
}

/// The dependencies worth asking Central about: a real coordinate, and a version this pom could
/// actually edit.
///
/// `<dependencyManagement>` is excluded on purpose even though its versions are literal — it is
/// *another* pom's version, and a module told its parent is behind is a module that cannot fix it.
/// The parent's own hints appear when the parent is open, where the edit belongs.
fn candidates(pom: &bennu_deps::prelude::Pom) -> Vec<&bennu_deps::prelude::RawDependency> {
    pom.dependencies
        .iter()
        .filter(|d| {
            !d.group_id.is_empty()
                && !d.artifact_id.is_empty()
                && !d.group_id.contains("${")
                && !d.artifact_id.contains("${")
                && !d.version.is_empty()
                && !d.version.contains("${")
        })
        .collect()
}

/// One artifact's metadata: cache first, then Central, then whatever is cached however old.
async fn metadata_of(group: &str, artifact: &str, cfg: &MavenConfig) -> Option<String> {
    let dir = cache_dir();
    if cache_is_fresh(&central_cache_path(&dir, group, artifact), ttl(cfg)) {
        if let Some(body) = read_cache(&dir, group, artifact) {
            return Some(body);
        }
    }
    match fetch_metadata(group, artifact).await {
        Ok(body) => {
            write_cache(&dir, group, artifact, &body);
            Some(body)
        }
        // Offline, blocked by a proxy, or an artifact Central has never had. An old copy is a
        // better answer than none, and for one that never existed there is none to fall back to.
        Err(_) => read_cache(&dir, group, artifact),
    }
}

/// GET one artifact's `maven-metadata.xml`.
///
/// Through the workspace client, so the request carries Arbor's user-agent and its bounded
/// timeout. A 404 is the ordinary answer for an internal artifact that Central has never seen, and
/// is an `Err` like any other: there is nothing to say about it.
async fn fetch_metadata(group: &str, artifact: &str) -> Result<String, String> {
    let url = metadata_url(CENTRAL, group, artifact);
    let resp = arbor_core::prelude::client()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("status {}", resp.status()));
    }
    resp.text().await.map_err(|e| format!("body: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deps_of(src: &str) -> Vec<String> {
        let pom = bennu_deps::prelude::parse_pom(src);
        candidates(&pom).into_iter().map(|d| d.coord()).collect()
    }

    #[test]
    fn only_a_dependency_this_pom_could_edit_is_asked_about() {
        let src = r#"<project>
          <dependencies>
            <dependency><groupId>g</groupId><artifactId>pinned</artifactId><version>1.0</version></dependency>
            <dependency><groupId>g</groupId><artifactId>managed</artifactId></dependency>
            <dependency><groupId>g</groupId><artifactId>prop</artifactId><version>${g.version}</version></dependency>
          </dependencies>
          <dependencyManagement><dependencies>
            <dependency><groupId>g</groupId><artifactId>elsewhere</artifactId><version>2.0</version></dependency>
          </dependencies></dependencyManagement>
        </project>"#;
        assert_eq!(deps_of(src), ["g:pinned"]);
    }

    #[test]
    fn a_coordinate_written_as_a_property_is_not_a_coordinate() {
        // `${project.groupId}` would be asked about literally, and Central would 404 on every pom
        // in a reactor that writes its own modules this way — which is most of them.
        let src = "<project><dependencies><dependency>\
<groupId>${project.groupId}</groupId><artifactId>core</artifactId><version>1.0</version>\
</dependency></dependencies></project>";
        assert!(deps_of(src).is_empty());
    }

    #[test]
    fn the_ttl_reads_zero_as_the_default() {
        let off = MavenConfig { central: true, metadata_ttl_hours: 0 };
        assert_eq!(ttl(&off), Duration::from_secs(24 * 3600));
        let two = MavenConfig { central: true, metadata_ttl_hours: 2 };
        assert_eq!(ttl(&two), Duration::from_secs(2 * 3600));
    }
}
