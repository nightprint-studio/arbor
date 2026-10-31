//! `list.toArray(|` wants `Item[]::new`: the collection says what the elements are, and the parameter is an
//! `IntFunction<T[]>` that can only be that. Offered whole, with the typed start taken off by the caller.
//!
//! Only the receivers whose element type is spelled out (`List<Item>`, `Stream<Item>`): a type variable or a raw type
//! says nothing, and nothing is guessed.

use bennu_java::prelude::{infer_expression_type, parse_java, TypeResolver};

use crate::hints::enclosing_call;

/// Receivers whose first type argument is the element type of the array `toArray` builds.
const ELEMENT_FIRST: &[&str] = &[
    "java/util/Collection",
    "java/util/List",
    "java/util/ArrayList",
    "java/util/LinkedList",
    "java/util/Set",
    "java/util/HashSet",
    "java/util/LinkedHashSet",
    "java/util/TreeSet",
    "java/util/SortedSet",
    "java/util/Queue",
    "java/util/Deque",
    "java/util/ArrayDeque",
    "java/util/Vector",
    "java/util/stream/Stream",
];

/// The array constructor reference the caret's argument should be (`Item[]::new`), when the caret is in the first
/// argument of a `toArray(` on a receiver whose element type is known.
pub fn array_constructor_argument(source: &str, offset: usize, resolver: &dyn TypeResolver) -> Option<String> {
    if !source.is_char_boundary(offset) {
        return None;
    }
    let repaired = format!("{}x);{}", &source[..offset], &source[offset..]);
    let tree = parse_java(&repaired)?;
    let call = enclosing_call(tree.root_node(), offset)?;
    if call.kind() != "method_invocation" || call.child_by_field_name("name")?.utf8_text(repaired.as_bytes()).ok()? != "toArray" {
        return None;
    }
    let args = call.child_by_field_name("arguments")?;
    if source.get(args.start_byte() + 1..offset)?.contains(',') {
        return None;
    }
    let object = call.child_by_field_name("object")?;
    let ty = infer_expression_type(&repaired, object.start_byte(), object.end_byte(), resolver)?;
    if !ELEMENT_FIRST.contains(&ty.binary_name.as_str()) {
        return None;
    }
    let element = ty.type_args.first()?;
    // a type variable or a wildcard has no slash: it is not a class the code can name
    if element.dims != 0 || !element.binary_name.contains('/') {
        return None;
    }
    let simple = element.binary_name.rsplit('/').next()?.replace('$', ".");
    Some(format!("{simple}[]::new"))
}
