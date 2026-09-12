//! The user's abbreviations, offered beside `psf` in completion — see [`crate::abbreviations`].

use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use bennu_templates::prelude::{kind_dir, list_templates, load_template, TemplateKind};

use crate::templates::root;

/// One of the user's abbreviations.
#[derive(Debug, Clone)]
pub(crate) struct LiveTemplate {
    /// The word that expands it: its `bennu.abbrev` when it has one, otherwise its file name.
    ///
    /// The two exist because a file name is a poor trigger word and a good label — `logd` is what
    /// you type, `logger-for-this-class` is what you want to find in a list of thirty templates.
    pub name: String,
    pub description: Option<String>,
    /// Its `bennu.requires`: offered only in a project that meets them.
    pub requires: Vec<String>,
    /// What it writes — `java`, `rs`, `sql` — from its file name, and empty when the name does not
    /// say. It decides which files the abbreviation is offered in.
    pub extension: String,
    pub text: String,
}

/// The user's abbreviations, re-read only when their directory changed.
///
/// Asked on completion, which is every keystroke: a directory listing each time, and the files only
/// when a name or a modification time moved.
pub(crate) fn live_templates() -> Vec<LiveTemplate> {
    type Signature = Vec<(String, Option<SystemTime>)>;
    static CACHE: OnceLock<Mutex<(Signature, Vec<LiveTemplate>)>> = OnceLock::new();
    let root = root();
    let dir = kind_dir(&root, TemplateKind::Live);
    let mut signature: Signature = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| (entry.file_name().to_string_lossy().into_owned(), entry.metadata().and_then(|m| m.modified()).ok()))
        .collect();
    signature.sort();
    let cache = CACHE.get_or_init(|| Mutex::new((Vec::new(), Vec::new())));
    let mut guard = cache.lock().unwrap_or_else(|p| p.into_inner());
    if guard.0 != signature || (signature.is_empty() && !guard.1.is_empty()) {
        // Starters are not abbreviations: listing with no built-ins leaves only the user's own files.
        guard.1 = list_templates(&root, TemplateKind::Live, &[])
            .into_iter()
            .filter_map(|info| {
                let template = load_template(&root, TemplateKind::Live, &info.name, &[]).ok()?;
                Some(LiveTemplate {
                    name: info.abbrev.clone().unwrap_or(info.name),
                    description: info.description,
                    requires: info.requires,
                    extension: info.extension,
                    text: template.text,
                })
            })
            .collect();
        guard.0 = signature;
    }
    guard.1.clone()
}
