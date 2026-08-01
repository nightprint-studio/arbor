//! What the user is about to send: [`RelPath`], [`ChangeBatch`], and the pure
//! functions that turn a set of changes into a commit message and an authorship
//! line.
//!
//! Message generation lives here rather than in [`crate::git`] because it is the
//! part with an opinion (`docs/garrulus-design.md` §4.2: the history should read
//! as a log of *what happened to the vault*, from *which machine*), and an
//! opinion deserves a test rather than a string literal buried in a subprocess
//! call.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A vault-relative path, always with `/` separators.
///
/// The whole seam speaks these: an absolute path is meaningless on the other
/// machine, and a `\`-separated one is meaningless in a git index.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelPath(String);

impl RelPath {
    /// Normalise into vault-relative form: backslashes become slashes, a
    /// leading `./` and any leading/trailing `/` are dropped.
    pub fn new(raw: impl Into<String>) -> Self {
        let mut s = raw.into().replace('\\', "/");
        while let Some(rest) = s.strip_prefix("./") {
            s = rest.to_string();
        }
        let trimmed = s.trim_matches('/').to_string();
        Self(trimmed)
    }

    /// Build from an absolute path known to live under `root`.
    pub fn from_abs(root: &Path, abs: &Path) -> Option<Self> {
        abs.strip_prefix(root)
            .ok()
            .map(|rel| Self::new(rel.to_string_lossy().to_string()))
    }

    /// The path as stored: vault-relative, `/`-separated.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Absolute path of this note inside `root`.
    pub fn to_path(&self, root: &Path) -> PathBuf {
        let mut p = root.to_path_buf();
        for seg in self.0.split('/').filter(|s| !s.is_empty()) {
            p.push(seg);
        }
        p
    }

    /// Last path segment, extension included.
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// Last path segment without its extension — the note's on-disk title.
    pub fn file_stem(&self) -> &str {
        let name = self.file_name();
        match name.rfind('.') {
            // A leading dot is not an extension separator (`.gitignore`).
            Some(i) if i > 0 => &name[..i],
            _ => name,
        }
    }

    /// Lowercased extension without the dot, if any.
    pub fn extension(&self) -> Option<String> {
        let name = self.file_name();
        name.rfind('.')
            .filter(|i| *i > 0)
            .map(|i| name[i + 1..].to_ascii_lowercase())
    }

    /// Parent folder, vault-relative; `None` at the vault root.
    pub fn parent(&self) -> Option<&str> {
        self.0.rfind('/').map(|i| &self.0[..i])
    }

    /// Is this note inside `folder` (or one of its descendants)?
    pub fn is_in_folder(&self, folder: &str) -> bool {
        let f = folder.trim_matches('/');
        !f.is_empty() && (self.0.starts_with(&format!("{f}/")))
    }

    /// Is this a markdown note (as opposed to an attachment)?
    pub fn is_note(&self) -> bool {
        matches!(self.extension().as_deref(), Some("md") | Some("markdown"))
    }

    /// Replace the file name, keeping the folder.
    pub fn with_file_name(&self, name: &str) -> Self {
        match self.parent() {
            Some(p) => Self::new(format!("{p}/{name}")),
            None => Self::new(name),
        }
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for RelPath {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for RelPath {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// What the user asked to send in one go.
///
/// `message` is `Some` only when the user typed one ("Commit only, with a
/// message" in the sync dropdown); otherwise the engine generates it with
/// [`auto_commit_message`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeBatch {
    /// Notes to send. Empty means "send whatever is already committed".
    pub notes: Vec<RelPath>,
    /// User-written commit message, if any.
    pub message: Option<String>,
}

impl ChangeBatch {
    /// A batch of notes with an auto-generated message.
    pub fn new(notes: Vec<RelPath>) -> Self {
        Self { notes, message: None }
    }

    /// A batch the user wrote a message for.
    pub fn with_message(notes: Vec<RelPath>, message: impl Into<String>) -> Self {
        Self { notes, message: Some(message.into()) }
    }

    /// No explicit note list — **send everything the user has changed**.
    ///
    /// This is the one meaning both implementations obey, and it is the one the
    /// sync button relies on: it never knows which notes are dirty, it just asks
    /// for a sync. A named list narrows the batch to exactly those notes.
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}

/// How a note changed, as far as a commit message cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    /// The note did not exist on the last sync.
    Created,
    /// The note existed and its bytes differ.
    Updated,
    /// The note is gone.
    Deleted,
    /// The note moved, with or without an edit.
    Renamed,
}

/// One changed note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteChange {
    /// Where the note is now (the destination, for a rename).
    pub path: RelPath,
    /// What happened to it.
    pub kind: ChangeKind,
}

impl NoteChange {
    /// A change of `kind` at `path`.
    pub fn new(path: impl Into<RelPath>, kind: ChangeKind) -> Self {
        Self { path: path.into(), kind }
    }
}

/// Parse `git diff --name-status -z`-free output (the plain, tab-separated
/// form) into changes.
///
/// Kept pure and tested because it is the only thing standing between git's
/// status letters and a commit message the user reads every day. Unknown status
/// letters degrade to [`ChangeKind::Updated`] rather than being dropped — a note
/// that changed must never go unmentioned.
pub fn parse_name_status(out: &str) -> Vec<NoteChange> {
    let mut changes = Vec::new();
    for line in out.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        let status = fields.next().unwrap_or("").trim();
        let first = fields.next().unwrap_or("").trim();
        let second = fields.next().map(str::trim);
        if first.is_empty() {
            continue;
        }
        // Rename/copy statuses carry a similarity score (`R100`) and two paths;
        // the destination is what the message should name.
        let (kind, path) = match status.chars().next() {
            Some('A') => (ChangeKind::Created, first),
            Some('D') => (ChangeKind::Deleted, first),
            Some('R') | Some('C') => (ChangeKind::Renamed, second.unwrap_or(first)),
            _ => (ChangeKind::Updated, first),
        };
        changes.push(NoteChange::new(path, kind));
    }
    changes
}

