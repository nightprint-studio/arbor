//! A note: a `.md` file, read into the four lists everything else is built on.
//!
//! The struct is deliberately a *view* of a file, not a handle to one. Nothing
//! here holds the document open, caches it, or is authoritative about it — the
//! file on disk is the record, and a [`Note`] is what the last read of it said.
//! Which is what makes the index rebuildable and the whole product safe to have
//! a second copy of on another machine.
//!
//! ## Serialisation is one-way
//!
//! [`Note`] and its parts derive `Serialize` and **not** `Deserialize`: they are
//! report types that cross the seam outwards, to the interface and to plugins.
//! Nothing sends a note back — a change to a note is a write of markdown, not a
//! deserialisation of this struct — and a type that cannot be received is a type
//! that can never be received *stale*.

use std::path::Path;

use garrulus_ast::prelude::{FrontValue, Frontmatter, Reader, Span};
use garrulus_parse::prelude::MarkdownReader;
use serde::{Deserialize, Serialize};

use crate::error::{VaultError, VaultResult};
use crate::extract::extract;
use crate::note_type::{classify, NoteType, TypeId};
use crate::path::RelPath;

/// The stable identity of a note.
///
/// The vault-relative path by default, or the `uid` the frontmatter declares.
/// A uid survives a rename and a move, which is why a note that expects to be
/// linked from elsewhere is worth giving one; the path is the honest default for
/// everything else, because it is what `[[wikilinks]]` actually resolve against.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NoteId(String);

impl NoteId {
    /// Wrap an identity.
    pub fn new(raw: impl Into<String>) -> NoteId {
        NoteId(raw.into())
    }

    /// The identity as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NoteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for NoteId {
    fn from(value: &str) -> Self {
        NoteId(value.to_string())
    }
}

impl From<String> for NoteId {
    fn from(value: String) -> Self {
        NoteId(value)
    }
}

impl From<&RelPath> for NoteId {
    fn from(value: &RelPath) -> Self {
        NoteId(value.as_str().to_string())
    }
}

impl AsRef<str> for NoteId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for NoteId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// One outgoing edge from a note.
///
/// Wikilinks, markdown links and embedded images are all one type on purpose:
/// the backlink panel, the unresolved-link check and the attachment GC all ask
/// "what does this note point at", and three near-identical structs would mean
/// three places to fix when the answer changes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Link {
    /// The note title, path or URL as written.
    pub target: String,
    /// The `#heading` part of `[[Nota#Sezione]]`.
    pub heading: Option<String>,
    /// The `|alias` part, or an image's alt text.
    pub alias: Option<String>,
    /// `![[…]]` or `![](…)` — the content is meant to appear inline.
    pub embed: bool,
    /// The target is outside the vault, so it is never an unresolved link.
    pub external: bool,
    /// Where it sits in the source, for jump-to and for rename-with-update.
    pub span: Span,
}

/// A tag on a note.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tag {
    /// Without the `#`. Nested tags keep their slashes: `arbor/corvus`.
    pub name: String,
    /// Where it sits in the body; `None` for a tag declared in frontmatter,
    /// which has no position in the text.
    pub span: Option<Span>,
}

/// A checkbox item.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Task {
    /// `- [x]` rather than `- [ ]`.
    pub done: bool,
    /// The first line of the item.
    pub text: String,
    /// The heading it sits under, which is what makes a vault-wide task list
    /// readable instead of a flat wall of checkboxes.
    pub heading: Option<String>,
    /// Where the item sits in the source.
    pub span: Span,
}

/// One entry in a note's outline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Heading {
    /// 1–6.
    pub level: u8,
    /// The heading's plain text.
    pub text: String,
    /// Where it sits in the source.
    pub span: Span,
}

/// A note, as of the last time it was read.
#[derive(Debug, Clone, Serialize)]
pub struct Note {
    /// Stable identity — see [`NoteId`].
    pub id: NoteId,
    /// Where it lives, relative to the vault root.
    pub path: RelPath,
    /// Frontmatter `title`, else the first `#` heading, else the file name.
    pub title: String,
    /// The YAML header, preserved so it round-trips byte-stable when untouched.
    pub frontmatter: Frontmatter,
    /// Which type it is, per [`crate::note_type::classify`]. `None` is a real
    /// answer, not a failure.
    pub kind: Option<TypeId>,
    /// What it points at.
    pub links: Vec<Link>,
    /// Its tags, body and frontmatter together.
    pub tags: Vec<Tag>,
    /// Its checkboxes.
    pub tasks: Vec<Task>,
    /// Its outline.
    pub headings: Vec<Heading>,
}

impl Note {
    /// The tag names, deduplicated, in first-seen order — what the tag panel and
    /// the index want, as opposed to every occurrence with its position.
    pub fn tag_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::with_capacity(self.tags.len());
        for tag in &self.tags {
            if !out.iter().any(|seen| seen == &tag.name) {
                out.push(tag.name.clone());
            }
        }
        out
    }

    /// Tasks still open. The number the sidebar shows.
    pub fn open_tasks(&self) -> usize {
        self.tasks.iter().filter(|task| !task.done).count()
    }
}

