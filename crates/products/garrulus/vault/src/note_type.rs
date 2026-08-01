//! A **note type** — the thing that makes this more than an Obsidian clone.
//!
//! A type is a first-class object, not a template file you copy from. It says
//! what a bug report *is*: which fields it has and what they may contain, where
//! new ones land, what they are called, what the body starts as, and which panels
//! open with it. One `.toml` per type under `<vault>/.arbor/garrulus/types/`, so
//! the types travel with the vault to the second machine.
//!
//! On disk a typed note is still ordinary markdown with ordinary YAML
//! frontmatter. The type is what turns `severity: major` into a dropdown, a
//! column in a table view, a lane on a board and a filter axis in search — none
//! of which costs the file its Obsidian compatibility.
//!
//! ## Recognising an existing note
//!
//! Two rules, in this order, and the order is the whole contract:
//!
//! 1. **Frontmatter wins.** `type: bug`, or every pair in the type's
//!    `match_frontmatter` matching. This is what the user *said*.
//! 2. **Then the folder.** `match_folder = "bugs/**"`. This is where the note
//!    *is*.
//!
//! A note that says `type: decision` while sitting in `bugs/` is a decision. The
//! reverse — letting the folder override the frontmatter — would mean moving a
//! file silently retypes it, and a user who wrote the type down meant it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{VaultError, VaultResult};
use crate::note::{front_scalar, Note};
use crate::path::glob_matches;

/// The id of a note type — the `type:` a note writes in its frontmatter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TypeId(String);

impl TypeId {
    /// Wrap an id. Trimmed and lowercased: `type: Bug` and `type: bug` are the
    /// same type, because a person typing frontmatter by hand will write both.
    pub fn new(raw: impl AsRef<str>) -> TypeId {
        TypeId(raw.as_ref().trim().to_lowercase())
    }

    /// The id as it is written in frontmatter and in a file name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for TypeId {
    fn from(value: &str) -> Self {
        TypeId::new(value)
    }
}

impl From<String> for TypeId {
    fn from(value: String) -> Self {
        TypeId::new(value)
    }
}

impl AsRef<str> for TypeId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for TypeId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// What kind of value a field holds, and therefore how it is edited, filtered
/// and sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// Free text, one line.
    #[default]
    Text,
    /// A number, sorted numerically rather than as a string.
    Number,
    /// A checkbox.
    Bool,
    /// `yyyy-MM-dd`.
    Date,
    /// One of a fixed set — see [`FieldSpec::values`] for the open case.
    Enum,
    /// A list of tags, sharing the vault's tag vocabulary.
    Tags,
    /// A link to another note.
    Link,
    /// A link into a Corvus repository — a commit, a branch, a file and line.
    /// The one field kind no other note app can offer, because no other note app
    /// is in the same process tree as your git client.
    CodeLink,
}

/// One frontmatter field a type declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FieldSpec {
    /// The frontmatter key. Lowercase, no spaces — it is YAML.
    pub key: String,
    /// What the form calls it. The user's language, not the key's.
    pub label: String,
    /// How it is edited and compared.
    #[serde(default)]
    pub kind: FieldKind,
    /// The options, for [`FieldKind::Enum`].
    ///
    /// **Empty means open**: the dropdown offers the values already used by notes
    /// of this type in this vault, and accepts a new one. That is how a field like
    /// *Applicazione* works — the list is the set of applications you actually
    /// have, and nobody wants to edit a TOML file to add the next one.
    #[serde(default)]
    pub values: Vec<String>,
    /// Prefilled when a note of this type is created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Written into the frontmatter even when empty, so the form shows the gap
    /// rather than hiding it.
    #[serde(default)]
    pub required: bool,
    /// This field groups the board view's columns. At most one per type; the
    /// first one wins if a type declares two.
    #[serde(default)]
    pub board: bool,
}

/// Which panels open with a note of this type — the "layouts diversi" half of the
/// ask.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default)]
pub struct NoteLayout {
    /// Panel ids, in the order they should appear. Rendered as declared: the host
    /// does not second-guess a type that asks for one panel.
    pub panels: Vec<String>,
    /// Give the editor the width and put the side panels away — for the note
    /// kinds that are read as prose rather than scanned as records.
    pub wide_editor: bool,
}

