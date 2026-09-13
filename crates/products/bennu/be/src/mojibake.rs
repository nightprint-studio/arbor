//! `mojibake` domain — `bennu_mojibake_check`: find characters in a file that are not the ones the
//! file was written with, and offer the correction where there is one.
//!
//! Two different corruptions, and they are worth telling apart because only one of them can be
//! repaired from the text alone.
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
//!
//! The second is a byte that could not be decoded **at all**. The read path is lossy by design
//! (`bennu_project`'s `decode`), so a stray `0xE8` in a file being read as UTF-8 arrives in the
//! buffer as the replacement character `U+FFFD` — one character, standing where `è` was written.
//! Nothing here can say what it was: the byte that would have said is gone before the text
//! reaches this function. So it is reported with **no fix**, and the answer is to reload the file
//! in the encoding it is actually written in (Project Configuration → Encoding), not to type over
//! the glyph — typing over it saves a file whose original byte has already been thrown away.
//!
//! Leaving it undetected was the worse option by a distance: it is the corruption that *looks* like
//! a typo, and a check that answers "no problems" over a visibly broken character teaches people
//! not to run it.

use std::sync::OnceLock;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::Diagnostic;
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
    /// The single correct character it should be (e.g. `"é"`), or **empty** when the bad text is
    /// the replacement character: the byte that would have said what it was is gone before the
    /// text reaches the scan, so there is nothing to offer and a guess would corrupt the file a
    /// second time. The fix for those is to reload in the right encoding.
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
    // Same budget as the index build: this walks every source file in the project, which is the
    // other sweep a user notices their machine doing.
    bennu_intel::prelude::set_background_workers(bennu_core::config::load().index_threads);
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
/// 2-char prefix — plus the set of bytes one can begin with.
///
/// Built once. It used to be built and sorted per call, which did not matter while the only caller
/// was a palette command on one file, and mattered a great deal the moment the scan joined ordinary
/// validation.
struct Table {
    entries: Vec<(String, char)>,
    /// The distinct first **bytes** of those sequences.
    ///
    /// This is what makes the scan linear. Every mojibake sequence starts with a byte the UTF-8
    /// encoding of a non-ASCII character begins with — `0xC3`, `0xC2`, `0xE2` — so a byte outside
    /// this set cannot start one, and the table never has to be consulted for it. Without the
    /// check the loop ran ~70 `starts_with` per character of the file: fine for one file on
    /// demand, seconds of dead time when every buffer in a reopened project asks at once.
    leads: [bool; 256],
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut entries: Vec<(String, char)> =
            TARGETS.iter().filter_map(|&c| mojibake_of(c).map(|m| (m, c))).collect();
        entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        let mut leads = [false; 256];
        for (bad, _) in &entries {
            if let Some(&first) = bad.as_bytes().first() {
                leads[first as usize] = true;
            }
        }
        Table { entries, leads }
    })
}

/// The corruptions in `source`, as wire diagnostics — what puts a squiggle under a broken
/// character while you type instead of only when you go and ask.
///
/// Two codes, because the two are two different problems with two different answers:
/// `encoding.mojibake` can be repaired in place (the correct character is known), and
/// `encoding.undecodable` cannot be repaired at all — it says the file is being read in the wrong
/// encoding, and the answer is to reload it in the right one.
///
/// Not restricted to any file type. Every other contributor here is about a language; this one is
/// about bytes, and a legacy tree keeps its accented text in `.properties` and templates far more
/// often than in `.java`.
pub(crate) fn diagnostics_for(source: &str) -> Vec<Diagnostic> {
    find_mojibake(source)
        .into_iter()
        .map(|hit| {
            // One line each. A diagnostic message is read in a hover the width of the editor, so
            // the paragraph explaining what to do about it belongs in the documentation — a
            // tooltip that has to be resized to be read is one nobody finishes reading.
            let (code, message) = if hit.fix.is_empty() {
                (
                    "encoding.undecodable",
                    "Unreadable character — the file is not in the encoding it is being decoded \
                     with. Reload it in the right one."
                        .to_string(),
                )
            } else {
                (
                    "encoding.mojibake",
                    format!("Mojibake: “{}” should be “{}”.", hit.bad, hit.fix),
                )
            };
            Diagnostic {
                message,
                severity: "warning".to_string(),
                code: code.to_string(),
                start: hit.start,
                end: hit.end,
            }
        })
        .collect()
}

