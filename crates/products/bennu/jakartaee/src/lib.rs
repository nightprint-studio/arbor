//! `bennu-jakartaee` — the container that wires CDI and EJB beans, and the servlets it deploys.
//!
//! ## Why a Jakarta EE application is hard to read
//!
//! `@Inject OrderService orders;` says what it wants and nothing about what it gets. The answer is
//! decided by the container at deployment — every bean in every archive, their types, their
//! qualifiers, which alternatives are enabled — and when there is no answer, or two, the
//! application does not start. The error names the injection point; finding the beans means a grep
//! for every class that might implement the type.
//!
//! This crate reads the same things the container reads, from the source: the bean-defining
//! annotations, the producers, the qualifiers, the archive's discovery mode. It puts the answer in
//! the gutter and in a panel, and reports the few failures the source is enough to be sure of.
//!
//! ## Silence is the default
//!
//! A CDI container gains beans from places a source tree does not show: portable extensions,
//! library bean archives, producers returning library types, alternatives enabled in a deployment
//! descriptor. Every check here therefore starts from a list of reasons to say nothing, and speaks
//! only when none of them applies. A missed report costs a deployment round-trip; a false one
//! teaches the reader to ignore the squiggle.
//!
//! ## Public API: use the [`prelude`]

// The bean discovery mode, read from `beans.xml`.
pub mod archive;
// Beans: classes and producers, with their qualifiers and flags.
pub mod beans;
// The `"beans"` and `"endpoints"` catalogue rows.
pub mod catalog;
// Every diagnostic this extension raises.
pub mod checks;
// The extension itself — what a host registers.
pub mod ext;
// Injection points: `@Inject` fields, constructors and initializers, `@EJB` fields.
pub mod inject;
// Gutter, hover, go-to and completion for a Java buffer.
pub mod intel;
// Which annotations are the platform's, and from which packages.
pub mod known;
// Matching an injection point against the beans that could satisfy it.
pub mod matching;
// The project model, and how it is built from a scan.
pub mod model;
pub mod prelude;
// Which annotations are qualifiers, stereotypes, or neither — and which cannot be told.
pub mod qualifiers;
// Offsets, lines, module roots, erasure — the text mechanics every module needs.
pub mod text;
// The project's type table: names resolved through imports, hierarchies walked.
pub mod types;
// Servlets and filters, from annotations and from `web.xml`.
pub mod web;

#[cfg(test)]
mod fixtures;
