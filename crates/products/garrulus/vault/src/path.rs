//! Vault-relative paths, in one place.
//!
//! Every path in Garrulus is relative to the vault root with POSIX separators,
//! including on Windows — that is the identity of a note everywhere in the
//! product, in the index, on the wire and in a sync batch. The same three
//! questions are asked of one from six modules, so they are answered here rather
//! than six times.
//!
//! The empty string is the root itself and is a legitimate folder path: a type
//! whose `folder` is `""` files its notes at the top level.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{VaultError, VaultResult};

/// A path relative to the vault root, POSIX separators, no leading slash.
///
/// A newtype rather than a `String` because it is the identity of a note: it is a
/// map key in the index, a line in a sync batch and a field on the wire, and the
/// one thing that must never quietly become an absolute path from one machine.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelPath(String);

impl RelPath {
    /// Normalise anything path-shaped into the canonical form: `\` becomes `/`,
    /// `.` segments and repeated separators collapse, leading and trailing
    /// separators are dropped.
    ///
    /// `..` is **kept** rather than resolved, so that [`RelPath::escapes`] can
    /// still see it. Resolving here would silently turn an attempt to leave the
    /// vault into a plausible-looking path.
    pub fn new(raw: impl AsRef<str>) -> RelPath {
        let mut out = String::with_capacity(raw.as_ref().len());
        for segment in raw.as_ref().split(['/', '\\']) {
            if segment.is_empty() || segment == "." {
                continue;
            }
            if !out.is_empty() {
                out.push('/');
            }
            out.push_str(segment);
        }
        RelPath(out)
    }

    /// The path as it is written everywhere else.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Does this path try to climb out of the vault?
    ///
    /// A note whose path escapes is never read, written or trashed: a sync remote
    /// or a plugin handing us `../../.ssh/id_rsa` is not a path, it is an attack.
    pub fn escapes(&self) -> bool {
        self.0.split('/').any(|segment| segment == "..")
    }

    /// Everything before the last separator; `""` for a top-level note.
    pub fn parent(&self) -> &str {
        parent_of(&self.0)
    }

    /// The file name, extension included.
    pub fn file_name(&self) -> &str {
        last_segment(&self.0)
    }

    /// The file name without its extension — the fallback title of a note whose
    /// frontmatter and body say nothing about what it is called.
    pub fn stem(&self) -> &str {
        let name = self.file_name();
        match name.rfind('.') {
            // A leading dot is a hidden file, not an extension: `.gitignore`.
            Some(index) if index > 0 => &name[..index],
            _ => name,
        }
    }

    /// The lowercased extension, without the dot.
    pub fn extension(&self) -> Option<String> {
        let name = self.file_name();
        let index = name.rfind('.')?;
        if index == 0 || index + 1 == name.len() {
            return None;
        }
        Some(name[index + 1..].to_ascii_lowercase())
    }

    /// Is this a note? Only `.md` counts — `.markdown` and `.txt` are files that
    /// happen to live in the vault, and treating them as notes would put them in
    /// the index, the graph and the sync batch without the user asking.
    pub fn is_note(&self) -> bool {
        self.extension().as_deref() == Some("md")
    }

    /// Resolve against a vault root.
    pub fn to_path(&self, root: &Path) -> PathBuf {
        let mut out = root.to_path_buf();
        for segment in self.0.split('/').filter(|s| !s.is_empty()) {
            out.push(segment);
        }
        out
    }

    /// Is this the empty (root) path?
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Join a child segment or sub-path below this one.
    pub fn join(&self, child: impl AsRef<str>) -> RelPath {
        if self.0.is_empty() {
            return RelPath::new(child);
        }
        RelPath::new(format!("{}/{}", self.0, child.as_ref()))
    }
}

impl std::fmt::Display for RelPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for RelPath {
    fn from(value: &str) -> Self {
        RelPath::new(value)
    }
}

impl From<String> for RelPath {
    fn from(value: String) -> Self {
        RelPath::new(value)
    }
}

impl AsRef<str> for RelPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for RelPath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Everything before the last separator. `""` for a top-level path, and for the
/// root itself.
pub fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

/// The last segment — the file name, or what a folder is called.
pub fn last_segment(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// A path and every folder above it, nearest first, ending at the root `""`.
///
/// `"a/b/c"` → `["a/b/c", "a/b", "a", ""]`. This is the order a lookup with
/// inheritance wants: the first ancestor that declares something wins.
pub fn self_and_ancestors(path: &str) -> Vec<&str> {
    let mut out = vec![path];
    let mut current = path;
    while !current.is_empty() {
        current = parent_of(current);
        out.push(current);
    }
    out
}

/// Is `folder` the path of a folder that contains `path`, at any depth?
///
/// The empty folder is the root and contains everything. A shared prefix is not
/// a parent: `bugs` does not contain `bugs-old/x.md`.
pub fn contains(folder: &str, path: &str) -> bool {
    if folder.is_empty() {
        return true;
    }
    path.len() > folder.len()
        && path.starts_with(folder)
        && path.as_bytes()[folder.len()] == b'/'
}

/// Express an absolute path as a vault-relative one, or `None` when it is not
/// inside the root.
pub fn to_rel(root: &Path, absolute: &Path) -> Option<RelPath> {
    let rest = absolute.strip_prefix(root).ok()?;
    let mut out = String::new();
    for component in rest.components() {
        let Component::Normal(segment) = component else {
            // A `..` or a prefix means the path left the root by another door.
            return None;
        };
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&segment.to_string_lossy());
    }
    Some(RelPath(out))
}

