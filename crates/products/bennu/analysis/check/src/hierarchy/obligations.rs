//! What a concrete class owes its supertypes, and what it pays with — by signature, not only by name.
//!
//! An abstract method is implemented by a method of the same name **and the same erased parameter
//! types** whose return type is compatible (JLS §8.4.8). Judging by name alone let two real mistakes
//! through: `greet(Object)` written against an interface's `greet(String)` is an overload that
//! implements nothing, and `Object name()` against `String name()` implements nothing either.
//!
//! Every comparison that cannot be made exactly counts as a match. A parameter typed by a type
//! variable (`compareTo(T)`) is erased to a bound this module does not compute, so a requirement or a
//! provision carrying one matches anything of its name; so does a varargs method, or one whose
//! parameter types do not resolve. A missed report costs nothing; a false one accuses working code.

use std::collections::{HashMap, HashSet};

use bennu_java::prelude::{same_binary_type, split_array_dims, ClassMembers, FileSymbols, Member, MemberKind, TypeResolver};
use tree_sitter::Node;

use crate::hierarchy::inheritance::{is_abstract_requirement, is_ctor};
use crate::support::method_sig::{method_param_binaries, written_binary};
use crate::support::walk::{for_each_supertype, hierarchy_fully_known, reaches};

/// What a method returns, as far as an override compares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Returns {
    Void,
    Primitive(String),
    /// A readable reference class, by binary name.
    Reference(String),
    /// An array, a type variable, anything unresolved — compatible with everything.
    Unknown,
}

/// One abstract method a concrete class owes.
pub(crate) struct Requirement {
    pub(crate) name: String,
    /// The erased parameter types, or `None` when one of them cannot be compared exactly.
    pub(crate) params: Option<Vec<String>>,
    pub(crate) returns: Returns,
}

/// One method that satisfies requirements of its name.
pub(crate) struct Provision {
    pub(crate) params: Option<Vec<String>>,
    pub(crate) returns: Returns,
}

/// The abstract methods `supers`' hierarchies require, and what they already provide — `Object`'s
/// methods included, which every class inherits.
pub(crate) fn obligations(
    supers: &[String],
    resolver: &dyn TypeResolver,
    object_methods: &HashSet<String>,
) -> (Vec<Requirement>, HashMap<String, Vec<Provision>>) {
    let mut required: Vec<Requirement> = Vec::new();
    let mut provided: HashMap<String, Vec<Provision>> = object_methods
        .iter()
        .map(|name| (name.clone(), vec![Provision { params: None, returns: Returns::Unknown }]))
        .collect();
    for s in supers {
        for_each_supertype(resolver, s, &mut |_bn, cm: &ClassMembers| {
            for m in &cm.methods {
                if m.kind != MemberKind::Method || is_ctor(&m.name) {
                    continue;
                }
                let params = indexed_params(m, resolver);
                if is_abstract_requirement(cm, m) {
                    if !required.iter().any(|r| r.name == m.name && r.params == params) {
                        required.push(Requirement {
                            name: m.name.clone(),
                            params,
                            returns: indexed_returns(m, resolver),
                        });
                    }
                } else {
                    provided
                        .entry(m.name.clone())
                        .or_default()
                        .push(Provision { params, returns: Returns::Unknown });
                }
            }
        });
    }
    (required, provided)
}

/// What an INDEXED member provides — a Lombok accessor, a method the index knows the class declares.
pub(crate) fn indexed_provision(m: &Member, resolver: &dyn TypeResolver) -> Provision {
    Provision { params: indexed_params(m, resolver), returns: Returns::Unknown }
}

/// What a method DECLARED in a body provides, read off the tree.
pub(crate) fn declared_provision(
    method: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Provision {
    Provision {
        params: method_param_binaries(method, bytes, symbols, resolver),
        returns: declared_returns(method, bytes, symbols, resolver),
    }
}

/// The requirements nothing in `provided` satisfies, in a stable order.
pub(crate) fn unmet<'r>(
    required: &'r [Requirement],
    provided: &HashMap<String, Vec<Provision>>,
    resolver: &dyn TypeResolver,
) -> Vec<&'r Requirement> {
    let mut missing: Vec<&Requirement> = required
        .iter()
        .filter(|r| !provided.get(&r.name).is_some_and(|ps| ps.iter().any(|p| satisfies(p, r, resolver))))
        .collect();
    missing.sort_by(|a, b| a.name.cmp(&b.name));
    missing.dedup_by(|a, b| a.name == b.name);
    missing
}

