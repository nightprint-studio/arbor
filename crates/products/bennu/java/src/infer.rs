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

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use tree_sitter::{Node, Parser};

use crate::seam::{Member, MemberKind, TypeRef, TypeResolver};
use crate::symbols::{node_text, FileSymbols};
use crate::typeparse::{parse_type_text, SimpleTypeRef};

/// How a local variable is typed, captured once when its scope is scanned.
enum LocalTy {
    /// An explicit declared type text (`Foo`, `Map<String,Object>`).
    Declared(String),
    /// `var x = <init>` — infer from the initializer's byte range (re-descended lazily).
    VarInit(usize, usize),
}

/// One local declaration in a scope: where the declaration statement starts + how it's typed.
struct LocalDecl {
    start: usize,
    ty: LocalTy,
}

/// A per-file inference cache. Validation types **thousands** of sites across a dozen checks, and
/// each site's inference walks up scopes resolving locals — without caching that's quadratic in a
/// big method (the 2.8k-line class that took seconds). This memoizes:
///   * receiver-type results by caret offset, and whole-expression results by byte range — so the
///     unknown-member / arity / argument-type / cast checks that all infer the *same* site pay once;
///   * each scope's local declarations, so resolving a local is a map lookup, not a re-scan.
///
/// Create ONE per file and thread `&InferCache` through the checks. Purely a speed-up: every cached
/// value equals what a fresh computation would return (keyed by position over a fixed tree), so
/// results are unchanged.
#[derive(Default)]
pub struct InferCache {
    receiver: RefCell<HashMap<usize, Option<TypeRef>>>,
    expr: RefCell<HashMap<(usize, usize), Option<TypeRef>>>,
    /// scope node id → (name → its declarations, in source order).
    scope_locals: RefCell<HashMap<usize, Rc<HashMap<String, Vec<LocalDecl>>>>>,
    /// `binary "\0" method` → the methods of that name reachable through the type's hierarchy.
    methods: RefCell<HashMap<String, Rc<MethodResolution>>>,
    /// written type text (`ResultSet`, `Map<String,Object>`) → its resolved `TypeRef`. A local's type
    /// is resolved once per file, not once per receiver occurrence: a DAO uses `Connection conn` /
    /// `PreparedStatement stat` / `ResultSet res` at hundreds of call sites, and resolving the same
    /// text through the resolver (imports + `resolve_simple_name`'s project/star-import probing) each
    /// time was pure repeat work.
    type_text: RefCell<HashMap<String, Option<TypeRef>>>,
}

impl InferCache {
    /// A fresh, empty cache for one file.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve — and memoize — every method named `name` reachable from `binary` (the type itself +
    /// its supertypes), together with whether the WHOLE hierarchy was resolvable. The unknown-method,
    /// arity and argument-type checks each need exactly this per call site, and the answer depends only
    /// on `(binary, name)` — so one walk here replaces a separate hierarchy traversal in every check
    /// for every one of the thousands of call sites (a DAO's `stat.setString(..)` called 400× resolves
    /// once, not 400× × 3 checks). Walking a real JDBC interface's deep hierarchy per call was the
    /// dominant cost on a call-heavy legacy class.
    pub fn resolve_methods(
        &self,
        resolver: &dyn TypeResolver,
        binary: &str,
        name: &str,
    ) -> Rc<MethodResolution> {
        let mut key = String::with_capacity(binary.len() + 1 + name.len());
        key.push_str(binary);
        key.push('\0');
        key.push_str(name);
        if let Some(hit) = self.methods.borrow().get(&key) {
            return hit.clone();
        }
        let resolved = Rc::new(walk_methods(resolver, binary, name));
        self.methods.borrow_mut().insert(key, resolved.clone());
        resolved
    }
}

/// The methods named `name` reachable from a type's hierarchy, plus whether that hierarchy was fully
/// known. Returned (memoized) by [`InferCache::resolve_methods`] and consumed by the resolver-backed
/// checks — see its docs.
pub struct MethodResolution {
    /// Every `Member` (kind `Method`) named `name` found in the known hierarchy — the overload set the
    /// arity / argument checks post-process (param counts, checkable signatures). Order is the walk
    /// order; duplicates across override/inherit are kept (callers dedupe as needed).
    pub candidates: Vec<Member>,
    /// True iff every class in the hierarchy resolved (no `members_of` gap and no depth blow-out). A
    /// `false` means the checks must stay conservative — an unknown supertype might declare/overload
    /// the method, so a "does not exist" / "no such arity" assertion would risk a false positive.
    pub complete: bool,
}

