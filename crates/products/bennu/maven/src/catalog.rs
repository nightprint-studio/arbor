//! The local repository, read once as a list of coordinates.
//!
//! ## What it is for
//!
//! Two questions, and they are the same question asked from opposite ends:
//!
//! - *"what can I type here"* — the completion popup inside `<groupId>` / `<artifactId>` /
//!   `<version>`, which needs every coordinate the machine has, ranked;
//! - *"does this exist"* — the red underline under a dependency, which needs to distinguish a
//!   wrong version from a wrong artifactId, and both from a coordinate nobody has ever downloaded.
//!
//! [`crate::repo::LocalRepo`] answers the second for one coordinate at a time with a `stat`, which
//! is all the checker needs. Completion cannot work that way — there is no coordinate yet — so the
//! repository is walked once and kept.
//!
//! ## The walk, and why it is bounded the way it is
//!
//! Maven's layout makes the walk trivial to terminate: a directory holding a `.pom` or a `.jar` **is**
//! a version, its parent is the artifact, and everything above that is the group. So the scan reads
//! each directory exactly once, stops descending the moment it recognises a version, and never opens
//! a file. On a large repository that is a few seconds cold and a fraction of one warm.
//!
//! It still is not something to do on a keystroke, so the result is cached to disk next to bennu's
//! other per-machine data and re-read from there. The cache carries a build time and is refreshed
//! on a TTL rather than on a stamp, because there is nothing on a repository root that changes when
//! an artifact lands three directories down — `mtime` on `~/.m2/repository` says nothing at all.
//! A resolve that installs something new can [`Catalog::note`] it in without a rescan, which covers
//! the case the TTL would otherwise get wrong: you add a dependency, build, and come back.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::repo::{sort_versions_desc, LocalRepo};

/// How long a cached scan is served before it is rebuilt. Long enough that opening five projects in
/// an afternoon scans once; short enough that a repository filled up yesterday is not still being
/// described as empty tomorrow.
const TTL_SECS: u64 = 12 * 60 * 60;

/// One artifact of the local repository, with every version installed for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub group_id: String,
    pub artifact_id: String,
    /// Newest first — see [`crate::repo::sort_versions_desc`].
    pub versions: Vec<String>,
}

impl Artifact {
    pub fn ga(&self) -> String {
        format!("{}:{}", self.group_id, self.artifact_id)
    }

    pub fn latest(&self) -> &str {
        self.versions.first().map(String::as_str).unwrap_or_default()
    }
}

/// Every coordinate the local repository holds.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    /// The repository this describes — a catalog is worthless against a different one.
    pub repo: String,
    /// Unix seconds. Zero for a catalog that was never scanned.
    pub built_at: u64,
    /// Sorted by `groupId` then `artifactId`, which is the order the search relies on.
    pub artifacts: Vec<Artifact>,
}

