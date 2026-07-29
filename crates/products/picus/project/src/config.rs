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
//! ## The shape: a flat list of declarations keyed by path
//!
//! ```toml
//! [[folder]]
//! path = "AGGIORNAMENTO"
//! role = "update"
//!
//! [[folder]]
//! path = "AGGIORNAMENTO/2024/ORA"
//! dialect = "oracle"
//! ```
//!
//! A declaration says what is true **of that folder**; everything below it
//! inherits until another declaration overrides it ([`crate::resolve`]). Flat and
//! keyed by path rather than nested, for one concrete reason: a subdirectory
//! appearing on disk must not need the file to be restructured. The previous
//! shape — an array of branches each holding an array of folders — could only
//! describe two fixed levels, and a repository whose dialect sits three levels
//! down had nowhere to say so.
//!
//! ## …and, where a repository is untidy, keyed by file
//!
//! Not every repository puts the engine in a directory. In a dirty one it is on
//! the file, with both engines side by side in one folder that can say nothing
//! about either:
//!
//! ```toml
//! [[file]]
//! path = "AGGIORNAMENTO/2024/4_12_POS.sql"
//! dialect = "postgres"
//! ```
//!
//! Same shape, same key, one level down — and the same precedence: a file that
//! declares nothing is in its folder's engine, which is the case for almost every
//! file in almost every repository. This exists so the exceptions have somewhere
//! to be written down rather than forcing a folder to be split.
//!
//! ## And a vocabulary, for the names that repeat
//!
//! A declaration answers for one path. A repository with a folder per delivered
//! version has eleven folders called `POS` and will have a twelfth next month, so
//! it also gets to say what a **name** means:
//!
//! ```toml
//! [[alias]]
//! name = "POS"
//! engine = "postgres"
//! ```
//!
//! That is [`crate::alias`], and it applies at discovery — so a `POS` folder
//! added later is classified without anyone touching this file again. A per-path
//! `[[folder]]` declaration still wins over it: a specific answer beats a general
//! rule.
//!
//! A `version = 1` file still loads: [`crate::legacy`] folds its branches and
//! folders into declarations, and nothing is lost.
//!
//! **This file is never written without the user's explicit confirmation.** The
//! functions here are plain I/O; the confirmation belongs to the caller, and the
//! proposal flow in [`crate::discover`] exists precisely so there is something to
//! confirm.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use picus_types::prelude::{FolderEngine, FolderRole};
use serde::{Deserialize, Serialize};

use crate::alias::{alias_key, AliasVocabulary, InferenceAlias};
use crate::error::ProjectError;
use crate::insertion::InsertionRule;
use crate::marker::MarkerTemplate;
use crate::naming::NamingScheme;
use crate::path::self_and_ancestors;
use crate::tree::LineEnding;

/// Where the file sits, relative to the project root.
pub const PROJECT_CONFIG_RELATIVE_PATH: &str = ".arbor/picus/project.toml";

/// The highest schema version this build understands.
///
/// `1` — branches holding folders — is still read, and migrated on the way in.
/// `2` is the flat `[[folder]]` shape. `3` adds file-level classification:
/// `[[file]]` declarations and aliases that match file names.
///
/// What is written is **not** this constant but
/// [`required_version`](ProjectConfig::required_version) — the lowest version that
/// can read the file correctly. See there for why.
pub const CURRENT_VERSION: u32 = 3;

/// The version of the flat `[[folder]]` shape, before anything could classify a
/// single file.
const FLAT_FOLDER_VERSION: u32 = 2;

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
    /// What the analysis is allowed to assume about this repository, and which
    /// rules it has been told not to run.
    #[serde(default)]
    pub analysis: AnalysisSettings,
    /// The products this repository installs, when it installs more than one.
    /// `product` singular in the file for the same reason `folder` is.
    ///
    /// Empty for the ordinary repository, which installs one thing and has one
    /// version row. See [`ProductSettings`].
    #[serde(default, rename = "product")]
    pub products: Vec<ProductSettings>,
    /// Named sets of destinations — "where a change like this always goes".
    /// See [`DestinationSet`].
    #[serde(default, rename = "destinations")]
    pub destination_sets: Vec<DestinationSet>,
    /// What each folder declares, keyed by its project-relative path. `folder`
    /// singular in the file because TOML spells an array of tables `[[folder]]`.
    ///
    /// Only folders that declare *something* appear: a folder that simply
    /// inherits is absent, and a repository that agrees with what Picus inferred
    /// writes a short file.
    #[serde(default, rename = "folder")]
    pub folders: Vec<FolderDeclaration>,
    /// What individual **files** declare, for the repositories where the engine
    /// is on the file rather than on the directory. Almost always empty.
    #[serde(default, rename = "file")]
    pub files: Vec<FileDeclaration>,
    /// **Names** that mean something in this repository — the vocabulary that
    /// answers for every folder called `POS`, including the ones not yet created,
    /// and (where the alias says so) for every file with `POS` in its name.
    /// `alias` singular for the same TOML reason as `folder`.
    ///
    /// Ordered after `folders` in this struct because `toml` refuses to emit a
    /// value after a table, and all of these are arrays of tables: the plain
    /// values have to come first, and between two arrays the order is free.
    #[serde(default, rename = "alias")]
    pub aliases: Vec<InferenceAlias>,
}

fn default_version() -> u32 {
    CURRENT_VERSION
}

/// `serde(default)` for a boolean that defaults to **true** — the derive's own
/// default is `false`, which would switch the cross-dialect comparison off for
/// every project file written before the setting existed.
fn yes() -> bool {
    true
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
    /// **Other** tables that also record a version in this repository.
    ///
    /// A repository that installs more than one product — or one product plus a
    /// portal it ships with — has a version table per module, and an update script
    /// belonging to the second module guards against the second table. With one
    /// name declared, `VER001` and `VER002` reported every one of those scripts as
    /// unguarded, which on a real repository is hundreds of findings about scripts
    /// that are guarded perfectly well.
    ///
    /// Names only, and that is the whole extent of it: these count as version
    /// tables when the rules ask "does this script check a version before it
    /// writes, and carry one forward after". They are not alternatives for
    /// *generation* — a generated block stamps [`table`](Self::table), because
    /// something has to be stamped and the primary is the one the project named
    /// first.
    #[serde(default)]
    pub also: Vec<String>,
}

impl VersionTableSettings {
    /// Every table that counts as a version table here, primary first.
    ///
    /// Empty when the project has emptied the primary name, which is how version
    /// guards are switched off — and the rules then report themselves as skipped
    /// rather than passing.
    pub fn all(&self) -> Vec<&str> {
        if self.table.trim().is_empty() {
            return Vec::new();
        }
        let mut out = vec![self.table.trim()];
        for extra in &self.also {
            let name = extra.trim();
            if !name.is_empty() && !out.iter().any(|seen| seen.eq_ignore_ascii_case(name)) {
                out.push(name);
            }
        }
        out
    }
}

