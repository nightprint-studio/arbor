//! The erased shape of a declared method, read off the tree.
//!
//! Every check that asks "does this method override that one" needs the same answer, and each one
//! used to carry its own copy. The copies drifted: one resolved a parameter type with
//! [`type_binary`](crate::resolve::type_binary) and the other with
//! [`type_binary_at`](crate::resolve::type_binary_at), so a nested parameter type resolved against
//! its owner in one check and against the whole file in the other — the same signature matching in
//! one place and not in the next. One copy, and it is the owner-aware one.
//!
//! The other half of that question — the supertypes of the class declaring the method — lives in
//! [`crate::supertypes`], which had five copies of its own.

use bennu_java::prelude::{FileSymbols, Member, TypeRef, TypeResolver};
use tree_sitter::Node;

use crate::resolve::type_binary_at;

/// The erased binary names of a method's parameter types, in order.
///
/// `None` — skip this method — when a parameter type does not resolve, or when the method is
/// varargs. Erased-array matching against a `spread_parameter` is finicky enough that guessing it
/// wrong would report an override that isn't one, and a missed varargs override costs nothing.
pub(crate) fn method_param_binaries(
    md: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<Vec<String>> {
    let params_node = md.child_by_field_name("parameters")?;
    let mut out = Vec::new();
    let mut c = params_node.walk();
    for p in params_node.named_children(&mut c) {
        match p.kind() {
            "formal_parameter" => {
                let ty = p.child_by_field_name("type")?;
                let written = ty.utf8_text(bytes).ok()?;
                out.push(type_binary_at(written, ty, bytes, symbols, resolver)?);
            }
            "spread_parameter" => return None,
            _ => {}
        }
    }
    Some(out)
}


/// A member's type in the SAME currency [`method_param_binaries`] produces: the array depth spelled
/// back into the name.
///
/// The two sides of every override question are read from different places — one from the tree,
/// through `resolve_written_type`, which returns `java/lang/String[]`; the other from the index,
/// where the depth lives in [`TypeRef::dims`] and the name holds the ELEMENT alone. Comparing them
/// directly means a method with an array parameter never matches the one it overrides, and every
/// check built on that question goes silent on it: a `final` method overridden, a widened `throws`,
/// a narrowed visibility, an incompatible return — none of them reported, on any signature taking
/// an array. Nothing looked broken, because a check that says nothing looks like a check that
/// agrees.
///
/// Brackets already in the name are left where they are: an index persisted before `dims` existed
/// spells the depth there, and adding a second pair would break the match this exists to make.
pub(crate) fn written_binary(ty: &TypeRef) -> String {
    if ty.binary_name.ends_with("[]") {
        return ty.binary_name.clone();
    }
    ty.with_brackets(&ty.binary_name)
}

/// The erased parameter types of an INDEXED method, ready to compare with [`method_param_binaries`].
pub(crate) fn member_param_binaries(m: &Member) -> Vec<String> {
    m.params.iter().map(written_binary).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two spellings have to meet: `String[]` written in a file and `String` + `dims: 1` read
    /// from the index are the same parameter, and an override matches only if they compare equal.
    #[test]
    fn an_array_parameter_reads_the_same_from_the_index_as_from_the_tree() {
        assert_eq!(written_binary(&TypeRef::simple("java/lang/String").arrayed(1)), "java/lang/String[]");
        assert_eq!(written_binary(&TypeRef::simple("int").arrayed(2)), "int[][]");
        assert_eq!(written_binary(&TypeRef::simple("java/lang/String")), "java/lang/String");
    }

    /// An index written before the depth moved out of the name already spells it — twice would
    /// match nothing.
    #[test]
    fn a_name_that_already_carries_its_brackets_keeps_exactly_those() {
        assert_eq!(written_binary(&TypeRef::simple("java/lang/String[]")), "java/lang/String[]");
    }
}
