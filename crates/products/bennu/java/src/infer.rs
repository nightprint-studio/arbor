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

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use tree_sitter::Node;

use crate::seam::{Member, MemberKind, TypeRef, TypeResolver};
use crate::symbols::{node_text, FileSymbols};
use crate::typeparse::{parse_type_text, SimpleTypeRef};

/// How a local variable is typed, captured once when its scope is scanned.
enum LocalTy {
    /// An explicit declared type text (`Foo`, `Map<String,Object>`).
    Declared(String),
    /// `var x = <init>` — infer from the initializer's byte range (re-descended lazily).
    VarInit(usize, usize),
    /// `for (var x : <iterable>)` — infer from the ITERABLE's byte range and peel one type
    /// argument (`List<Foo>` → `Foo`). Distinct from [`LocalTy::VarInit`], which would type the
    /// loop variable as the collection itself.
    IterElem(usize, usize),
    /// The name IS bound here, but its type is not something we compute — a multi-catch union,
    /// whose binding is the least upper bound of its alternatives.
    ///
    /// A variant rather than simply recording nothing, because the two are opposite answers: with
    /// no entry the name falls through to an enclosing scope and can resolve to a **field it
    /// shadows**, which is a confidently wrong type. This one shadows correctly and then resolves
    /// to nothing, which leaves every member check silent — the honest outcome.
    Opaque,
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


/// Whether a declaration's written type is one the compiler infers rather than one the source
/// states: `var` (Java 10+) and Lombok's `val`, which is a `final var` an annotation processor
/// writes.
///
/// One predicate rather than a `matches!` at each site, because the sites do not agree by accident:
/// an inlay hint that knew only `var` left every Lombok `val` without the type the inference had
/// already worked out, and nothing said so.
pub fn is_inferred_type(type_text: &str) -> bool {
    matches!(type_text.trim(), "var" | "val")
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
    MethodResolution {
        candidates,
        complete,
    }
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
    // Clamp the caret to a valid char boundary ≤ len before slicing `source` below — a stale/
    // out-of-range offset (or, defensively, one mid-multibyte-char) would panic the slice.
    let mut byte_offset = byte_offset.min(source.len());
    while byte_offset > 0 && !source.is_char_boundary(byte_offset) {
        byte_offset -= 1;
    }
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

    let tree = crate::grammar::parse_java(&buf)?;
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
    let tree = crate::grammar::parse_java(source)?;
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
    infer_expression_type_cached(
        root,
        source,
        symbols,
        start,
        end,
        resolver,
        &InferCache::new(),
    )
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
    let result = root
        .named_descendant_for_byte_range(start, end)
        .and_then(|node| {
            let ctx = Ctx {
                root: *root,
                bytes,
                resolver,
                symbols,
                cache,
                depth: Cell::new(0),
            };
            let enclosing = enclosing_type_fqn(&node, bytes, symbols);
            ctx.infer_expr(&node, enclosing.as_deref())
        })
        .filter(|t| is_resolved(t, resolver));
    cache.expr.borrow_mut().insert((start, end), result.clone());
    result
}

/// Whether an inferred type is something a caller can act on. A bare type VARIABLE (`T`, `S`)
/// is not: it names a binding Phase-1 didn't resolve, and handing it out as a type invites the
/// caller to look members up on a class called `S`. Every public entry filters it out, so
/// "unknown" arrives as `None` rather than as a type that happens to resolve to nothing.
/// Whether a [`TypeRef`] names a type we actually resolved — see
/// [`crate::typename::is_resolved_binary`], which is the same question every consumer asks.
fn is_resolved(t: &TypeRef, resolver: &dyn TypeResolver) -> bool {
    crate::typename::is_resolved_binary(&t.binary_name, resolver)
}

/// The primitive `text` names, when it is exactly one — no array dimensions, no type arguments.
/// `void` is excluded on purpose: an expression of type `void` has no value to reason about, and
/// handing one out as a type would let checks compare against it.
fn primitive_type_text(text: &str) -> Option<&'static str> {
    const PRIMITIVES: &[&str] =
        &["boolean", "byte", "char", "short", "int", "long", "float", "double"];
    let text = text.trim();
    PRIMITIVES.iter().copied().find(|p| *p == text)
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
    let ctx = Ctx {
        root: *root,
        bytes,
        resolver,
        symbols,
        cache,
        depth: Cell::new(0),
    };
    let enclosing = enclosing_type_fqn(node, bytes, symbols);
    let result = ctx
        .infer_expr(node, enclosing.as_deref())
        .filter(|t| is_resolved(t, resolver));
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
    infer_receiver_type_cached(
        root,
        source,
        symbols,
        byte_offset,
        resolver,
        &InferCache::new(),
    )
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
    let result = find_receiver(root, byte_offset)
        .and_then(|receiver| {
            let ctx = Ctx {
                root: *root,
                bytes,
                resolver,
                symbols,
                cache,
                depth: Cell::new(0),
            };
            let enclosing = enclosing_type_fqn(&receiver, bytes, symbols);
            ctx.infer_expr(&receiver, enclosing.as_deref())
        })
        .filter(|t| is_resolved(t, resolver));
    cache
        .receiver
        .borrow_mut()
        .insert(byte_offset, result.clone());
    result
}

/// How deep [`Ctx::infer_expr`] may recurse before it gives up and answers "unknown".
///
/// Inference descends the expression tree, and the tree's depth is not bounded by
/// anything a person wrote: `"a" + "b" + "c" + …` nests one level per operand, so a
/// machine-generated concatenation of a few thousand pieces — an unrolled SQL
/// builder, a generated messages class — is a few thousand levels. Recursing that
/// far exhausts the stack, and a stack overflow in Rust is **not** a panic that
/// [`std::panic::catch_unwind`] can turn into an error: the process aborts. One
/// generated file in the project would take the whole backend down with it, and
/// every other file's diagnostics with that.
///
/// So depth is capped and the answer past it is `None` — "I don't know", which every
/// caller already handles, because inference not resolving is the ordinary case for
/// anything off the classpath. 128 is far past hand-written code (a long fluent
/// chain is tens of levels) and far short of what troubles a stack, counting the
/// several frames each level actually costs.
const MAX_INFER_DEPTH: usize = 128;

/// Shared inference context. `root` is the file's parse tree root (to re-descend for `var`
/// initializers); `cache` memoizes results + per-scope locals.
struct Ctx<'a> {
    root: Node<'a>,
    bytes: &'a [u8],
    resolver: &'a dyn TypeResolver,
    symbols: &'a FileSymbols,
    cache: &'a InferCache,
    /// Current recursion depth of [`Ctx::infer_expr`]. See [`MAX_INFER_DEPTH`].
    depth: Cell<usize>,
}

/// Holds a level of [`Ctx::depth`] for as long as it lives.
///
/// A `Drop` guard and not a decrement at the end of the function, because
/// `infer_expr` leaves through a dozen `?` operators and a hand-written decrement
/// would be missed by all of them — the counter would climb and inference would go
/// permanently silent for the rest of the file.
struct DepthGuard<'c>(&'c Cell<usize>);

impl Drop for DepthGuard<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_sub(1));
    }
}

/// Classification of a bare identifier against the enclosing lambda scopes: it's a lambda parameter
/// we could TARGET-TYPE, a lambda parameter we couldn't type (but that still SHADOWS a same-named
/// field, so the caller must not read the field), or not a lambda parameter at all.
enum LambdaParam {
    Typed(TypeRef),
    Untyped,
    NotParam,
}

/// What looking a bare name up as a local found.
///
/// Three outcomes rather than an `Option`, for the reason [`LambdaParam`] has three: "there is no
/// local called this" and "there is one and I cannot type it" lead to opposite next steps. The
/// first should go on to try a field; the second must not, because the local **shadows** that
/// field and typing the name as it would be confidently wrong.
enum LocalLookup {
    Typed(TypeRef),
    /// Found, untypeable — a multi-catch union's binding (whose type is the least upper bound of
    /// its alternatives), or an initializer we could not infer.
    Opaque,
    NotLocal,
}

