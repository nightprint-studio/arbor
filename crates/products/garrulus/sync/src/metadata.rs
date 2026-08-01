//! `.arbor/garrulus/` metadata: merged **by rule**, never handed back as a
//! conflict.
//!
//! `docs/garrulus-design.md` §4.4.4 is a one-line rule with a large consequence:
//! *the vault's own metadata never conflicts*. It is merged by union of the type
//! set and per-key last-writer-wins on settings, because a conflict in a
//! settings file is pure noise — the user did not write `vault.toml` by hand,
//! they ticked a box, and asking them to arbitrate a diff of a file they have
//! never opened is the fastest way to make them stop trusting the sync. Left to
//! the note merger a conflicted `vault.toml` is worse than noise: [`merge_note`]
//! splits a file as *YAML frontmatter plus a markdown body*, which a TOML file
//! is not, so it reliably fails and leaves a `vault (conflitto — casa, 31-07
//! 14:22).toml` next to the real one plus an entry in the conflicts dock.
//!
//! [`merge_note`]: crate::conflict::merge_note
//!
//! ## Deliberately not a TOML parser
//!
//! Same trade as [`crate::frontmatter`] makes for YAML, for the same reason: the
//! merged file has to round-trip byte-stable when nobody touched it, comments
//! included, and pulling in a real parser would mean re-serialising the whole
//! document on every merge — every comment gone, every hand-written ordering
//! reshuffled, and the next sync a diff of the entire file.
//!
//! So this is a conservative *chunker*. A document is a list of sections (the
//! implicit root one, then a section per `[table]` / `[[array]]` header), and a
//! section is a list of keyed chunks: a `key = value` line plus every line its
//! value spans. Comments and blank lines attach to the chunk above them, as they
//! do in the frontmatter splitter. Nothing is re-rendered; chunks are moved
//! around verbatim.
//!
//! The one thing the chunker *does* have to be strict about is recognising a
//! file it cannot make sense of — a truncated multi-line string, a header with
//! no closing bracket, a line that is not a key, a header or a comment. That is
//! the single case where metadata still falls back to a side file: refusing to
//! merge a file whose shape we misread is the only alternative to silently
//! dropping a setting the user changed.

use crate::change::RelPath;
use crate::keyed::{keeps_one_sided, merge_keyed, render_fields, Clash, Field};

/// The folder Garrulus owns inside a vault, vault-relative.
///
/// Spelled out rather than imported from `garrulus_vault::prelude`
/// (`MARKER_RELATIVE_PATH`, same value): the sync engine deliberately references
/// no type from the vault crate so it stays drivable from a unit test with no
/// vault loaded — see the crate docs.
pub const METADATA_DIR: &str = ".arbor/garrulus";

/// Where deleted notes wait, vault-relative.
///
/// Inside [`METADATA_DIR`] but **not** metadata: a trashed note is a note, and
/// it merges like one.
pub const TRASH_DIR: &str = ".arbor/garrulus/trash";

/// Is this file vault metadata — the thing that never conflicts?
pub fn is_metadata_path(path: &RelPath) -> bool {
    path.is_in_folder(METADATA_DIR) && !path.is_in_folder(TRASH_DIR)
}

