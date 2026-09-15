//! Centralized XML parse helper.
//!
//! The Struts and Tiles config fragments declare a `<!DOCTYPE …>` pointing at a remote
//! DTD. `roxmltree` rejects DTDs unless `allow_dtd` is set (it never *fetches* the DTD —
//! it only tolerates the declaration). One helper so every parser opts in identically.

use roxmltree::{Document, ParsingOptions};

/// Parse XML text with DTD declarations tolerated (never fetched). Returns `None` on a
/// genuine parse error (malformed fragment) — callers treat that as skip-and-continue,
/// so one bad file never aborts a project-wide config parse (docs §8 lesson 10).
pub fn parse(text: &str) -> Option<Document<'_>> {
    let opts = ParsingOptions { allow_dtd: true, ..ParsingOptions::default() };
    Document::parse_with_options(text, opts).ok()
}
