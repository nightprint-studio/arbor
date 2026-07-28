//! Reading a file so that it can be written back **unchanged**.
//!
//! Picus edits legacy repositories: windows-1252, CRLF, occasionally a BOM. Every
//! one of those is a way to accidentally rewrite a whole file while intending to
//! add three lines. [`SourceText`] carries the original bytes alongside the
//! decoded text precisely so the question "would saving this file change anything
//! I did not intend?" can be answered **before** anything is written.
//!
//! That check is [`SourceText::verify_round_trip`], and it is the load-bearing
//! idea of this crate: encode the decoded text back and compare it with the bytes
//! that came off disk. If they differ — a file that is not valid in the encoding
//! we detected, a lossy decode that produced U+FFFD — Picus refuses to write to
//! that file at all. A tool that cannot reproduce a file has no business saving
//! it.

use std::path::{Path, PathBuf};

use arbor_fs::prelude::encoding::{bom_for, decode_in_context, encode_strict, EncodingContext};
use encoding_rs::Encoding;

use crate::error::RewriteError;

/// How a file's lines end, as found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    Crlf,
    Lf,
}

impl Eol {
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
        }
    }

    /// Majority wins; a file with no line ending at all is treated as CRLF,
    /// because these repositories are Windows-authored and inventing LF in one of
    /// them turns a three-line addition into a whole-file diff.
    pub fn detect(text: &str) -> Eol {
        let lf_total = text.matches('\n').count();
        let crlf = text.matches("\r\n").count();
        let lone_lf = lf_total - crlf;
        if lone_lf > crlf {
            Eol::Lf
        } else {
            Eol::Crlf
        }
    }

    /// Rewrite `text`'s line endings to this one.
    ///
    /// Generated SQL arrives with `\n` because that is what a Rust string literal
    /// holds. Splicing it into a CRLF file unchanged produces a file with mixed
    /// endings, which every diff tool then reports as a change to lines nobody
    /// touched.
    pub fn normalise(self, text: &str) -> String {
        let unix = text.replace("\r\n", "\n");
        match self {
            Eol::Lf => unix,
            Eol::Crlf => unix.replace('\n', "\r\n"),
        }
    }
}

/// A file as read, with everything needed to write it back the way it was.
#[derive(Debug, Clone)]
pub struct SourceText {
    pub path: PathBuf,
    /// Exactly what was on disk. Kept so the round trip can be *verified* rather
    /// than assumed, and so a rollback has something to restore.
    pub bytes: Vec<u8>,
    /// The decoded text, with any BOM removed.
    pub text: String,
    pub encoding: &'static Encoding,
    pub had_bom: bool,
    pub eol: Eol,
    /// `false` when the file does not exist yet and is about to be created.
    pub exists: bool,
}

impl SourceText {
    /// Decode bytes that have already been read. Pure — this is the half worth
    /// testing, and it is tested without a filesystem.
    pub fn from_bytes(path: impl Into<PathBuf>, bytes: Vec<u8>, ctx: &EncodingContext) -> SourceText {
        let (text, detection) = decode_in_context(&bytes, ctx);
        let eol = Eol::detect(&text);
        SourceText {
            path: path.into(),
            bytes,
            text,
            encoding: detection.encoding,
            had_bom: detection.had_bom,
            eol,
            exists: true,
        }
    }

    /// A file that does not exist yet, to be created with the folder's own
    /// conventions rather than with whatever the platform defaults to.
    pub fn new_file(
        path: impl Into<PathBuf>,
        encoding: &'static Encoding,
        eol: Eol,
    ) -> SourceText {
        SourceText {
            path: path.into(),
            bytes: Vec::new(),
            text: String::new(),
            encoding,
            had_bom: false,
            eol,
            exists: false,
        }
    }

