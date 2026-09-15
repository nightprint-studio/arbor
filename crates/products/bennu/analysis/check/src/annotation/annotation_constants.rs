//! A name written for an element whose declared type demands a **constant expression**, checked
//! against what that name actually is — javac's `attribute.value.must.be.constant`.
//!
//! Only elements of a primitive or `String` type get here. The other legal element types take
//! something a constant expression never is — an enum element takes an enum constant, a `Class`
//! element a class literal, an annotation element an annotation — so a name written for one of those
//! is not this check's business, and judging it by these rules would report the correct spelling.
//!
//! An operator expression is read through (`"a" + counter` is as non-constant as `counter`), and so
//! is an array initialiser, which asks the question once per entry.

use bennu_java::prelude::{FileSymbols, Member, MemberKind, TypeRef, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;

/// Report every name inside `value` that provably is not a constant variable.
pub(crate) fn check_constant_names(
    value: Node,
    declared: &TypeRef,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    // The ELEMENT family is what decides: `String[]` takes strings, one per entry.
    let (base, _) = bennu_java::prelude::split_array_dims(&declared.binary_name);
    if !(crate::support::nodes::is_primitive(base) || base == "java/lang/String") {
        return;
    }
    let mut stack = vec![value];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "element_value_array_initializer" | "binary_expression" | "parenthesized_expression" | "cast_expression" => {
                let mut c = n.walk();
                stack.extend(n.named_children(&mut c).filter(|ch| !ch.kind().ends_with("_type") && ch.kind() != "type_identifier"));
            }
            "unary_expression" => stack.extend(n.child_by_field_name("operand")),
            "identifier" | "field_access" => {
                if let Some(why) = not_a_constant_variable(n, bytes, symbols, resolver) {
                    out.push(CheckId::NonConstantAnnotationValue.at(
                        n,
                        format!("an annotation value must be a constant, and {why}"),
                    ));
                }
            }
            _ => {}
        }
    }
}

/// Why the name `value` reads is **provably** not a constant variable — or `None` when it may be
/// one, or is not a name we can resolve at all.
///
/// Java's rule (JLS §4.12.4): a constant variable is `final`, of a primitive or `String` type, and
/// initialised with a constant expression. The first two are read off the index:
///
///   * **not `final`** — never a constant, whatever it holds;
///   * **`final`, but not of a primitive or `String` type** — `static final MyObj[] OBJ = …` is as
///     `final` as anything and still not a constant variable.
///
/// The third is decided only for a field THIS file declares, whose initializer is in the tree, and
/// only when that initializer is provably computed at run time — a call, a `new`, an array read.
/// `static final String N = "n"` is the overwhelmingly common spelling and is never touched.
///
/// Two things narrow it further, both to avoid saying something wrong:
///   * a bare name written anywhere inside a `block`, a lambda or a parameter list is skipped — a
///     local or a parameter can shadow the field;
///   * a name that resolves to nothing (a static import, a field of an ENCLOSING class rather than a
///     supertype, an unindexed type) yields `None` and no diagnostic.
fn not_a_constant_variable(
    value: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<String> {
    let (owner, name) = match value.kind() {
        "identifier" => {
            if shadowable_position(value) {
                return None;
            }
            let name = value.utf8_text(bytes).ok()?;
            let crate::support::type_scope::TypeScope::Inside(owner) =
                crate::support::resolve::enclosing_scope(value, bytes, symbols)
            else {
                return None;
            };
            (owner, name)
        }
        // `Other.K` — a qualified read, so no local can shadow it. The receiver has to name a TYPE.
        "field_access" => {
            let object = value.child_by_field_name("object")?;
            let field = value.child_by_field_name("field")?;
            let owner = crate::support::resolve::type_binary_at(
                object.utf8_text(bytes).ok()?,
                value,
                bytes,
                symbols,
                resolver,
            )?;
            (owner, field.utf8_text(bytes).ok()?)
        }
        _ => return None,
    };
    let field = find_field(resolver, &owner, name)?;
    if !field.is_final {
        return Some(format!("`{name}` is not `final`"));
    }
    // An ARRAY is never a constant variable, however constant its elements would be — so the family
    // test has to be asked of the whole type.
    let ty = &field.return_type;
    if ty.is_array()
        || !(crate::support::nodes::is_primitive(&ty.binary_name) || ty.binary_name == "java/lang/String")
    {
        return Some(format!(
            "`{name}` is declared `{}`, and only a `final` primitive or `String` is one",
            crate::annotation::annotation_values::pretty(ty)
        ));
    }
    initializer_computed_at_run_time(value, &owner, name, bytes)
        .then(|| format!("`{name}` is `final`, but its value is computed at run time"))
}

/// Whether the field `name` of `owner`, declared in this file, is initialized with an expression
/// that can never be constant.
fn initializer_computed_at_run_time(site: Node, owner: &str, name: &str, bytes: &[u8]) -> bool {
    let mut root = site;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let simple = owner.rsplit(['/', '$']).next().unwrap_or(owner);
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "variable_declarator"
            && n.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) == Some(name)
            && declared_in_type(n, simple, bytes)
        {
            return n.child_by_field_name("value").is_some_and(never_constant);
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    false
}

/// Whether a declarator is a FIELD of the type named `simple`.
fn declared_in_type(declarator: Node, simple: &str, bytes: &[u8]) -> bool {
    let Some(field) = declarator.parent().filter(|p| matches!(p.kind(), "field_declaration" | "constant_declaration")) else {
        return false;
    };
    let owner = field.parent().and_then(|body| body.parent());
    owner.is_some_and(|t| t.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) == Some(simple))
}

/// Whether an expression contains something a constant expression (JLS §15.29) never does.
fn never_constant(expr: Node) -> bool {
    let mut stack = vec![expr];
    while let Some(n) = stack.pop() {
        if matches!(
            n.kind(),
            "method_invocation" | "object_creation_expression" | "array_creation_expression" | "array_access"
                | "lambda_expression" | "method_reference" | "assignment_expression" | "update_expression"
                | "instanceof_expression" | "this" | "super" | "switch_expression" | "array_initializer"
        ) {
            return true;
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    false
}

/// Whether `node` sits somewhere a local or a parameter could shadow a field of the same name.
fn shadowable_position(node: Node) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if matches!(n.kind(), "block" | "formal_parameters" | "lambda_expression") {
            return true;
        }
        cur = n.parent();
    }
    false
}

/// The field named `name` on `owner` or any KNOWN supertype — `None` when nothing declares it (the
/// hierarchy may simply be incomplete, which is why the caller treats `None` as "say nothing").
fn find_field(resolver: &dyn TypeResolver, owner: &str, name: &str) -> Option<Member> {
    let mut found = None;
    crate::support::walk::for_each_supertype(resolver, owner, &mut |_, cm| {
        if found.is_none() {
            found = cm.fields.iter().find(|f| f.name == name && f.kind == MemberKind::Field).cloned();
        }
    });
    found
}
