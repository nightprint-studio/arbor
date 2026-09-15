//! `bennu-cargo` — the Cargo side of a Rust project, read off its manifests.
//!
//! Bennu already *edits* a Rust project (tree, go-to, find-in-files) and rust-analyzer answers
//! everything about the code. What neither of them answers is the question a `Cargo.toml` is:
//! which crates exist, what each one builds, which features it has, what it depends on, and
//! whether the file in front of you is even valid. That is this crate.
//!
//! ## One schema, two consumers
//!
//! The centre of gravity is [`schema`]: the manifest's tables and keys, each with its value kind
//! and a line of prose. Validation reads it to answer *"is this key real"* and completion reads
//! it to answer *"what can I type here"* — deliberately the same table, because those two
//! answers must never disagree. A key that completes and then flags itself as unknown is worse
//! than having neither feature.
//!
//! ## Nothing is executed, and nothing here opens a socket
//!
//! No `cargo metadata`, no build, and no HTTP. The one question that cannot be answered from the
//! machine — "is there a newer version of this crate" — lives in [`registry`], which builds the URL,
//! parses the body and owns the on-disk cache while leaving the **request itself to the caller**. So
//! this crate stays testable and runnable offline, and the HTTP client stays where the runtime is.
//!
//! `cargo metadata` on a cold workspace is seconds and
//! wants the network for a manifest it has not seen; a tool window that costs that much to open is
//! one nobody opens twice. So the graph is read from the manifests themselves and from
//! `Cargo.lock` when it is there — which is also why a workspace that has never been built still
//! lists its crates correctly, and merely cannot say which versions were locked.
//!
//! The consequence is honest and worth stating: feature *unification*, target-specific dependency
//! resolution and the exact locked graph are Cargo's own answers, not ours. Everything here is a
//! reading of what is written down.
//!
//! ## Tolerance is a rule, not a quality
//!
//! Every entry point takes a manifest that may be mid-keystroke and must degrade to *less
//! information*, never to an error and never to a false positive. A `Cargo.toml` that does not
//! parse is still a project you can open and fix, and a squiggle under a key somebody is halfway
//! through typing is a bug.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_cargo::prelude::...`.

// A manifest read into tables and keys, every one of them carrying its span.
pub mod manifest;
// What a Cargo manifest may contain — the table that validation and completion share.
pub mod schema;
// The dependencies a manifest declares, as written. Read by four consumers, so read once.
pub mod deps;
// "Is this manifest right" — diagnostics over a live buffer.
pub mod validate;
// "What can I type here" — completion at a caret.
pub mod complete;
// The crate graph: members, targets, features. What the Rust tool window shows.
pub mod workspace;
// The cargo subcommands Bennu offers, and how one becomes an argv.
pub mod commands;
// Where cargo keeps things on this machine.
pub mod home;
// The crates.io index — the one thing here that needs the network, with the fetch left to the caller.
pub mod registry;
pub mod scaffold;
pub mod prelude;