impl Default for VersionTableSettings {
    fn default() -> Self {
        VersionTableSettings {
            table: "VERSIONE_DB".to_string(),
            version_column: "VERSIONE".to_string(),
            date_column: Some("DATA_AGG".to_string()),
            filter: String::new(),
            also: Vec::new(),
        }
    }
}

/// One installed product, and how its row in the version table is told apart.
///
/// A repository that ships more than one product records a version per product,
/// most often as rows of one table discriminated by a column
/// (`MODULO = 'PORTALE'`). Which row a generated block should read and stamp is
/// then a property of **where the script is going**, not of the project — and
/// nothing in the SQL says it, so the repository does:
///
/// ```toml
/// [[product]]
/// name = "PORTALE"
/// version_filter = "MODULO = 'PORTALE'"
///
/// [[folder]]
/// path = "PORTALE/AGGIORNAMENTO"
/// product = "PORTALE"
/// ```
///
/// The alternative was asking for the predicate on every destination, every time.
/// That works — and it is still available, because a destination may override —
/// but it is the same sentence retyped per generation, which is exactly the class
/// of repetition this product exists to remove.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductSettings {
    /// What folders name to say they belong here. Matched case-insensitively.
    pub name: String,
    /// The predicate that selects this product's row — `MODULO = 'PORTALE'`.
    /// Empty means this product's scripts read the table's only row, which is
    /// also what the project-wide filter means when it is empty.
    #[serde(default)]
    pub version_filter: String,
}

/// A named set of destinations — "where a change like this always goes".
///
/// Every repository writes the same datum into the same four or six places, over
/// and over: the Oracle initialisation, the PostgreSQL initialisation, this
/// release's Oracle update script, its PostgreSQL twin. Rebuilding that list per
/// generation is the single most repetitive thing about the product.
///
/// ## Why an entry names a folder and not a file
///
/// Because half of those paths are different every release. `4_13.sql` becomes
/// `4_14.sql`, and a template of literal paths is stale the moment it is most
/// useful. An entry names the **folder**; the file is either fixed (an
/// initialisation script, which really does keep its name) or left to the
/// folder's naming scheme, which already knows what the next update file is
/// called — and, with it, the versions the guard should carry.
///
/// This lives in the project file rather than in the profile because it describes
/// the repository's shape: a colleague opening the same folder should find the
/// same sets, not have to reconstruct them.
///
/// ```toml
/// [[destinations]]
/// name = "Release"
///
/// [[destinations.entry]]
/// folder = "ORACLE/AGGIORNAMENTO"
/// wrap = "block"
/// version_guard = true
///
/// [[destinations.entry]]
/// folder = "ORACLE/INIZIALIZZAZIONE"
/// file = "parametri.sql"
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DestinationSet {
    /// What the user picks it by. Matched case-insensitively; unique per project.
    pub name: String,
    /// `entry` singular in the file for the same TOML reason as `folder`.
    #[serde(default, rename = "entry")]
    pub entries: Vec<DestinationEntry>,
}

/// One destination of a set.
///
/// The rules are the ones that survive a release. The version guard's **bounds**
/// are not stored when the naming scheme can work them out again: they are
/// `4.12 → 4.13` this month and something else next month, and a template that
/// filled in last release's numbers would be worse than one that filled in
/// nothing — it would look right.
///
/// Which leaves the case where the scheme *cannot* work them out. There the file
/// name is kept too (see [`DestinationEntry::file`]), the entry names one fixed
/// file for ever, and the bounds are the only thing that can say which versions
/// that file moves between — so they are stored. The invariant is one sentence:
/// **the file records what cannot be derived, and nothing else.**
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DestinationEntry {
    /// Project-relative folder path. The engine, the role and the product all come
    /// from it, exactly as they do when a destination is added by hand.
    pub folder: String,
    /// The file inside it. Absent means **the next update file**, named by the
    /// folder's scheme — which is what makes a set usable for more than one
    /// release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// `"block"` or `"plain"`. Absent takes the role's preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<String>,
    /// Whether this destination carries a version guard at all. Its bounds are
    /// filled in when the set is applied, from the naming scheme where it can.
    #[serde(default)]
    pub version_guard: bool,
    /// The guard's bounds, stored **only** for an entry whose file is fixed —
    /// where there is no scheme to re-derive them from. Absent otherwise, and
    /// always ignored when the scheme has an answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    #[serde(default)]
    pub skip_if_present: bool,
    #[serde(default)]
    pub require_object: bool,
    #[serde(default)]
    pub transactional: bool,
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

/// What the **initialisation folders are**, relative to the update folders.
///
/// Two folders can both hold `INSERT`s and mean completely different things, and
/// no amount of reading the SQL settles which: it is a fact about how the team
/// works, so the project states it. `CONS002` and `CONS003` — the two rules that
/// compare one dialect's install half against its upgrade half — are the only
/// readers, and each of them is meaningful under exactly one reading of this.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InitialisationModel {
    /// The initialisation is **kept at the latest version**: it is a photograph of
    /// the database as it should be today, and the update folder carries every
    /// change since the first release.
    ///
    /// The default, because it is what regenerated install scripts almost always
    /// are, and because the other reading is expensive to be wrong about: on a
    /// repository of this shape it reports every row that predates the update
    /// folder — hundreds of them — and a first report that is mostly noise is a
    /// report nobody reads a second time.
    ///
    /// What follows, and it is not symmetric:
    ///
    /// * a row an update inserts **must** also be in the initialisation, or a
    ///   fresh install comes up missing something every older database has. That
    ///   is `CONS003`, and it stays on;
    /// * a row the initialisation inserts need **not** be in any update — it was
    ///   in the first release, and there is no update for the beginning. That is
    ///   `CONS002`, and it goes off.
    ///
    /// The cost of switching `CONS002` off is worth stating plainly: adding a row
    /// to the initialisation and forgetting the matching update script is a real
    /// mistake, and under this model nothing here catches it. Nothing readable
    /// from the tree tells that mistake apart from an ordinary first-release row,
    /// so the choice is between missing one and reporting all of them.
    #[default]
    Cumulative,
    /// The two halves are **two accounts of the same thing** and must agree in
    /// both directions: everything installed is also carried forward, and
    /// everything carried forward was also installed.
    ///
    /// Both rules on. Right for a repository whose initialisation is frozen at
    /// the first release and whose updates are the only way anything changes.
    Mirrored,
    /// The two halves are maintained separately and comparing them says nothing.
    /// Both rules off.
    Independent,
}

impl InitialisationModel {
    /// Must a row the initialisation writes also appear in some update? (`CONS002`)
    pub fn expects_installed_rows_in_updates(self) -> bool {
        matches!(self, InitialisationModel::Mirrored)
    }

    /// Must a row an update writes also appear in the initialisation? (`CONS003`)
    pub fn expects_updated_rows_in_the_initialisation(self) -> bool {
        matches!(self, InitialisationModel::Cumulative | InitialisationModel::Mirrored)
    }