impl Ctx<'_> {
    /// Infer the type of an arbitrary receiver expression node.
    ///
    /// Every recursive path through inference comes back here, so this is the one
    /// place the depth has to be counted (see [`MAX_INFER_DEPTH`]).
    fn infer_expr(&self, node: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        if self.depth.get() >= MAX_INFER_DEPTH {
            return None;
        }
        self.depth.set(self.depth.get() + 1);
        let _depth = DepthGuard(&self.depth);

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
            "decimal_integer_literal"
            | "hex_integer_literal"
            | "octal_integer_literal"
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
        let op = node
            .child_by_field_name("operator")
            .and_then(|o| node_text(&o, self.bytes));
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

        match self.resolve_local(node, &name) {
            LocalLookup::Typed(tr) => return Some(tr),
            // Bound here, type unknown. Same rule as an untyped lambda parameter below: leave it
            // unresolved rather than reach past the binding to the field it hides.
            LocalLookup::Opaque => return None,
            LocalLookup::NotLocal => {}
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
            if let Some(tr) = self.field_in_enclosing_scope(fqn, &name) {
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
                        if let Some(n) = ch
                            .child_by_field_name("name")
                            .and_then(|n| node_text(&n, self.bytes))
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

    /// The functional-interface type a lambda is assigned to: the type of the parameter at its
    /// argument position, the declared type of the variable it initialises, or the return type of
    /// the method it is returned from. `None` for anything else (a cast, an array initialiser) or
    /// when the callee / position is ambiguous.
    ///
    /// Getting this wrong is not a missing nicety. A lambda parameter with no target type is a
    /// receiver with no type, so **every member use on it disappears from the reference index** —
    /// and a rename of that member then leaves each of those call sites spelling a name that no
    /// longer exists. On a real project the argument position alone accounted for ~57 broken call
    /// sites, all on one record read inside a lambda.
    fn lambda_target_type(&self, lambda: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        let arg_list = lambda.parent()?;
        if arg_list.kind() != "argument_list" {
            return self.lambda_target_from_context(lambda, enclosing);
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
                let name = call
                    .child_by_field_name("name")
                    .and_then(|n| node_text(&n, self.bytes))?;
                let recv = match call.child_by_field_name("object") {
                    Some(obj) => match self.infer_expr(&obj, enclosing) {
                        Some(t) => t,
                        // A STATIC call is qualified by a TYPE name, and a type name is not an
                        // expression — `infer_expr` has nothing to say about it. Without this
                        // fallback every lambda passed to a static factory (`Checker.build(x -> …)`,
                        // the shape half a codebase uses) had no target type at all.
                        None => self.resolve_type_text(&node_text(&obj, self.bytes)?)?,
                    },
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

    /// The target type of a lambda that is NOT an argument: the declared type of the variable it
    /// initialises, or the return type of the method it is returned from.
    ///
    /// Both are written down in the source a line or two away, which is what makes them worth
    /// reading — no inference, just the type the author already spelled.
    fn lambda_target_from_context(
        &self,
        lambda: &Node,
        _enclosing: Option<&str>,
    ) -> Option<TypeRef> {
        let parent = lambda.parent()?;
        match parent.kind() {
            // `Creator c = confirm -> …;`, and the field form of the same thing.
            "variable_declarator" => {
                let decl = parent.parent()?;
                if !matches!(
                    decl.kind(),
                    "local_variable_declaration" | "field_declaration"
                ) {
                    return None;
                }
                let text = node_text(&decl.child_by_field_name("type")?, self.bytes)?;
                self.resolve_type_text(&text)
            }
            // `return confirm -> …;` — the enclosing method's return type. The walk stops at an
            // intervening lambda: a `return` inside a lambda BODY answers to that lambda's
            // interface, not to the method, and guessing the method's type there would be wrong.
            "return_statement" => {
                let mut cur = parent.parent();
                while let Some(n) = cur {
                    match n.kind() {
                        "lambda_expression" => return None,
                        "method_declaration" => {
                            let ty = n.child_by_field_name("type")?;
                            let text = node_text(&ty, self.bytes)?;
                            return self.resolve_type_text(&text);
                        }
                        _ => {}
                    }
                    cur = n.parent();
                }
                None
            }
            _ => None,
        }
    }

    /// The type of parameter `idx` of method `name` on `recv`, walking supertypes — but ONLY when
    /// every overload of that name agrees on that parameter's type (so an ambiguous overloaded call
    /// yields `None`, never a guess). The receiver's generics are then substituted, so
    /// `List<Foo>.forEach(Consumer<? super E>)` yields `Consumer<Foo>`.
    fn param_at(&self, recv: &TypeRef, name: &str, idx: usize) -> Option<TypeRef> {
        // The DECLARING class travels with each candidate. A method inherited from a supertype is
        // written in that supertype's type variables — `Iterable.forEach(Consumer<? super T>)` — and
        // substituting them against the RECEIVER's list (`List<E>`) matches nothing, leaves `T`
        // standing, and the lambda parameter ends up untyped. Which is why `list.replaceAll(x -> …)`
        // typed `x` and `list.forEach(x -> …)`, the commoner of the two by far, did not.
        let mut types: Vec<(String, TypeRef)> = Vec::new();
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
                    types.push((bn.clone(), m.params.get(idx)?.clone()));
                }
            }
            if let Some(sc) = cm.superclass.clone() {
                stack.push(sc);
            }
            stack.extend(cm.interfaces.iter().cloned());
        }
        // Require a single distinct parameter type across all overloads.
        let mut uniq: Vec<(String, TypeRef)> = Vec::new();
        for t in &types {
            if !uniq.iter().any(|(_, ty)| ty == &t.1) {
                uniq.push(t.clone());
            }
        }
        let [(declaring, fi)] = uniq.as_slice() else { return None };
        Some(self.substitute_generics(fi, &self.as_declared_by(declaring, recv)))
    }

    /// The receiver seen AS the class that declares the method: that class's binary name carrying
    /// the receiver's own type arguments.
    ///
    /// Sound only while the subtype passes its parameters to the supertype in order — `List<E>
    /// extends Collection<E> extends Iterable<E>`, which is every JDK collection — because the index
    /// records supertypes by binary name and drops their type arguments, so the real mapping is not
    /// available to read. Guarded on the arities matching, and it falls back to the receiver
    /// untouched when they do not, which is the previous behaviour rather than a worse guess.
    fn as_declared_by(&self, declaring: &str, recv: &TypeRef) -> TypeRef {
        if declaring == recv.binary_name || recv.type_args.is_empty() {
            return recv.clone();
        }
        let same_arity = self
            .resolver
            .members_of(declaring)
            .is_some_and(|cm| cm.type_params.len() == recv.type_args.len());
        if !same_arity {
            return recv.clone();
        }
        let mut seen = recv.clone();
        seen.binary_name = declaring.to_string();
        seen
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
            if !uniq
                .iter()
                .any(|u| u.name == m.name && u.params == m.params)
            {
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
        // `Outer.this` — the qualified `this` an INNER class uses to reach its enclosing instance.
        // It is spelled as a field access whose "field" is the keyword `this`, and there is no such
        // field: the expression's type is simply the qualifying type. Without this,
        // `Outer.this.member(…)` typed to nothing — and that is how an inner class calls its outer
        // one throughout Apache Commons, so those calls were invisible to the index and a rename of
        // the member left every one of them behind.
        if field_name == "this" {
            let text = node_text(&obj, self.bytes)?;
            let binary =
                self.simple_to_binary_resolved(text.split('<').next().unwrap_or(&text).trim())?;
            return Some(TypeRef::simple(binary));
        }

        let obj_type = match self.infer_expr(&obj, enclosing) {
            Some(t) => t,
            // Not a value — so it may be a TYPE, and this a STATIC field read: `Headers.USERNAME`,
            // `Integer.MAX_VALUE`. The same fallback (and the same order) as a static CALL, which
            // had it and this did not: an enum constant is a static field of its enum, so
            // `Headers.USERNAME.header_name()` dead-ended at the constant and every accessor
            // reached through one was missing from the index.
            None => self.type_receiver(&obj)?,
        };
        if let Some(t) = self.field_type_on(&obj_type, &field_name) {
            return Some(t);
        }
        // `Outer.Nested` — the "field" is a nested TYPE, and the expression denotes that type
        // rather than a value. Needed for anything reached through one: `Dto.Fields.file_name` is
        // two field accesses, and the first one is this.
        let nested = format!("{}/{field_name}", obj_type.binary_name);
        self.resolver
            .members_of(&nested)
            .is_some()
            .then(|| TypeRef::simple(nested))
    }

    /// `recv.foo(args)` or bare `foo(args)`. Resolves `foo`'s return type (arity-aware overload
    /// selection), applying generics carry-through from the receiver.
    fn infer_method_invocation(&self, node: &Node, enclosing: Option<&str>) -> Option<TypeRef> {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| node_text(&n, self.bytes))?;
        let args = self.call_arg_nodes(node);

        let recv_type = match node.child_by_field_name("object") {
            Some(obj) => match self.infer_expr(&obj, enclosing) {
                Some(t) => t,
                // Not a value — so it may be a TYPE, and this a static call:
                // `StringUtils.isEmpty(s)`, `Math.max(a, b)`, `Retriever.properties(svc)`.
                // Tried only after value resolution fails, which is also Java's own rule (a
                // variable named `Foo` obscures the type `Foo`).
                None => self.type_receiver(&obj)?,
            },
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

    /// A receiver that names a **type** rather than a value — the static-call form.
    ///
    /// Without this, `Foo.bar()` dead-ends: the receiver resolves as a local, a lambda parameter,
    /// a field or a static-imported field, and a *type* is none of those, so the whole invocation
    /// yields nothing. That silently cost every `var`/`val` initialised from a static call its
    /// type, which is a very common way to write one.
    ///
    /// The capitalisation guard is what keeps it honest: type names are capitalised in every
    /// codebase this will run on, and without it an unresolved lowercase variable would be
    /// "resolved" as a phantom type whose members then fail to match — turning a clean miss into
    /// a confusing one. A name that resolves to no known type still yields nothing.
    fn type_receiver(&self, obj: &Node) -> Option<TypeRef> {
        if !matches!(
            obj.kind(),
            "identifier" | "scoped_identifier" | "field_access"
        ) {
            return None;
        }
        let text = node_text(obj, self.bytes)?;
        let last = text.rsplit('.').next()?;
        if !last.chars().next()?.is_uppercase() {
            return None;
        }
        self.resolve_type_text(&text)
    }

    /// The real argument nodes of a call — the `arguments` (`argument_list`) named children, skipping
    /// comments (which ARE named children in tree-sitter). Their count + inferred types drive
    /// arity/argument overload selection.
    fn call_arg_nodes<'t>(&self, call: &Node<'t>) -> Vec<Node<'t>> {
        let Some(list) = call.child_by_field_name("arguments") else {
            return Vec::new();
        };
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
            cm.fields
                .iter()
                .find(|m| m.name == field_name)
                .map(|m| m.return_type.clone())
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
        if let Some(tr) =
            self.method_return_of_source_type(&from_binary(&recv.binary_name), method_name, args)
        {
            return Some(tr);
        }
        let res = self
            .cache
            .resolve_methods(self.resolver, &recv.binary_name, method_name);
        let (ret, picked) = self.select_overload(&res.candidates, args, enclosing)?;
        // Receiver-bound variables first (`List<Foo>.get` → `Foo`), then the ones the receiver can't
        // answer for because the METHOD declares them.
        let ret = self.substitute_generics(&ret, recv);
        Some(match picked {
            Some(m) => self.bind_method_type_vars(&ret, m, args, enclosing),
            None => ret,
        })
    }

    /// Resolve a **method-level** type variable from the argument that determines it.
    ///
    /// `static <T> Optional<T> ofNullable(T value)` is the shape: `T` is declared by the *method*, so
    /// the receiver carries no type argument to substitute it with — [`Self::substitute_generics`]
    /// correctly leaves it alone, and the chain that follows (`.orElse(…)` → `T`) stays unresolved.
    /// The binding was in the call all along: whatever was passed *as* `value`.
    ///
    /// Deliberately limited to an **identity parameter** — one whose declared type is exactly the
    /// variable, with no type arguments of its own. That covers the static factories real code is
    /// full of (`Optional.of/ofNullable`, `Objects.requireNonNull`, `Collections.singletonList`) and
    /// stops short of structural unification, which is what `Stream.map(Function<? super T, ? extends
    /// R>)` would need: recovering `R` there means typing a lambda or method reference, which this
    /// inference doesn't do. Those stay unresolved, exactly as before.
    ///
    /// Every step abstains rather than guesses: an argument we can't type, an argument that is itself
    /// a type variable, a variadic or wrapped parameter — each simply contributes no binding.
    fn bind_method_type_vars(
        &self,
        ret: &TypeRef,
        m: &Member,
        args: &[Node],
        enclosing: Option<&str>,
    ) -> TypeRef {
        // Only worth the argument inference when something in the return type is still open.
        if !self.mentions_type_variable(ret) {
            return ret.clone();
        }
        let mut bindings: Vec<(String, TypeRef)> = Vec::new();
        for (i, p) in m.params.iter().enumerate() {
            if !p.type_args.is_empty() || !self.is_type_variable(&p.binary_name) {
                continue; // not an identity parameter
            }
            if bindings.iter().any(|(v, _)| *v == p.binary_name) {
                continue; // first occurrence wins; a second would need a join we don't compute
            }
            let Some(arg) = args.get(i) else { continue };
            let Some(arg_ty) = self.infer_expr(arg, enclosing) else { continue };
            if arg_ty.binary_name.is_empty() || self.is_type_variable(&arg_ty.binary_name) {
                continue; // an untypeable argument binds nothing
            }
            bindings.push((p.binary_name.clone(), arg_ty));
        }
        if bindings.is_empty() {
            return ret.clone();
        }
        apply_bindings(ret, &bindings)
    }

    /// Whether `ty` — at any depth — still names a type variable.
    fn mentions_type_variable(&self, ty: &TypeRef) -> bool {
        self.is_type_variable(&ty.binary_name)
            || ty.type_args.iter().any(|a| self.mentions_type_variable(a))
    }

    /// Whether `name` is a type **variable** rather than a type.
    ///
    /// A binary name is package-qualified, so a slash settles it outright. What is left is a bare
    /// identifier, which is a variable only when it names no type the resolver knows — that last
    /// test is what keeps a class in the default package (`Foo`) from reading as a variable, and it
    /// is why this is a method on the walk rather than the free-standing shape test [`is_type_var`].
    fn is_type_variable(&self, name: &str) -> bool {
        !name.is_empty()
            && !name.contains('/')
            && !is_primitive(name)
            && self.resolver.members_of(name).is_none()
    }

    /// Pick the return type of the overload a call of `args` binds to, from all same-named `candidates`
    /// on the receiver's hierarchy — JLS §15.12.2 in miniature, deliberately conservative:
    ///   1. keep candidates whose ARITY admits the call (a trailing array/varargs param admits 0+ extra);
    ///   2. if those all agree on a return type → use it (the common case, and what fixes a 1-arg
    ///      `df.format(date)` → `String` that the old first-by-name pick mis-resolved to a 3-arg
    ///      `Format.format(…)` → `StringBuffer`);
    ///   3. otherwise narrow the tie by argument types, rejecting only a DEFINITE primitive/reference
    ///      clash, and use the return type iff it is now unique;
    ///   4. still not unique → `None` for the return type. An ambiguous overload is never guessed: a
    ///      wrong return type mistypes the expression and risks a false "cannot resolve member" /
    ///      assignment diagnostic.
    ///
    /// The second half of the pair is the MEMBER the return type came from, and only **when exactly
    /// one candidate survived**. The distinction matters to [`Self::bind_method_type_vars`] and to
    /// nothing else: several overloads can agree on a return type while differing in their
    /// parameters, and reading a type variable's binding off the parameters of a method the call
    /// might not have chosen would be a guess. `None` there means "the return type is trustworthy,
    /// the parameter list is not".
    fn select_overload<'m>(
        &self,
        candidates: &'m [Member],
        args: &[Node],
        enclosing: Option<&str>,
    ) -> Option<(TypeRef, Option<&'m Member>)> {
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
            return Some((only.return_type.clone(), Some(only)));
        }
        let argc = args.len();
        let arity_ok: Vec<&Member> = distinct
            .into_iter()
            .filter(|m| arity_admits(m.params.len(), last_is_array(m), argc))
            .collect();
        if let Some(ret) = unique_return(&arity_ok) {
            return Some((ret, sole(&arity_ok)));
        }
        if arity_ok.is_empty() {
            return None;
        }
        // Return types disagree → narrow by argument types (each inferred once; unknown args abstain).
        let arg_types: Vec<Option<TypeRef>> =
            args.iter().map(|a| self.infer_expr(a, enclosing)).collect();
        let applicable: Vec<&Member> = arity_ok
            .into_iter()
            .filter(|m| args_admissible(&m.params, &arg_types))
            .collect();
        unique_return(&applicable).map(|ret| (ret, sole(&applicable)))
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
        // Position of `bn` in the receiver CLASS's real declared type-parameter list (from source
        // `<…>` or the bytecode signature).
        let cm = self.resolver.members_of(&recv.binary_name);
        let type_params_known = cm.as_ref().is_some_and(|c| !c.type_params.is_empty());
        let declared_idx = cm
            .as_ref()
            .and_then(|c| c.type_params.iter().position(|p| p == bn));
        // Whether `bn` IS a type variable here. The receiver class's own `<…>` list is the exact
        // answer and the only one that works for a variable named `Param`: a type variable is an
        // identifier, not a letter, and `record Edit<Source, Param>` is ordinary Java. Judging by
        // shape alone — an uppercase letter plus digits — silently declined to substitute every
        // multi-letter variable, so `edit.param()` typed as the literal `Param` and every member
        // read off it disappeared from the index.
        //
        // The shape test survives as the fallback for a library class whose declared list we could
        // not decode, where there is nothing else to go on.
        if declared_idx.is_some() || (!type_params_known && is_type_var(bn)) {
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
            return match idx.and_then(|i| recv.type_args.get(i)) {
                // An `Object` type argument is where an unbounded wildcard lands: the bytecode
                // decoder collapses `?` to its upper bound, and `Object` is what "no bound" looks
                // like. Propagating it is what turns a fluent builder into an `Object` and makes
                // the NEXT call in the chain a false "cannot resolve method": Spring's
                // `RestClient.get()` returns `RequestHeadersUriSpec<?>`, whose `uri(…)` returns
                // its own self-type — so `.uri(…).accept(…)` is perfectly legal, and reporting
                // `accept` as missing on `Object` is a diagnostic about code that compiles.
                // Java resolves this through the wildcard's capture (or the variable's bound),
                // neither of which Phase-1 models, so the honest answer is "unknown" — the type
                // variable itself, which every consumer treats as unresolved.
                Some(a) if a.binary_name == "java/lang/Object" => member_ret.clone(),
                Some(a) => a.clone(),
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
    /// A field named `name` visible from inside `fqn` — declared there, inherited there, or
    /// declared by any class `fqn` is written INSIDE.
    ///
    /// A nested class sees its enclosing classes' members and writes them unqualified (JLS §8.1.3),
    /// and an anonymous body is a nested class. Stopping at the innermost type left the whole
    /// expression untyped, so everything read off it disappeared: Guava writes
    /// `upperBoundWindow.upperBound.isLessThan(…)` inside an anonymous iterator, and renaming
    /// `isLessThan` could not see that call at all.
    ///
    /// The climb is over the FQN because nesting IS the FQN here, and it stops as soon as the
    /// trimmed prefix is no longer a type — what is above the outermost one is the package.
    fn field_in_enclosing_scope(&self, fqn: &str, name: &str) -> Option<TypeRef> {
        let mut scope = fqn;
        loop {
            if let Some(tr) = self.field_type_of_source_type(scope, name) {
                return Some(tr);
            }
            // Inherited by this level, too — the enclosing class's own supertypes are in scope for
            // everything written inside it.
            if let Some(tr) = self.field_type_on(&TypeRef::simple(to_binary(scope)), name) {
                return Some(tr);
            }
            let Some(i) = scope.rfind('.') else {
                return None;
            };
            let outer = &scope[..i];
            if !self.is_type_fqn(outer) {
                return None;
            }
            scope = outer;
        }
    }

    /// Whether `fqn` names a type — this file's own declarations first, then the resolver, so a
    /// chain that crosses files still climbs.
    fn is_type_fqn(&self, fqn: &str) -> bool {
        self.symbols.types.iter().any(|t| t.fqn == fqn)
            || self.resolver.members_of(&to_binary(fqn)).is_some()
    }

    fn field_type_of_source_type(&self, fqn: &str, field_name: &str) -> Option<TypeRef> {
        let td = self
            .symbols
            .types
            .iter()
            .find(|t| t.fqn == fqn || t.name == fqn)?;
        let fd = td.fields.iter().find(|f| f.name == field_name)?;
        self.resolve_type_text(&fd.type_text)
    }

    /// The resolved superclass type of the source type `fqn` (for `super.member`). `None` when
    /// the type has no `extends` clause (implicit `Object`, which the project-only engine won't
    /// decode anyway) or its declaration isn't in this file's symbols.
    fn superclass_type(&self, fqn: &str) -> Option<TypeRef> {
        let td = self
            .symbols
            .types
            .iter()
            .find(|t| t.fqn == fqn || t.name == fqn)?;
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
        let td = self
            .symbols
            .types
            .iter()
            .find(|t| t.fqn == fqn || t.name == fqn)?;
        let same_named: Vec<&crate::symbols::MethodDecl> = td
            .methods
            .iter()
            .filter(|m| m.name == method_name)
            .collect();
        // A single method of this name isn't an overload → trust it (behavior identical to before).
        if let [only] = same_named.as_slice() {
            return self.resolve_type_text(&only.return_type_text);
        }
        let argc = args.len();
        let mut ret_text: Option<&str> = None;
        for md in same_named.iter().copied() {
            let last_array = md.params.last().is_some_and(|p| {
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
    fn resolve_local(&self, use_node: &Node, name: &str) -> LocalLookup {
        let use_start = use_node.start_byte();
        let mut scope = use_node.parent();
        while let Some(s) = scope {
            // method / lambda / constructor parameters. Only these scopes HAVE a `parameters` field;
            // gating here avoids an O(children) `child_by_field_name` scan on every block / if / try /
            // catch ancestor (a huge method body has hundreds of statements, scanned per identifier).
            if matches!(
                s.kind(),
                "method_declaration" | "constructor_declaration" | "lambda_expression"
            ) {
                if let Some(tr) = self.param_type(&s, name) {
                    return LocalLookup::Typed(tr);
                }
            }
            // local variable declarations directly in this scope, before the use (last one wins,
            // matching the previous in-order scan).
            let locals = self.scope_locals(&s);
            if let Some(decls) = locals.get(name) {
                if let Some(decl) = decls.iter().rev().find(|d| d.start < use_start) {
                    return match self.resolve_local_ty(&decl.ty) {
                        Some(tr) => LocalLookup::Typed(tr),
                        None => LocalLookup::Opaque,
                    };
                }
            }
            scope = s.parent();
        }
        LocalLookup::NotLocal
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
                let t = p
                    .child_by_field_name("type")
                    .and_then(|n| node_text(&n, self.bytes))?;
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
        // A binder declared BY the scope node itself, not by a child statement: the enhanced-`for`
        // variable (`for (Foo x : xs) { … }`), whose scope is the loop body. Without it, `x` fell
        // through to a same-named FIELD of the enclosing class and every `x.method()` was typed
        // against the field's type — a false "cannot resolve method" on perfectly legal shadowing.
        if scope.kind() == "enhanced_for_statement" {
            self.add_for_each_var(scope, &mut map);
        }
        // Pattern variables (`o instanceof Foo f`, `case Foo f ->`), whose scope is expressed by the
        // flow around `scope` rather than by a declaration inside it.
        self.add_pattern_vars(scope, &mut map);
        let mut cw = scope.walk();
        for c in scope.named_children(&mut cw) {
            match c.kind() {
                "local_variable_declaration" => {
                    let type_text = c
                        .child_by_field_name("type")
                        .and_then(|n| node_text(&n, self.bytes));
                    let start = c.start_byte();
                    let mut dw = c.walk();
                    for d in c.named_children(&mut dw) {
                        if d.kind() != "variable_declarator" {
                            continue;
                        }
                        let name = d
                            .child_by_field_name("name")
                            .and_then(|n| node_text(&n, self.bytes));
                        let value = d.child_by_field_name("value");
                        if let (Some(vn), Some(ty)) =
                            (name, self.local_ty_of(type_text.as_deref(), value))
                        {
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
                        let type_text = r
                            .child_by_field_name("type")
                            .and_then(|n| node_text(&n, self.bytes));
                        let name = r
                            .child_by_field_name("name")
                            .and_then(|n| node_text(&n, self.bytes));
                        let value = r.child_by_field_name("value");
                        if let (Some(vn), Some(ty)) =
                            (name, self.local_ty_of(type_text.as_deref(), value))
                        {
                            map.entry(vn).or_default().push(LocalDecl {
                                start: r.start_byte(),
                                ty,
                            });
                        }
                    }
                }
                // `catch (FooException e) { e.getMessage(); }` — the parameter is a child of the
                // `catch_clause` (whose body is the scope), and it carries a `catch_type` node
                // instead of a `type` field. A multi-catch union (`A | B`) binds the LUB, which we
                // don't compute → left unresolved rather than typed as its first alternative.
                "catch_formal_parameter" => {
                    let name = c
                        .child_by_field_name("name")
                        .and_then(|n| node_text(&n, self.bytes));
                    let mut tw = c.walk();
                    let catch_type = c.named_children(&mut tw).find(|n| n.kind() == "catch_type");
                    // A single alternative is the binding's type. A union has none we can name, so
                    // the binding is recorded as OPAQUE rather than skipped: skipping it would let
                    // the name fall through to a field it shadows and be typed as that instead.
                    let ty = match catch_type {
                        Some(ct) if ct.named_child_count() == 1 => ct
                            .named_child(0)
                            .and_then(|t| node_text(&t, self.bytes))
                            .map(LocalTy::Declared),
                        Some(_) => Some(LocalTy::Opaque),
                        None => None,
                    };
                    if let (Some(vn), Some(ty)) = (name, ty) {
                        map.entry(vn).or_default().push(LocalDecl {
                            start: c.start_byte(),
                            ty,
                        });
                    }
                }
                _ => {}
            }
        }
        let rc = Rc::new(map);
        self.cache.scope_locals.borrow_mut().insert(id, rc.clone());
        rc
    }

    /// Record the enhanced-`for` loop variable of `loop_node` (`for (Foo x : xs)`) as a local of the
    /// loop scope. `var`/`val` take the ELEMENT type of the iterable, never the iterable itself.
    ///
    /// The declaration is anchored AFTER the iterable expression: the variable is in scope only in
    /// the body, so a same-named identifier inside `xs` (`for (Foo x : x.getKids())`) still resolves
    /// to the outer binding it actually refers to.
    fn add_for_each_var(&self, loop_node: &Node, map: &mut HashMap<String, Vec<LocalDecl>>) {
        let Some(name) = loop_node
            .child_by_field_name("name")
            .and_then(|n| node_text(&n, self.bytes))
        else {
            return;
        };
        let type_text = loop_node
            .child_by_field_name("type")
            .and_then(|n| node_text(&n, self.bytes));
        let value = loop_node.child_by_field_name("value");
        let start = value.map_or_else(|| loop_node.start_byte(), |v| v.end_byte());
        let ty = match type_text.as_deref() {
            Some(t) if is_inferred_type(t) => {
                value.map(|v| LocalTy::IterElem(v.start_byte(), v.end_byte()))
            }
            Some(t) => Some(LocalTy::Declared(t.to_string())),
            None => None,
        };
        if let Some(ty) = ty {
            map.entry(name).or_default().push(LocalDecl { start, ty });
        }
    }

    /// Register the pattern variables in scope inside `scope` (`o instanceof Foo f`, `case Foo f`).
    /// A pattern variable's scope is defined by FLOW, not by nesting, so each shape is matched
    /// explicitly and conservatively — a binding is registered only where Java guarantees it is
    /// definitely assigned, never "somewhere in the method":
    ///   * the branch/body governed by a positive test — `if (o instanceof Foo f) { f.… }`,
    ///     `while (o instanceof Foo f) { … }`, `cond ? f.… : …`;
    ///   * the rest of a `&&` chain — `o instanceof Foo f && f.…`;
    ///   * the statements AFTER a guard that returns/throws — `if (!(o instanceof Foo f)) return;`;
    ///   * a `switch` case body — `case Foo f -> f.…` / `case Foo f: …`.
    /// Anything else (an `||` branch, a negation without an abrupt exit, a record-pattern shape we
    /// can't read) is left unregistered: the identifier stays unresolved instead of being typed wrong.
    fn add_pattern_vars(&self, scope: &Node, map: &mut HashMap<String, Vec<LocalDecl>>) {
        let mut binds = Vec::new();

        // The body governed by a positive test — the binding holds throughout it.
        if let Some(p) = scope.parent() {
            let governing = match p.kind() {
                "if_statement" | "ternary_expression" => {
                    (field_is(&p, "consequence", scope)).then(|| p.child_by_field_name("condition"))
                }
                "while_statement" | "for_statement" => {
                    (field_is(&p, "body", scope)).then(|| p.child_by_field_name("condition"))
                }
                _ => None,
            };
            if let Some(cond) = governing.flatten() {
                self.collect_true_bindings(&cond, &mut binds);
            }
        }

        match scope.kind() {
            // `o instanceof Foo f && f.bar()` — the left operand's bindings hold in the right one.
            "binary_expression"
                if scope
                    .child_by_field_name("operator")
                    .and_then(|o| node_text(&o, self.bytes))
                    .as_deref()
                    == Some("&&") =>
            {
                if let Some(left) = scope.child_by_field_name("left") {
                    self.collect_true_bindings(&left, &mut binds);
                }
            }
            // `case Foo f ->` / `case Foo f:` — the label's bindings hold in the case body.
            "switch_rule" | "switch_block_statement_group" => {
                let mut lw = scope.walk();
                for l in scope
                    .named_children(&mut lw)
                    .filter(|l| l.kind() == "switch_label")
                {
                    self.collect_true_bindings(&l, &mut binds);
                }
            }
            _ => {}
        }

        // The guard idiom: `if (!(o instanceof Foo f)) return;` binds `f` for the REST of the block.
        // Anchored at the guard's end, so the negative branch itself (where `f` is NOT in scope, and
        // the name may legitimately mean a field) is excluded.
        let mut gw = scope.walk();
        for st in scope
            .named_children(&mut gw)
            .filter(|s| s.kind() == "if_statement")
        {
            let is_guard = st
                .child_by_field_name("consequence")
                .is_some_and(|c| completes_abruptly(&c))
                && st.child_by_field_name("alternative").is_none();
            if !is_guard {
                continue;
            }
            let Some(negated) = st
                .child_by_field_name("condition")
                .and_then(|c| unwrap_negation(&c))
            else {
                continue;
            };
            let mut guarded = Vec::new();
            self.collect_true_bindings(&negated, &mut guarded);
            binds.extend(guarded.into_iter().map(|(n, t, _)| (n, t, st.end_byte())));
        }

        for (name, type_text, start) in binds {
            map.entry(name).or_default().push(LocalDecl {
                start,
                ty: LocalTy::Declared(type_text),
            });
        }
    }

    /// Collect `(name, declared type text, binding end offset)` for every pattern variable that is
    /// definitely bound when `expr` evaluates to TRUE. Descent stops at the constructs that break
    /// that guarantee: a negation, either side of an `||`, and a nested lambda (its own scope).
    fn collect_true_bindings(&self, expr: &Node, out: &mut Vec<(String, String, usize)>) {
        match expr.kind() {
            // A negation inverts the guarantee; a lambda, a nested `switch` and an anonymous class
            // body all carry their own scopes — nothing they bind is in scope out here.
            "unary_expression" | "lambda_expression" | "switch_expression" | "class_body" => return,
            "binary_expression"
                if expr
                    .child_by_field_name("operator")
                    .and_then(|o| node_text(&o, self.bytes))
                    .as_deref()
                    == Some("||") =>
            {
                return
            }
            // `o instanceof Foo f` — the type is the `right` field, the binding the `name` field.
            // A record deconstruction (`o instanceof Point(int x, int y)`) has no `name`; its
            // components are picked up by the descent below.
            "instanceof_expression" => {
                if let (Some(n), Some(t)) = (
                    expr.child_by_field_name("name")
                        .and_then(|n| node_text(&n, self.bytes)),
                    expr.child_by_field_name("right")
                        .and_then(|t| node_text(&t, self.bytes)),
                ) {
                    out.push((n, t, expr.end_byte()));
                }
            }
            // `case Foo f` (switch patterns) and record-pattern components are positional:
            // `<type> <identifier>`, with no field names.
            "type_pattern" | "record_pattern_component" => {
                if let (Some(t), Some(n)) = (expr.named_child(0), expr.named_child(1)) {
                    if let (Some(tt), Some(nn)) =
                        (node_text(&t, self.bytes), node_text(&n, self.bytes))
                    {
                        out.push((nn, tt, expr.end_byte()));
                    }
                }
            }
            _ => {}
        }
        let mut w = expr.walk();
        for c in expr.named_children(&mut w) {
            self.collect_true_bindings(&c, out);
        }
    }

    /// Classify a local/resource declaration's type from its written `type_text` and optional
    /// initializer `value`. `var` (Java 10+) and Lombok `val` (a `final var`) both infer from the
    /// initializer — so `val x = repo.find();` / `try (var in = open())` resolve `x`/`in`'s members
    /// (without this, `val` would resolve as a phantom type named `val` and every `x.method()` would be
    /// falsely flagged). A written type is taken as-is; a missing type/initializer yields `None` (skip).
    fn local_ty_of(&self, type_text: Option<&str>, value: Option<Node>) -> Option<LocalTy> {
        match type_text {
            Some(t) if is_inferred_type(t) => {
                value.map(|init| LocalTy::VarInit(init.start_byte(), init.end_byte()))
            }
            Some(t) => Some(LocalTy::Declared(t.to_string())),
            None => None,
        }
    }

    /// Resolve a cached local's type — a declared type text, or a `var` initializer re-descended
    /// from the tree by its byte range.
    ///
    /// A re-descended expression is inferred with ITS OWN enclosing type, so an initializer that
    /// reads a field (`var rows = this.dao.find();`, `for (var r : rows())`) resolves like it does
    /// anywhere else instead of dead-ending on an unknown bare name.
    fn resolve_local_ty(&self, ty: &LocalTy) -> Option<TypeRef> {
        match ty {
            // Bound, but to nothing we can name. The `None` is the answer, not a fall-through.
            LocalTy::Opaque => None,
            LocalTy::Declared(t) => self.resolve_type_text(t),
            LocalTy::VarInit(start, end) => {
                let init = self.root.named_descendant_for_byte_range(*start, *end)?;
                let enclosing = enclosing_type_fqn(&init, self.bytes, self.symbols);
                self.infer_expr(&init, enclosing.as_deref())
            }
            LocalTy::IterElem(start, end) => {
                let iter = self.root.named_descendant_for_byte_range(*start, *end)?;
                let enclosing = enclosing_type_fqn(&iter, self.bytes, self.symbols);
                let ty = self.infer_expr(&iter, enclosing.as_deref())?;
                // `List<Foo>` / `Iterable<Foo>` → `Foo`. A RAW collection, a multi-argument type or
                // an array (arrays aren't modelled in Phase-1) yields nothing rather than a guess.
                (ty.type_args.len() == 1).then(|| ty.type_args[0].clone())
            }
        }
    }

    // ---- type text -> TypeRef ----

    /// Resolve a written type text (`Map<String,Object>`, `HttpServletRequest`, `long`) to a
    /// `TypeRef` with binary names, using imports + the resolver.
    ///
    /// [`parse_type_text`] answers a MEMBER-model question and so returns `None` for a primitive —
    /// there is nothing to look a member up on. Inference asks a different one: `long` is the whole
    /// answer for `long l`, and returning nothing here left every primitive **local and parameter**
    /// untyped. Fields were spared (their type comes back off the index, which does carry `long`),
    /// which is what made the hole so hard to see from a test: the same check fired on `int i = f;`
    /// and stayed silent on `int i = l;`. Everything downstream that needs the static type of a
    /// primitive — the lossy-narrowing check, the condition-type check, argument types — was blind
    /// on the single commonest shape in Java.
    fn resolve_type_text(&self, text: &str) -> Option<TypeRef> {
        if let Some(hit) = self.cache.type_text.borrow().get(text) {
            return hit.clone();
        }
        let result = match parse_type_text(text) {
            Some(parsed) => Some(self.to_binary_ref(&parsed)),
            // A bare primitive, and only a bare one: `int[]` keeps answering `None` because the rest
            // of the walk does not model arrays (`array_access` already infers nothing), and typing
            // the declaration without typing its uses would be a half-truth in the cache.
            None => primitive_type_text(text).map(TypeRef::simple),
        };
        self.cache
            .type_text
            .borrow_mut()
            .insert(text.to_string(), result.clone());
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
        // The unbound spelling is kept deliberately: a `TypeRef` carries it so the walk can go on
        // and `is_resolved` decides later whether anything may be concluded from it. Taking the
        // TEXT rather than a slashed guess is what makes that later test honest — an unbound
        // `Outer.Nested` stays dotted, and a dotted name is visibly not a binary one.
        crate::typename::resolve_written_type(simple, self)
            .text()
            .to_string()
    }

    /// [`Self::simple_to_binary`] for the callers that must not act on a guess.
    fn simple_to_binary_resolved(&self, simple: &str) -> Option<String> {
        crate::typename::resolve_written_type(simple, self).resolved()
    }
}

/// The inference walk's view of what binds a simple type name: the file's imports, the types the
/// file itself declares, its package, then the resolver.
///
/// The ORDER is Java's and the shape of the lookup is shared — see [`crate::typename`], which
/// exists because three copies of this had drifted and carried the same bug.
impl crate::typename::NameScope for Ctx<'_> {
    fn simple(&self, simple: &str) -> Option<String> {
        for imp in &self.symbols.imports {
            if imp.simple_name() == Some(simple) {
                return Some(imp.path.replace('.', "/"));
            }
        }
        // A type declared in THIS file (or a nested type of it) is authoritative: its FQN
        // comes straight off the extracted symbols, so a local of a same-file type resolves
        // even when the resolver's simple→binary hints weren't seeded for it yet (e.g. a
        // freshly-added type before the next full reindex).
        if let Some(td) = self.symbols.types.iter().find(|t| t.name == simple) {
            return Some(td.fqn.replace('.', "/"));
        }
        // A type in the file's OWN package is in scope without an import. Resolve it to its exact
        // binary (`com/acme/C`, a unique key) BEFORE the resolver's flat simple-name lookup, which
        // collapses same-simple-name types across packages and could otherwise pick the wrong `C`
        // (mis-typing every `C` receiver in the file, and downstream falsely flagging its members).
        if let Some(pkg) = self.symbols.package.as_deref() {
            if !pkg.is_empty() {
                let candidate = format!("{}/{}", pkg.replace('.', "/"), simple);
                if self.resolver.members_of(&candidate).is_some() {
                    return Some(candidate);
                }
            }
        }
        // A member type INHERITED from a supertype — `Entry` inside a `Map` implementation. Ahead of
        // the resolver's flat simple-name lookup, which collapses same-named nested types across the
        // whole project and would answer with an unrelated one.
        if let Some(bn) =
            crate::typename::inherited_member_type(self.symbols, self.resolver, simple)
        {
            return Some(bn);
        }
        if let Some(bn) = self
            .resolver
            .resolve_simple_name(simple, &self.symbols.imports)
        {
            return Some(bn);
        }
        crate::typename::java_lang_implicit(simple)
    }

    fn is_type(&self, binary: &str) -> bool {
        self.resolver.members_of(binary).is_some()
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
            // `obj::member` — the same question as `obj.member(…)`, and it used to get no answer
            // at all: the grammar gives a method reference's parts no field names, so neither arm
            // above matched and every reference qualified by an EXPRESSION (`this::run`,
            // `service::fetch`) was invisible to the index. A rename then rewrote the declaration
            // and left every `::` site spelling a method that no longer exists.
            //
            // Only a type-qualified reference survived, through the caller's separate
            // "resolve the text as a type" fallback — which is why the gap looked partial.
            "method_reference" => {
                let mut walk = n.walk();
                let children: Vec<Node<'t>> = n.named_children(&mut walk).collect();
                if let (Some(qualifier), Some(name)) = (children.first(), children.last()) {
                    // `Foo::new` has one named child; the qualifier would BE the "name".
                    if qualifier.id() != name.id()
                        && name.start_byte() <= byte_offset
                        && byte_offset <= name.end_byte()
                    {
                        return Some(*qualifier);
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
///
/// ## An anonymous class body is a type of its own
///
/// `new Foo<>() { private Bar b; … this.b … }` declares a type with no name in the source, and this
/// used to answer `None` for anything inside it — on the grounds that its members were not indexed,
/// so `this.b` would be looked up on the enclosing class, which has no `b`, and correct code would
/// be reported as a missing field.
///
/// They ARE indexed now: the extractor files an anonymous body under the ordinal javac gives it
/// (`p.Outer.1`), members and all. So the honest answer is that ordinal, and `None` became the
/// costly one — with no enclosing type, a bare name written inside an anonymous body could not be
/// typed at all, and neither could anything read off it. Guava writes
/// `upperBoundWindow.upperBound.isLessThan(…)` inside an anonymous iterator; the whole chain went
/// untyped and a rename of `isLessThan` could not see the call.
///
/// Derived exactly as the reference index derives it (`anonymous_type_name` + the outer FQN), so
/// the two cannot disagree about what an anonymous type is called — which would be worse than
/// either answer, since a key nothing looks up is silence that looks like success.
/// The [`TypeDecl`](crate::symbols::TypeDecl) the extractor produced for the type declaration at
/// `node`, found by POSITION.
///
/// Not by simple name, which is what every caller used to do. A file may declare several types with
/// the same simple name — guava's `Maps.java` declares two `KeySet`, `ConcurrentHashMultiset.java`
/// two `EntrySet` — and a search by name answers with whichever the extractor listed first. Every
/// consumer of that answer then reads one class's declared members as another's: eight of guava's
/// classes were reported for not implementing methods they declare on themselves, and the FQN
/// recovery below handed out the wrong owner for anything written inside them.
///
/// `node` is the type DECLARATION node. The extractor records each type's whole span, so the match
/// is an equality on the start offset — exact, with no containment ambiguity between a nested type
/// and the type it is nested in.
pub fn type_decl_at<'s>(node: &Node, symbols: &'s FileSymbols) -> Option<&'s crate::symbols::TypeDecl> {
    let start = node.start_byte();
    symbols
        .types
        .iter()
        .find(|t| t.span.map(|s| s.start) == Some(start))
}

pub fn enclosing_type_fqn(node: &Node, bytes: &[u8], symbols: &FileSymbols) -> Option<String> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        // An anonymous class body — see the doc above. Must be tested BEFORE the declaration arm,
        // which is automatic walking bottom-up: the body is always crossed first.
        if crate::symbols::is_anonymous_body(&n) {
            let name = crate::symbols::anonymous_type_name(&n, bytes)?;
            let outer = n
                .parent()
                .and_then(|p| enclosing_type_fqn(&p, bytes, symbols));
            return Some(match outer {
                Some(outer) => format!("{outer}.{name}"),
                None => name,
            });
        }
        if matches!(
            n.kind(),
            // `record_declaration` belongs here too: a record body is an ordinary type body, and
            // leaving it out made `this` inside a record mean the record's ENCLOSING type.
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
        ) {
            // Recover the FQN from the extracted TypeDecl at this exact position. By NAME, this
            // answered with whichever same-named type the file listed first — see `type_decl_at`.
            if let Some(td) = type_decl_at(&n, symbols) {
                return Some(td.fqn.clone());
            }
            let name = n
                .child_by_field_name("name")
                .and_then(|x| node_text(&x, bytes))?;
            return Some(name);
        }
        cur = n.parent();
    }
    None
}

/// Whether `node` is exactly the `field`-named child of `parent` (identity, not text).
fn field_is(parent: &Node, field: &str, node: &Node) -> bool {
    parent.child_by_field_name(field).map(|c| c.id()) == Some(node.id())
}

/// The operand of a `!` — `!(o instanceof Foo f)` → `(o instanceof Foo f)`. `None` for anything else.
fn unwrap_negation<'t>(cond: &Node<'t>) -> Option<Node<'t>> {
    let inner = if cond.kind() == "parenthesized_expression" {
        cond.named_child(0)?
    } else {
        *cond
    };
    if inner.kind() != "unary_expression" {
        return None;
    }
    let op = inner.child_by_field_name("operator")?;
    (op.kind() == "!")
        .then(|| inner.child_by_field_name("operand"))
        .flatten()
}

/// Whether a statement always completes abruptly (`return` / `throw` / `break` / `continue`), or is a
/// block whose last statement does. Used to recognise the guard shape
/// `if (!(o instanceof Foo f)) return;`, after which `f` is definitely bound.
fn completes_abruptly(stmt: &Node) -> bool {
    match stmt.kind() {
        "return_statement" | "throw_statement" | "break_statement" | "continue_statement" => true,
        // Comments are named nodes too — a trailing `// done` must not hide the `return`.
        "block" => {
            let mut w = stmt.walk();
            stmt.named_children(&mut w)
                .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
                .last()
                .is_some_and(|last| completes_abruptly(&last))
        }
        _ => false,
    }
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

/// Whether `m`'s parameter list admits a call passing `argc` arguments: an exact arity, or a
/// trailing array (the resolved shape of `T...`) soaking up any count from the fixed prefix on.
///
/// The ONE rule for "could this call have bound to this method". The inference walk narrows an
/// overload set with it; hover picks which overload's signature to show with it. Before they shared
/// it, hover took the first method of the name it found in the hierarchy — so `o.customer("x")`
/// answered with the no-argument getter.
pub fn method_admits_argc(m: &Member, argc: usize) -> bool {
    arity_admits(m.params.len(), last_is_array(m), argc)
}

/// Whether a member's last parameter is an array type (`T[]`) — the resolved shape of a `T...` varargs
/// parameter (no explicit flag on the seam).
fn last_is_array(m: &Member) -> bool {
    m.params
        .last()
        .is_some_and(|p| p.binary_name.ends_with("[]"))
}

/// The single return type shared by every member in `members`, or `None` when the set is empty or its
/// members return more than one distinct type — the "unique return" gate that keeps an ambiguous
/// overload UNRESOLVED (never guessed) while resolving the common single-return case.
/// The one member in `members`, or `None` when there are several — "which method was called" is only
/// answerable when the field narrowed to one, and a caller that reads parameters off the answer has
/// to know the difference.
fn sole<'m>(members: &[&'m Member]) -> Option<&'m Member> {
    match members {
        [only] => Some(only),
        _ => None,
    }
}

/// Substitute every `(variable, type)` binding into `ty`, at any depth.
fn apply_bindings(ty: &TypeRef, bindings: &[(String, TypeRef)]) -> TypeRef {
    if let Some((_, bound)) = bindings.iter().find(|(v, _)| *v == ty.binary_name) {
        return bound.clone();
    }
    TypeRef {
        binary_name: ty.binary_name.clone(),
        type_args: ty.type_args.iter().map(|a| apply_bindings(a, bindings)).collect(),
    }
}

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
        assert_eq!(
            type_var_index("V", 1),
            Some(0),
            "Future<V>.get() → V even though V is 'second'"
        );
        assert_eq!(type_var_index("R", 1), Some(0));
    }

    #[test]
    fn two_arg_maps_conventional_roles() {
        // Map<K,V>: key→0, value→1. Pair<L,R>: left→0, right→1. Entry<K,V> likewise.
        assert_eq!(type_var_index("K", 2), Some(0));
        assert_eq!(type_var_index("V", 2), Some(1));
        assert_eq!(type_var_index("L", 2), Some(0));
        assert_eq!(
            type_var_index("R", 2),
            Some(1),
            "Pair<L,R>.getRight() → the 2nd arg"
        );
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
        assert!(
            !is_type_var("KK"),
            "two letters is not a type var (could be a class)"
        );
        assert!(!is_type_var("java/lang/String"));
        assert!(!is_type_var("Foo"));
        assert!(!is_type_var(""));
        assert!(!is_type_var("String"));
    }
}

/// Fixtures shared by the inference test modules: a fixed `binary → members` resolver plus the two
/// builders every suite needs. One copy, so a change to the seam's shape lands in one place.
#[cfg(test)]
mod test_support {
    use super::*;
    use crate::seam::ClassMembers;
    use crate::symbols::Import;
    use std::collections::HashMap;
    use std::sync::Arc;

    pub(super) struct MapResolver {
        pub(super) members: HashMap<String, ClassMembers>,
        pub(super) simple: HashMap<String, String>,
    }
    impl TypeResolver for MapResolver {
        fn members_of(&self, b: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(b).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, n: &str, _i: &[Import]) -> Option<String> {
            self.simple.get(n).cloned()
        }
    }

    /// A class with `methods`, extending `Object` — enough for the nominal walk.
    pub(super) fn cm(methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            superclass: Some("java/lang/Object".into()),
            interfaces: vec![],
            methods,
            fields: vec![],
            flags: Default::default(),
            type_params: vec![],
        }
    }

    pub(super) fn meth(name: &str, ret: &str, params: &[&str]) -> Member {
        Member::method(
            name,
            TypeRef::simple(ret),
            params.iter().map(|p| TypeRef::simple(*p)).collect(),
        )
    }

    /// The inferred binary type of the (unique) `expr` substring of `src`.
    pub(super) fn infer_call(src: &str, expr: &str, r: &MapResolver) -> Option<String> {
        let start = src.find(expr).expect("expression substring present");
        assert_eq!(
            src.matches(expr).count(),
            1,
            "`{expr}` must occur once in the fixture"
        );
        infer_expression_type(src, start, start + expr.len(), r).map(|t| t.binary_name)
    }
}

#[cfg(test)]
mod overload_tests {
    use super::test_support::*;
    use super::*;
    use std::collections::HashMap;

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
                    &[
                        "java/lang/Object",
                        "java/lang/StringBuffer",
                        "acme/FieldPosition",
                    ],
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

    #[test]
    fn a_static_call_resolves_through_its_type_receiver() {
        // The reported bug: `val x = Retriever.properties(svc)` showed no hover at all, because
        // the receiver is a TYPE and identifier resolution only ever looked for values.
        let r = resolver();
        // This expression is exactly what a `val`'s initializer re-descends into, so typing it
        // is what gives the `val` its type — and its tooltip.
        let src = "class C { void m() { val s = Fmt.format(null); } }";
        assert_eq!(
            infer_call(src, "Fmt.format(null)", &r).as_deref(),
            Some("java/lang/String")
        );
        // A qualified receiver works the same way.
        let qualified = "class C { void m() { Object o = acme.Fmt.format(null); } }";
        assert_eq!(
            infer_call(qualified, "acme.Fmt.format(null)", &r).as_deref(),
            Some("java/lang/String")
        );
    }

    #[test]
    fn a_value_receiver_still_wins_over_a_type_of_the_same_name() {
        // Java's obscuring rule: a variable named `Fmt` hides the type `Fmt`. The type fallback
        // must run only after value resolution has failed.
        let r = resolver();
        let src = "class C { void m(acme.Fmt Fmt) { Object o = Fmt.format(Fmt); } }";
        assert_eq!(
            infer_call(src, "Fmt.format(Fmt)", &r).as_deref(),
            Some("java/lang/String")
        );
    }

    #[test]
    fn a_lowercase_receiver_is_never_guessed_to_be_a_type() {
        // An unresolved lowercase name is a variable we could not type, not a class. Resolving it
        // as one would turn a clean miss into a confusing one.
        let r = resolver();
        let src = "class C { void m() { Object o = unknownThing.format(null); } }";
        assert_eq!(infer_call(src, "unknownThing.format(null)", &r), None);
    }

    #[test]
    fn arity_picks_the_one_arg_overload() {
        // `f.format(f)` is 1-arg → the `format(Object) -> String` overload, NOT the 3-arg
        // `format(…) -> StringBuffer` the old first-by-name pick returned (the reported false positive).
        let r = resolver();
        let src = "class C { void m(acme.Fmt f) { Object o = f.format(f); } }";
        assert_eq!(
            infer_call(src, "f.format(f)", &r).as_deref(),
            Some("java/lang/String")
        );
    }

    #[test]
    fn argument_type_breaks_a_same_arity_tie() {
        // Two 1-arg overloads with different returns; a `String` argument rules out the `int` one via
        // the primitive/reference clash → `pick(String) -> B`.
        let r = resolver();
        let src = "class C { void m(acme.Ov v) { Object o = v.pick(\"s\"); } }";
        assert_eq!(
            infer_call(src, "v.pick(\"s\")", &r).as_deref(),
            Some("acme/B")
        );
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
        assert!(
            !arity_admits(3, true, 1),
            "varargs needs at least the fixed prefix"
        );
    }

    #[test]
    fn primitive_reference_clash_is_definite_only() {
        assert!(
            primitive_ref_clash("int", "java/lang/String"),
            "int param vs String arg"
        );
        assert!(
            primitive_ref_clash("java/lang/String", "int"),
            "String param vs int arg"
        );
        assert!(
            !primitive_ref_clash("int", "java/lang/Integer"),
            "autoboxing bridge, not a clash"
        );
        assert!(
            !primitive_ref_clash("java/lang/Object", "java/lang/String"),
            "both references"
        );
        assert!(!primitive_ref_clash("int", "long"), "both primitives");
    }

    #[test]
    fn unique_return_requires_agreement() {
        let a = meth("x", "acme/A", &[]);
        let b = meth("x", "acme/B", &[]);
        let a2 = meth("x", "acme/A", &["int"]);
        assert_eq!(
            unique_return(&[&a, &a2]).map(|t| t.binary_name),
            Some("acme/A".to_string())
        );
        assert!(
            unique_return(&[&a, &b]).is_none(),
            "different returns → no unique"
        );
        assert!(unique_return(&[]).is_none(), "empty → none");
    }
}

/// Every construct that can legally shadow a FIELD of the enclosing class. Java allows a local
/// binding to reuse a field's name, and the bare identifier then means the BINDING — inference that
/// falls through to the field types the expression against the wrong class and the member checks
/// report a false "Cannot resolve method" on correct code (the reported bug: an enhanced-`for`
/// variable named like a field).
///
/// The fixture is deliberately adversarial: the field `impresa` is an `acme.Holder` whose members
/// return `acme/B`, every shadowing binding is an `acme.Impresa` whose members return `acme/A`. So
/// `acme/A` proves the binding won, `acme/B` proves the field leaked through.
#[cfg(test)]
mod shadowing_tests {
    use super::test_support::*;
    use super::*;
    use std::collections::HashMap;

    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".into(), cm(vec![]));
        members.insert("java/util/List".into(), cm(vec![]));
        members.insert("acme/A".into(), cm(vec![]));
        members.insert("acme/B".into(), cm(vec![]));
        members.insert("acme/Impresa".into(), cm(vec![meth("id", "acme/A", &[])]));
        members.insert("acme/Ex".into(), cm(vec![meth("id", "acme/A", &[])]));
        members.insert("acme/Ex2".into(), cm(vec![meth("id", "acme/A", &[])]));
        members.insert(
            "acme/Holder".into(),
            cm(vec![
                meth("id", "acme/B", &[]),
                // `List<Impresa>` — the element type an enhanced-`for` must peel for `var`.
                Member::method(
                    "list",
                    TypeRef {
                        binary_name: "java/util/List".into(),
                        type_args: vec![TypeRef::simple("acme/Impresa")],
                    },
                    vec![],
                ),
            ]),
        );
        MapResolver {
            members,
            simple: HashMap::new(),
        }
    }

    /// A class with a field `impresa` of the DECOY type, wrapping `body` in a method.
    fn with_field(body: &str) -> String {
        format!("class C {{ private acme.Holder impresa; void m(Object o) {{ {body} }} }}")
    }

    #[test]
    fn for_each_variable_shadows_a_same_named_field() {
        let r = resolver();
        let src =
            with_field("for (acme.Impresa impresa : impresa.list()) { Object x = impresa.id(); }");
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
        // …and the ITERABLE, evaluated before the variable exists, still means the field.
        assert_eq!(
            infer_call(&src, "impresa.list()", &r).as_deref(),
            Some("java/util/List")
        );
    }

    #[test]
    fn for_each_var_takes_the_element_type_not_the_collection() {
        let r = resolver();
        let src = with_field("for (var impresa : impresa.list()) { Object x = impresa.id(); }");
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    #[test]
    fn catch_parameter_shadows_a_same_named_field() {
        let r = resolver();
        let src = with_field("try { hop(); } catch (acme.Ex impresa) { Object x = impresa.id(); }");
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    #[test]
    fn multi_catch_union_stays_unresolved_rather_than_guessing() {
        // The binding's type is the LUB of `Ex | Ex2`, which we don't compute — and it must NOT fall
        // back to the shadowed field either. Unresolved keeps the member checks silent.
        let r = resolver();
        let src = with_field(
            "try { hop(); } catch (acme.Ex | acme.Ex2 impresa) { Object x = impresa.id(); }",
        );
        assert_eq!(infer_call(&src, "impresa.id()", &r), None);
    }

    #[test]
    fn classic_for_init_shadows_a_same_named_field() {
        let r = resolver();
        let src = with_field("for (acme.Impresa impresa = null; ; ) { Object x = impresa.id(); }");
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    #[test]
    fn try_with_resources_shadows_a_same_named_field() {
        let r = resolver();
        let src = with_field("try (acme.Impresa impresa = null) { Object x = impresa.id(); }");
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    #[test]
    fn local_declaration_shadows_a_same_named_field() {
        let r = resolver();
        let src = with_field("acme.Impresa impresa = null; Object x = impresa.id();");
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    #[test]
    fn instanceof_pattern_shadows_a_same_named_field_in_the_true_branch() {
        let r = resolver();
        let src = with_field("if (o instanceof acme.Impresa impresa) { Object x = impresa.id(); }");
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    #[test]
    fn instanceof_pattern_binds_in_the_rest_of_an_and_chain() {
        let r = resolver();
        let src =
            with_field("boolean b = o instanceof acme.Impresa impresa && impresa.id() != null;");
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    #[test]
    fn instanceof_guard_binds_after_an_early_return() {
        // The `if (!(x instanceof T v)) return;` idiom: `v` is definitely bound BELOW the guard.
        let r = resolver();
        let src = with_field(
            "if (!(o instanceof acme.Impresa impresa)) { return; } Object x = impresa.id();",
        );
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    #[test]
    fn instanceof_pattern_does_not_leak_into_the_else_branch() {
        // `impresa` is NOT in scope in the `else` — there it legitimately means the field, and typing
        // it as the pattern's type would invent a shadow Java never created.
        let r = resolver();
        let src = with_field(
            "if (o instanceof acme.Impresa impresa) { hop(); } else { Object x = impresa.id(); }",
        );
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/B")
        );
    }

    #[test]
    fn instanceof_pattern_does_not_leak_past_a_non_abrupt_if() {
        // No early exit → after the `if`, `impresa` is the field again.
        let r = resolver();
        let src = with_field(
            "if (!(o instanceof acme.Impresa impresa)) { hop(); } Object x = impresa.id();",
        );
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/B")
        );
    }

    #[test]
    fn switch_case_pattern_shadows_a_same_named_field() {
        let r = resolver();
        let arrow = with_field("switch (o) { case acme.Impresa impresa -> { Object x = impresa.id(); } default -> { hop(); } }");
        assert_eq!(
            infer_call(&arrow, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );

        let colon = with_field(
            "switch (o) { case acme.Impresa impresa: Object x = impresa.id(); break; default: break; }",
        );
        assert_eq!(
            infer_call(&colon, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    #[test]
    fn while_pattern_binds_in_the_loop_body() {
        let r = resolver();
        let src =
            with_field("while (o instanceof acme.Impresa impresa) { Object x = impresa.id(); }");
        assert_eq!(
            infer_call(&src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    /// The reported bug: a fluent builder reached through a wildcard. `get()` hands back
    /// `Spec<?>`; the decoder writes that wildcard as `Object`; substituting the self-type `S`
    /// with it made the chain an `Object`, and the next call — legal Java — was reported as
    /// "cannot resolve method … in Object". Unknown is the honest answer, and it is silent.
    #[test]
    fn a_wildcard_type_argument_does_not_collapse_a_chain_to_object() {
        let mut r = resolver();
        // `Spec<S>` with `S uri()` — the self-returning shape of Spring's RestClient specs.
        let mut spec = cm(vec![meth("uri", "S", &["java/lang/String"])]);
        spec.type_params = vec!["S".to_string()];
        r.members.insert("acme/Spec".into(), spec);
        r.members.insert("acme/Client".into(), {
            // `get()` returns `Spec<?>` — the wildcard arrives decoded as Object.
            cm(vec![Member::method(
                "get",
                TypeRef {
                    binary_name: "acme/Spec".into(),
                    type_args: vec![TypeRef::simple("java/lang/Object")],
                },
                vec![],
            )])
        });

        let src = "class C { void m(acme.Client c) { Object o = c.get().uri(\"/x\"); } }";
        assert_eq!(
            infer_call(src, "c.get().uri(\"/x\")", &r),
            None,
            "unresolved, NOT java/lang/Object — Object is what makes the next call a false error",
        );
    }

    #[test]
    fn method_parameter_shadows_a_same_named_field() {
        let r = resolver();
        let src = "class C { private acme.Holder impresa; void m(acme.Impresa impresa) { Object x = impresa.id(); } }";
        assert_eq!(
            infer_call(src, "impresa.id()", &r).as_deref(),
            Some("acme/A")
        );
    }

    #[test]
    fn untyped_lambda_parameter_shadows_without_guessing() {
        // A lambda parameter we can't target-type must still SHADOW the field: unresolved (silent),
        // never the field's type (which would flag `id()` against the wrong class).
        let r = resolver();
        let src = with_field("java.util.function.Consumer<Object> c = impresa -> impresa.id();");
        assert_eq!(infer_call(&src, "impresa.id()", &r), None);
    }
}

/// The depth cap ([`MAX_INFER_DEPTH`]): what it stops, and what it must not cost.
#[cfg(test)]
mod depth_tests {
    use super::test_support::*;
    use super::*;
    use std::collections::HashMap;

    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cm(vec![]));
        members.insert("java/lang/String".to_string(), cm(vec![]));
        MapResolver {
            members,
            simple: HashMap::new(),
        }
    }

    /// `"x" + "x" + …` nests one level per operand, so a machine-generated
    /// concatenation is thousands of levels deep. Without the cap this recursed
    /// until the stack ran out — and a stack overflow is not a catchable panic, so
    /// it took the whole backend process down with it.
    ///
    /// The assertion is that this **returns at all**: if the guard regresses, the
    /// test process aborts rather than failing.
    #[test]
    fn deep_concatenation_answers_instead_of_overflowing() {
        let chain = vec!["\"x\""; 3000].join(" + ");
        let src = format!("class A {{ void m() {{ String s = {chain}; }} }}");
        let start = src.find('"').expect("the chain is in the fixture");
        let _ = infer_expression_type(&src, start, start + chain.len(), &resolver());
    }

    /// The cap is a backstop, not a budget: code anyone might actually write still
    /// types. A twenty-piece concatenation is a long line, not a deep one.
    #[test]
    fn ordinary_concatenation_still_types() {
        let chain = vec!["\"x\""; 20].join(" + ");
        let src = format!("class A {{ void m() {{ String s = {chain}; }} }}");
        let start = src.find('"').expect("the chain is in the fixture");
        assert_eq!(
            infer_expression_type(&src, start, start + chain.len(), &resolver())
                .map(|t| t.binary_name),
            Some("java/lang/String".to_string()),
        );
    }

    /// The guard gives its level back when it goes out of scope — which is the whole
    /// reason it is a `Drop` type and not a decrement at the end of `infer_expr`,
    /// since that function leaves through a dozen `?` operators. A level that leaked
    /// would make the counter climb and inference go silent for the rest of the call.
    #[test]
    fn guard_releases_its_level_when_dropped() {
        let depth = Cell::new(1);
        {
            let _held = DepthGuard(&depth);
            assert_eq!(depth.get(), 1, "held for as long as the guard lives");
        }
        assert_eq!(depth.get(), 0, "released on the way out");
    }
}

/// A method-level type variable bound by the ARGUMENT that determines it — the static-factory shape
/// (`Optional.ofNullable(x)`) whose result the receiver alone can never explain.
#[cfg(test)]
mod method_type_var_tests {
    use super::test_support::*;
    use super::*;
    use crate::seam::ClassMembers;
    use std::collections::HashMap;

    /// `Opt` is `java.util.Optional` in miniature: a class variable `T`, a static factory
    /// `<T> Opt<T> ofNullable(T)` whose `T` is the METHOD's, and `orElse(T) -> T` whose `T` is the
    /// class's. `Repo.kind()` returns an enum-ish `acme/Kind`; `Box.of(T, String) -> Opt<T>` exists
    /// to prove a second parameter doesn't confuse the binding.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".into(), cm(vec![]));
        members.insert("java/lang/String".into(), cm(vec![]));
        members.insert("acme/Kind".into(), cm(vec![]));
        let opt = ClassMembers {
            superclass: Some("java/lang/Object".into()),
            interfaces: vec![],
            methods: vec![
                Member::method(
                    "ofNullable",
                    TypeRef { binary_name: "acme/Opt".into(), type_args: vec![TypeRef::simple("T")] },
                    vec![TypeRef::simple("T")],
                )
                .stat(),
                meth("orElse", "T", &["T"]),
            ],
            fields: vec![],
            flags: Default::default(),
            // The class declares `T` — which is exactly why a STATIC factory's `T` cannot be read
            // off the receiver: same spelling, different variable.
            type_params: vec!["T".into()],
        };
        members.insert("acme/Opt".into(), opt);
        members.insert("acme/Repo".into(), cm(vec![meth("kind", "acme/Kind", &[])]));
        let simple = [
            ("Opt", "acme/Opt"),
            ("Repo", "acme/Repo"),
            ("Kind", "acme/Kind"),
            ("String", "java/lang/String"),
            ("Object", "java/lang/Object"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// The reported miss: the chain's type is the ARGUMENT's, and without the binding it stayed the
    /// bare variable `T` — which every consumer reads as "unknown", so the return-type check that
    /// should have caught `Integer m() { return …; }` had nothing to compare.
    #[test]
    fn a_static_factorys_type_variable_comes_from_its_argument() {
        let src = "class C { Repo r; void m() { Opt.ofNullable(r.kind()).orElse(null); } }";
        assert_eq!(
            infer_call(src, "Opt.ofNullable(r.kind()).orElse(null)", &resolver()),
            Some("acme/Kind".to_string()),
        );
    }

    /// The intermediate step, on its own: `Opt<Kind>`, not `Opt<T>`.
    #[test]
    fn the_factory_call_itself_carries_the_bound_argument() {
        let src = "class C { Repo r; void m() { Opt.ofNullable(r.kind()); } }";
        let start = src.find("Opt.ofNullable(r.kind())").unwrap();
        let ty = infer_expression_type(src, start, start + "Opt.ofNullable(r.kind())".len(), &resolver())
            .expect("inferred");
        assert_eq!(ty.binary_name, "acme/Opt");
        assert_eq!(
            ty.type_args.first().map(|a| a.binary_name.as_str()),
            Some("acme/Kind"),
        );
    }

    /// An argument that types to nothing binds nothing — the variable stays open rather than being
    /// filled with a guess.
    #[test]
    fn an_untypeable_argument_binds_nothing() {
        let src = "class C { void m() { Opt.ofNullable(mystery()).orElse(null); } }";
        let got = infer_call(src, "Opt.ofNullable(mystery()).orElse(null)", &resolver());
        assert!(
            got.is_none() || got.as_deref() == Some("T"),
            "expected unresolved, got {got:?}"
        );
    }
}