/// `arbor-fs` speaks in `&str` paths; this is the one place a `Path` that is not
/// valid UTF-8 turns into an error the user can read instead of a lossy guess.
pub fn path_str(path: &Path) -> VaultResult<&str> {
    path.to_str().ok_or_else(|| VaultError::BadPath {
        raw: path.to_string_lossy().into_owned(),
        reason: "the path is not valid UTF-8".to_string(),
    })
}

/// Does a folder glob match a vault-relative path?
///
/// The dialect is the small, predictable one: `*` matches within one segment,
/// `?` matches one character, `**` matches zero or more whole segments. Matching
/// is against the **note's own path**, so `bugs/**` matches `bugs/Crash.md` and
/// `bugs/2026/Crash.md` alike, and also the folder `bugs` itself.
///
/// Deliberately not a full `glob` crate: the whole grammar is three tokens, and a
/// dependency whose extra features (`[a-z]`, `{a,b}`, escaping) nobody would ever
/// put in a `match_folder` line is a dependency that only adds ways to be
/// surprised.
pub fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match_segments(&pattern_segments, &path_segments)
}

fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => {
            if rest.is_empty() {
                return true;
            }
            (0..=path.len()).any(|skip| match_segments(rest, &path[skip..]))
        }
        Some((head, rest)) => match path.split_first() {
            Some((segment, tail)) if match_segment(head, segment) => match_segments(rest, tail),
            _ => false,
        },
    }
}

/// `*` and `?` inside a single segment, matched greedily with backtracking.
fn match_segment(pattern: &str, segment: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let segment: Vec<char> = segment.chars().collect();
    match_chars(&pattern, &segment)
}

fn match_chars(pattern: &[char], text: &[char]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some(('*', rest)) => (0..=text.len()).any(|skip| match_chars(rest, &text[skip..])),
        Some((head, rest)) => match text.split_first() {
            Some((c, tail)) if *head == '?' || head == c => match_chars(rest, tail),
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_normalised_to_posix_separators() {
        assert_eq!(RelPath::new("bugs\\2026\\Crash.md").as_str(), "bugs/2026/Crash.md");
        assert_eq!(RelPath::new("/bugs//Crash.md/").as_str(), "bugs/Crash.md");
        assert_eq!(RelPath::new("./bugs/./Crash.md").as_str(), "bugs/Crash.md");
        assert_eq!(RelPath::new("").as_str(), "");
    }

    #[test]
    fn a_climbing_path_stays_visible_instead_of_being_resolved() {
        assert!(RelPath::new("../../.ssh/id_rsa").escapes());
        assert!(RelPath::new("bugs/../../out.md").escapes());
        assert!(!RelPath::new("bugs/Crash.md").escapes());
    }

    #[test]
    fn the_stem_is_the_fallback_title_and_a_dotfile_has_no_extension() {
        assert_eq!(RelPath::new("bugs/Crash all'avvio.md").stem(), "Crash all'avvio");
        assert_eq!(RelPath::new("bugs/Crash all'avvio.md").extension().as_deref(), Some("md"));
        assert_eq!(RelPath::new(".gitignore").stem(), ".gitignore");
        assert_eq!(RelPath::new(".gitignore").extension(), None);
        assert!(RelPath::new("a/b.MD").is_note());
        assert!(!RelPath::new("a/b.txt").is_note());
    }

    #[test]
    fn a_path_walks_up_to_the_root() {
        assert_eq!(self_and_ancestors("a/b/c"), ["a/b/c", "a/b", "a", ""]);
        assert_eq!(self_and_ancestors(""), [""]);
    }

    #[test]
    fn the_root_contains_everything_and_a_prefix_is_not_a_parent() {
        assert!(contains("", "bugs/x.md"));
        assert!(contains("bugs", "bugs/2026/x.md"));
        assert!(!contains("bugs", "bugs-old/x.md"));
        assert!(!contains("bugs", "bugs"));
    }

    #[test]
    fn a_double_star_spans_any_number_of_folders_including_none() {
        assert!(glob_matches("bugs/**", "bugs/Crash.md"));
        assert!(glob_matches("bugs/**", "bugs/2026/07/Crash.md"));
        assert!(glob_matches("bugs/**", "bugs"));
        assert!(!glob_matches("bugs/**", "bugs-old/Crash.md"));
        assert!(!glob_matches("bugs/**", "design/Crash.md"));
    }

    #[test]
    fn a_single_star_stops_at_a_separator() {
        assert!(glob_matches("daily/*.md", "daily/2026-07-31.md"));
        assert!(!glob_matches("daily/*.md", "daily/2026/07-31.md"));
        assert!(glob_matches("*/decisioni/**", "arbor/decisioni/ADR-1.md"));
        assert!(glob_matches("bug?.md", "bug1.md"));
        assert!(!glob_matches("bug?.md", "bug12.md"));
    }

    #[test]
    fn an_absolute_path_outside_the_root_has_no_relative_form() {
        let root = Path::new("/vault");
        assert_eq!(
            to_rel(root, Path::new("/vault/bugs/Crash.md")).map(|p| p.as_str().to_string()),
            Some("bugs/Crash.md".to_string())
        );
        assert_eq!(to_rel(root, Path::new("/elsewhere/Crash.md")), None);
    }
}
