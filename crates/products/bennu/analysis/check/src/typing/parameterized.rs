//! Parameterized types are **invariant**: a `List<Integer>` is not a `List<Number>` (JLS §4.10.2),
//! and a wildcard argument contains only what its bound admits (§4.5.1) — `List<? extends Number>`
//! holds no `List<String>`, and `List<String>` holds no `List<?>`.
//!
//! The class walk in [`crate::typing::casts`] stops at the erasure, where all of those are fine. This
//! reads the arguments — and only where both sides WRITE them, because that is the only place they
//! are certain:
//!
//!   * the target's arguments are read from its text. The engine's [`TypeRef`] collapses `? extends B`
//!     and `? super B` onto `B` alike, which erases exactly the variance this rule is about;
//!   * the value must be a local or parameter DECLARED with arguments, or a `new C<…>()` that spells
//!     them. A diamond, or a call to a generic method (`Arrays.asList(1, 2)` into a `List<Number>`),
//!     takes its arguments from the target — javac infers them there, so what our inference answers
//!     for the value on its own would be a different, wrong question.
//!
//! Anything unreadable — a type variable, a nested wildcard, an annotated argument, a raw side —
//! leaves the pair unjudged.

use bennu_java::prelude::{
    is_inferred_type, same_binary_type, seen_as, written_type_ref, FileSymbols, InferCache, TypeRef, TypeResolver,
};
use tree_sitter::Node;

use crate::support::nodes::is_primitive;
use crate::support::walk::{hierarchy_fully_known, reaches};

const OBJECT: &str = "java/lang/Object";

/// A value refused, as the two types are written.
pub(crate) struct Refusal {
    pub(crate) found: String,
    pub(crate) expected: String,
}

/// What a judgement reads about the file.
struct Ctx<'a, 't> {
    root: &'a Node<'t>,
    source: &'a str,
    symbols: &'a FileSymbols,
    resolver: &'a dyn TypeResolver,
    cache: &'a InferCache,
}

impl Ctx<'_, '_> {
    fn type_ref(&self, text: &str) -> Option<TypeRef> {
        written_type_ref(self.root, self.source, self.symbols, text.trim(), self.resolver, self.cache)
    }
}

