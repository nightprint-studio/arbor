//! Canonical entry point for `bennu-query`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_query::prelude::...`.

// Member-access completion (the SEAM's `completion(pos)`).
pub use crate::completion::completion;

// The resolver + the `Send + Sync` JDK member-index wrapper it composes, plus the two-tier
// classpath index (JDK + optional per-project dependency tier) the completion/validation resolver
// runs over.
pub use crate::classpath_index::ClasspathIndex;
pub use crate::jdk::JdkMemberIndex;
pub use crate::resolver::{convert_members, IndexResolver, ProjectView};

// Per-file dependency recording for the incremental validation cache: a recording scope
// (`record`) captures every project type a validation reads (`RecordedDeps`), so the cache can
// tell when a cached diagnostic list is still valid. `fnv1a` is the shared members-JSON hash.
pub use crate::dep_record::{fnv1a, record, RecordedDeps};

// The project source file (path + text) whole-project queries take.
pub use crate::source::PlanFile;

// Inherited ("super") members of a type — the Structure panel's lazy "Inherited" bucket. Reuses the
// resolver's supertype walk (superclass + interfaces), one level up from the type's own members.
pub use crate::inherited::{inherited_members, InheritedMember, InheritedSource};

// Which methods the class under the caret can override — the "Implement / override methods"
// dialog's list, grouped by the supertype that declares them.
pub use crate::overridable::{by_declaring_type, overridable_at, Overridable};

// The accessibility rules the queries share (who can see what).
pub use crate::access::{package_of, same_package, same_top_level};

// What the editor draws around a call: the signature of the one the caret is inside, and the
// parameter names / inferred `var` types drawn between the code.
pub use crate::hints::{inlay_hints, signature_at, InlayHint, SignatureHelp};

// Turning a resolved member back into the text a person reads — shared so two features cannot
// disagree about what a method's parameters are called.
pub use crate::member_text::{
    named_parameters, parameters, render_param, render_signature, render_type,
    signature_param_names, simple_of, split_top_level,
};
