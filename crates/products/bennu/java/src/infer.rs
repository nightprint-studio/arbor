//! Local type inference: [`infer_receiver_type`]`(source, byte_offset, resolver)`.
//!
//! Phase-1 scope (Spike B — nominal walks, NOT flow-typing):
//!   * local variable declared types (incl. `Foo x = ...`), method parameters
//!   * `this` and `this.field` field types
//!   * method-return-type chaining (`a.getB().getC()`)
//!   * simple generics carry-through (`List<Foo>` -> `.get(i)` / `.iterator().next()`
//!     element = `Foo`)
//!
//! Overload selection is arity-first (JLS §15.12.2 in miniature): among the same-named overloads on
//! the receiver's hierarchy we keep those whose arity admits the call, and take their return type only
//! when it is UNIQUE — narrowing a return-type tie by a conservative primitive/reference argument
//! check, and yielding "unknown" rather than *guessing* an ambiguous overload (a wrong return type
//! would mistype the expression and could surface a false diagnostic downstream).
//!
//! Explicitly NOT handled (documented in the crate README): full argument-subtype overload resolution
//! (boxing/varargs/most-specific), flow-typing / reassignment, conditional/ternary narrowing, raw-array
//! element inference, static member access on bare type names, wildcard/bound modelling.

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

/// Classification of a bare identifier against the enclosing lambda scopes: it's a lambda parameter
/// we could TARGET-TYPE, a lambda parameter we couldn't type (but that still SHADOWS a same-named
/// field, so the caller must not read the field), or not a lambda parameter at all.
enum LambdaParam {
    Typed(TypeRef),
    Untyped,
    NotParam,
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
            // `Foo.class` / `int.class` — a class literal is a `java.lang.Class`, so `Foo.class.getName()`
            // and completion after `Foo.class.` resolve. (The `<Foo>` argument isn't tracked; Class's
            // common methods don't need it.)
            "class_literal" => Some(TypeRef::simple("java/lang/Class")),
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

    /// A bare identifier: resolve as local var / parameter first (walking up scopes), then as an
    /// (untyped) lambda parameter — which SHADOWS a same-named field, so it's checked BEFORE the
    /// field — then as a field of the enclosing type.
    fn infer_identifier(&self, node: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        let name = node_text(node, self.bytes)?;

        if let Some(tr) = self.resolve_local(node, &name) {
            return Some(tr);
        }
        // An untyped lambda parameter (`x -> x.foo()`, `(a, b) -> …`) shadows a field of the same
        // name. Recognise it — and TARGET-TYPE it from the lambda's functional interface when we can
        // — before the field fallback, so `result` in `foo(result -> result.bar())` is never
        // mis-resolved to a field named `result` (which surfaced as a false "cannot resolve method").
        match self.lambda_param(node, &name, enclosing) {
            LambdaParam::Typed(tr) => return Some(tr),
            // A lambda param we couldn't type: leave it unresolved (so its uses are skipped), but do
            // NOT fall through to the field — the param shadows it.
            LambdaParam::Untyped => return None,
            LambdaParam::NotParam => {}
        }
        if let Some(fqn) = enclosing {
            if let Some(tr) = self.field_type_of_source_type(fqn, &name) {
                return Some(tr);
            }
        }
        // A statically-imported FIELD (`import static X.PI;` / `import static X.*;`) used bare.
        self.static_import_field(&name)
    }

    /// The type of a statically-imported FIELD named `name` (a bare `PI` value reference), or `None`.
    /// Looks the field up on each `import static` owner whose specific member matches (or any wildcard
    /// owner), walking the owner's hierarchy like any field access.
    fn static_import_field(&self, name: &str) -> Option<TypeRef> {
        for t in crate::static_import::static_import_targets(&self.symbols.imports) {
            if t.member.as_deref().map_or(true, |m| m == name) {
                let owner = TypeRef::simple(t.owner_binary);
                if let Some(tr) = self.field_type_on(&owner, name) {
                    return Some(tr);
                }
            }
        }
        None
    }

    /// The return type of a statically-imported static METHOD `name` (a bare `max(…)` call), or `None`.
    fn static_import_method(
        &self,
        name: &str,
        args: &[Node],
        enclosing: Option<&str>,
    ) -> Option<TypeRef> {
        for t in crate::static_import::static_import_targets(&self.symbols.imports) {
            if t.member.as_deref().map_or(true, |m| m == name) {
                let owner = TypeRef::simple(t.owner_binary);
                if let Some(tr) = self.method_return_on(&owner, name, args, enclosing) {
                    return Some(tr);
                }
            }
        }
        None
    }

