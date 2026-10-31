//! The expression that makes the value a position wants, drawn as ghost text: `Collections.addAll(tabellati, |` wants
//! a `Pair`, so `Pair.of(`; the second argument of that wants a `List`, so `List.of(`; and inside it a method of this
//! very file that returns a `Tabellato` (`tabellato(`) is the way the code writes one.
//!
//! In that order of preference: a method of this file that returns exactly the wanted class, then a static factory of the
//! class (`of`, `from`, `valueOf`, `create`, `newInstance`), then a constructor. Only when the wanted class is one
//! non-enum class the slot or declaration fixes, and only when the candidate is the only one of its rank: two helper
//! methods that both return the class are a choice, not a guess.

use bennu_java::prelude::{extract_symbols, TypeResolver, Visibility};

use crate::completion::split_prefix;
use crate::enum_arguments::in_case_label;

/// Classes whose creation says nothing about a position.
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

/// Factory names in the order a person would reach for them.
const FACTORIES: &[&str] = &["of", "from", "valueOf", "create", "newInstance", "copyOf"];

/// The creation expression up to its opening parenthesis (`Pair.of(`, `new Foo(`, `tabellato(`) for the class the
/// position at `caret` wants, or `None`. The word being typed is not removed: the caller strips it.
pub fn expected_creation(source: &str, caret: usize, resolver: &dyn TypeResolver) -> Option<String> {
    let mut caret = caret.min(source.len());
    while caret > 0 && !source.is_char_boundary(caret) {
        caret -= 1;
    }
    let (word_start, prefix) = split_prefix(source, caret);
    let head = source[..word_start].trim_end();
    if head.ends_with('.') || head.ends_with("::") || in_case_label(head) || !(head.ends_with(['(', ',', '=']) || head.ends_with("return")) {
        return None;
    }
    // a call still open at the caret does not parse as one: it is closed for the analysis, unless the editor did that
    let closed;
    // whether the user's own text closes THIS call, as opposed to the call around it: only then is the number of
    // arguments written final
    let own_closer = crate::hints::closes_at_caret(source, caret);
    let text = if source[caret..].trim_start().starts_with(')') {
        source
    } else {
        closed = format!("{});{}", &source[..caret], &source[caret..]);
        &closed
    };
    let slots = crate::hints::argument_slot_types_with(text, caret, resolver, own_closer);
    let wanted = match slots.as_slice() {
        [one] if one.binary_name.contains('/') => one.clone(),
        // a parameter that is a type variable (`T`, `R`) is settled from the arguments already written and from what
        // the call around it wants
        [_] => crate::slot_inference::inferred_slot_type_with(text, caret, resolver, own_closer)?,
        [] => match bennu_java::prelude::expected_type(text, caret, resolver) {
            Some(t) => t,
            None => crate::slot_inference::inferred_slot_type_with(text, caret, resolver, own_closer)?,
        },
        _ => return None,
    };
    if wanted.dims > 0 || !wanted.binary_name.contains('/') || IGNORED.contains(&wanted.binary_name.as_str()) {
        return None;
    }
    let members = resolver.members_of(&wanted.binary_name)?;
    if members.flags.is_enum || members.flags.is_annotation {
        return None;
    }
    let simple = wanted.binary_name.rsplit(['/', '$']).next()?.to_string();
    let starts = |text: &str| text.starts_with(prefix.as_str());

    // a method of this file that returns the class: the way this code writes one
    let mut own: Vec<String> = extract_symbols(text)
        .types
        .iter()
        .flat_map(|t| t.methods.iter())
        .filter(|m| m.return_type_text.split('<').next().map(str::trim) == Some(simple.as_str()) && !m.params.is_empty() && m.name.starts_with(|c: char| c.is_ascii_lowercase()))
        .map(|m| m.name.clone())
        .collect();
    own.sort();
    own.dedup();
    if let [one] = own.as_slice() {
        let text = format!("{one}(");
        return starts(&text).then_some(text);
    }
    if own.len() > 1 {
        return None;
    }
    // a static factory of the class itself
    for name in FACTORIES {
        let has = members.methods.iter().any(|m| {
            m.is_static && m.visibility == Visibility::Public && m.name == *name && !m.params.is_empty() && m.return_type.dims == 0 && m.return_type.binary_name == wanted.binary_name
        });
        if has {
            let text = format!("{simple}.{name}(");
            return starts(&text).then_some(text);
        }
    }
    // a constructor that takes something
    if !members.flags.is_interface && !members.flags.is_abstract {
        let ctor = members.methods.iter().any(|m| m.name == "<init>" && m.visibility == Visibility::Public && !m.params.is_empty());
        if ctor {
            let text = format!("new {simple}(");
            return starts(&text).then_some(text);
        }
    }
    None
}