/// Generate the commit message for a set of changes.
///
/// Italian, because it lands in a git log the user reads (§4.2). One changed
/// note names it; several are counted, and the count is the useful part.
pub fn auto_commit_message(changes: &[NoteChange]) -> String {
    if changes.is_empty() {
        return "Aggiornamento vault".to_string();
    }
    if let [only] = changes {
        let title = only.path.file_stem();
        return match only.kind {
            ChangeKind::Created => format!("Nuova nota: {title}"),
            ChangeKind::Updated => format!("Aggiornata nota: {title}"),
            ChangeKind::Deleted => format!("Eliminata nota: {title}"),
            ChangeKind::Renamed => format!("Rinominata nota: {title}"),
        };
    }
    let n = changes.len();
    let all = |k: ChangeKind| changes.iter().all(|c| c.kind == k);
    if all(ChangeKind::Created) {
        format!("Aggiunte {n} note")
    } else if all(ChangeKind::Deleted) {
        format!("Eliminate {n} note")
    } else {
        format!("Aggiornate {n} note")
    }
}

/// Commit identity for automatic commits: `Garrulus (<device>)`.
///
/// Authorship is how the history answers *where was I working* (§4.2), which is
/// the only question a vault log gets asked. The e-mail is synthetic and local —
/// nothing must ever try to deliver to it.
pub fn commit_identity(device: &str) -> (String, String) {
    let slug = slugify_device(device);
    let display = if device.trim().is_empty() {
        "Garrulus".to_string()
    } else {
        format!("Garrulus ({})", device.trim())
    };
    (display, format!("garrulus+{slug}@arbor.local"))
}

/// Reduce a device name to something safe inside an e-mail local part.
pub fn slugify_device(device: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in device.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-').to_string();
    if trimmed.is_empty() {
        "device".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relpath_normalises_separators_and_prefixes() {
        assert_eq!(RelPath::new("bugs\\crash.md").as_str(), "bugs/crash.md");
        assert_eq!(RelPath::new("./bugs/crash.md").as_str(), "bugs/crash.md");
        assert_eq!(RelPath::new("/bugs/crash.md/").as_str(), "bugs/crash.md");
    }

    #[test]
    fn relpath_parts() {
        let p = RelPath::new("bugs/2026-07-31-crash.md");
        assert_eq!(p.file_name(), "2026-07-31-crash.md");
        assert_eq!(p.file_stem(), "2026-07-31-crash");
        assert_eq!(p.extension().as_deref(), Some("md"));
        assert_eq!(p.parent(), Some("bugs"));
        assert!(p.is_in_folder("bugs"));
        assert!(!p.is_in_folder("diario"));
        assert!(p.is_note());
        assert_eq!(RelPath::new("nota.md").parent(), None);
    }

    #[test]
    fn relpath_to_path_joins_segment_by_segment() {
        let root = Path::new("/vault");
        assert_eq!(
            RelPath::new("bugs/crash.md").to_path(root),
            Path::new("/vault").join("bugs").join("crash.md")
        );
    }

    #[test]
    fn name_status_parses_every_letter() {
        let out = "A\tbugs/new.md\nM\tdaily/2026-07-31.md\nD\told.md\nR100\ta.md\tb.md\n";
        let changes = parse_name_status(out);
        assert_eq!(
            changes,
            vec![
                NoteChange::new("bugs/new.md", ChangeKind::Created),
                NoteChange::new("daily/2026-07-31.md", ChangeKind::Updated),
                NoteChange::new("old.md", ChangeKind::Deleted),
                NoteChange::new("b.md", ChangeKind::Renamed),
            ]
        );
    }

    #[test]
    fn name_status_ignores_blank_lines_and_unknown_letters() {
        let changes = parse_name_status("\nT\tlink.md\n");
        assert_eq!(changes, vec![NoteChange::new("link.md", ChangeKind::Updated)]);
    }

    #[test]
    fn single_change_names_the_note() {
        let one = [NoteChange::new("bugs/crash-all-avvio.md", ChangeKind::Created)];
        assert_eq!(auto_commit_message(&one), "Nuova nota: crash-all-avvio");
    }

    #[test]
    fn many_changes_are_counted() {
        let mixed = [
            NoteChange::new("a.md", ChangeKind::Created),
            NoteChange::new("b.md", ChangeKind::Updated),
            NoteChange::new("c.md", ChangeKind::Updated),
        ];
        assert_eq!(auto_commit_message(&mixed), "Aggiornate 3 note");
        let added = [
            NoteChange::new("a.md", ChangeKind::Created),
            NoteChange::new("b.md", ChangeKind::Created),
        ];
        assert_eq!(auto_commit_message(&added), "Aggiunte 2 note");
        assert_eq!(auto_commit_message(&[]), "Aggiornamento vault");
    }

    #[test]
    fn identity_carries_the_device() {
        let (name, email) = commit_identity("Casa");
        assert_eq!(name, "Garrulus (Casa)");
        assert_eq!(email, "garrulus+casa@arbor.local");
        let (name, email) = commit_identity("  ");
        assert_eq!(name, "Garrulus");
        assert_eq!(email, "garrulus+device@arbor.local");
    }

    #[test]
    fn device_slug_is_email_safe() {
        assert_eq!(slugify_device("PC dell'ufficio!"), "pc-dell-ufficio");
        assert_eq!(slugify_device("—"), "device");
    }
}
