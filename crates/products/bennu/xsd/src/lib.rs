//! `bennu-xsd` — reading an XML Schema, for the half of a Java codebase that ships one.
//!
//! The other half of [`bennu-dtd`]'s job, for the grammar that replaced it: `pom.xml`, Spring's
//! `beans`/`context`/`mvc` namespaces, `web.xml` from Servlet 2.4 on, `persistence.xml`,
//! `faces-config.xml`.
//!
//! ## What it produces
//!
//! An [`Xsd`] — the schema's declarations, resolved as far as one file allows: named complex
//! types with their extension chains folded in, attribute and model groups expanded at their use
//! sites, `xs:documentation` attached to whatever it documents, and a byte offset on every
//! declaration so an editor can jump to it.
//!
//! What it does **not** do is decide which schema a document should be read against, merge
//! several schemas, or check a file. That is [`bennu-xml`]'s job, and keeping it out of here is
//! what lets the same questions be asked of a DTD.
//!
//! ## Two deliberate simplifications
//!
//! **Particles are flattened.** `xs:sequence`, `xs:choice` and `xs:all` all become "these
//! elements may appear here", and cardinality is kept per element rather than per group. An
//! editor's questions are *may this element appear inside that one* and *what may I type here*,
//! and both are answered by the flattened form. Reconstructing the order would let a checker
//! report an out-of-order child — and a false one of those is worse than the true ones are
//! worth, which is the standing rule for every check in bennu.
//!
//! **Namespaces are carried but not enforced.** Each declaration records the schema's target
//! namespace; matching a document's prefixes back to it is the consumer's job, because a
//! document mixing four namespaces is the normal case in Spring XML and the prefix bindings live
//! in the document, not here.
//!
//! ## Not a fetcher
//!
//! `xs:include` / `xs:import` are **recorded, not followed** ([`Xsd::includes`]). A schema
//! arrives here as text — from the project, or out of a jar entry — because a parser that opens
//! sockets cannot run where this one has to. The consumer decides what it can resolve and hands
//! the next file back in.
//!
//! [`bennu-dtd`]: https://docs.rs/bennu-dtd
//! [`bennu-xml`]: https://docs.rs/bennu-xml
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_xsd::prelude::...`.

pub mod model;
pub mod parse;
pub mod prelude;

pub use model::*;