/// A real class hierarchy is shallow; exceeding this many visited nodes means we bail CONSERVATIVELY
/// (`complete = false`) rather than loop on a pathological / cyclic graph.
const MAX_HIER_NODES: usize = 256;

/// Collect every `Member` (kind Method) named `name` across `binary` + its supertypes in one walk,
/// tracking whether the whole hierarchy resolved. Shared by [`InferCache::resolve_methods`].
fn walk_methods(resolver: &dyn TypeResolver, binary: &str, name: &str) -> MethodResolution {
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack = vec![binary.to_string()];
    let mut candidates: Vec<Member> = Vec::new();
    let mut complete = true;
    while let Some(bn) = stack.pop() {
        if visited.len() > MAX_HIER_NODES {
            complete = false;
            break;
        }
        if !visited.insert(bn.clone()) {
            continue;
        }
        match resolver.members_of(&bn) {
            None => complete = false, // unknown class → hierarchy incomplete (conservative)
            Some(cm) => {
                for m in &cm.methods {
                    if m.name == name && m.kind == MemberKind::Method {
                        candidates.push(m.clone());
                    }
                }
                if let Some(sc) = &cm.superclass {
                    stack.push(sc.clone());
                }
                stack.extend(cm.interfaces.iter().cloned());
            }
        }
    }
    MethodResolution { candidates, complete }
}

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

/// Infer the static type of the **whole expression** spanning `[start, end)` (an assigned value, a
/// returned value, a cast operand) — not the receiver before a dot. Reuses the same nominal walk as
/// [`infer_receiver_type`]; returns `None` for anything Phase-1 can't type (a literal, an unknown
/// name, a construct outside scope). Re-parses `source`, so callers on a hot path should prefer the
/// tree-reusing internals.
pub fn infer_expression_type(
    source: &str,
    start: usize,
    end: usize,
    resolver: &dyn TypeResolver,
) -> Option<TypeRef> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let symbols = crate::symbols::extract_symbols(source);
    infer_expression_type_at(&tree.root_node(), source, &symbols, start, end, resolver)
}

/// Like [`infer_expression_type`] but reusing an ALREADY-parsed `root` + ALREADY-extracted
/// `symbols` — the hot path for validation, which types many expressions in one file and must not
/// re-parse per site (that made a large file's checks quadratic and pegged the CPU).
pub fn infer_expression_type_at(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    start: usize,
    end: usize,
    resolver: &dyn TypeResolver,
) -> Option<TypeRef> {
    infer_expression_type_cached(root, source, symbols, start, end, resolver, &InferCache::new())
}

