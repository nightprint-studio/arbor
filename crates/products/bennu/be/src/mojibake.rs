//! `mojibake` domain — `bennu_mojibake_check`: find UTF-8-decoded-as-Cp1252 corruption in a
//! file and offer the corrected character.
//!
//! "Mojibake" here is the classic double-decode: text that was UTF-8 but got read as Windows-1252
//! (Latin-1), so `é` (`C3 A9`) shows up as `Ã©`, a right quote `'` (`E2 80 99`) as `â€™`, a
//! non-breaking space as `Â `, and so on. We detect it precisely (not heuristically) by building a
//! table of `correct-char → its mojibake rendering` at runtime — each correct char's UTF-8 bytes
//! re-interpreted through the Cp1252 code page — then scanning for those exact sequences. Building
//! the table from char codes (rather than pasting the garbled strings) is deliberate: it keeps the
//! detector from flagging its own source (the CLAUDE.md self-test rule) and makes it trivially
//! correct. Each hit carries the byte span + the bad text + the single correct char, so the FE can
//! squiggle it and offer a one-click replace.

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

/// Args for [`bennu_mojibake_check`].
#[derive(Deserialize)]
pub struct MojibakeArgs {
    /// Absolute path of the file (unused by the scan, echoed for symmetry with the other
    /// per-file handlers / future project scan).
    #[allow(dead_code)]
    pub file: String,
    /// The current (possibly-unsaved) buffer text — scanned as-is.
    pub source: String,
}

/// One detected mojibake sequence + its correction.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MojibakeHit {
    /// Start byte offset of the garbled sequence in the source.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The garbled text as it appears (e.g. `"Ã©"`).
    pub bad: String,
    /// The single correct character it should be (e.g. `"é"`).
    pub fix: String,
}

/// Scan `source` for mojibake and return every hit (byte spans + suggested fix).
#[arbor_rpc::handler]
fn bennu_mojibake_check(_ctx: &BennuState, args: MojibakeArgs) -> Result<Vec<MojibakeHit>, String> {
    Ok(find_mojibake(&args.source))
}

/// Args for [`bennu_mojibake_project`].
#[derive(Deserialize)]
pub struct MojibakeProjectArgs {
    /// Absolute path to the project root to scan.
    pub root: String,
}

/// One file's mojibake hits, for the project-scan result.
#[derive(Debug, Clone, Serialize)]
pub struct FileMojibake {
    /// Absolute (forward-slashed) path of the file.
    pub file: String,
    /// Every mojibake hit in the file (byte spans + fixes), in document order.
    pub hits: Vec<MojibakeHit>,
}

/// The whole-project mojibake scan result: headline counts + the affected files (only those WITH
/// hits, most-affected first).
#[derive(Debug, Clone, Serialize)]
pub struct ProjectMojibakeResult {
    /// How many text files were read + scanned.
    pub total_files_scanned: usize,
    /// How many of them had at least one hit.
    pub files_with_hits: usize,
    /// Total hits across the project.
    pub total_hits: usize,
    /// The affected files (hits > 0), sorted by hit count descending then path.
    pub files: Vec<FileMojibake>,
}

/// Scan every text file in the project for mojibake, in parallel. Each file is decoded in the
/// project's **resolved encoding** (per-project override → pom `sourceEncoding` → config default) —
/// the same decode the index uses — then normalized to LF, so the scanned text (and its byte
/// offsets) match exactly what the editor shows. This catches mojibake in legacy Cp1252 projects
/// too, not just UTF-8 files. Runs the same per-file detector as the editor's on-demand check.
#[arbor_rpc::handler]
fn bennu_mojibake_project(
    _ctx: &BennuState,
    args: MojibakeProjectArgs,
) -> Result<ProjectMojibakeResult, String> {
    let label = crate::index_service::resolve_index_encoding(&args.root);
    let paths = crate::find::collect_text_paths(std::path::Path::new(&args.root));
    let total_files_scanned = paths.len();

    // Decode + scan each file independently on the shared work-stealing pool (leaves ~2 cores free
    // for the UI). Reading in the closure parallelises the I/O too; an unreadable file → no hits.
    let scanned: Vec<FileMojibake> = bennu_intel::prelude::parallel_map(&paths, |path| {
        let hits = match std::fs::read(path) {
            Ok(bytes) => {
                let decoded = bennu_project::prelude::decode_for_index(&bytes, &label);
                let text = bennu_project::prelude::normalize_newlines(&decoded.text);
                find_mojibake(&text)
            }
            Err(_) => Vec::new(),
        };
        FileMojibake { file: path.to_string_lossy().replace('\\', "/"), hits }
    });

    let mut files: Vec<FileMojibake> = scanned.into_iter().filter(|f| !f.hits.is_empty()).collect();
    files.sort_by(|a, b| b.hits.len().cmp(&a.hits.len()).then_with(|| a.file.cmp(&b.file)));
    let files_with_hits = files.len();
    let total_hits = files.iter().map(|f| f.hits.len()).sum();

    Ok(ProjectMojibakeResult { total_files_scanned, files_with_hits, total_hits, files })
}

