//! The index, in a shape the frontend can render.
//!
//! [`crate::registry::ExtensionIndex`] is built from paths and enums that are right for
//! resolving a call and wrong for a wire: a `PathBuf` on one platform is not a string on
//! another, and an enum variant carrying three different payloads is a discriminated union
//! the frontend would have to re-derive.
//!
//! So problems arrive **pre-rendered**. The message a user reads about a broken extension is
//! written once, next to the code that decided it was broken — not assembled again in
//! TypeScript from fields that would have to stay in step with the Rust variant forever.

use serde::{Deserialize, Serialize};

use crate::registry::{ExtensionIndex, IndexProblem};

/// One working extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionRow {
    /// The package that provides it.
    pub plugin: String,
    pub interface: String,
    pub version: u32,
    pub id: String,
    /// Absolute path of the module, forward-slashed like every other path the backend reports.
    pub module: String,
}

/// One thing that is wrong, and what to do about it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionProblemRow {
    /// `missing-module` | `conflict` | `unsupported-target`. A stable tag for an icon, since
    /// matching on a rendered sentence is how a UI breaks on a reworded message.
    pub kind: String,
    /// The package at fault, when exactly one is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// Every package involved. One for most problems, several for a conflict.
    pub plugins: Vec<String>,
    /// `interface@version/id`.
    pub key: String,
    /// The whole explanation, already written.
    pub message: String,
}

/// What the Plugin Manager shows.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtensionsReport {
    pub entries: Vec<ExtensionRow>,
    pub problems: Vec<ExtensionProblemRow>,
    /// Whether this build can actually instantiate what it listed.
    ///
    /// Reported rather than assumed: with the runtime feature off the index still resolves —
    /// which is worth showing, because "we found your extension and it is correctly declared"
    /// is different information from "it is running", and a panel that conflated them would
    /// be lying in whichever direction the build happened to be.
    pub runtime_available: bool,
}

impl ExtensionsReport {
    pub fn from_index(index: &ExtensionIndex) -> Self {
        let entries = index
            .all()
            .map(|e| ExtensionRow {
                plugin: e.plugin.clone(),
                interface: e.key.interface.clone(),
                version: e.key.version,
                id: e.key.id.clone(),
                module: e.module.to_string_lossy().replace('\\', "/"),
            })
            .collect();

        let problems = index
            .problems()
            .iter()
            .map(|p| {
                let message = p.to_string();
                match p {
                    IndexProblem::MissingModule { plugin, key, .. } => ExtensionProblemRow {
                        kind: "missing-module".into(),
                        plugin: Some(plugin.clone()),
                        plugins: vec![plugin.clone()],
                        key: key.to_string(),
                        message,
                    },
                    IndexProblem::Conflict { key, plugins } => ExtensionProblemRow {
                        kind: "conflict".into(),
                        plugin: None,
                        plugins: plugins.clone(),
                        key: key.to_string(),
                        message,
                    },
                    IndexProblem::UnsupportedTarget { plugin, key, .. } => ExtensionProblemRow {
                        kind: "unsupported-target".into(),
                        plugin: Some(plugin.clone()),
                        plugins: vec![plugin.clone()],
                        key: key.to_string(),
                        message,
                    },
                }
            })
            .collect();

        Self { entries, problems, runtime_available: cfg!(feature = "runtime") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_index_reports_nothing_wrong() {
        let r = ExtensionsReport::from_index(&ExtensionIndex::default());
        assert!(r.entries.is_empty());
        assert!(r.problems.is_empty());
    }

    #[test]
    fn the_report_says_whether_this_build_can_run_what_it_found() {
        // The two are genuinely different facts, and a panel that showed only one would be
        // wrong in whichever direction the build happened to be.
        let r = ExtensionsReport::from_index(&ExtensionIndex::default());
        assert_eq!(r.runtime_available, cfg!(feature = "runtime"));
    }
}
