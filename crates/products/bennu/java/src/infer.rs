//! Local type inference: [`infer_receiver_type`]`(source, byte_offset, resolver)`.
//!
//! Phase-1 scope (Spike B — nominal walks, NOT flow-typing):
//!   * local variable declared types (incl. `Foo x = ...`), method parameters
//!   * `this` and `this.field` field types
//!   * method-return-type chaining (`a.getB().getC()`)
//!   * simple generics carry-through (`List<Foo>` -> `.get(i)` / `.iterator().next()`
//!     element = `Foo`)
//!
//! Explicitly NOT handled (documented in the crate README): overload resolution by
//! argument types (we pick the FIRST method matching by name), full flow-typing /
//! reassignment, conditional/ternary narrowing, raw-array element inference, static
//! member access on bare type names, wildcard/bound modelling.

use tree_sitter::{Node, Parser};

use crate::seam::{TypeRef, TypeResolver};
use crate::symbols::{node_text, FileSymbols};
use crate::typeparse::{parse_type_text, SimpleTypeRef};

/// Infer the static type of the expression immediately LEFT of the `.` at
/// `byte_offset`. Returns `None` when we can't resolve it under Phase-1 rules.
pub fn infer_receiver_type(
    source: &str,
    byte_offset: usize,
    resolver: &dyn TypeResolver,
) -> Option<TypeRef> {
    // Completion trick: when the caret sits right after a `.` with no identifier yet
    // (`expr.<caret>`), the trailing dot makes tree-sitter flatten the enclosing
    // members into one ERROR node — which loses the `class_declaration` ancestor and
    // local scopes. Splice a dummy call at the caret so the buffer parses cleanly,
    // then work on the repaired source. In a real editor there is usually a partial
    // identifier already; this just normalises the empty-prefix case.
    let needs_stub = matches!(
        source.as_bytes().get(byte_offset),
        None | Some(b' ' | b'\t' | b'\n' | b'\r' | b'}' | b')' | b';')
    );
    let (buf, off) = if needs_stub {
        let mut s = String::with_capacity(source.len() + 11);
        s.push_str(&source[..byte_offset]);
        // A call, not a bare name: `s.__bennu__` alone parses as a scoped *type*
        // (declaration ambiguity); `s.__bennu__()` is unambiguously an expression.
        s.push_str("__bennu__()");
        s.push_str(&source[byte_offset..]);
        (s, byte_offset)
    } else {
        (source.to_string(), byte_offset)
    };

    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).ok()?;
    let tree = parser.parse(&buf, None)?;
    let symbols = crate::symbols::extract_symbols(&buf);
    infer_receiver_type_at(&tree.root_node(), &buf, &symbols, off, resolver)
}

/// Infer the receiver type at `byte_offset` reusing an ALREADY-parsed `root` and
/// ALREADY-extracted `symbols` over `source` — the hot path for the reference-index walk.
///
/// The walk queries the receiver type at every `obj.method()` / `obj.field` site in a file;
/// [`infer_receiver_type`] would re-parse the whole file AND re-extract its symbols on each
/// call, which is quadratic per file and, on a large legacy project (tens of thousands of
/// members), makes the reference-index build effectively never finish. This variant does
/// neither: it walks the caller's tree + symbols. It assumes the offset sits on a real
/// member name (no trailing-dot completion stub) — the reference walk always does.
pub fn infer_receiver_type_at(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    byte_offset: usize,
    resolver: &dyn TypeResolver,
) -> Option<TypeRef> {
    let bytes = source.as_bytes();
    let receiver = find_receiver(root, byte_offset)?;
    let ctx = Ctx { bytes, resolver, symbols };
    let enclosing = enclosing_type_fqn(&receiver, bytes, symbols);
    ctx.infer_expr(&receiver, enclosing.as_deref())
}

/// Shared inference context.
struct Ctx<'a> {
    bytes: &'a [u8],
    resolver: &'a dyn TypeResolver,
    symbols: &'a FileSymbols,
}