/// Cp1252 decode of a single byte. `0x80–0x9F` are the code page's specials; `0xA0–0xFF` are
/// Latin-1 (== Unicode); `0x00–0x7F` are ASCII. Returns `None` for the five bytes Cp1252 leaves
/// undefined (`0x81 0x8D 0x8F 0x90 0x9D`) — a char whose UTF-8 uses one can't round-trip.
fn cp1252_char(b: u8) -> Option<char> {
    let c = match b {
        0x80 => '\u{20AC}', 0x82 => '\u{201A}', 0x83 => '\u{0192}', 0x84 => '\u{201E}',
        0x85 => '\u{2026}', 0x86 => '\u{2020}', 0x87 => '\u{2021}', 0x88 => '\u{02C6}',
        0x89 => '\u{2030}', 0x8A => '\u{0160}', 0x8B => '\u{2039}', 0x8C => '\u{0152}',
        0x8E => '\u{017D}', 0x91 => '\u{2018}', 0x92 => '\u{2019}', 0x93 => '\u{201C}',
        0x94 => '\u{201D}', 0x95 => '\u{2022}', 0x96 => '\u{2013}', 0x97 => '\u{2014}',
        0x98 => '\u{02DC}', 0x99 => '\u{2122}', 0x9A => '\u{0161}', 0x9B => '\u{203A}',
        0x9C => '\u{0153}', 0x9E => '\u{017E}', 0x9F => '\u{0178}',
        0x81 | 0x8D | 0x8F | 0x90 | 0x9D => return None,
        _ => b as char, // ASCII + Latin-1 map straight through
    };
    Some(c)
}

/// The mojibake rendering of `c`: its UTF-8 bytes each re-read as a Cp1252 char. `None` if any
/// byte is an undefined Cp1252 code.
fn mojibake_of(c: char) -> Option<String> {
    let mut buf = [0u8; 4];
    let bytes = c.encode_utf8(&mut buf).as_bytes();
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        s.push(cp1252_char(b)?);
    }
    Some(s)
}

/// The correct characters worth detecting: European accents (IT/FR/DE/ES/PT), smart quotes /
/// dashes / ellipsis / bullet, guillemets, degree, euro, and the non-breaking space. Each 1-byte
/// ASCII char is excluded (its "mojibake" is itself).
const TARGETS: &[char] = &[
    'à', 'á', 'â', 'ä', 'ã', 'å', 'è', 'é', 'ê', 'ë', 'ì', 'í', 'î', 'ï', 'ò', 'ó', 'ô', 'ö',
    'õ', 'ù', 'ú', 'û', 'ü', 'ç', 'ñ', 'ß', 'ý', 'ÿ',
    'À', 'Á', 'Â', 'Ä', 'Ã', 'È', 'É', 'Ê', 'Ë', 'Ì', 'Í', 'Î', 'Ï', 'Ò', 'Ó', 'Ô', 'Ö', 'Ù',
    'Ú', 'Û', 'Ü', 'Ç', 'Ñ',
    '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2013}', '\u{2014}', '\u{2026}',
    '\u{2022}', '\u{00AB}', '\u{00BB}', '\u{00B0}', '\u{20AC}', '\u{00A0}',
];

/// `(mojibake sequence, correct char)` for every [`TARGETS`] char whose sequence round-trips,
/// sorted **longest sequence first** so a 3-char match (a smart quote) wins over a coincidental
/// 2-char prefix.
fn mojibake_table() -> Vec<(String, char)> {
    let mut table: Vec<(String, char)> =
        TARGETS.iter().filter_map(|&c| mojibake_of(c).map(|m| (m, c))).collect();
    table.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    table
}

/// Scan `text` for mojibake sequences, returning each as a [`MojibakeHit`] (byte span + fix), in
/// document order, non-overlapping.
pub fn find_mojibake(text: &str) -> Vec<MojibakeHit> {
    let table = mojibake_table();
    let mut hits = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        // Longest-first table, so the first `starts_with` is the maximal match.
        if let Some((bad, fix)) = table.iter().find(|(bad, _)| rest.starts_with(bad.as_str())) {
            let end = i + bad.len();
            hits.push(MojibakeHit {
                start: i,
                end,
                bad: bad.clone(),
                fix: fix.to_string(),
            });
            i = end;
        } else {
            // Advance one whole char (byte indices stay on char boundaries).
            i += rest.chars().next().map(char::len_utf8).unwrap_or(1);
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a mojibake string for `c` the same way the detector does (from char codes), so the
    /// test never pastes a garbled literal (which would itself be flagged if the file were scanned).
    fn garble(c: char) -> String {
        mojibake_of(c).expect("target round-trips")
    }

    #[test]
    fn detects_accented_letter_mojibake() {
        let text = format!("Perch{} vero", garble('é')); // "Perché vero" corrupted
        let hits = find_mojibake(&text);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].fix, "é");
        assert_eq!(&text[hits[0].start..hits[0].end], hits[0].bad);
    }

    #[test]
    fn detects_smart_quote_mojibake() {
        let text = format!("It{}s here", garble('\u{2019}')); // right single quote
        let hits = find_mojibake(&text);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].fix, "\u{2019}");
        // The garbled form is 3 chars (E2 80 99 → â € ™), so a longest-match is essential.
        assert_eq!(hits[0].bad.chars().count(), 3);
    }

    #[test]
    fn finds_multiple_hits_in_order() {
        let text = format!("{} {} {}", garble('à'), garble('è'), garble('ù'));
        let hits = find_mojibake(&text);
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].fix, "à");
        assert_eq!(hits[1].fix, "è");
        assert_eq!(hits[2].fix, "ù");
        assert!(hits[0].start < hits[1].start && hits[1].start < hits[2].start);
    }

    #[test]
    fn clean_utf8_has_no_hits() {
        // Correctly-encoded accented text must NOT be flagged.
        assert!(find_mojibake("Perché è così, ìnutile — «ok»").is_empty());
        assert!(find_mojibake("plain ascii only").is_empty());
        assert!(find_mojibake("").is_empty());
    }

    #[test]
    fn spans_are_char_aligned_and_sliceable() {
        let text = format!("a{}b{}c", garble('ò'), garble('\u{201C}'));
        for h in find_mojibake(&text) {
            // Never panics → the byte span sits on char boundaries.
            assert_eq!(&text[h.start..h.end], h.bad);
        }
    }
}
