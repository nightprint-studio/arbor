//! Unit tests for the encoding layer.
//!
//! Two kinds live here on purpose:
//! * tests of the **new** detection/encode surface, and
//! * **characterisation** tests that pin what the frozen helpers already do
//!   (including two known warts), so a future change to them is a deliberate
//!   act and not an accident — `corvus-git`, the studio backends and the shell
//!   all read through those functions.

use encoding_rs::{UTF_16BE, UTF_16LE, UTF_8, WINDOWS_1252};

use super::*;

/// The accented set an Italian install script actually contains, plus the euro
/// sign — the character that lives at 0x80 in CP1252 and nowhere in Latin-1.
const ACCENTS: &str = "à è ì ò ù €";

fn cp1252(text: &str) -> Vec<u8> {
    encode_strict(text, WINDOWS_1252).expect("sample must be CP1252-representable")
}

// ── Rung 4: legacy single-byte ───────────────────────────────────────────────

#[test]
fn windows_1252_accents_round_trip() {
    let bytes = cp1252(ACCENTS);
    assert_eq!(
        bytes,
        vec![0xE0, 0x20, 0xE8, 0x20, 0xEC, 0x20, 0xF2, 0x20, 0xF9, 0x20, 0x80],
        "CP1252 must encode the accents as single bytes, € at 0x80",
    );

    let ctx = EncodingContext::new();
    let (text, detection) = decode_in_context(&bytes, &ctx);

    assert_eq!(text, ACCENTS);
    assert_eq!(detection.label(), "windows-1252");
    assert_eq!(detection.source, EncodingSource::Heuristic);
    assert!(!detection.had_bom);
    assert_eq!(encode_strict(&text, detection.encoding).unwrap(), bytes);
}

#[test]
fn crlf_survives_decode() {
    // Picus's scripts are CRLF; a detector that normalised line endings would
    // rewrite every file it touched.
    let bytes = cp1252("INSERT à\r\nINSERT è\r\n");
    let (text, _) = decode_in_context(&bytes, &EncodingContext::new());
    assert_eq!(text, "INSERT à\r\nINSERT è\r\n");
}

#[test]
fn legacy_encoding_is_configurable() {
    let ctx = EncodingContext::new().with_legacy(encoding_rs::ISO_8859_15);
    let detection = detect_in_context(&[0xE0, 0xA4], &ctx);
    assert_eq!(detection.label(), "ISO-8859-15");
    assert_eq!(detection.source, EncodingSource::Heuristic);
}

// ── Rung 2: UTF-8 with real evidence ─────────────────────────────────────────

#[test]
fn utf8_multibyte_is_evidence() {
    let bytes = ACCENTS.as_bytes();
    assert_eq!(evidence(bytes), EncodingEvidence::Utf8Multibyte);

    let detection = detect_in_context(bytes, &EncodingContext::new());
    assert_eq!(detection.encoding, UTF_8);
    assert_eq!(detection.source, EncodingSource::Utf8);
    assert!(!detection.source.is_guess(), "multibyte UTF-8 is proof, not a guess");
}

#[test]
fn utf8_wins_even_inside_a_cp1252_folder() {
    // The whole point of rung 2: a file that IS valid multibyte UTF-8 is never
    // relabelled by the folder — otherwise the ENC001 diagnostic could not fire.
    let ctx = EncodingContext::new().with_dominant(WINDOWS_1252);
    let detection = detect_in_context(ACCENTS.as_bytes(), &ctx);
    assert_eq!(detection.encoding, UTF_8);
    assert_eq!(detection.source, EncodingSource::Utf8);
}

// ── Rung 1: BOMs ─────────────────────────────────────────────────────────────

fn with_bom(encoding: &'static encoding_rs::Encoding, text: &str) -> Vec<u8> {
    encode_for_disk_strict(text, Some(encoding.name()), true).unwrap()
}

#[test]
fn utf8_bom_is_detected_and_not_leaked() {
    let bytes = with_bom(UTF_8, "select 1;");
    assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);

    let (text, detection) = decode_in_context(&bytes, &EncodingContext::new());
    assert_eq!(detection.encoding, UTF_8);
    assert_eq!(detection.source, EncodingSource::Bom);
    assert!(detection.had_bom);
    assert_eq!(text, "select 1;", "the BOM must not reach the decoded text");
    assert!(!text.contains('\u{FEFF}'));
}

#[test]
fn utf16le_bom_is_detected_and_not_leaked() {
    let bytes = with_bom(UTF_16LE, ACCENTS);
    assert_eq!(&bytes[..2], &[0xFF, 0xFE]);

    let (text, detection) = decode_in_context(&bytes, &EncodingContext::new());
    assert_eq!(detection.encoding, UTF_16LE);
    assert_eq!(detection.source, EncodingSource::Bom);
    assert!(detection.had_bom);
    assert_eq!(text, ACCENTS);
    assert!(!text.contains('\u{FEFF}'));
}

