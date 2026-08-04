//! `bennu-dtd` — a DTD parser, for the half of a Java codebase that still ships one.
//!
//! ## Why a DTD parser in 2026
//!
//! Because the files a legacy Java project opens every day declare one. `struts.xml`,
//! `struts-config.xml`, `web.xml` before Servlet 2.4, `hibernate.cfg.xml`, `*.hbm.xml`, every
//! Ant-era build fragment: all of them say `<!DOCTYPE … SYSTEM "…dtd">` and none of them say
//! `xsi:schemaLocation`. An editor that only understands XSD understands the `pom.xml` and
//! nothing else the user has open.
//!
//! It is also, bluntly, the cheap one: a DTD has three declaration forms and no namespaces, no
//! imports, no substitution groups and no type system. Roughly a quarter of the code an XSD
//! parser needs, for most of the value.
//!
//! ## What it parses
//!
//! - `<!ELEMENT name content>` — with the content model kept as a [`Particle`] tree rather than
//!   flattened, because `(a, (b | c)+, d?)` says things a set of names cannot;
//! - `<!ATTLIST element name type default …>` — every attribute, its enumeration when it has one,
//!   and whether it is `#REQUIRED`;
//! - `<!ENTITY % name "…">` — parameter entities, **expanded**, which is not optional: a real
//!   DTD is written almost entirely in them, and one that does not expand them parses an empty
//!   document.
//!
//! Every declaration carries the byte offset it starts at, so "go to the definition of this tag"
//! has somewhere to land.
//!
//! ## What it does not do
//!
//! Validate a document. This crate answers *what does the grammar say*; deciding whether a
//! particular file obeys it is [`bennu-xml`]'s job, and keeping the two apart is what lets the
//! same question be asked of an XSD.
//!
//! It also does not fetch anything. A DTD arrives here as text — read from the project, or out
//! of a jar entry — because a parser that opens sockets cannot run where this one has to.
//!
//! [`bennu-xml`]: https://docs.rs/bennu-xml
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_dtd::prelude::...`.

pub mod model;
pub mod parse;
pub mod prelude;

pub use model::*;
