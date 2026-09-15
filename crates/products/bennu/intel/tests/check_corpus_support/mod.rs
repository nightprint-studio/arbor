//! Support for `tests/check_corpus.rs`: reading the corpus, running Bennu over it, scoring, reporting.
//!
//! * [`golden`]  — the committed javac transcript and the inline `// error:` markers.
//! * [`anchor`]  — which source lines a diagnostic stands for (its statement, when that spans lines).
//! * [`module`]  — one corpus module indexed against the real JDK and validated file by file.
//! * [`verdict`] — javac vs Bennu: hits, misses, false positives, mislabels.
//! * [`report`]  — the markdown report and the failure listing.
//! * [`clean`]   — the clean modules: legal code where every Bennu diagnostic is a false positive.

pub mod anchor;
pub mod clean;
pub mod golden;
pub mod module;
pub mod report;
pub mod verdict;