impl Ctx<'_> {
    /// Infer the type of an arbitrary receiver expression node.
    fn infer_expr(&self, node: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        match node.kind() {
            // `foo` — a bare name: local var / param / field of the enclosing type.
            "identifier" => self.infer_identifier(node, enclosing),
            // `this`
            "this" => enclosing.map(|fqn| TypeRef::simple(to_binary(fqn))),
            // `super` — the enclosing type's SUPERCLASS, so `super.m()` / `super.f` resolve to
            // the parent's member (not the enclosing type's override of the same name).
            "super" => enclosing.and_then(|fqn| self.superclass_type(fqn)),
            // `a.b` — field access (possibly `this.b`)
            "field_access" => self.infer_field_access(node, enclosing),
            // `a.foo(...)` or `foo(...)`
            "method_invocation" => self.infer_method_invocation(node, enclosing),
            // `(Foo) x`
            "cast_expression" => {
                let ty = node.child_by_field_name("type")?;
                let text = node_text(&ty, self.bytes)?;
                self.resolve_type_text(&text)
            }
            // `(expr)`
            "parenthesized_expression" => {
                let inner = node.named_child(0)?;
                self.infer_expr(&inner, enclosing)
            }
            // `new Foo(...)` / `new List<Foo>()`
            "object_creation_expression" => {
                let ty = node.child_by_field_name("type")?;
                let text = node_text(&ty, self.bytes)?;
                self.resolve_type_text(&text)
            }
            // Raw-array element access is out of Phase-1 scope (only generics
            // carry-through, handled at method-invocation).
            "array_access" => None,
            _ => None,
        }
    }

    /// A bare identifier: resolve as local var / parameter first (walking up scopes),
    /// then as a field of the enclosing type.
    fn infer_identifier(&self, node: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        let name = node_text(node, self.bytes)?;

        if let Some(tr) = self.resolve_local(node, &name) {
            return Some(tr);
        }
        if let Some(fqn) = enclosing {
            if let Some(tr) = self.field_type_of_source_type(fqn, &name) {
                return Some(tr);
            }
        }
        None
    }

    /// `a.b`: infer `a`, then look up field `b` on it. Handles `this.b`.
    fn infer_field_access(&self, node: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        let obj = node.child_by_field_name("object")?;
        let field = node.child_by_field_name("field")?;
        let field_name = node_text(&field, self.bytes)?;

        let obj_type = self.infer_expr(&obj, enclosing)?;
        self.field_type_on(&obj_type, &field_name)
    }

    /// `recv.foo(args)` or bare `foo(args)`. Resolves `foo`'s return type, applying
    /// generics carry-through from the receiver.
    fn infer_method_invocation(&self, node: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        let name =
            node.child_by_field_name("name").and_then(|n| node_text(&n, self.bytes))?;

        let recv_type = match node.child_by_field_name("object") {
            Some(obj) => self.infer_expr(&obj, enclosing)?,
            // Bare call `foo()` → resolves against the enclosing type.
            None => TypeRef::simple(to_binary(enclosing?)),
        };

        self.method_return_on(&recv_type, &name)
    }

    // ---- member resolution over a resolved TypeRef (walking supertypes) ----

    /// Field type of `field_name` on a resolved type (source type or classpath type),
    /// applying generic substitution from `recv`.
    fn field_type_on(&self, recv: &TypeRef, field_name: &str) -> Option<TypeRef> {
        if let Some(tr) =
            self.field_type_of_source_type(&from_binary(&recv.binary_name), field_name)
        {
            return Some(tr);
        }
        let member = self.resolve_and_walk(&recv.binary_name, |cm| {
            cm.fields.iter().find(|m| m.name == field_name).map(|m| m.return_type.clone())
        })?;
        Some(self.substitute_generics(&member, recv))
    }

    /// Return type of `method_name` on a resolved type, applying generic
    /// substitution from `recv` (so `List<Foo>.get` → `Foo`).
    fn method_return_on(&self, recv: &TypeRef, method_name: &str) -> Option<TypeRef> {
        if let Some(tr) =
            self.method_return_of_source_type(&from_binary(&recv.binary_name), method_name)
        {
            return Some(tr);
        }
        let ret = self.resolve_and_walk(&recv.binary_name, |cm| {
            cm.methods.iter().find(|m| m.name == method_name).map(|m| m.return_type.clone())
        })?;
        Some(self.substitute_generics(&ret, recv))
    }

    /// Walk the class + its superclass/interfaces, returning the first `f` hit.
    fn resolve_and_walk<T>(
        &self,
        binary_name: &str,
        f: impl Fn(&crate::seam::ClassMembers) -> Option<T>,
    ) -> Option<T> {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![binary_name.to_string()];
        while let Some(bn) = stack.pop() {
            if !visited.insert(bn.clone()) {
                continue;
            }
            if let Some(cm) = self.resolver.members_of(&bn) {
                if let Some(hit) = f(&cm) {
                    return Some(hit);
                }
                // `cm` is a shared `Arc` — clone the (small) supertype links rather than
                // moving them out.
                if let Some(sc) = cm.superclass.clone() {
                    stack.push(sc);
                }
                stack.extend(cm.interfaces.iter().cloned());
            }
        }
        None
    }

    /// Substitute type variables in `member_ret` with the receiver's actual type
    /// arguments. Phase-1 heuristic (seam caveat C2): a single-uppercase-letter type
    /// (`E`, `T`, `K`, `V`) maps to the receiver's first type arg (`V` → second when
    /// present, for `Map<K,V>`); nested type args recurse one hop (so `List<Foo>`'s
    /// `iterator() -> Iterator<E>` becomes `Iterator<Foo>`). Deliberately shallow.
    fn substitute_generics(&self, member_ret: &TypeRef, recv: &TypeRef) -> TypeRef {
        if recv.type_args.is_empty() {
            return member_ret.clone();
        }
        let bn = &member_ret.binary_name;
        let is_type_var = bn.len() == 1 && bn.chars().all(|c| c.is_ascii_uppercase());
        if is_type_var {
            let idx = if bn == "V" && recv.type_args.len() >= 2 { 1 } else { 0 };
            return recv.type_args.get(idx).cloned().unwrap_or_else(|| member_ret.clone());
        }
        TypeRef {
            binary_name: bn.clone(),
            type_args: member_ret
                .type_args
                .iter()
                .map(|a| self.substitute_generics(a, recv))
                .collect(),
        }
    }

    // ---- source-declared type lookups ----

    /// Look up a field on a source-declared type by its FQN (or bare name).
    fn field_type_of_source_type(&self, fqn: &str, field_name: &str) -> Option<TypeRef> {
        let td = self.symbols.types.iter().find(|t| t.fqn == fqn || t.name == fqn)?;
        let fd = td.fields.iter().find(|f| f.name == field_name)?;
        self.resolve_type_text(&fd.type_text)
    }

    /// The resolved superclass type of the source type `fqn` (for `super.member`). `None` when
    /// the type has no `extends` clause (implicit `Object`, which the project-only engine won't
    /// decode anyway) or its declaration isn't in this file's symbols.
    fn superclass_type(&self, fqn: &str) -> Option<TypeRef> {
        let td = self.symbols.types.iter().find(|t| t.fqn == fqn || t.name == fqn)?;
        let ext = td.extends.as_ref()?;
        self.resolve_type_text(ext)
    }

    /// Look up a method return on a source-declared type by its FQN (or bare name).
    fn method_return_of_source_type(&self, fqn: &str, method_name: &str) -> Option<TypeRef> {
        let td = self.symbols.types.iter().find(|t| t.fqn == fqn || t.name == fqn)?;
        let md = td.methods.iter().find(|m| m.name == method_name)?;
        self.resolve_type_text(&md.return_type_text)
    }

    // ---- local scope resolution ----

    /// Resolve `name` as a local variable or method parameter visible at `use_node`.
    /// Walks ancestors, scanning each block/method for a matching declaration that
    /// precedes the use.
    fn resolve_local(&self, use_node: &Node, name: &str) -> Option<TypeRef> {
        let use_start = use_node.start_byte();
        let mut scope = use_node.parent();
        while let Some(s) = scope {
            // method / lambda / constructor parameters
            if let Some(params) = s.child_by_field_name("parameters") {
                let mut pw = params.walk();
                for p in params.named_children(&mut pw) {
                    if p.kind() == "formal_parameter" || p.kind() == "spread_parameter" {
                        if let Some(pn) =
                            p.child_by_field_name("name").and_then(|n| node_text(&n, self.bytes))
                        {
                            if pn == name {
                                let t = p
                                    .child_by_field_name("type")
                                    .and_then(|n| node_text(&n, self.bytes))?;
                                return self.resolve_type_text(&t);
                            }
                        }
                    }
                }
            }
            // local variable declarations directly in this scope, before the use.
            if let Some(tr) = self.scan_locals(&s, name, use_start) {
                return Some(tr);
            }
            scope = s.parent();
        }
        None
    }

    /// Scan direct children of `scope` for `local_variable_declaration`s of `name`
    /// that start before `use_start`.
    fn scan_locals(&self, scope: &Node, name: &str, use_start: usize) -> Option<TypeRef> {
        let mut cw = scope.walk();
        let mut found: Option<TypeRef> = None;
        for c in scope.named_children(&mut cw) {
            if c.start_byte() >= use_start {
                break;
            }
            if c.kind() == "local_variable_declaration" {
                let ty = c.child_by_field_name("type").and_then(|n| node_text(&n, self.bytes));
                let mut dw = c.walk();
                for d in c.named_children(&mut dw) {
                    if d.kind() == "variable_declarator" {
                        if let Some(vn) =
                            d.child_by_field_name("name").and_then(|n| node_text(&n, self.bytes))
                        {
                            if vn == name {
                                if let Some(t) = &ty {
                                    if t == "var" {
                                        // `var x = ...`: infer from the initializer.
                                        if let Some(init) = d.child_by_field_name("value") {
                                            found = self.infer_expr(&init, None);
                                        }
                                    } else {
                                        found = self.resolve_type_text(t);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        found
    }

    // ---- type text -> TypeRef ----

    /// Resolve a written type text (`Map<String,Object>`, `HttpServletRequest`) to a
    /// `TypeRef` with binary names, using imports + the resolver.
    fn resolve_type_text(&self, text: &str) -> Option<TypeRef> {
        let parsed = parse_type_text(text)?;
        Some(self.to_binary_ref(&parsed))
    }

    /// Map a parsed (simple-name) type tree to binary names via imports/resolver.
    fn to_binary_ref(&self, t: &SimpleTypeRef) -> TypeRef {
        TypeRef {
            binary_name: self.simple_to_binary(&t.name),
            type_args: t.args.iter().map(|a| self.to_binary_ref(a)).collect(),
        }
    }

    /// Resolve a simple type name to a binary name: dotted → slashed, imports, then
    /// the resolver (project types / star imports / java.lang), with a java.lang
    /// fallback for the common bare names.
    fn simple_to_binary(&self, simple: &str) -> String {
        if simple.contains('.') {
            return simple.replace('.', "/");
        }
        for imp in &self.symbols.imports {
            if imp.simple_name() == Some(simple) {
                return imp.path.replace('.', "/");
            }
        }
        // A type declared in THIS file (or a nested type of it) is authoritative: its FQN
        // comes straight off the extracted symbols, so a local of a same-file type resolves
        // even when the resolver's simple→binary hints weren't seeded for it yet (e.g. a
        // freshly-added type before the next full reindex).
        if let Some(td) = self.symbols.types.iter().find(|t| t.name == simple) {
            return td.fqn.replace('.', "/");
        }
        if let Some(bn) = self.resolver.resolve_simple_name(simple, &self.symbols.imports) {
            return bn;
        }
        match simple {
            "String" | "Object" | "Integer" | "Long" | "Boolean" | "Double" | "Float"
            | "Character" | "Byte" | "Short" | "Number" | "CharSequence" | "Iterable"
            | "Comparable" | "Runnable" | "Thread" | "Class" | "Exception" | "Throwable" => {
                format!("java/lang/{simple}")
            }
            _ => simple.to_string(),
        }
    }
}

/// Find the receiver expression whose end sits at the `.` immediately left of the
/// caret. Among all expression nodes ending there we keep the LARGEST span (prefer
/// the whole `a.b()` over its inner `a`).
fn find_receiver<'t>(root: &Node<'t>, byte_offset: usize) -> Option<Node<'t>> {
    // `byte_offset` sits on (the start of) the member NAME in `receiver.member`. Resolve the
    // receiver through the CST `object`/`field` structure — NOT by assuming the receiver ends
    // right before the `.`. Real code (and reformatters) split the call onto its own line
    //     stepper
    //         .add_step(...)
    // so the receiver ends well before the dot; offset math got `None` there and go-to /
    // find-usages / completion silently failed. Climb from the point node to the enclosing
    // member access whose name/field contains the caret, and take its object. O(tree depth).
    let node = root.named_descendant_for_byte_range(byte_offset, byte_offset)?;
    let mut cur = Some(node);
    while let Some(n) = cur {
        match n.kind() {
            "method_invocation" => {
                if let Some(name) = n.child_by_field_name("name") {
                    if name.start_byte() <= byte_offset && byte_offset <= name.end_byte() {
                        return n.child_by_field_name("object");
                    }
                }
            }
            "field_access" => {
                if let Some(field) = n.child_by_field_name("field") {
                    if field.start_byte() <= byte_offset && byte_offset <= field.end_byte() {
                        return n.child_by_field_name("object");
                    }
                }
            }
            _ => {}
        }
        cur = n.parent();
    }
    None
}

/// The FQN of the type enclosing a node (for `this` / field / local resolution).
fn enclosing_type_fqn(node: &Node, bytes: &[u8], symbols: &FileSymbols) -> Option<String> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if matches!(
            n.kind(),
            "class_declaration" | "interface_declaration" | "enum_declaration"
        ) {
            let name = n.child_by_field_name("name").and_then(|x| node_text(&x, bytes))?;
            // Recover the FQN by matching the extracted TypeDecl on simple name.
            if let Some(td) = symbols.types.iter().find(|t| t.name == name) {
                return Some(td.fqn.clone());
            }
            return Some(name);
        }
        cur = n.parent();
    }
    None
}

/// `com.foo.Bar` -> `com/foo/Bar`.
fn to_binary(fqn: &str) -> String {
    fqn.replace('.', "/")
}

/// `com/foo/Bar` -> `com.foo.Bar`.
fn from_binary(bn: &str) -> String {
    bn.replace('/', ".")
}