/// The definite refusal of `val` where `target_text` is declared, on type arguments alone.
pub(crate) fn parameterized_mismatch(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    target_text: &str,
    val: Node,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<Refusal> {
    let ctx = Ctx { root, source, symbols, resolver, cache };
    let target_text = target_text.trim();
    let arguments = top_level_arguments(target_text)?;
    let value_text = written_value_type(val, source.as_bytes())?;
    let value = ctx.type_ref(&value_text)?;
    let target = ctx.type_ref(target_text)?;
    if value.dims > 0 || target.dims > 0 || value.type_args.is_empty() {
        return None;
    }
    let seen = seen_as(resolver, &value, &target.binary_name)?;
    if seen.type_args.len() != arguments.len() {
        return None;
    }
    let refused = arguments.iter().zip(&seen.type_args).any(|(written, actual)| contains(written, actual, &ctx) == Some(false));
    refused.then(|| Refusal { found: value_text, expected: target_text.to_string() })
}

/// Whether the written argument `written` contains the actual argument `actual` — `None` when it
/// cannot be told.
fn contains(written: &str, actual: &TypeRef, ctx: &Ctx) -> Option<bool> {
    let resolver = ctx.resolver;
    let Some(wildcard) = written.strip_prefix('?') else {
        let expected = ctx.type_ref(written)?;
        if !concrete(&expected, resolver) {
            return None;
        }
        if actual.wildcard {
            return Some(false); // a capture is never a type anyone can name
        }
        return concrete(actual, resolver).then(|| same_type(&expected, actual));
    };
    let wildcard = wildcard.trim();
    if wildcard.is_empty() {
        return Some(true);
    }
    let (upper, bound_text) = match (wildcard.strip_prefix("extends"), wildcard.strip_prefix("super")) {
        (Some(bound), _) => (true, bound),
        (_, Some(bound)) => (false, bound),
        _ => return None,
    };
    let bound = ctx.type_ref(bound_text)?;
    let flat = |t: &TypeRef| t.dims == 0 && t.type_args.is_empty() && concrete(t, resolver);
    if !flat(&bound) || !flat(actual) {
        return None;
    }
    let (sub, sup) = if upper { (actual, &bound) } else { (&bound, actual) };
    if same_binary_type(&sub.binary_name, &sup.binary_name) || sup.binary_name == OBJECT {
        return Some(true);
    }
    hierarchy_fully_known(resolver, &sub.binary_name).then(|| reaches(resolver, &sub.binary_name, &sup.binary_name))
}

/// The written type of a value that spells its arguments: a local or parameter declared with them,
/// or a `new C<…>()` — never a diamond, a call, or a field (see the module doc).
fn written_value_type(val: Node, bytes: &[u8]) -> Option<String> {
    match val.kind() {
        "identifier" => {
            let name = val.utf8_text(bytes).ok()?;
            let text = crate::support::scopes::declared_type_text(val, name, bytes, false)?;
            (!is_inferred_type(&text) && top_level_arguments(&text).is_some()).then_some(text)
        }
        "object_creation_expression" if !crate::support::nodes::is_qualified_creation(val) => {
            let ty = val.child_by_field_name("type").filter(|t| t.kind() == "generic_type")?;
            let text = ty.utf8_text(bytes).ok()?.to_string();
            top_level_arguments(&text).is_some().then_some(text)
        }
        "parenthesized_expression" => written_value_type(val.named_child(0)?, bytes),
        _ => None,
    }
}

/// The arguments of a written `C<A, B<X>>`, split at the top level. `None` for a diamond, for no
/// `<…>`, and for anything whose arguments do not close the text (`Outer<A>.Inner`, `List<A>[]`).
fn top_level_arguments(text: &str) -> Option<Vec<String>> {
    let open = text.find('<')?;
    let inner = text.strip_suffix('>')?.get(open + 1..)?;
    let mut args = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (i, ch) in inner.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            ',' if depth == 0 => {
                args.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim().to_string());
    (depth == 0 && args.iter().all(|a| !a.is_empty() && !a.starts_with('@'))).then_some(args)
}

/// A type every argument can be compared on: a readable class (or a primitive array), never a type
/// variable or a capture, at any depth.
fn concrete(t: &TypeRef, resolver: &dyn TypeResolver) -> bool {
    let name = t.binary_name.as_str();
    let readable = (is_primitive(name) && t.dims > 0) || (name.contains('/') && resolver.members_of(name).is_some());
    !t.names_a_wildcard() && readable && t.type_args.iter().all(|a| concrete(a, resolver))
}

fn same_type(a: &TypeRef, b: &TypeRef) -> bool {
    a.dims == b.dims
        && same_binary_type(&a.binary_name, &b.binary_name)
        && a.type_args.len() == b.type_args.len()
        && a.type_args.iter().zip(&b.type_args).all(|(x, y)| same_type(x, y))
}

#[cfg(test)]
mod tests {
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, TypeRef, TypeResolver};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct Resolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    impl TypeResolver for Resolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn class(superclass: Option<&str>, interfaces: Vec<TypeRef>, type_params: &[&str], interface: bool) -> ClassMembers {
        ClassMembers {
            type_params: type_params.iter().map(|p| p.to_string()).collect(),
            superclass: superclass.map(TypeRef::simple),
            interfaces,
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags { is_interface: interface, ..ClassFlags::default() },
        }
    }

    fn of(binary: &str, argument: &str) -> TypeRef {
        TypeRef { type_args: vec![TypeRef::simple(argument)], ..TypeRef::simple(binary) }
    }

    /// `Integer extends Number`; `ArrayList<E> implements List<E> extends Collection<E>`.
    fn resolver() -> Resolver {
        let object = "java/lang/Object";
        let mut members = HashMap::new();
        members.insert(object.to_string(), class(None, vec![], &[], false));
        members.insert("java/lang/String".to_string(), class(Some(object), vec![], &[], false));
        members.insert("java/lang/Number".to_string(), class(Some(object), vec![], &[], false));
        members.insert("java/lang/Integer".to_string(), class(Some("java/lang/Number"), vec![], &[], false));
        members.insert("java/util/Collection".to_string(), class(None, vec![], &["E"], true));
        members.insert("java/util/List".to_string(), class(None, vec![of("java/util/Collection", "E")], &["E"], true));
        members.insert("java/util/ArrayList".to_string(), class(Some(object), vec![of("java/util/List", "E")], &["E"], false));
        let simple = ["Object", "String", "Number", "Integer"]
            .iter()
            .map(|s| (s.to_string(), format!("java/lang/{s}")))
            .chain(["Collection", "List", "ArrayList"].iter().map(|s| (s.to_string(), format!("java/util/{s}"))))
            .collect();
        Resolver { members, simple }
    }

    fn diags(src: &str) -> Vec<String> {
        crate::typing::casts::type_compat_errors(src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn a_list_of_a_subtype_is_not_a_list_of_its_supertype() {
        let d = diags("class C { void m(List<Integer> ints) { List<Number> numbers = ints; } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`List<Integer>` cannot be assigned to `List<Number>`"), "{d:?}");
    }

    #[test]
    fn a_creation_spelling_other_arguments_is_flagged_through_the_supertype() {
        let d = diags("class C { void m() { Collection<Object> objects = new ArrayList<String>(); } }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn a_wildcard_holds_only_what_its_bound_admits() {
        assert_eq!(diags("class C { void m(List<String> s) { List<? extends Number> n = s; } }").len(), 1);
        assert_eq!(diags("class C { void m(List<?> any) { List<String> s = any; } }").len(), 1);
        assert_eq!(diags("class C { List<Object> m(List<String> s) { return s; } }").len(), 1);
    }

    #[test]
    fn wildcards_diamonds_raw_types_and_same_arguments_are_ok() {
        let src = "class C { void m(List<String> s, List<Number> nums) { \
                   List<? extends Object> a = s; List<?> b = s; List<? super Integer> c = nums; \
                   List<Number> d = new ArrayList<>(); Collection<String> e = new ArrayList<String>(); List f = s; } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }
}