/// Merge one metadata file three ways.
///
/// `None` on either side means *absent there*, and the return follows
/// [`crate::frontmatter::merge_frontmatter`]'s shape:
///
/// * `Some(Some(text))` — the decided text.
/// * `Some(None)` — the file is gone (both sides removed it, or one removed a
///   file the other never touched).
/// * `None` — at least one side is not a file this module can read. The caller
///   falls back to the side-file path, which is loud but loses nothing.
///
/// A file present on one side only survives: that is the *union of the type
/// set* — a note type the other machine added is a type this machine now has,
/// and an edit outranks a delete for the same reason it does for a note.
pub fn merge_metadata(
    base: Option<&str>,
    local: Option<&str>,
    remote: Option<&str>,
) -> Option<Option<String>> {
    // The cheap answers first, and they are byte-exact: a file only one machine
    // touched merges without ever being parsed, so a metadata file this module
    // could not read still syncs as long as the two machines did not both edit
    // it.
    if local == remote {
        return Some(local.map(str::to_string));
    }
    if base.is_some() && base == local {
        return Some(remote.map(str::to_string));
    }
    if base.is_some() && base == remote {
        return Some(local.map(str::to_string));
    }
    let (local_text, remote_text) = match (local, remote) {
        (Some(l), Some(r)) => (l, r),
        // Present on one side only, and the genuine-delete cases were settled by
        // the base shortcuts above. What is left is an add, or a delete racing
        // an edit: the file exists.
        (Some(l), None) => return Some(Some(l.to_string())),
        (None, Some(r)) => return Some(Some(r.to_string())),
        (None, None) => return Some(None),
    };

    let local_doc = parse_document(local_text)?;
    let remote_doc = parse_document(remote_text)?;
    // A base that will not parse is simply treated as no base: the two sides
    // still merge, they just lose the ability to tell an edit from an addition,
    // which under `Clash::KeepLocal` costs nothing but the remote's value.
    let base_doc = base.and_then(parse_document).unwrap_or_default();

    let merged = merge_sections(&base_doc, &local_doc, &remote_doc);
    let mut out = render_document(&merged);
    // The file's shape follows the machine the user is sitting at, as it does in
    // `merge_text3`.
    if local_text.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    Some(Some(out))
}

// -- the document model ------------------------------------------------------

/// One `[table]` / `[[array of tables]]` section, or the implicit root one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Section {
    /// The header line exactly as written; empty for the root section.
    header: String,
    /// The dotted name between the brackets; empty for the root section.
    name: String,
    /// `[[name]]` rather than `[name]`.
    array: bool,
    /// The section's keyed chunks, in source order.
    entries: Vec<Field>,
}

impl Section {
    fn root() -> Self {
        Section { header: String::new(), name: String::new(), array: false, entries: Vec::new() }
    }

    fn is_root(&self) -> bool {
        self.header.is_empty()
    }

    fn render(&self) -> String {
        let body = render_fields(&self.entries);
        if self.is_root() {
            body
        } else if body.is_empty() {
            self.header.clone()
        } else {
            format!("{}\n{}", self.header, body)
        }
    }

    /// What makes this *the same section* as one on the other machine.
    ///
    /// A `[table]` is identified by its name. An `[[array of tables]]` entry has
    /// no name of its own, so it is identified by its **first key and value** —
    /// for `[[fields]]` that is `key = "status"`, which is exactly the identity
    /// a note type's field has. Position would be the obvious alternative and it
    /// is the wrong one: two machines each appending a field would collide at
    /// the same index and one of the two would be dropped, which is the failure
    /// this whole module exists to prevent.
    fn identity(&self) -> String {
        if !self.array {
            return self.name.clone();
        }
        let first = self
            .entries
            .iter()
            .find(|e| !e.key.is_empty())
            .and_then(|e| e.raw.lines().next())
            .unwrap_or_default()
            .trim();
        format!("{}\u{1}{}", self.name, first)
    }
}

/// Pair every section with its identity, disambiguating the ones that collide
/// so the mapping stays total (two `[[fields]]` with no keys at all, say).
fn keyed_sections(sections: &[Section]) -> Vec<(String, &Section)> {
    let mut out: Vec<(String, &Section)> = Vec::new();
    for section in sections {
        let base_id = section.identity();
        let mut id = base_id.clone();
        let mut n = 1;
        while out.iter().any(|(existing, _)| *existing == id) {
            id = format!("{base_id}\u{1}#{n}");
            n += 1;
        }
        out.push((id, section));
    }
    out
}

fn pick<'a>(list: &[(String, &'a Section)], id: &str) -> Option<&'a Section> {
    list.iter().find(|(existing, _)| existing.as_str() == id).map(|(_, s)| *s)
}

