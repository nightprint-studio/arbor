//! Canonical entry point for `bennu-query`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_query::prelude::...`.

// Member-access completion (the SEAM's `completion(pos)`).
pub use crate::completion::completion;

// The resolver + the `Send + Sync` JDK member-index wrapper it composes.
pub use crate::jdk::JdkMemberIndex;
pub use crate::resolver::{convert_members, IndexResolver};

// The project source file (path + text) whole-project queries take.
pub use crate::source::PlanFile;

// Inherited ("super") members of a type — the Structure panel's lazy "Inherited" bucket. Reuses the
// resolver's supertype walk (superclass + interfaces), one level up from the type's own members.
pub use crate::inherited::{inherited_members, InheritedMember, InheritedSource};