/// Read one frontmatter value as a string.
///
/// **Every read of a frontmatter value in this crate goes through here**, so the
/// coupling to [`garrulus_ast`]'s frontmatter representation is one function
/// wide. A `Map` value has no string form and yields `None` rather than a
/// debug-formatted approximation of one.
pub fn front_scalar(frontmatter: &Frontmatter, key: &str) -> Option<String> {
    frontmatter.get(key).and_then(scalar_of)
}

/// Read one frontmatter value as a list of strings.
///
/// A single scalar counts as a one-element list, because `tags: bug` and
/// `tags: [bug]` mean the same thing to the person who wrote them.
pub fn front_list(frontmatter: &Frontmatter, key: &str) -> Vec<String> {
    match frontmatter.get(key) {
        Some(FrontValue::List(items)) => items.iter().filter_map(scalar_of).collect(),
        Some(other) => scalar_of(other).into_iter().collect(),
        None => Vec::new(),
    }
}

fn scalar_of(value: &FrontValue) -> Option<String> {
    match value {
        FrontValue::Str(text) => Some(text.clone()),
        // `3.0` reads back as `3`: a version or a count written as a YAML number
        // must not acquire a decimal point on its way to a filter.
        FrontValue::Num(n) if n.fract() == 0.0 && n.is_finite() => Some(format!("{}", *n as i64)),
        FrontValue::Num(n) => Some(n.to_string()),
        FrontValue::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Frontmatter `tags:`, normalised the way inline tags are.
///
/// Accepts a list, or one comma-separated string, and strips a leading `#` —
/// Obsidian vaults contain all three spellings and they all mean the same tag.
pub fn front_tags(frontmatter: &Frontmatter) -> Vec<String> {
    front_list(frontmatter, "tags")
        .iter()
        .flat_map(|entry| entry.split(',').map(str::trim).map(str::to_string).collect::<Vec<_>>())
        .map(|entry| entry.trim_start_matches('#').to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// Build a note from its source text.
///
/// Pure: no filesystem, no clock. The whole of the interesting work — the title
/// rule, the identity rule, the four lists, the type — is testable from a string
/// literal.
pub fn parse_note(path: &RelPath, source: &str, types: &[NoteType]) -> VaultResult<Note> {
    let document = MarkdownReader
        .read(source)
        .map_err(|e| VaultError::Parse { path: path.clone(), reason: e.to_string() })?;

    let extracted = extract(&document);
    let frontmatter = document.frontmatter;

    let mut tags = extracted.tags;
    for name in front_tags(&frontmatter) {
        tags.push(Tag { name, span: None });
    }

    let title = front_scalar(&frontmatter, "title")
        .filter(|t| !t.trim().is_empty())
        .or(extracted.first_h1)
        .unwrap_or_else(|| path.stem().to_string());

    let id = front_scalar(&frontmatter, "uid")
        .filter(|uid| !uid.trim().is_empty())
        .map(NoteId::new)
        .unwrap_or_else(|| NoteId::from(path));

    let mut note = Note {
        id,
        path: path.clone(),
        title,
        frontmatter,
        kind: None,
        links: extracted.links,
        tags,
        tasks: extracted.tasks,
        headings: extracted.headings,
    };
    note.kind = classify(&note, types);
    Ok(note)
}

/// Read a note off disk and parse it.
///
/// The one I/O function in this module. A note that is not valid UTF-8 is an
/// error rather than a lossy decode: the vault is UTF-8 by construction, and a
/// file that is not is a file that did not come from here.
pub fn read_note(root: &Path, path: &RelPath, types: &[NoteType]) -> VaultResult<Note> {
    if path.escapes() {
        return Err(VaultError::BadPath {
            raw: path.as_str().to_string(),
            reason: "it points outside the vault".to_string(),
        });
    }
    let absolute = path.to_path(root);
    if !absolute.is_file() {
        return Err(VaultError::NoteMissing { path: path.clone() });
    }
    let source = arbor_fs::prelude::read::read_text(crate::path::path_str(&absolute)?)
        .map_err(|e| VaultError::io(&absolute, e))?;
    parse_note(path, &source, types)
}

/// Write a note's source text, creating the folder it lives in.
///
/// Refuses to leave the vault, and refuses to overwrite when `overwrite` is
/// false. There is no third option: this product never silently replaces text
/// somebody typed.
pub fn write_note(root: &Path, path: &RelPath, source: &str, overwrite: bool) -> VaultResult<()> {
    if path.escapes() {
        return Err(VaultError::BadPath {
            raw: path.as_str().to_string(),
            reason: "it points outside the vault".to_string(),
        });
    }
    let absolute = path.to_path(root);
    if !overwrite && absolute.exists() {
        return Err(VaultError::NoteExists { path: path.clone() });
    }
    arbor_fs::prelude::mutate::write_text(crate::path::path_str(&absolute)?, source)
        .map_err(|e| VaultError::io(&absolute, e))
}
