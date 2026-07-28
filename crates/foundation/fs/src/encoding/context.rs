//! The context an ambiguous (pure-ASCII) file inherits its encoding from.
//!
//! Why an explicit object instead of "just look at the folder": detection has
//! to stay a pure function of its inputs to be testable and reproducible, and
//! *which* files are allowed to vote is policy the caller owns (a Picus script
//! folder votes with its `.sql` files, not with its `README.md`). So the
//! caller scans, feeds the bytes in, and passes the resulting context down.
//! This module never touches the filesystem.

use encoding_rs::Encoding;

use super::detect::{evidence, EncodingEvidence, DEFAULT_LEGACY_ENCODING};

/// The dominant-encoding context for one folder.
///
/// Build it by folding the folder's files through [`EncodingContext::observe`]
/// (or [`EncodingContext::from_samples`]), then pass it to
/// [`super::detect_in_context`] for every file in that folder.
#[derive(Debug, Clone)]
pub struct EncodingContext {
    /// Set by [`EncodingContext::with_dominant`]: an explicit answer from
    /// config, which outranks the vote.
    pinned: Option<&'static Encoding>,
    /// The single-byte encoding used for non-UTF-8 bytes and as the last-resort
    /// answer for ambiguous ones.
    legacy: &'static Encoding,
    /// One entry per encoding that got at least one vote. A `Vec` rather than a
    /// map because the realistic candidate count is two or three, and a `Vec`
    /// has no iteration-order surprises.
    votes: Vec<(&'static Encoding, usize)>,
}

impl Default for EncodingContext {
    fn default() -> Self {
        EncodingContext::new()
    }
}

impl EncodingContext {
    /// An empty context: nothing has voted, legacy = windows-1252.
    pub fn new() -> Self {
        EncodingContext {
            pinned: None,
            legacy: DEFAULT_LEGACY_ENCODING,
            votes:  Vec::new(),
        }
    }

    /// Override the legacy single-byte encoding (e.g. a repository that is
    /// actually ISO-8859-15). Chainable.
    pub fn with_legacy(mut self, encoding: &'static Encoding) -> Self {
        self.legacy = encoding;
        self
    }

    /// Pin the dominant encoding, skipping the vote — for when the project
    /// configuration already states what the folder must be. Observations are
    /// still tallied, so the caller can show that the folder disagrees with
    /// its own declaration.
    pub fn with_dominant(mut self, encoding: &'static Encoding) -> Self {
        self.pinned = Some(encoding);
        self
    }

    /// Fold one file's bytes into the vote and return what those bytes proved,
    /// so a caller building the context can record per-file evidence without a
    /// second scan. Ambiguous (pure-ASCII) files deliberately cast no vote —
    /// they are the ones being decided.
    pub fn observe(&mut self, bytes: &[u8]) -> EncodingEvidence {
        let ev = evidence(bytes);
        match ev {
            EncodingEvidence::Bom(enc)     => self.cast(enc),
            EncodingEvidence::Utf8Multibyte => self.cast(encoding_rs::UTF_8),
            EncodingEvidence::SingleByte    => self.cast(self.legacy),
            EncodingEvidence::Ascii         => {}
        }
        ev
    }

    /// Build a context from a folder's file contents in one go.
    pub fn from_samples<'a, I>(samples: I) -> Self
    where
        I: IntoIterator<Item = &'a [u8]>,
    {
        let mut ctx = EncodingContext::new();
        for bytes in samples {
            ctx.observe(bytes);
        }
        ctx
    }

    /// The encoding ambiguous files inherit, or `None` when no file in the
    /// folder was decidable (empty folder, or every file pure ASCII).
    pub fn dominant(&self) -> Option<&'static Encoding> {
        self.pinned.or_else(|| self.tally().first().map(|(enc, _)| *enc))
    }

    /// The legacy single-byte encoding this context assumes.
    pub fn legacy(&self) -> &'static Encoding {
        self.legacy
    }

    /// `true` when nothing decidable has been observed **and** nothing was
    /// pinned, i.e. [`EncodingContext::dominant`] is `None`.
    pub fn is_empty(&self) -> bool {
        self.dominant().is_none()
    }

    /// The vote, strongest first. Reporting this is what makes the inheritance
    /// explainable in the UI ("6 windows-1252, 1 UTF-8 → folder is CP1252").
    ///
    /// The order is the tie-break rule, applied in this priority:
    /// 1. **more votes wins** — plurality, not majority;
    /// 2. on a tie, the **legacy encoding** wins if it is one of the tied
    ///    candidates. Inheriting the legacy label is the conservative choice:
    ///    an ASCII file's bytes are identical under either candidate, so the
    ///    label only feeds the "this folder expects X" diagnostic, and that
    ///    diagnostic must stay stable rather than flip when one file is added;
    /// 3. still tied → the encoding whose canonical name sorts first. Arbitrary
    ///    but **total and order-independent**, which is the property that
    ///    matters: the answer must not depend on directory-iteration order.
    pub fn tally(&self) -> Vec<(&'static Encoding, usize)> {
        let mut out = self.votes.clone();
        out.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| (a.0 != self.legacy).cmp(&(b.0 != self.legacy)))
                .then_with(|| a.0.name().cmp(b.0.name()))
        });
        out
    }

    fn cast(&mut self, encoding: &'static Encoding) {
        match self.votes.iter_mut().find(|(enc, _)| *enc == encoding) {
            Some((_, count)) => *count += 1,
            None => self.votes.push((encoding, 1)),
        }
    }
}