    /// Read a file from disk, or return an empty [`SourceText::new_file`] when it
    /// is not there.
    pub fn read(
        path: &Path,
        ctx: &EncodingContext,
        fallback_encoding: &'static Encoding,
        fallback_eol: Eol,
    ) -> Result<SourceText, RewriteError> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(SourceText::from_bytes(path, bytes, ctx)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(SourceText::new_file(path, fallback_encoding, fallback_eol))
            }
            Err(e) => Err(RewriteError::Io { path: path.to_path_buf(), reason: e.to_string() }),
        }
    }

    /// Encode text back into this file's own encoding, BOM included if it had one.
    pub fn encode(&self, text: &str) -> Result<Vec<u8>, RewriteError> {
        let body = encode_strict(text, self.encoding).map_err(|e| RewriteError::Unrepresentable {
            path: self.path.clone(),
            detail: e.to_string(),
        })?;
        let Some(bom) = bom_for(self.encoding).filter(|_| self.had_bom) else {
            return Ok(body);
        };
        let mut out = Vec::with_capacity(bom.len() + body.len());
        out.extend_from_slice(bom);
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// **The guard.** Can this file be written back exactly as it was found?
    ///
    /// Called before any edit is prepared. A file that fails here is one Picus
    /// decoded in a way it cannot undo — a mis-detected encoding, or bytes that
    /// are not valid in the encoding we chose and came back as U+FFFD. Writing it
    /// would silently rewrite parts of the file the user never touched, which is
    /// exactly the harm this crate exists to avoid.
    pub fn verify_round_trip(&self) -> Result<(), RewriteError> {
        if !self.exists {
            return Ok(());
        }
        let reproduced = self.encode(&self.text)?;
        if reproduced == self.bytes {
            return Ok(());
        }
        Err(RewriteError::NotReproducible {
            path: self.path.clone(),
            detail: describe_difference(&self.bytes, &reproduced, self.encoding.name()),
        })
    }
}

