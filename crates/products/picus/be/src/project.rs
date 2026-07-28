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
    alias_key, discover, file_stem, name_matches, parent_of, AliasScope, FileDeclaration,
    FolderDeclaration, FolderEngine, FolderRole, InferenceAlias, InitialisationModel, LineEnding,
    Project, ProjectConfig, ProposalNote,
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
        problems: config_problems(&proposal.config),
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
    state: &PicusState,
    root: String,
    edits: Vec<ProjectEdit>,
) -> Result<ConfirmedProject, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    let mut config = proposal.config;
    apply_edits(&mut config, &edits);
    save_and_replan(state, &root, &config)
}

/// Declare — or forget — what a folder **name** means in this repository.
///
/// The other half of classification, and the half that scales. A per-path edit
/// answers for one folder; this answers for every folder called `POS`, including
/// the ones the next release will add. Which is the whole reason it exists: a
/// repository with a folder set per delivered version cannot be described folder
/// by folder without re-describing it every release.
///
/// Every field is **replaced, not merged**: an alias has exactly these three, so
/// "set it to this" is unambiguous and needs none of the three-valued machinery
/// [`ProjectEdit`] needs. Passing no engine and no role **removes** the alias,
/// which is the honest reading of "this name means nothing in particular".
///
/// `applies_to` says where the name is looked for — folder names (the default and
/// what every alias written before this meant), file names, or both. File names
/// are opt-in because a file name is a sentence: `ORA` is Italian for *now*, and
/// a repository has hundreds of file names to a dozen folder names.
#[arbor_rpc::handler]
fn picus_set_folder_alias(
    state: &PicusState,
    root: String,
    name: String,
    engine: Option<FolderEngine>,
    role: Option<FolderRole>,
    applies_to: Option<AliasScope>,
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
            let scope = applies_to.unwrap_or_default();
            let alias = config.alias_mut(&name);
            alias.engine = engine.map(|e| e.as_str().to_string());
            alias.role = role.map(|r| r.as_str().to_string());
            // Written only when it is not the default, so a repository that never
            // asked for file matching keeps a file with no mention of it.
            alias.applies_to =
                (scope != AliasScope::default()).then(|| scope.as_str().to_string());
        }
    }
    config.tidy();
    save_and_replan(state, &root, &config)
}

