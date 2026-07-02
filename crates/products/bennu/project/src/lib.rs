//! `bennu-project` — the Bennu project / workspace model.
//!
//! **Leaf crate** (docs §10): it owns the **capability-detection** (Spike D ruleset)
//! and the project model (Maven pom parse, module list, encoding detection, per-
//! project JDK detection, file tree). The analyzer crates depend on *this* crate,
//! never the reverse — capability detection gates which index sources even get built
//! (docs §10 C-note), so it must sit at the bottom.
//!
//! Depends only on the shared contract (`bennu-proto`) + serde: no analyzer, no
//! backend runtime. The pom parsing here is deliberately lightweight (targeted tag
//! extraction, no XML crate — the approved dep list has none): Phase-0 capability
//! detection needs dependency coordinates, `<properties>`, `<modules>` and the
//! compiler settings, not a full DOM. A real XML model of the config graph is
//! `bennu-web`'s job in a later phase.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_project::prelude::...`. The submodules stay `pub` for rustdoc navigation,
//! but the prelude is the canonical call-site path.

pub mod capability;
pub mod encoding;
pub mod error;
pub mod jdk;
pub mod model;
pub mod pom;
pub mod prelude;
pub mod tree;