#[test]
fn utf16be_bom_is_detected_and_not_leaked() {
    let bytes = with_bom(UTF_16BE, ACCENTS);
    assert_eq!(&bytes[..2], &[0xFE, 0xFF]);

    let (text, detection) = decode_in_context(&bytes, &EncodingContext::new());
    assert_eq!(detection.encoding, UTF_16BE);
    assert_eq!(detection.source, EncodingSource::Bom);
    assert_eq!(text, ACCENTS);
    assert!(!text.contains('\u{FEFF}'));
}

#[test]
fn lone_bom_decodes_to_nothing() {
    let bytes = vec![0xEF, 0xBB, 0xBF];
    let (text, detection) = decode_in_context(&bytes, &EncodingContext::new());
    assert_eq!(text, "", "a file that is only a BOM has no content");
    assert_eq!(detection.encoding, UTF_8);
    assert_eq!(detection.source, EncodingSource::Bom);
    assert!(detection.had_bom);
    // Round-trip: had_bom is what lets the save put the three bytes back.
    assert_eq!(encode_for_disk_strict(&text, Some("UTF-8"), detection.had_bom).unwrap(), bytes);
}

// ── Rung 3: ambiguity and inheritance ────────────────────────────────────────

#[test]
fn pure_ascii_is_ambiguous() {
    let ev = evidence(b"SELECT * FROM users;");
    assert_eq!(ev, EncodingEvidence::Ascii);
    assert!(ev.is_ambiguous(), "ASCII is valid UTF-8 *and* valid CP1252");
}

#[test]
fn empty_file_is_ambiguous() {
    assert_eq!(evidence(b""), EncodingEvidence::Ascii);
    let (text, detection) = decode_in_context(b"", &EncodingContext::new());
    assert_eq!(text, "");
    assert!(!detection.had_bom);
    assert!(detection.source.is_guess());
}

#[test]
fn ascii_inherits_the_folder_vote() {
    let utf8_file = ACCENTS.as_bytes().to_vec();
    let ascii_file = b"SELECT 1;".to_vec();
    let ctx = EncodingContext::from_samples([utf8_file.as_slice(), ascii_file.as_slice()]);

    assert_eq!(ctx.dominant(), Some(UTF_8));
    let detection = detect_in_context(&ascii_file, &ctx);
    assert_eq!(detection.encoding, UTF_8);
    assert_eq!(detection.source, EncodingSource::Inherited);
}

#[test]
fn ambiguous_files_cast_no_vote() {
    // Otherwise a folder of mostly-ASCII files would drown out its own evidence.
    let ctx = EncodingContext::from_samples([&b"SELECT 1;"[..], &b"SELECT 2;"[..]]);
    assert!(ctx.tally().is_empty());
    assert!(ctx.is_empty());

    let detection = detect_in_context(b"SELECT 1;", &ctx);
    assert_eq!(detection.encoding, WINDOWS_1252, "falls back to the legacy encoding");
    assert_eq!(
        detection.source,
        EncodingSource::Heuristic,
        "nothing to inherit from, so it is a guess and must say so",
    );
}

#[test]
fn a_single_utf8_intruder_does_not_flip_the_folder() {
    let cp = cp1252("città");
    let mut samples: Vec<&[u8]> = vec![cp.as_slice(); 8];
    let intruder = "città".as_bytes();
    samples.push(intruder);

    let ctx = EncodingContext::from_samples(samples);
    assert_eq!(ctx.dominant(), Some(WINDOWS_1252));
    assert_eq!(ctx.tally(), vec![(WINDOWS_1252, 8), (UTF_8, 1)]);
}

#[test]
fn pinned_dominant_outranks_the_vote() {
    let mut ctx = EncodingContext::new().with_dominant(WINDOWS_1252);
    ctx.observe(ACCENTS.as_bytes());
    ctx.observe(ACCENTS.as_bytes());

    assert_eq!(ctx.dominant(), Some(WINDOWS_1252), "config wins over the sample");
    assert_eq!(
        ctx.tally(),
        vec![(UTF_8, 2)],
        "the vote is still visible so the disagreement can be reported",
    );
}

// ── Tie-break ────────────────────────────────────────────────────────────────

