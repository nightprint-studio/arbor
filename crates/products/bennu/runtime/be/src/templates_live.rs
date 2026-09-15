//! The user's templates that completion offers as they type: abbreviations beside `psf`
//! ([`crate::abbreviations`]) and postfix templates beside `for` ([`crate::postfix_templates`]).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use bennu_templates::prelude::{kind_dir, list_templates, load_template, TemplateKind};

use crate::templates::root;

/// One of the user's abbreviations or postfix templates.
#[derive(Debug, Clone)]
pub(crate) struct UserTemplate {
    /// What is typed to get it. For an abbreviation, its `bennu.abbrev` when it has one, otherwise its file
    /// name; for a postfix template, its file name.
    ///
    /// An abbreviation has the two because a file name is a poor trigger word and a good label — `logd` is
    /// what you type, `logger-for-this-class` is what you want to find in a list of thirty templates.
    pub name: String,
    pub description: Option<String>,
    /// Its `bennu.requires`: offered only in a project that meets them.
    pub requires: Vec<String>,
    /// What it writes — `java`, `rs`, `sql` — from its file name, and empty when the name does not
    /// say. It decides which files an abbreviation is offered in.
    pub extension: String,
    pub text: String,
}

/// The user's templates of `kind`, re-read only when their directory changed.
///
/// Asked on completion, which is every keystroke: a directory listing each time, and the files only
/// when a name or a modification time moved.
pub(crate) fn user_templates(kind: TemplateKind) -> Vec<UserTemplate> {
    type Signature = Vec<(String, Option<SystemTime>)>;
    type Cached = (Signature, Vec<UserTemplate>);
    static CACHE: OnceLock<Mutex<HashMap<TemplateKind, Cached>>> = OnceLock::new();
    let root = root();
    let dir = kind_dir(&root, kind);
    let mut signature: Signature = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| (entry.file_name().to_string_lossy().into_owned(), entry.metadata().and_then(|m| m.modified()).ok()))
        .collect();
    signature.sort();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|p| p.into_inner());
    let cached = guard.entry(kind).or_default();
    if cached.0 != signature || (signature.is_empty() && !cached.1.is_empty()) {
        // Starters are not offered: listing with no built-ins leaves only the user's own files.
        cached.1 = list_templates(&root, kind, &[])
            .into_iter()
            .filter_map(|info| {
                let template = load_template(&root, kind, &info.name, &[]).ok()?;
                let name = match kind {
                    TemplateKind::Live => info.abbrev.clone().unwrap_or(info.name),
                    _ => info.name,
                };
                Some(UserTemplate {
                    name,
                    description: info.description,
                    requires: info.requires,
                    extension: info.extension,
                    text: template.text,
                })
            })
            .collect();
        cached.0 = signature;
    }
    cached.1.clone()
}