/// Merge section by section: the same presence rules as [`merge_keyed`] one
/// level up, with the two-sided case delegating to it.
fn merge_sections(base: &[Section], local: &[Section], remote: &[Section]) -> Vec<Section> {
    let (base, local, remote) =
        (keyed_sections(base), keyed_sections(local), keyed_sections(remote));

    // Local order is the spine; a section only the other machine has is
    // appended. The root section is always first in both, so it stays first —
    // which matters more than style, since TOML reads a bare key written after a
    // header as a key *of that table*.
    let mut ids: Vec<String> = local.iter().map(|(id, _)| id.clone()).collect();
    for (id, _) in &remote {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }

    let mut merged: Vec<Section> = Vec::new();
    for id in &ids {
        let b = pick(&base, id);
        let l = pick(&local, id);
        let r = pick(&remote, id);
        match (l, r) {
            (Some(l), Some(r)) => {
                let base_entries = b.map(|s| s.entries.as_slice()).unwrap_or(&[]);
                // `Clash::KeepLocal` never fails, but the fallback keeps the
                // function total without an unwrap.
                let entries = merge_keyed(base_entries, &l.entries, &r.entries, Clash::KeepLocal)
                    .unwrap_or_else(|| l.entries.clone());
                merged.push(Section { entries, ..l.clone() });
            }
            (Some(l), None) => {
                if keeps_one_sided(b.map(Section::render).as_deref(), &l.render()) {
                    merged.push(l.clone());
                }
            }
            (None, Some(r)) => {
                if keeps_one_sided(b.map(Section::render).as_deref(), &r.render()) {
                    merged.push(r.clone());
                }
            }
            (None, None) => {}
        }
    }
    merged
}

fn render_document(sections: &[Section]) -> String {
    sections
        .iter()
        .map(Section::render)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

// -- the chunker -------------------------------------------------------------

/// Cut a metadata file into sections. `None` means *this is not a file this
/// module can read*, which is the caller's cue to stop merging it.
fn parse_document(text: &str) -> Option<Vec<Section>> {
    let body = text.strip_suffix('\n').unwrap_or(text);
    if body.is_empty() {
        return Some(vec![Section::root()]);
    }
    let lines: Vec<&str> = body.split('\n').map(|l| l.trim_end_matches('\r')).collect();

    let mut sections = vec![Section::root()];
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.starts_with('[') {
            let (name, array) = parse_header(trimmed)?;
            sections.push(Section { header: line.to_string(), name, array, entries: Vec::new() });
            i += 1;
            continue;
        }

        if let Some((key, value_at)) = entry_key(line) {
            let mut open = Open::default();
            if !open.feed(&line[value_at..]) {
                return None;
            }
            let mut raw = line.to_string();
            while !open.is_closed() {
                i += 1;
                // Running out of lines inside a value means the file is
                // truncated, not that the value ended.
                let next = lines.get(i)?;
                if !open.feed(next) {
                    return None;
                }
                raw.push('\n');
                raw.push_str(next);
            }
            let section = sections.last_mut()?;
            // A key twice in one table is not valid TOML, and merging it as if
            // it were would quietly pick one of the two.
            if section.entries.iter().any(|e| e.key == key) {
                return None;
            }
            section.entries.push(Field { key, raw });
            i += 1;
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('#') {
            let section = sections.last_mut()?;
            match section.entries.last_mut() {
                Some(last) => {
                    last.raw.push('\n');
                    last.raw.push_str(line);
                }
                // Nothing above it yet: an empty key, so the block is carried
                // along instead of being dropped.
                None => section.entries.push(Field { key: String::new(), raw: line.to_string() }),
            }
            i += 1;
            continue;
        }

        // Not a header, not a key, not a comment.
        return None;
    }

    // Two `[table]` headers with the same name is not valid TOML either, and it
    // would make two different sections answer to one identity.
    let named: Vec<&str> =
        sections.iter().filter(|s| !s.array).map(|s| s.name.as_str()).collect();
    if (1..named.len()).any(|i| named[i..].contains(&named[i - 1])) {
        return None;
    }
    Some(sections)
}

/// `[name]` / `[[name]]` → `(name, is_array)`.
fn parse_header(trimmed: &str) -> Option<(String, bool)> {
    let (open, close, array) = if trimmed.starts_with("[[") {
        ("[[", "]]", true)
    } else if trimmed.starts_with('[') {
        ("[", "]", false)
    } else {
        return None;
    };
    let rest = &trimmed[open.len()..];
    let end = rest.find(close)?;
    let name = rest[..end].trim().to_string();
    let tail = rest[end + close.len()..].trim();
    if name.is_empty() || !(tail.is_empty() || tail.starts_with('#')) {
        return None;
    }
    Some((name, array))
}

/// The key of a `key = value` line, and the byte offset just past its `=`.
fn entry_key(line: &str) -> Option<(String, usize)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    let lead = line.len() - trimmed.len();
    let separator = key_separator(trimmed)?;
    let key = trimmed[..separator].trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), lead + separator + 1))
}