impl Catalog {
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.artifacts.len()
    }

    /// Total installed versions across every artifact — the number that says how big a repository is.
    pub fn version_count(&self) -> usize {
        self.artifacts.iter().map(|a| a.versions.len()).sum()
    }

    /// The catalog for `repo`: the cached scan when it is fresh, otherwise a new one (saved).
    pub fn ensure(repo: &LocalRepo) -> Self {
        if let Some(hit) = Self::cached(repo) {
            return hit;
        }
        let built = Self::scan(repo);
        built.save();
        built
    }

    /// The cached scan, when there is one for this repository and it has not expired.
    pub fn cached(repo: &LocalRepo) -> Option<Self> {
        let cached: Catalog = serde_json::from_slice(&std::fs::read(cache_path(repo)).ok()?).ok()?;
        if cached.repo != repo.root().to_string_lossy() {
            return None;
        }
        (now_secs().saturating_sub(cached.built_at) < TTL_SECS).then_some(cached)
    }

    /// Walk the repository. Cheap per directory, and never opens a file — see the module docs.
    pub fn scan(repo: &LocalRepo) -> Self {
        /// `<repo>/<up to ~10 group segments>/<artifact>/<version>` — deeper than any real groupId.
        const MAX_DEPTH: usize = 14;
        let mut found: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        walk(repo.root(), repo.root(), MAX_DEPTH, &mut found);
        let artifacts = found
            .into_iter()
            .map(|((group_id, artifact_id), mut versions)| {
                sort_versions_desc(&mut versions);
                Artifact { group_id, artifact_id, versions }
            })
            .collect();
        Self { repo: repo.root().to_string_lossy().to_string(), built_at: now_secs(), artifacts }
    }

    /// Persist, best-effort: a failed write only means the next session scans again.
    pub fn save(&self) {
        let path = cache_path(&LocalRepo::at(self.repo.as_str()));
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec(self) {
            let _ = std::fs::write(path, bytes);
        }
    }

    /// Record a coordinate the catalog did not have — what a resolve calls after Maven installs
    /// something, so the completion list and the checker agree with the repository without a rescan.
    ///
    /// Returns whether anything changed, so a caller can skip the save.
    pub fn note(&mut self, group_id: &str, artifact_id: &str, version: &str) -> bool {
        match self.position(group_id, artifact_id) {
            Ok(i) => {
                if self.artifacts[i].versions.iter().any(|v| v == version) {
                    return false;
                }
                self.artifacts[i].versions.push(version.to_string());
                sort_versions_desc(&mut self.artifacts[i].versions);
            }
            Err(i) => self.artifacts.insert(
                i,
                Artifact {
                    group_id: group_id.to_string(),
                    artifact_id: artifact_id.to_string(),
                    versions: vec![version.to_string()],
                },
            ),
        }
        true
    }

    fn position(&self, group_id: &str, artifact_id: &str) -> Result<usize, usize> {
        self.artifacts
            .binary_search_by(|a| (a.group_id.as_str(), a.artifact_id.as_str()).cmp(&(group_id, artifact_id)))
    }

    /// One artifact, exactly.
    pub fn artifact(&self, group_id: &str, artifact_id: &str) -> Option<&Artifact> {
        self.position(group_id, artifact_id).ok().map(|i| &self.artifacts[i])
    }

    /// Every version installed for a coordinate, newest first.
    pub fn versions(&self, group_id: &str, artifact_id: &str) -> &[String] {
        self.artifact(group_id, artifact_id).map(|a| a.versions.as_slice()).unwrap_or_default()
    }

    /// Whether the repository holds this artifact at any version — the question that separates a
    /// mistyped version from a mistyped artifactId.
    pub fn knows(&self, group_id: &str, artifact_id: &str) -> bool {
        self.artifact(group_id, artifact_id).is_some()
    }

    /// Distinct groupIds beginning with `prefix`, alphabetical, capped.
    pub fn groups_with_prefix(&self, prefix: &str, limit: usize) -> Vec<&str> {
        let lower = prefix.to_ascii_lowercase();
        let mut out: Vec<&str> = Vec::new();
        for a in &self.artifacts {
            if !a.group_id.to_ascii_lowercase().starts_with(&lower) {
                continue;
            }
            if out.last() != Some(&a.group_id.as_str()) && !out.contains(&a.group_id.as_str()) {
                out.push(&a.group_id);
                if out.len() >= limit {
                    break;
                }
            }
        }
        out
    }

    /// The artifacts of one group whose artifactId begins with `prefix`.
    pub fn artifacts_in(&self, group_id: &str, prefix: &str, limit: usize) -> Vec<&Artifact> {
        let lower = prefix.to_ascii_lowercase();
        self.artifacts
            .iter()
            .filter(|a| a.group_id == group_id && a.artifact_id.to_ascii_lowercase().starts_with(&lower))
            .take(limit)
            .collect()
    }

    /// Free search over `groupId:artifactId`, for the caret that has only half a coordinate.
    ///
    /// Ranked, because an unranked list of everything matching `spring` is a list nobody reads:
    /// an artifactId that *starts* with the query is what was being typed, a group that starts with
    /// it is next, and a match anywhere is the fallback.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&Artifact> {
        let q = query.to_ascii_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u8, &Artifact)> = Vec::new();
        for a in &self.artifacts {
            let artifact = a.artifact_id.to_ascii_lowercase();
            let group = a.group_id.to_ascii_lowercase();
            let rank = if artifact == q {
                0
            } else if artifact.starts_with(&q) {
                1
            } else if group.starts_with(&q) {
                2
            } else if artifact.contains(&q) || group.contains(&q) {
                3
            } else {
                continue;
            };
            hits.push((rank, a));
        }
        hits.sort_by(|(ra, a), (rb, b)| ra.cmp(rb).then_with(|| a.artifact_id.cmp(&b.artifact_id)));
        hits.into_iter().take(limit).map(|(_, a)| a).collect()
    }
}