    /// The wire word, which is also what the TOML file holds.
    pub fn as_wire(self) -> &'static str {
        match self {
            InitialisationModel::Cumulative => "cumulative",
            InitialisationModel::Mirrored => "mirrored",
            InitialisationModel::Independent => "independent",
        }
    }

    /// Written for the person reading a skipped-rule line, and for the settings
    /// panel: one sentence, in the present tense, about the repository.
    pub fn describe(self) -> &'static str {
        match self {
            InitialisationModel::Cumulative => {
                "the initialisation is kept at the latest version, so it holds rows no update \
                 carries"
            }
            InitialisationModel::Mirrored => {
                "the initialisation and the updates are two accounts of the same changes and must \
                 agree in both directions"
            }
            InitialisationModel::Independent => {
                "the initialisation and the updates are maintained separately"
            }
        }
    }
}

/// What the analysis may assume, and what it has been told not to look at.
///
/// `Default` is written out rather than derived: the derive would give
/// `compare_dialects: false`, which is the opposite of the intended default and
/// would silently switch off the comparison this product exists for. A boolean
/// whose honest default is `true` cannot be derived, and getting that wrong is
/// invisible until somebody notices their report went quiet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AnalysisSettings {
    /// See [`InitialisationModel`].
    #[serde(default)]
    pub initialisation: InitialisationModel,
    /// Compare one dialect's scripts against the other's at all.
    ///
    /// On by default, because it is what Picus is **for**. Off for a repository
    /// whose two halves have diverged far enough that the comparison says nothing
    /// usable — different layouts, different table names, one side generated by a
    /// dump and the other written by hand. There, `CONS001` and `CONS004` produce
    /// a wall of findings that is technically accurate and completely unactionable,
    /// and the rules that remain — the version chain, the duplicates, the
    /// dangerous DML, the encodings — are worth having on their own.
    ///
    /// A switch rather than "turn off those two rules", because it is one decision
    /// about the repository and it should read as one. The two rules still report
    /// themselves as not run, naming this setting.
    #[serde(default = "yes")]
    pub compare_dialects: bool,
    /// Rules this repository does not want run, by id (`"CONS001"`).
    ///
    /// Held as **strings** rather than as a parsed enum for the same reason the
    /// insertion rules are: an id from a newer build, or a typo, must degrade to
    /// "this line does nothing" rather than fail the parse and take every other
    /// setting in the file down with it. `picus-analyze` owns the closed set and
    /// reports the ones it does not recognise — see `rule_settings_problems`
    /// there, because this crate deliberately does not know what a rule is.
    ///
    /// A disabled rule is never silently absent: it is reported in the analysis
    /// as a rule that did not run, with this file named as the reason. A report
    /// that cannot be told apart from a clean one is the failure this whole
    /// product exists to prevent.
    #[serde(default)]
    pub disabled_rules: Vec<String>,
    /// Objects the rules say nothing about, by name.
    ///
    /// The escape hatch for the handful of tables in every real repository that
    /// are a special case for a reason nothing in the scripts can express: a
    /// staging table one dialect fills and the other does not need, a log the
    /// installer writes to, a legacy table kept alive for one customer. Turning a
    /// whole rule off to silence one table is a bad trade — it stops watching the
    /// other four hundred.
    ///
    /// Matched on the **name**, case-insensitively, whatever kind of object
    /// carries it: a name is what a person knows, and a repository that has both
    /// a `MECATALOGO` table and a `MECATALOGO` view means one thing by it.
    ///
    /// It excludes the object from the **rules**, not from the index. It still
    /// appears in the inventory with its coverage, because "what is in this
    /// repository" and "what should be checked" are different questions and
    /// answering the first one wrongly would hide the very thing the exclusion was
    /// reasoned about.
    #[serde(default)]
    pub excluded_objects: Vec<String>,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        AnalysisSettings {
            initialisation: InitialisationModel::default(),
            compare_dialects: true,
            disabled_rules: Vec::new(),
            excluded_objects: Vec::new(),
        }
    }
}

impl AnalysisSettings {
    /// Is this rule id switched off? Case-insensitive: the value is typed by a
    /// person into a TOML file, and `cons001` unmistakably means `CONS001`.
    pub fn disables(&self, rule: &str) -> bool {
        self.disabled_rules.iter().any(|d| d.trim().eq_ignore_ascii_case(rule))
    }
}

/// What one folder declares about itself.
///
/// Every field except the path is optional and every one of them **inherits**:
/// a folder that declares nothing about its encoding is in its nearest
/// ancestor's, exactly as it is in its nearest ancestor's dialect.
///
/// Field order is load-bearing for TOML: the values first, the `naming` table
/// last, because `toml` refuses to emit a value after a table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FolderDeclaration {
    /// Project-relative path, POSIX separators. `""` is the repository root, and
    /// a declaration there applies to everything.
    pub path: String,
    /// The engine every script under here is written in, unless something below
    /// says otherwise. Absent means "inherit"; nothing is generated into a folder
    /// no ancestor declares one for.
    ///
    /// The value may name an engine Picus does **not** read — `dialect =
    /// "sqlserver"` — which is how one folder is pinned as somebody else's
    /// territory without inventing a second key for it. A folder has one engine,
    /// so it has one key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect: Option<FolderEngine>,
    /// What the folder is for. Absent means "inherit", falling back to
    /// [`FolderRole::Ignored`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<FolderRole>,
    /// Overrides the project's default encoding from here down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    /// Which installed product's scripts live here, from here down. Absent means
    /// "inherit". Names a [`ProductSettings`]; a name no product declares is
    /// reported by [`problems`](ProjectConfig::problems) and otherwise behaves as
    /// though nothing was said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// Leave this folder — and everything under it — out of the project
    /// entirely. Absent means "inherit"; a descendant may set it back to `false`.
    ///
    /// **Not the same as `role = "ignored"`**, and the difference is the reason
    /// this is a separate field rather than a fifth role. `ignored` says *this is
    /// not an installation folder*: nothing is generated into it and it takes
    /// part in no cross-dialect comparison, but it is still read, its objects
    /// still appear in the inventory, and its files are still checked. That is
    /// deliberate — knowing that `MIGRAZIONE_2019` creates a table is useful.
    ///
    /// `excluded` says *pretend this is not in the repository*: not parsed, not
    /// indexed, no column, no findings, never a destination.
    ///
    /// They cannot be merged, because `ignored` is also the **fallback** for a
    /// folder nobody has classified. Making the fallback mean "excluded" would
    /// silently drop from the report exactly the folders that need attention —
    /// the same trap the model already avoids by keeping "an engine Picus cannot
    /// read" distinct from "an engine nobody has named".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluded: Option<bool>,
    /// Overrides the update-file naming from here down — a folder whose update
    /// files are named differently from its sibling's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naming: Option<NamingScheme>,
}

