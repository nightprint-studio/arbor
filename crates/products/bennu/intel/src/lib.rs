//! `bennu-intel` — the code-intel provider seam (**Phase-0 skeleton**).
//!
//! The heart of the abstraction (docs §2): the FE speaks **one** protocol for every
//! language via [`IntelProvider`]; Java goes to the native, index-backed engine
//! (rich, custom, fast, no JSON-RPC), and Rust *will* go to rust-analyzer via an
//! LSP-client — the "predisposed LSP" the design requires (docs §2). This crate owns
//! the trait + the two impl slots:
//!
//! - [`NativeJavaProvider`] — the MVP impl, index-backed. **Stub** in Phase 0: every
//!   method returns the empty / unimplemented answer, so the be layer can wire the
//!   seam before the index queries land.
//! - [`LspClientProvider`] — the **predisposed** rust-analyzer slot. Documented and
//!   present, **not** implemented in the MVP (docs §2/§4: tower-lsp deferred). Its
//!   methods return [`IntelError::Unimplemented`] so the seam is complete and the
//!   later LSP wiring is a fill-in, not a new shape.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_intel::prelude::...`.

pub mod config;
pub mod java_index;
// Internal: Lombok generated-member synthesis, an implementation detail of index-building
// (java_index). Not part of the public surface, so it stays a private module (no prelude entry).
mod lombok;
pub mod prelude;
pub mod provider;
pub mod refcache;
pub mod refs;
pub mod rename;
pub mod spell;
// Spring stereotype-bean policy: reads bennu-java's generic annotations and derives the
// annotation-declared beans (`@Service`/`@Component`/…) the config resolver consults as a
// fallback in the C1 chain. Mirrors `lombok` (annotation policy on the java model), but public
// (the be build calls its project-scan collector).
pub mod spring_beans;
pub mod typemap;
