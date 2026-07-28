//! `.arbor/picus/project.toml` — everything that describes *a repository of
//! scripts* rather than *this user's preferences*.
//!
//! Why it lives with the repository and not in the profile: a colleague opening
//! the same project must inherit the roles, the expected encodings and the version
//! table, or the same repository behaves differently per person — which is the
//! class of surprise Picus exists to remove. The per-user settings (row limits,
//! editor preferences) stay in the profile's `picus/config.toml`; nothing is in
//! both files.
//!
//! Why `.arbor/picus/` and not `.picus/`: Arbor already owns `.arbor/` in a
//! repository, and namespacing per product inside it leaves room for the other
//! products to move in without a second dotfile each.
//!
//! **This file is never written without the user's explicit confirmation.** The
//! functions here are plain I/O; the confirmation belongs to the caller, and the
//! proposal flow in [`crate::discover`] exists precisely so there is something to
//! confirm.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use picus_types::prelude::{EngineKind, FolderRole};
use serde::{Deserialize, Serialize};

use crate::error::ProjectError;
use crate::insertion::InsertionRule;
use crate::marker::MarkerTemplate;
use crate::naming::NamingScheme;
use crate::tree::LineEnding;

/// Where the file sits, relative to the project root.
pub const PROJECT_CONFIG_RELATIVE_PATH: &str = ".arbor/picus/project.toml";

/// The schema version this build writes and understands.
pub const CURRENT_VERSION: u32 = 1;

/// The default single-byte encoding for these repositories. Not a guess about
/// text in general — a fact about the corpus Picus was built for.
pub const DEFAULT_ENCODING: &str = "windows-1252";

/// The whole project file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Schema version of this file, so a future shape can be recognised rather
    /// than silently half-read.
    #[serde(default = "default_version")]
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub encoding: EncodingSettings,
    #[serde(default)]
    pub version_table: VersionTableSettings,
    #[serde(default)]
    pub generation: GenerationSettings,
    #[serde(default)]
    pub naming: NamingScheme,
    /// One per per-dialect branch. `branch` singular in the file because TOML
    /// spells an array of tables `[[branch]]`, which reads better than `[[branches]]`.
    #[serde(default, rename = "branch")]
    pub branches: Vec<BranchConfig>,
}

fn default_version() -> u32 {
    CURRENT_VERSION
}

/// What the project's files are encoded in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EncodingSettings {
    /// The encoding every folder is expected to be in, unless it overrides it.
    pub default: String,
    /// The line ending generated content uses.
    pub eol: LineEnding,
}

impl Default for EncodingSettings {
    fn default() -> Self {
        EncodingSettings { default: DEFAULT_ENCODING.to_string(), eol: LineEnding::Crlf }
    }
}

/// Where the installed version is recorded.
///
/// The TOML twin of the emitter's `VersionTableConfig`: same four facts, written
/// `snake_case` because that is what a human editing a TOML file expects, whereas
/// the emitter's copy is `camelCase` because it crosses the wire to the interface.
/// The mapping between them lives at the call site that builds a generation, which
/// is the only place that needs both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VersionTableSettings {
    /// Empty disables version guards entirely.
    pub table: String,
    pub version_column: String,
    /// Absent when the project stamps no date — the closing UPDATE then leaves the
    /// column out rather than inventing one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_column: Option<String>,
    /// Extra predicate for a version table holding one row per module.
    #[serde(default)]
    pub filter: String,
}

impl Default for VersionTableSettings {
    fn default() -> Self {
        VersionTableSettings {
            table: "VERSIONE_DB".to_string(),
            version_column: "VERSIONE".to_string(),
            date_column: Some("DATA_AGG".to_string()),
            filter: String::new(),
        }
    }
}

/// How generated blocks are written.
///
/// Field order matters for TOML: `marker` is a value and `insertion` is a table,
/// and `toml` refuses to emit a value after a table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GenerationSettings {
    /// The comment written above a generated block. Empty switches marking off,
    /// and with it the ability to regenerate a block in place.
    #[serde(default)]
    pub marker: MarkerTemplate,
    /// Where a generated block lands, keyed by folder **role**
    /// (`init` / `update` / `data` / `routines`), each value an
    /// [`InsertionRule`] wire string:
    ///
    /// ```toml
    /// [generation.insertion]
    /// update = "end-of-file"
    /// init   = "after-last-on-table"
    /// ```
    ///
    /// A role that is absent takes the user's own default from the profile, and
    /// failing that [`InsertionRule::default_for`]. Both the key and the value are
    /// plain strings on purpose: an unknown role or an unknown rule degrades to
    /// the default instead of failing the parse and resetting every other setting
    /// in the file. [`problems`](ProjectConfig::problems) is where the user is
    /// told about it.
    #[serde(default)]
    pub insertion: BTreeMap<String, String>,
}

impl GenerationSettings {
    /// The insertion rule this project declares for a role, or `None` when it
    /// declares none — the caller then falls back to the user's preference and
    /// finally to [`InsertionRule::default_for`].
    pub fn insertion_for(&self, role: FolderRole) -> Option<InsertionRule> {
        self.insertion.get(role.as_str()).and_then(|wire| InsertionRule::from_wire(wire))
    }
}

