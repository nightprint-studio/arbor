//! The call to `super()` a constructor makes without being asked, and the checked exception it throws
//! (resolver-backed, JLS §8.8.7, §11.2.2).
//!
//! A constructor that does not start with `this(…)` or `super(…)` starts with an implicit `super()`.
//! When the superclass's no-argument constructor `throws IOException`, that call raises it:
//!   * a class that **declares no constructor** gets a default one that declares no `throws`, so the
//!     class itself does not compile (`unreported.exception.default.constructor`);
//!   * a constructor **without** an explicit invocation must declare the exception, or covering
//!     supertype of it (`unreported.exception.need.to.catch.or.throw`).
//!
//! Judged only when the superclass has exactly one no-argument constructor and the exception's hierarchy
//! is fully known. A class carrying any annotation is left alone — Lombok's `@NoArgsConstructor` and its
//! relatives write the constructor this reads as absent.

use bennu_java::prelude::{FileSymbols, Member, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::bare_call::index_covers_file_sigs;
use crate::support::nodes::{child_of_kind, is_type_var};
use crate::support::resolve::written_type_binary;
use crate::support::walk::{hierarchy_fully_known, reaches};
use crate::throwing::checked_throw::is_checked;

/// Every constructor, or default constructor, whose implicit `super()` throws an unhandled exception.
pub fn implicit_super_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &class in nodes.iter().filter(|n| n.kind() == "class_declaration" && !n.has_error()) {
        check_class(class, bytes, symbols, resolver, &mut out);
    }
    out
}

fn check_class(class: Node, bytes: &[u8], symbols: &FileSymbols, resolver: &dyn TypeResolver, out: &mut Vec<Diagnostic>) {
    if child_of_kind(class, "modifiers").is_some_and(|m| has_annotation(m)) {
        return;
    }
    let Some(extends) = class.child_by_field_name("superclass").and_then(|s| s.named_child(0)) else { return };
    let Some(superclass) = written_type_binary(extends, bytes, symbols, resolver) else { return };
    let Some(members) = resolver.members_of(&superclass) else { return };
    let constructors: Vec<Member> = members.methods.iter().filter(|m| m.name == "<init>").cloned().collect();
    if declared_here(symbols, &superclass) && !index_covers_file_sigs(&superclass, "<init>", &constructors, symbols, resolver) {
        return;
    }
    let mut no_arg = constructors.iter().filter(|c| c.params.is_empty());
    let (Some(constructor), None) = (no_arg.next(), no_arg.next()) else { return };
    let raised: Vec<&String> = constructor
        .throws
        .iter()
        .filter(|t| !is_type_var(t) && hierarchy_fully_known(resolver, t) && is_checked(resolver, t))
        .collect();
    if raised.is_empty() {
        return;
    }
    let Some(body) = class.child_by_field_name("body") else { return };
    let mut c = body.walk();
    let own: Vec<Node> = body.named_children(&mut c).filter(|m| m.kind() == "constructor_declaration").collect();
    let class_name = class.child_by_field_name("name").unwrap_or(class);

    if own.is_empty() {
        if let Some(thrown) = raised.first() {
            out.push(CheckId::UnhandledCheckedException.at(
                class_name,
                format!("The default constructor calls `super()`, which throws `{}`; write a constructor that declares it", simple(thrown)),
            ));
        }
        return;
    }
    for ctor in own {
        if starts_with_invocation(ctor) {
            continue;
        }
        let Some(declared) = declared_throws(ctor, bytes, symbols, resolver) else { continue };
        if let Some(thrown) = raised.iter().find(|t| !declared.iter().any(|allowed| reaches(resolver, t, allowed))) {
            out.push(CheckId::UnhandledCheckedException.at(
                ctor.child_by_field_name("name").unwrap_or(ctor),
                format!("The implicit `super()` throws `{}`; declare it or call a super constructor explicitly", simple(thrown)),
            ));
        }
    }
}

fn simple(binary: &str) -> &str {
    binary.rsplit(['/', '$']).next().unwrap_or(binary)
}

fn has_annotation(modifiers: Node) -> bool {
    let mut c = modifiers.walk();
    let found = modifiers.named_children(&mut c).any(|m| matches!(m.kind(), "annotation" | "marker_annotation"));
    found
}

fn declared_here(symbols: &FileSymbols, binary: &str) -> bool {
    symbols.types.iter().any(|t| bennu_java::prelude::same_binary_type(&t.fqn.replace('.', "/"), binary))
}

fn starts_with_invocation(ctor: Node) -> bool {
    ctor.child_by_field_name("body").is_some_and(|b| child_of_kind(b, "explicit_constructor_invocation").is_some())
}

/// The binary names of a constructor's `throws` clause; `None` when one does not resolve, because then
/// what it covers is unknown.
fn declared_throws(ctor: Node, bytes: &[u8], symbols: &FileSymbols, resolver: &dyn TypeResolver) -> Option<Vec<String>> {
    let Some(clause) = child_of_kind(ctor, "throws") else { return Some(Vec::new()) };
    let mut c = clause.walk();
    let types: Vec<Node> = clause.named_children(&mut c).collect();
    types.into_iter().map(|t| written_type_binary(t, bytes, symbols, resolver)).collect()
}