fn satisfies(p: &Provision, r: &Requirement, resolver: &dyn TypeResolver) -> bool {
    let (Some(given), Some(wanted)) = (&p.params, &r.params) else { return true };
    params_match(given, wanted) && !returns_incompatible(&p.returns, &r.returns, resolver)
}

/// Two erased parameter lists naming the same types — however each side spells a nested type.
pub(crate) fn params_match(a: &[String], b: &[String]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            let (xe, xd) = split_array_dims(x);
            let (ye, yd) = split_array_dims(y);
            xd == yd && same_binary_type(xe, ye)
        })
}

/// Whether an override returning `sub` provably cannot stand in for one returning `sup`: `void`
/// against a value, a primitive against anything else, or a reference that is no subtype of the
/// other over hierarchies read to the end.
pub(crate) fn returns_incompatible(sub: &Returns, sup: &Returns, resolver: &dyn TypeResolver) -> bool {
    match (sub, sup) {
        (Returns::Unknown, _) | (_, Returns::Unknown) | (Returns::Void, Returns::Void) => false,
        (Returns::Void, _) | (_, Returns::Void) => true,
        (Returns::Primitive(a), Returns::Primitive(b)) => a != b,
        (Returns::Primitive(_), Returns::Reference(_)) | (Returns::Reference(_), Returns::Primitive(_)) => true,
        (Returns::Reference(a), Returns::Reference(b)) => {
            !same_binary_type(a, b)
                && hierarchy_fully_known(resolver, a)
                && hierarchy_fully_known(resolver, b)
                && !reaches(resolver, a, b)
        }
    }
}

/// An indexed member's return type.
pub(crate) fn indexed_returns(m: &Member, resolver: &dyn TypeResolver) -> Returns {
    let ty = &m.return_type;
    if ty.is_array() {
        return Returns::Unknown;
    }
    match ty.binary_name.as_str() {
        "void" => Returns::Void,
        p if crate::support::nodes::is_primitive(p) => Returns::Primitive(p.to_string()),
        b if b.contains('/') && resolver.members_of(b).is_some() => Returns::Reference(b.to_string()),
        _ => Returns::Unknown,
    }
}

/// A declared method's return type, read off the tree.
pub(crate) fn declared_returns(
    method: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Returns {
    let Some(ty) = method.child_by_field_name("type") else { return Returns::Unknown };
    // `int m()[]` returns an array, whatever the type node says.
    if method.child_by_field_name("dimensions").is_some() {
        return Returns::Unknown;
    }
    let Ok(text) = ty.utf8_text(bytes) else { return Returns::Unknown };
    match ty.kind() {
        "void_type" => Returns::Void,
        "integral_type" | "floating_point_type" | "boolean_type" => Returns::Primitive(text.trim().to_string()),
        "type_identifier" | "scoped_type_identifier" | "generic_type" => {
            match crate::support::resolve::type_binary_at(text, ty, bytes, symbols, resolver) {
                Some(b) if resolver.members_of(&b).is_some() => Returns::Reference(b),
                _ => Returns::Unknown,
            }
        }
        _ => Returns::Unknown,
    }
}

/// An indexed member's erased parameter types — `None` when one is a type variable, whose erasure
/// is a bound this does not compute.
fn indexed_params(m: &Member, resolver: &dyn TypeResolver) -> Option<Vec<String>> {
    m.params
        .iter()
        .map(|p| {
            let variable = p.dims == 0
                && !crate::support::nodes::is_primitive(&p.binary_name)
                && !p.binary_name.contains('/')
                && resolver.members_of(&p.binary_name).is_none();
            (!variable && !p.binary_name.ends_with("[]")).then(|| written_binary(p))
        })
        .collect()
}
