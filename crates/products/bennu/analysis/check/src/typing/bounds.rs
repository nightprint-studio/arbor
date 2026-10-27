//! Type arguments outside the bounds of the type variable they fill (resolver-backed, JLS §4.5).
//!
//! `NumberBox<String>` is an error when `NumberBox<T extends Number>` is declared in the file: a type
//! argument must be a subtype of every bound of its variable. Judged over the **erasure** of the bound,
//! which is all that can be said without inference — an argument that is not even a subtype of the
//! raw bound (`Object` against `Comparable`, `String` against `Number`) is outside it whatever the
//! bound's own type arguments are, while one that is a subtype of the raw bound might still miss the
//! parameterisation, and is left alone.
//!
//! Only a *plain class* type argument is judged. A wildcard `? extends X` is outside the bound only
//! when `X` and the bound are two classes with no subtype relation between them (two interfaces, or an
//! interface and a class, may still share a subtype). Never judged: a type variable in scope, an array,
//! a primitive, a type that does not resolve, a hierarchy with a gap, a bound that is itself a type
//! variable, and a generic type declared more than once in the file.

use std::collections::HashMap;

use bennu_java::prelude::{FileSymbols, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::source::generics_syntax::type_params_in_scope;
use crate::support::resolve::written_type_binary;
use crate::support::walk::{hierarchy_fully_known, reaches};

const OBJECT: &str = "java/lang/Object";

/// One declared type variable and the bounds written on it.
struct Variable<'t> {
    name: String,
    bounds: Vec<Node<'t>>,
}

/// A generic type declared in the file: its name and variables, or `None` when the name is ambiguous.
type Declared<'t> = HashMap<&'t str, Option<(&'t str, Vec<Variable<'t>>)>>;

/// Flag type arguments outside the declared bounds of a generic type of this file.
pub fn bound_errors_in(nodes: &[Node], source: &str, symbols: &FileSymbols, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let declared = declared_generics(nodes, bytes);
    let mut out = Vec::new();
    for &n in nodes.iter().filter(|n| n.kind() == "generic_type") {
        check_usage(n, bytes, &declared, symbols, resolver, &mut out);
    }
    out
}

fn declared_generics<'t>(nodes: &[Node<'t>], bytes: &'t [u8]) -> Declared<'t> {
    let mut map: Declared = HashMap::new();
    for &n in nodes.iter().filter(|n| {
        matches!(n.kind(), "class_declaration" | "interface_declaration" | "record_declaration" | "enum_declaration")
    }) {
        let Some(name) = n.child_by_field_name("name").and_then(|id| id.utf8_text(bytes).ok()) else { continue };
        let variables = n.child_by_field_name("type_parameters").map(|tp| variables_of(tp, bytes)).unwrap_or_default();
        let entry = Some((name, variables));
        map.entry(name).and_modify(|slot| *slot = None).or_insert(entry);
    }
    map
}

fn variables_of<'t>(type_parameters: Node<'t>, bytes: &[u8]) -> Vec<Variable<'t>> {
    let mut c = type_parameters.walk();
    let params: Vec<Node> = type_parameters.named_children(&mut c).filter(|p| p.kind() == "type_parameter").collect();
    params
        .into_iter()
        .filter_map(|p| {
            let mut pc = p.walk();
            let name = p.named_children(&mut pc).find(|ch| ch.kind() == "type_identifier")?.utf8_text(bytes).ok()?.to_string();
            let bounds = match crate::support::nodes::child_of_kind(p, "type_bound") {
                Some(bound) => {
                    let mut bc = bound.walk();
                    let list: Vec<Node> = bound.named_children(&mut bc).collect();
                    list
                }
                None => Vec::new(),
            };
            Some(Variable { name, bounds })
        })
        .collect()
}

fn check_usage(
    usage: Node,
    bytes: &[u8],
    declared: &Declared,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(base) = usage.named_child(0).filter(|b| b.kind() == "type_identifier") else { return };
    let Ok(base_name) = base.utf8_text(bytes) else { return };
    let Some(Some((declared_name, variables))) = declared.get(base_name) else { return };
    let Some(arguments) = crate::support::nodes::child_of_kind(usage, "type_arguments") else { return };
    let mut ac = arguments.walk();
    let args: Vec<Node> = arguments.named_children(&mut ac).collect();
    if args.len() != variables.len() {
        return;
    }
    let in_scope = type_params_in_scope(usage, bytes);
    for (arg, variable) in args.iter().zip(variables) {
        for bound in &variable.bounds {
            if outside_bound(*arg, *bound, bytes, &in_scope, variables, symbols, resolver) {
                let written = arg.utf8_text(bytes).unwrap_or_default();
                out.push(CheckId::IllegalGenericUsage.at(
                    *arg,
                    format!(
                        "Type argument `{written}` is not within the bounds of type variable `{}` of `{declared_name}`",
                        variable.name
                    ),
                ));
                break;
            }
        }
    }
}

/// Whether `arg` is certainly not a legal argument for a variable bounded by `bound`.
fn outside_bound(
    arg: Node,
    bound: Node,
    bytes: &[u8],
    in_scope: &[String],
    variables: &[Variable],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> bool {
    // A bound that IS one of the declaration's own variables (`U extends T`) needs substitution. One
    // that merely mentions it (`Comparable<T>`) is judged by its erasure, `Comparable`.
    if is_a_variable(bound, variables, bytes) {
        return false;
    }
    let Some(bound_binary) = written_type_binary(bound, bytes, symbols, resolver) else { return false };
    if bound_binary == OBJECT || !is_known(&bound_binary, resolver) {
        return false;
    }
    match arg.kind() {
        "type_identifier" | "scoped_type_identifier" | "generic_type" => {
            let Ok(text) = arg.utf8_text(bytes) else { return false };
            if in_scope.iter().any(|p| p == text) {
                return false;
            }
            let Some(arg_binary) = written_type_binary(arg, bytes, symbols, resolver) else { return false };
            is_known(&arg_binary, resolver) && !reaches(resolver, &arg_binary, &bound_binary)
        }
        "wildcard" => {
            // `? extends X` only; `?` and `? super X` leave room for a type that fits.
            let mut c = arg.walk();
            let has_super = arg.children(&mut c).any(|ch| ch.kind() == "super");
            let mut bc = arg.walk();
            let upper = arg.named_children(&mut bc).find(|ch| !matches!(ch.kind(), "annotation" | "marker_annotation"));
            let Some(upper) = upper.filter(|_| !has_super) else { return false };
            let Some(upper_binary) = written_type_binary(upper, bytes, symbols, resolver) else { return false };
            is_a_class(&upper_binary, resolver)
                && is_a_class(&bound_binary, resolver)
                && !reaches(resolver, &upper_binary, &bound_binary)
                && !reaches(resolver, &bound_binary, &upper_binary)
        }
        _ => false,
    }
}

/// Resolvable, with no gap anywhere in its hierarchy — so "not a subtype" is a fact and not a guess.
fn is_known(binary: &str, resolver: &dyn TypeResolver) -> bool {
    resolver.members_of(binary).is_some() && hierarchy_fully_known(resolver, binary)
}

fn is_a_class(binary: &str, resolver: &dyn TypeResolver) -> bool {
    is_known(binary, resolver)
        && resolver.members_of(binary).is_some_and(|m| !m.flags.is_interface && !m.flags.is_annotation)
}

fn is_a_variable(bound: Node, variables: &[Variable], bytes: &[u8]) -> bool {
    bound.kind() == "type_identifier" && bound.utf8_text(bytes).is_ok_and(|t| variables.iter().any(|v| v.name == t))
}