#[test]
fn split_vote_ties_to_the_legacy_encoding() {
    let cp = cp1252("città");
    let utf = "città".as_bytes();

    // Both orders, because the whole point is order-independence.
    let a = EncodingContext::from_samples([cp.as_slice(), utf]);
    let b = EncodingContext::from_samples([utf, cp.as_slice()]);

    assert_eq!(a.dominant(), Some(WINDOWS_1252));
    assert_eq!(b.dominant(), Some(WINDOWS_1252));
    assert_eq!(a.tally(), b.tally());

    let detection = detect_in_context(b"SELECT 1;", &a);
    assert_eq!(detection.source, EncodingSource::Inherited);
    assert_eq!(detection.encoding, WINDOWS_1252);
}

#[test]
fn split_vote_without_the_legacy_encoding_ties_by_name() {
    // Neither candidate is the legacy encoding → the deterministic last resort
    // is the canonical name, ascending: "UTF-16LE" < "UTF-8".
    let utf16 = with_bom(UTF_16LE, "a");
    let utf8 = "à".as_bytes();

    let a = EncodingContext::from_samples([utf16.as_slice(), utf8]);
    let b = EncodingContext::from_samples([utf8, utf16.as_slice()]);

    assert_eq!(a.dominant(), Some(UTF_16LE));
    assert_eq!(b.dominant(), Some(UTF_16LE), "order must not change the answer");
}

// ── Representability ─────────────────────────────────────────────────────────

#[test]
fn unrepresentable_char_is_named_and_located() {
    let text = "INSERT ok;\nVALUES ('日');";
    let err = check_representable(text, WINDOWS_1252).expect_err("日 is not in CP1252");

    assert_eq!(err.ch, '日');
    assert_eq!(err.encoding, "windows-1252");
    assert_eq!(err.line, 2);
    assert_eq!(err.column, 10);
    assert_eq!(err.char_index, 20);
    assert!(
        err.to_string().contains('日') && err.to_string().contains("U+65E5"),
        "the message must identify the character: {err}",
    );
}

#[test]
fn strict_encode_refuses_what_the_lossy_one_mangles() {
    let text = "VALUES ('日')";

    // What the frozen helper does today: substitutes and returns bytes.
    let lossy = encode_for_disk(text, Some("windows-1252"));
    let lossy_text = decode_with(&lossy, WINDOWS_1252);
    assert_ne!(lossy_text, text, "the frozen path corrupts silently");

    assert!(encode_strict(text, WINDOWS_1252).is_err());
    assert!(encode_for_disk_strict(text, Some("windows-1252"), false).is_err());
}

#[test]
fn accents_and_unicode_targets_are_representable() {
    assert!(check_representable(ACCENTS, WINDOWS_1252).is_ok());
    // Any str is valid Unicode, so the Unicode targets never reject anything.
    for enc in [UTF_8, UTF_16LE, UTF_16BE] {
        assert!(check_representable("日本語 — à", enc).is_ok());
    }
}

#[test]
fn strict_encode_writes_real_utf16() {
    let bytes = encode_strict("Aà", UTF_16LE).unwrap();
    assert_eq!(bytes, vec![0x41, 0x00, 0xE0, 0x00]);

    let bytes_be = encode_strict("Aà", UTF_16BE).unwrap();
    assert_eq!(bytes_be, vec![0x00, 0x41, 0x00, 0xE0]);
}

// ── Mojibake: the case the whole feature exists for ──────────────────────────

#[test]
fn cp1252_lossily_read_as_utf8_becomes_utf8_with_replacement_chars() {
    // An outside tool read the CP1252 script as UTF-8, hit an invalid byte,
    // dropped in U+FFFD, and saved. The bytes are now genuinely UTF-8.
    let original = cp1252("città");
    let mangled = String::from_utf8_lossy(&original).into_owned();
    let rewritten = mangled.as_bytes();

    let ctx = EncodingContext::new().with_dominant(WINDOWS_1252);
    let (text, detection) = decode_in_context(rewritten, &ctx);

    assert_eq!(detection.encoding, UTF_8, "the file really is UTF-8 now");
    assert_eq!(detection.source, EncodingSource::Utf8);
    assert!(text.contains('\u{FFFD}'), "the accent was destroyed, not converted");
    assert_ne!(text, "città");

    // And the damage cannot be written back into a CP1252 folder unnoticed:
    // U+FFFD has no CP1252 encoding.
    let err = check_representable(&text, WINDOWS_1252).expect_err("U+FFFD is not in CP1252");
    assert_eq!(err.ch, '\u{FFFD}');
}