/// What one **file** declares about itself.
///
/// Only the engine, and deliberately only the engine. A role is what a directory
/// of scripts is *for*, and the file beside this one in the same directory is for
/// the same thing; an encoding is measured from the bytes rather than declared.
/// The engine is the one fact that genuinely varies file by file in an untidy
/// repository, so it is the one fact this can say.
///
/// Unlike [`FolderDeclaration`], nothing here inherits **downwards** — a file has
/// nothing below it. It is a leaf answer, and it beats everything.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileDeclaration {
    /// Project-relative path of the file, POSIX separators.
    pub path: String,
    /// The engine this one file is written in. Named `dialect` to match
    /// [`FolderDeclaration::dialect`]: it is the same question with the same
    /// answers, including `generic` for a portable script and `sqlserver` for one
    /// that wandered in from another product.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect: Option<FolderEngine>,
    /// Leave this one script out of the project — the migration script nobody
    /// wants counted, sitting in a folder full of ones they do. Absent means
    /// "inherit from the folder"; `false` rescues a single file from an excluded
    /// folder. See [`FolderDeclaration::excluded`] for why this is not a role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluded: Option<bool>,
}

impl FileDeclaration {
    pub fn new(path: impl Into<String>) -> FileDeclaration {
        FileDeclaration { path: path.into(), ..FileDeclaration::default() }
    }

    /// A declaration that says neither of the two things it can say is noise — a
    /// file with no declaration already inherits its folder — so it is dropped
    /// rather than written.
    pub fn is_empty(&self) -> bool {
        self.dialect.is_none() && self.excluded.is_none()
    }
}

impl FolderDeclaration {
    /// A declaration for a path that says nothing yet.
    pub fn new(path: impl Into<String>) -> FolderDeclaration {
        FolderDeclaration { path: path.into(), ..FolderDeclaration::default() }
    }

    /// Does this declaration say anything at all? One that does not is noise in
    /// the file and is dropped rather than written.
    pub fn is_empty(&self) -> bool {
        self.dialect.is_none()
            && self.role.is_none()
            && self.encoding.is_none()
            && self.excluded.is_none()
            && self.naming.is_none()
    }
}

impl ProjectConfig {
    /// The absolute path of the project file for a root.
    pub fn path_in(root: &Path) -> PathBuf {
        root.join(".arbor").join("picus").join("project.toml")
    }

    /// Parse a project file, migrating a `version = 1` one on the way in.
    pub fn parse(text: &str) -> Result<ProjectConfig, toml::de::Error> {
        crate::legacy::parse(text)
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
        let config = ProjectConfig::parse(&text)
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
        // Stamped with what this content actually needs, not with what this build
        // happens to be — see `required_version`.
        let stamped = ProjectConfig { version: self.required_version(), ..self.clone() };
        let text = toml::to_string_pretty(&stamped)
            .map_err(|e| ProjectError::Malformed { path: path.clone(), reason: e.to_string() })?;
        std::fs::write(&path, text)
            .map_err(|e| ProjectError::Io { path: path.clone(), reason: e.to_string() })?;
        Ok(path)
    }

    /// What this exact folder declares, if anything.
    pub fn declaration(&self, path: &str) -> Option<&FolderDeclaration> {
        self.folders.iter().find(|f| f.path == path)
    }

    /// The declaration for a folder, created empty if there is none.
    ///
    /// The tree has folders the file has never mentioned — that is the normal
    /// state, since only folders that declare something are written — so an edit
    /// has to be able to start one.
    pub fn declaration_mut(&mut self, path: &str) -> &mut FolderDeclaration {
        if let Some(index) = self.folders.iter().position(|f| f.path == path) {
            return &mut self.folders[index];
        }
        self.folders.push(FolderDeclaration::new(path));
        self.folders.last_mut().expect("just pushed")
    }

    /// What this exact **file** declares, if anything.
    pub fn file_declaration(&self, path: &str) -> Option<&FileDeclaration> {
        self.files.iter().find(|f| f.path == path)
    }

    /// The declaration for a file, created empty if there is none.
    pub fn file_declaration_mut(&mut self, path: &str) -> &mut FileDeclaration {
        if let Some(index) = self.files.iter().position(|f| f.path == path) {
            return &mut self.files[index];
        }
        self.files.push(FileDeclaration::new(path));
        self.files.last_mut().expect("just pushed")
    }

    /// Forget what a file declared, so it inherits its folder again. `true` when
    /// there was something to forget.
    pub fn clear_file_declaration(&mut self, path: &str) -> bool {
        let before = self.files.len();
        self.files.retain(|f| f.path != path);
        self.files.len() != before
    }

    /// Drop declarations that no longer say anything, and keep the file ordered
    /// by path so a diff of it reads like the tree. The vocabulary is tidied the
    /// same way, by name.
    pub fn tidy(&mut self) {
        self.folders.retain(|f| !f.is_empty());
        self.folders.sort_by(|a, b| a.path.cmp(&b.path));
        self.files.retain(|f| !f.is_empty() && !f.path.is_empty());
        self.files.sort_by(|a, b| a.path.cmp(&b.path));
        self.aliases.retain(|a| !a.is_empty() && !a.key().is_empty());
        self.aliases.sort_by(|a, b| a.key().cmp(&b.key()));
    }

    /// The lowest schema version that can read this configuration **correctly**.
    ///
    /// Written instead of [`CURRENT_VERSION`] because a version number is a claim
    /// about compatibility, and the honest claim depends on what the file says.
    /// The project file is committed and shared: stamping every save with the
    /// newest number would lock a colleague on an older build out of a repository
    /// that uses nothing their build lacks, while stamping every save with the
    /// oldest would let that build silently ignore the `[[file]]` declarations
    /// that decide which dialect a script is parsed as — and silently ignoring a
    /// classification is the failure this product exists to prevent.
    ///
    /// So: `3` exactly when something here classifies an individual file, `2`
    /// otherwise.
    ///
    /// `[analysis]` deliberately does **not** bump it. An older build that ignores
    /// the section runs the two propagation rules and honours no disabled rule, so
    /// it reports *more* than it was asked to — noisy, and visibly so. The version
    /// number exists to stop a build from silently reporting **less**, and this
    /// section cannot cause that.
    pub fn required_version(&self) -> u32 {
        // Exclusion counts for the same reason file classification does: a build
        // that silently ignored it would analyse and report on scripts their
        // owner has said do not belong to this project.
        let excludes = self.folders.iter().any(|f| f.excluded.is_some());
        let classifies_files =
            !self.files.is_empty() || self.aliases.iter().any(|a| a.scope().covers_files());
        if excludes || classifies_files {
            CURRENT_VERSION
        } else {
            FLAT_FOLDER_VERSION
        }
    }

    // ── The project's own vocabulary ──────────────────────────────────────────

    /// The vocabulary, compiled and ready to answer questions about a folder.
    ///
    /// Built per call rather than cached: a `ProjectConfig` is a plain value that
    /// callers clone and edit, and a cache on it would be one more thing that can
    /// disagree with the field beside it. Discovery compiles it once per scan,
    /// which is the only hot path.
    pub fn vocabulary(&self) -> AliasVocabulary {
        AliasVocabulary::compile(&self.aliases)
    }

