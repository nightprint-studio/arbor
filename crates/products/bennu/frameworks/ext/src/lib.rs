//! `bennu-ext` — the framework-extension seam.
//!
//! Bennu's language core knows Java. It does not know Spring, and it should not: the
//! moment "is this bean ambiguous" lives next to "does this method exist", the two grow
//! into each other and neither can be replaced. So framework knowledge is a **plugin**:
//! a self-contained unit that is handed the project and answers the editor's questions
//! about it.
//!
//! Today a plugin is a crate the backend links ([`bennu-spring`] is the first). Tomorrow
//! it is a WASM module loaded at runtime. This crate is the boundary that makes those the
//! same thing to everyone on the calling side, which is why it is shaped the way it is:
//!
//! - **Object-safe, no associated types.** Every method takes and returns plain data, so
//!   the trait can be implemented by a struct *or* by a proxy that forwards to a WASM
//!   instance.
//! - **The extension owns its model.** [`FrameworkExtension::reindex`] hands over a
//!   project scan and the extension keeps whatever it builds from it, behind its own
//!   interior mutability. The host stores no framework state — nothing to keep in step,
//!   and a WASM extension keeps its model on its own side of the wall.
//! - **Contributions are wire types.** Everything crossing this boundary
//!   ([`ExtHighlight`], [`ExtTarget`], [`ExtGutterMark`], …) is serde-serializable and
//!   free of Java/Spring concepts, so the same values travel to the frontend unchanged.
//! - **Capability-gated.** [`ExtensionRegistry::new`] keeps only the extensions whose
//!   [`FrameworkExtension::applies`] says yes for this project. A Struts-free project
//!   never even asks the Struts extension a question — the same rule the UI follows when
//!   it hides a tool that could only ever be empty.
//!
//! ## What an extension may not assume
//!
//! It is queried on **any** file the editor has open, including ones it has nothing to do
//! with, and possibly **before** [`reindex`](FrameworkExtension::reindex) has ever run.
//! Every method therefore has an empty default, and returning nothing is always a correct
//! answer. Extensions are also queried from several threads at once (the backend
//! dispatches each request on its own thread), which is why the trait requires
//! `Send + Sync` and hands out `&self` rather than `&mut self`.
//!
//! [`bennu-spring`]: https://docs.rs/bennu-spring
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_ext::prelude::...`.

pub mod model;
pub mod prelude;
pub mod registry;