/// Offset of the `=` that ends the key, skipping over a quoted key.
fn key_separator(trimmed: &str) -> Option<usize> {
    let bytes = trimmed.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'=' => return Some(i),
            q @ (b'"' | b'\'') => i = quoted_end(bytes, i, q)? + 1,
            _ => i += 1,
        }
    }
    None
}

/// How much of a value is still open at the end of a line.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Open {
    /// Inside a `"""` / `'''` block, holding the quote byte that closes it.
    multiline: Option<u8>,
    /// Unclosed `[` / `{` of a value spanning lines.
    depth: i32,
}

impl Open {
    fn is_closed(&self) -> bool {
        self.multiline.is_none() && self.depth == 0
    }

    /// Feed one line of a value. `false` means this cannot be TOML at all — an
    /// unterminated single-line string, or a bracket closing one that never
    /// opened.
    fn feed(&mut self, line: &str) -> bool {
        let bytes = line.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if let Some(quote) = self.multiline {
                match find_triple(bytes, i, quote) {
                    Some(end) => {
                        self.multiline = None;
                        i = end + 3;
                    }
                    // The rest of the line is string content: a `#`, a `[` or a
                    // stray quote inside a template means nothing here.
                    None => return true,
                }
                continue;
            }
            match bytes[i] {
                b'#' => return true, // a comment runs to the end of the line
                quote @ (b'"' | b'\'') => {
                    if is_triple(bytes, i, quote) {
                        match find_triple(bytes, i + 3, quote) {
                            Some(end) => i = end + 3, // opened and closed on one line
                            None => {
                                self.multiline = Some(quote);
                                return true;
                            }
                        }
                    } else {
                        match quoted_end(bytes, i, quote) {
                            Some(end) => i = end + 1,
                            None => return false, // a single-line string cannot span lines
                        }
                    }
                }
                b'[' | b'{' => {
                    self.depth += 1;
                    i += 1;
                }
                b']' | b'}' => {
                    self.depth -= 1;
                    if self.depth < 0 {
                        return false;
                    }
                    i += 1;
                }
                _ => i += 1,
            }
        }
        true
    }
}

/// Offset of the quote closing a single-line string opened at `start`.
fn quoted_end(bytes: &[u8], start: usize, quote: u8) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        // Only a basic string has escapes; `'literal'` takes its bytes as they
        // come, backslash included.
        if quote == b'"' && bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn is_triple(bytes: &[u8], at: usize, quote: u8) -> bool {
    bytes.len() >= at + 3 && bytes[at..at + 3].iter().all(|b| *b == quote)
}

