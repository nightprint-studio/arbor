//! `project` domain — opening a repository of scripts and agreeing on what it is.
//!
//! The flow this serves has one shape, and the shape is the point:
//!
//! 1. `picus_open_project` reads the folder and **proposes** what it thinks the
//!    repository is, with a note for everything it could not work out.
//! 2. The user looks, corrects the branches whose engine was not obvious and the
//!    folders whose purpose was not, and confirms.
//! 3. `picus_confirm_project` applies those corrections and writes
//!    `.arbor/picus/project.toml`.
//!
//! **Nothing writes before step 3.** That file lands in someone's repository and
//! gets committed, so "the user pressed the button" is part of the contract rather
//! than a nicety — the same rule as everywhere else in Arbor, but with more at
//! stake than usual.
//!
//! Note what does *not* cross this seam: the `ProjectConfig` itself. Its shape is
//! `snake_case` because a human edits it in a TOML file, while everything the
//! interface receives is `camelCase`. Rather than leak one convention into the
//! other, the corrections travel as a small list of edits and the backend remains
//! the only thing that has ever seen the file's shape.

use std::path::{Path, PathBuf};

use picus_core::prelude::PicusState;
use picus_project::prelude::{
    discover, EngineKind, FolderRole, Project, ProjectConfig, ProposalNote,
};
use serde::{Deserialize, Serialize};

/// What `picus_open_project` answers.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedProject {
    pub project: Project,
    pub notes: Vec<ProposalNote>,
    /// `true` when there is no `project.toml` yet, i.e. this is a proposal
    /// awaiting confirmation rather than a project already agreed on.
    pub is_new: bool,
    /// Problems with an existing configuration — an update-file pattern that will
    /// not compile, a marker placeholder that will always be empty. Reported, not
    /// fatal: refusing to open would leave the user nowhere to fix it from.
    pub problems: Vec<String>,
}

/// One correction the user made to the proposal.
///
/// Keyed by path rather than by id because a path is what the user sees and what
/// survives a rescan; ids are derived and would silently stop matching if the
/// slug rule ever changed.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEdit {
    /// Project-relative path of the branch or folder being corrected.
    pub path: String,
    /// New role for a folder.
    #[serde(default)]
    pub role: Option<FolderRole>,
    /// New engine for a branch. Explicitly `null` clears it back to "unknown",
    /// which is a legitimate thing to want — an unset branch is simply one nothing
    /// is generated into.
    ///
    /// `deserialize_with` is load-bearing, not decoration: a plain
    /// `Option<Option<T>>` with `#[serde(default)]` collapses an explicit `null`
    /// into the same `None` as an absent field, and the two mean opposite things
    /// here ("clear it" vs "leave it alone"). The same distinction the connection
    /// password already needed.
    #[serde(default, deserialize_with = "explicit_null")]
    pub dialect: Option<Option<EngineKind>>,
    /// New display label.
    #[serde(default)]
    pub label: Option<String>,
}

/// Deserialise a field that is meaningfully three-valued: absent, `null`, or set.
///
/// Serde's own handling of `Option<Option<T>>` cannot express it — the derive maps
/// both "absent" and "null" to `None` — so the outer `Some` has to be added here,
/// where the field being present is the only thing this function is called for.
fn explicit_null<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// What `picus_confirm_project` answers.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedProject {
    /// Absolute path of the file that was written — shown to the user, because a
    /// tool that writes into your repository should say where.
    pub config_path: String,
    /// The tree as it stands after the corrections.
    pub project: Project,
}

/// Read a repository and say what it looks like. Writes nothing.
#[arbor_rpc::handler]
fn picus_open_project(_state: &PicusState, root: String) -> Result<OpenedProject, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    Ok(OpenedProject {
        problems: proposal.config.problems(),
        project: proposal.project,
        notes: proposal.notes,
        is_new: proposal.is_new,
    })
}

/// Apply the user's corrections and write `.arbor/picus/project.toml`.
///
/// Discovery is re-run rather than trusting a client-held snapshot: between the
/// proposal and the confirmation the folder may have changed, and writing a
/// configuration that describes a tree which no longer exists would be worse than
/// asking again.
#[arbor_rpc::handler]
fn picus_confirm_project(
    _state: &PicusState,
    root: String,
    edits: Vec<ProjectEdit>,
) -> Result<ConfirmedProject, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    let mut config = proposal.config;
    apply_edits(&mut config, &edits);

    let path = config.save(&root).map_err(|e| e.to_string())?;

    // Re-plan against the saved configuration so the caller receives the tree as
    // it will look from now on — the roles it just chose, not the ones inferred.
    let confirmed = discover(&root).map_err(|e| e.to_string())?;
    Ok(ConfirmedProject {
        config_path: path.display().to_string(),
        project: confirmed.project,
    })
}

/// The name the next update file in a folder should have, under that folder's
/// naming scheme.
///
/// `Ok(None)` when the folder holds no file the scheme recognises: rather than
/// invent a first version, the user is asked. Inventing one is how a repository
/// ends up with a `1_0__1_1.sql` sitting next to `4_12__4_13.sql`.
#[arbor_rpc::handler]
fn picus_propose_update_file(
    _state: &PicusState,
    root: String,
    folder_path: String,
) -> Result<Option<ProposedUpdateFile>, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    let config = proposal.config;

    let folder = config
        .branches
        .iter()
        .flat_map(|b| b.folders.iter())
        .find(|f| f.path == folder_path)
        .ok_or_else(|| format!("{folder_path} is not a folder of this project"))?;

    let naming = config.naming_for(folder).compile().map_err(|e| e.to_string())?;

    let existing: Vec<&str> = proposal
        .project
        .branches
        .iter()
        .flat_map(|b| b.folders.iter())
        .filter(|f| f.path == folder_path)
        .flat_map(|f| f.files.iter().map(|file| file.name.as_str()))
        .collect();

    Ok(naming.propose_next(existing).map(|range| ProposedUpdateFile {
        file_name: naming.render(&range),
        from_version: range.from.map(|v| v.to_string()),
        to_version: range.to.to_string(),
    }))
}

