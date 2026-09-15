//! Structural search & replace — find code by its **shape**, count it, and rewrite it.
//!
//! ## What it is for
//!
//! A text search knows nothing about the language, so `log.debug("x" + y)` and
//! ```text
//! log
//!   .debug( "x"
//!         + y )
//! ```
//! are two different strings and one construct. A structural search compares *nodes*, so
//! whitespace, line breaks and interleaved comments do not take part — and a capture can be moved
//! in a replacement, which is the thing no textual find/replace can do at all
//! (`assertEquals($msg$, $a$, $b$)` → `assertEquals($a$, $b$, $msg$)`).
//!
//! ## The four modules
//!
//! * [`query`] — the language: what you type, and the [`Query`](query::Query) it parses to.
//! * [`engine`] — running it: compiling the alternatives, matching, filtering by constraint,
//!   de-duplicating.
//! * [`report`] — the table `group` asks for.
//! * [`replace`] — the template, and the edits it produces.
//!
//! ## What this crate deliberately does not know
//!
//! **Java.** The grammar arrives as a parameter, as it does for the crate underneath
//! ([`arbor_syntax`]) — Picus points the same code at SQL.
//!
//! **Types.** Deciding that `svc` is a `com.acme.OrderService` needs the classpath, the imports
//! and local inference, all of which live behind Bennu's index. So it is a trait
//! ([`engine::TypeOracle`]) the caller implements: the tests here hand it a two-line fake, and the
//! backend hands it the resolver. That is what keeps every rule in here exhaustively testable
//! without a project on disk.
//!
//! **The filesystem.** A [`engine::Subject`] is a path and a string. Which files those are, and
//! how they were decoded, is the caller's problem.

pub mod engine;
pub mod prelude;
pub mod query;
pub mod replace;
pub mod report;