    /// What this project declares about a folder name, if anything.
    ///
    /// Looked up by [`alias_key`], not by the literal string: `POS` and `pos`
    /// are the same alias because they match the same folders, and an editor
    /// that thought otherwise would let the file grow two entries fighting over
    /// the same rows.
    pub fn alias(&self, name: &str) -> Option<&InferenceAlias> {
        let key = alias_key(name);
        self.aliases.iter().find(|a| a.key() == key)
    }

    /// The alias for a name, created empty if there is none.
    pub fn alias_mut(&mut self, name: &str) -> &mut InferenceAlias {
        let key = alias_key(name);
        if let Some(index) = self.aliases.iter().position(|a| a.key() == key) {
            return &mut self.aliases[index];
        }
        self.aliases.push(InferenceAlias::new(name));
        self.aliases.last_mut().expect("just pushed")
    }

    /// Forget a name. `true` when there was one to forget.
    pub fn remove_alias(&mut self, name: &str) -> bool {
        let key = alias_key(name);
        let before = self.aliases.len();
        self.aliases.retain(|a| a.key() != key);
        self.aliases.len() != before
    }

    /// The first thing a folder or one of its ancestors declares.
    ///
    /// The shared half of every inherited setting: the nearest declaration wins,
    /// and the repository root (`""`) is the last one tried.
    fn inherited<'a, T>(
        &'a self,
        folder_path: &str,
        pick: impl Fn(&'a FolderDeclaration) -> Option<T>,
    ) -> Option<T> {
        self_and_ancestors(folder_path)
            .into_iter()
            .find_map(|ancestor| self.declaration(ancestor).and_then(&pick))
    }

    /// The naming scheme a folder or one of its ancestors declares, if any.
    pub fn declared_naming(&self, folder_path: &str) -> Option<&NamingScheme> {
        self.inherited(folder_path, |d| d.naming.as_ref())
    }

    /// The encoding a folder or one of its ancestors declares, if any.
    ///
    /// Distinct from [`encoding_for`](Self::encoding_for) because "the project's
    /// default" and "somebody pinned this folder" are different facts: discovery
    /// lets the files vote in the first case and refuses to in the second.
    pub fn declared_encoding(&self, folder_path: &str) -> Option<&str> {
        self.inherited(folder_path, |d| d.encoding.as_deref())
    }

    /// The naming scheme in force for a folder — the nearest declared one, or the
    /// project's.
    pub fn naming_for(&self, folder_path: &str) -> &NamingScheme {
        self.declared_naming(folder_path).unwrap_or(&self.naming)
    }

    /// The encoding a folder's files are expected to be in.
    pub fn encoding_for(&self, folder_path: &str) -> &str {
        self.declared_encoding(folder_path).unwrap_or(&self.encoding.default)
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
        for folder in &self.folders {
            if let Some(naming) = &folder.naming {
                if let Err(e) = naming.compile() {
                    out.push(format!("{}: {e}", folder.path));
                }
            }
        }
        for name in self.generation.marker.unknown_placeholders() {
            out.push(format!(
                "the block marker uses `{{{name}}}`, which is not a placeholder Picus knows — it will always be empty"
            ));
        }
        for (role, rule) in &self.generation.insertion {
            match FolderRole::from_wire(role) {
                None => out.push(format!(
                    "`[generation.insertion]` declares `{role}`, which is not a folder role — \
                     no folder will ever use it"
                )),
                Some(known) if InsertionRule::from_wire(rule).is_none() => out.push(format!(
                    "`{rule}` is not an insertion rule Picus knows, so `{role}` folders keep the \
                     default ({})",
                    InsertionRule::default_for(known).describe()
                )),
                Some(_) => {}
            }
        }
        for folder in &self.folders {
            let Some(named) = folder.product.as_deref() else { continue };
            if self.product(named).is_none() {
                out.push(format!(
                    "{}: `product = \"{named}\"` names no `[[product]]`, so generated blocks there \
                     read the project's version row rather than that product's",
                    folder.path
                ));
            }
        }
        out.extend(crate::alias::problems(&self.aliases));
        out
    }

    /// One destination set by name, case-insensitively.
    pub fn destination_set(&self, name: &str) -> Option<&DestinationSet> {
        let name = name.trim();
        self.destination_sets.iter().find(|s| s.name.trim().eq_ignore_ascii_case(name))
    }

    /// Add a set, or replace the one of that name. Returns `true` when it replaced.
    ///
    /// Replace rather than append, because "save this as Release" said twice means
    /// the second one — and two sets of one name would leave the picker showing
    /// the same entry twice with different contents behind it.
    pub fn put_destination_set(&mut self, set: DestinationSet) -> bool {
        let name = set.name.trim().to_string();
        match self
            .destination_sets
            .iter()
            .position(|s| s.name.trim().eq_ignore_ascii_case(&name))
        {
            Some(at) => {
                self.destination_sets[at] = DestinationSet { name, ..set };
                true
            }
            None => {
                self.destination_sets.push(DestinationSet { name, ..set });
                self.destination_sets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                false
            }
        }
    }

    /// Forget a set. Returns `true` when there was one to forget.
    pub fn remove_destination_set(&mut self, name: &str) -> bool {
        let before = self.destination_sets.len();
        let name = name.trim();
        self.destination_sets.retain(|s| !s.name.trim().eq_ignore_ascii_case(name));
        self.destination_sets.len() != before
    }

    /// One declared product by name, case-insensitively. `None` when nothing
    /// declares it — which is a state the user is told about rather than one that
    /// invents a predicate.
    pub fn product(&self, name: &str) -> Option<&ProductSettings> {
        let name = name.trim();
        self.products.iter().find(|p| p.name.trim().eq_ignore_ascii_case(name))
    }

