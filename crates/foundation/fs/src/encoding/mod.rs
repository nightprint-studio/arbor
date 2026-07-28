//! Encoding-aware decode/encode for file content surfaced to the UI, plus
//! **detection that explains itself**.
//!
//! Rationale: legacy codebases (Java, PHP, `.properties`, SQL install scripts)
//! on Windows often ship in `windows-1252` (CP1252, Latin-1 superset). Naïve
//! UTF-8 decoding either fails outright (`std::fs::read_to_string`) or produces
//! U+FFFD garbage (`from_utf8_lossy`). With the helpers here we sniff the
//! file's actual encoding once, decode losslessly, and remember the label so a
//! later write back reproduces the original byte representation.
//!
//! Foundation home: encoding ↔ bytes is a pure filesystem-content concern, so
//! it lives here in `arbor-fs`. The git-domain crate (`corvus-git`), the
//! shell's studio backends and Picus's script diagnostics all reach it through
//! this single implementation (`corvus_git::encoding` re-exports this module).
//!
//! ## Two layers
//!
//! * [`detect`] / [`decode_bytes`] / [`encode_for_disk`] & co. — the original
//!   helpers, **frozen**. They answer with an encoding and nothing else, and
//!   they claim pure-ASCII files for UTF-8. Existing callers keep that answer.
//! * [`detect_in_context`] / [`decode_in_context`] / [`encode_for_disk_strict`]
//!   — the newer surface. Same first two rungs, but the guess is labelled with
//!   an [`EncodingSource`], the ambiguous rung is resolved by an explicit
//!   [`EncodingContext`] instead of an assumption, and writes fail loudly on an
//!   unrepresentable character rather than substituting for it.
//!
//! The detection chain, in order:
//!
//! | rung | evidence | source |
//! |---|---|---|
//! | 1 | BOM | [`EncodingSource::Bom`] |
//! | 2 | valid UTF-8 with a multibyte sequence | [`EncodingSource::Utf8`] |
//! | 3 | pure ASCII → the folder's dominant encoding | [`EncodingSource::Inherited`] |
//! | 4 | anything else → the legacy single-byte encoding | [`EncodingSource::Heuristic`] |

mod codec;
mod context;
mod detect;
mod representable;

#[cfg(test)]
mod tests;

// Re-exported flat so the call-site paths stay exactly what they were
// (`arbor_fs::prelude::encoding::detect`) and there is only ever one path to
// each item.
pub use codec::{
    bom_for, decode_bytes, decode_bytes_full, decode_with, detect, encode_for_disk,
    encode_for_disk_with_bom, encoding_for_label, has_bom,
};
pub use context::EncodingContext;
pub use detect::{
    decode_in_context, detect_in_context, evidence, Detection, EncodingEvidence, EncodingSource,
    DEFAULT_LEGACY_ENCODING,
};
pub use representable::{
    check_representable, encode_for_disk_strict, encode_strict, UnrepresentableChar,
};
