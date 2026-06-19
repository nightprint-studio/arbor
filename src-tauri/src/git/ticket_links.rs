//! `ticket_links` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The git/regex/TOML logic moved into [`corvus_git::tickets`] (so the headless
//! `corvus-be` shares it). This module keeps the original shell-facing API —
//! same signatures, `AppError` results, same `crate::git::ticket_links::*` paths
//! — so the in-process consumers (the ticket-link IPC handlers in
//! `ipc/corvus/tickets.rs`, plus the config structs that key off `StorageBackend`
//! and `AppState` which owns the `TicketLinkCache`) are untouched.
//!
//! This domain has NO git-CLI calls and takes NO recovery snapshot, so there is
//! nothing to inject — each wrapper just delegates and maps the crate error to
//! `AppError` via `?` (`From<GitError>` is implemented in `crate::error`).

use std::path::Path;

use git2::Repository;
use regex::Regex;

use crate::error::Result;

// Re-export the data types / const so existing `crate::git::ticket_links::*`
// paths (config structs, AppState cache field, IPC handlers) keep resolving.
pub use corvus_git::prelude::{
    LinkSource, StorageBackend, TicketLink, TicketLinkCache, TicketLinkConfig, NOTES_REF,
};

pub fn parse_text(text: &str, tracker: &str, source: LinkSource, custom_re: Option<&Regex>) -> Vec<TicketLink> {
    corvus_git::tickets::parse_text(text, tracker, source, custom_re)
}

pub fn read_git_notes(repo: &Repository, sha: &str) -> Result<Vec<TicketLink>> {
    Ok(corvus_git::tickets::read_git_notes(repo, sha)?)
}

pub fn write_git_notes(repo: &Repository, sha: &str, links: &[TicketLink]) -> Result<()> {
    Ok(corvus_git::tickets::write_git_notes(repo, sha, links)?)
}

pub fn check_notes_push_refspec(repo: &Repository) -> bool {
    corvus_git::tickets::check_notes_push_refspec(repo)
}

pub fn read_all_toml_links(workdir: &Path) -> Result<std::collections::HashMap<String, Vec<TicketLink>>> {
    Ok(corvus_git::tickets::read_all_toml_links(workdir)?)
}

pub fn add_toml_link(workdir: &Path, sha: &str, ticket_id: &str, tracker: &str) -> Result<()> {
    Ok(corvus_git::tickets::add_toml_link(workdir, sha, ticket_id, tracker)?)
}

pub fn remove_toml_link(workdir: &Path, sha: &str, ticket_id: &str) -> Result<()> {
    Ok(corvus_git::tickets::remove_toml_link(workdir, sha, ticket_id)?)
}