/// Declare — or forget — the engine of **one file**.
///
/// The leaf of the same chain `picus_confirm_project` and `picus_set_folder_alias`
/// sit on, and the one that answers for an untidy repository: a directory holding
/// `4_12_ORA.sql` beside `4_12_POS.sql` can say nothing about either, and neither
/// a folder declaration nor a name rule fits a one-off.
///
/// `dialect` absent **clears** the declaration, and the file goes back to
/// inheriting its folder. Two-valued rather than three, unlike [`ProjectEdit`]:
/// this verb names one file and one field, so there is no "leave it alone" to
/// express — not calling it is what leaves it alone.
#[arbor_rpc::handler]
fn picus_set_file_engine(
    state: &PicusState,
    root: String,
    path: String,
    dialect: Option<FolderEngine>,
) -> Result<ConfirmedProject, String> {
    if path.trim().is_empty() {
        return Err("a file declaration needs the path of a file".to_string());
    }
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    // Refused rather than written blindly: a declaration for a path that is not in
    // the tree would sit in the project file for ever, doing nothing and looking
    // like it did something.
    if proposal.project.file_at(&path).is_none() {
        return Err(format!(
            "{path} is not one of this project's scripts — refresh if it has just been added"
        ));
    }
    let mut config = proposal.config;
    match dialect {
        Some(engine) => config.file_declaration_mut(&path).dialect = Some(engine),
        None => {
            config.clear_file_declaration(&path);
        }
    }
    config.tidy();
    save_and_replan(state, &root, &config)
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

// ── The settings that belong to the repository ────────────────────────────────

/// The project-wide settings the interface edits, flattened for the wire.
///
/// Every one of these is a fact about **the repository**, not about this machine:
/// a colleague opening the same folder has to inherit them, or the same scripts
/// are judged differently per person — which is the class of surprise Picus
/// exists to remove. They live in `.arbor/picus/project.toml`, which is committed;
/// the profile's `picus/config.toml` holds only what is genuinely per-user (row
/// limits, whether to confirm before writing).
///
/// Flat and `camelCase` rather than a mirror of the TOML shape, for the reason the
/// module header gives: the file's shape is `snake_case` because a person edits
/// it by hand, and neither convention is allowed to leak into the other.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    /// The encoding folders are expected to be in unless one overrides it.
    pub encoding: String,
    /// `CRLF` or `LF` — what generated content is written with.
    pub eol: String,
    /// Where the installed version is recorded. **Empty switches the version
    /// guards off**, which the report then states out loud rather than passing.
    pub version_table: String,
    pub version_column: String,
    /// Empty means the project stamps no date, and the closing `UPDATE` leaves
    /// the column out rather than inventing one. That is why it is a plain string
    /// here and an `Option` in the file: the wire has no room for the difference
    /// between "absent" and "named the empty string", and only one of them is a
    /// real answer.
    pub date_column: String,
    /// Extra predicate, for a version table holding one row per module.
    pub version_filter: String,
    /// Other tables that also record a version in this repository.
    ///
    /// Names only. They satisfy the guard rules — a repository installing more
    /// than one product has a version table per module, and a script belonging to
    /// the second module guards against the second table — but generation still
    /// stamps the primary, because something has to be stamped.
    pub other_version_tables: Vec<String>,
    /// What the initialisation folders are, relative to the updates —
    /// [`InitialisationModel`]'s wire word.
    pub initialisation: String,
    /// Compare one dialect's scripts against the other's at all. See
    /// [`AnalysisSettings::compare_dialects`].
    pub compare_dialects: bool,
    /// Rule ids this repository does not want run.
    pub disabled_rules: Vec<String>,
    /// Object names the rules say nothing about. See
    /// [`AnalysisSettings::excluded_objects`].
    pub excluded_objects: Vec<String>,
}

impl ProjectSettings {
    fn read(config: &ProjectConfig) -> ProjectSettings {
        ProjectSettings {
            encoding: config.encoding.default.clone(),
            eol: match config.encoding.eol {
                LineEnding::Crlf => "CRLF".to_string(),
                LineEnding::Lf => "LF".to_string(),
            },
            version_table: config.version_table.table.clone(),
            version_column: config.version_table.version_column.clone(),
            date_column: config.version_table.date_column.clone().unwrap_or_default(),
            version_filter: config.version_table.filter.clone(),
            other_version_tables: config.version_table.also.clone(),
            initialisation: config.analysis.initialisation.as_wire().to_string(),
            compare_dialects: config.analysis.compare_dialects,
            disabled_rules: config.analysis.disabled_rules.clone(),
            excluded_objects: config.analysis.excluded_objects.clone(),
        }
    }

