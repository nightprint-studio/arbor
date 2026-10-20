//! Known vulnerabilities in a pom's dependencies — `bennu_maven_advisories`, answered by OSV.dev.
//!
//! Like [`crate::maven_central`] this is a question the machine cannot answer by itself, and it is
//! shaped the same way for the same reasons:
//!
//! 1. **A switch, off by default.** [`MavenConfig::advisories`] sends the coordinates (`group:artifact`
//!    and version) of the pom's dependencies to `api.osv.dev`. That is a fact about the editor the
//!    user should have agreed to, so it is opt-in — unlike the version hints, which fetch a public
//!    file and send nothing about the project.
//! 2. **A cache with a TTL.** One small JSON file per `group:artifact:version`, fresh for
//!    [`MavenConfig::metadata_ttl_hours`]. A published version's advisories change rarely.
//! 3. **Bounded and convergent.** One request reads at most [`MAX_COORDS`] coordinates and fetches at
//!    most [`MAX_DETAIL_FETCHES`] advisory records; what it does not reach stays uncached, so the
//!    next call continues. A pom with a hundred dependencies converges over a few passes instead of
//!    stalling on the first.
//!
//! ## Only what the pom itself pins
//!
//! Same rule as the version hints: a dependency with no literal `<version>` has no span to mark, and
//! a warning on a line other than the one that would change is a warning nobody can act on.
//!
//! ## Two calls, because OSV splits them
//!
//! `querybatch` answers *which* advisories affect each coordinate, as bare ids. The summary, the
//! severity and the fixed version live in each advisory's own record, fetched once per id and shared
//! by every coordinate it affects.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bennu_core::prelude::{BennuState, MavenConfig};
use bennu_maven::prelude::{cache_is_fresh, is_newer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const OSV: &str = "https://api.osv.dev/v1";
/// Coordinates one request may look up.
const MAX_COORDS: usize = 60;
/// Advisory records one request may fetch.
const MAX_DETAIL_FETCHES: usize = 30;

/// Args for [`bennu_maven_advisories`].
#[derive(Deserialize)]
pub struct PomAdvisoryArgs {
    /// The pom's path. Unused today, kept so the call is shaped like every other buffer request.
    pub file: String,
    /// The live buffer — the spans must be in *this* text.
    pub source: String,
}

/// One advisory affecting a dependency, on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Advisory {
    /// OSV's id (`GHSA-…`).
    pub id: String,
    /// The CVE alias, when there is one — what people search for.
    pub cve: Option<String>,
    pub summary: String,
    /// `critical` | `high` | `moderate` | `low` | `unknown`.
    pub severity: String,
    /// The lowest version above the current one that fixes it, when the record says.
    pub fixed: Option<String>,
}

/// One dependency with at least one advisory.
#[derive(Debug, Clone, Serialize)]
pub struct PomAdvisory {
    /// `groupId:artifactId`.
    pub coord: String,
    /// 1-based line of the `<dependency>` tag.
    pub line: u32,
    /// Byte span of the version **value** — where the warning is drawn.
    pub start: usize,
    pub end: usize,
    /// The version the pom writes.
    pub current: String,
    pub advisories: Vec<Advisory>,
}

/// Which of the pom's pinned dependencies have known vulnerabilities.
///
/// Empty — never an error — when the switch is off, the buffer is not a pom, nothing is affected, or
/// OSV cannot be reached and nothing is cached: a warning that failed loudly would be a dialog about
/// an absent underline.
#[arbor_rpc::handler]
async fn bennu_maven_advisories(
    _ctx: &BennuState,
    args: PomAdvisoryArgs,
) -> Result<Vec<PomAdvisory>, String> {
    let cfg = bennu_core::config::load().maven;
    if !cfg.advisories {
        return Ok(Vec::new());
    }
    let _ = &args.file;
    Ok(advisories_for(&cfg, &args.source).await)
}

fn cache_dir() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("maven-advisories")
}

fn ttl(cfg: &MavenConfig) -> Duration {
    let hours = if cfg.metadata_ttl_hours == 0 { 24 } else { cfg.metadata_ttl_hours };
    Duration::from_secs(u64::from(hours) * 3600)
}

/// A file name for one coordinate: every character a path would object to becomes `_`.
fn cache_file(dir: &Path, coord: &str, version: &str) -> PathBuf {
    let safe: String = format!("{coord}@{version}")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '@') { c } else { '_' })
        .collect();
    dir.join(format!("{safe}.json"))
}

