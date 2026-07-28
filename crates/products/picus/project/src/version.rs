//! [`Version`] — an application version as it appears in a filename, in a version
//! table, and in a guard.
//!
//! Deliberately **not** semver. These are the version strings real installation
//! repositories use — `4.12`, `4_12_3`, `10.0` — where the only operations that
//! matter are "is this one after that one" and "what comes next". Reaching for a
//! semver crate would impose a three-segment shape that half of these projects do
//! not have.
//!
//! The separator is not part of the value: `4.12`, `4_12` and `4-12` are the same
//! version written for three different places (a version table, a filename, a
//! branch name). Parsing normalises; rendering asks for the separator it wants.

use std::cmp::Ordering;
use std::fmt;

/// A dotted/underscored numeric version, normalised to its segments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version(Vec<u32>);

impl Version {
    /// Parse `4.12`, `4_12_3`, `4-12`. Returns `None` for anything that is not a
    /// non-empty run of numeric segments — a version we cannot order is worse
    /// than no version at all, because ordering is the whole point.
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let segments: Option<Vec<u32>> = text
            .split(['.', '_', '-'])
            .map(|s| if s.is_empty() { None } else { s.parse::<u32>().ok() })
            .collect();
        segments.map(Version)
    }

    /// The numeric segments, most significant first.
    pub fn segments(&self) -> &[u32] {
        &self.0
    }

    /// Render with an explicit separator — `'.'` for a version table, `'_'` for a
    /// filename. There is no default: the caller always knows which it wants, and
    /// picking one here is how the wrong one ends up in a filename.
    pub fn render(&self, separator: char) -> String {
        self.0
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(&separator.to_string())
    }

    /// The next version: increment the last segment.
    ///
    /// A proposal, never a decision — `4.12` → `4.13` is right far more often
    /// than not, and the one time it is a major bump the user retypes it. Picus
    /// does not know what the change *means*, so it must not pretend to.
    pub fn bump(&self) -> Version {
        let mut next = self.0.clone();
        if let Some(last) = next.last_mut() {
            *last = last.saturating_add(1);
        }
        Version(next)
    }
}

impl Ord for Version {
    /// Segment-wise, with a missing segment counting as zero — so `4.12` and
    /// `4.12.0` compare equal in order while remaining distinct values, which is
    /// what a repository mixing the two forms needs.
    fn cmp(&self, other: &Self) -> Ordering {
        let len = self.0.len().max(other.0.len());
        for i in 0..len {
            let a = self.0.get(i).copied().unwrap_or(0);
            let b = other.0.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                Ordering::Equal => continue,
                other => return other,
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    /// Dotted — the form a version table holds and a human reads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render('.'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).expect("valid version")
    }

    #[test]
    fn the_separator_is_not_part_of_the_value() {
        assert_eq!(v("4.12"), v("4_12"));
        assert_eq!(v("4.12"), v("4-12"));
        assert_eq!(v("4.12").render('_'), "4_12");
        assert_eq!(v("4.12").to_string(), "4.12");
    }

    #[test]
    fn anything_non_numeric_is_refused() {
        assert!(Version::parse("").is_none());
        assert!(Version::parse("4.x").is_none());
        assert!(Version::parse("v4.12").is_none());
        assert!(Version::parse("4..12").is_none());
        assert!(Version::parse("4.").is_none());
    }

    #[test]
    fn ordering_is_numeric_not_lexicographic() {
        // The bug this prevents: sorting filenames as strings puts 4.9 after 4.12,
        // so "the highest version on disk" reads the wrong file.
        assert!(v("4.9") < v("4.12"));
        assert!(v("4.12") < v("4.12.1"));
        assert!(v("10.0") > v("9.99"));
    }

    #[test]
    fn a_missing_segment_counts_as_zero() {
        assert_eq!(v("4.12").cmp(&v("4.12.0")), Ordering::Equal);
        assert!(v("4.12") < v("4.12.1"));
    }

    #[test]
    fn bump_moves_the_last_segment_only() {
        assert_eq!(v("4.12").bump(), v("4.13"));
        assert_eq!(v("4.12.9").bump(), v("4.12.10"));
        assert_eq!(v("7").bump(), v("8"));
    }
}
