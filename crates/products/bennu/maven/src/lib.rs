//! `bennu-maven` — the local repository as a source of answers, and the pom as a file that can be
//! checked against it.
//!
//! ## The problem this exists for
//!
//! A Java project's dependencies are the one part of it that is not in the project. They are
//! coordinates naming files in `~/.m2`, and when one of those files is not there the symptom is
//! never *"a dependency is missing"* — it is **every type from that jar reading as unresolvable**,
//! in source files that are perfectly correct, with the pom that could explain it saying nothing at
//! all. On a real project that was three thousand errors on a tree the compiler builds without a
//! warning.
//!
//! The answer to that has three parts, and they are the three halves of this crate:
//!
//! | | |
//! |---|---|
//! | [`repo`] · [`catalog`] | **where the repository is and what is in it** — resolved from `settings.xml` and `-Dmaven.repo.local` rather than assumed, and walked once into a list of coordinates |
//! | [`resolve`] · [`effective`] | **what the project resolves to against it**, with no Maven and no network: the parent chains, the imported BOMs, the transitive closure, the exclusions — so a project whose Maven is broken, slow or absent still gets a classpath, and one whose artifacts were never downloaded gets *their names* instead of a zero |
//! | [`check`] · [`complete`] · [`explain`] | **the pom as an edited file** — the coordinate that does not exist underlined where it is written, completion from the repository itself, and every "where does this version come from" answered with a jump |
//!
//! ## Nothing is executed, and nothing is downloaded
//!
//! No Maven process, no network, ever. That is what makes the answers cheap enough to give on a
//! keystroke, and it is also the limit: a coordinate that was never fetched stays missing, and this
//! crate's job is to say so precisely rather than to fix it. Fetching is a deliberate action, taken
//! by a caller that can tell the user it is happening.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_maven::prelude::...`.

// The coordinate-bearing blocks of a pom, found once for every answer.
pub mod blocks;
// What is in the local repository, as a searchable list.
pub mod catalog;
// What is wrong with a pom.
pub mod check;
// What can be typed at the caret.
pub mod complete;
// A pom addressed as elements.
pub mod doc;
// Inheritance, properties and managed versions, folded in.
pub mod effective;
// What an answer about a pom needs to know besides the pom.
pub mod env;
// The extension a host registers.
pub mod ext;
// Hover and go-to.
pub mod explain;
// The coordinates every Java project reaches for, for the machine that has none of them yet.
pub mod known;
pub mod prelude;
// Where the local repository is, and what a coordinate is called inside it.
pub mod repo;
// The project's classpath, without running Maven.
pub mod resolve;
