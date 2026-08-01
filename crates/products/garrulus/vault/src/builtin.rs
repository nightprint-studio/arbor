//! The seven note types a new vault starts with.
//!
//! Shipped as TOML text rather than as constructed structs, for one reason that
//! matters: **they are copied into the vault and are then the user's**. What a
//! new vault gets is a folder of ordinary type files, editable, deletable, and
//! syncing to the other machine like everything else. Nothing here is
//! privileged, nothing re-appears after being deleted, and there is no "built-in
//! type" concept anywhere else in the crate to keep in step.
//!
//! Keeping them as text also means the shipped files *are* the schema
//! documentation: what a user reads when they open `bug.toml` to add a field is
//! exactly what was tested.
//!
//! ## The TOML gotcha these files are written around
//!
//! `template` is declared **above** the `[[fields]]` blocks. Written below them
//! it would parse as a key of the last field rather than of the type — TOML has
//! no way to climb back out of an array-of-tables — and the type would silently
//! lose its body template. Same rule for `[match_frontmatter]` and `[layout]`:
//! every plain value comes first.

use std::path::{Path, PathBuf};

use crate::error::{VaultError, VaultResult};
use crate::note_type::NoteType;

/// A bug report. The type the design document works through.
pub const BUG: &str = r##"id     = "bug"
name   = "Bug"
icon   = "bug"
accent = "#f28b82"
folder = "bugs"
naming = "{{date}}-{{slug}}"
match_folder = "bugs/**"

template = """
## Passi per riprodurre
1. {{cursor}}

## Atteso

## Ottenuto

## Note
"""

[match_frontmatter]
type = "bug"

[layout]
panels = ["backlinks", "tasks"]
wide_editor = false

# `values` left empty on purpose: the dropdown offers the applications this vault
# already mentions, and accepts a new one. Nobody should edit a TOML file to file
# a bug against a project they just started.
[[fields]]
key = "app"
label = "Applicazione"
kind = "enum"
required = true

[[fields]]
key = "version"
label = "Versione"
kind = "text"

[[fields]]
key = "severity"
label = "Gravità"
kind = "enum"
values = ["blocker", "major", "minor", "cosmetic"]
default = "major"

[[fields]]
key = "status"
label = "Stato"
kind = "enum"
values = ["aperto", "in corso", "risolto", "non riproducibile"]
default = "aperto"
board = true

[[fields]]
key = "commit"
label = "Commit"
kind = "code_link"
"##;

/// An improvement to an application — the second half of "bug reports and
/// application improvements" in the brief.
pub const IMPROVEMENT: &str = r##"id     = "improvement"
name   = "Miglioramento"
icon   = "sparkles"
accent = "#8fce6a"
folder = "miglioramenti"
naming = "{{slug}}"
match_folder = "miglioramenti/**"

template = """
## Problema
{{cursor}}

## Proposta

## Perché vale

## Note
"""

[match_frontmatter]
type = "improvement"

[layout]
panels = ["backlinks"]
wide_editor = false

[[fields]]
key = "app"
label = "Applicazione"
kind = "enum"
required = true

[[fields]]
key = "area"
label = "Area"
kind = "text"

[[fields]]
key = "impact"
label = "Impatto"
kind = "enum"
values = ["alto", "medio", "basso"]
default = "medio"

[[fields]]
key = "status"
label = "Stato"
kind = "enum"
values = ["proposta", "in corso", "fatta", "scartata"]
default = "proposta"
board = true
"##;

/// A game-design note. Read as prose, so it opens wide.
pub const GAMEDESIGN: &str = r##"id     = "gamedesign"
name   = "Game design"
icon   = "gamepad-2"
accent = "#b58cf0"
folder = "design"
naming = "{{slug}}"
match_folder = "design/**"

template = """
## Pilastro
{{cursor}}

## Meccanica

## Riferimenti

## Rischi
"""

[match_frontmatter]
type = "gamedesign"

[layout]
panels = ["graph", "outline"]
wide_editor = true

[[fields]]
key = "game"
label = "Gioco"
kind = "enum"
required = true

[[fields]]
key = "pillar"
label = "Pilastro"
kind = "text"

[[fields]]
key = "status"
label = "Stato"
kind = "enum"
values = ["idea", "in corso", "validata", "scartata"]
default = "idea"
board = true
"##;

/// The daily note. Named after its day, and append-merged rather than
/// three-way-merged when two machines both wrote one.
pub const DAILY: &str = r##"id     = "daily"
name   = "Diario"
icon   = "calendar-days"
accent = "#e8a857"
folder = "daily"
naming = "{{date}}"
match_folder = "daily/**"

template = """
## Fatto

## Da fare
- [ ] {{cursor}}

## Note
"""

[match_frontmatter]
type = "daily"

[layout]
panels = ["tasks"]
wide_editor = false

[[fields]]
key = "date"
label = "Data"
kind = "date"
required = true
"##;

/// Meeting notes. The type whose output is mostly tasks.
pub const MEETING: &str = r##"id     = "meeting"
name   = "Riunione"
icon   = "users"
accent = "#4fbfa8"
folder = "riunioni"
naming = "{{date}}-{{slug}}"
match_folder = "riunioni/**"

template = """
## Ordine del giorno
{{cursor}}

## Discussione

## Decisioni

## Azioni
- [ ]
"""

[match_frontmatter]
type = "meeting"

[layout]
panels = ["tasks", "backlinks"]
wide_editor = false

[[fields]]
key = "date"
label = "Data"
kind = "date"
required = true

[[fields]]
key = "project"
label = "Progetto"
kind = "enum"

[[fields]]
key = "attendees"
label = "Partecipanti"
kind = "tags"
"##;

