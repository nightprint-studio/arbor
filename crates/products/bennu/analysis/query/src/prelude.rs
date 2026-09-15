//! Canonical entry point for `bennu-query`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_query::prelude::...`.

// Member-access completion (the SEAM's `completion(pos)`).
pub use crate::completion::{completion, completion_in, TypeNameCatalog};

// Completion after `::`: the site reading, and the methods (and `new`) a reference can name there,
// the ones that fit the slot's functional interface first. `completion_in` already dispatches to it.
pub use crate::method_reference::{
    method_reference_completion, method_reference_site, MethodReferenceSite,
};

// The types a function slot receives — offered first when a type is named in `opt.map(Re|)`.
pub use crate::functional_types::functional_argument_types;

// The ranking's reading of a method reference's slot, and the fit test behind it — and how well a
// candidate answers the type the position wants, the key every completion list is ordered by first.
pub use crate::rank::{fits_reference, origin as member_origin, params_fit, Fit, ReferenceShape};

// Completion for a BARE identifier — what the lexical scope at the caret binds, the members of
// the enclosing type, and the static imports. The half of completion that is not after a dot.
pub use crate::scope_completion::scope_completion;

// The completion memory: what was accepted where, so the same reach is offered first next time.
pub use crate::picked::{record as record_pick, weight as pick_weight, ANNOTATION_CONTEXT};
// Where the identifier under a caret starts and what has been typed of it — the one reading of
// "the prefix", shared so the popup filters on the same token the query answered for.
pub use crate::completion::split_prefix as split_completion_prefix;

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
pub use crate::access::{package_of, protected_visible, same_package, same_top_level};

// What the editor draws around a call: the signature of the one the caret is inside, and the
// parameter names / inferred `var` types drawn between the code.
pub use crate::hints::{
    inlay_hints, inlay_hints_with, signature_at, InlayHint, LibraryParamNames, SignatureHelp,
};

// Turning a resolved member back into the text a person reads — shared so two features cannot
// disagree about what a method's parameters are called.
pub use crate::member_text::{
    named_parameters, parameters, render_param, render_signature, render_type,
    signature_param_names, simple_of, split_top_level,
};
