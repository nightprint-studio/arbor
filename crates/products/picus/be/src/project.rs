//! `project` domain — opening a repository of scripts and agreeing on what it is.
//!
//! The flow this serves has one shape, and the shape is the point:
//!
//! 1. `picus_open_project` reads the folder and **proposes** what it thinks the
//!    repository is, with a note for everything it could not work out.
//! 2. The user looks, corrects the folders whose engine was not obvious and the
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
//!
//! An edit names a **folder path**, any folder path, at any depth — there is no
//! separate vocabulary for "the folder that carries the dialect" and "the folder
//! that carries the role", because in a real repository they are as likely to be
//! `AGGIORNAMENTO` and `AGGIORNAMENTO/2024/ORA` as the other way round.
//!
//! ## Two ways to say what a folder is, and both are here
//!
//! `picus_confirm_project` edits **paths**: this folder, this answer. That is the
//! right shape for a correction and the wrong shape for a repository that ships a
//! folder set per delivered version, where eleven folders are called `POS` and a
//! twelfth arrives next month. `picus_set_folder_alias` edits **names**: every
//! folder called `POS` means PostgreSQL here, including the ones that do not
//! exist yet.
//!
//! The two are ordered, not overlapping — a path declaration beats an alias,
//! because a specific answer beats a general rule — and the ordering lives in
//! `picus-project`'s discovery, not here. `picus_folders_named` exists so the
//! interface can say how many folders the general rule would reach *before* the
//! user agrees to it.

use std::path::{Path, PathBuf};

use picus_core::prelude::PicusState;
use picus_project::prelude::{
    alias_key, discover, name_matches, FolderEngine, FolderRole, InferenceAlias, Project,
    ProjectConfig, ProposalNote,
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
    /// The folder names this repository has declared a meaning for.
    ///
    /// Sent with the tree rather than fetched separately because the interface
    /// needs them to explain the tree: a `POS` folder reading as PostgreSQL when
    /// nothing about `POS` says PostgreSQL is a mystery until the vocabulary is
    /// on screen next to it.
    ///
    /// The wire shape is [`InferenceAlias`] unchanged — `name` / `engine` / `role`,
    /// all plain strings, all single words, so `camelCase` and the TOML spelling
    /// are the same thing and nothing translates.
    pub aliases: Vec<InferenceAlias>,
}

/// One correction the user made to the proposal.
///
/// Keyed by path, which is a folder's identity everywhere in Picus and the thing
/// the user is actually looking at.
///
/// Both fields are three-valued — absent, `null`, or set — and both need to be.
/// `null` means "clear the declaration", which is how a wrong guess is undone:
/// the folder then inherits from whatever is above it, and a folder nothing above
/// it declares a dialect for is simply one nothing is generated into.
///
/// `deserialize_with` is load-bearing, not decoration: a plain `Option<Option<T>>`
/// with `#[serde(default)]` collapses an explicit `null` into the same `None` as
/// an absent field, and the two mean opposite things here ("clear it" vs "leave it
/// alone"). The same distinction the connection password already needed.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEdit {
    /// Project-relative path of the folder being corrected. `""` is the
    /// repository root, whose declaration every folder inherits from.
    pub path: String,
    /// What the folder is for, from here down.
    #[serde(default, deserialize_with = "explicit_null")]
    pub role: Option<Option<FolderRole>>,
    /// The engine its scripts are written in, from here down.
    ///
    /// May name an engine Picus does **not** read (`"sqlserver"`), which is how a
    /// single folder is marked as somebody else's territory. One key, because a
    /// folder has one engine.
    #[serde(default, deserialize_with = "explicit_null")]
    pub dialect: Option<Option<FolderEngine>>,
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
    /// The vocabulary as it stands after them, so the interface never has to
    /// guess what it just wrote.
    pub aliases: Vec<InferenceAlias>,
    /// What is wrong with the configuration now — a hand-edited alias naming an
    /// engine Picus does not know, and so on. Answered on every write for the
    /// same reason it is answered on every open: a problem nobody is told about
    /// is a problem nobody fixes.
    pub problems: Vec<String>,
}