/// Say *where* the reproduction diverged, so the user can look at it.
fn describe_difference(original: &[u8], reproduced: &[u8], encoding: &str) -> String {
    let at = original
        .iter()
        .zip(reproduced.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| original.len().min(reproduced.len()));
    if original.len() != reproduced.len() && at == original.len().min(reproduced.len()) {
        return format!(
            "read as {encoding}, it comes back {} bytes instead of {}",
            reproduced.len(),
            original.len()
        );
    }
    format!("read as {encoding}, byte {at} comes back different")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp1252_context() -> EncodingContext {
        EncodingContext::new().with_legacy(encoding_rs::WINDOWS_1252)
    }

    fn cp1252(text: &str) -> Vec<u8> {
        encoding_rs::WINDOWS_1252.encode(text).0.into_owned()
    }

    #[test]
    fn a_windows_1252_file_reproduces_itself_exactly() {
        // The property the whole crate rests on, on the encoding these
        // repositories actually use.
        let bytes = cp1252("-- soglia già applicata\r\nINSERT INTO A VALUES ('città');\r\n");
        let source = SourceText::from_bytes("x.sql", bytes.clone(), &cp1252_context());
        assert_eq!(source.encoding.name(), "windows-1252");
        assert_eq!(source.eol, Eol::Crlf);
        source.verify_round_trip().expect("reproducible");
        assert_eq!(source.encode(&source.text).unwrap(), bytes);
    }

    #[test]
    fn a_utf8_file_with_a_bom_reproduces_the_bom_too() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("-- perché\n".as_bytes());
        let source = SourceText::from_bytes("x.sql", bytes.clone(), &cp1252_context());
        assert!(source.had_bom);
        // The BOM is not in the text…
        assert!(!source.text.starts_with('\u{feff}'));
        // …but it comes back on the way out.
        source.verify_round_trip().expect("reproducible");
        assert_eq!(source.encode(&source.text).unwrap(), bytes);
    }

    #[test]
    fn a_file_that_cannot_be_reproduced_is_refused_before_anything_is_written() {
        // Bytes that are not valid UTF-8 but were detected as UTF-8 would decode
        // to U+FFFD and never come back. Simulate the situation directly: a file
        // whose declared encoding cannot round-trip its own bytes.
        let mut source = SourceText::from_bytes("x.sql", cp1252("caffè"), &cp1252_context());
        // Pretend detection got it wrong.
        source.encoding = encoding_rs::UTF_8;
        let err = source.verify_round_trip().expect_err("must refuse");
        assert!(matches!(err, RewriteError::NotReproducible { .. }));
        assert!(err.to_string().contains("will not write"));
    }

    #[test]
    fn a_character_the_encoding_cannot_hold_fails_by_name() {
        let source = SourceText::from_bytes("x.sql", cp1252("-- ok\r\n"), &cp1252_context());
        let err = source.encode("-- 日本語\r\n").expect_err("cp1252 cannot hold this");
        assert!(matches!(err, RewriteError::Unrepresentable { .. }));
        // The message names the character, not just "encoding error".
        assert!(err.to_string().contains('日'));
    }

    #[test]
    fn line_endings_are_detected_and_generated_text_is_converted() {
        assert_eq!(Eol::detect("a\r\nb\r\n"), Eol::Crlf);
        assert_eq!(Eol::detect("a\nb\n"), Eol::Lf);
        assert_eq!(Eol::detect("no newline"), Eol::Crlf);
        // Mixed: the majority decides.
        assert_eq!(Eol::detect("a\r\nb\r\nc\n"), Eol::Crlf);

        assert_eq!(Eol::Crlf.normalise("a\nb\n"), "a\r\nb\r\n");
        assert_eq!(Eol::Lf.normalise("a\r\nb\r\n"), "a\nb\n");
        // Idempotent — converting twice must not double the carriage returns.
        assert_eq!(Eol::Crlf.normalise(&Eol::Crlf.normalise("a\nb")), "a\r\nb");
    }

    #[test]
    fn a_file_that_does_not_exist_yet_carries_the_conventions_it_will_be_born_with() {
        let source = SourceText::new_file("new.sql", encoding_rs::WINDOWS_1252, Eol::Crlf);
        assert!(!source.exists);
        assert!(source.text.is_empty());
        // Nothing to reproduce, so the guard passes rather than failing on empty.
        source.verify_round_trip().expect("a new file is trivially reproducible");
    }

    /// The corpus the round-trip property is asserted over.
    ///
    /// Deliberately awkward: the shapes that actually appear in these
    /// repositories, plus the ones that historically break naive text handling.
    /// This is the most important test in the crate — everything else is only
    /// safe because this holds.
    fn corpus() -> Vec<(&'static str, Vec<u8>)> {
        let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
        utf8_bom.extend_from_slice("-- perché\r\nSELECT 1;\r\n".as_bytes());

        vec![
            ("empty", Vec::new()),
            ("ascii crlf", cp1252("SELECT 1;\r\nSELECT 2;\r\n")),
            ("ascii lf", cp1252("SELECT 1;\nSELECT 2;\n")),
            ("accents cp1252", cp1252("-- città, perché, però\r\nINSERT INTO A VALUES ('è');\r\n")),
            ("euro and quotes", cp1252("-- 15€ “virgolette” — trattino\r\n")),
            ("utf8 multibyte", "-- città\n-- 日本語\n".as_bytes().to_vec()),
            ("utf8 with bom", utf8_bom),
            ("mixed line endings", cp1252("a\r\nb\nc\r\n")),
            ("no trailing newline", cp1252("SELECT 1;")),
            ("lone cr", cp1252("a\rb\r\n")),
            ("tabs and trailing spaces", cp1252("SELECT\t1;   \r\n\t\r\n")),
            ("blank lines only", cp1252("\r\n\r\n\r\n")),
            ("very long line", cp1252(&format!("-- {}\r\n", "x".repeat(20_000)))),
            ("nul-free control bytes", cp1252("a\x0Cb\r\n")),
        ]
    }

    #[test]
    fn every_file_in_the_corpus_reproduces_itself_byte_for_byte() {
        for (name, bytes) in corpus() {
            let source = SourceText::from_bytes("x.sql", bytes.clone(), &cp1252_context());
            source
                .verify_round_trip()
                .unwrap_or_else(|e| panic!("`{name}` is not reproducible: {e}"));
            assert_eq!(
                source.encode(&source.text).unwrap(),
                bytes,
                "`{name}` did not come back byte for byte"
            );
        }
    }

    #[test]
    fn an_empty_existing_file_is_still_reproducible() {
        let source = SourceText::from_bytes("empty.sql", Vec::new(), &cp1252_context());
        source.verify_round_trip().expect("reproducible");
        assert_eq!(source.encode("").unwrap(), Vec::<u8>::new());
    }
}
