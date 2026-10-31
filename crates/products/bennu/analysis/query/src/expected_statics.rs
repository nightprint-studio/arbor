//! The **constants and static factories of the class a position wants**, offered without writing the class first.
//!
//! `Duration d = |` wants a `Duration`, and `Duration.ZERO` and `Duration.ofSeconds()`-style factories are the usual
//! way to get one; `Optional<Foo> x = |` wants `Optional.empty()`. IntelliJ's `JavaMembersGetter` does this for the
//! expected class; here it is the same idea for a class that is not an enum (enums have their own, ranked list in
//! [`crate::enum_arguments`]).
//!
//! Only members that return **exactly** the wanted class: a constant field of that type, or a static method of no
//! parameters returning it. Nothing a type needs an argument for is guessed.

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_complete::prelude::{MatchCase, Typed};
use bennu_java::prelude::{TypeResolver, Visibility};
use bennu_proto::prelude::CompletionItem;

use crate::completion::split_prefix;
use crate::enum_arguments::in_case_label;
use crate::hints::argument_slot_types;
use crate::resolver::IndexResolver;

/// How many rows at most: this is a hint list, not the class's whole static surface.
const MAX_ROWS: usize = 8;

/// Classes whose statics say nothing about a position (a `String` slot is not asking for `String.valueOf`).
const IGNORED: &[&str] = &[
    "java/lang/Object",
    "java/lang/String",
    "java/lang/CharSequence",
    "java/lang/Number",
    "java/lang/Integer",
    "java/lang/Long",
    "java/lang/Short",
    "java/lang/Byte",
    "java/lang/Double",
    "java/lang/Float",
    "java/lang/Boolean",
    "java/lang/Character",
    "java/lang/Class",
    "java/lang/Void",
];

/// `Type.CONSTANT` and `Type.factory()` rows for the class the argument slot or the declaration at `caret` wants.
/// Empty unless the type is one non-enum class, the caret is not after a `.`/`::` or in a `case` label, and the word
/// under the caret (when there is one) starts a member's name or the class's.
pub fn expected_type_statics<M: CpMemberIndex>(
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
    let head = source[..word_start].trim_end();
    if head.ends_with('.') || head.ends_with("::") || in_case_label(head) {
        return Vec::new();
    }
    let slots = argument_slot_types(source, caret, resolver as &dyn TypeResolver);
    let wanted = match slots.as_slice() {
        [one] => one.clone(),
        [] => match bennu_java::prelude::expected_type(source, caret, resolver as &dyn TypeResolver) {
            Some(t) => t,
            None => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    if wanted.dims > 0 || !wanted.binary_name.contains('/') || IGNORED.contains(&wanted.binary_name.as_str()) {
        return Vec::new();
    }
    let Some(members) = resolver.members_of(&wanted.binary_name) else { return Vec::new() };
    if members.flags.is_enum {
        return Vec::new();
    }
    let fqn = wanted.binary_name.replace(['/', '$'], ".");
    let simple = wanted.binary_name.rsplit(['/', '$']).next().unwrap_or(&wanted.binary_name).to_string();
    let package = fqn.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
    let typed = Typed::new(&prefix, case);
    let own = |m: &bennu_java::prelude::Member| {
        m.is_static && m.visibility == Visibility::Public && m.return_type.dims == 0 && m.return_type.binary_name == wanted.binary_name && !m.name.starts_with(['$', '<'])
    };
    // a typed word that only starts the CLASS's name is a type being written: the type-name list answers that
    let wanted_by_word = |name: &str| prefix.is_empty() || typed.matches(name);
    let row = |name: &str, call: bool| CompletionItem {
        label: if call { format!("{simple}.{name}()") } else { format!("{simple}.{name}") },
        kind: if call { "method" } else { "field" }.to_string(),
        detail: Some(fqn.clone()),
        auto_import: (package != "java.lang").then(|| fqn.clone()),
        owner: Some(wanted.binary_name.clone()),
        ..Default::default()
    };
    let constants = members.fields.iter().filter(|f| own(f) && wanted_by_word(&f.name)).map(|f| row(&f.name, false));
    let factories = members.methods.iter().filter(|m| own(m) && m.params.is_empty() && wanted_by_word(&m.name)).map(|m| row(&m.name, true));
    constants.chain(factories).take(MAX_ROWS).collect()
}