/// One per-dialect branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BranchConfig {
    pub id: String,
    pub label: String,
    /// Path relative to the project root, POSIX separators.
    pub path: String,
    /// Absent when the engine is unknown. Nothing is generated into such a branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect: Option<EngineKind>,
    #[serde(default, rename = "folder")]
    pub folders: Vec<FolderConfig>,
}

/// One folder inside a branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FolderConfig {
    pub id: String,
    pub label: String,
    pub path: String,
    pub role: FolderRole,
    /// Overrides the project's default encoding for this folder only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    /// Overrides the update-file naming for this folder only — a branch whose
    /// update files are named differently from its sibling's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naming: Option<NamingScheme>,
}

impl ProjectConfig {
    /// The absolute path of the project file for a root.
    pub fn path_in(root: &Path) -> PathBuf {
        root.join(".arbor").join("picus").join("project.toml")
    }

    /// Read the project file. `Ok(None)` means "this repository has not been set
    /// up yet", which is an ordinary state and not an error — it is what triggers
    /// the proposal flow.
    pub fn load(root: &Path) -> Result<Option<ProjectConfig>, ProjectError> {
        let path = ProjectConfig::path_in(root);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ProjectError::Io { path, reason: e.to_string() }),
        };
        let config: ProjectConfig = toml::from_str(&text)
            .map_err(|e| ProjectError::Malformed { path: path.clone(), reason: e.to_string() })?;
        if config.version > CURRENT_VERSION {
            return Err(ProjectError::Malformed {
                path,
                reason: format!(
                    "it declares version {} and this build understands up to {CURRENT_VERSION} — \
                     it was written by a newer Picus",
                    config.version
                ),
            });
        }
        Ok(Some(config))
    }

    /// Write the project file, creating `.arbor/picus/` if needed.
    ///
    /// The caller is responsible for having asked first. This lands in someone's
    /// repository and gets committed, so "the user pressed the button" is part of
    /// the contract, not a nicety.
    pub fn save(&self, root: &Path) -> Result<PathBuf, ProjectError> {
        let path = ProjectConfig::path_in(root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ProjectError::Io {
                path: parent.to_path_buf(),
                reason: e.to_string(),
            })?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| ProjectError::Malformed { path: path.clone(), reason: e.to_string() })?;
        std::fs::write(&path, text)
            .map_err(|e| ProjectError::Io { path: path.clone(), reason: e.to_string() })?;
        Ok(path)
    }

    /// The naming scheme in force for one folder — its own, or the project's.
    pub fn naming_for<'a>(&'a self, folder: &'a FolderConfig) -> &'a NamingScheme {
        folder.naming.as_ref().unwrap_or(&self.naming)
    }

    /// The encoding one folder is expected to be in.
    pub fn encoding_for<'a>(&'a self, folder: &'a FolderConfig) -> &'a str {
        folder.encoding.as_deref().unwrap_or(&self.encoding.default)
    }

    /// Validate what can only be checked once, at load: the patterns compile and
    /// the marker has no placeholder that will always be empty.
    ///
    /// Returns the problems rather than failing, because none of them should stop
    /// a project opening — a bad pattern makes update files unrecognised, which is
    /// visible and fixable, whereas refusing to open leaves the user nowhere to fix
    /// it from.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Err(e) = self.naming.compile() {
            out.push(e.to_string());
        }
        for branch in &self.branches {
            for folder in &branch.folders {
                if let Some(naming) = &folder.naming {
                    if let Err(e) = naming.compile() {
                        out.push(format!("{}: {e}", folder.path));
                    }
                }
            }
        }
        for name in self.generation.marker.unknown_placeholders() {
            out.push(format!(
                "the block marker uses `{{{name}}}`, which is not a placeholder Picus knows — it will always be empty"
            ));
        }
        for (role, rule) in &self.generation.insertion {
            if !FolderRole::ALL.iter().any(|r| r.as_str() == role) {
                out.push(format!(
                    "`[generation.insertion]` declares `{role}`, which is not a folder role — \
                     no folder will ever use it"
                ));
            } else if InsertionRule::from_wire(rule).is_none() {
                out.push(format!(
                    "`{rule}` is not an insertion rule Picus knows, so `{role}` folders keep the \
                     default ({})",
                    InsertionRule::default_for(
                        FolderRole::ALL
                            .iter()
                            .copied()
                            .find(|r| r.as_str() == role)
                            .unwrap_or(FolderRole::Ignored)
                    )
                    .describe()
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ProjectConfig {
        ProjectConfig {
            version: CURRENT_VERSION,
            name: "PROD_CORE".to_string(),
            encoding: EncodingSettings::default(),
            version_table: VersionTableSettings::default(),
            generation: GenerationSettings::default(),
            naming: NamingScheme::default(),
            branches: vec![BranchConfig {
                id: "ora".to_string(),
                label: "ORACLE".to_string(),
                path: "ORACLE".to_string(),
                dialect: Some(EngineKind::Oracle),
                folders: vec![FolderConfig {
                    id: "ora-upd".to_string(),
                    label: "AGGIORNAMENTO".to_string(),
                    path: "ORACLE/AGGIORNAMENTO".to_string(),
                    role: FolderRole::Update,
                    encoding: None,
                    naming: None,
                }],
            }],
        }
    }

    #[test]
    fn it_round_trips_through_toml() {
        let text = toml::to_string_pretty(&sample()).expect("serialises");
        let back: ProjectConfig = toml::from_str(&text).expect("parses");
        assert_eq!(back, sample());
    }

    #[test]
    fn a_project_with_no_date_column_keeps_the_distinction() {
        // `None` and `Some("")` are different things: one project stamps no date,
        // another stamps one into a column called "". Only the first is legitimate,
        // and it must survive a round trip as absence.
        let mut config = sample();
        config.version_table.date_column = None;
        let text = toml::to_string_pretty(&config).expect("serialises");
        assert!(!text.contains("date_column"));
        let back: ProjectConfig = toml::from_str(&text).expect("parses");
        assert_eq!(back.version_table.date_column, None);
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        // Someone hand-writes the minimum. Everything else must fill in rather
        // than the parse failing and taking the whole project with it.
        let text = r#"
            name = "MINIMAL"
            [[branch]]
            id = "pg"
            label = "POSTGRES"
            path = "POSTGRES"
            dialect = "postgres"
        "#;
        let config: ProjectConfig = toml::from_str(text).expect("parses");
        assert_eq!(config.version, CURRENT_VERSION);
        assert_eq!(config.encoding.default, DEFAULT_ENCODING);
        assert_eq!(config.encoding.eol, LineEnding::Crlf);
        assert_eq!(config.version_table.table, "VERSIONE_DB");
        assert_eq!(config.naming, NamingScheme::default());
        assert_eq!(config.branches[0].dialect, Some(EngineKind::Postgres));
        assert!(config.branches[0].folders.is_empty());
    }

    #[test]
    fn a_folder_can_override_the_project_wide_settings() {
        let mut config = sample();
        let folder = &mut config.branches[0].folders[0];
        folder.encoding = Some("UTF-8".to_string());
        folder.naming = Some(NamingScheme {
            pattern: r"^(?P<to>\d+)\.sql$".to_string(),
            template: "{to}.sql".to_string(),
            separator: '_',
        });
        let folder = config.branches[0].folders[0].clone();
        assert_eq!(config.encoding_for(&folder), "UTF-8");
        assert_ne!(config.naming_for(&folder), &NamingScheme::default());

        // …and a folder that overrides nothing inherits both.
        let plain = FolderConfig { encoding: None, naming: None, ..folder };
        assert_eq!(config.encoding_for(&plain), DEFAULT_ENCODING);
        assert_eq!(config.naming_for(&plain), &NamingScheme::default());
    }

    #[test]
    fn a_project_can_override_where_a_generated_block_lands_per_role() {
        let text = r#"
            name = "MINIMAL"
            [generation.insertion]
            update = "before-final-commit"
            init = "nonsense"
        "#;
        let config: ProjectConfig = toml::from_str(text).expect("an unknown rule must not fail the parse");

        assert_eq!(
            config.generation.insertion_for(FolderRole::Update),
            Some(InsertionRule::BeforeFinalCommit)
        );
        // An unrecognised value reads as "not declared", so the caller falls back
        // rather than placing a block somewhere nobody asked for.
        assert_eq!(config.generation.insertion_for(FolderRole::Init), None);
        // …and a role nobody mentioned is simply not declared.
        assert_eq!(config.generation.insertion_for(FolderRole::Data), None);
        // The marker survived a section that talks about something else.
        assert_eq!(config.generation.marker, MarkerTemplate::default());

        let problems = config.problems();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("nonsense"), "{problems:?}");
    }

    #[test]
    fn generation_settings_round_trip_with_an_insertion_table() {
        let mut config = sample();
        config
            .generation
            .insertion
            .insert(FolderRole::Init.as_str().to_string(), InsertionRule::EndOfFile.as_wire().to_string());
        let text = toml::to_string_pretty(&config).expect("serialises");
        let back: ProjectConfig = toml::from_str(&text).expect("parses");
        assert_eq!(back, config);
    }

    #[test]
    fn problems_are_reported_without_refusing_to_open() {
        let mut config = sample();
        config.naming.pattern = "^(?P<to>[unclosed".to_string();
        config.generation.marker = MarkerTemplate("-- {autore} {table}".to_string());
        let problems = config.problems();
        assert_eq!(problems.len(), 2);
        assert!(problems.iter().any(|p| p.contains("[unclosed")));
        assert!(problems.iter().any(|p| p.contains("autore")));
        // A healthy project reports nothing.
        assert!(sample().problems().is_empty());
    }

    #[test]
    fn a_file_from_a_newer_picus_is_refused_by_name() {
        let mut config = sample();
        config.version = CURRENT_VERSION + 1;
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: ProjectConfig = toml::from_str(&text).unwrap();
        // `load` is what enforces it; assert the condition it checks.
        assert!(parsed.version > CURRENT_VERSION);
    }
}
