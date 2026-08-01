//! `garrulus-vault` — what a vault *is*.
//!
//! A Garrulus vault is a folder of `.md` files with YAML frontmatter, and that is
//! not an implementation detail: it is the product's escape hatch. The same
//! folder opens in Obsidian, so nothing a user writes is ever trapped inside
//! Arbor. Everything this crate adds — note types, templates, filename patterns,
//! a recoverable trash — is written *beside* the notes, under a single
//! `<vault>/.arbor/garrulus/` directory, and the notes themselves stay ordinary
//! markdown.
//!
//! ## The invariant this crate exists to serve
//!
//! **The vault on disk is the record.** Nothing here is a database. An index may
//! be built over what this crate reads ([`garrulus_index`](../garrulus_index)),
//! but it is a cache: delete it and it rebuilds. If this crate and the disk ever
//! disagree, the disk is right.
//!
//! ## The one dot-folder
//!
//! `<vault>/.arbor/garrulus/` — **not** `.garrulus/`. One dot-folder per project
//! across the whole suite, namespaced per product inside it, so a vault that is
//! also a Corvus repository and a Picus project still has exactly one dot-folder
//! to inspect, back up or delete.
//!
//! | Path | Holds |
//! |---|---|
//! | `.arbor/garrulus/vault.toml` | vault-scoped settings ([`config`]) |
//! | `.arbor/garrulus/types/*.toml` | note types + templates ([`note_type`], [`builtin`]) |
//! | `.arbor/garrulus/trash/` | deleted notes, recoverable without digging in git ([`trash`]) |
//!
//! Types living *inside* the vault is deliberate: they sync with it, so on the
//! second PC the templates are already there.
//!
//! ## Modules
//!
//! | Module | Holds |
//! |---|---|
//! | [`discovery`] | finding, opening and creating a vault; the note scan |
//! | [`config`] | `.arbor/garrulus/vault.toml` and where everything lives |
//! | [`note_type`] | a note type, its fields, and which type an existing note *is* |
//! | [`builtin`] | the seven types a new vault starts with, as TOML |
//! | [`note`] | a parsed note: title, frontmatter, links, tags, tasks, outline |
//! | [`extract`] | the single AST walk those four lists come out of |
//! | [`template`] | `{{title}}` / `{{date}}` / `{{slug}}` / `{{cursor}}` expansion |
//! | [`naming`] | a naming pattern to a filename that is legal on every platform |
//! | [`trash`] | move a note aside, list what is aside, put it back |
//! | [`path`] | vault-relative paths and the folder globs matched against them |
//! | [`error`] | what can go wrong, phrased for the person who has to fix it |
//!
//! ## Two rules the whole crate is built around
//!
//! * **Pure first, I/O at the edge.** Classification, expansion, naming and glob
//!   matching take strings and return strings; only [`discovery`], [`config`],
//!   [`note::read_note`] and [`trash`] touch a filesystem. That is what makes the
//!   interesting half testable without a temp directory.
//! * **Never reformat what the user wrote.** Frontmatter round-trips through
//!   [`garrulus_ast`] byte-stable when untouched. A vault that turns every note
//!   into a diff the first time it is opened has destroyed its own sync history.
//!
//! ## Public API: use the [`prelude`]

pub mod builtin;
pub mod config;
pub mod discovery;
pub mod error;
pub mod extract;
pub mod naming;
pub mod note;
pub mod note_type;
pub mod path;
pub mod prelude;
pub mod template;
pub mod trash;
