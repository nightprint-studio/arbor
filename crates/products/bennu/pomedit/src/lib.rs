//! `bennu-pomedit` — writing into a `pom.xml` the way a person would.
//!
//! ## The rule the whole crate follows
//!
//! **A pom keeps everything the change is not about.** Its comments, its element order, its
//! indentation, the licence header at the top, the blank line somebody put between two blocks: all
//! of it survives, because every write here is a byte-range edit against the text as it stands
//! rather than a model serialised back over it. The diff of a build file is the one artefact a
//! reviewer actually reads, and a whole-file diff is a diff nobody reads.
//!
//! ## What it is for
//!
//! Three writes, and they are the three a build file needs from an editor:
//!
//! - [`write::set_property`] — declare a property, creating `<properties>` if there is none;
//! - [`write::add_module`] — list a module, creating `<modules>` if there is none;
//! - [`surefire::convert_pinned_suite`] — the one that is a *fix* rather than a mechanism: turn a
//!   Surefire `<test>` pinned to a literal into a property reference defaulted to it, so a project
//!   whose every "run one test" button was silently inert becomes steerable without changing what
//!   a plain `mvn test` does.
//!
//! Plus [`scaffold`], which generates a new module's pom instead of editing one.
//!
//! ## What it deliberately is not
//!
//! It does not touch the filesystem, it does not run Maven, and it does not decide what a project
//! is allowed to write. It is given text and returns edits. That is what makes every rule in it
//! testable against a string, which is why the awkward shapes — Surefire configured per execution,
//! a `<properties>` that already uses the name we wanted, a module listed twice — are covered by
//! tests rather than by hoping.

// The mechanics: a byte-range replacement, and putting an element where it belongs.
pub mod edit;
pub mod prelude;
// Generating a module rather than editing one.
pub mod scaffold;
// Reading and rewriting the Surefire `<test>` selector.
pub mod surefire;
// The writes themselves.
pub mod write;
