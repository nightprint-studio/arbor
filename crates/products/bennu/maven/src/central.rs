//! Maven Central as a source of *"there is a newer one of these"* — the pure half.
//!
//! ## Why this is not in `catalog`
//!
//! [`crate::catalog`] answers "what is in the local repository", which is a different question that
//! looks like this one. A repository holds what the project has already asked for, so the newest
//! version it knows about is the newest version somebody on this machine has already adopted — a
//! dependency nobody has updated is, by that measure, permanently up to date. Which is precisely
//! the dependency worth saying something about.
//!
//! So this reads Central's own `maven-metadata.xml`: one small XML file per artifact, at a URL
//! computed from the coordinate, listing every version ever published. No API, no key, no search
//! endpoint — the same file Maven itself reads to resolve a range.
//!
//! ## The socket is the caller's
//!
//! The crate's rule (see the module docs) is that nothing here executes a process or opens a
//! connection: that is what keeps every other answer cheap enough to give on a keystroke. This
//! module keeps that rule. It computes the URL, parses the response, and owns the on-disk cache —
//! the `be` layer is what does the GET, exactly as it is for crates.io.
//!
//! ## Stale beats absent
//!
//! A cached metadata file is served past its TTL when a fetch fails. On a train, or behind a proxy
//! that eats `repo1.maven.org`, last week's answer is the right answer, and no answer is a hint
//! that silently stops existing.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use bennu_xml::prelude::{scan, TagKind};

/// Maven Central's canonical mirror. The one repository whose layout is guaranteed and whose
/// metadata is public — a corporate Nexus may or may not proxy it, and asking one for an artifact
/// it has never heard of is a 404 rather than an answer.
pub const CENTRAL: &str = "https://repo1.maven.org/maven2";

/// The `maven-metadata.xml` URL for one artifact.
///
/// The group's dots are path separators, which is the whole of Maven's repository layout —
/// `org.springframework:spring-core` lives at `org/springframework/spring-core/`.
pub fn metadata_url(base: &str, group: &str, artifact: &str) -> String {
    format!("{}/{}/{artifact}/maven-metadata.xml", base.trim_end_matches('/'), group.replace('.', "/"))
}

/// What an artifact's metadata says, reduced to the two things a hint needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    /// Every version listed, in the order the file lists them (oldest first, by convention).
    pub versions: Vec<String>,
    /// The `<release>` the file names, when it names one — Maven's own idea of the newest
    /// non-snapshot version.
    pub release: Option<String>,
}

/// Parse a `maven-metadata.xml`. Never fails: an unreachable-turned-HTML response, a truncated
/// download and an artifact with no versions all yield the empty answer, which every caller
/// already handles as "nothing to say".
pub fn parse_metadata(xml: &str) -> Metadata {
    let s = scan(xml);
    let mut versions = Vec::new();
    let mut release = None;
    for (i, tag) in s.tags.iter().enumerate() {
        if tag.kind != TagKind::Open {
            continue;
        }
        let name = tag.local();
        if name != "version" && name != "release" {
            continue;
        }
        // The close tag is the next one at this depth; for a leaf like `<version>` that is simply
        // the next close tag, and anything else means the document is not what it claimed to be.
        let Some(close) = s.tags.get(i + 1).filter(|t| t.kind == TagKind::Close) else { continue };
        let (start, end) = (tag.end, close.start);
        if start > end || end > xml.len() {
            continue;
        }
        let text = xml[start..end].trim();
        if text.is_empty() || text.contains('<') {
            continue;
        }
        if name == "release" {
            release = Some(text.to_string());
        } else {
            versions.push(text.to_string());
        }
    }
    Metadata { versions, release }
}

/// The newest **release** the metadata knows about, or `None` when it knows of none.
///
/// `<release>` is preferred because it is what the repository itself maintains, and it is what
/// Maven resolves `LATEST` to. It is not blindly trusted: a few artifacts publish a milestone as
/// their release, so the version list is consulted too and the newer of the two wins — by
/// [`crate::repo::compare_versions`], which already ranks a release above its own pre-releases.
pub fn latest_release(meta: &Metadata) -> Option<String> {
    let newest_listed = meta
        .versions
        .iter()
        .filter(|v| !is_prerelease(v))
        .max_by(|a, b| crate::repo::compare_versions(a, b))
        .cloned();
    match (meta.release.clone(), newest_listed) {
        (Some(release), Some(listed)) => Some(
            if crate::repo::compare_versions(&listed, &release) == std::cmp::Ordering::Greater {
                listed
            } else {
                release
            },
        ),
        (a, b) => a.or(b),
    }
}

/// Whether a version names something not meant to be depended on yet.
///
/// Suffix-matched on the qualifier rather than substring-matched on the whole string, because
/// `1.0-alpha` is a pre-release and `alpha-vantage-client:1.0` is a library.
pub fn is_prerelease(version: &str) -> bool {
    const MARKERS: [&str; 9] = ["snapshot", "alpha", "beta", "rc", "cr", "m", "ea", "preview", "dev"];
    let lower = version.to_ascii_lowercase();
    let Some(qualifier) = lower.split(['-', '_']).nth(1) else {
        // No `-`/`_` qualifier: `1.2.3.RELEASE` and `1.2.3` are both releases; `1.2.3.M1` is not.
        return lower
            .rsplit('.')
            .next()
            .is_some_and(|last| MARKERS.iter().any(|m| is_marker(last, m)));
    };
    MARKERS.iter().any(|m| is_marker(qualifier, m))
}

