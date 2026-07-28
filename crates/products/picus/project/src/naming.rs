//! [`NamingScheme`] — how a project names its update files, and what the next one
//! should be called.
//!
//! There is a default and it is versioned (`4_12__4_13.sql`), because that is what
//! these repositories overwhelmingly use and a default nobody has to configure is
//! worth a great deal. But it is only a default: a project whose files are named
//! some other way declares its own **regex**, and nothing else in Picus changes.
//!
//! Two named capture groups are recognised: `from` and `to`. `to` is the one that
//! matters — it is what "the highest version on disk" means. `from` is optional,
//! and a scheme without it simply cannot support the unbroken-chain check
//! (`VER003`), which is reported as a skipped rule rather than silently passing.

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::ProjectError;
use crate::version::Version;

/// The default pattern: `4_12__4_13.sql`, with either separator inside a version.
const DEFAULT_PATTERN: &str =
    r"(?i)^(?P<from>\d+(?:[._-]\d+)*)__(?P<to>\d+(?:[._-]\d+)*)\.sql$";
/// The default template, in the same shape the default pattern reads.
const DEFAULT_TEMPLATE: &str = "{from}__{to}.sql";

/// How update files are named in one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NamingScheme {
    /// Regex with named groups `from` (optional) and `to`, matched against the
    /// file **name**, not its path.
    pub pattern: String,
    /// Filename template. `{from}` and `{to}` are substituted with the version
    /// rendered using `separator`.
    pub template: String,
    /// How a version's segments are written inside a filename. `4_12`, not `4.12`,
    /// on every filesystem anyone has ever had to zip up and email.
    pub separator: char,
}

impl Default for NamingScheme {
    fn default() -> Self {
        NamingScheme {
            pattern: DEFAULT_PATTERN.to_string(),
            template: DEFAULT_TEMPLATE.to_string(),
            separator: '_',
        }
    }
}

/// The version transition one update file performs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRange {
    /// `None` when the scheme carries no starting version (a dated scheme, or one
    /// that names only its destination).
    pub from: Option<Version>,
    pub to: Version,
}

/// A [`NamingScheme`] with its regex already compiled.
///
/// Compiling is the expensive part and a project scan asks the same question of
/// hundreds of filenames, so the compile happens once and the caller holds the
/// result. It also means an invalid pattern is reported **once, at load**, rather
/// than once per file.
#[derive(Debug, Clone)]
pub struct CompiledNaming {
    scheme: NamingScheme,
    regex: Regex,
    has_from: bool,
}

impl NamingScheme {
    /// Compile the pattern, failing with the user's own pattern in the message.
    pub fn compile(&self) -> Result<CompiledNaming, ProjectError> {
        let regex = Regex::new(&self.pattern)
            .map_err(|e| ProjectError::NamingPattern { pattern: self.pattern.clone(), reason: e.to_string() })?;
        let has_from = regex.capture_names().flatten().any(|n| n == "from");
        if !regex.capture_names().flatten().any(|n| n == "to") {
            return Err(ProjectError::NamingPattern {
                pattern: self.pattern.clone(),
                reason: "the pattern has no (?P<to>…) group, so there is no way to tell which version a file installs".to_string(),
            });
        }
        Ok(CompiledNaming { scheme: self.clone(), regex, has_from })
    }
}

impl CompiledNaming {
    /// Does this scheme carry a starting version? `VER003` depends on it.
    pub fn tracks_starting_version(&self) -> bool {
        self.has_from
    }

    /// Read one filename. `None` means "this file is not an update script under
    /// this scheme" — a perfectly ordinary answer for a README or a rollback file.
    pub fn parse(&self, file_name: &str) -> Option<VersionRange> {
        let caps = self.regex.captures(file_name)?;
        let to = Version::parse(caps.name("to")?.as_str())?;
        let from = caps.name("from").and_then(|m| Version::parse(m.as_str()));
        Some(VersionRange { from, to })
    }

