//! Reference-type compatibility diagnostics, driven by the nominal type walk:
//!
//!   * **cast** — `(T) expr` where `T` and the value's type are unrelated concrete classes
//!     (`(String) anInteger`): the JLS "inconvertible types" error;
//!   * **assignment** — `T x = expr;` / `T f = expr;` where the value's type is not a subtype of `T`;
//!   * **return** — `return expr;` where the value's type is not a subtype of the method's return type.
//!
//! Extremely conservative (docs: never a false positive):
//!   * only when BOTH sides are **concrete classes** the resolver knows — if either is an interface,
//!     a type variable, an array, or a primitive, we skip (an interface value could implement the
//!     other side through an un-modelled path; primitives have widening/boxing rules we don't model);
//!   * the value side's hierarchy must be **fully resolvable** — an un-indexed base could establish
//!     the relation;
//!   * only value expressions the nominal walk can type (a name, a call, a field, a `new`, a cast) —
//!     a literal (`1`, `"x"`, `null`) yields no type and is skipped, dodging boxing/widening entirely.

use bennu_java::prelude::{extract_symbols, infer_node_type_cached, FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::nodes::{is_primitive, is_type_var, simple_name};

use crate::resolve::type_binary;
use crate::walk::{hierarchy_fully_known, reaches};

/// Parse `source` and flag cast / assignment / return type mismatches.
pub fn type_compat_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::check::collect_nodes(root);
    type_compat_errors_in(root, &nodes, source, &symbols, resolver, &InferCache::new())
}