fn find_triple(bytes: &[u8], from: usize, quote: u8) -> Option<usize> {
    (from..bytes.len().saturating_sub(2)).find(|&i| is_triple(bytes, i, quote))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed-down `vault.toml`, comments and all.
    const VAULT: &str = "\
version = 1
name = \"Appunti\"
# Dove finisce un'immagine incollata.
attachments = \"attachments\"
excluded = [
  \"archivio\",
]

[daily]
folder = \"daily\"
naming = \"{{date}}\"
";

    #[test]
    fn only_the_vaults_own_metadata_is_metadata() {
        assert!(is_metadata_path(&RelPath::new(".arbor/garrulus/vault.toml")));
        assert!(is_metadata_path(&RelPath::new(".arbor/garrulus/types/bug.toml")));
        // A trashed note is a note.
        assert!(!is_metadata_path(&RelPath::new(".arbor/garrulus/trash/nota.md")));
        assert!(!is_metadata_path(&RelPath::new("bugs/crash.md")));
        assert!(!is_metadata_path(&RelPath::new(".arbor/config.toml")));
    }

    #[test]
    fn an_untouched_file_round_trips_byte_stable() {
        let parsed = parse_document(VAULT).expect("the sample parses");
        let mut rendered = render_document(&parsed);
        rendered.push('\n');
        assert_eq!(rendered, VAULT);
    }

    #[test]
    fn the_same_key_changed_on_both_sides_keeps_the_local_one() {
        let base = "version = 1\nattachments = \"attachments\"\n";
        let local = "version = 1\nattachments = \"media\"\n";
        let remote = "version = 1\nattachments = \"allegati\"\n";
        let merged = merge_metadata(Some(base), Some(local), Some(remote)).expect("no conflict");
        assert_eq!(merged.as_deref(), Some("version = 1\nattachments = \"media\"\n"));
    }

    #[test]
    fn a_key_added_on_one_side_survives() {
        let base = "version = 1\n";
        let local = "version = 1\nlink_style = \"markdown\"\n";
        let remote = "version = 1\nname = \"Appunti\"\n";
        let merged = merge_metadata(Some(base), Some(local), Some(remote)).expect("no conflict");
        assert_eq!(
            merged.as_deref(),
            Some("version = 1\nlink_style = \"markdown\"\nname = \"Appunti\"\n")
        );
    }

    #[test]
    fn a_type_file_present_only_on_the_remote_is_taken() {
        let bug = "id = \"bug\"\nname = \"Bug\"\n";
        // Never seen here, never seen in the common history: the other machine
        // wrote a new type. Union of the type set.
        assert_eq!(
            merge_metadata(None, None, Some(bug)),
            Some(Some(bug.to_string()))
        );
        // Deleted here while the other machine edited it: an edit outranks a
        // delete.
        let edited = "id = \"bug\"\nname = \"Difetto\"\n";
        assert_eq!(
            merge_metadata(Some(bug), None, Some(edited)),
            Some(Some(edited.to_string()))
        );
        // Deleted here and untouched there: a genuine delete.
        assert_eq!(merge_metadata(Some(bug), None, Some(bug)), Some(None));
    }

    #[test]
    fn malformed_toml_is_refused_rather_than_guessed() {
        let good = "version = 1\n";
        for broken in [
            "version = 1\nquesto non è toml\n",          // not a key, header or comment
            "version = 1\n[daily\nfolder = \"daily\"\n", // header with no closing bracket
            "template = \"\"\"\nmai chiuso\n",           // truncated multi-line string
            "excluded = [\n  \"archivio\",\n",           // truncated array
            "version = 1\nversion = 2\n",                // the same key twice
            "[daily]\nfolder = \"a\"\n[daily]\nfolder = \"b\"\n", // the same table twice
            "{\"json\": true}\n",                        // not TOML at all
        ] {
            assert_eq!(
                merge_metadata(None, Some(good), Some(broken)),
                None,
                "should refuse to merge {broken:?}"
            );
        }
    }

    #[test]
    fn a_file_only_one_side_touched_merges_without_being_parsed() {
        let broken = "questo non è toml\n";
        let edited = "questo non è toml, e cambia\n";
        // Local untouched -> take the remote, unparseable or not.
        assert_eq!(
            merge_metadata(Some(broken), Some(broken), Some(edited)),
            Some(Some(edited.to_string()))
        );
    }

    #[test]
    fn sections_merge_key_by_key_and_stay_in_place() {
        let base = "version = 1\n\n[daily]\nfolder = \"daily\"\nnaming = \"{{date}}\"\n";
        let local = "version = 1\n\n[daily]\nfolder = \"diario\"\nnaming = \"{{date}}\"\n";
        let remote = "version = 1\n\n[daily]\nfolder = \"daily\"\nnaming = \"{{date}} {{title}}\"\n";
        let merged = merge_metadata(Some(base), Some(local), Some(remote)).expect("no conflict");
        assert_eq!(
            merged.as_deref(),
            Some("version = 1\n\n[daily]\nfolder = \"diario\"\nnaming = \"{{date}} {{title}}\"\n")
        );
    }

    #[test]
    fn a_section_added_on_one_side_lands_at_the_end() {
        let base = "id = \"bug\"\n";
        let local = "id = \"bug\"\n\n[layout]\nwide_editor = true\n";
        let remote = "id = \"bug\"\n\n[[fields]]\nkey = \"status\"\nlabel = \"Stato\"\n";
        let merged = merge_metadata(Some(base), Some(local), Some(remote)).expect("no conflict");
        // The blank line that separated the remote's section from the keys above
        // it belonged to a chunk both sides share, so the moved-in section
        // arrives without one. Cosmetic, and the price of never re-rendering a
        // chunk: byte-stability for the untouched file outranks it (§5.1).
        assert_eq!(
            merged.as_deref(),
            Some(
                "id = \"bug\"\n\n[layout]\nwide_editor = true\n[[fields]]\nkey = \"status\"\nlabel = \"Stato\"\n"
            )
        );
    }

    #[test]
    fn two_machines_each_appending_a_field_keep_both() {
        let base = "id = \"bug\"\n\n[[fields]]\nkey = \"status\"\n";
        let local = "id = \"bug\"\n\n[[fields]]\nkey = \"status\"\n\n[[fields]]\nkey = \"severity\"\n";
        let remote = "id = \"bug\"\n\n[[fields]]\nkey = \"status\"\n\n[[fields]]\nkey = \"app\"\n";
        let merged = merge_metadata(Some(base), Some(local), Some(remote)).expect("no conflict");
        let merged = merged.expect("the type file survives");
        assert!(merged.contains("key = \"severity\""), "{merged}");
        assert!(merged.contains("key = \"app\""), "{merged}");
        assert_eq!(merged.matches("key = \"status\"").count(), 1, "{merged}");
    }

    #[test]
    fn a_multi_line_template_is_carried_verbatim() {
        let base = "id = \"bug\"\ntemplate = \"\"\"\n## Passi\n\n1. \n\"\"\"\n";
        // The template holds a `#`, a blank line and a `[` — none of which are
        // TOML while the string is open.
        let local = "id = \"bug\"\ntemplate = \"\"\"\n## Passi\n\n1. [[nota]]\n\"\"\"\n";
        let remote = "id = \"bug\"\nname = \"Bug\"\ntemplate = \"\"\"\n## Passi\n\n1. \n\"\"\"\n";
        let merged = merge_metadata(Some(base), Some(local), Some(remote)).expect("no conflict");
        assert_eq!(
            merged.as_deref(),
            Some(
                "id = \"bug\"\ntemplate = \"\"\"\n## Passi\n\n1. [[nota]]\n\"\"\"\nname = \"Bug\"\n"
            )
        );
    }

    #[test]
    fn a_comment_travels_with_the_chunk_above_it() {
        let parsed = parse_document(VAULT).expect("the sample parses");
        let root = &parsed[0];
        assert_eq!(root.entries[1].key, "name");
        assert!(root.entries[1].raw.ends_with("# Dove finisce un'immagine incollata."));
        assert_eq!(root.entries[3].key, "excluded");
        assert_eq!(root.entries[3].raw, "excluded = [\n  \"archivio\",\n]\n");
    }

    #[test]
    fn a_value_is_read_to_its_end_not_to_its_first_line() {
        let parsed = parse_document(VAULT).expect("the sample parses");
        let keys: Vec<&str> = parsed[0].entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["version", "name", "attachments", "excluded"]);
        assert_eq!(parsed[1].name, "daily");
        assert!(!parsed[1].array);
    }

    #[test]
    fn a_quoted_key_and_an_equals_in_a_value_do_not_confuse_the_splitter() {
        let (key, at) = entry_key("\"my = key\" = \"a = b\"").expect("a key line");
        assert_eq!(key, "\"my = key\"");
        assert_eq!(at, 12);
        assert!(entry_key("# commento = no").is_none());
        assert!(entry_key("[daily]").is_none());
    }
}