    /// Classify `name` against the lambda scopes enclosing `use_node`. If it's a parameter of an
    /// enclosing lambda, try to TARGET-TYPE it: find the functional interface the lambda is passed to
    /// (its argument position in a call / construction), take that interface's single abstract method,
    /// and read the parameter at the lambda-param index (with the interface's generics substituted).
    /// Conservative throughout — any ambiguity (overloaded call, multi-abstract-method interface,
    /// unknown type) yields [`LambdaParam::Untyped`] (shadow the field, but leave the type unknown so
    /// nothing is falsely flagged), never a guess.
    fn lambda_param(&self, use_node: &Node, name: &str, enclosing: Option<&str>) -> LambdaParam {
        let mut scope = use_node.parent();
        while let Some(s) = scope {
            if s.kind() == "lambda_expression" {
                if let Some(params) = s.child_by_field_name("parameters") {
                    let names = self.lambda_param_names(&params);
                    if let Some(idx) = names.iter().position(|n| n == name) {
                        let typed = self
                            .lambda_target_type(&s, enclosing)
                            .and_then(|fi| self.sam_param_type(&fi, idx));
                        return match typed {
                            // Never surface a bare type-variable as a resolved type (it would just be
                            // an unresolvable receiver) — treat it as untyped.
                            Some(ty) if !is_type_var(&ty.binary_name) => LambdaParam::Typed(ty),
                            _ => LambdaParam::Untyped,
                        };
                    }
                }
            }
            // A lambda parameter can't be declared above its method/constructor — stop climbing.
            if matches!(s.kind(), "method_declaration" | "constructor_declaration") {
                break;
            }
            scope = s.parent();
        }
        LambdaParam::NotParam
    }