/// Read a repository and say what it looks like. Writes nothing.
#[arbor_rpc::handler]
fn picus_open_project(_state: &PicusState, root: String) -> Result<OpenedProject, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    Ok(OpenedProject {
        problems: proposal.config.problems(),
        aliases: proposal.config.aliases.clone(),
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
    save_and_replan(&root, &config)
}

/// Declare — or forget — what a folder **name** means in this repository.
///
/// The other half of classification, and the half that scales. A per-path edit
/// answers for one folder; this answers for every folder called `POS`, including
/// the ones the next release will add. Which is the whole reason it exists: a
/// repository with a folder set per delivered version cannot be described folder
/// by folder without re-describing it every release.
///
/// `engine` and `role` are both replaced, not merged: an alias has exactly these
/// two fields, so "set it to this" is unambiguous and needs none of the
/// three-valued machinery [`ProjectEdit`] needs. Passing neither **removes** the
/// alias, which is the honest reading of "this name means nothing in particular".
#[arbor_rpc::handler]
fn picus_set_folder_alias(
    _state: &PicusState,
    root: String,
    name: String,
    engine: Option<FolderEngine>,
    role: Option<FolderRole>,
) -> Result<ConfirmedProject, String> {
    if alias_key(&name).is_empty() {
        // An alias with no name would match every folder in the repository — the
        // one mistake that cannot be undone by editing a row, because there would
        // be no row left un-wrong.
        return Err("an alias needs a folder name to apply to".to_string());
    }
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    let mut config = proposal.config;

    match (engine, role) {
        (None, None) => {
            config.remove_alias(&name);
        }
        (engine, role) => {
            let alias = config.alias_mut(&name);
            alias.engine = engine.map(|e| e.as_str().to_string());
            alias.role = role.map(|r| r.as_str().to_string());
        }
    }
    config.tidy();
    save_and_replan(&root, &config)
}

/// Every folder whose name this alias would apply to, in tree order.
///
/// Asked **before** an alias is offered, so the offer can say "and the other ten
/// folders called POS" with a number that is true rather than a guess. The
/// matching rule lives in `picus-project` and is not reimplemented here or in the
/// interface: `POS` matching `01_POS` but not `POSIZIONI` is a load-bearing rule,
/// and a second copy of it is a second copy that drifts.
#[arbor_rpc::handler]
fn picus_folders_named(
    _state: &PicusState,
    root: String,
    name: String,
) -> Result<Vec<String>, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    Ok(folders_matching(&proposal.project, &name))
}

/// Write the configuration and answer with the repository as it now reads.
///
/// Discovery is re-run rather than the in-memory tree being patched: a
/// declaration changes what every descendant *effectively* is, and re-resolving
/// is the only thing that knows the inheritance rule. Shared by both writers so
/// the two can never answer with differently-shaped truth.
fn save_and_replan(root: &Path, config: &ProjectConfig) -> Result<ConfirmedProject, String> {
    let path = config.save(root).map_err(|e| e.to_string())?;
    let confirmed = discover(root).map_err(|e| e.to_string())?;
    Ok(ConfirmedProject {
        config_path: path.display().to_string(),
        aliases: confirmed.config.aliases.clone(),
        problems: confirmed.config.problems(),
        project: confirmed.project,
    })
}

