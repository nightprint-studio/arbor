//! The name a declaration of a **written** type reads as — `List<Order>` is `orders`, `URLBuilder` a
//! `urlBuilder`, `Class<?>` a `clazz`.
//!
//! The word-level rules — decapitalising an acronym, English plurals, the keyword guard — are the
//! postfix templates' own (`crate::postfix::names`), so `.var` on a list and a field typed as one
//! propose the same name: there is one implementation of what a name is. What is here is only the
//! part a type *as written in the source* adds: type arguments, array brackets, a qualified name.

use crate::postfix::names::{decapitalize, plural, type_name_hint, usable};

/// Containers whose single type argument names what they hold: a `List<Order>` is `orders`.
const COLLECTIONS: &[&str] = &[
    "Collection", "Iterable", "List", "ArrayList", "LinkedList", "CopyOnWriteArrayList", "Vector",
    "Stack", "Set", "HashSet", "LinkedHashSet", "TreeSet", "SortedSet", "NavigableSet", "EnumSet",
    "Queue", "Deque", "ArrayDeque", "PriorityQueue", "BlockingQueue",
];

/// Wrappers that stand for the one value they hold: an `Optional<Order>` is an `order`.
const WRAPPERS: &[&str] = &["Optional"];

/// The name IntelliJ would propose for a variable declared with the type `written`, or `None` when
/// `written` is not a type (an unbalanced `List<Order`).
///
/// * arrays and varargs are the plural of their element: `Order[]` → `orders`, `int...` → `ints`;
/// * a collection is the plural of what it holds: `List<Order>` → `orders`;
/// * an `Optional` is what it holds: `Optional<Order>` → `order`;
/// * anything else is its simple name, generics dropped: `Map<K, V>` → `map`, `String` → `s`,
///   `Class<?>` → `clazz`.
pub fn suggested_name_for_type(written: &str) -> Option<String> {
    let written = Written::parse(written)?;
    if written.dims > 0 {
        return usable(plural(&decapitalize(written.simple)));
    }
    if let [only] = written.arguments().as_slice() {
        if COLLECTIONS.contains(&written.simple) {
            if let Some(name) = bound_of(only).and_then(Written::parse).and_then(|element| {
                usable(plural(&decapitalize(element.simple)))
            }) {
                return Some(name);
            }
        }
        if WRAPPERS.contains(&written.simple) {
            if let Some(name) = bound_of(only).and_then(suggested_name_for_type) {
                return Some(name);
            }
        }
    }
    Some(type_name_hint(&binary_of(written.base)))
}

/// A type as written, taken apart just far enough to name it.
struct Written<'a> {
    /// `java.util.List` in `java.util.List<Order>[]`.
    base: &'a str,
    /// `List`.
    simple: &'a str,
    /// What is between the outer `<` and `>`, or empty.
    arguments: &'a str,
    /// Array brackets and a varargs ellipsis, counted together — both make a plural.
    dims: usize,
}