/// A note type.
///
/// Field order is load-bearing: `toml` refuses to emit a plain value after a
/// table, so every scalar is declared before `match_frontmatter`, `layout` and
/// `fields`. This is also why the shipped TOML puts `template` **above** the
/// `[[fields]]` blocks — written below them it would parse as a key of the last
/// field, not of the type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NoteType {
    /// Stable id; also the file name under `types/`.
    pub id: TypeId,
    /// What the user sees.
    pub name: String,
    /// A lucide icon name.
    #[serde(default)]
    pub icon: String,
    /// The type's colour, used for state and kind — the tab, the graph node, the
    /// search result, the board lane. Never for decoration.
    #[serde(default)]
    pub accent: String,
    /// Where new notes of this type land. `""` is the vault root.
    #[serde(default)]
    pub folder: String,
    /// The filename pattern, expanded by [`crate::naming::file_name`].
    #[serde(default = "default_naming")]
    pub naming: String,
    /// A glob over the note's vault-relative path. The *second* recognition rule
    /// — frontmatter wins over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_folder: Option<String>,
    /// The body a new note of this type starts with.
    #[serde(default)]
    pub template: String,
    /// Frontmatter pairs that, all together, identify a note as this type. The
    /// *first* recognition rule.
    #[serde(default)]
    pub match_frontmatter: BTreeMap<String, String>,
    /// Which panels open with it.
    #[serde(default)]
    pub layout: NoteLayout,
    /// The frontmatter schema.
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
}

fn default_naming() -> String {
    "{{title}}".to_string()
}

impl NoteType {
    /// A minimal type: an id, a name, and every other decision left at its
    /// default. The starting point for a type built in code, and for tests.
    pub fn new(id: impl AsRef<str>, name: impl Into<String>) -> NoteType {
        NoteType {
            id: TypeId::new(id),
            name: name.into(),
            icon: String::new(),
            accent: String::new(),
            folder: String::new(),
            naming: default_naming(),
            match_folder: None,
            template: String::new(),
            match_frontmatter: BTreeMap::new(),
            layout: NoteLayout::default(),
            fields: Vec::new(),
        }
    }

    /// Parse one type file. Kept separate from [`load_types`] so the schema is
    /// testable without a filesystem.
    pub fn parse(text: &str) -> Result<NoteType, toml::de::Error> {
        toml::from_str(text)
    }

    /// Render one type file.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// The field a board view groups by, if the type declares one.
    pub fn board_field(&self) -> Option<&FieldSpec> {
        self.fields.iter().find(|field| field.board)
    }

    /// The field with this key.
    pub fn field(&self, key: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|field| field.key == key)
    }
}

/// Which type is this note?
///
/// Frontmatter first, folder second — see the module note for why the order is
/// not negotiable. `None` is a real answer: an untyped note is an ordinary note,
/// not a problem, and the interface offers to promote it rather than guessing.
pub fn classify(note: &Note, types: &[NoteType]) -> Option<TypeId> {
    let frontmatter = |key: &str| front_scalar(&note.frontmatter, key);
    classify_with(&frontmatter, note.path.as_str(), types)
}

/// The pure half of [`classify`]: the same two rules, over a frontmatter lookup
/// and a path.
///
/// Split out so the precedence can be tested exhaustively without building a
/// document — and so the one place this crate reaches into a `Frontmatter` is a
/// single closure at the call site above.
pub fn classify_with(
    frontmatter: &dyn Fn(&str) -> Option<String>,
    path: &str,
    types: &[NoteType],
) -> Option<TypeId> {
    // Rule 1a: the note names its type outright.
    if let Some(declared) = frontmatter("type") {
        let declared = TypeId::new(declared);
        if let Some(found) = types.iter().find(|t| t.id == declared) {
            return Some(found.id.clone());
        }
    }

    // Rule 1b: the note matches a type's declared frontmatter signature.
    for candidate in types {
        if candidate.match_frontmatter.is_empty() {
            continue;
        }
        let all_match = candidate
            .match_frontmatter
            .iter()
            .all(|(key, value)| frontmatter(key).as_deref() == Some(value.as_str()));
        if all_match {
            return Some(candidate.id.clone());
        }
    }

    // Rule 2: the note is where a type keeps its notes.
    types
        .iter()
        .find(|candidate| {
            candidate.match_folder.as_deref().is_some_and(|glob| glob_matches(glob, path))
        })
        .map(|candidate| candidate.id.clone())
}

/// Read every `.toml` under `<root>/.arbor/garrulus/types/`, sorted by id.
///
/// A type file that will not parse is **skipped and reported**, not fatal: one
/// bad file must not stop a vault from opening, and the user needs to be told
/// which one it was. The returned errors are the caller's to surface.
pub fn load_types(root: &Path) -> VaultResult<(Vec<NoteType>, Vec<VaultError>)> {
    let dir = crate::config::types_dir(root);
    if !dir.is_dir() {
        return Ok((Vec::new(), Vec::new()));
    }
    let entries = std::fs::read_dir(&dir).map_err(|e| VaultError::io(&dir, e))?;

    let mut types = Vec::new();
    let mut problems = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| VaultError::io(&dir, e))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => match NoteType::parse(&text) {
                Ok(note_type) => types.push(note_type),
                Err(e) => problems.push(VaultError::malformed(&path, e)),
            },
            Err(e) => problems.push(VaultError::io(&path, e)),
        }
    }
    types.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((types, problems))
}

