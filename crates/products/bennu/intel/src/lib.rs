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

pub mod completion;
pub mod java_index;
pub mod jdk;
pub mod prelude;
pub mod provider;
pub mod resolver;
pub mod typemap;
