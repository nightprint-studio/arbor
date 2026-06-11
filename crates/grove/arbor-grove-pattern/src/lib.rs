//! # arbor-grove-pattern
//!
//! The pure, deterministic **pattern algebra** at the heart of grove (Arbor's
//! live-coding music engine). A pattern is a function from a time window to the
//! events in it:
//!
//! ```text
//! Pattern<T> = (TimeSpan) -> [Hap<T>]
//! ```
//!
//! Design pillars (see `design/grove/semantics.md`):
//!
//! - **Exact rational time** ([`time::Time`]) — no float drift, so haps land on
//!   cycle boundaries and loops are bit-identical.
//! - **Absolute timeline** — patterns are queried at the true cycle `N`; looping
//!   is a transform, not baked in.
//! - **Source spans from day one** ([`span::SourceSpan`] on every [`hap::Hap`])
//!   for live editor highlight.
//! - **Per-cycle seeded RNG** ([`rng`]) — same cycle, same result, every loop.
//! - **Totality / purity** — a query always terminates and depends only on time.
//!
//! Zero external dependencies (std only): the time type, RNG and the
//! pitch/scale model are all hand-rolled. The crate compiles and tests
//! instantly and is trivially splittable.
//!
//! ## Entry point
//!
//! Reach the public API through [`prelude`] (workspace convention):
//!
//! ```
//! use arbor_grove_pattern::prelude::*;
//!
//! // "a b c" in one cycle, reversed → c, b, a
//! let p = fastcat(vec![pure("a"), pure("b"), pure("c")]).rev();
//! let mut haps = p.query(TimeSpan::cycle(0));
//! // Query results aren't time-ordered (Tidal-style); sort by onset to read them.
//! haps.sort_by_key(|h| h.part.begin);
//! assert_eq!(haps.iter().map(|h| h.value).collect::<Vec<_>>(), vec!["c", "b", "a"]);
//! ```

pub mod combinators;
pub mod control;
pub mod error;
pub mod hap;
pub mod pattern;
pub mod pitch;
pub mod prelude;
pub mod rng;
pub mod span;
pub mod tempo;
pub mod time;
