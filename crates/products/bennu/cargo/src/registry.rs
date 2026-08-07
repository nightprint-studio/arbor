//! The crates.io index: what versions of a crate exist, and which is the newest.
//!
//! Everything else in this crate answers from files that are already on the machine — the manifest,
//! `Cargo.lock`, the unpacked sources in `$CARGO_HOME`. This module is the one that needs the
//! network, and it is shaped around that fact:
//!
//! * **the fetch is not here.** This module builds the URL, parses the body, and owns the on-disk
//!   cache; the caller performs the request. `bennu-cargo` stays a crate you can run offline and test
//!   without a server, and the HTTP client stays where the runtime is.
//! * **the cache is the normal path.** A version list is answered from disk until it is older than the
//!   caller's TTL. A crate publishes a few times a year; asking the network on every keystroke to
//!   learn that would be indefensible.
//! * **stale beats absent.** When a fetch fails and there is an old cached copy, the old copy wins.
//!   Offline, an aeroplane, a corporate proxy — the right answer to all of them is last week's version
//!   list, not silence.
//!
//! ## The sparse index
//!
//! `index.crates.io` serves one file per crate, at a path derived from the crate's name, containing
//! **one JSON object per line** — one line per published version. No search, no listing: you have to
//! know the name. That is why "add a dependency" here means "type a name and pick a version" rather
//! than a search box; searching is a different API with its own rate limits and etiquette, and it is
//! not needed to answer either of the questions Bennu asks.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::complete::version_key;

/// Where the sparse index lives. The protocol Cargo itself uses since 1.68.
pub const CRATES_IO_INDEX: &str = "https://index.crates.io";

/// One published version of a crate, as the index describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexVersion {
    pub version: String,
    /// Withdrawn by its author. Still resolvable for a lockfile that already pins it, but never
    /// something to *offer* — which is the whole reason this flag is carried.
    pub yanked: bool,
    /// The features it declares, in the index's own order. Free to parse (it is on the same line) and
    /// the only place a feature list can be known without unpacking the crate.
    pub features: Vec<String>,
}

/// The index's path for `name`.
///
/// The layout is by name length, and it is not a hash: 1 and 2 characters get their own top-level
/// buckets, 3 characters nest under their first letter, and everything else under its first two pairs
/// of letters. Lowercased, because the index is.
///
/// ```text
/// a        → 1/a
/// io       → 2/io
/// syn      → 3/s/syn
/// serde    → se/rd/serde
/// ```
pub fn index_path(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match lower.chars().count() {
        0 => String::new(),
        1 => format!("1/{lower}"),
        2 => format!("2/{lower}"),
        3 => format!("3/{}/{}", &lower[..1], lower),
        _ => format!("{}/{}/{}", &lower[..2], &lower[2..4], lower),
    }
}

/// The full URL for `name` against `base` (normally [`CRATES_IO_INDEX`]).
pub fn index_url(base: &str, name: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), index_path(name))
}

/// Parse an index file: one JSON object per line.
///
/// Tolerant by line rather than all-or-nothing. The index grows fields over time and a body may be
/// truncated by a proxy; one unreadable line must not cost a crate its other forty versions.
pub fn parse_index(body: &str) -> Vec<IndexVersion> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Some(version) = value.get("vers").and_then(|v| v.as_str()) else { continue };
        out.push(IndexVersion {
            version: version.to_string(),
            yanked: value.get("yanked").and_then(|y| y.as_bool()).unwrap_or(false),
            features: value
                .get("features")
                .and_then(|f| f.as_object())
                .map(|f| f.keys().cloned().collect())
                .unwrap_or_default(),
        });
    }
    out
}

/// The newest version worth offering: not yanked, not a pre-release.
///
/// Pre-releases are excluded rather than ranked last. "The latest version of `tokio`" must not be an
/// alpha — a hint that suggested one would be advice to break your build, and someone who wants a
/// pre-release asks for it by name.
pub fn latest_release(versions: &[IndexVersion]) -> Option<&IndexVersion> {
    versions
        .iter()
        .filter(|v| !v.yanked && is_release(&v.version))
        .max_by_key(|v| version_key(&v.version))
}

/// Whether `version` is a release rather than a pre-release (`1.0.0-rc.1`).
///
/// Public because a consumer that lists versions has to mark the pre-releases, and deciding that in a
/// frontend would mean a second semver parser — which is exactly the drift this crate exists to avoid.
pub fn is_release(version: &str) -> bool {
    version_key(version).3 == 1
}

