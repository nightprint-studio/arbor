//! `picus-project` — what a repository of SQL scripts *is*.
//!
//! Picus's second half maintains the install/upgrade scripts a database is built
//! from, where the same logical change lives twice: once in Oracle syntax, once
//! in PostgreSQL's. This crate is the part that knows the shape of such a
//! repository — which folder each dialect lives in, what every folder is for,
//! what encoding its files are in, and how its update files are named.
//!
//! ## The invariant it exists to serve
//!
//! **The dialect is a property of the folder.** Not of a toolbar, not of a
//! session, not of an open connection. Every [`tree::FolderNode`] may declare
//! one, its descendants inherit it, and `effective_dialect` is an `Option`: a
//! folder nobody could identify has no dialect and receives no generated SQL,
//! because guessing wrong writes Oracle syntax into a PostgreSQL file — the exact
//! failure this product exists to catch.
//!
//! There is no "branch" here. The tree is the repository's own directory
//! hierarchy, and a repository is free to put the role at the top and the dialect
//! at the bottom (`AGGIORNAMENTO/2024/ORA`) — which is what real ones do.
//!
//! ## Three engine states
//!
//! "No dialect" is not one answer but two, and they behave differently: a folder
//! **nobody classified** is a question the interface asks, while a folder written
//! in an engine Picus **recognises and does not support** — SQL Server, DB2 — is
//! an answer, and is never asked about, never compared and never parsed. See
//! [`picus_types::prelude::FolderEngine`].
//!
//! ## Modules
//!
//! | Module | Holds |
//! |---|---|
//! | [`tree`] | what is on disk, in the shape the interface renders |
//! | [`resolve`] | inheritance: what a folder declares → what applies to it |
//! | [`config`] | `.arbor/picus/project.toml` — the settings that belong to the repository |
//! | [`legacy`] | reading a `version = 1` file and folding it into declarations |
//! | [`discover`] | a pure planner plus a thin filesystem scan |
//! | [`infer`] | guessing a folder's role and engine from its name, with the evidence |
//! | [`alias`] | folder names that mean something in **this** repository |
//! | [`naming`] | how update files are named, and what the next one is called |
//! | [`marker`] | the comment above a generated block, and recognising it again |
//! | [`insertion`] | where a generated block lands, per folder role |
//! | [`version`] | an application version that orders numerically |
//! | [`path`] | project-relative paths: the parent, the last segment, the ancestry |
//!
//! ## Two rules the whole crate is built around
//!
//! * **Nothing is written without an explicit confirmation.** Discovery produces a
//!   [`discover::Proposal`], not a file. The user sees what Picus concluded and
//!   agrees before anything lands in their repository.
//! * **An existing project file wins over every inference.** A scan never
//!   overwrites what the user decided; it only fills in what the file cannot know.
//!
//! ## Public API: use the [`prelude`]

pub mod alias;
pub mod config;
pub mod discover;
pub mod error;
pub mod infer;
pub mod legacy;
pub mod insertion;
pub mod marker;
pub mod naming;
pub mod path;
pub mod prelude;
pub mod resolve;
pub mod tree;
pub mod version;