/// The proposal for a new update file.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedUpdateFile {
    pub file_name: String,
    /// Absent when the project's scheme carries no starting version.
    pub from_version: Option<String>,
    pub to_version: String,
}

/// Apply the corrections in place. Unknown paths are ignored rather than refused:
/// a stale edit from a tree that has since changed should not cost the user every
/// other correction they made in the same pass.
fn apply_edits(config: &mut ProjectConfig, edits: &[ProjectEdit]) {
    for edit in edits {
        for branch in &mut config.branches {
            if branch.path == edit.path {
                if let Some(dialect) = edit.dialect {
                    branch.dialect = dialect;
                }
                if let Some(label) = &edit.label {
                    branch.label = label.clone();
                }
            }
            for folder in &mut branch.folders {
                if folder.path == edit.path {
                    if let Some(role) = edit.role {
                        folder.role = role;
                    }
                    if let Some(label) = &edit.label {
                        folder.label = label.clone();
                    }
                }
            }
        }
    }
}

/// Is this root already a Picus project? Cheap enough to ask on every open.
#[arbor_rpc::handler]
fn picus_is_project(_state: &PicusState, root: String) -> Result<bool, String> {
    Ok(ProjectConfig::path_in(Path::new(&root)).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_project::prelude::{BranchConfig, FolderConfig, NamingScheme};

    fn config() -> ProjectConfig {
        ProjectConfig {
            version: picus_project::prelude::CURRENT_VERSION,
            name: "PROD_CORE".to_string(),
            encoding: Default::default(),
            version_table: Default::default(),
            generation: Default::default(),
            naming: NamingScheme::default(),
            branches: vec![BranchConfig {
                id: "common".to_string(),
                label: "COMMON".to_string(),
                path: "COMMON".to_string(),
                dialect: None,
                folders: vec![FolderConfig {
                    id: "common-misc".to_string(),
                    label: "MISCELLANEA".to_string(),
                    path: "COMMON/MISCELLANEA".to_string(),
                    role: FolderRole::Ignored,
                    encoding: None,
                    naming: None,
                }],
            }],
        }
    }

    #[test]
    fn a_correction_reaches_the_branch_and_the_folder() {
        let mut c = config();
        apply_edits(
            &mut c,
            &[
                ProjectEdit {
                    path: "COMMON".to_string(),
                    role: None,
                    dialect: Some(Some(EngineKind::Postgres)),
                    label: Some("POSTGRES".to_string()),
                },
                ProjectEdit {
                    path: "COMMON/MISCELLANEA".to_string(),
                    role: Some(FolderRole::Update),
                    dialect: None,
                    label: None,
                },
            ],
        );
        assert_eq!(c.branches[0].dialect, Some(EngineKind::Postgres));
        assert_eq!(c.branches[0].label, "POSTGRES");
        assert_eq!(c.branches[0].folders[0].role, FolderRole::Update);
        // Untouched fields stay untouched.
        assert_eq!(c.branches[0].folders[0].label, "MISCELLANEA");
    }

    #[test]
    fn clearing_a_dialect_is_different_from_not_mentioning_it() {
        // `dialect: null` means "I do not know", which is a legitimate answer.
        // Omitting the field entirely means "leave it alone". Collapsing the two
        // would make an edit to a folder wipe its branch's engine.
        let mut c = config();
        c.branches[0].dialect = Some(EngineKind::Oracle);

        apply_edits(&mut c, &[ProjectEdit {
            path: "COMMON".to_string(),
            role: None,
            dialect: None,
            label: None,
        }]);
        assert_eq!(c.branches[0].dialect, Some(EngineKind::Oracle));

        apply_edits(&mut c, &[ProjectEdit {
            path: "COMMON".to_string(),
            role: None,
            dialect: Some(None),
            label: None,
        }]);
        assert_eq!(c.branches[0].dialect, None);
    }

    #[test]
    fn an_edit_for_a_path_that_no_longer_exists_costs_nothing() {
        let mut c = config();
        let before = c.clone();
        apply_edits(&mut c, &[ProjectEdit {
            path: "GONE".to_string(),
            role: Some(FolderRole::Init),
            dialect: Some(Some(EngineKind::Oracle)),
            label: Some("x".to_string()),
        }]);
        assert_eq!(c, before);
    }

    #[test]
    fn the_edit_wire_shape_accepts_what_the_frontend_sends() {
        // Omitted vs null on `dialect` is the distinction that matters; assert the
        // deserialiser preserves it rather than trusting the derive.
        let omitted: ProjectEdit = serde_json::from_str(r#"{"path":"ORACLE"}"#).unwrap();
        assert!(omitted.dialect.is_none());

        let cleared: ProjectEdit =
            serde_json::from_str(r#"{"path":"ORACLE","dialect":null}"#).unwrap();
        assert_eq!(cleared.dialect, Some(None));

        let set: ProjectEdit =
            serde_json::from_str(r#"{"path":"ORACLE","dialect":"oracle","role":"update"}"#).unwrap();
        assert_eq!(set.dialect, Some(Some(EngineKind::Oracle)));
        assert_eq!(set.role, Some(FolderRole::Update));
    }
}