/// Tree-driven core: iterates the shared `nodes` + reuses `root` + `symbols` + inference `cache`.
pub fn type_compat_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "cast_expression" => check_cast(n, &root, source, bytes, symbols, resolver, cache, &mut out),
            "local_variable_declaration" | "field_declaration" => {
                check_declaration(n, &root, source, bytes, symbols, resolver, cache, &mut out)
            }
            "return_statement" => check_return(n, &root, source, bytes, symbols, resolver, cache, &mut out),
            _ => {}
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn check_cast(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let Some(ty_node) = n.child_by_field_name("type") else { return };
    let Some(val) = n.child_by_field_name("value") else { return };
    let Ok(type_text) = ty_node.utf8_text(bytes) else { return };
    let Some(value_ty) = infer_node_type_cached(root, source, symbols, &val, resolver, cache)
    else {
        return;
    };
    // A String ↔ primitive cast is always illegal (the concrete-class path below skips primitives).
    if let Some((s, t)) = string_primitive_pair(type_text, &value_ty.binary_name, symbols, resolver) {
        out.push(err(format!("Inconvertible types: cannot cast `{s}` to `{t}`"), ty_node));
        return;
    }
    let Some(target) = concrete_class(type_text, symbols, resolver) else { return };
    let Some(source_ty) = concrete_binary(value_ty.binary_name, resolver) else { return };
    // `java/lang/Object` is the universal supertype: `(Foo) anObject` and `(Object) foo` are always
    // legal casts (only ever checked at runtime), so skip either direction. This also dodges a project
    // class whose implicit `extends Object` isn't recorded in the index — the hierarchy walk would
    // otherwise judge it "unrelated" to Object and wrongly flag `(That) obj`.
    if source_ty == "java/lang/Object" || target == "java/lang/Object" {
        return;
    }
    // Both concrete classes, both hierarchies known: a cast is legal only up or down the chain.
    if !hierarchy_fully_known(resolver, &source_ty) || !hierarchy_fully_known(resolver, &target) {
        return;
    }
    // Same simple name resolved to two binaries → treat as the same nominal type (resolution
    // artifact, e.g. an interface-declared return type), never an inconvertible cast.
    if simple_name(&source_ty) == simple_name(&target) {
        return;
    }
    if !reaches(resolver, &source_ty, &target) && !reaches(resolver, &target, &source_ty) {
        out.push(err(
            format!(
                "Inconvertible types: cannot cast `{}` to `{}`",
                simple_name(&source_ty),
                simple_name(&target)
            ),
            ty_node,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn check_declaration(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let Some(ty_node) = n.child_by_field_name("type") else { return };
    let Ok(type_text) = ty_node.utf8_text(bytes) else { return };
    if type_text == "var" {
        return; // inferred — nothing declared to violate
    }
    let mut c = n.walk();
    for d in n.named_children(&mut c) {
        if d.kind() != "variable_declarator" {
            continue;
        }
        let Some(val) = d.child_by_field_name("value") else { continue };
        assign_check(root, source, symbols, type_text, val, resolver, cache, "assigned to", out);
    }
}

#[allow(clippy::too_many_arguments)]
fn check_return(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let Some(val) = first_value_child(n) else { return };
    let Some(method) = enclosing_method(n) else { return };
    let Some(ret) = method.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok()) else {
        return;
    };
    assign_check(root, source, symbols, ret, val, resolver, cache, "returned as", out);
}

/// Flag a value that can't be assigned/returned as `target_text` under any conversion we model.
#[allow(clippy::too_many_arguments)]
fn assign_check(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    target_text: &str,
    val: Node,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    verb: &str,
    out: &mut Vec<Diagnostic>,
) {
    // A chain that passes a **function** — a lambda or a method reference — is the one shape whose
    // result type this inference cannot reach: `list.stream().map(X::getId).max(…).orElse(null)` is
    // a `Long` only because `X::getId` says so, and typing a method reference is not something the
    // walk does. It leaves the variable unresolved, which is the honest answer, and a mismatch
    // computed against an unresolved variable would be noise — so a chain like that is skipped
    // outright and left to the real compiler.
    //
    // Every OTHER chain is checked. It used to be that all of them were skipped, because generic
    // substitution once mapped a method-level type variable onto the receiver's element type and
    // produced a confidently wrong concrete answer. It no longer does — an unresolvable variable
    // comes back as itself, and `definite_assign_mismatch` cannot make a diagnostic out of one
    // (`concrete_binary` rejects it). Skipping the rest was therefore costing real errors:
    // `Optional.ofNullable(repo.findKind(id)).orElse(null)` returned as an `Integer` is a mismatch
    // in the code someone actually wrote, and it lived in a chain.
    if passes_a_function(&val) {
        return;
    }
    // `null` has no type to infer — it is assignable to every REFERENCE and to no primitive, which
    // is the one thing about it worth checking and the one thing an inference-first check cannot
    // see: `int x = null;` reached the inference, got `None`, and returned in silence.
    if val.kind() == "null_literal" {
        if let Some(p) = primitive_keyword(target_text) {
            out.push(err(format!("Incompatible types: `null` cannot be {verb} `{p}`"), val));
        }
        return;
    }
    let Some(value_ty) = infer_node_type_cached(root, source, symbols, &val, resolver, cache)
    else {
        return;
    };
    if let Some((s, t)) = definite_assign_mismatch(&value_ty.binary_name, target_text, symbols, resolver) {
        out.push(err(format!("Incompatible types: `{s}` cannot be {verb} `{t}`"), val));
    }
}

/// A definite assignment/return mismatch: `(value_display, target_display)` when `value_binary` can't
/// become `target_text`, or `None` when compatible or uncertain (never a false positive). Covers the
/// String ↔ primitive cases (which the concrete-class path skips) plus two unrelated concrete classes.
fn definite_assign_mismatch(
    value_binary: &str,
    target_text: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<(String, String)> {
    // String ↔ primitive, either direction.
    if let Some(pair) = string_primitive_pair(target_text, value_binary, symbols, resolver) {
        return Some(pair);
    }
    // A primitive value → a reference target is boxing (Object / Number / the box …) — modelled as
    // always OK here (only the String case above is a definite error). And a primitive TARGET with a
    // non-String value is widening/boxing → skip.
    if is_primitive(value_binary) || primitive_keyword(target_text).is_some() {
        return None;
    }
    // An `Object`-typed value assigns/returns to ANY reference target: Object is the universal
    // supertype, and an `Object` here is very often an erased generic whose real type is a subtype
    // (`list.get(i)` on a raw `List`, `map.get(k)`, reflection). Never a definite mismatch — every
    // type IS-A Object. Mirrors the cast rule.
    if value_binary == "java/lang/Object" {
        return None;
    }
    // Two reference concrete classes: assignment allows only widening (value is-a target).
    let target = concrete_class(target_text, symbols, resolver)?;
    if target == "java/lang/Object" {
        return None;
    }
    let source_ty = concrete_binary(value_binary.to_string(), resolver)?;
    if !hierarchy_fully_known(resolver, &source_ty) {
        return None;
    }
    // Same simple name, different binary → almost certainly the SAME nominal type resolved through
    // two paths (e.g. a method whose return type is declared on an INTERFACE it overrides, resolved
    // without the impl's full package context). Reporting "`X` cannot be assigned to `X`" is
    // nonsensical and a pure resolution artifact — never flag it (cardinal rule: no false positives).
    if simple_name(&source_ty) == simple_name(&target) {
        return None;
    }
    (!reaches(resolver, &source_ty, &target))
        .then(|| (simple_name(&source_ty).to_string(), simple_name(&target).to_string()))
}

/// When `target_text` and `value_binary` are a String/primitive pair (either direction) — an
/// inter-conversion Java never allows — return the two for the diagnostic; else `None`.
fn string_primitive_pair(
    target_text: &str,
    value_binary: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<(String, String)> {
    if let Some(p) = primitive_keyword(target_text) {
        if value_binary == "java/lang/String" {
            return Some(("String".to_string(), p.to_string()));
        }
    }
    if is_primitive(value_binary)
        && type_binary(target_text, symbols, resolver).as_deref() == Some("java/lang/String")
    {
        return Some((value_binary.to_string(), "String".to_string()));
    }
    None
}

/// The primitive keyword `text` names, if any (`int`, `long`, …). Used to catch String↔primitive.
fn primitive_keyword(text: &str) -> Option<&'static str> {
    match text.trim() {
        "int" => Some("int"),
        "long" => Some("long"),
        "short" => Some("short"),
        "byte" => Some("byte"),
        "char" => Some("char"),
        "boolean" => Some("boolean"),
        "float" => Some("float"),
        "double" => Some("double"),
        _ => None,
    }
}

/// Resolve a **written** type name (source text: `Foo`, `com.acme.Foo`) to a concrete-class binary
/// name, or `None` when it isn't one we can reason about.
fn concrete_class(text: &str, symbols: &FileSymbols, resolver: &dyn TypeResolver) -> Option<String> {
    let binary = type_binary(text, symbols, resolver)?;
    concrete_binary(binary, resolver)
}

/// Validate an already-resolved **binary** name as a concrete class: not an interface, type variable,
/// array, primitive, or unknown.
fn concrete_binary(binary: String, resolver: &dyn TypeResolver) -> Option<String> {
    if is_type_var(&binary) || binary.ends_with("[]") || is_primitive(&binary) {
        return None;
    }
    let cm = resolver.members_of(&binary)?;
    if cm.flags.is_interface {
        return None;
    }
    Some(binary)
}

/// The first non-comment named child of a `return_statement` (the returned value), or `None` for a
/// bare `return;`.
fn first_value_child(ret: Node) -> Option<Node> {
    let mut c = ret.walk();
    for n in ret.named_children(&mut c) {
        if !matches!(n.kind(), "line_comment" | "block_comment") {
            return Some(n);
        }
    }
    None
}

/// Whether `val` is a method call whose receiver is itself a method call (`a.b().c()`) — a chained
/// invocation. Our shallow generic substitution can yield a confident-but-wrong type through a chain
/// (an argument-bound type variable mis-bound to the receiver's element type — the `Stream.map`/
/// `Optional.orElse` case), so we never hard-flag a compat error off a chained value.
/// Whether `val` — or anything it is chained off — hands a **lambda or method reference** to a call.
///
/// That is the marker of a result type this inference can't compute: the type variable is bound by
/// the *function*, and typing a function is not something the walk does. Anything the chain does
/// after that point is computed from an unresolved variable.
fn passes_a_function(val: &Node) -> bool {
    let mut cur = Some(*val);
    while let Some(n) = cur {
        if n.kind() != "method_invocation" {
            return false;
        }
        if let Some(list) = n.child_by_field_name("arguments") {
            let mut c = list.walk();
            for arg in list.named_children(&mut c) {
                if matches!(arg.kind(), "lambda_expression" | "method_reference") {
                    return true;
                }
            }
        }
        cur = n.child_by_field_name("object");
    }
    false
}

/// The nearest enclosing `method_declaration`, stopping at a `lambda_expression` (a `return` inside a
/// lambda targets the lambda, not the method) — in which case there's nothing to check here.
fn enclosing_method(n: Node) -> Option<Node> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        match p.kind() {
            "lambda_expression" => return None,
            "method_declaration" => return Some(p),
            _ => cur = p.parent(),
        }
    }
    None
}

fn err(message: String, node: Node) -> Diagnostic {
    crate::check_id::CheckId::IncompatibleType.at(node, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MapResolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn getter(name: &str, ret: &str) -> Member {
        Member::method(name, TypeRef::simple(ret.to_string()), Vec::new())
    }

    fn cls(superclass: Option<&str>, methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(str::to_string),
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    /// Object; Animal; Dog extends Animal; Cat extends Animal; unrelated Widget.
    /// `Provider` with `animal()->Animal`, `dog()->Dog`, `widget()->Widget`.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cls(None, vec![]));
        members.insert("com/acme/Animal".to_string(), cls(Some("java/lang/Object"), vec![]));
        // Dog carries `cat() -> Cat` so a CHAINED call (`p.dog().cat()`) infers to a concrete type,
        // and `pick(Object) -> Cat` so a chain that passes a LAMBDA can be written — the one chain
        // shape that is still skipped.
        members.insert(
            "com/acme/Dog".to_string(),
            cls(
                Some("com/acme/Animal"),
                vec![
                    getter("cat", "com/acme/Cat"),
                    Member::method(
                        "pick",
                        TypeRef::simple("com/acme/Cat".to_string()),
                        vec![TypeRef::simple("java/lang/Object".to_string())],
                    ),
                ],
            ),
        );
        members.insert("com/acme/Cat".to_string(), cls(Some("com/acme/Animal"), vec![]));
        members.insert("com/acme/Widget".to_string(), cls(Some("java/lang/Object"), vec![]));
        members.insert(
            "com/acme/Provider".to_string(),
            cls(
                Some("java/lang/Object"),
                vec![
                    getter("animal", "com/acme/Animal"),
                    getter("dog", "com/acme/Dog"),
                    getter("widget", "com/acme/Widget"),
                    // `obj() -> Object` feeds the "Object is universally castable/assignable" tests.
                    getter("obj", "java/lang/Object"),
                ],
            ),
        );
        members.insert("java/lang/String".to_string(), cls(Some("java/lang/Object"), vec![]));
        // `List<E>` with `get(int) -> E` — drives the generic-element assignability test (a
        // `List<Dog>.get(0)` element is a `Dog`, assignable to its supertype `Animal`).
        members.insert(
            "java/util/List".to_string(),
            cls(
                None,
                vec![Member::method(
                    "get",
                    TypeRef::simple("E".to_string()),
                    vec![TypeRef::simple("int".to_string())],
                )],
            ),
        );
        // `Orphan` has NO superclass recorded (superclass = None) — stands in for a project class whose
        // implicit `extends Object` the index didn't capture, so its hierarchy walk never reaches
        // Object. Casting an `Object` to it must still be legal.
        members.insert("com/acme/Orphan".to_string(), cls(None, vec![]));
        let simple = [
            ("Object", "java/lang/Object"),
            ("Animal", "com/acme/Animal"),
            ("Dog", "com/acme/Dog"),
            ("Cat", "com/acme/Cat"),
            ("Widget", "com/acme/Widget"),
            ("Provider", "com/acme/Provider"),
            ("Orphan", "com/acme/Orphan"),
            ("String", "java/lang/String"),
            ("List", "java/util/List"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{ Provider p; void m() {{ {body} }} }}");
        type_compat_errors(&src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn null_assigned_to_a_primitive_is_flagged() {
        let d = diags("int x = null;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`null`") && d[0].contains("`int`"), "{d:?}");
    }

    #[test]
    fn null_assigned_to_a_reference_is_ok() {
        assert!(diags("String s = null;").is_empty());
    }

    #[test]
    fn null_returned_from_a_primitive_method_is_flagged() {
        let d = type_compat_errors("class C { int m() { return null; } }", &resolver())
            .into_iter()
            .map(|d| d.message)
            .collect::<Vec<_>>();
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn upcast_assignment_is_ok() {
        // Dog is-an Animal.
        assert!(diags("Animal a = p.dog();").is_empty());
    }

    #[test]
    fn unrelated_assignment_is_flagged() {
        let d = diags("Widget w = p.dog();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Dog") && d[0].contains("Widget"), "{d:?}");
    }

    #[test]
    fn downcast_assignment_without_cast_is_flagged() {
        // Animal is not a Dog without a cast.
        let d = diags("Dog x = p.animal();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("assigned to"), "{d:?}");
    }

    #[test]
    fn assign_to_object_is_ok() {
        assert!(diags("Object o = p.dog();").is_empty());
    }

    #[test]
    fn valid_downcast_is_ok() {
        // (Dog) animal — legal downcast.
        assert!(diags("Dog d = (Dog) p.animal();").is_empty());
    }

    #[test]
    fn inconvertible_cast_is_flagged() {
        let d = diags("Widget w = (Widget) p.dog();");
        assert!(d.iter().any(|m| m.contains("Inconvertible") && m.contains("Dog") && m.contains("Widget")), "{d:?}");
    }

    #[test]
    fn return_wrong_type_is_flagged() {
        let src = "class C { Provider p; Widget m() { return p.dog(); } }";
        let d: Vec<String> = type_compat_errors(src, &resolver()).into_iter().map(|x| x.message).collect();
        assert!(d.iter().any(|m| m.contains("returned as") && m.contains("Dog")), "{d:?}");
    }

    #[test]
    fn return_subtype_is_ok() {
        let src = "class C { Provider p; Animal m() { return p.dog(); } }";
        assert!(type_compat_errors(src, &resolver()).is_empty());
    }

    #[test]
    fn subtype_assigned_to_supertype_is_ok() {
        // Direct: `Dog` (from `p.dog()`) assigned to its supertype `Animal` — legal, must not flag.
        assert!(diags("Animal a = p.dog();").is_empty(), "{:?}", diags("Animal a = p.dog();"));
    }

    #[test]
    fn generic_element_subtype_assigned_to_supertype_is_ok() {
        // The user's case: `List<Dog>.get(0)` is a `Dog` (generic element), assigned to its supertype
        // `Animal`. The assignability check must walk the (substituted) element type's hierarchy —
        // NOT flag it as an incompatible type. A non-chained single call, so the chain-skip doesn't
        // hide it; this genuinely exercises subtype resolution through a generic.
        let src = "class C { java.util.List<Dog> list; void m() { Animal a = list.get(0); } }";
        let d: Vec<String> =
            type_compat_errors(src, &resolver()).into_iter().map(|x| x.message).collect();
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn same_simple_name_two_binaries_is_not_flagged() {
        // A method whose return type is declared on an interface it overrides can resolve to a
        // DIFFERENT binary than the assignment target while sharing the simple name (`Twin`). The
        // walk would judge them unrelated and emit the nonsensical "`Twin` cannot be assigned to
        // `Twin`" — the guard must suppress it. Two `Twin`s in different packages, both concrete.
        let mut r = resolver();
        r.members.insert("com/a/Twin".to_string(), cls(Some("java/lang/Object"), vec![]));
        r.members.insert("com/b/Twin".to_string(), cls(Some("java/lang/Object"), vec![]));
        r.members.insert(
            "com/acme/TwinSource".to_string(),
            cls(Some("java/lang/Object"), vec![getter("make", "com/a/Twin")]),
        );
        r.simple.insert("TwinSource".to_string(), "com/acme/TwinSource".to_string());
        // The written target name `Twin` resolves to the OTHER package's binary.
        r.simple.insert("Twin".to_string(), "com/b/Twin".to_string());
        let src = "class C { TwinSource s; void m() { Twin t = s.make(); } }";
        let d: Vec<String> =
            type_compat_errors(src, &r).into_iter().map(|x| x.message).collect();
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn a_mismatch_at_the_end_of_a_chain_is_flagged() {
        // `p.dog().cat()` is a `Cat`, which is not a `Widget`. A chain used to be skipped wholesale;
        // what is skipped now is only a chain whose type depends on a function it was passed.
        let src = "class C { Provider p; Widget m() { return p.dog().cat(); } }";
        let d: Vec<String> =
            type_compat_errors(src, &resolver()).into_iter().map(|x| x.message).collect();
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`Cat` cannot be returned as `Widget`"), "{d:?}");
    }

    #[test]
    fn a_mismatch_assigned_from_a_chain_is_flagged() {
        let d = diags("Widget w = p.dog().cat();");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    /// A lambda anywhere in the chain binds a type variable this inference can't read, so the whole
    /// chain is left to the compiler — even though the fixture's `pick` really does return a `Cat`.
    #[test]
    fn a_chain_that_passes_a_lambda_is_skipped() {
        assert!(diags("Widget w = p.dog().pick(x -> x);").is_empty());
    }

    /// Same for a method reference — the shape (`stream().map(X::getId)`) the skip exists for.
    #[test]
    fn a_chain_that_passes_a_method_reference_is_skipped() {
        assert!(diags("Widget w = p.dog().pick(String::valueOf);").is_empty());
    }

    #[test]
    fn assigning_an_object_value_to_a_class_is_not_flagged() {
        // `Object` is the universal supertype (and often an erased generic) → assigning it to a more
        // specific type is never a definite mismatch. `p.obj()` returns Object.
        assert!(diags("Widget w = p.obj();").is_empty(), "{:?}", diags("Widget w = p.obj();"));
    }

    #[test]
    fn casting_an_object_value_to_a_class_is_not_flagged() {
        // `(Orphan) obj` — Orphan has no recorded Object ancestor (a project class missing its implicit
        // `extends Object`), so without the Object special-case the walk would judge them unrelated and
        // flag the cast. Casting from Object is always legal.
        assert!(diags("Orphan o = (Orphan) p.obj();").is_empty(), "{:?}", diags("Orphan o = (Orphan) p.obj();"));
    }

    #[test]
    fn literal_and_unknown_values_are_skipped() {
        assert!(diags("Dog d = null; int n = 1 + 2;").is_empty());
    }

    // ── String ↔ primitive (literal / expression typing) ───────────────────────

    #[test]
    fn string_literal_to_int_is_flagged() {
        let d = diags("int x = \"1\";");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("String") && d[0].contains("int"), "{d:?}");
    }

    #[test]
    fn string_concat_to_int_is_flagged() {
        // `"1" + 1` is String concatenation → assigning it to int is an error.
        let d = diags("int ciao = \"1\" + 1;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("String") && d[0].contains("int"), "{d:?}");
    }

    #[test]
    fn int_literal_to_string_is_flagged() {
        let d = diags("String s = 1;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("int") && d[0].contains("String"), "{d:?}");
    }

    #[test]
    fn matching_string_and_int_are_ok() {
        assert!(diags("String s = \"ok\"; int n = 1; int m = 1 + 2;").is_empty());
    }

    #[test]
    fn numeric_widening_and_boxing_are_not_flagged() {
        // int → long (widening) and int → Object (boxing) are legal → no error.
        assert!(diags("long l = 1; Object o = 1;").is_empty());
    }

    #[test]
    fn string_to_int_cast_is_flagged() {
        let d = diags("int x = (int) \"nope\";");
        assert!(d.iter().any(|m| m.contains("Inconvertible") && m.contains("String")), "{d:?}");
    }

    #[test]
    fn return_string_from_int_method_is_flagged() {
        let src = "class C { int m() { return \"x\"; } }";
        let d: Vec<String> = type_compat_errors(src, &resolver()).into_iter().map(|x| x.message).collect();
        assert!(d.iter().any(|m| m.contains("returned as") && m.contains("String")), "{d:?}");
    }
}
