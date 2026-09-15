//! Which constructors a `new T(…)`, a `super(…)` or a `this(…)` can bind to — one answer for the
//! argument-count and the argument-type checks, so the two never disagree about a candidate set.
//!
//! Constructors are not inherited, so the candidates are one type's own `<init>` members: the
//! created type's, the enclosing type's for `this(…)`, its superclass's for `super(…)`. Plus the
//! constructor the language writes when the source writes none (JLS §8.8.9): a class read from a
//! class file carries it already, because javac emits it, but one indexed from source does not —
//! and without it `new Plain(1)` had no candidate to be refused by.
//!
//! Never a guess (`None`):
//!   * `outer.new Inner()` and `outer.super()` — see [`is_qualified_creation`];
//!   * an anonymous class of an interface, which takes no arguments at all — a different error;
//!   * a type with no constructor in the index that this file does not declare as a plain,
//!     unannotated class: an annotation may generate the constructors (Lombok), and an index may
//!     omit them;
//!   * `super(…)` from anything but a class, or into a nested type this file does not declare — a
//!     class file's inner-class constructor takes the enclosing instance as a parameter no source
//!     writes.

use bennu_java::prelude::{
    same_binary_type, type_decl_at, ClassMembers, FileSymbols, Member, MemberKind, TypeKind,
    TypeRef, TypeResolver,
};
use tree_sitter::Node;

use crate::support::nodes::is_qualified_creation;
use crate::support::resolve::type_binary;

/// A constructor call and the constructors it may bind to.
pub(crate) struct ConstructorCall<'t> {
    /// The type whose constructors the candidates are.
    pub(crate) binary: String,
    pub(crate) candidates: Vec<Member>,
    /// Where a whole-call report starts: the created type, or the `this` / `super` keyword.
    pub(crate) head: Node<'t>,
    pub(crate) args: Node<'t>,
}

/// The constructor call `n` makes — an `object_creation_expression` or an
/// `explicit_constructor_invocation` — when its candidate set is known exactly.
pub(crate) fn constructor_call<'t>(
    n: Node<'t>,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<ConstructorCall<'t>> {
    let args = n.child_by_field_name("arguments")?;
    let (binary, head) = match n.kind() {
        "object_creation_expression" => created_type(n, bytes, symbols, resolver)?,
        "explicit_constructor_invocation" => invoked_type(n, bytes, symbols, resolver)?,
        _ => return None,
    };
    let members = resolver.members_of(&binary)?;
    let candidates = constructors_of(&binary, &members, symbols)?;
    Some(ConstructorCall { binary, candidates, head, args })
}

/// `new Foo(…)`, with or without an anonymous body: the body's implicit constructor forwards to the
/// one of `Foo` the arguments select (JLS §15.9.5.1) — unless `Foo` is an interface.
fn created_type<'t>(
    n: Node<'t>,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<(String, Node<'t>)> {
    if is_qualified_creation(n) {
        return None;
    }
    let ty = n.child_by_field_name("type")?;
    let binary = type_binary(ty.utf8_text(bytes).ok()?, symbols, resolver)?;
    if has_class_body(n) && resolver.members_of(&binary)?.flags.is_interface {
        return None;
    }
    Some((binary, ty))
}

/// `this(…)` binds to the enclosing type's constructors, `super(…)` to its superclass's.
fn invoked_type<'t>(
    n: Node<'t>,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<(String, Node<'t>)> {
    if n.child_by_field_name("object").is_some() {
        return None;
    }
    let keyword = n.child_by_field_name("constructor")?;
    let owner = type_decl_at(&enclosing_type_declaration(n)?, symbols)?;
    let own = type_binary(&owner.fqn, symbols, resolver)?;
    match keyword.utf8_text(bytes).ok()? {
        "this" if !matches!(owner.kind, TypeKind::Interface | TypeKind::Annotation) => Some((own, keyword)),
        "super" if owner.kind == TypeKind::Class => {
            let superclass = resolver.members_of(&own)?.superclass.clone()?.binary_name;
            let foreign_nested = superclass.contains('$') && !declares(symbols, &superclass);
            (!foreign_nested).then_some((superclass, keyword))
        }
        _ => None,
    }
}

/// `binary`'s own constructors, or the implicit default one of a plain class this file declares
/// with none.
fn constructors_of(binary: &str, members: &ClassMembers, symbols: &FileSymbols) -> Option<Vec<Member>> {
    let declared: Vec<Member> = members
        .methods
        .iter()
        .filter(|m| m.name == "<init>" && m.kind == MemberKind::Method)
        .cloned()
        .collect();
    if !declared.is_empty() {
        return Some(declared);
    }
    let plain = symbols.types.iter().any(|t| {
        t.kind == TypeKind::Class
            && !t.is_anonymous
            && t.annotations.is_empty()
            && t.methods.iter().all(|m| m.name != "<init>")
            && same_binary_type(&t.fqn.replace('.', "/"), binary)
    });
    plain.then(|| vec![Member::method("<init>", TypeRef::simple("void"), Vec::new())])
}

fn declares(symbols: &FileSymbols, binary: &str) -> bool {
    symbols.types.iter().any(|t| same_binary_type(&t.fqn.replace('.', "/"), binary))
}

/// The type declaration a constructor body sits in; `None` inside an anonymous class.
fn enclosing_type_declaration(n: Node) -> Option<Node> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        match p.kind() {
            "class_declaration" | "enum_declaration" | "record_declaration" | "interface_declaration" => {
                return Some(p)
            }
            "object_creation_expression" => return None,
            _ => cur = p.parent(),
        }
    }
    None
}

fn has_class_body(n: Node) -> bool {
    let mut c = n.walk();
    for child in n.named_children(&mut c) {
        if child.kind() == "class_body" {
            return true;
        }
    }
    false
}
