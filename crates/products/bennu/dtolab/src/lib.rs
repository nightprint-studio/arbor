//! `bennu-dtolab` — a DTO, tried out.
//!
//! Three questions a class raises and no amount of reading it answers with certainty: *what JSON does
//! it read and write*, *which violations does a given value produce*, and *what would a test pinning
//! those down look like*. The DTO Lab answers all three, and this crate is its pure half:
//!
//! - [`cases`] turns each constraint into the values that violate it, and a value that satisfies
//!   every constraint of a field;
//! - [`generate`] assembles what a validation-test template is rendered with ([`context`]), checking
//!   every expectation on the JVM when one is available;
//! - [`protocol`] is the wire to the Java harness, whose source ships in this crate;
//! - [`values`] maps field names and constraints to the values a field is given.
//!
//! The class model, the template engine and the template store are `bennu-templates`': the lab's
//! tests are one kind of template among several.
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
pub mod context;
pub mod generate;
pub mod paths;
pub mod prelude;
pub mod protocol;
pub mod skeleton;
pub mod values;