/// Where `name`'s cached index file goes under `dir`.
///
/// The crate name is the file name, which is safe without escaping: crates.io permits only
/// alphanumerics, `-` and `_`. Lowercased so a cache written for `Serde` is found for `serde`.
pub fn cache_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.json", name.to_ascii_lowercase()))
}

/// Whether a cached file exists and is younger than `ttl`.
///
/// By mtime, which is what the write sets. A file whose mtime cannot be read (a filesystem that does
/// not keep one) counts as stale, so the worst case is a fetch rather than a permanently frozen
/// answer.
pub fn is_fresh(path: &Path, ttl: Duration) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    let Ok(modified) = meta.modified() else { return false };
    SystemTime::now().duration_since(modified).map(|age| age < ttl).unwrap_or(false)
    // A future mtime (a clock that moved, a copied tree) makes `duration_since` fail, and that
    // reads as stale — one wasted fetch, versus a cache entry that never expires again.
}

/// Read a cached index file, whatever its age. `None` when there is none.
///
/// Deliberately indifferent to freshness: the caller decides, because the answer to "may I use this
/// old copy" is different before a fetch (no) and after one has failed (yes).
pub fn read_cache(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(cache_path(dir, name)).ok()
}

/// Write `body` as `name`'s cached index file, creating `dir` if needed.
///
/// Best-effort: a cache that cannot be written costs a fetch next time, which is not worth failing a
/// lookup over.
pub fn write_cache(dir: &Path, name: &str, body: &str) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let _ = std::fs::write(cache_path(dir, name), body);
}