    /// The parameter NAMES of a lambda's `parameters` node, in order — across the three shapes:
    /// a single bare `identifier` (`x -> …`), `inferred_parameters` (`(a, b) -> …`), and typed
    /// `formal_parameters` (`(Foo a) -> …`).
    fn lambda_param_names(&self, params: &Node) -> Vec<String> {
        match params.kind() {
            "identifier" => node_text(params, self.bytes).into_iter().collect(),
            "inferred_parameters" => {
                let mut out = Vec::new();
                let mut c = params.walk();
                for ch in params.named_children(&mut c) {
                    if ch.kind() == "identifier" {
                        if let Some(t) = node_text(&ch, self.bytes) {
                            out.push(t);
                        }
                    }
                }
                out
            }
            "formal_parameters" => {
                let mut out = Vec::new();
                let mut c = params.walk();
                for ch in params.named_children(&mut c) {
                    if matches!(ch.kind(), "formal_parameter" | "spread_parameter") {
                        if let Some(n) =
                            ch.child_by_field_name("name").and_then(|n| node_text(&n, self.bytes))
                        {
                            out.push(n);
                        }
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }

    /// The functional-interface type a lambda is assigned to, when it is passed as an argument to a
    /// method call or a constructor (the common case, and the only one modelled): the type of the
    /// parameter at the lambda's argument position. `None` for any other position (declared variable,
    /// return, cast) or when the callee / position is ambiguous.
    fn lambda_target_type(&self, lambda: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        let arg_list = lambda.parent()?;
        if arg_list.kind() != "argument_list" {
            return None;
        }
        let call = arg_list.parent()?;
        // The lambda's index among the REAL arguments (comments are named children — skip them).
        let mut idx = 0usize;
        let mut found = false;
        let mut c = arg_list.walk();
        for a in arg_list.named_children(&mut c) {
            if matches!(a.kind(), "line_comment" | "block_comment") {
                continue;
            }
            if a.id() == lambda.id() {
                found = true;
                break;
            }
            idx += 1;
        }
        if !found {
            return None;
        }
        match call.kind() {
            "method_invocation" => {
                let name =
                    call.child_by_field_name("name").and_then(|n| node_text(&n, self.bytes))?;
                let recv = match call.child_by_field_name("object") {
                    Some(obj) => self.infer_expr(&obj, enclosing)?,
                    None => TypeRef::simple(to_binary(enclosing?)),
                };
                self.param_at(&recv, &name, idx)
            }
            "object_creation_expression" => {
                let ty = call.child_by_field_name("type")?;
                let text = node_text(&ty, self.bytes)?;
                let recv = self.resolve_type_text(&text)?;
                self.param_at(&recv, "<init>", idx)
            }
            _ => None,
        }
    }

    /// The type of parameter `idx` of method `name` on `recv`, walking supertypes — but ONLY when
    /// every overload of that name agrees on that parameter's type (so an ambiguous overloaded call
    /// yields `None`, never a guess). The receiver's generics are then substituted, so
    /// `List<Foo>.forEach(Consumer<? super E>)` yields `Consumer<Foo>`.
    fn param_at(&self, recv: &TypeRef, name: &str, idx: usize) -> Option<TypeRef> {
        let mut types: Vec<TypeRef> = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = vec![recv.binary_name.clone()];
        while let Some(bn) = stack.pop() {
            if !visited.insert(bn.clone()) {
                continue;
            }
            // An unknown class in the hierarchy could hide a differing overload → give up (unknown,
            // not a guess).
            let cm = self.resolver.members_of(&bn)?;
            for m in &cm.methods {
                if m.kind == MemberKind::Method && m.name == name {
                    types.push(m.params.get(idx)?.clone());
                }
            }
            if let Some(sc) = cm.superclass.clone() {
                stack.push(sc);
            }
            stack.extend(cm.interfaces.iter().cloned());
        }
        // Require a single distinct parameter type across all overloads.
        let mut uniq: Vec<TypeRef> = Vec::new();
        for t in &types {
            if !uniq.contains(t) {
                uniq.push(t.clone());
            }
        }
        let [fi] = uniq.as_slice() else { return None };
        Some(self.substitute_generics(fi, recv))
    }

    /// The type of parameter `idx` of a functional interface's SINGLE abstract method (its SAM),
    /// with the interface's generics substituted (`Consumer<Foo>` → its `accept(T)` param → `Foo`).
    /// `None` when the interface's hierarchy isn't fully known, or it doesn't have exactly one
    /// abstract instance method (so we never mistype against a non-functional interface).
    fn sam_param_type(&self, fi: &TypeRef, idx: usize) -> Option<TypeRef> {
        let mut abstracts: Vec<Member> = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = vec![fi.binary_name.clone()];
        while let Some(bn) = stack.pop() {
            if !visited.insert(bn.clone()) {
                continue;
            }
            let cm = self.resolver.members_of(&bn)?; // incomplete hierarchy → give up
            for m in &cm.methods {
                if m.kind == MemberKind::Method
                    && m.is_abstract
                    && !m.is_default
                    && !m.is_static
                    && m.name != "<init>"
                {
                    abstracts.push(m.clone());
                }
            }
            if let Some(sc) = cm.superclass.clone() {
                stack.push(sc);
            }
            stack.extend(cm.interfaces.iter().cloned());
        }
        // Dedup an override that appears at multiple hierarchy levels, then require EXACTLY one
        // abstract method — the SAM. (A precise Object-method carve-out isn't modelled; more than one
        // → give up rather than guess.)
        let mut uniq: Vec<&Member> = Vec::new();
        for m in &abstracts {
            if !uniq.iter().any(|u| u.name == m.name && u.params == m.params) {
                uniq.push(m);
            }
        }
        let [sam] = uniq.as_slice() else { return None };
        let pty = sam.params.get(idx)?;
        Some(self.substitute_generics(pty, fi))
    }

    /// `a.b`: infer `a`, then look up field `b` on it. Handles `this.b`.
    fn infer_field_access(&self, node: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        let obj = node.child_by_field_name("object")?;
        let field = node.child_by_field_name("field")?;
        let field_name = node_text(&field, self.bytes)?;

        let obj_type = self.infer_expr(&obj, enclosing)?;
        self.field_type_on(&obj_type, &field_name)
    }

    /// `recv.foo(args)` or bare `foo(args)`. Resolves `foo`'s return type (arity-aware overload
    /// selection), applying generics carry-through from the receiver.
    fn infer_method_invocation(&self, node: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        let name =
            node.child_by_field_name("name").and_then(|n| node_text(&n, self.bytes))?;
        let args = self.call_arg_nodes(node);

        let recv_type = match node.child_by_field_name("object") {
            Some(obj) => self.infer_expr(&obj, enclosing)?,
            // Bare call `foo()` → the enclosing type's method, else a statically-imported static method
            // (`import static X.max;` → `max(…)`). We try the enclosing type first (an instance/own
            // method wins over an import, as in Java), then fall back to the static import.
            None => {
                if let Some(fqn) = enclosing {
                    if let Some(tr) = self.method_return_on(
                        &TypeRef::simple(to_binary(fqn)),
                        &name,
                        &args,
                        enclosing,
                    ) {
                        return Some(tr);
                    }
                }
                return self.static_import_method(&name, &args, enclosing);
            }
        };

        self.method_return_on(&recv_type, &name, &args, enclosing)
    }

    /// The real argument nodes of a call — the `arguments` (`argument_list`) named children, skipping
    /// comments (which ARE named children in tree-sitter). Their count + inferred types drive
    /// arity/argument overload selection.
    fn call_arg_nodes<'t>(&self, call: &Node<'t>) -> Vec<Node<'t>> {
        let Some(list) = call.child_by_field_name("arguments") else { return Vec::new() };
        let mut out = Vec::new();
        let mut c = list.walk();
        for a in list.named_children(&mut c) {
            if matches!(a.kind(), "line_comment" | "block_comment") {
                continue;
            }
            out.push(a);
        }
        out
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

    /// Return type of `method_name` on a resolved type, applying generic substitution from `recv` (so
    /// `List<Foo>.get` → `Foo`). Overload-aware: the arity (and, on a return-type tie, the argument
    /// types) of `args` select which same-named overload the call binds to; an unresolvable overload
    /// yields `None` rather than a guess.
    fn method_return_on(
        &self,
        recv: &TypeRef,
        method_name: &str,
        args: &[Node],
        enclosing: Option<&str>,
    ) -> Option<TypeRef> {
        if let Some(tr) = self.method_return_of_source_type(
            &from_binary(&recv.binary_name),
            method_name,
            args,
        ) {
            return Some(tr);
        }
        let res = self.cache.resolve_methods(self.resolver, &recv.binary_name, method_name);
        let ret = self.select_overload_return(&res.candidates, args, enclosing)?;
        Some(self.substitute_generics(&ret, recv))
    }

    /// Pick the return type of the overload a call of `args` binds to, from all same-named `candidates`
    /// on the receiver's hierarchy — JLS §15.12.2 in miniature, deliberately conservative:
    ///   1. keep candidates whose ARITY admits the call (a trailing array/varargs param admits 0+ extra);
    ///   2. if those all agree on a return type → use it (the common case, and what fixes a 1-arg
    ///      `df.format(date)` → `String` that the old first-by-name pick mis-resolved to a 3-arg
    ///      `Format.format(…)` → `StringBuffer`);
    ///   3. otherwise narrow the tie by argument types, rejecting only a DEFINITE primitive/reference
    ///      clash, and use the return type iff it is now unique;
    ///   4. still not unique → `None`. An ambiguous overload is never guessed: a wrong return type
    ///      mistypes the expression and risks a false "cannot resolve member" / assignment diagnostic.
    fn select_overload_return(
        &self,
        candidates: &[Member],
        args: &[Node],
        enclosing: Option<&str>,
    ) -> Option<TypeRef> {
        // Collapse OVERRIDE chains: the same method reachable at several hierarchy levels (a plain
        // inherit, or a COVARIANT override where the derived return type is more specific) arrives as
        // several candidates with identical parameter signatures. Keep the most-derived occurrence —
        // `resolve_methods` visits the receiver's own members first — so a covariant override reads as
        // its derived return type, not as an "ambiguous overload". Only genuinely distinct signatures
        // survive as real overloads to disambiguate.
        let mut distinct: Vec<&Member> = Vec::new();
        for m in candidates {
            if !distinct.iter().any(|d| d.params == m.params) {
                distinct.push(m);
            }
        }
        // A single method of this name is NOT an overload — trust it whatever the arity. This keeps the
        // (correct) single-method behavior identical to the old first-by-name resolution and avoids
        // second-guessing an imperfect param model; only genuine overload sets go through arity/argument
        // disambiguation below.
        if let [only] = distinct.as_slice() {
            return Some(only.return_type.clone());
        }
        let argc = args.len();
        let arity_ok: Vec<&Member> = distinct
            .into_iter()
            .filter(|m| arity_admits(m.params.len(), last_is_array(m), argc))
            .collect();
        if let Some(ret) = unique_return(&arity_ok) {
            return Some(ret);
        }
        if arity_ok.is_empty() {
            return None;
        }
        // Return types disagree → narrow by argument types (each inferred once; unknown args abstain).
        let arg_types: Vec<Option<TypeRef>> =
            args.iter().map(|a| self.infer_expr(a, enclosing)).collect();
        let applicable: Vec<&Member> =
            arity_ok.into_iter().filter(|m| args_admissible(&m.params, &arg_types)).collect();
        unique_return(&applicable)
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

    /// Substitute type variables in `member_ret` with the receiver's actual type arguments: a
    /// type-variable name maps to a receiver type-arg by POSITION; nested type args recurse one hop
    /// (so `List<Foo>`'s `iterator() -> Iterator<E>` becomes `Iterator<Foo>`). Deliberately shallow.
    ///
    /// The position comes from the receiver class's REAL declared type-parameter list
    /// (`ClassMembers::type_params`, from source `<…>` or the bytecode signature) — exact for any
    /// naming (`Pair<X,Y>.getRight() -> Y`). Only when that list is unavailable do we fall back to the
    /// naming-convention heuristic ([`type_var_index`]): a 1-arg generic's sole variable, `Map<K,V>`
    /// value, `Pair<L,R>`/`Entry<K,V>` right, a numeric `T1`/`T2` suffix. An unrecognised variable on
    /// a 2+-arg generic with no known type-param list is left UNRESOLVED rather than guessed — a wrong
    /// guess would mis-type the value and could surface a false "cannot resolve method" downstream.
    fn substitute_generics(&self, member_ret: &TypeRef, recv: &TypeRef) -> TypeRef {
        if recv.type_args.is_empty() {
            return member_ret.clone();
        }
        let bn = &member_ret.binary_name;
        if is_type_var(bn) {
            // Position of `bn` in the receiver CLASS's real declared type-parameter list (from source
            // `<…>` or the bytecode signature).
            let cm = self.resolver.members_of(&recv.binary_name);
            let type_params_known = cm.as_ref().is_some_and(|c| !c.type_params.is_empty());
            let declared_idx = cm.and_then(|c| c.type_params.iter().position(|p| p == bn));
            let idx = if type_params_known {
                // The receiver class's real `<…>` list is known. A variable IN it maps to the actual
                // type argument by position (`Pair<X,Y>.getRight() -> Y`, exact for any naming). A
                // variable NOT in it is a METHOD-level type variable — `Stream.map`'s `<R>`,
                // `Stream.collect`'s `<R>`, `Optional.map`'s `<U>` — whose binding comes from the
                // CALL's arguments, which Phase-1 doesn't model. The old code mapped it to a receiver
                // type argument via the single-arg heuristic, which was WRONG: it inferred
                // `stream.collect(toList())` as the stream's element type instead of `List`, then
                // falsely flagged `.indexOf(…)` as missing. Leave such a variable UNRESOLVED.
                declared_idx
            } else {
                // No declared list (a generic library type whose signature we couldn't decode) — the
                // naming-convention heuristic is the best guess available.
                type_var_index(bn, recv.type_args.len())
            };
            return match idx {
                Some(i) => recv.type_args.get(i).cloned().unwrap_or_else(|| member_ret.clone()),
                // Unresolved (method-level var, or position unknown on a heuristic miss).
                None => member_ret.clone(),
            };
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

    /// Look up a method return on a source-declared type by its FQN (or bare name) — arity-aware over
    /// the same-named overloads (a trailing `T...`/`T[]` param admits 0+ trailing args). Returns the
    /// return type only when the arity-admitting overloads AGREE on it; an ambiguous project overload
    /// yields `None` (never the first-declared one).
    fn method_return_of_source_type(
        &self,
        fqn: &str,
        method_name: &str,
        args: &[Node],
    ) -> Option<TypeRef> {
        let td = self.symbols.types.iter().find(|t| t.fqn == fqn || t.name == fqn)?;
        let same_named: Vec<&crate::symbols::MethodDecl> =
            td.methods.iter().filter(|m| m.name == method_name).collect();
        // A single method of this name isn't an overload → trust it (behavior identical to before).
        if let [only] = same_named.as_slice() {
            return self.resolve_type_text(&only.return_type_text);
        }
        let argc = args.len();
        let mut ret_text: Option<&str> = None;
        for md in same_named.iter().copied() {
            let last_array = md
                .params
                .last()
                .is_some_and(|p| {
                    let t = p.type_text.trim_end();
                    t.ends_with("...") || t.ends_with("[]")
                });
            if !arity_admits(md.params.len(), last_array, argc) {
                continue;
            }
            match ret_text {
                None => ret_text = Some(md.return_type_text.as_str()),
                Some(t) if t == md.return_type_text.as_str() => {}
                Some(_) => return None, // arity-admitting overloads disagree on return → don't guess
            }
        }
        self.resolve_type_text(ret_text?)
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
            match c.kind() {
                "local_variable_declaration" => {
                    let type_text =
                        c.child_by_field_name("type").and_then(|n| node_text(&n, self.bytes));
                    let start = c.start_byte();
                    let mut dw = c.walk();
                    for d in c.named_children(&mut dw) {
                        if d.kind() != "variable_declarator" {
                            continue;
                        }
                        let name =
                            d.child_by_field_name("name").and_then(|n| node_text(&n, self.bytes));
                        let value = d.child_by_field_name("value");
                        if let (Some(vn), Some(ty)) = (name, self.local_ty_of(type_text.as_deref(), value)) {
                            map.entry(vn).or_default().push(LocalDecl { start, ty });
                        }
                    }
                }
                // Try-with-resources: `try (var in = open(); Foo f = ...) { … }`. Each `resource` is a
                // local visible in the try body, with `type`/`name`/`value` like a declarator (and
                // `var`/`val` infer from the initializer). A bare `try (existing) {}` resource is just an
                // identifier / field_access with no `type` → skipped by `local_ty_of`.
                "resource_specification" => {
                    let mut rw = c.walk();
                    for r in c.named_children(&mut rw) {
                        if r.kind() != "resource" {
                            continue;
                        }
                        let type_text =
                            r.child_by_field_name("type").and_then(|n| node_text(&n, self.bytes));
                        let name =
                            r.child_by_field_name("name").and_then(|n| node_text(&n, self.bytes));
                        let value = r.child_by_field_name("value");
                        if let (Some(vn), Some(ty)) = (name, self.local_ty_of(type_text.as_deref(), value)) {
                            map.entry(vn).or_default().push(LocalDecl { start: r.start_byte(), ty });
                        }
                    }
                }
                _ => {}
            }
        }
        let rc = Rc::new(map);
        self.cache.scope_locals.borrow_mut().insert(id, rc.clone());
        rc
    }

    /// Classify a local/resource declaration's type from its written `type_text` and optional
    /// initializer `value`. `var` (Java 10+) and Lombok `val` (a `final var`) both infer from the
    /// initializer — so `val x = repo.find();` / `try (var in = open())` resolve `x`/`in`'s members
    /// (without this, `val` would resolve as a phantom type named `val` and every `x.method()` would be
    /// falsely flagged). A written type is taken as-is; a missing type/initializer yields `None` (skip).
    fn local_ty_of(&self, type_text: Option<&str>, value: Option<Node>) -> Option<LocalTy> {
        match type_text {
            Some("var") | Some("val") => {
                value.map(|init| LocalTy::VarInit(init.start_byte(), init.end_byte()))
            }
            Some(t) => Some(LocalTy::Declared(t.to_string())),
            None => None,
        }
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
        // A type in the file's OWN package is in scope without an import. Resolve it to its exact
        // binary (`com/acme/C`, a unique key) BEFORE the resolver's flat simple-name lookup, which
        // collapses same-simple-name types across packages and could otherwise pick the wrong `C`
        // (mis-typing every `C` receiver in the file, and downstream falsely flagging its members).
        if let Some(pkg) = self.symbols.package.as_deref() {
            if !pkg.is_empty() {
                let candidate = format!("{}/{}", pkg.replace('.', "/"), simple);
                if self.resolver.members_of(&candidate).is_some() {
                    return candidate;
                }
            }
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

/// Whether a binary name is really a **type variable** (`E`, `T`, `K`, `V`, `R`, or a numeric-suffixed
/// `T1`/`T2`) rather than a class: a single leading uppercase letter optionally followed by digits,
/// no package separator. A real class always carries a lowercase package segment or a mixed-case /
/// multi-letter simple name, so this never misfires on `String`/`Foo`. Kept to ONE letter (plus
/// digits) to avoid classifying a rare 2-letter default-package class as a variable.
fn is_type_var(bn: &str) -> bool {
    let mut chars = bn.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_digit())
}

// ---- overload selection helpers ----

/// Whether an overload of `nparams` (its last parameter being an array → possibly varargs) can bind a
/// call of `argc` arguments: an exact arity match, or — for a trailing array/varargs parameter — any
/// count from `nparams - 1` up (0+ variadic arguments). The seam carries no explicit varargs flag, so
/// a trailing `T[]` is treated as varargs-capable; the extra matches this admits are harmless (a real
/// call to a fixed `T[]` param supplies exactly one argument, matching the exact arm anyway).
fn arity_admits(nparams: usize, last_is_array: bool, argc: usize) -> bool {
    argc == nparams || (last_is_array && argc + 1 >= nparams)
}

/// Whether a member's last parameter is an array type (`T[]`) — the resolved shape of a `T...` varargs
/// parameter (no explicit flag on the seam).
fn last_is_array(m: &Member) -> bool {
    m.params.last().is_some_and(|p| p.binary_name.ends_with("[]"))
}

/// The single return type shared by every member in `members`, or `None` when the set is empty or its
/// members return more than one distinct type — the "unique return" gate that keeps an ambiguous
/// overload UNRESOLVED (never guessed) while resolving the common single-return case.
fn unique_return(members: &[&Member]) -> Option<TypeRef> {
    let mut ret: Option<&TypeRef> = None;
    for m in members {
        match ret {
            None => ret = Some(&m.return_type),
            Some(r) if r == &m.return_type => {}
            Some(_) => return None,
        }
    }
    ret.cloned()
}

/// Whether a fixed-arity overload's `params` could accept arguments of `arg_types` — used ONLY to
/// break a return-type tie. Conservative: rejects a candidate only on a DEFINITE primitive/reference
/// clash (an `int` param can't take a `String` argument, nor vice versa). An unknown argument, a type
/// variable, or an arity/varargs slack never rejects — we keep the candidate rather than risk dropping
/// the real one and mistyping the call.
fn args_admissible(params: &[TypeRef], arg_types: &[Option<TypeRef>]) -> bool {
    if params.len() != arg_types.len() {
        return true; // varargs / arity slack → no argument verdict
    }
    for (p, a) in params.iter().zip(arg_types) {
        let Some(a) = a else { continue }; // argument type unknown → abstain
        if is_type_var(&p.binary_name) {
            continue; // a generic parameter accepts anything
        }
        if primitive_ref_clash(&p.binary_name, &a.binary_name) {
            return false;
        }
    }
    true
}

/// Whether a parameter and argument sit on OPPOSITE sides of the primitive/reference divide with no
/// autoboxing bridge — a definite non-match (`int` param vs `String` arg; `String` param vs `int`
/// arg). A primitive paired with its own wrapper (`int`/`Integer`) is NOT a clash. Two primitives or
/// two references are left undecided here (returns `false`).
fn primitive_ref_clash(param: &str, arg: &str) -> bool {
    if is_primitive(param) == is_primitive(arg) {
        return false; // same side of the divide → not a primitive/reference clash
    }
    !boxes(param, arg) && !boxes(arg, param)
}

/// A JVM primitive binary name.
fn is_primitive(bn: &str) -> bool {
    matches!(
        bn,
        "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double" | "void"
    )
}

/// Whether primitive `a` autoboxes to reference wrapper `b` (`int` → `java/lang/Integer`).
fn boxes(a: &str, b: &str) -> bool {
    let wrapper = match a {
        "int" => "java/lang/Integer",
        "long" => "java/lang/Long",
        "short" => "java/lang/Short",
        "byte" => "java/lang/Byte",
        "char" => "java/lang/Character",
        "boolean" => "java/lang/Boolean",
        "float" => "java/lang/Float",
        "double" => "java/lang/Double",
        _ => return false,
    };
    b == wrapper
}

/// The receiver type-arg index a type variable named `name` maps to, given a generic of `arity`
/// arguments — or `None` when the position can't be inferred conventionally (so the caller leaves it
/// unresolved instead of guessing). Heuristic, because the class's real type-parameter list isn't on
/// the seam:
///   * a **1-arg** generic has exactly one variable → index 0, whatever it's named (`List<E>`,
///     `Optional<T>`, `Future<V>`, `Supplier<S>`);
///   * a **numeric suffix** encodes the position directly (`T1`→0, `T2`→1, `K2`→1);
///   * otherwise a small table of conventional role names: value/right/second `V`/`R`/`U`/`B`/`S`→1,
///     key/element/left/first `K`/`T`/`E`/`A`/`L`/`N`/`O`→0 (`Map<K,V>`, `Pair<L,R>`, `Entry<K,V>`,
///     `BiFunction`'s `T`/`U`). An unrecognised name on a 2+-arg generic yields `None`.
fn type_var_index(name: &str, arity: usize) -> Option<usize> {
    if arity == 0 {
        return None;
    }
    // A single type argument: the only variable is that argument, regardless of its letter.
    if arity == 1 {
        return Some(0);
    }
    // `T1`/`T2`/`K2` — the digit is a 1-based position.
    if let Some(pos) = name.find(|c: char| c.is_ascii_digit()) {
        if let Ok(n) = name[pos..].parse::<usize>() {
            return (n >= 1 && n <= arity).then_some(n - 1);
        }
    }
    let idx = match name {
        "V" | "R" | "U" | "B" | "S" => 1,
        "K" | "T" | "E" | "A" | "L" | "N" | "O" => 0,
        _ => return None,
    };
    (idx < arity).then_some(idx)
}

#[cfg(test)]
mod generic_tests {
    use super::*;

    #[test]
    fn single_arg_maps_any_var_to_the_sole_argument() {
        assert_eq!(type_var_index("E", 1), Some(0));
        assert_eq!(type_var_index("V", 1), Some(0), "Future<V>.get() → V even though V is 'second'");
        assert_eq!(type_var_index("R", 1), Some(0));
    }

    #[test]
    fn two_arg_maps_conventional_roles() {
        // Map<K,V>: key→0, value→1. Pair<L,R>: left→0, right→1. Entry<K,V> likewise.
        assert_eq!(type_var_index("K", 2), Some(0));
        assert_eq!(type_var_index("V", 2), Some(1));
        assert_eq!(type_var_index("L", 2), Some(0));
        assert_eq!(type_var_index("R", 2), Some(1), "Pair<L,R>.getRight() → the 2nd arg");
    }

    #[test]
    fn numeric_suffix_is_positional() {
        assert_eq!(type_var_index("T1", 2), Some(0));
        assert_eq!(type_var_index("T2", 2), Some(1));
        assert_eq!(type_var_index("T3", 2), None, "out of range → unknown");
    }

    #[test]
    fn unknown_name_on_multi_arg_is_unresolved() {
        // Unconventional names on a 2-arg generic can't be positioned → None (never guessed).
        assert_eq!(type_var_index("X", 2), None);
        assert_eq!(type_var_index("Y", 2), None);
    }

    #[test]
    fn type_var_recognition() {
        assert!(is_type_var("T"));
        assert!(is_type_var("R"));
        assert!(is_type_var("T1"));
        assert!(!is_type_var("KK"), "two letters is not a type var (could be a class)");
        assert!(!is_type_var("java/lang/String"));
        assert!(!is_type_var("Foo"));
        assert!(!is_type_var(""));
        assert!(!is_type_var("String"));
    }
}

#[cfg(test)]
mod overload_tests {
    use super::*;
    use crate::seam::ClassMembers;
    use crate::symbols::Import;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MapResolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }
    impl TypeResolver for MapResolver {
        fn members_of(&self, b: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(b).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, n: &str, _i: &[Import]) -> Option<String> {
            self.simple.get(n).cloned()
        }
    }

    fn cm(methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            superclass: Some("java/lang/Object".into()),
            interfaces: vec![],
            methods,
            fields: vec![],
            flags: Default::default(),
            type_params: vec![],
        }
    }
    fn meth(name: &str, ret: &str, params: &[&str]) -> Member {
        Member::method(name, TypeRef::simple(ret), params.iter().map(|p| TypeRef::simple(*p)).collect())
    }

    /// `Fmt` mimics `java.text.Format`/`SimpleDateFormat`: `format(Object) -> String` and
    /// `format(Object, StringBuffer, FieldPosition) -> StringBuffer`. `Ov` has two SAME-arity overloads
    /// with different returns (`pick(int) -> A`, `pick(String) -> B`) to exercise the argument tie-break.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".into(), cm(vec![]));
        members.insert("java/lang/String".into(), cm(vec![]));
        members.insert("java/lang/StringBuffer".into(), cm(vec![]));
        members.insert(
            "acme/Fmt".into(),
            cm(vec![
                meth("format", "java/lang/String", &["java/lang/Object"]),
                meth(
                    "format",
                    "java/lang/StringBuffer",
                    &["java/lang/Object", "java/lang/StringBuffer", "acme/FieldPosition"],
                ),
            ]),
        );
        members.insert(
            "acme/Ov".into(),
            cm(vec![
                meth("pick", "acme/A", &["int"]),
                meth("pick", "acme/B", &["java/lang/String"]),
                // Two REFERENCE overloads of the same arity — a tie the primitive/reference check
                // can't break.
                meth("amb", "acme/A", &["java/lang/Object"]),
                meth("amb", "acme/B", &["java/lang/String"]),
            ]),
        );
        members.insert("acme/A".into(), cm(vec![]));
        members.insert("acme/B".into(), cm(vec![]));
        let simple = [
            ("Fmt", "acme/Fmt"),
            ("Ov", "acme/Ov"),
            ("String", "java/lang/String"),
            ("Object", "java/lang/Object"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// Infer the binary type of the `call` substring of `src`.
    fn infer_call(src: &str, call: &str, r: &MapResolver) -> Option<String> {
        let start = src.find(call).expect("call substring present");
        infer_expression_type(src, start, start + call.len(), r).map(|t| t.binary_name)
    }

    #[test]
    fn arity_picks_the_one_arg_overload() {
        // `f.format(f)` is 1-arg → the `format(Object) -> String` overload, NOT the 3-arg
        // `format(…) -> StringBuffer` the old first-by-name pick returned (the reported false positive).
        let r = resolver();
        let src = "class C { void m(acme.Fmt f) { Object o = f.format(f); } }";
        assert_eq!(infer_call(src, "f.format(f)", &r).as_deref(), Some("java/lang/String"));
    }

    #[test]
    fn argument_type_breaks_a_same_arity_tie() {
        // Two 1-arg overloads with different returns; a `String` argument rules out the `int` one via
        // the primitive/reference clash → `pick(String) -> B`.
        let r = resolver();
        let src = "class C { void m(acme.Ov v) { Object o = v.pick(\"s\"); } }";
        assert_eq!(infer_call(src, "v.pick(\"s\")", &r).as_deref(), Some("acme/B"));
    }

    #[test]
    fn ambiguous_same_arity_is_unresolved_not_guessed() {
        // `amb(Object)->A` and `amb(String)->B` are both 1-arg references; an `Object` argument rules
        // out neither → the return type isn't unique → None (never the first-declared overload's A).
        let r = resolver();
        let src = "class C { void m(acme.Ov v, Object x) { Object o = v.amb(x); } }";
        assert_eq!(infer_call(src, "v.amb(x)", &r), None);
    }

    #[test]
    fn arity_admits_matches_exact_and_varargs() {
        assert!(arity_admits(2, false, 2), "exact");
        assert!(!arity_admits(2, false, 1), "fixed arity, wrong count");
        assert!(arity_admits(1, true, 0), "varargs with zero variadic args");
        assert!(arity_admits(2, true, 5), "varargs with many variadic args");
        assert!(!arity_admits(3, true, 1), "varargs needs at least the fixed prefix");
    }

    #[test]
    fn primitive_reference_clash_is_definite_only() {
        assert!(primitive_ref_clash("int", "java/lang/String"), "int param vs String arg");
        assert!(primitive_ref_clash("java/lang/String", "int"), "String param vs int arg");
        assert!(!primitive_ref_clash("int", "java/lang/Integer"), "autoboxing bridge, not a clash");
        assert!(!primitive_ref_clash("java/lang/Object", "java/lang/String"), "both references");
        assert!(!primitive_ref_clash("int", "long"), "both primitives");
    }

    #[test]
    fn unique_return_requires_agreement() {
        let a = meth("x", "acme/A", &[]);
        let b = meth("x", "acme/B", &[]);
        let a2 = meth("x", "acme/A", &["int"]);
        assert_eq!(unique_return(&[&a, &a2]).map(|t| t.binary_name), Some("acme/A".to_string()));
        assert!(unique_return(&[&a, &b]).is_none(), "different returns → no unique");
        assert!(unique_return(&[]).is_none(), "empty → none");
    }
}
