//! `bennu-query` — the Bennu code-intel **query engine**: the read-only resolver + member-access
//! completion that sits on top of the base crates.
//!
//! [`IndexResolver`](resolver::IndexResolver) is the [`bennu_java`] `TypeResolver` backed by the
//! persisted project index ([`bennu_index`]) with a JDK/library bytecode fall-through
//! ([`bennu_classpath`], wrapped `Send + Sync` in [`jdk::JdkMemberIndex`]). It is the layer the
//! editor's type-aware features resolve against — completion and inherited-members live here;
//! find-usages, rename and hover consume the same resolver from `bennu-intel`.
//!
//! This crate depends only on the four already-clean base crates (`bennu-index`, `bennu-classpath`,
//! `bennu-java`, `bennu-proto`) — the free-crate-split unit `bennu-intel` unified before.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_query::prelude::...`.

pub mod completion;
pub mod dep_record;
pub mod inherited;
pub mod jdk;
pub mod prelude;
pub mod resolver;
pub mod source;
