//! `arbor-plugin-wasm` — the host for extensions Arbor calls **into**.
//!
//! ## Extensions are not plugins
//!
//! A Lua plugin *calls* Arbor's API: it reacts to a hook, builds a form, asks for a file. An
//! extension is the other direction — it **implements an interface Arbor defines** and gets
//! called when the host needs an answer: parse this text, list this bucket. That inversion is
//! why the two share nothing but a manifest, and why this crate exists beside
//! `arbor-plugin-core` rather than inside it: a runtime measured in megabytes has no business
//! in front of every plugin that will never need one.
//!
//! The interfaces live in the workspace's `wit/` directory and are the public contract. See
//! `wit/README.md` for why they are WIT files and not a Rust trait behind a proc-macro.
//!
//! ## What is here, and what is behind a feature
//!
//! | module | what it decides | needs wasmtime |
//! |---|---|---|
//! | [`registry`] | which package implements which interface, and what is broken | no |
//! | [`caps`] | what one guest may reach | no |
//! | [`guest`] | that the gate runs **before** the effect | no |
//! | [`services`] | how the embedder performs an effect | no |
//! | [`engine`] | loading and instantiating a component | **yes** (`runtime`) |
//!
//! The split is the point. Arbor's rules about extensions — who provides what, who may reach
//! what, in which order — are ordinary Rust with ordinary tests, and none of them waits on a
//! runtime being wired up.
//!
//! ## Two invariants
//!
//! **A guest has no ambient capability.** No WASI filesystem, no WASI sockets, no
//! `wasi:http`. Everything it can do arrives as a host function, and every host function
//! starts by asking [`caps::GuestCaps`]. A guest that could open a socket would hold its own
//! credential, and "only the slots you declared" would stop being enforceable.
//!
//! **A guest's identity is not a parameter.** [`caps::GuestCaps`] carries the package name it
//! was built for, so there is no host function that *could* be written to resolve a
//! credential for a different package — the name is not something a guest can pass in.

pub mod caps;
#[cfg(feature = "runtime")]
pub mod dispatch;
#[cfg(feature = "runtime")]
pub mod dynamic;
#[cfg(feature = "runtime")]
pub mod engine;
pub mod guest;
pub mod prelude;
pub mod registry;
pub mod report;
pub mod services;