/// Write one type file to `<root>/.arbor/garrulus/types/<id>.toml`.
pub fn save_type(root: &Path, note_type: &NoteType) -> VaultResult<std::path::PathBuf> {
    let dir = crate::config::types_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| VaultError::io(&dir, e))?;
    let path = dir.join(format!("{}.toml", note_type.id));
    let text = note_type.to_toml().map_err(|e| VaultError::malformed(&path, e))?;
    std::fs::write(&path, text).map_err(|e| VaultError::io(&path, e))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bug() -> NoteType {
        NoteType {
            folder: "bugs".into(),
            match_folder: Some("bugs/**".into()),
            ..NoteType::new("bug", "Bug")
        }
    }

    fn decision() -> NoteType {
        NoteType {
            folder: "decisioni".into(),
            match_folder: Some("decisioni/**".into()),
            ..NoteType::new("decision", "Decisione")
        }
    }

    fn meeting_by_signature() -> NoteType {
        let mut t = NoteType::new("meeting", "Riunione");
        t.match_frontmatter.insert("kind".into(), "meeting".into());
        t.match_frontmatter.insert("format".into(), "notes".into());
        t
    }

    fn front(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: BTreeMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |key: &str| owned.get(key).cloned()
    }

    #[test]
    fn frontmatter_beats_the_folder_glob() {
        let types = vec![bug(), decision()];
        // The note sits in `bugs/` and says it is a decision. It is a decision.
        let declared = front(&[("type", "decision")]);
        assert_eq!(
            classify_with(&declared, "bugs/ADR — Model D.md", &types),
            Some(TypeId::new("decision"))
        );
    }

    #[test]
    fn the_folder_answers_when_the_frontmatter_is_silent() {
        let types = vec![bug(), decision()];
        let silent = front(&[]);
        assert_eq!(
            classify_with(&silent, "bugs/Crash all'avvio.md", &types),
            Some(TypeId::new("bug"))
        );
        assert_eq!(classify_with(&silent, "appunti/Spesa.md", &types), None);
    }

    #[test]
    fn a_declared_type_nobody_defines_falls_through_to_the_folder() {
        // `type: idea` matches no loaded type, so the note is still a bug by
        // where it lives — rather than being classified as nothing at all.
        let types = vec![bug()];
        let declared = front(&[("type", "idea")]);
        assert_eq!(classify_with(&declared, "bugs/Crash.md", &types), Some(TypeId::new("bug")));
    }

    #[test]
    fn a_type_id_is_matched_case_insensitively() {
        let types = vec![bug()];
        let declared = front(&[("type", "  Bug ")]);
        assert_eq!(classify_with(&declared, "appunti/x.md", &types), Some(TypeId::new("bug")));
    }

    #[test]
    fn a_frontmatter_signature_needs_every_pair() {
        let types = vec![meeting_by_signature(), bug()];
        let partial = front(&[("kind", "meeting")]);
        assert_eq!(classify_with(&partial, "appunti/x.md", &types), None);
        let complete = front(&[("kind", "meeting"), ("format", "notes")]);
        assert_eq!(
            classify_with(&complete, "appunti/x.md", &types),
            Some(TypeId::new("meeting"))
        );
    }

    #[test]
    fn a_type_round_trips_through_toml() {
        let mut original = bug();
        original.fields.push(FieldSpec {
            key: "severity".into(),
            label: "Gravità".into(),
            kind: FieldKind::Enum,
            values: vec!["major".into(), "minor".into()],
            default: Some("major".into()),
            required: true,
            board: true,
        });
        original.layout.panels = vec!["backlinks".into()];
        let text = original.to_toml().expect("a type serialises");
        assert_eq!(NoteType::parse(&text).expect("and reads back"), original);
    }

    #[test]
    fn the_board_field_is_the_first_one_that_asks() {
        let mut t = bug();
        for key in ["status", "severity"] {
            t.fields.push(FieldSpec {
                key: key.into(),
                label: key.into(),
                kind: FieldKind::Enum,
                values: Vec::new(),
                default: None,
                required: false,
                board: true,
            });
        }
        assert_eq!(t.board_field().map(|f| f.key.as_str()), Some("status"));
    }
}
