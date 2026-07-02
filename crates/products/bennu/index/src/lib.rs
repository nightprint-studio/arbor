//! `bennu-index` — ★ the product (docs §1).
//!
//! A multi-source symbol / relation index over a lean, low-RAM **mmap'd store**:
//! an [`fst::Map`] from name → `u64` offset, plus a framed `rkyv` blob of archived
//! records read zero-copy on demand. Ported from the proven `bennu-spike-index`
//! (docs §10: 2M symbols = 251 MiB on disk, queried at ~15 MiB working set), with
//! the two production hardenings the spike surfaced:
//!
//! - **16-byte-aligned records** in the blob (docs §10) — rkyv's archived form has
//!   alignment requirements; a misaligned slice fails `bytecheck` loudly rather than
//!   risking UB. The writer pads every record to a 16-byte boundary and stores the
//!   padded offset in the fst.
//! - **Format-version header** on every file → a mismatch triggers a rebuild from
//!   sources (rkyv 0.8.x has no migration; rebuilding from jars/sources is cheap and
//!   is the safest evolution strategy — docs §3/§4).
//!
//! Every record carries a [`schema::Source`] tag (docs §3): adding a source (Maven
//! `.m2`, a new config kind) is a new variant feeding the same table, not a rewrite.
//!
//! **Leaf crate**: no Bennu dependencies. The query engine that composes these into
//! completion / refs / goto lives in the analyzers above it.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_index::prelude::...`. The submodules stay `pub` for rustdoc navigation,
//! but the prelude is the canonical call-site path.

pub mod builder;
pub mod prelude;
pub mod query;
pub mod schema;
pub mod store;