/// One directory: recognise a version, or descend.
fn walk(root: &Path, dir: &Path, depth_left: usize, out: &mut BTreeMap<(String, String), Vec<String>>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut subdirs: Vec<PathBuf> = Vec::new();
    let mut is_version = false;
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            subdirs.push(entry.path());
            continue;
        }
        if is_version {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // `.lastUpdated` is Maven's record of a download that FAILED. A directory holding only
        // those is not a version you have — offering it would propose the one version that is
        // certain not to resolve.
        is_version = (name.ends_with(".pom") || name.ends_with(".jar") || name.ends_with(".war")
            || name.ends_with(".aar"))
            && !name.ends_with(".lastUpdated");
    }

    if is_version {
        if let Some((group_id, artifact_id, version)) = coordinate_of(root, dir) {
            out.entry((group_id, artifact_id)).or_default().push(version);
        }
        return; // a version directory holds files, not more coordinates
    }
    if depth_left == 0 {
        return;
    }
    for sub in subdirs {
        walk(root, &sub, depth_left - 1, out);
    }
}

/// `<root>/org/springframework/spring-web/5.3.27` → `("org.springframework", "spring-web", "5.3.27")`.
///
/// The root is known here, so the group is exact rather than found by a marker segment — which is
/// the whole distinction [`coord_under`] exists for, and the reason this is one line instead of a
/// fourth copy of the layout.
fn coordinate_of(root: &Path, version_dir: &Path) -> Option<(String, String, String)> {
    let coord = crate::repo::coord_under(root, version_dir)?;
    Some((coord.group_id, coord.artifact_id, coord.version))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `bennu_data_dir()/maven-catalog/<repo-hash>.json` — keyed by the repository, so a machine with a
/// relocated one does not read the default repository's catalog.
fn cache_path(repo: &LocalRepo) -> PathBuf {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in repo.root().to_string_lossy().as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    arbor_core::prelude::bennu_data_dir()
        .join("maven-catalog")
        .join(format!("{hash:016x}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, LocalRepo) {
        let dir = std::env::temp_dir()
            .join(format!("bennu-mvn-catalog-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, file) in [
            ("org/springframework/spring-web/5.3.27", "spring-web-5.3.27.jar"),
            ("org/springframework/spring-web/5.3.30", "spring-web-5.3.30.jar"),
            ("org/springframework/spring-core/5.3.27", "spring-core-5.3.27.jar"),
            ("junit/junit/4.13.2", "junit-4.13.2.jar"),
            // A download that failed: no artifact, only Maven's note that it could not be had.
            ("com/acme/ghost/1.0", "ghost-1.0.jar.lastUpdated"),
        ] {
            let d = dir.join(path);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(file), b"x").unwrap();
        }
        let repo = LocalRepo::at(&dir);
        (dir, repo)
    }

    #[test]
    fn the_layout_is_read_back_as_coordinates() {
        let (dir, repo) = fixture();
        let catalog = Catalog::scan(&repo);
        assert_eq!(catalog.len(), 3, "{:?}", catalog.artifacts);
        assert_eq!(catalog.versions("org.springframework", "spring-web"), ["5.3.30", "5.3.27"]);
        assert!(catalog.knows("junit", "junit"));
        // The failed download is not a version you have.
        assert!(!catalog.knows("com.acme", "ghost"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn search_puts_what_was_being_typed_first() {
        let (dir, repo) = fixture();
        let catalog = Catalog::scan(&repo);
        let hits = catalog.search("spring-w", 10);
        assert_eq!(hits.first().map(|a| a.artifact_id.as_str()), Some("spring-web"));
        // A group-prefix match still answers, after the artifact ones.
        assert_eq!(catalog.search("org.spring", 10).len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_newly_installed_artifact_is_noted_without_a_rescan() {
        let (dir, repo) = fixture();
        let mut catalog = Catalog::scan(&repo);
        assert!(catalog.note("com.acme", "widget", "1.0"));
        assert!(!catalog.note("com.acme", "widget", "1.0"), "already known");
        assert!(catalog.knows("com.acme", "widget"));
        // The sorted invariant the binary search stands on survives the insert.
        let mut sorted = catalog.artifacts.clone();
        sorted.sort_by(|a, b| (&a.group_id, &a.artifact_id).cmp(&(&b.group_id, &b.artifact_id)));
        assert_eq!(sorted.iter().map(|a| a.ga()).collect::<Vec<_>>(),
                   catalog.artifacts.iter().map(|a| a.ga()).collect::<Vec<_>>());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn groups_and_artifacts_complete_by_prefix() {
        let (dir, repo) = fixture();
        let catalog = Catalog::scan(&repo);
        assert_eq!(catalog.groups_with_prefix("org.spring", 10), ["org.springframework"]);
        let arts = catalog.artifacts_in("org.springframework", "spring-c", 10);
        assert_eq!(arts.iter().map(|a| a.artifact_id.as_str()).collect::<Vec<_>>(), ["spring-core"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
