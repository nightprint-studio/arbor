//! `bennu-dtolab` — a DTO, tried out.
//!
//! Three questions a class raises and no amount of reading it answers with certainty: *what JSON
//! does it read and write*, *which violations does a given value produce*, and *what would a test
//! pinning those down look like*. The DTO Lab answers all three, and this crate is its pure half:
//!
//! - [`model`] reads a class from **source** — fields, types, JSON names, accessors, constraints —
//!   instantly and with nothing compiled;
//! - [`cases`] turns each constraint into the values that violate it, and a value that satisfies
//!   every constraint of a field;
//! - [`generate`] assembles what a test template is rendered with, checking every expectation on
//!   the JVM when one is available;
//! - [`template`] renders a user-editable template, and [`placement`] decides where the result goes;
//! - [`protocol`] is the wire to the Java harness, whose source ships in this crate.
//!
//! ## Why the answers that matter come from the JVM
//!
//! A constraint written in source is a *claim* about what the validator will do. Whether it holds
//! depends on the provider, its version, the message bundle the project configured, a custom
//! `ConstraintValidator`, and how Jackson binds the payload in the first place — none of which is in
//! the file. So the lab is hybrid on purpose: the skeleton of a payload and the list of cases are
//! static and instant, and every *expected violation* is what the project's own validator produced,
//! run by [`protocol::HARNESS_SOURCE`] on the project's own JDK and classpath. When that is not
//! possible the static prediction is still given, and marked as such.

pub mod cases;
pub mod generate;
pub mod model;
pub mod names;
pub mod placement;
pub mod prelude;
pub mod protocol;
pub mod skeleton;
pub mod template;