/// A dependency to look up, with where to draw its warning.
struct Target {
    coord: String,
    version: String,
    line: u32,
    span: (usize, usize),
}

async fn advisories_for(cfg: &MavenConfig, source: &str) -> Vec<PomAdvisory> {
    let pom = bennu_deps::prelude::parse_pom(source);
    let targets: Vec<Target> = crate::maven_central::candidates(&pom)
        .into_iter()
        .filter_map(|d| {
            d.version_span.map(|span| Target {
                coord: d.coord(),
                version: d.version.clone(),
                line: d.line,
                span,
            })
        })
        .collect();

    let dir = cache_dir();
    let mut found: Vec<(usize, Vec<Advisory>)> = Vec::new();
    let mut misses: Vec<usize> = Vec::new();
    for (i, t) in targets.iter().enumerate() {
        match read_cached(&dir, t, cfg) {
            Some(list) => found.push((i, list)),
            None => misses.push(i),
        }
    }
    misses.truncate(MAX_COORDS);

    if !misses.is_empty() {
        for (i, list) in lookup(&targets, &misses, &dir).await {
            found.push((i, list));
        }
    }

    found.sort_by_key(|(i, _)| *i);
    found
        .into_iter()
        .filter(|(_, list)| !list.is_empty())
        .map(|(i, advisories)| {
            let t = &targets[i];
            PomAdvisory {
                coord: t.coord.clone(),
                line: t.line,
                start: t.span.0,
                end: t.span.1,
                current: t.version.clone(),
                advisories,
            }
        })
        .collect()
}

