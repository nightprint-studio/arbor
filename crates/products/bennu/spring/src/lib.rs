//! `bennu-spring` — Spring support for Bennu, as a framework extension.
//!
//! The first implementation of the [`bennu-ext`] seam, and the shape every later one
//! should copy: it knows nothing about Bennu's Java engine, brings its own parser, owns
//! its own model, and answers editor questions through a trait that a WASM module could
//! implement instead.
//!
//! ## What it knows
//!
//! - **Beans** — `@Service`/`@Component`/…, `@Bean` factory methods, and `<bean>` XML with
//!   its `parent=` chain, in one registry ([`beans`], [`model`]).
//! - **Injection points** — annotated fields, constructors (including the implicit single
//!   one and the Lombok-generated one), setters ([`beans`]).
//! - **Endpoints** — `@RequestMapping` and its shorthands, class path joined with method
//!   path ([`endpoints`]).
//! - **Configuration** — `application*.properties` / `.yml` flattened to one dotted key
//!   space, with the user's pick of which file resolves first ([`props`]).
//! - **The expressions inside annotations** — `${…}` and `#{…}`, via [`bennu-spel`]
//!   ([`highlight`]).
//!
//! ## What it does with it
//!
//! [`java_intel`] and [`xml_intel`] turn that into the editor's answers: colouring,
//! diagnostics, go-to, hover, completion and gutter marks. [`ext`] routes each question by
//! file kind and owns the model.
//!
//! ## The rule that shapes every check
//!
//! Under-report rather than risk a false positive (docs §7). Every diagnostic in this
//! crate is gated on a condition that makes silence the default: a property set that is
//! *known* to be complete, a package the project *actually* declares, a bean id that
//! *looks like a typo* of a real one, a property namespace the configuration *already*
//! uses. A framework tool that cries wolf is turned off, and then it protects nothing.
//!
//! [`bennu-ext`]: https://docs.rs/bennu-ext
//! [`bennu-spel`]: https://docs.rs/bennu-spel
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_spring::prelude::...`.

pub mod beans;
// The curated stand-in used until the jars' own metadata is available.
pub mod builtin_meta;
// `@ConfigurationProperties` → the full key each bound field binds (nesting, maps, lists, renames).
pub mod config_props;
pub mod endpoints;
// A configuration key → the environment variable that overrides it.
pub mod env;
pub mod ext;
pub mod highlight;
pub mod java_intel;
// Resolving an annotation's ORIGIN through the file's imports, the way the compiler does.
// `@Service` is not a reserved word: without this, a project's own annotation of the same
// name would register beans that do not exist.
pub mod known;
pub mod library_beans;
// What Spring says its own properties are — `spring-configuration-metadata.json` out of the
// dependency jars, which is where the types, defaults and prose in a hover come from.
pub mod metadata;
pub mod model;
pub mod prelude;
pub mod props;
// The editor's answers for a property file — who reads each key.
pub mod props_intel;
pub mod scan;
// The reverse index: which Java / XML sites read a configuration key.
pub mod usages;
pub mod xml;
pub mod xml_intel;