#[test]
fn double_encoded_mojibake_is_utf8_and_only_the_folder_reveals_it() {
    // The other direction, the nastier one: a UTF-8 file read as CP1252 and
    // saved as UTF-8. "città" → "cittÃ\u{A0}". Every character is legal, the
    // bytes are valid UTF-8, and the text is *representable* in CP1252 — so no
    // encode check can catch it. Only "detected UTF-8 inside a CP1252 folder"
    // does, which is exactly what the ENC001 diagnostic is built on.
    let utf8_original = "città".as_bytes();
    let misread = decode_with(utf8_original, WINDOWS_1252);
    assert_eq!(misread, "cittÃ\u{A0}");

    let rewritten = misread.as_bytes();
    let ctx = EncodingContext::new().with_dominant(WINDOWS_1252);
    let detection = detect_in_context(rewritten, &ctx);

    assert_eq!(detection.encoding, UTF_8);
    assert_eq!(detection.source, EncodingSource::Utf8);
    assert!(check_representable(&misread, WINDOWS_1252).is_ok(), "no write-side signal");
    assert_ne!(
        Some(detection.encoding),
        ctx.dominant(),
        "the only signal is the mismatch with the folder",
    );
}

// ── Wire contract ────────────────────────────────────────────────────────────

#[test]
fn encoding_source_words_match_the_frontend_union() {
    // src/lib/types/picus/index.ts: 'bom' | 'utf8' | 'inherited' | 'heuristic' | 'forced'
    assert_eq!(EncodingSource::Bom.as_str(), "bom");
    assert_eq!(EncodingSource::Utf8.as_str(), "utf8");
    assert_eq!(EncodingSource::Inherited.as_str(), "inherited");
    assert_eq!(EncodingSource::Heuristic.as_str(), "heuristic");
    assert_eq!(EncodingSource::Forced.as_str(), "forced");
    assert_eq!(EncodingSource::Forced.to_string(), "forced");
}

#[test]
fn forced_detection_reports_forced() {
    let detection = Detection::forced(WINDOWS_1252, false);
    assert_eq!(detection.source, EncodingSource::Forced);
    assert_eq!(detection.label(), "windows-1252");
    assert!(!detection.source.is_guess(), "an explicit choice is not a guess");
}

// ── Characterisation: the frozen helpers must not shift ──────────────────────

#[test]
fn frozen_detect_still_claims_ascii_for_utf8() {
    // The old contract Bennu/Corvus depend on: ASCII → UTF-8, no ambiguity.
    assert_eq!(detect(b"SELECT 1;"), UTF_8);
    assert_eq!(detect(ACCENTS.as_bytes()), UTF_8);
    assert_eq!(detect(&cp1252(ACCENTS)), WINDOWS_1252);
    assert_eq!(detect(&[0xEF, 0xBB, 0xBF]), UTF_8);

    // …while the new chain calls the same ASCII bytes ambiguous.
    assert!(evidence(b"SELECT 1;").is_ambiguous());
}

#[test]
fn frozen_decode_bytes_full_still_leaks_the_bom() {
    // Deliberately pinned, not fixed: the studio backends (FROZEN F16) strip
    // the U+FEFF themselves and corvus round-trips it. `decode_in_context` is
    // the path that removes it.
    let bytes = [0xEF, 0xBB, 0xBF, b'a'];
    let (text, enc, had_bom) = decode_bytes_full(&bytes);
    assert_eq!(text, "\u{FEFF}a");
    assert_eq!(enc, UTF_8);
    assert!(had_bom);

    let (clean, _) = decode_in_context(&bytes, &EncodingContext::new());
    assert_eq!(clean, "a");
}

#[test]
fn frozen_encode_for_disk_cannot_write_utf16() {
    // Known limitation pinned so it stays visible: `encoding_rs` has no UTF-16
    // encoder, so this writes UTF-8 bytes under a UTF-16LE BOM. Use
    // `encode_for_disk_strict` instead — which is what the studio save paths now
    // do, so nothing in the app reaches the broken behaviour below; this test
    // exists to keep the trap documented for the next caller who finds the
    // shorter name first.
    let broken = encode_for_disk_with_bom("Aà", Some("utf-16le"), true);
    assert_eq!(broken, vec![0xFF, 0xFE, 0x41, 0xC3, 0xA0]);

    let correct = encode_for_disk_strict("Aà", Some("utf-16le"), true).unwrap();
    assert_eq!(correct, vec![0xFF, 0xFE, 0x41, 0x00, 0xE0, 0x00]);
    let (round_tripped, detection) = decode_in_context(&correct, &EncodingContext::new());
    assert_eq!(round_tripped, "Aà");
    assert_eq!(detection.encoding, UTF_16LE);
}

#[test]
fn frozen_helpers_keep_their_labels() {
    assert_eq!(encoding_for_label("windows-1252"), WINDOWS_1252);
    assert_eq!(encoding_for_label("nonsense"), UTF_8);
    assert!(has_bom(&[0xFF, 0xFE, 0x00]));
    assert!(!has_bom(b"SELECT 1;"));
    assert_eq!(bom_for(UTF_8), Some(&[0xEF, 0xBB, 0xBF][..]));
    assert_eq!(bom_for(WINDOWS_1252), None);
}
