//! The types a **function slot** receives — offered first when a type is being named there.
//!
//! `identity_resolver.resolve_identity().map(Re|)` is about to name a type, and the call already
//! says which: `Optional<ResolvedIdentity>.map` takes a `Function<? super ResolvedIdentity, …>`, so
//! what is written there is `ResolvedIdentity::something` or a lambda over one. The type-name index
//! knows nothing of that — it ranks every `Re…` on the classpath by how well the name matches and
//! how near its package is — and so the one right answer sat among `ReadableByteChannel`,
//! `Record`, `Reference`… or, with nothing typed, was not offered at all: type names need a letter.
//!
//! This answers the slot's question on its own, and leaves ordering against everything else to the
//! caller, which puts these at the top.

use std::collections::HashSet;

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_complete::prelude::{MatchCase, Typed};
use bennu_java::prelude::functional_descriptor_at;
use bennu_proto::prelude::CompletionItem;

use crate::completion::split_prefix;
use crate::resolver::IndexResolver;

/// The classes the functional interface at `caret` receives, as type-name completions — the first
/// one preselected, each carrying its import.
///
/// Empty unless all of these hold, which is what keeps it from ever competing with a better answer:
///
/// * the caret is on a word, or on nothing, **directly in** an argument (or a declared variable's
///   initializer, or a `return`) whose target is a functional interface;
/// * the word is empty or **capitalised** — `map(re|` is far more likely a lambda parameter being
///   named, or a local being passed, than a type;
/// * the word is not after a `.` or a `::`, where a member is being written instead.
///
/// With nothing typed it answers too, and that is the point: Ctrl+Space in `map(|)` should open on
/// the type the function takes, and the type-name index refuses an empty prefix.
///
/// Only classes: a primitive, an array and `Object` (what an unbound variable reads as) are not
/// names anyone reaches for here.
pub fn functional_argument_types<M: CpMemberIndex>(
    source: &str,
    caret: usize,
    resolver: &IndexResolver<M>,
    case: MatchCase,
) -> Vec<CompletionItem> {
    let mut caret = caret.min(source.len());
    while caret > 0 && !source.is_char_boundary(caret) {
        caret -= 1;
    }
    let (word_start, prefix) = split_prefix(source, caret);
    if prefix.starts_with(|c: char| !c.is_uppercase()) {
        return Vec::new();
    }
    let head = source[..word_start].trim_end();
    if head.ends_with('.') || head.ends_with("::") {
        return Vec::new();
    }
    let Some(descriptor) = functional_descriptor_at(source, caret, resolver) else {
        return Vec::new();
    };
    let typed = Typed::new(&prefix, case);
    let mut seen = HashSet::new();
    let mut out: Vec<CompletionItem> = Vec::new();
    for param in descriptor.params {
        // A class reaches here as a slashed binary name; a primitive and a bare type variable do not.
        if param.dims > 0 || param.binary_name == "java/lang/Object" || !param.binary_name.contains('/') {
            continue;
        }
        let simple = param.binary_name.rsplit(['/', '$']).next().unwrap_or(&param.binary_name).to_string();
        if !typed.matches(&simple) || !seen.insert(param.binary_name.clone()) {
            continue;
        }
        let fqn = param.binary_name.replace(['/', '$'], ".");
        let package = fqn.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
        out.push(CompletionItem {
            label: simple,
            kind: "class".to_string(),
            detail: Some(fqn.clone()),
            // `java.lang` needs no import; the same-package and already-imported cases are decided
            // when the item is accepted, exactly as for every other type-name completion.
            auto_import: (package != "java.lang").then(|| fqn.clone()),
            preselect: out.is_empty(),
            owner: Some(param.binary_name.clone()),
            ..Default::default()
        });
    }
    out
}
