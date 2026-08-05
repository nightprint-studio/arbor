//! `arbor-logscan` — a log interpreter.
//!
//! A line of program output goes in; what level it is and what its parts *are* comes out.
//!
//! ```
//! use arbor_logscan::prelude::*;
//!
//! let mut reader = LogReader::new(RuleSet::java());
//! let line = reader.read("2026-08-05 12:33:01 ERROR [main] com.acme.Boot - see https://acme.test/x");
//! assert_eq!(line.level, Some(Level::Error));
//!
//! // The next line says nothing about its own severity. It does not have to.
//! let frame = reader.read("\tat com.acme.Order.total(Order.java:118)");
//! assert_eq!(frame.level, Some(Level::Error));
//! assert!(matches!(
//!     frame.links().next(),
//!     Some(Link::Source { class, line: Some(118), .. }) if class == "com.acme.Order"
//! ));
//! ```
//!
//! ## What it is for
//!
//! A console that renders a program's output as a wall of identical grey text makes you
//! read every line to find the one that matters. Every IDE therefore interprets that text:
//! the level is coloured, the timestamp and the thread recede, and a stack frame becomes a
//! link to the line it names. That is a real feature and it is entirely mechanical, which is
//! why it belongs in a crate rather than in whichever console needed it first.
//!
//! ## The shape of the answer
//!
//! [`interpret`] (or [`LogReader::read`], which remembers the line before) returns a
//! [`Line`]: the text with the ANSI escapes gone, its [`Level`], and the annotated
//! [`Span`]s over it. A host that renders across an IPC seam calls [`Line::pieces`] instead
//! and gets the text already cut up — no byte offsets cross the wire, because Rust counts
//! UTF-8 bytes and a JavaScript frontend counts UTF-16 code units, and a range that means
//! two different things on the two sides is a bug waiting for the first accented log line.
//!
//! A [`Link`] is what clicking a piece should mean. [`Link::Url`] and [`Link::File`] are
//! ready to use; [`Link::Source`] is deliberately **not resolved** — a stack frame names a
//! class, and only the host's index can turn a class into a file. Resolving it here would
//! make this a Java tool.
//!
//! ## Extending it
//!
//! A [`RuleSet`] is an ordered list, first match wins. [`RuleSet::common`] knows what every
//! log has (levels, timestamps, threads, URLs, paths); [`RuleSet::java`] adds the JVM's
//! qualified names, exceptions and stack frames. Anything else is [`RuleSet::with`] and a
//! closure returning a [`Hit`] — see [`crate::rule`]. A new dialect (a Python traceback, a
//! `cargo` diagnostic, an application's own request-id format) is one module of matchers and
//! one constructor, and it does not touch what is already here.
//!
//! ## What it is not
//!
//! Not a terminal emulator. SGR (colour, bold) becomes [`Style`]; cursor movement, erase-line
//! and the alternate screen are *discarded*, because what this produces is a transcript of
//! what a program printed and a transcript has no cursor to move.

pub mod ansi;
pub mod common;
pub mod java;
pub mod model;
pub mod prelude;
pub mod reader;
pub mod rule;
pub mod scan;

// Re-exported at the root so the crate-level docs above can link to them; the canonical
// import path is still [`prelude`].
#[doc(inline)]
pub use crate::model::{Level, Line, Link, Span, Style};
#[doc(inline)]
pub use crate::reader::{interpret, LogReader};
#[doc(inline)]
pub use crate::rule::{Hit, RuleSet};
