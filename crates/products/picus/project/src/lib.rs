//! `picus-project` — what a repository of SQL scripts *is*.
//!
//! Picus's second half maintains the install/upgrade scripts a database is built
//! from, where the same logical change lives twice: once in the Oracle branch,
//! once in the PostgreSQL one. This crate is the part that knows the shape of such
//! a repository — which folders are branches, which engine each speaks, what every
//! folder is for, what encoding its files are in, and how its update files are
//! named.
//!
//! ## The invariant it exists to serve
//!
//! **The dialect is a property of the folder.** Not of a toolbar, not of a
//! session, not of an open connection. [`tree::Branch`] carries it, and it is an
//! `Option`: a branch nobody could identify has no dialect and receives no
//! generated SQL, because guessing wrong writes Oracle syntax into a PostgreSQL
//! file — the exact failure this product exists to catch.
//!
//! ## Modules
//!
//! | Module | Holds |
//! |---|---|
//! | [`tree`] | what is on disk, in the shape the interface renders |
//! | [`config`] | `.arbor/picus/project.toml` — the settings that belong to the repository |
//! | [`discover`] | a pure planner plus a thin filesystem scan |
//! | [`infer`] | guessing a folder's role and a branch's engine, with the evidence |
//! | [`naming`] | how update files are named, and what the next one is called |
//! | [`marker`] | the comment above a generated block, and recognising it again |
//! | [`insertion`] | where a generated block lands, per folder role |
//! | [`version`] | an application version that orders numerically |
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

pub mod config;
pub mod discover;
pub mod error;
pub mod infer;
pub mod insertion;
pub mod marker;
pub mod naming;
pub mod prelude;
pub mod tree;
pub mod version;