/// The folders an alias of this name matches, by the crate's own rule.
///
/// The repository root is left out: its "name" is the repository's, not a
/// directory's, and discovery never infers anything about it either.
fn folders_matching(project: &Project, name: &str) -> Vec<String> {
    project
        .walk()
        .filter(|folder| !folder.path.is_empty() && name_matches(name, &folder.name))
        .map(|folder| folder.path.clone())
        .collect()
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

    let folder = proposal
        .project
        .folder_at(&folder_path)
        .ok_or_else(|| format!("{folder_path} is not a folder of this project"))?;

    let naming = config.naming_for(&folder.path).compile().map_err(|e| e.to_string())?;

    let existing: Vec<&str> = folder.files.iter().map(|file| file.name.as_str()).collect();

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

/// Apply the corrections in place.
///
/// A folder the file has never mentioned is the ordinary case rather than an
/// error — only folders that declare something are written — so an edit starts a
/// declaration where there is none, and clears one that ends up saying nothing.
fn apply_edits(config: &mut ProjectConfig, edits: &[ProjectEdit]) {
    for edit in edits {
        if edit.role.is_none() && edit.dialect.is_none() {
            continue;
        }
        let declaration = config.declaration_mut(&edit.path);
        if let Some(role) = edit.role {
            declaration.role = role;
        }
        if let Some(dialect) = edit.dialect {
            declaration.dialect = dialect;
        }
    }
    config.tidy();
}

/// Is this root already a Picus project? Cheap enough to ask on every open.
#[arbor_rpc::handler]
fn picus_is_project(_state: &PicusState, root: String) -> Result<bool, String> {
    Ok(ProjectConfig::path_in(Path::new(&root)).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_project::prelude::{
        resolve, EngineKind, FolderDeclaration, FolderNode, ForeignEngine, NamingScheme,
    };

    fn edit(path: &str) -> ProjectEdit {
        ProjectEdit { path: path.to_string(), role: None, dialect: None }
    }

    fn oracle() -> Option<Option<FolderEngine>> {
        Some(Some(FolderEngine::Supported(EngineKind::Oracle)))
    }

    fn config() -> ProjectConfig {
        ProjectConfig {
            version: picus_project::prelude::CURRENT_VERSION,
            name: "PROD_CORE".to_string(),
            encoding: Default::default(),
            version_table: Default::default(),
            generation: Default::default(),
            naming: NamingScheme::default(),
            folders: vec![FolderDeclaration {
                path: "AGGIORNAMENTO".to_string(),
                role: Some(FolderRole::Update),
                ..FolderDeclaration::default()
            }],
            aliases: Vec::new(),
        }
    }

    #[test]
    fn a_correction_can_declare_a_dialect_on_any_folder_at_any_depth() {
        // The repository this shape exists for: the role three levels above the
        // dialect. Both are ordinary edits and neither knows about the other.
        let mut c = config();
        apply_edits(
            &mut c,
            &[
                ProjectEdit {
                    dialect: Some(Some(FolderEngine::Supported(EngineKind::Postgres))),
                    ..edit("AGGIORNAMENTO/2024/POS")
                },
                ProjectEdit { role: Some(Some(FolderRole::Init)), ..edit("INIZIALIZZAZIONE") },
            ],
        );
        let pos = c.declaration("AGGIORNAMENTO/2024/POS").expect("declared");
        assert_eq!(pos.dialect, Some(FolderEngine::Supported(EngineKind::Postgres)));
        assert_eq!(pos.role, None, "the role is inherited, not copied down");
        assert_eq!(c.declaration("INIZIALIZZAZIONE").unwrap().role, Some(FolderRole::Init));
        // The declaration that was already there is untouched.
        assert_eq!(c.declaration("AGGIORNAMENTO").unwrap().role, Some(FolderRole::Update));
    }

    #[test]
    fn clearing_a_dialect_is_different_from_not_mentioning_it() {
        // `dialect: null` means "I do not know", which is a legitimate answer:
        // the folder falls back to whatever is above it, and a folder nothing
        // above it declares one for receives no generated SQL. Omitting the field
        // means "leave it alone", and collapsing the two would wipe a declaration
        // on every unrelated edit.
        let mut c = config();
        c.declaration_mut("ORACLE").dialect = Some(FolderEngine::Supported(EngineKind::Oracle));

        apply_edits(&mut c, &[ProjectEdit { role: Some(Some(FolderRole::Init)), ..edit("ORACLE") }]);
        assert_eq!(
            c.declaration("ORACLE").unwrap().dialect,
            Some(FolderEngine::Supported(EngineKind::Oracle))
        );

        apply_edits(&mut c, &[ProjectEdit { dialect: Some(None), ..edit("ORACLE") }]);
        assert_eq!(c.declaration("ORACLE").unwrap().dialect, None);
        // …and the role it still declares keeps the declaration alive.
        assert_eq!(c.declaration("ORACLE").unwrap().role, Some(FolderRole::Init));
    }

    #[test]
    fn a_declaration_cleared_of_everything_stops_being_written() {
        let mut c = config();
        apply_edits(&mut c, &[ProjectEdit { role: Some(None), ..edit("AGGIORNAMENTO") }]);
        assert!(c.declaration("AGGIORNAMENTO").is_none());
        assert!(c.folders.is_empty());
    }

    #[test]
    fn an_edit_for_a_folder_the_file_never_mentioned_starts_a_declaration() {
        // Only folders that declare something are written, so most of the tree has
        // no entry — an edit has to be able to make one rather than refusing.
        let mut c = config();
        apply_edits(
            &mut c,
            &[ProjectEdit { dialect: oracle(), ..edit("NUOVA/2026/ORA") }],
        );
        assert_eq!(
            c.declaration("NUOVA/2026/ORA").unwrap().dialect,
            Some(FolderEngine::Supported(EngineKind::Oracle))
        );
    }

    #[test]
    fn an_edit_that_says_nothing_changes_nothing() {
        let mut c = config();
        let before = c.clone();
        apply_edits(&mut c, &[edit("GONE")]);
        assert_eq!(c, before);
    }

    #[test]
    fn the_edit_wire_shape_accepts_what_the_frontend_sends() {
        // Omitted vs null is the distinction that matters, on both fields; assert
        // the deserialiser preserves it rather than trusting the derive.
        let omitted: ProjectEdit = serde_json::from_str(r#"{"path":"ORACLE"}"#).unwrap();
        assert!(omitted.dialect.is_none());
        assert!(omitted.role.is_none());

        let cleared: ProjectEdit =
            serde_json::from_str(r#"{"path":"ORACLE","dialect":null,"role":null}"#).unwrap();
        assert_eq!(cleared.dialect, Some(None));
        assert_eq!(cleared.role, Some(None));

        let set: ProjectEdit =
            serde_json::from_str(r#"{"path":"ORACLE","dialect":"oracle","role":"update"}"#).unwrap();
        assert_eq!(set.dialect, oracle());
        assert_eq!(set.role, Some(Some(FolderRole::Update)));

        // …and one key carries an engine Picus does not read, in the same field.
        let foreign: ProjectEdit =
            serde_json::from_str(r#"{"path":"MSQ","dialect":"sqlserver"}"#).unwrap();
        assert_eq!(
            foreign.dialect,
            Some(Some(FolderEngine::Unsupported(ForeignEngine::SqlServer)))
        );
    }

    // ── The vocabulary seam ───────────────────────────────────────────────────

    /// A repository shaped like the real one: a folder set per delivered version.
    fn versioned_project() -> Project {
        let mut tree = Vec::new();
        for version in ["4_11", "4_12", "4_13"] {
            let mut year = FolderNode::new(
                format!("AGGIORNAMENTO/{version}"),
                version,
            );
            year.children = ["ORA", "POS", "MSQ"]
                .iter()
                .map(|engine| {
                    FolderNode::new(format!("AGGIORNAMENTO/{version}/{engine}"), *engine)
                })
                .collect();
            tree.push(year);
        }
        let mut project =
            Project { name: "PROD_CORE".to_string(), root: r"C:\p".to_string(), tree };
        resolve(&mut project.tree, None, None);
        project
    }

    #[test]
    fn the_offer_can_count_the_folders_an_alias_would_reach() {
        // "and every folder named POS in this project" is only worth offering if
        // the number is true — this is where it comes from.
        let project = versioned_project();
        assert_eq!(
            folders_matching(&project, "POS"),
            [
                "AGGIORNAMENTO/4_11/POS",
                "AGGIORNAMENTO/4_12/POS",
                "AGGIORNAMENTO/4_13/POS"
            ]
        );
        assert_eq!(folders_matching(&project, "MSQ").len(), 3);
        // A name nothing is called reaches nothing, rather than everything.
        assert!(folders_matching(&project, "NOWHERE").is_empty());
        assert!(folders_matching(&project, "").is_empty());
    }

    #[test]
    fn the_count_uses_the_same_whole_word_rule_the_alias_will() {
        // Under-counting here would make the offer a lie; over-counting would
        // make it a scarier lie. Both come from `picus-project`'s own rule.
        let mut project = versioned_project();
        project.tree.push(FolderNode::new("POSIZIONI", "POSIZIONI"));
        project.tree.push(FolderNode::new("01_POS", "01_POS"));
        let matched = folders_matching(&project, "POS");
        assert!(matched.contains(&"01_POS".to_string()));
        assert!(!matched.contains(&"POSIZIONI".to_string()));
    }

    #[test]
    fn an_alias_is_set_replaced_and_removed_through_one_shape() {
        // The handler's body, without the filesystem: an alias has two fields, so
        // "set it to this" needs none of the three-valued machinery an edit does.
        let mut c = config();
        c.alias_mut("POS").engine = Some("postgres".to_string());
        assert_eq!(c.alias("POS").unwrap().engine.as_deref(), Some("postgres"));

        // Replacing, not merging.
        let alias = c.alias_mut("POS");
        alias.engine = Some("oracle".to_string());
        alias.role = None;
        assert_eq!(c.alias("POS").unwrap().engine.as_deref(), Some("oracle"));
        assert_eq!(c.alias("POS").unwrap().role, None);

        assert!(c.remove_alias("POS"));
        assert!(c.aliases.is_empty());
    }
}