impl<'a> Written<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let mut rest = text.trim();
        let mut dims = 0;
        loop {
            if let Some(shorter) = rest.strip_suffix("...") {
                rest = shorter.trim_end();
            } else if let Some(shorter) = rest.strip_suffix(']') {
                rest = shorter.trim_end().strip_suffix('[')?.trim_end();
            } else {
                break;
            }
            dims += 1;
        }
        let (base, arguments) = match rest.find('<') {
            Some(open) => (rest[..open].trim_end(), rest[open + 1..].strip_suffix('>')?),
            None => (rest, ""),
        };
        let simple = base.rsplit('.').next()?.trim();
        let is_name = simple.starts_with(|c: char| c.is_alphabetic() || c == '_' || c == '$');
        is_name.then_some(Written { base, simple, arguments, dims })
    }

    /// The top-level type arguments: `String, List<Order>` is two, not three.
    fn arguments(&self) -> Vec<&'a str> {
        let arguments = self.arguments;
        if arguments.trim().is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut depth = 0usize;
        let mut start = 0;
        for (i, byte) in arguments.bytes().enumerate() {
            match byte {
                b'<' => depth += 1,
                b'>' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => {
                    out.push(arguments[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
        out.push(arguments[start..].trim());
        out
    }
}

/// The type a type argument names: `Order` for `Order` and for `? extends Order`; `None` for a bare `?`,
/// which names nothing.
fn bound_of(argument: &str) -> Option<&str> {
    let Some(wildcard) = argument.strip_prefix('?') else {
        return Some(argument);
    };
    let wildcard = wildcard.trim_start();
    wildcard
        .strip_prefix("extends")
        .or_else(|| wildcard.strip_prefix("super"))
        .map(str::trim)
        .filter(|bound| !bound.is_empty())
}

/// What [`type_name_hint`] keys its special cases on. Only the three `java.lang` classes it knows by
/// name need their package spelled out, since a source file names them without one; everything else
/// is named by its last segment, which is all the hint reads.
fn binary_of(base: &str) -> String {
    match base {
        "String" | "Object" | "Class" => format!("java/lang/{base}"),
        _ => base.replace('.', "/"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(written: &str) -> Option<String> {
        suggested_name_for_type(written)
    }

    #[test]
    fn a_class_name_is_decapitalised_acronyms_included() {
        assert_eq!(name("MyMsRestClientApi").as_deref(), Some("myMsRestClientApi"));
        assert_eq!(name("URLBuilder").as_deref(), Some("urlBuilder"));
        assert_eq!(name("URL").as_deref(), Some("url"));
        assert_eq!(name("com.acme.IdentityResolver").as_deref(), Some("identityResolver"));
    }

    #[test]
    fn a_collection_is_the_plural_of_what_it_holds() {
        assert_eq!(name("List<Order>").as_deref(), Some("orders"));
        assert_eq!(name("java.util.Set<com.acme.Entry>").as_deref(), Some("entries"));
        assert_eq!(name("Collection<? extends Address>").as_deref(), Some("addresses"));
        assert_eq!(name("List<String>").as_deref(), Some("strings"));
        assert_eq!(name("Set<Map.Entry<String, Integer>>").as_deref(), Some("entries"));
    }

    /// `List<?>` holds nothing nameable, so it is named for what it is.
    #[test]
    fn a_collection_of_a_bare_wildcard_is_named_after_itself() {
        assert_eq!(name("List<?>").as_deref(), Some("list"));
        assert_eq!(name("List").as_deref(), Some("list"));
    }

    #[test]
    fn an_array_or_varargs_is_the_plural_of_its_element() {
        assert_eq!(name("Order[]").as_deref(), Some("orders"));
        assert_eq!(name("Order [ ] [ ]").as_deref(), Some("orders"));
        assert_eq!(name("int[]").as_deref(), Some("ints"));
        assert_eq!(name("String...").as_deref(), Some("strings"));
        assert_eq!(name("Class[]").as_deref(), Some("classes"));
    }

    #[test]
    fn a_map_is_a_map_whatever_it_maps() {
        assert_eq!(name("Map<String, List<Order>>").as_deref(), Some("map"));
    }

    #[test]
    fn an_optional_is_what_it_holds() {
        assert_eq!(name("Optional<Order>").as_deref(), Some("order"));
        assert_eq!(name("Optional<List<Order>>").as_deref(), Some("orders"));
        assert_eq!(name("Optional<?>").as_deref(), Some("optional"));
    }

    /// The type's own name would be a keyword, which cannot be declared.
    #[test]
    fn a_keyword_clash_is_avoided() {
        assert_eq!(name("Class<?>").as_deref(), Some("clazz"));
        assert_eq!(name("java.lang.Class").as_deref(), Some("clazz"));
    }

    /// The shared rules' short names for the types too common to spell out.
    #[test]
    fn primitives_and_string_get_the_conventional_short_names() {
        assert_eq!(name("int").as_deref(), Some("i"));
        assert_eq!(name("String").as_deref(), Some("s"));
        assert_eq!(name("Object").as_deref(), Some("o"));
    }

    #[test]
    fn something_that_is_not_a_type_has_no_name() {
        assert_eq!(name("List<Order"), None);
        assert_eq!(name("Order]"), None);
        assert_eq!(name(""), None);
        assert_eq!(name("<T>"), None);
    }
}
