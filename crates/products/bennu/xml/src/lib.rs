//! `bennu-xml` — schema-driven editing for the XML a Java project is made of.
//!
//! ## The shape of the problem
//!
//! An XML file in a Java project is a configuration language whose vocabulary is written down,
//! precisely, in a file the editor can read — and almost no editor reads it. So `struts.xml`,
//! `web.xml`, `pom.xml` and `beans.xml` are edited as prose: you remember the tag names or you
//! look them up, and a misspelling is found at deploy time.
//!
//! Everything here follows from taking that grammar seriously: [`Grammar`] is what a schema says,
//! [`catalog`] decides *which* schema a document is written against, and [`intel`] answers the
//! editor's questions from the two.
//!
//! ## Three crates, on purpose
//!
//! [`bennu-dtd`] and [`bennu-xsd`] each parse their own format and know nothing of the other or
//! of this crate. The **unified model and the adapters live here**, which is what keeps them from
//! having to agree on anything: a DTD parser stays a DTD parser, and "what does an editor need
//! from a schema" is a question asked in one place.
//!
//! DTD came first, and not for sentimental reasons — it is the format the files in front of this
//! user actually declare, its grammar is a quarter the size, and getting the shared model right
//! against the simpler language meant XSD dropped into a shape that already existed rather than
//! defining it.
//!
//! ## Nothing is answered without a grammar
//!
//! The standing rule (docs §7) is sharper here than anywhere else in bennu, because XML *looks*
//! checkable even when nothing is known about it. So:
//!
//! - no schema resolved → **no completion, no ghost text, no diagnostics**. Not a guess from the
//!   tags already in the file, which would confidently propose whatever typo is already there;
//! - a schema that declares the element **open** (`ANY`, `xs:any`, `mixed`) silences every check
//!   under it;
//! - an unresolvable `xs:import` silences the namespaces it would have brought, rather than
//!   reporting everything in them as unknown.
//!
//! The scanner is deliberately its own ([`scan`]) rather than a document parser: the buffer is
//! being typed into, so it is malformed most of the time, and a parser that returns nothing for
//! `<dependen` is a parser that is absent exactly when the user wanted help.
//!
//! [`bennu-dtd`]: https://docs.rs/bennu-dtd
//! [`bennu-xsd`]: https://docs.rs/bennu-xsd
//! [`Grammar`]: crate::grammar::Grammar
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_xml::prelude::...`.

// A curated grammar for the one file every Java project has and no jar ships a schema for.
pub mod builtin;
// Where the caret is, in XML's terms.
pub mod caret;
// Which schema a document is written against, and the sources one can be found in.
pub mod catalog;
pub mod ext;
// One model behind both DTD and XSD, plus the two adapters.
pub mod grammar;
// The editor's answers.
pub mod intel;
pub mod prelude;
// The tolerant scanner over a buffer being typed.
pub mod scan;
