//! `bennu-jpa` — JPA and Spring Data support for Bennu, as a framework extension.
//!
//! The second implementation of the [`bennu-ext`] seam, and the one that paid for extracting
//! [`bennu-facts`]: the Java scan and the annotation-origin rule are shared with
//! `bennu-spring`, everything below is this crate's own.
//!
//! ## What it knows
//!
//! - **Entities** — `@Entity` / `@Embeddable` / `@MappedSuperclass`, their table, id, columns
//!   and relations, with the inheritance chain folded in ([`index`], [`model`]).
//! - **Repositories** — the Spring Data interfaces, recognised by what they extend rather than
//!   by an annotation, because they usually carry none.
//! - **Queries in both languages** — the JPQL or SQL inside a `@Query`, tokenized so it stops
//!   being an opaque string ([`hql`]), and the **derived query names** that are queries in
//!   themselves ([`derived`]).
//!
//! ## Why the derived names are the point
//!
//! `findByCustomerNameAndTotalGreaterThan` is compiled by Spring Data at **application start**.
//! A typo in one is invisible to the compiler, invisible to every test that does not touch that
//! repository, and then it takes the whole context down on deploy. Resolving every segment
//! against the entity model — and refusing to when the model is incomplete — is the single most
//! valuable thing here.
//!
//! ## The rule that shapes every check
//!
//! Under-report rather than risk a false positive. An entity whose `@MappedSuperclass` chain
//! leaves the project, a relation whose target was never scanned, a repository over a type we do
//! not have: each of those turns the check **off** for that method rather than guessing. A
//! framework tool that cries wolf is turned off, and then it protects nothing.
//!
//! What is deliberately **not** checked: anything about the database. Whether the column exists,
//! whether the type matches, whether the native SQL parses — that needs a connection, which is
//! Picus's business. Claiming otherwise on a legacy schema nobody has migrated is how a tool
//! starts lying.
//!
//! [`bennu-ext`]: https://docs.rs/bennu-ext
//! [`bennu-facts`]: https://docs.rs/bennu-facts
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_jpa::prelude::...`.

// Derived query method names — parsing them, and resolving them against the entity.
pub mod derived;
pub mod ext;
// Repository / projection / query-method generation. Text only; nothing writes to disk.
pub mod generate;
// The query text inside a `@Query`, in both of its languages.
pub mod hql;
// Building the model out of the scan.
pub mod index;
// The editor's answers.
pub mod intel;
// JPA's annotation catalogue.
pub mod known;
pub mod model;
pub mod prelude;
// What a buffer is (entity / repository / neither) and the toolbar that follows from it.
pub mod roles;
// The scan (shared) plus JPA's own relevance markers.
pub mod scan;