/// An architecture decision record: context, decision, consequences.
pub const DECISION: &str = r##"id     = "decision"
name   = "Decisione (ADR)"
icon   = "gavel"
accent = "#7c9cf5"
folder = "decisioni"
naming = "{{date}}-{{slug}}"
match_folder = "decisioni/**"

template = """
## Contesto
{{cursor}}

## Decisione

## Conseguenze
"""

[match_frontmatter]
type = "decision"

[layout]
panels = ["backlinks"]
wide_editor = false

[[fields]]
key = "status"
label = "Stato"
kind = "enum"
values = ["proposta", "accettata", "superata", "rifiutata"]
default = "proposta"
board = true

[[fields]]
key = "app"
label = "Applicazione"
kind = "enum"

[[fields]]
key = "supersedes"
label = "Sostituisce"
kind = "link"

[[fields]]
key = "commit"
label = "Commit"
kind = "code_link"
"##;

/// A piece of code worth keeping, with where it came from.
pub const SNIPPET: &str = r##"id     = "snippet"
name   = "Snippet"
icon   = "code"
accent = "#6fb3d9"
folder = "snippet"
naming = "{{slug}}"
match_folder = "snippet/**"

template = """
```
{{cursor}}
```

## Quando serve
"""

[match_frontmatter]
type = "snippet"

[layout]
panels = ["backlinks"]
wide_editor = false

[[fields]]
key = "lang"
label = "Linguaggio"
kind = "enum"
required = true

[[fields]]
key = "source"
label = "Origine"
kind = "code_link"
"##;

/// Every shipped type, as `(file stem, TOML text)`.
///
/// The stem is what the file is called under `types/`; it is also the type's id,
/// and the test below is what keeps those two from drifting.
pub const BUILTIN_TYPE_FILES: &[(&str, &str)] = &[
    ("bug", BUG),
    ("daily", DAILY),
    ("decision", DECISION),
    ("gamedesign", GAMEDESIGN),
    ("improvement", IMPROVEMENT),
    ("meeting", MEETING),
    ("snippet", SNIPPET),
];

/// Parse the shipped types.
///
/// Returns a `Result` rather than unwrapping, even though the input is a
/// compile-time constant a test already parses: a panic inside a headless backend
/// takes the whole product's window with it, and "the built-in types are broken"
/// is a sentence a user can act on.
pub fn builtin_types() -> VaultResult<Vec<NoteType>> {
    BUILTIN_TYPE_FILES
        .iter()
        .map(|(stem, text)| {
            NoteType::parse(text)
                .map_err(|e| VaultError::malformed(PathBuf::from(format!("{stem}.toml")), e))
        })
        .collect()
}

/// Copy the shipped types into `<root>/.arbor/garrulus/types/`.
///
/// A type file that is already there is **left alone**: the user has edited it,
/// or deleted a field on purpose, and re-installing over the top would undo that
/// on the next vault open. Returns the files actually written.
pub fn install_builtin_types(root: &Path) -> VaultResult<Vec<PathBuf>> {
    let dir = crate::config::types_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| VaultError::io(&dir, e))?;

    let mut written = Vec::new();
    for (stem, text) in BUILTIN_TYPE_FILES {
        let path = dir.join(format!("{stem}.toml"));
        if path.exists() {
            continue;
        }
        std::fs::write(&path, text).map_err(|e| VaultError::io(&path, e))?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_type::{FieldKind, TypeId};

    #[test]
    fn all_seven_shipped_types_parse() {
        let types = builtin_types().expect("the shipped types are valid TOML");
        assert_eq!(types.len(), 7);
    }

    #[test]
    fn every_file_stem_is_its_type_id() {
        for (stem, text) in BUILTIN_TYPE_FILES {
            let parsed = NoteType::parse(text).expect("shipped type parses");
            assert_eq!(parsed.id, TypeId::new(stem), "{stem}.toml declares a different id");
        }
    }

    #[test]
    fn the_bug_type_carries_the_fields_the_design_calls_for() {
        let bug = NoteType::parse(BUG).expect("bug parses");
        let keys: Vec<&str> = bug.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["app", "version", "severity", "status", "commit"]);
        assert_eq!(bug.board_field().map(|f| f.key.as_str()), Some("status"));
        assert_eq!(bug.field("commit").map(|f| f.kind), Some(FieldKind::CodeLink));
        assert_eq!(bug.field("severity").and_then(|f| f.default.as_deref()), Some("major"));
        // Left open deliberately — see the comment in the shipped file.
        assert!(bug.field("app").is_some_and(|f| f.values.is_empty()));
    }

    #[test]
    fn a_template_declared_after_the_fields_would_have_been_lost() {
        // The regression this asserts is silent: TOML would have attached
        // `template` to the last `[[fields]]` entry and the type would still
        // parse, with an empty body.
        for (stem, text) in BUILTIN_TYPE_FILES {
            let parsed = NoteType::parse(text).expect("shipped type parses");
            assert!(!parsed.template.trim().is_empty(), "{stem}.toml lost its template");
        }
    }

    #[test]
    fn every_shipped_type_recognises_its_own_notes_both_ways() {
        for (stem, text) in BUILTIN_TYPE_FILES {
            let parsed = NoteType::parse(text).expect("shipped type parses");
            assert_eq!(
                parsed.match_frontmatter.get("type").map(String::as_str),
                Some(*stem),
                "{stem}.toml does not match its own frontmatter"
            );
            assert!(parsed.match_folder.is_some(), "{stem}.toml claims no folder");
            assert!(!parsed.folder.is_empty(), "{stem}.toml files new notes nowhere");
        }
    }
}