    /// Write these onto a configuration, leaving everything else — the folder
    /// declarations, the vocabulary, the naming scheme — exactly as it was.
    ///
    /// An unreadable initialisation model keeps the current one rather than
    /// silently resetting it to the default: this arrives from a select with three
    /// options, so an unknown value means something is wrong on the wire, and
    /// quietly changing which rules run is the worst available response to that.
    fn write(&self, config: &mut ProjectConfig) {
        config.encoding.default = self.encoding.trim().to_string();
        config.encoding.eol =
            if self.eol.eq_ignore_ascii_case("LF") { LineEnding::Lf } else { LineEnding::Crlf };

        config.version_table.table = self.version_table.trim().to_string();
        config.version_table.version_column = self.version_column.trim().to_string();
        let date = self.date_column.trim();
        config.version_table.date_column =
            (!date.is_empty()).then(|| date.to_string());
        config.version_table.filter = self.version_filter.trim().to_string();
        // Trimmed, de-duplicated, and never repeating the primary — the list is
        // committed and read by people, and the same table named twice reads as a
        // second module that is not there.
        let mut others: Vec<String> = Vec::new();
        for written in &self.other_version_tables {
            let name = written.trim();
            if name.is_empty()
                || name.eq_ignore_ascii_case(config.version_table.table.as_str())
                || others.iter().any(|seen| seen.eq_ignore_ascii_case(name))
            {
                continue;
            }
            others.push(name.to_string());
        }
        config.version_table.also = others;

        config.analysis.compare_dialects = self.compare_dialects;
        if let Some(model) = initialisation_from_wire(&self.initialisation) {
            config.analysis.initialisation = model;
        }
        // Trimmed, folded to the canonical spelling and de-duplicated on the way
        // in. The list is committed and read by people, and three spellings of one
        // rule in it is three chances to think a rule is off twice.
        let mut rules: Vec<String> = Vec::new();
        for written in &self.disabled_rules {
            let Some(rule) = picus_analyze::prelude::RuleId::parse(written.trim()) else {
                continue;
            };
            let id = rule.as_str().to_string();
            if !rules.contains(&id) {
                rules.push(id);
            }
        }
        rules.sort();
        config.analysis.disabled_rules = rules;

        // Folded to the comparison form on the way in, so the committed file holds
        // the name the rules will actually compare against rather than however it
        // was typed — and a reader of the TOML sees the same spelling the report
        // uses.
        let mut objects: Vec<String> = Vec::new();
        for written in &self.excluded_objects {
            let name = written.trim();
            if name.is_empty() {
                continue;
            }
            let folded = picus_analyze::prelude::fold_identifier(name);
            if !objects.contains(&folded) {
                objects.push(folded);
            }
        }
        objects.sort();
        config.analysis.excluded_objects = objects;
    }
}

fn initialisation_from_wire(wire: &str) -> Option<InitialisationModel> {
    [
        InitialisationModel::Cumulative,
        InitialisationModel::Mirrored,
        InitialisationModel::Independent,
    ]
    .into_iter()
    .find(|m| m.as_wire() == wire.trim())
}

/// What this repository currently says about itself.
#[arbor_rpc::handler]
fn picus_project_settings(_state: &PicusState, root: String) -> Result<ProjectSettings, String> {
    let proposal = discover(&PathBuf::from(&root)).map_err(|e| e.to_string())?;
    Ok(ProjectSettings::read(&proposal.config))
}

/// Write the project-wide settings and answer with the repository as it now reads.
///
/// The whole set is replaced rather than patched field by field: these come from
/// one form the user pressed Save on, and a partial write would leave the file
/// describing a state nobody chose. Everything the form does not cover — the
/// folder declarations, the aliases, the naming scheme — is untouched, because it
/// is re-read from disk here and only these fields are assigned.
#[arbor_rpc::handler]
fn picus_set_project_settings(
    state: &PicusState,
    root: String,
    settings: ProjectSettings,
) -> Result<ConfirmedProject, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    let mut config = proposal.config;
    settings.write(&mut config);
    save_and_replan(state, &root, &config)
}

/// Everything wrong with a project file, from both crates that can judge it.
///
/// `picus-project` validates what it owns — the patterns, the marker, the
/// vocabulary. It deliberately does not know what an analysis *rule* is, so
/// `[analysis] disabled_rules` holds plain strings and a typo in one degrades to
/// a line that silences nothing. `picus-analyze` owns that closed set and is
/// asked here, so the two halves of a project file's validity arrive together
/// rather than one of them being reported nowhere.
pub(crate) fn config_problems(config: &ProjectConfig) -> Vec<String> {
    let mut out = config.problems();
    out.extend(picus_analyze::prelude::rule_settings_problems(&config.analysis));
    out
}