fn read_cached(dir: &Path, t: &Target, cfg: &MavenConfig) -> Option<Vec<Advisory>> {
    let path = cache_file(dir, &t.coord, &t.version);
    if !cache_is_fresh(&path, ttl(cfg)) {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_cached(dir: &Path, t: &Target, list: &[Advisory]) {
    if std::fs::create_dir_all(dir).is_ok() {
        if let Ok(body) = serde_json::to_string(list) {
            let _ = std::fs::write(cache_file(dir, &t.coord, &t.version), body);
        }
    }
}

/// Ask OSV about `targets[misses]`, then fetch the records of the advisories it names. Coordinates
/// whose records could not all be fetched are returned but **not cached**, so the next call retries.
async fn lookup(targets: &[Target], misses: &[usize], dir: &Path) -> Vec<(usize, Vec<Advisory>)> {
    let Ok(ids_per_target) = query_batch(targets, misses).await else { return Vec::new() };

    let mut records: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    let mut wanted: Vec<String> = ids_per_target.iter().flatten().cloned().collect();
    wanted.sort();
    wanted.dedup();
    wanted.truncate(MAX_DETAIL_FETCHES);
    let handles: Vec<_> = wanted
        .into_iter()
        .map(|id| tokio::spawn(async move { (id.clone(), fetch_record(&id).await) }))
        .collect();
    for h in handles {
        if let Ok((id, Ok(v))) = h.await {
            records.insert(id, v);
        }
    }

    let mut out = Vec::new();
    for (slot, ids) in misses.iter().zip(ids_per_target) {
        let t = &targets[*slot];
        let complete = ids.iter().all(|id| records.contains_key(id));
        let list: Vec<Advisory> = ids
            .iter()
            .map(|id| match records.get(id) {
                Some(rec) => advisory_of(rec, &t.coord, &t.version),
                None => Advisory {
                    id: id.clone(),
                    cve: None,
                    summary: String::new(),
                    severity: "unknown".to_string(),
                    fixed: None,
                },
            })
            .collect();
        if complete {
            write_cached(dir, t, &list);
        }
        out.push((*slot, list));
    }
    out
}

/// `querybatch`: the advisory ids affecting each of `targets[misses]`, in the same order.
async fn query_batch(targets: &[Target], misses: &[usize]) -> Result<Vec<Vec<String>>, String> {
    let queries: Vec<Value> = misses
        .iter()
        .map(|&i| {
            let t = &targets[i];
            json!({ "package": { "ecosystem": "Maven", "name": t.coord }, "version": t.version })
        })
        .collect();
    let body = json!({ "queries": queries }).to_string();
    let resp = arbor_core::prelude::client()
        .post(format!("{OSV}/querybatch"))
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("status {}", resp.status()));
    }
    let v: Value = resp.json().await.map_err(|e| format!("body: {e}"))?;
    Ok(v["results"]
        .as_array()
        .map(|rs| {
            rs.iter()
                .map(|r| {
                    r["vulns"]
                        .as_array()
                        .map(|vs| vs.iter().filter_map(|x| x["id"].as_str().map(String::from)).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default())
}

async fn fetch_record(id: &str) -> Result<Value, String> {
    let resp = arbor_core::prelude::client()
        .get(format!("{OSV}/vulns/{id}"))
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("status {}", resp.status()));
    }
    resp.json().await.map_err(|e| format!("body: {e}"))
}

/// One OSV record, read for `coord` at `current`. Pure — the whole of what the editor shows is
/// decided here, so it is the part with tests.
fn advisory_of(record: &Value, coord: &str, current: &str) -> Advisory {
    let id = record["id"].as_str().unwrap_or_default().to_string();
    let cve = record["aliases"]
        .as_array()
        .and_then(|a| a.iter().filter_map(Value::as_str).find(|s| s.starts_with("CVE-")))
        .map(String::from)
        .or_else(|| id.starts_with("CVE-").then(|| id.clone()));
    let summary = record["summary"]
        .as_str()
        .map(str::to_string)
        .or_else(|| record["details"].as_str().and_then(|d| d.lines().next()).map(str::to_string))
        .unwrap_or_default();
    // GitHub advisories carry a plain word; other databases carry only a CVSS vector, whose score is
    // not worth re-deriving here — they read as `unknown` and are still reported.
    let severity = record["database_specific"]["severity"]
        .as_str()
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "unknown".to_string());
    Advisory { id, cve, summary, severity, fixed: fixed_version(record, coord, current) }
}

/// The lowest fixed version above `current` among the ranges that name `coord`.
fn fixed_version(record: &Value, coord: &str, current: &str) -> Option<String> {
    let mut best: Option<String> = None;
    for affected in record["affected"].as_array()? {
        if affected["package"]["name"].as_str() != Some(coord) {
            continue;
        }
        for range in affected["ranges"].as_array().into_iter().flatten() {
            for ev in range["events"].as_array().into_iter().flatten() {
                let Some(f) = ev["fixed"].as_str() else { continue };
                if !is_newer(current, f) {
                    continue;
                }
                // Keep the smallest: replace `best` when it is newer than `f`.
                if best.as_deref().map_or(true, |b| is_newer(f, b)) {
                    best = Some(f.to_string());
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Value {
        json!({
            "id": "GHSA-36p3-wjmg-h94x",
            "aliases": ["CVE-2022-22965"],
            "summary": "Remote code execution in Spring Framework",
            "database_specific": { "severity": "CRITICAL" },
            "affected": [{
                "package": { "ecosystem": "Maven", "name": "org.springframework:spring-beans" },
                "ranges": [{ "type": "ECOSYSTEM", "events": [
                    { "introduced": "5.3.0" }, { "fixed": "5.3.18" },
                    { "introduced": "5.2.0" }, { "fixed": "5.2.20" }
                ]}]
            }, {
                "package": { "ecosystem": "Maven", "name": "other:artifact" },
                "ranges": [{ "type": "ECOSYSTEM", "events": [{ "fixed": "9.9.9" }] }]
            }]
        })
    }

    #[test]
    fn a_record_becomes_what_the_editor_shows() {
        let a = advisory_of(&record(), "org.springframework:spring-beans", "5.3.10");
        assert_eq!(a.cve.as_deref(), Some("CVE-2022-22965"));
        assert_eq!(a.severity, "critical");
        assert_eq!(a.summary, "Remote code execution in Spring Framework");
    }

    #[test]
    fn the_fix_is_the_nearest_one_above_the_current_version() {
        // 5.2.20 is older than the pom's 5.3.10, so it is not an upgrade path.
        let a = advisory_of(&record(), "org.springframework:spring-beans", "5.3.10");
        assert_eq!(a.fixed.as_deref(), Some("5.3.18"));
        // Another coordinate's ranges in the same record are not this one's.
        assert_eq!(fixed_version(&record(), "unrelated:thing", "1.0"), None);
    }

    #[test]
    fn a_record_without_a_word_severity_reads_unknown() {
        let r = json!({ "id": "OSV-1", "details": "first line\nsecond", "affected": [] });
        let a = advisory_of(&r, "g:a", "1.0");
        assert_eq!(a.severity, "unknown");
        assert_eq!(a.summary, "first line");
        assert_eq!(a.cve, None);
    }

    #[test]
    fn a_cache_file_name_is_safe_on_every_platform() {
        let p = cache_file(Path::new("/c"), "org.x:y/z", "1.0-RC 1");
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        assert!(!name.contains(['/', ':', ' ']), "{name}");
    }
}
