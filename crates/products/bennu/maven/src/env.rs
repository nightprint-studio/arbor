//! What an answer about a pom needs to know besides the pom.
//!
//! Every check, completion and hover in this crate needs the same four things, and gathering them
//! separately per feature is how the popup and the underline come to disagree about whether a
//! dependency resolves. So they are gathered once, per file, and handed down.

use std::collections::HashMap;

use crate::catalog::Catalog;
use crate::effective::{Effective, Managed};
use crate::repo::{Coord, LocalRepo};

/// The project context an answer about one `pom.xml` is given.
pub struct PomEnv<'a> {
    /// The local repository — where "does this exist" is answered.
    pub repo: &'a LocalRepo,
    /// Its coordinates, for the questions that have no coordinate yet (completion).
    pub catalog: &'a Catalog,
    /// The reactor: `groupId:artifactId` → the absolute, forward-slashed path of that module's pom.
    ///
    /// Load-bearing for the checks: a sibling module is built from source and is not supposed to be
    /// in anybody's repository. Without this, a multi-module project underlines half of itself.
    pub reactor: &'a HashMap<String, String>,
    /// This pom with its parents folded in — the properties a `${…}` expands against and the
    /// `<dependencyManagement>` that supplies a version the dependency does not write.
    pub effective: &'a Effective,
    /// This file's own path, forward-slashed.
    pub path: &'a str,
}

impl PomEnv<'_> {
    /// The directory the pom sits in — what a `<module>` and a `<relativePath>` resolve against.
    pub fn dir(&self) -> &str {
        self.path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
    }

    /// `${…}` expanded against everything in scope.
    pub fn expand(&self, text: &str) -> String {
        crate::effective::expand(text, &self.effective.properties)
    }

    /// The coordinate a block writes, expanded and with its managed version applied when it writes
    /// none — which is the version Maven will actually use, and therefore the one to check.
    pub fn resolve_coord(&self, raw: &Coord) -> Coord {
        let mut coord = Coord {
            group_id: self.expand(&raw.group_id),
            artifact_id: self.expand(&raw.artifact_id),
            version: self.expand(&raw.version),
            classifier: self.expand(&raw.classifier),
            packaging: self.expand(&raw.packaging),
        };
        if coord.version.is_empty() {
            if let Some(pin) = self.managed(&coord) {
                coord.version = pin.version.clone();
            }
        }
        coord
    }

    /// What `<dependencyManagement>` says about this coordinate, if anything.
    pub fn managed(&self, coord: &Coord) -> Option<&Managed> {
        self.effective.managed.get(&coord.key())
    }

    /// Whether this artifact is one of the project's own modules — built from source, never looked
    /// for in a repository.
    pub fn is_reactor(&self, coord: &Coord) -> bool {
        self.reactor.contains_key(&coord.ga())
    }

    /// The pom of a reactor module, for the jump.
    pub fn reactor_pom(&self, coord: &Coord) -> Option<&str> {
        self.reactor.get(&coord.ga()).map(String::as_str)
    }

    /// Whether the repository is usable at all.
    ///
    /// The gate in front of **every** existence check: on a machine whose `~/.m2` has not been
    /// populated, every dependency of every project is "missing", and a pom painted entirely red
    /// says nothing except that the feature should be turned off. No repository, no claims.
    pub fn repo_is_usable(&self) -> bool {
        self.repo.exists() && !self.catalog.is_empty()
    }
}

/// Property names that come from outside the pom, and are therefore never "undefined".
///
/// `${env.PATH}`, `${settings.localRepository}`, `${java.version}`, `${basedir}` — all supplied by
/// Maven or the JVM at build time. Reporting them is the fastest way to make the property check
/// worthless, because a legacy pom is full of them.
pub const EXTERNAL_PROPERTY_PREFIXES: &[&str] = &[
    "env.", "settings.", "user.", "java.", "os.", "line.", "file.", "sun.", "maven.", "project.",
    "pom.", "basedir", "session.", "mojoExecution.", "reporting.",
    // Maven's own CI-friendly versions: a pom is *expected* to leave these undefined and have them
    // supplied on the command line (`-Drevision=1.2.3`), which is the whole point of the feature.
    "revision", "sha1", "changelist",
];

/// Whether a `${…}` name is one the pom is expected to define itself.
pub fn is_own_property(name: &str) -> bool {
    !EXTERNAL_PROPERTY_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Every `${name}` in a string, with the span of each relative to the string's start.
pub fn property_references(text: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(start) = text[at..].find("${") {
        let start = at + start;
        let Some(len) = text[start..].find('}') else { break };
        out.push((text[start + 2..start + len].to_string(), start, start + len + 1));
        at = start + len + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_placeholder_is_found_with_its_span() {
        let refs = property_references("${a}-${b.c}");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].0, "a");
        assert_eq!((refs[1].1, refs[1].2), (5, 11));
    }

    /// A pom is not expected to define `${env.HOME}`. Reporting it would make the check unusable
    /// on exactly the legacy poms it is for.
    #[test]
    fn a_build_supplied_property_is_not_the_poms_to_define() {
        assert!(!is_own_property("env.JAVA_HOME"));
        assert!(!is_own_property("project.version"));
        assert!(is_own_property("spring.version"));
    }
}
