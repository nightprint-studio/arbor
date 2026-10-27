//! Two operand-type errors that need an inferred type to state and nothing else:
//!
//! * **A `void` call used as a value** (`void.not.allowed.here`) — `"a" + log()` where `log()` returns
//!   `void`. Judged only for an operand of an operator, where there is no reading of it that works.
//! * **Indexing something that is not an array** (`array.req.but.found`) — `count[0]` where `count`
//!   is an `int`. Judged only when the indexed expression has a definite, resolved, non-array type.
//!
//! Both are silent the moment inference has any doubt: no type, an unresolved class, a type variable.

use bennu_java::prelude::{infer_node_type_cached, FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::{is_primitive, is_type_var, simple_name};

/// Every void operand and non-array index in the pre-collected node slice.
pub fn operand_type_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "binary_expression" => {
                for field in ["left", "right"] {
                    if let Some(operand) = n.child_by_field_name(field) {
                        void_operand(operand, &root, source, symbols, resolver, cache, &mut out);
                    }
                }
                incomparable(n, &root, source, symbols, resolver, cache, &mut out);
            }
            "array_access" => {
                if let Some(array) = n.child_by_field_name("array") {
                    not_an_array(array, &root, source, symbols, resolver, cache, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

/// `a == b` where `a` and `b` are classes and neither is the other or a subclass of it: no value can be
/// both (`incomparable.types`). Only for two fully-known, non-interface classes — an interface may be
/// implemented by a subclass of the other operand's type, and a type that is not fully known may
/// extend the other.
fn incomparable(
    n: Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let is_equality = n.child_by_field_name("operator").and_then(|o| o.utf8_text(source.as_bytes()).ok()).is_some_and(|o| o == "==" || o == "!=");
    if !is_equality {
        return;
    }
    let (Some(left), Some(right)) = (n.child_by_field_name("left"), n.child_by_field_name("right")) else { return };
    let (Some(a), Some(b)) = (
        infer_node_type_cached(root, source, symbols, &left, resolver, cache),
        infer_node_type_cached(root, source, symbols, &right, resolver, cache),
    ) else {
        return;
    };
    let class_of = |t: &bennu_java::prelude::TypeRef| -> Option<String> {
        if t.dims != 0 || t.binary_name.is_empty() || is_primitive(&t.binary_name) || is_type_var(&t.binary_name) {
            return None;
        }
        let members = resolver.members_of(&t.binary_name)?;
        let f = &members.flags;
        (!f.is_interface && !f.is_annotation && !f.has_hidden_members).then(|| t.binary_name.clone())
    };
    let (Some(a), Some(b)) = (class_of(&a), class_of(&b)) else { return };
    if a == b
        || !crate::support::walk::hierarchy_fully_known(resolver, &a)
        || !crate::support::walk::hierarchy_fully_known(resolver, &b)
        || crate::support::walk::reaches(resolver, &a, &b)
        || crate::support::walk::reaches(resolver, &b, &a)
    {
        return;
    }
    out.push(CheckId::IncompatibleType.at(
        n,
        format!("Incomparable types: `{}` and `{}`", simple_name(&a), simple_name(&b)),
    ));
}

fn void_operand(
    operand: Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    if operand.kind() != "method_invocation" {
        return;
    }
    let Some(ty) = infer_node_type_cached(root, source, symbols, &operand, resolver, cache) else { return };
    if ty.binary_name == "void" && ty.dims == 0 {
        out.push(CheckId::IllegalDeclaration.at(operand, "A `void` method call cannot be used as a value"));
    }
}

fn not_an_array(
    array: Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let Some(ty) = infer_node_type_cached(root, source, symbols, &array, resolver, cache) else { return };
    let name = ty.binary_name.as_str();
    if ty.dims != 0 || name.is_empty() || name == "void" || is_type_var(name) {
        return;
    }
    // A primitive is certainly not an array; a class counts only when the resolver knows it.
    let definite = is_primitive(name) || resolver.members_of(name).is_some();
    if definite {
        out.push(CheckId::IncompatibleType.at(
            array,
            format!("Array required, but `{}` found", simple_name(name)),
        ));
    }
}
