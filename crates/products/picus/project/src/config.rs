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
        out.extend(crate::alias::problems(&self.aliases));
        out
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
}