/// The character a lossy decode leaves where a byte it could not read used to be.
const REPLACEMENT: char = '\u{FFFD}';

/// Its first UTF-8 byte (`EF BF BD`), for the lead-byte skip in [`find_mojibake`].
const REPLACEMENT_LEAD: u8 = 0xEF;

/// Scan `text` for corrupted characters, returning each as a [`MojibakeHit`] (byte span, and a fix
/// where one can be known), in document order, non-overlapping.
pub fn find_mojibake(text: &str) -> Vec<MojibakeHit> {
    let table = table();
    let bytes = text.as_bytes();
    let mut hits = Vec::new();
    let mut i = 0;
    while i < text.len() {
        // The fast path, and the reason this can run on every keystroke: a byte that cannot begin
        // any corrupted sequence is skipped without touching the table at all. That is every ASCII
        // byte, which is nearly all of every file.
        //
        // Stepping one **byte** here is safe, and the invariant is worth stating because the slice
        // below would panic if it were not: every lead byte in the table is the first byte of a
        // multi-byte UTF-8 character (`0xC3`, and `0xEF` for the replacement character), and a
        // UTF-8 continuation byte is always `0x80..=0xBF`. So a byte this guard stops on is always
        // the start of a character, and the bytes it skips over are never sliced.
        let lead = bytes[i];
        if !table.leads[lead as usize] && lead != REPLACEMENT_LEAD {
            i += 1;
            continue;
        }
        let rest = &text[i..];
        // Longest-first table, so the first `starts_with` is the maximal match.
        if let Some((bad, fix)) = table.entries.iter().find(|(bad, _)| rest.starts_with(bad.as_str())) {
            let end = i + bad.len();
            hits.push(MojibakeHit {
                start: i,
                end,
                bad: bad.clone(),
                fix: fix.to_string(),
            });
            i = end;
        } else if rest.starts_with(REPLACEMENT) {
            // A run of them is one wound, not three: a single character of a multi-byte encoding
            // can lose several bytes, and three squiggles side by side say nothing the first does
            // not. Reported with an empty `fix` — see the module doc for why there cannot be one.
            let run = rest.len() - rest.trim_start_matches(REPLACEMENT).len();
            hits.push(MojibakeHit {
                start: i,
                end: i + run,
                bad: rest[..run].to_string(),
                fix: String::new(),
            });
            i += run;
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
    fn detects_a_byte_that_could_not_be_decoded_at_all() {
        // What the editor's lossy read leaves where a Cp1252 `è` sat in a file read as UTF-8 — the
        // corruption that looks like a typo, and the one the scan used to walk straight past.
        let text = "# Il campo \u{FFFD} obbligatorio";
        let hits = find_mojibake(text);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].bad, "\u{FFFD}");
        assert!(hits[0].fix.is_empty(), "nothing can be known about what it was");
        assert_eq!(&text[hits[0].start..hits[0].end], hits[0].bad);
    }

    #[test]
    fn a_run_of_replacement_characters_is_one_hit() {
        let hits = find_mojibake("a\u{FFFD}\u{FFFD}\u{FFFD}b");
        assert_eq!(hits.len(), 1, "one wound, not three");
        assert_eq!(hits[0].bad.chars().count(), 3);
    }

    /// The guard in `find_mojibake` steps one byte at a time and then slices. That is only sound
    /// while no lead byte can also be a UTF-8 continuation byte — which a `TARGETS` entry outside
    /// the Latin-1 and General-Punctuation ranges could break without any test failing.
    #[test]
    fn no_lead_byte_is_a_utf8_continuation_byte() {
        for (bad, correct) in &table().entries {
            let lead = bad.as_bytes()[0];
            assert!(
                !(0x80..=0xBF).contains(&lead),
                "the sequence for {correct:?} starts with a continuation byte",
            );
        }
    }

    #[test]
    fn a_long_ascii_buffer_is_scanned_without_touching_the_table() {
        // Not a timing assertion — a correctness one that happens to describe the fast path: the
        // scan walks a file of ordinary source and finds nothing, on any length.
        let text = "public class Order { private String name; }\n".repeat(2_000);
        assert!(find_mojibake(&text).is_empty());
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