    /// The filename for a transition.
    pub fn render(&self, range: &VersionRange) -> String {
        let sep = self.scheme.separator;
        let from = range.from.as_ref().map(|v| v.render(sep)).unwrap_or_default();
        self.scheme
            .template
            .replace("{from}", &from)
            .replace("{to}", &range.to.render(sep))
    }

    /// Propose the next update file from what is already on disk: the highest `to`
    /// becomes the new `from`, and its bump becomes the new `to`.
    ///
    /// `None` when no file in the folder matches the scheme — an empty update
    /// folder, or one whose scheme is wrong. Both cases want the user to type the
    /// versions rather than have Picus invent a first one.
    pub fn propose_next<'a, I>(&self, file_names: I) -> Option<VersionRange>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let highest = file_names
            .into_iter()
            .filter_map(|name| self.parse(name))
            .map(|range| range.to)
            .max()?;
        Some(VersionRange { to: highest.bump(), from: Some(highest) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_naming() -> CompiledNaming {
        NamingScheme::default().compile().expect("the default pattern compiles")
    }

    #[test]
    fn the_default_scheme_reads_and_writes_the_same_shape() {
        let n = default_naming();
        let parsed = n.parse("4_12__4_13.sql").expect("matches");
        assert_eq!(parsed.from, Version::parse("4.12"));
        assert_eq!(parsed.to, Version::parse("4.13").unwrap());
        assert_eq!(n.render(&parsed), "4_12__4_13.sql");
    }

    #[test]
    fn files_that_are_not_update_scripts_simply_do_not_match() {
        let n = default_naming();
        assert!(n.parse("README.md").is_none());
        assert!(n.parse("01_TABELLE.sql").is_none());
        assert!(n.parse("rollback_4_13.sql").is_none());
    }

    #[test]
    fn the_proposal_reads_the_highest_version_not_the_last_line() {
        // 4_9 must not beat 4_12: this is the bug a string sort would produce.
        let n = default_naming();
        let next = n
            .propose_next(["4_11__4_12.sql", "4_12__4_9.sql", "4_9__4_11.sql", "notes.txt"])
            .expect("a proposal");
        assert_eq!(next.from, Version::parse("4.12"));
        assert_eq!(next.to, Version::parse("4.13").unwrap());
        assert_eq!(n.render(&next), "4_12__4_13.sql");
    }

    #[test]
    fn an_empty_folder_gets_no_invented_first_version() {
        let n = default_naming();
        assert!(n.propose_next(["README.md"]).is_none());
        assert!(n.propose_next([]).is_none());
    }

    #[test]
    fn a_project_can_declare_its_own_shape() {
        // A dated scheme: no starting version, so VER003 has to stand down.
        let scheme = NamingScheme {
            pattern: r"(?i)^V(?P<to>\d+(?:_\d+)*)__.+\.sql$".to_string(),
            template: "V{to}__change.sql".to_string(),
            separator: '_',
        };
        let n = scheme.compile().expect("compiles");
        assert!(!n.tracks_starting_version());

        let parsed = n.parse("V4_13__add_discount_threshold.sql").expect("matches");
        assert_eq!(parsed.from, None);
        assert_eq!(parsed.to, Version::parse("4.13").unwrap());

        let next = n.propose_next(["V4_12__x.sql", "V4_13__y.sql"]).expect("a proposal");
        assert_eq!(next.to, Version::parse("4.14").unwrap());
        assert_eq!(n.render(&next), "V4_14__change.sql");
    }

    #[test]
    fn a_pattern_without_a_destination_version_is_refused_at_load() {
        let scheme = NamingScheme { pattern: r"^(?P<from>\d+)\.sql$".to_string(), ..Default::default() };
        let err = scheme.compile().expect_err("no `to` group");
        assert!(err.to_string().contains("to"));
    }

    #[test]
    fn an_invalid_pattern_reports_the_users_own_pattern() {
        let scheme = NamingScheme { pattern: "^(?P<to>[unclosed".to_string(), ..Default::default() };
        let err = scheme.compile().expect_err("invalid regex");
        assert!(err.to_string().contains("[unclosed"));
    }
}