/// Whether a qualifier segment IS a marker — the marker itself, or the marker followed by a
/// number (`rc1`, `m2`, `beta-3` reaching us as `beta`). `m` needs this most: every version
/// ending in a word starting with `m` is not a milestone.
fn is_marker(segment: &str, marker: &str) -> bool {
    let Some(rest) = segment.strip_prefix(marker) else { return false };
    rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Whether `latest` is worth telling someone on `current` about.
///
/// Not merely "different": a project pinned to an internal build, or to something newer than
/// Central's release, must not be told to go backwards. And a version written as a property
/// (`${spring.version}`) is not a version at all — it is a name — so it is left alone rather than
/// compared against as if it were `${spring.version}` literally.
pub fn is_newer(current: &str, latest: &str) -> bool {
    if current.is_empty() || current.contains("${") {
        return false;
    }
    crate::repo::compare_versions(latest, current) == std::cmp::Ordering::Greater
}

// ── the on-disk cache ────────────────────────────────────────────────────────

/// Where one artifact's cached metadata lives under `dir`.
///
/// The coordinate is flattened into a single file name rather than into directories: the cache is
/// a flat pile that is deleted wholesale, and a tree of six-deep group folders is a tree somebody
/// has to walk to clear.
pub fn cache_path(dir: &Path, group: &str, artifact: &str) -> PathBuf {
    let safe: String = format!("{group}__{artifact}")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    dir.join(format!("{safe}.xml"))
}

/// Whether a cached file is younger than `ttl`. A file that cannot be stat'd is not fresh, which
/// is the direction that refetches rather than the one that serves nothing.
pub fn cache_is_fresh(path: &Path, ttl: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < ttl)
}

/// Read a cached metadata file, however old.
pub fn read_cache(dir: &Path, group: &str, artifact: &str) -> Option<String> {
    std::fs::read_to_string(cache_path(dir, group, artifact)).ok()
}

/// Write one, creating the directory. Silent on failure: a cache that cannot be written costs a
/// request next time, not an error the user has to read.
pub fn write_cache(dir: &Path, group: &str, artifact: &str, body: &str) {
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(cache_path(dir, group, artifact), body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>org.springframework</groupId>
  <artifactId>spring-core</artifactId>
  <versioning>
    <latest>6.2.0-M1</latest>
    <release>6.1.14</release>
    <versions>
      <version>5.3.39</version>
      <version>6.1.14</version>
      <version>6.2.0-M1</version>
    </versions>
  </versioning>
</metadata>"#;

    #[test]
    fn the_url_turns_the_group_into_a_path() {
        assert_eq!(
            metadata_url(CENTRAL, "org.springframework", "spring-core"),
            "https://repo1.maven.org/maven2/org/springframework/spring-core/maven-metadata.xml"
        );
    }

    #[test]
    fn a_trailing_slash_on_the_base_does_not_double_up() {
        assert!(!metadata_url("https://x/m2/", "g", "a").contains("//g/"));
    }

    #[test]
    fn the_versions_and_the_release_are_both_read() {
        let m = parse_metadata(META);
        assert_eq!(m.versions, ["5.3.39", "6.1.14", "6.2.0-M1"]);
        assert_eq!(m.release.as_deref(), Some("6.1.14"));
    }

    #[test]
    fn the_milestone_is_never_the_answer() {
        // `<latest>` says `6.2.0-M1`, and it is deliberately not read: a milestone is not something
        // to tell a project it is behind.
        assert_eq!(latest_release(&parse_metadata(META)).as_deref(), Some("6.1.14"));
    }

    #[test]
    fn a_release_newer_than_the_declared_one_still_wins() {
        let xml = "<metadata><versioning><release>1.0</release>\
<versions><version>1.0</version><version>2.0</version></versions></versioning></metadata>";
        assert_eq!(latest_release(&parse_metadata(xml)).as_deref(), Some("2.0"));
    }

    #[test]
    fn nothing_parseable_is_nothing_said() {
        assert_eq!(parse_metadata("<html><body>404</body></html>"), Metadata::default());
        assert_eq!(latest_release(&Metadata::default()), None);
    }

    #[test]
    fn prereleases_are_recognised_by_their_qualifier_and_not_by_a_substring() {
        for v in ["1.0-SNAPSHOT", "2.0-rc1", "6.2.0-M1", "1.0.0-alpha", "3.0.0.M2", "1.0-beta2"] {
            assert!(is_prerelease(v), "{v} should read as a pre-release");
        }
        for v in ["1.0", "1.2.3.RELEASE", "31.1-jre", "2.0-android", "1.0.0.Final", "9.4.53.v2023"] {
            assert!(!is_prerelease(v), "{v} should read as a release");
        }
    }

    #[test]
    fn nobody_is_told_to_go_backwards() {
        assert!(is_newer("1.0", "1.1"));
        assert!(!is_newer("2.0", "1.9"));
        assert!(!is_newer("1.0", "1.0"));
    }

    #[test]
    fn a_property_is_a_name_and_not_a_version() {
        // Comparing `${spring.version}` against `6.1.14` would say "newer" for every artifact whose
        // version a pom keeps in one place — which is most well-kept poms.
        assert!(!is_newer("${spring.version}", "6.1.14"));
        assert!(!is_newer("", "6.1.14"));
    }

    #[test]
    fn the_cache_file_name_survives_a_coordinate_with_separators_in_it() {
        let p = cache_path(Path::new("/c"), "org.spring/x", "core:1");
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), "org.spring_x__core_1.xml");
    }
}