    /// The predicate that selects the version row for scripts belonging to
    /// `product` — the product's own when it declares one, the project-wide filter
    /// otherwise.
    ///
    /// The fallback is what keeps this feature free for the repositories that do
    /// not need it: with no `[[product]]` anywhere, every destination gets the
    /// project's filter exactly as it did before any of this existed.
    pub fn version_filter_for(&self, product: Option<&str>) -> &str {
        product
            .and_then(|name| self.product(name))
            .map(|p| p.version_filter.as_str())
            .unwrap_or(&self.version_table.filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_types::prelude::{EngineKind, ForeignEngine};

    fn oracle() -> Option<FolderEngine> {
        Some(FolderEngine::Supported(EngineKind::Oracle))
    }

    fn sample() -> ProjectConfig {
        ProjectConfig {
            version: CURRENT_VERSION,
            name: "PROD_CORE".to_string(),
            encoding: EncodingSettings::default(),
            version_table: VersionTableSettings::default(),
            generation: GenerationSettings::default(),
            naming: NamingScheme::default(),
            analysis: AnalysisSettings::default(),
            products: Vec::new(),
            destination_sets: Vec::new(),
            folders: vec![
                FolderDeclaration {
                    path: "AGGIORNAMENTO".to_string(),
                    role: Some(FolderRole::Update),
                    ..FolderDeclaration::default()
                },
                FolderDeclaration {
                    path: "AGGIORNAMENTO/2024/ORA".to_string(),
                    dialect: oracle(),
                    ..FolderDeclaration::default()
                },
            ],
            files: Vec::new(),
            aliases: Vec::new(),
        }
    }

    #[test]
    fn it_round_trips_through_toml() {
        let text = toml::to_string_pretty(&sample()).expect("serialises");
        let back = ProjectConfig::parse(&text).expect("parses");
        assert_eq!(back, sample());
    }

    #[test]
    fn the_analysis_section_round_trips_and_defaults_to_a_cumulative_initialisation() {
        // The default is the reading real repositories have: the initialisation is
        // regenerated to the current state, so it holds rows no update carries.
        assert_eq!(AnalysisSettings::default().initialisation, InitialisationModel::Cumulative);

        let mut config = sample();
        config.analysis.initialisation = InitialisationModel::Mirrored;
        config.analysis.disabled_rules = vec!["CONS001".to_string()];

        let text = toml::to_string_pretty(&config).expect("serialises");
        assert!(text.contains(r#"initialisation = "mirrored""#), "{text}");
        assert_eq!(ProjectConfig::parse(&text).expect("parses"), config);
    }

    #[test]
    fn a_project_file_written_before_the_analysis_section_still_loads() {
        // Every project file already on disk predates it, and a section that made
        // them unreadable would be a section that emptied somebody's settings.
        let mut older = sample();
        older.analysis = AnalysisSettings::default();
        let text = toml::to_string_pretty(&older).expect("serialises");
        let without: String =
            text.lines().filter(|l| !l.contains("initialisation")).collect::<Vec<_>>().join("\n");

        let back = ProjectConfig::parse(&without).expect("parses");
        assert_eq!(back.analysis.initialisation, InitialisationModel::Cumulative);
    }

    #[test]
    fn the_analysis_section_does_not_raise_the_version_a_reader_needs() {
        // An older build that ignores it runs MORE rules than it was asked to,
        // which is noisy and visible. The version number exists to stop a build
        // reporting LESS, and this section cannot cause that.
        let mut config = sample();
        let before = config.required_version();
        config.analysis.initialisation = InitialisationModel::Independent;
        config.analysis.disabled_rules = vec!["CONS001".to_string(), "DML002".to_string()];
        assert_eq!(config.required_version(), before);
    }

    #[test]
    fn a_disabled_rule_is_recognised_however_it_was_typed() {
        let settings = AnalysisSettings {
            disabled_rules: vec!["  cons001 ".to_string()],
            ..AnalysisSettings::default()
        };
        assert!(settings.disables("CONS001"));
        assert!(!settings.disables("CONS002"));
    }

    #[test]
    fn the_declarations_are_a_flat_list_keyed_by_path() {
        // The shape the whole model rests on: the role is declared at the top of
        // the tree and the dialect three levels down, in the same list.
        let text = toml::to_string_pretty(&sample()).expect("serialises");
        assert!(text.contains("[[folder]]"), "{text}");
        assert!(text.contains(r#"path = "AGGIORNAMENTO/2024/ORA""#), "{text}");
        assert!(!text.contains("[[branch]]"), "{text}");
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
        let back = ProjectConfig::parse(&text).expect("parses");
        assert_eq!(back.version_table.date_column, None);
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        // Someone hand-writes the minimum. Everything else must fill in rather
        // than the parse failing and taking the whole project with it.
        let text = r#"
            name = "MINIMAL"
            [[folder]]
            path = "POSTGRES"
            dialect = "postgres"
        "#;
        let config = ProjectConfig::parse(text).expect("parses");
        assert_eq!(config.version, CURRENT_VERSION);
        assert_eq!(config.encoding.default, DEFAULT_ENCODING);
        assert_eq!(config.encoding.eol, LineEnding::Crlf);
        assert_eq!(config.version_table.table, "VERSIONE_DB");
        assert_eq!(config.naming, NamingScheme::default());
        assert_eq!(config.folders[0].dialect, Some(FolderEngine::Supported(EngineKind::Postgres)));
        // A declaration that says nothing about a role means "inherit", not
        // "ignored" — the resolver decides that, not the parse.
        assert_eq!(config.folders[0].role, None);
    }

    #[test]
    fn an_overridden_setting_reaches_every_folder_below_it() {
        let mut config = sample();
        config.declaration_mut("AGGIORNAMENTO").encoding = Some("UTF-8".to_string());
        config.declaration_mut("AGGIORNAMENTO").naming = Some(NamingScheme {
            pattern: r"^(?P<to>\d+)\.sql$".to_string(),
            template: "{to}.sql".to_string(),
            separator: '_',
        });

        // The folder itself, and a folder three levels under it.
        for path in ["AGGIORNAMENTO", "AGGIORNAMENTO/2024/ORA"] {
            assert_eq!(config.encoding_for(path), "UTF-8", "{path}");
            assert_ne!(config.naming_for(path), &NamingScheme::default(), "{path}");
        }
        // …and a folder somewhere else inherits neither.
        assert_eq!(config.encoding_for("INIZIALIZZAZIONE"), DEFAULT_ENCODING);
        assert_eq!(config.naming_for("INIZIALIZZAZIONE"), &NamingScheme::default());
    }

    #[test]
    fn a_declaration_on_the_root_is_the_projects_own_setting() {
        let mut config = sample();
        config.declaration_mut("").encoding = Some("UTF-8".to_string());
        assert_eq!(config.encoding_for("AGGIORNAMENTO/2024/ORA"), "UTF-8");
        // The nearest declaration still wins over the root's.
        config.declaration_mut("AGGIORNAMENTO").encoding = Some("windows-1252".to_string());
        assert_eq!(config.encoding_for("AGGIORNAMENTO/2024/ORA"), "windows-1252");
    }

    #[test]
    fn an_edit_starts_a_declaration_for_a_folder_the_file_never_mentioned() {
        let mut config = sample();
        config.declaration_mut("AGGIORNAMENTO/2024/POS").dialect =
            Some(FolderEngine::Supported(EngineKind::Postgres));
        assert_eq!(config.folders.len(), 3);
        assert_eq!(
            config.declaration("AGGIORNAMENTO/2024/POS").unwrap().dialect,
            Some(FolderEngine::Supported(EngineKind::Postgres))
        );
    }

    #[test]
    fn a_folder_can_be_pinned_to_an_engine_picus_does_not_read() {
        // One key for one slot: the same `dialect` that says `oracle` says
        // `sqlserver`, and it survives the round trip as that.
        let text = r#"
            name = "MINIMAL"
            [[folder]]
            path = "AGGIORNAMENTO/2024/MSQ"
            dialect = "sqlserver"
        "#;
        let config = ProjectConfig::parse(text).expect("parses");
        assert_eq!(
            config.folders[0].dialect,
            Some(FolderEngine::Unsupported(ForeignEngine::SqlServer))
        );
        let back = ProjectConfig::parse(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn tidying_drops_a_declaration_that_says_nothing() {
        let mut config = sample();
        config.declaration_mut("AGGIORNAMENTO/2024/POS");
        assert_eq!(config.folders.len(), 3);
        config.tidy();
        assert_eq!(config.folders.len(), 2);
        // …and orders what is left by path.
        let paths: Vec<&str> = config.folders.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["AGGIORNAMENTO", "AGGIORNAMENTO/2024/ORA"]);
    }

    #[test]
    fn a_project_can_override_where_a_generated_block_lands_per_role() {
        let text = r#"
            name = "MINIMAL"
            [generation.insertion]
            update = "before-final-commit"
            init = "nonsense"
        "#;
        let config =
            ProjectConfig::parse(text).expect("an unknown rule must not fail the parse");

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
        let back = ProjectConfig::parse(&text).expect("parses");
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
    fn a_bad_pattern_on_one_folder_is_reported_against_that_folder() {
        let mut config = sample();
        config.declaration_mut("AGGIORNAMENTO/2024/ORA").naming = Some(NamingScheme {
            pattern: "^(?P<to>[unclosed".to_string(),
            template: "{to}.sql".to_string(),
            separator: '_',
        });
        let problems = config.problems();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("AGGIORNAMENTO/2024/ORA:"), "{problems:?}");
    }

    // ── Classifying one file ──────────────────────────────────────────────────

    #[test]
    fn a_single_file_can_be_declared_and_round_trips() {
        let mut config = sample();
        config.file_declaration_mut("AGGIORNAMENTO/2024/4_12_POS.sql").dialect =
            Some(FolderEngine::Supported(EngineKind::Postgres));

        let text = toml::to_string_pretty(&config).expect("serialises");
        assert!(text.contains("[[file]]"), "{text}");
        assert!(text.contains(r#"path = "AGGIORNAMENTO/2024/4_12_POS.sql""#), "{text}");
        assert_eq!(ProjectConfig::parse(&text).expect("parses"), config);
    }

    #[test]
    fn a_file_declaration_takes_the_same_four_answers_a_folder_does() {
        let text = r#"
            name = "MINIMAL"
            [[file]]
            path = "AGGIORNAMENTO/comune.sql"
            dialect = "generic"
            [[file]]
            path = "AGGIORNAMENTO/4_12_MSQ.sql"
            dialect = "sqlserver"
        "#;
        let config = ProjectConfig::parse(text).expect("parses");
        assert_eq!(
            config.file_declaration("AGGIORNAMENTO/comune.sql").unwrap().dialect,
            Some(FolderEngine::Generic)
        );
        assert_eq!(
            config.file_declaration("AGGIORNAMENTO/4_12_MSQ.sql").unwrap().dialect,
            Some(FolderEngine::Unsupported(ForeignEngine::SqlServer))
        );
        assert!(config.file_declaration("AGGIORNAMENTO/other.sql").is_none());
    }

    #[test]
    fn clearing_a_file_declaration_puts_it_back_in_its_folders_hands() {
        let mut config = sample();
        config.file_declaration_mut("a/b.sql").dialect =
            Some(FolderEngine::Supported(EngineKind::Postgres));
        assert!(config.clear_file_declaration("a/b.sql"));
        assert!(config.file_declaration("a/b.sql").is_none());
        assert!(!config.clear_file_declaration("a/b.sql"), "nothing left to clear");
    }

    #[test]
    fn tidying_drops_a_file_declaration_that_says_nothing_and_orders_the_rest() {
        let mut config = sample();
        config.file_declaration_mut("z.sql").dialect = Some(FolderEngine::Generic);
        config.file_declaration_mut("a.sql").dialect = Some(FolderEngine::Generic);
        config.file_declaration_mut("m.sql"); // started and never filled in
        config.tidy();
        let paths: Vec<&str> = config.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a.sql", "z.sql"]);
    }

    #[test]
    fn the_written_version_is_the_lowest_that_can_read_the_file_correctly() {
        // A version number is a claim about compatibility, so it has to depend on
        // what the file says rather than on which build wrote it.
        let plain = sample();
        assert_eq!(plain.required_version(), 2, "nothing here classifies a file");

        let mut with_file = sample();
        with_file.file_declaration_mut("a.sql").dialect = Some(FolderEngine::Generic);
        assert_eq!(with_file.required_version(), CURRENT_VERSION);

        // …and an alias pointed at file names counts just as much: an older build
        // would read it, ignore the `applies_to`, and classify the files wrongly.
        let mut with_alias = sample();
        with_alias.alias_mut("POS").engine = Some("postgres".to_string());
        assert_eq!(with_alias.required_version(), 2);
        with_alias.alias_mut("POS").applies_to = Some("both".to_string());
        assert_eq!(with_alias.required_version(), CURRENT_VERSION);
    }

    #[test]
    fn a_version_two_file_still_loads_and_declares_no_files() {
        let text = r#"
            version = 2
            name = "PROD_CORE"
            [[folder]]
            path = "AGGIORNAMENTO"
            role = "update"
        "#;
        let config = ProjectConfig::parse(text).expect("parses");
        assert_eq!(config.version, 2);
        assert!(config.files.is_empty());
        assert_eq!(config.declaration("AGGIORNAMENTO").unwrap().role, Some(FolderRole::Update));
    }

    // ── The project's own vocabulary ──────────────────────────────────────────

    #[test]
    fn the_vocabulary_round_trips_as_an_array_of_tables() {
        let mut config = sample();
        config.alias_mut("POS").engine = Some("postgres".to_string());
        config.alias_mut("MSQ").engine = Some("sqlserver".to_string());
        config.alias_mut("CONSEGNE").role = Some("update".to_string());

        let text = toml::to_string_pretty(&config).expect("serialises");
        assert!(text.contains("[[alias]]"), "{text}");
        assert!(text.contains(r#"name = "POS""#), "{text}");
        assert_eq!(ProjectConfig::parse(&text).expect("parses"), config);
    }

    #[test]
    fn a_repository_that_declares_no_vocabulary_writes_none() {
        // The file has to stay readable as "the handful of decisions this
        // repository embodies"; an empty table for a feature nobody used is noise.
        let text = toml::to_string_pretty(&sample()).expect("serialises");
        assert!(!text.contains("[[alias]]"), "{text}");
        assert!(ProjectConfig::parse(&text).unwrap().aliases.is_empty());
    }

    #[test]
    fn an_alias_is_the_same_alias_however_it_is_spelled() {
        let mut config = sample();
        config.alias_mut("POS").engine = Some("postgres".to_string());
        // Editing it by another spelling reaches the same entry rather than
        // adding a second one that fights over the same folders.
        config.alias_mut("pos").engine = Some("oracle".to_string());
        assert_eq!(config.aliases.len(), 1);
        assert_eq!(config.alias("POS").unwrap().engine.as_deref(), Some("oracle"));

        assert!(config.remove_alias("Pos"));
        assert!(config.alias("POS").is_none());
        assert!(!config.remove_alias("POS"), "there is nothing left to remove");
    }

    #[test]
    fn tidying_drops_an_alias_that_says_nothing_and_orders_the_rest() {
        let mut config = sample();
        config.alias_mut("POS").engine = Some("postgres".to_string());
        config.alias_mut("MSQ");
        config.alias_mut("CONSEGNE").role = Some("update".to_string());
        assert_eq!(config.aliases.len(), 3);

        config.tidy();
        let names: Vec<&str> = config.aliases.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["CONSEGNE", "POS"]);
    }

    #[test]
    fn a_bad_alias_is_reported_and_the_file_still_opens() {
        // Same contract as `[generation.insertion]`: degrade, report, never fail
        // the parse and reset every other setting in the file.
        let text = r#"
            name = "MINIMAL"
            [[folder]]
            path = "AGGIORNAMENTO"
            role = "update"
            [[alias]]
            name = "MSQ"
            engine = "sqlserver2019"
            [[alias]]
            name = "POS"
            engine = "postgres"
        "#;
        let config = ProjectConfig::parse(text).expect("an unknown engine must not fail the parse");
        // Everything else in the file survived.
        assert_eq!(config.declaration("AGGIORNAMENTO").unwrap().role, Some(FolderRole::Update));

        let problems = config.problems();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("sqlserver2019"), "{problems:?}");

        // The good alias still works; the bad one claims nothing.
        let vocabulary = config.vocabulary();
        assert!(vocabulary.engine("POS").is_some());
        assert!(vocabulary.engine("MSQ").is_none());
    }

    #[test]
    fn a_file_from_a_newer_picus_is_refused_by_name() {
        let mut config = sample();
        config.version = CURRENT_VERSION + 1;
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed = ProjectConfig::parse(&text).unwrap();
        // `load` is what enforces it; assert the condition it checks.
        assert!(parsed.version > CURRENT_VERSION);
    }

    #[test]
    fn a_repository_with_no_products_gives_every_folder_the_project_filter() {
        // The ordinary case, and the one that must not pay for this feature.
        let mut config = sample();
        config.version_table.filter = "MODULO = 'CORE'".to_string();
        assert_eq!(config.version_filter_for(None), "MODULO = 'CORE'");
        assert_eq!(config.version_filter_for(Some("PORTALE")), "MODULO = 'CORE'");
    }

    #[test]
    fn a_declared_product_supplies_its_own_row() {
        let mut config = sample();
        config.version_table.filter = "MODULO = 'CORE'".to_string();
        config.products = vec![ProductSettings {
            name: "Portale".to_string(),
            version_filter: "MODULO = 'PORTALE'".to_string(),
        }];
        // Matched however it was spelled on the folder — the name is written by
        // hand in two places and must not have to agree letter for letter.
        assert_eq!(config.version_filter_for(Some("PORTALE")), "MODULO = 'PORTALE'");
        assert_eq!(config.version_filter_for(Some("  portale ")), "MODULO = 'PORTALE'");
        assert_eq!(config.version_filter_for(None), "MODULO = 'CORE'");
    }

    #[test]
    fn a_product_may_declare_that_it_reads_the_only_row() {
        // An empty filter on a product is an answer, not an omission: it says this
        // product's table is not shared, under a project whose default filters.
        let mut config = sample();
        config.version_table.filter = "MODULO = 'CORE'".to_string();
        config.products =
            vec![ProductSettings { name: "LEGACY".to_string(), version_filter: String::new() }];
        assert_eq!(config.version_filter_for(Some("LEGACY")), "");
    }

    #[test]
    fn a_folder_naming_a_product_nobody_declared_is_reported() {
        let mut config = sample();
        config.declaration_mut("AGGIORNAMENTO").product = Some("PORTALE".to_string());
        let problems = config.problems();
        assert!(
            problems.iter().any(|p| p.contains("PORTALE") && p.contains("AGGIORNAMENTO")),
            "{problems:?}"
        );
    }

    fn set(name: &str, folders: &[&str]) -> DestinationSet {
        DestinationSet {
            name: name.to_string(),
            entries: folders
                .iter()
                .map(|f| DestinationEntry { folder: f.to_string(), ..DestinationEntry::default() })
                .collect(),
        }
    }

    #[test]
    fn saving_a_set_under_a_name_that_exists_replaces_it() {
        // "Save as Release" said twice means the second one. Two sets of one name
        // would show the same entry twice in the picker with different contents
        // behind it.
        let mut config = sample();
        assert!(!config.put_destination_set(set("Release", &["A"])));
        assert!(config.put_destination_set(set("release", &["A", "B"])), "matched case-insensitively");
        assert_eq!(config.destination_sets.len(), 1);
        assert_eq!(config.destination_set("RELEASE").unwrap().entries.len(), 2);
        // The name is stored as it was last written, not as it was first created.
        assert_eq!(config.destination_sets[0].name, "release");
    }

    #[test]
    fn forgetting_a_set_says_whether_there_was_one() {
        let mut config = sample();
        config.put_destination_set(set("Release", &["A"]));
        assert!(config.remove_destination_set(" release "));
        assert!(!config.remove_destination_set("Release"));
        assert!(config.destination_sets.is_empty());
    }

    #[test]
    fn a_set_survives_a_round_trip_through_the_file() {
        let mut config = sample();
        config.put_destination_set(DestinationSet {
            name: "Release".into(),
            entries: vec![
                // The two shapes that matter: a fixed file, and one left to the
                // naming scheme.
                DestinationEntry {
                    folder: "ORACLE/INIZIALIZZAZIONE".into(),
                    file: Some("parametri.sql".into()),
                    wrap: Some("plain".into()),
                    ..DestinationEntry::default()
                },
                DestinationEntry {
                    folder: "ORACLE/AGGIORNAMENTO".into(),
                    file: None,
                    wrap: Some("block".into()),
                    version_guard: true,
                    skip_if_present: true,
                    ..DestinationEntry::default()
                },
            ],
        });
        let text = toml::to_string_pretty(&config).unwrap();
        let back = ProjectConfig::parse(&text).expect("parses");
        assert_eq!(back.destination_sets, config.destination_sets);
        // The distinction the whole feature rests on: "no file" must not come back
        // as an empty name.
        assert_eq!(back.destination_sets[0].entries[1].file, None);
    }

    #[test]
    fn products_survive_a_round_trip_through_the_file() {
        let mut config = sample();
        config.products = vec![
            ProductSettings { name: "CORE".into(), version_filter: "MODULO = 'CORE'".into() },
            ProductSettings { name: "PORTALE".into(), version_filter: "MODULO = 'PORTALE'".into() },
        ];
        let text = toml::to_string_pretty(&config).unwrap();
        let back = ProjectConfig::parse(&text).expect("parses");
        assert_eq!(back.products, config.products);
    }
}