/// Whether `requirement` already admits `version` — the question a version hint has to answer.
///
/// **Not** a semver-requirement parser, and deliberately not: this is asked about the requirement a
/// human wrote in a manifest, and the shapes that occur there in practice are a bare version
/// (`"1.0.150"`, which Cargo reads as `^1.0.150`), a caret, a tilde, `*`, and a comparison. What it
/// answers is the narrow question "would a hint be telling me something I do not already have", so it
/// errs towards **silence**: anything it cannot read confidently is treated as satisfied, because a
/// wrong "update available" on a deliberate pin is worse than a missing one.
///
/// The cases it does decide:
///
/// * `*`, any requirement with a comma, and anything starting with `<` or `=` → satisfied (a range or
///   a deliberate pin; not something to nag about).
/// * `^`/`~`/bare, where the leading numbers are readable → compared by the semver-compatible rule
///   the operator implies.
pub fn requirement_admits(requirement: &str, version: &str) -> bool {
    let req = requirement.trim();
    if req.is_empty() || req == "*" || req.contains(',') {
        return true;
    }
    if req.starts_with('<') || req.starts_with('=') {
        return true;
    }
    let (op, rest) = match req.strip_prefix('^') {
        Some(rest) => ('^', rest),
        None => match req.strip_prefix('~') {
            Some(rest) => ('~', rest),
            // `>=1.2` behaves like a floor: a newer version satisfies it, so there is nothing to say.
            None => match req.strip_prefix(">=").or_else(|| req.strip_prefix('>')) {
                Some(_) => return true,
                None => ('^', req),
            },
        },
    };
    let want = version_key(rest.trim());
    let have = version_key(version);
    // A pre-release on either side is not something this comparison can rank meaningfully.
    if want.3 == 0 || have.3 == 0 {
        return true;
    }
    match op {
        // `^1.2.3` admits anything up to (not including) `2.0.0`; `^0.2.3` up to `0.3.0` — the
        // leading non-zero component is the one that must match, which is the rule that makes a
        // `0.x` crate's minor bump a breaking change.
        '^' if want.0 > 0 => have.0 == want.0 && (have.1, have.2) >= (want.1, want.2),
        '^' if want.1 > 0 => have.0 == 0 && have.1 == want.1 && have.2 >= want.2,
        '^' => have.0 == 0 && have.1 == 0 && have.2 == want.2,
        // `~1.2.3` admits `1.2.x` only.
        _ => have.0 == want.0 && have.1 == want.1 && have.2 >= want.2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_path_buckets_by_name_length() {
        assert_eq!(index_path("a"), "1/a");
        assert_eq!(index_path("io"), "2/io");
        assert_eq!(index_path("syn"), "3/s/syn");
        assert_eq!(index_path("serde"), "se/rd/serde");
        assert_eq!(index_path("tracing-subscriber"), "tr/ac/tracing-subscriber");
        // Lowercased, because the index is — a lookup for `Serde` must find `serde`.
        assert_eq!(index_path("Serde"), "se/rd/serde");
        assert_eq!(index_path(""), "");
    }

    #[test]
    fn the_url_survives_a_base_with_a_trailing_slash() {
        assert_eq!(index_url("https://index.crates.io", "syn"), "https://index.crates.io/3/s/syn");
        assert_eq!(index_url("https://index.crates.io/", "syn"), "https://index.crates.io/3/s/syn");
    }

    #[test]
    fn an_index_file_is_parsed_line_by_line_and_a_bad_line_costs_only_itself() {
        let body = concat!(
            r#"{"name":"serde","vers":"1.0.100","yanked":false,"features":{"std":[],"derive":[]}}"#,
            "\n",
            "not json at all\n",
            r#"{"name":"serde","vers":"1.0.101","yanked":true}"#,
            "\n\n",
            // A line with no `vers` describes no version.
            r#"{"name":"serde"}"#,
            "\n",
        );
        let found = parse_index(body);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].version, "1.0.100");
        assert!(!found[0].yanked);
        assert_eq!(found[0].features.len(), 2);
        assert!(found[1].yanked);
    }

    #[test]
    fn the_latest_release_is_neither_yanked_nor_a_pre_release() {
        let versions = parse_index(concat!(
            r#"{"vers":"1.9.0"}"#, "\n",
            // Higher by string, lower by semver — the reason this is not a string sort.
            r#"{"vers":"1.10.0"}"#, "\n",
            r#"{"vers":"1.11.0-beta.1"}"#, "\n",
            r#"{"vers":"1.10.1","yanked":true}"#, "\n",
        ));
        assert_eq!(latest_release(&versions).unwrap().version, "1.10.0");

        // A crate with nothing but pre-releases has no latest RELEASE, and saying so is better than
        // offering an alpha.
        let only_pre = parse_index("{\"vers\":\"0.1.0-alpha.1\"}\n");
        assert!(latest_release(&only_pre).is_none());
    }

    #[test]
    fn freshness_is_by_mtime_and_a_missing_file_is_never_fresh() {
        let dir = std::env::temp_dir().join(format!(
            "bennu-registry-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!is_fresh(&cache_path(&dir, "serde"), Duration::from_secs(60)));
        assert!(read_cache(&dir, "serde").is_none());

        write_cache(&dir, "serde", "{\"vers\":\"1.0.0\"}\n");
        assert_eq!(read_cache(&dir, "serde").as_deref(), Some("{\"vers\":\"1.0.0\"}\n"));
        assert!(is_fresh(&cache_path(&dir, "serde"), Duration::from_secs(60)));
        // A TTL that has already elapsed makes the same file stale — which is what a caller uses to
        // decide to refetch, while still being able to read it if that fetch fails.
        assert!(!is_fresh(&cache_path(&dir, "serde"), Duration::from_nanos(1)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_requirement_is_only_reported_as_outdated_when_it_certainly_is() {
        // The common case: a caret (explicit or implied) whose major matches admits newer patches
        // and minors, and does not admit the next major.
        assert!(requirement_admits("1.0.150", "1.0.219"));
        assert!(requirement_admits("^1.0", "1.9.0"));
        assert!(!requirement_admits("1.0.150", "2.0.0"));
        assert!(!requirement_admits("^1", "2.1.0"));

        // `0.x` — the minor is the breaking component.
        assert!(requirement_admits("0.12.1", "0.12.9"));
        assert!(!requirement_admits("0.12.1", "0.13.0"));
        assert!(!requirement_admits("0.12.1", "1.0.0"));

        // A tilde pins the minor.
        assert!(requirement_admits("~1.2.3", "1.2.9"));
        assert!(!requirement_admits("~1.2.3", "1.3.0"));

        // Deliberate pins and ranges say nothing — the user already decided.
        assert!(requirement_admits("=1.0.0", "2.0.0"));
        assert!(requirement_admits("*", "9.9.9"));
        assert!(requirement_admits(">=1.0", "2.0.0"));
        assert!(requirement_admits(">=1.0, <2.0", "3.0.0"));
        assert!(requirement_admits("", "1.0.0"));

        // Anything with a pre-release on either side is left alone rather than guessed at.
        assert!(requirement_admits("1.0.0-rc.1", "1.0.0"));
        assert!(requirement_admits("1.0.0", "2.0.0-rc.1"));
    }
}