/// [`infer_expression_type_at`] backed by a shared per-file [`InferCache`] — the hot path for
/// validation, so the same expression isn't re-inferred by every check and scope-local resolution
/// isn't quadratic.
pub fn infer_expression_type_cached(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    start: usize,
    end: usize,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<TypeRef> {
    if let Some(hit) = cache.expr.borrow().get(&(start, end)) {
        return hit.clone();
    }
    let bytes = source.as_bytes();
    let result = root.named_descendant_for_byte_range(start, end).and_then(|node| {
        let ctx = Ctx { root: *root, bytes, resolver, symbols, cache };
        let enclosing = enclosing_type_fqn(&node, bytes, symbols);
        ctx.infer_expr(&node, enclosing.as_deref())
    });
    cache.expr.borrow_mut().insert((start, end), result.clone());
    result
}

/// Infer the type of an **already-located** node — the caller found it during its own tree walk, so
/// this skips the `descendant_for_byte_range` search that [`infer_expression_type_cached`] /
/// [`infer_receiver_type_cached`] do (that search is O(siblings) per site — the remaining quadratic
/// on a huge flat method). Memoized by the node's byte range in the shared [`InferCache`]. The
/// validation checks, which already hold the receiver / value node, use this.
pub fn infer_node_type_cached(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    node: &Node,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<TypeRef> {
    let key = (node.start_byte(), node.end_byte());
    if let Some(hit) = cache.expr.borrow().get(&key) {
        return hit.clone();
    }
    let bytes = source.as_bytes();
    let ctx = Ctx { root: *root, bytes, resolver, symbols, cache };
    let enclosing = enclosing_type_fqn(node, bytes, symbols);
    let result = ctx.infer_expr(node, enclosing.as_deref());
    cache.expr.borrow_mut().insert(key, result.clone());
    result
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
    infer_receiver_type_cached(root, source, symbols, byte_offset, resolver, &InferCache::new())
}

/// [`infer_receiver_type_at`] backed by a shared per-file [`InferCache`] — the hot path for
/// validation and the reference-index walk, which query the receiver type at every `obj.member` site
/// and must not re-scan scopes per site.
pub fn infer_receiver_type_cached(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    byte_offset: usize,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<TypeRef> {
    if let Some(hit) = cache.receiver.borrow().get(&byte_offset) {
        return hit.clone();
    }
    let bytes = source.as_bytes();
    let result = find_receiver(root, byte_offset).and_then(|receiver| {
        let ctx = Ctx { root: *root, bytes, resolver, symbols, cache };
        let enclosing = enclosing_type_fqn(&receiver, bytes, symbols);
        ctx.infer_expr(&receiver, enclosing.as_deref())
    });
    cache.receiver.borrow_mut().insert(byte_offset, result.clone());
    result
}

/// Shared inference context. `root` is the file's parse tree root (to re-descend for `var`
/// initializers); `cache` memoizes results + per-scope locals.
struct Ctx<'a> {
    root: Node<'a>,
    bytes: &'a [u8],
    resolver: &'a dyn TypeResolver,
    symbols: &'a FileSymbols,
    cache: &'a InferCache,
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
            // Literals — typed just enough for the assignment / argument checks to catch a
            // `String` ↔ primitive mismatch (`int x = "1";`, `foo(1)` where `foo` wants a String).
            "string_literal" | "text_block" => Some(TypeRef::simple("java/lang/String")),
            "character_literal" => Some(TypeRef::simple("char")),
            "true" | "false" => Some(TypeRef::simple("boolean")),
            "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal"
            | "binary_integer_literal" => {
                let t = node_text(node, self.bytes).unwrap_or_default();
                let is_long = t.ends_with('l') || t.ends_with('L');
                Some(TypeRef::simple(if is_long { "long" } else { "int" }))
            }
            "decimal_floating_point_literal" | "hex_floating_point_literal" => {
                let t = node_text(node, self.bytes).unwrap_or_default();
                let is_float = t.ends_with('f') || t.ends_with('F');
                Some(TypeRef::simple(if is_float { "float" } else { "double" }))
            }
            // `null` is the bottom of the reference lattice — assignable to any reference; leave it
            // untyped so no check asserts anything about it.
            "null_literal" => None,
            // Only string concatenation needs typing (`"x" + n` → String); arithmetic stays untyped
            // (the checks skip primitives, so we avoid any widening/promotion guesswork).
            "binary_expression" => self.infer_binary(node, enclosing),
            _ => None,
        }
    }

    /// Type a `+` binary expression as `String` when either operand is a `String` (concatenation);
    /// everything else is left untyped.
    fn infer_binary(&self, node: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        let op = node.child_by_field_name("operator").and_then(|o| node_text(&o, self.bytes));
        if op.as_deref() != Some("+") {
            return None;
        }
        let is_string = |field: &str| {
            node.child_by_field_name(field)
                .and_then(|n| self.infer_expr(&n, enclosing))
                .is_some_and(|t| t.binary_name == "java/lang/String")
        };
        (is_string("left") || is_string("right")).then(|| TypeRef::simple("java/lang/String"))
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
    /// Walks ancestors, checking parameters + each scope's (cached) local declarations for a match
    /// that precedes the use.
    fn resolve_local(&self, use_node: &Node, name: &str) -> Option<TypeRef> {
        let use_start = use_node.start_byte();
        let mut scope = use_node.parent();
        while let Some(s) = scope {
            // method / lambda / constructor parameters. Only these scopes HAVE a `parameters` field;
            // gating here avoids an O(children) `child_by_field_name` scan on every block / if / try /
            // catch ancestor (a huge method body has hundreds of statements, scanned per identifier).
            if matches!(s.kind(), "method_declaration" | "constructor_declaration" | "lambda_expression") {
                if let Some(tr) = self.param_type(&s, name) {
                    return Some(tr);
                }
            }
            // local variable declarations directly in this scope, before the use (last one wins,
            // matching the previous in-order scan).
            let locals = self.scope_locals(&s);
            if let Some(decls) = locals.get(name) {
                if let Some(decl) = decls.iter().rev().find(|d| d.start < use_start) {
                    return self.resolve_local_ty(&decl.ty);
                }
            }
            scope = s.parent();
        }
        None
    }

    /// A parameter of `scope` named `name`, resolved to its declared type (`None` if `scope` has no
    /// parameter list, or none matches). Parameters are few, so this stays a direct scan.
    fn param_type(&self, scope: &Node, name: &str) -> Option<TypeRef> {
        let params = scope.child_by_field_name("parameters")?;
        let mut pw = params.walk();
        for p in params.named_children(&mut pw) {
            if !matches!(p.kind(), "formal_parameter" | "spread_parameter") {
                continue;
            }
            let matches = p
                .child_by_field_name("name")
                .and_then(|n| node_text(&n, self.bytes))
                .is_some_and(|pn| pn == name);
            if matches {
                let t = p.child_by_field_name("type").and_then(|n| node_text(&n, self.bytes))?;
                return self.resolve_type_text(&t);
            }
        }
        None
    }

    /// The local declarations directly in `scope` (name → its declarations in source order), built
    /// once and cached by scope node id. This is what makes local resolution non-quadratic: without
    /// it, every identifier use re-scanned the whole enclosing scope.
    fn scope_locals(&self, scope: &Node) -> Rc<HashMap<String, Vec<LocalDecl>>> {
        let id = scope.id();
        if let Some(m) = self.cache.scope_locals.borrow().get(&id) {
            return m.clone();
        }
        let mut map: HashMap<String, Vec<LocalDecl>> = HashMap::new();
        let mut cw = scope.walk();
        for c in scope.named_children(&mut cw) {
            if c.kind() != "local_variable_declaration" {
                continue;
            }
            let type_text = c.child_by_field_name("type").and_then(|n| node_text(&n, self.bytes));
            let start = c.start_byte();
            let mut dw = c.walk();
            for d in c.named_children(&mut dw) {
                if d.kind() != "variable_declarator" {
                    continue;
                }
                let Some(vn) = d.child_by_field_name("name").and_then(|n| node_text(&n, self.bytes))
                else {
                    continue;
                };
                let ty = match type_text.as_deref() {
                    Some("var") => match d.child_by_field_name("value") {
                        Some(init) => LocalTy::VarInit(init.start_byte(), init.end_byte()),
                        None => continue,
                    },
                    Some(t) => LocalTy::Declared(t.to_string()),
                    None => continue,
                };
                map.entry(vn).or_default().push(LocalDecl { start, ty });
            }
        }
        let rc = Rc::new(map);
        self.cache.scope_locals.borrow_mut().insert(id, rc.clone());
        rc
    }

    /// Resolve a cached local's type — a declared type text, or a `var` initializer re-descended
    /// from the tree by its byte range.
    fn resolve_local_ty(&self, ty: &LocalTy) -> Option<TypeRef> {
        match ty {
            LocalTy::Declared(t) => self.resolve_type_text(t),
            LocalTy::VarInit(start, end) => {
                let init = self.root.named_descendant_for_byte_range(*start, *end)?;
                self.infer_expr(&init, None)
            }
        }
    }

    // ---- type text -> TypeRef ----

    /// Resolve a written type text (`Map<String,Object>`, `HttpServletRequest`) to a
    /// `TypeRef` with binary names, using imports + the resolver.
    fn resolve_type_text(&self, text: &str) -> Option<TypeRef> {
        if let Some(hit) = self.cache.type_text.borrow().get(text) {
            return hit.clone();
        }
        let result = parse_type_text(text).map(|parsed| self.to_binary_ref(&parsed));
        self.cache.type_text.borrow_mut().insert(text.to_string(), result.clone());
        result
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