/// Write the configuration and answer with the repository as it now reads.
///
/// Discovery is re-run rather than the in-memory tree being patched: a
/// declaration changes what every descendant *effectively* is, and re-resolving
/// is the only thing that knows the inheritance rule. Shared by both writers so
/// the two can never answer with differently-shaped truth.
fn save_and_replan(
    state: &PicusState,
    root: &Path,
    config: &ProjectConfig,
) -> Result<ConfirmedProject, String> {
    let path = config.save(root).map_err(|e| e.to_string())?;
    // The held snapshot carries the configuration the repository was *opened*
    // with. Leaving it there is how switching a rule off, or changing the
    // initialisation model, or classifying a folder changed nothing until the
    // user re-read the whole repository from disk — and "re-run the check"
    // answered exactly as before, which reads as a broken button rather than as a
    // stale cache. The decoded text is kept; only the configuration is re-read.
    crate::scripts::refresh_configuration(state, root);
    let confirmed = discover(root).map_err(|e| e.to_string())?;
    Ok(ConfirmedProject {
        config_path: path.display().to_string(),
        aliases: confirmed.config.aliases.clone(),
        problems: config_problems(&confirmed.config),
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

/// Take something out of the project — or put it back.
///
/// One verb for both a folder and a script, because it is one decision and the
/// user is pointing at one row: the path names whichever it is, and a folder path
/// and a file path cannot collide in a tree built from real directories.
///
/// **Not the same as `role = "ignored"`.** An ignored folder is still read, still
/// indexed and still checked — it just is not an installation folder and nothing
/// is generated into it, which is worth knowing about a folder full of old
/// migrations. An excluded one is treated as though it were not in the repository:
/// not parsed, not indexed, no coverage column, no findings, never a destination.
/// The two cannot be merged because `ignored` is also the fallback for a folder
/// nobody has classified, and dropping *those* from the report silently would hide
/// exactly what needs attention.
///
/// Excluded things stay visible in the tree, marked. Hiding them would leave the
/// user no way to change their mind.
#[arbor_rpc::handler]
fn picus_set_excluded(
    state: &PicusState,
    root: String,
    path: String,
    excluded: bool,
) -> Result<ConfirmedProject, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    let mut config = proposal.config;

    // A folder first: `""` is the repository root, which is a legitimate — if
    // drastic — thing to exclude, and it is not a file.
    //
    // In both branches the declaration is written **only when it differs from
    // what would be inherited**, and cleared otherwise. That is the same rule the
    // proposed configuration already follows for engines and roles, and it is
    // what stops "put this script back" from leaving an inert `excluded = false`
    // in a repository where nothing was excluded in the first place. A project
    // file should read as the decisions it embodies, not as a log of the buttons
    // that were pressed.
    if proposal.project.folder_at(&path).is_some() {
        let inherited = inherited_exclusion(&proposal.project, parent_of(&path), path.is_empty());
        set_or_clear(config.declaration_mut(&path), excluded, inherited);
    } else if let Some(folder) = proposal.project.folder_of(&path) {
        set_or_clear(config.file_declaration_mut(&path), excluded, folder.is_excluded());
    } else {
        return Err(format!(
            "{path} is not a folder or a script in this project — refresh if it has just been added"
        ));
    }
    config.tidy();
    save_and_replan(state, &root, &config)
}

/// What a folder at `path` would be if it declared nothing — its parent's answer.
///
/// `true` for the repository root only when the root declaration says so, which
/// this cannot read from the tree (the root node exists only when scripts sit
/// directly in it), so the caller says whether we are at the top.
fn inherited_exclusion(project: &Project, parent: &str, at_root: bool) -> bool {
    if at_root {
        return false;
    }
    project.folder_at(parent).map(|folder| folder.is_excluded()).unwrap_or(false)
}

/// Anything with an `excluded` slot — one function so a folder and a file cannot
/// end up following different rules about when a declaration is worth keeping.
trait Excludable {
    fn excluded_mut(&mut self) -> &mut Option<bool>;
}

impl Excludable for FolderDeclaration {
    fn excluded_mut(&mut self) -> &mut Option<bool> {
        &mut self.excluded
    }
}

impl Excludable for FileDeclaration {
    fn excluded_mut(&mut self) -> &mut Option<bool> {
        &mut self.excluded
    }
}

fn set_or_clear(declaration: &mut impl Excludable, wanted: bool, inherited: bool) {
    *declaration.excluded_mut() = (wanted != inherited).then_some(wanted);
}

/// Every **file** whose name this alias would apply to, in tree order.
///
/// The twin of [`picus_folders_named`], and it exists for the same reason: the
/// offer to turn one classification into a repository-wide rule is only safe to
/// accept because the number beside it is true. A count worked out in the
/// interface would be a second implementation of `name_matches` — and matching
/// the stem rather than the whole name, and whole words rather than substrings,
/// are exactly the rules that must not drift.
#[arbor_rpc::handler]
fn picus_files_named(
    _state: &PicusState,
    root: String,
    name: String,
) -> Result<Vec<String>, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    Ok(files_matching(&proposal.project, &name))
}

/// Matched against the **stem**, exactly as classification does, so `.sql` can
/// never match an alias called `SQL` and the preview cannot promise a reach the
/// rule will not deliver.
fn files_matching(project: &Project, name: &str) -> Vec<String> {
    project
        .all_files()
        .filter(|file| name_matches(name, file_stem(&file.name)))
        .map(|file| file.path.clone())
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
            analysis: Default::default(),
            folders: vec![FolderDeclaration {
                path: "AGGIORNAMENTO".to_string(),
                role: Some(FolderRole::Update),
                ..FolderDeclaration::default()
            }],
            files: Vec::new(),
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
    fn the_project_settings_round_trip_without_touching_anything_else() {
        let mut c = config();
        c.alias_mut("POS").engine = Some("postgres".to_string());
        let declarations = c.folders.clone();

        let mut settings = ProjectSettings::read(&c);
        settings.version_table = " VERSIONE_MODULO ".to_string();
        settings.date_column = "  ".to_string();
        settings.initialisation = "mirrored".to_string();
        settings.disabled_rules = vec!["cons001".into(), "CONS001".into(), "nonsense".into()];
        settings.write(&mut c);

        assert_eq!(c.version_table.table, "VERSIONE_MODULO", "trimmed on the way in");
        // Blank is the project stamping no date, which is a different fact from a
        // column literally called "".
        assert_eq!(c.version_table.date_column, None);
        assert_eq!(c.analysis.initialisation, InitialisationModel::Mirrored);
        // Folded to the canonical spelling, de-duplicated, and the id that names
        // no rule is dropped rather than written into a committed file.
        assert_eq!(c.analysis.disabled_rules, vec!["CONS001".to_string()]);

        // The half the form does not cover is untouched.
        assert_eq!(c.alias("POS").unwrap().engine.as_deref(), Some("postgres"));
        assert_eq!(c.folders, declarations, "the folder declarations are not this form's business");

        // Reading it back gives the normalised answer, not what was typed — which
        // is what the form will show the next time it opens.
        let again = ProjectSettings::read(&c);
        assert_eq!(again.version_table, "VERSIONE_MODULO");
        assert_eq!(again.date_column, "");
        assert_eq!(again.disabled_rules, vec!["CONS001".to_string()]);
    }

    #[test]
    fn an_unreadable_initialisation_model_keeps_the_one_already_declared() {
        // It arrives from a select with three options, so an unknown value means
        // something is wrong on the wire — and quietly changing which rules run is
        // the worst available response to that.
        let mut c = config();
        c.analysis.initialisation = InitialisationModel::Mirrored;
        let mut settings = ProjectSettings::read(&c);
        settings.initialisation = "whatever".to_string();
        settings.write(&mut c);
        assert_eq!(c.analysis.initialisation, InitialisationModel::Mirrored);
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
