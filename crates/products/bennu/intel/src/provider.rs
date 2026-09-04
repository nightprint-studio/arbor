//! The [`IntelProvider`] trait + its two impl slots.
//!
//! One protocol for every language (docs §2). The Phase-0 skeleton defines the trait
//! and both impls' *shapes*; the bodies are stubs that return empty / unimplemented,
//! so `bennu-be` can wire the seam now and later waves fill the native engine in
//! (and, post-MVP, the LSP client).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bennu_index::prelude::SymbolKind;
use bennu_proto::prelude::{CompletionItem, Diagnostic};

use bennu_classpath::prelude::ClassSource;
use bennu_query::prelude::{ClasspathIndex, IndexResolver, JdkMemberIndex};

use crate::class_names::ClassNameIndex;

/// One project member (method / field) enumerated from the built symbol index, for the
/// index inspector's "members" list. A be-agnostic view: the be layer maps this onto its
/// wire `IndexEntry` (and resolves the declaring type's line off its own class cache).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMember {
    /// The member's simple name (`getOrder`, `count`).
    pub name: String,
    /// The owning type's binary name (slash form, `com/acme/Order`).
    pub owner_binary: String,
    /// The rendered member signature (`Order getOrder(long id)` / `int count`).
    pub signature: String,
    /// Absolute path (forward slashes) of the project source file declaring the member,
    /// or empty when the member carries no source location.
    pub file: String,
    /// `true` for a method, `false` for a field.
    pub is_method: bool,
}

/// The member to land on in a library source view — a method or field name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryMember {
    /// The member's simple name (`add`, `MAX_VALUE`).
    pub name: String,
    /// `true` for a field, `false` for a method.
    pub is_field: bool,
}

/// A go-to target resolved from a caret INSIDE a library/JDK source view: the binary name of the
/// type to open, plus (for a member access) the member to land on. Produced by
/// [`NativeJavaProvider::library_target_at`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryTarget {
    /// Binary name of the target type to open the source view of (`java/util/function/Supplier`).
    pub binary: String,
    /// The member to jump to within that type, or `None` for a plain type reference (jump to the
    /// type declaration).
    pub member: Option<LibraryMember>,
}

/// A location in a file — byte offset, matching the wire diagnostics (docs §3: byte
/// ranges, the FE maps them). Used by definition / references results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// Absolute path to the file.
    pub file: String,
    /// Start byte offset.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

/// A document symbol for the outline (docs §5 #16 outline / #9 "everywhere").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSymbol {
    pub name: String,
    /// Kind tag (`"class"` / `"method"` / `"field"` / …).
    pub kind: String,
    pub location: Location,
}

/// A file edit for rename / format results: replace `[start, end)` with `new_text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub new_text: String,
}

/// Render a [`bennu_java::prelude::TypeRef`] to a readable type string — the written-Java shape
/// rather than the binary name (`java/util/List<com/acme/Foo>` → `List<Foo>`). For the hover card
/// and the member signatures, which read it rather than write it.
///
/// The same rule as [`render_type_for_source`] with the import list thrown away, and deliberately
/// not a second spelling of it: a hover that said `Entry` where a declaration would say
/// `Map.Entry` is two answers about one type, and the one you are looking at is never the one you
/// are about to write.
fn render_type_ref(t: &bennu_java::prelude::TypeRef) -> String {
    render_type_for_source(t, &mut Vec::new())
}

/// The hover card for a local whose type could NOT be resolved.
///
/// The alternative is showing nothing, and showing nothing is the worst of the three possible
/// answers: it looks exactly like a broken tooltip, so a user learns "hover doesn't work on
/// `val`" from a case where the truth is "this one initializer didn't resolve". This card states
/// what is certain — that it is a local, and how it was written — and names the expression whose
/// type is missing, which is also the thing to report if it turns out to be a gap in the inference.
///
/// `None` only when the name has no local declaration in any enclosing scope (a field, a type — not
/// this function's business).
fn unresolved_local_hover(
    bytes: &[u8],
    node: tree_sitter::Node,
    name: &str,
) -> Option<crate::rename::HoverInfo> {
    let (written, initializer) = local_declaration_of(node, bytes, name)?;
    let doc = match initializer {
        Some(expr) => format!(
            "The type could not be inferred from the initializer — `{}`. \
             Hovering a part of the chain shows how far the inference gets.",
            summarize_expr(expr, bytes)
        ),
        None => "The type could not be inferred.".to_string(),
    };
    Some(crate::rename::HoverInfo {
        signature: format!("{written} {name}"),
        kind: "variable".to_string(),
        container: None,
        doc: Some(doc),
    })
}

/// How much of an initializer's shape a tooltip is allowed to spend.
const MAX_SHAPE_CHARS: usize = 110;

/// An initializer rendered as its **shape**: the calls that decide its type, with their arguments
/// elided (`repo.search(…).map(…).orElseGet(…)`).
///
/// This replaced "the first 120 characters of the source text", which on exactly the expressions
/// that defeat the inference is unreadable. A builder chain spends that whole budget inside its own
/// arguments — `…builder().applicativo(root.getId().getIdprg()).chiave1(exact_value(…` — and is cut
/// off *before* the `.map(…).orElseGet(…)` that actually determines the type, so the card ends up
/// quoting the least relevant part of the expression back at the reader. The shape fits on a line
/// and names the links that matter.
///
/// When even the shape is too long the **head** is dropped rather than the tail: the type comes out
/// of the last call, so that is the end worth keeping.
fn summarize_expr(node: tree_sitter::Node, bytes: &[u8]) -> String {
    fn text(node: tree_sitter::Node, bytes: &[u8]) -> String {
        node.utf8_text(bytes).unwrap_or("").to_string()
    }
    fn field(node: tree_sitter::Node, name: &str, bytes: &[u8]) -> Option<String> {
        node.child_by_field_name(name).map(|n| text(n, bytes))
    }
    /// `()` or `(…)` — whether a call takes arguments, never which ones.
    fn call_args(node: tree_sitter::Node) -> &'static str {
        match node.child_by_field_name("arguments") {
            Some(a) if a.named_child_count() > 0 => "(…)",
            _ => "()",
        }
    }

    // Walk down the receiver chain, collecting the links outermost-first.
    let mut links: Vec<String> = Vec::new();
    let mut current = node;
    loop {
        let link = match current.kind() {
            "method_invocation" => {
                format!(
                    "{}{}",
                    field(current, "name", bytes).unwrap_or_default(),
                    call_args(current)
                )
            }
            "field_access" => field(current, "field", bytes).unwrap_or_default(),
            _ => break,
        };
        links.push(link);
        match current.child_by_field_name("object") {
            Some(receiver) => current = receiver,
            None => break,
        }
    }
    links.reverse();

    // Whatever the chain stands on: a name, a `new`, or an expression this does not model — a
    // ternary, a lambda, a cast — which is simply shown, shortened.
    let base = match current.kind() {
        "object_creation_expression" => format!(
            "new {}{}",
            field(current, "type", bytes).unwrap_or_default(),
            call_args(current)
        ),
        "identifier"
        | "this"
        | "super"
        | "scoped_identifier"
        | "type_identifier"
        | "string_literal"
        | "decimal_integer_literal"
        | "null_literal" => text(current, bytes),
        _ => ellipsize(&text(current, bytes), 60),
    };

    let mut parts: Vec<String> = std::iter::once(base).chain(links).collect();
    let mut trimmed = false;
    while parts.len() > 1 && parts.join(".").chars().count() > MAX_SHAPE_CHARS {
        parts.remove(0);
        trimmed = true;
    }
    let joined = ellipsize(&parts.join("."), MAX_SHAPE_CHARS);
    if trimmed {
        format!("…{joined}")
    } else {
        joined
    }
}

/// The `(written type, initializer)` of the local named `name`, searched outwards from `node`
/// through the enclosing scopes. Covers the two forms that carry an inferred type: an ordinary
/// declaration and an enhanced-`for` variable.
fn local_declaration_of<'t>(
    node: tree_sitter::Node<'t>,
    bytes: &[u8],
    name: &str,
) -> Option<(String, Option<tree_sitter::Node<'t>>)> {
    fn text(n: &tree_sitter::Node, bytes: &[u8]) -> Option<String> {
        n.utf8_text(bytes).ok().map(|s| s.to_string())
    }
    /// `(type, value)` of `decl` when it declares `name`. The value comes back as a NODE: the
    /// caller renders its shape, which cannot be done from the flattened source text.
    fn declares<'t>(
        decl: &tree_sitter::Node<'t>,
        bytes: &[u8],
        name: &str,
    ) -> Option<(String, Option<tree_sitter::Node<'t>>)> {
        let declared = decl
            .child_by_field_name("type")
            .and_then(|t| text(&t, bytes))?;
        if decl.kind() == "enhanced_for_statement" {
            let n = decl
                .child_by_field_name("name")
                .and_then(|n| text(&n, bytes))?;
            return (n == name).then(|| (declared, decl.child_by_field_name("value")));
        }
        let mut w = decl.walk();
        for d in decl.named_children(&mut w) {
            if d.kind() != "variable_declarator" {
                continue;
            }
            if d.child_by_field_name("name")
                .and_then(|n| text(&n, bytes))
                .as_deref()
                == Some(name)
            {
                return Some((declared, d.child_by_field_name("value")));
            }
        }
        None
    }

    let mut scope = Some(node);
    while let Some(s) = scope {
        if matches!(
            s.kind(),
            "local_variable_declaration" | "enhanced_for_statement"
        ) {
            if let Some(hit) = declares(&s, bytes, name) {
                return Some(hit);
            }
        }
        let mut w = s.walk();
        let mut found = None;
        for c in s.named_children(&mut w) {
            if matches!(
                c.kind(),
                "local_variable_declaration" | "enhanced_for_statement"
            ) {
                if let Some(hit) = declares(&c, bytes, name) {
                    found = Some(hit);
                    break;
                }
            }
        }
        if found.is_some() {
            return found;
        }
        scope = s.parent();
    }
    None
}

/// Shorten `s` to `max` characters with an ellipsis — an initializer can be a whole chained
/// expression, and a tooltip is not the place to reproduce it in full.
fn ellipsize(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max).collect();
    format!("{cut}…")
}

/// Whether `text` even has the shape of a type name — a bare or dotted identifier.
///
/// The guard in front of [`NativeJavaProvider::type_named`]: its caller hands it the source text
/// of whatever a placeholder matched, which can be `a.b().c`, a string literal, or a whole block.
/// A resolver asked whether `"hello"` names a type has no honest way to say "that is not even a
/// question", so the shape is checked before it is asked.
fn reads_as_type_name(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    text.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.')
        && !text.contains("..")
        && !text.ends_with('.')
}

/// Render a decompiled-from-bytecode **Java stub** (signatures only) for a type's members. A readable
/// approximation of the class file's API surface — package, the type declaration with its
/// `extends`/`implements`, then each field and method signature. Method bodies don't exist in
/// bytecode, so concrete methods get a `throw`-ing placeholder body (keeping the stub valid Java);
/// interface/abstract methods stay bodyless. A header comment marks it as generated.
fn render_stub(binary: &str, cm: &bennu_java::prelude::ClassMembers) -> String {
    use bennu_java::prelude::{MemberKind, Visibility};
    let simple = binary.rsplit(['/', '$']).next().unwrap_or(binary);
    let kind = if cm.flags.is_annotation {
        "@interface"
    } else if cm.flags.is_interface {
        "interface"
    } else if cm.flags.is_enum {
        "enum"
    } else {
        "class"
    };
    let vis = |v: Visibility| match v {
        Visibility::Public => "public ",
        Visibility::Protected => "protected ",
        Visibility::Private => "private ",
        Visibility::Package => "",
    };
    let type_name = |b: &str| b.rsplit(['/', '$']).next().unwrap_or(b).to_string();

    let mut s = String::new();
    s.push_str(
        "// Decompiled from bytecode — no source attached. Signatures only (method bodies\n",
    );
    s.push_str("// are not present in a .class file). Generated by Bennu.\n\n");
    if let Some((pkg, _)) = binary.rsplit_once('/') {
        s.push_str(&format!("package {};\n\n", pkg.replace('/', ".")));
    }
    s.push_str("public ");
    s.push_str(kind);
    s.push(' ');
    s.push_str(simple);
    // Class-level type parameters (`class Optional<T>`, `interface Map<K, V>`) — names only (the seam
    // carries no bounds for a class's own parameters).
    if !cm.type_params.is_empty() {
        s.push_str(&format!("<{}>", cm.type_params.join(", ")));
    }
    if !cm.flags.is_interface {
        if let Some(sc) = cm
            .superclass
            .as_deref()
            .filter(|sc| *sc != "java/lang/Object")
        {
            s.push_str(&format!(" extends {}", type_name(sc)));
        }
    }
    if !cm.interfaces.is_empty() {
        let word = if cm.flags.is_interface {
            "extends"
        } else {
            "implements"
        };
        let list: Vec<String> = cm.interfaces.iter().map(|i| type_name(i)).collect();
        s.push_str(&format!(" {word} {}", list.join(", ")));
    }
    s.push_str(" {\n");

    for f in cm.fields.iter().filter(|f| f.kind == MemberKind::Field) {
        s.push_str(&format!(
            "    {}{}{} {};\n",
            vis(f.visibility),
            if f.is_static { "static " } else { "" },
            render_type_ref(&f.return_type),
            f.name,
        ));
    }
    if !cm.fields.is_empty() && !cm.methods.is_empty() {
        s.push('\n');
    }
    for m in cm.methods.iter().filter(|m| m.kind == MemberKind::Method) {
        let is_ctor = m.name == "<init>";
        // Prefer the bytecode GENERIC `Signature` (method type parameters `<X extends Throwable>`,
        // wildcards `Supplier<? extends X>`, and a type-variable `throws X`) — the IntelliJ-style
        // shape. Fall back to the erased seam fields when the method carries no generic signature
        // (a plain descriptor either renders identically here or fails to parse → this branch).
        let core = generic_method_core(&m.raw_signature, &m.name, is_ctor.then_some(simple))
            .unwrap_or_else(|| {
                let params: Vec<String> = m
                    .params
                    .iter()
                    .enumerate()
                    .map(|(i, p)| format!("{} arg{i}", render_type_ref(p)))
                    .collect();
                let throws = if m.throws.is_empty() {
                    String::new()
                } else {
                    let list: Vec<String> = m.throws.iter().map(|t| type_name(t)).collect();
                    format!(" throws {}", list.join(", "))
                };
                // A constructor is `<init>` in bytecode → render it as `Simple(...)`.
                let (ret, name) = if is_ctor {
                    (String::new(), simple.to_string())
                } else {
                    (
                        format!("{} ", render_type_ref(&m.return_type)),
                        m.name.clone(),
                    )
                };
                format!("{ret}{name}({}){throws}", params.join(", "))
            });
        // Interface/abstract methods have no body; concrete ones get a placeholder so the stub parses.
        let body = if cm.flags.is_interface || m.is_abstract {
            ";".to_string()
        } else {
            " { throw new RuntimeException(\"compiled code\"); }".to_string()
        };
        s.push_str(&format!(
            "    {}{}{}{}\n",
            vis(m.visibility),
            if m.is_static { "static " } else { "" },
            core,
            body,
        ));
    }
    s.push_str("}\n");
    s
}

/// The generic method "core" — `<TypeParams> Ret name(Params) throws X` (no modifiers / body) —
/// decoded from a method's bytecode `Signature` (carried in `raw_signature`). `None` when there's no
/// generic signature to decode (a plain erased descriptor may still parse, in which case it renders
/// identically to the erased path). `ctor_simple` is `Some(simpleClassName)` for a `<init>` so it
/// renders as `Simple(...)` without a return type. This is what makes the decompiled stub match
/// IntelliJ's `<X extends Throwable> T orElseThrow(Supplier<? extends X> arg0) throws X` instead of the
/// erased `T orElseThrow(Supplier<X> arg0) throws Throwable`.
fn generic_method_core(
    raw_signature: &str,
    name: &str,
    ctor_simple: Option<&str>,
) -> Option<String> {
    let ms = bennu_classpath::prelude::parse_method_signature(raw_signature).ok()?;
    let type_params = if ms.type_params.is_empty() {
        String::new()
    } else {
        let ps: Vec<String> = ms.type_params.iter().map(render_sig_type_param).collect();
        format!("<{}> ", ps.join(", "))
    };
    let params: Vec<String> = ms
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{} arg{i}", render_sig_type(p)))
        .collect();
    let throws = if ms.throws.is_empty() {
        String::new()
    } else {
        let ts: Vec<String> = ms.throws.iter().map(render_sig_type).collect();
        format!(" throws {}", ts.join(", "))
    };
    let head = match ctor_simple {
        Some(simple) => format!("{type_params}{simple}"),
        None => format!("{type_params}{} {name}", render_sig_type(&ms.result)),
    };
    Some(format!("{head}({}){throws}", params.join(", ")))
}

/// Render a decoded generic-signature type to readable simple-name Java (`Supplier<? extends X>`,
/// `List<String>`, `T`, `int[]`). Simple names only — a stub reads cleaner than fully-qualified.
fn render_sig_type(t: &bennu_classpath::prelude::TypeSig) -> String {
    use bennu_classpath::prelude::TypeSig;
    match t {
        TypeSig::Base(c) => match c {
            'I' => "int",
            'J' => "long",
            'S' => "short",
            'B' => "byte",
            'C' => "char",
            'Z' => "boolean",
            'F' => "float",
            'D' => "double",
            _ => "Object",
        }
        .to_string(),
        TypeSig::Void => "void".to_string(),
        TypeSig::TypeVar(n) => n.clone(),
        TypeSig::Array(inner) => format!("{}[]", render_sig_type(inner)),
        TypeSig::Class(ct) => {
            let mut s = ct.name.rsplit('.').next().unwrap_or(&ct.name).to_string();
            if !ct.args.is_empty() {
                let a: Vec<String> = ct.args.iter().map(render_sig_arg).collect();
                s.push_str(&format!("<{}>", a.join(", ")));
            }
            for (iname, iargs) in &ct.inners {
                s.push('.');
                s.push_str(iname);
                if !iargs.is_empty() {
                    let a: Vec<String> = iargs.iter().map(render_sig_arg).collect();
                    s.push_str(&format!("<{}>", a.join(", ")));
                }
            }
            s
        }
    }
}

/// Render one `<...>` type argument (`?`, `? extends X`, `? super X`, or an exact type).
fn render_sig_arg(a: &bennu_classpath::prelude::TypeArg) -> String {
    use bennu_classpath::prelude::TypeArg;
    match a {
        TypeArg::Unbounded => "?".to_string(),
        TypeArg::Extends(t) => format!("? extends {}", render_sig_type(t)),
        TypeArg::Super(t) => format!("? super {}", render_sig_type(t)),
        TypeArg::Exact(t) => render_sig_type(t),
    }
}

/// Render a method/class type parameter (`X extends Throwable`, `T`), suppressing a vacuous
/// `extends Object` bound.
fn render_sig_type_param(tp: &bennu_classpath::prelude::TypeParam) -> String {
    let mut bounds: Vec<String> = Vec::new();
    if let Some(cb) = &tp.class_bound {
        let s = render_sig_type(cb);
        if s != "Object" {
            bounds.push(s);
        }
    }
    for ib in &tp.interface_bounds {
        bounds.push(render_sig_type(ib));
    }
    if bounds.is_empty() {
        tp.name.clone()
    } else {
        format!("{} extends {}", tp.name, bounds.join(" & "))
    }
}

/// Errors a provider can return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntelError {
    /// The capability isn't implemented by this provider (the predisposed LSP slot,
    /// and any Phase-0 stub method that isn't a benign empty answer).
    Unimplemented(&'static str),
    /// A provider-specific failure (index miss, transport error).
    Provider(String),
}

impl std::fmt::Display for IntelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntelError::Unimplemented(what) => write!(f, "intel: {what} not implemented"),
            IntelError::Provider(e) => write!(f, "intel: {e}"),
        }
    }
}

impl std::error::Error for IntelError {}

/// A request position: a file + a byte offset into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub file: String,
    pub offset: usize,
}

/// The single code-intel protocol the FE speaks, for every language (docs §2). Java
/// binds [`NativeJavaProvider`]; Rust will bind [`LspClientProvider`].
///
/// The full capability set (completion / hover / definition / references /
/// diagnostics / rename / format / symbols) is declared now so the seam is complete;
/// Phase-0 impls stub the semantic ones and return empty for the list-shaped ones.
pub trait IntelProvider: Send + Sync {
    /// Completion candidates at a position (docs §5 #4). `source` is the live editor buffer when the
    /// caller has it — the caret `offset` is in ITS coordinates (the just-typed, unsaved `.` lives
    /// only there), so completion MUST parse that text, not the stale on-disk file. `None` falls back
    /// to reading the file from disk (a programmatic query with no buffer). Phase-0 native impl → `[]`.
    fn completion(
        &self,
        at: &Position,
        source: Option<&str>,
    ) -> Result<Vec<CompletionItem>, IntelError>;

    /// Hover documentation / type at a position.
    fn hover(&self, at: &Position) -> Result<Option<String>, IntelError>;

    /// Go-to-definition target(s) (docs §5 #8).
    fn definition(&self, at: &Position) -> Result<Vec<Location>, IntelError>;

    /// Find-usages / references (docs §5 #7).
    fn references(&self, at: &Position) -> Result<Vec<Location>, IntelError>;

    /// Diagnostics for a file (docs §5 #2). Phase-0 native impl → `[]`.
    fn diagnostics(&self, file: &str) -> Result<Vec<Diagnostic>, IntelError>;

    /// Rename the symbol at a position to `new_name`, returning the edits (docs §5
    /// #10–12). Domain-aware for Java (also the `class="…"` in struts.xml — docs §5).
    fn rename(&self, at: &Position, new_name: &str) -> Result<Vec<TextEdit>, IntelError>;

    /// Format a whole file, returning the edits (docs §5 #20).
    fn format(&self, file: &str) -> Result<Vec<TextEdit>, IntelError>;

    /// The document symbols of a file, for the outline (docs §5 #16).
    fn symbols(&self, file: &str) -> Result<Vec<DocumentSymbol>, IntelError>;
}

/// The MVP provider: native, index-backed Java intel.
///
/// Phase 1 implements **member-access completion** end to end: it holds a
/// [`IndexResolver`] over the built project index + the JDK member index, infers the
/// receiver type at the caret (`bennu-java`), walks its members (superclass +
/// interfaces), and prefix-filters. Hover / definition / references / rename / format
/// stay stubbed until later waves.
///
/// A provider with **no resolver** (constructed via [`new`](Self::new), e.g. before a
/// project is opened / while the index is still building) answers completion with the
/// benign empty list — never an error — so the FE degrades gracefully.
#[derive(Default)]
pub struct NativeJavaProvider {
    /// The completion resolver: `Some` once a project index is built + the classpath (JDK, plus the
    /// project's dependency jars when resolvable) is available; `None` for the empty (pre-index)
    /// provider.
    resolver: Option<Arc<IndexResolver<ClasspathIndex>>>,
    /// The same project index over a JDK-ONLY classpath — what the reference walk resolves against.
    /// `None` for the pre-index provider. See [`Self::walk_resolver`].
    walk_resolver: Option<Arc<IndexResolver<ClasspathIndex>>>,
    /// Simple type name → importable FQNs (JDK + dependency + project), for the "Import class"
    /// intention. Empty for the pre-index provider.
    class_names: ClassNameIndex,
    /// The JDK's `.java` source archive (`src.zip`), when the resolved JDK ships one. Lets
    /// [`jdk_source_text`](Self::jdk_source_text) serve the REAL source (method bodies, locals,
    /// lambdas) for a JDK type instead of a signatures-only stub. `None` on a bare-JRE / no-sources
    /// install (→ stub) and on the pre-index provider.
    jdk_sources: Option<bennu_classpath::prelude::JavaSourceZip>,
}

impl std::fmt::Debug for NativeJavaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeJavaProvider")
            .field("has_resolver", &self.resolver.is_some())
            .finish()
    }
}

impl NativeJavaProvider {
    /// Construct the empty native provider (no index yet). Completion returns `[]`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a provider backed by a resolver over a built project index + the classpath
    /// (JDK + optional dependency) member index — the Phase-1 completion path. The class-name index
    /// (for "Import class") is empty here; [`for_project`](Self::for_project) populates it.
    pub fn with_resolver(resolver: IndexResolver<ClasspathIndex>) -> Self {
        Self {
            resolver: Some(Arc::new(resolver)),
            ..Default::default()
        }
    }

    /// Candidate importable FQNs (dotted, sorted) for a simple type name — the "Import class"
    /// intention's picker list. Empty for the pre-index provider or an unknown name.
    pub fn import_candidates(&self, simple: &str) -> &[String] {
        self.class_names.candidates(simple)
    }

    /// Whether `binary` names a PROJECT source type (not the JDK / a dependency). Used by the
    /// incremental re-index to resolve a wildcard-imported supertype/return/parameter to the exact
    /// package when a simple name collides across packages. `false` for the pre-index provider.
    pub fn is_project_type(&self, binary: &str) -> bool {
        use bennu_java::prelude::TypeResolver; // brings `is_project_type` into scope
        self.resolver
            .as_ref()
            .is_some_and(|r| r.is_project_type(binary))
    }

    /// Type-name completion candidates at `offset` in `text`: distinct simple type names from the
    /// class-name index whose name starts with the capitalised prefix under the caret. Empty unless
    /// the caret is on a bare identifier (NOT after a `.`) whose first char is uppercase.
    fn type_completions(&self, text: &str, offset: usize) -> Vec<CompletionItem> {
        const MAX: usize = 50;
        let (ident_start, prefix) = ident_prefix(text, offset);
        // A type reference starts with an uppercase letter; requiring it keeps the list focused and
        // avoids firing on a variable / method prefix (which member completion, not this, serves).
        //
        // Only the FIRST letter is held to it. What follows is matched by
        // [`ClassNameIndex::matches_for_prefix`], which also answers to the wrong case and to the
        // camel humps — `SBA` finds `SpringBootApplication`, which is how a name you already know
        // is actually reached for.
        if prefix.is_empty() || !prefix.starts_with(|c: char| c.is_ascii_uppercase()) {
            return Vec::new();
        }
        // `recv.Prefix` is a member access, not a type reference — leave it to member completion.
        if is_member_access(text, ident_start) {
            return Vec::new();
        }
        self.class_names
            .matches_for_prefix(&prefix, MAX)
            .into_iter()
            .map(|simple| {
                let candidates = self.class_names.candidates(simple);
                CompletionItem {
                    label: simple.to_string(),
                    kind: "class".to_string(),
                    detail: type_detail(candidates),
                    // Auto-import ONLY when unambiguous — a single candidate that isn't `java.lang`
                    // (which needs no import). Ambiguous names (several packages) are left to the
                    // Alt+Enter picker, so we never silently import the wrong `List`.
                    auto_import: single_import_candidate(candidates),
                    ..Default::default()
                }
            })
            .collect()
    }

    /// Completions for a **qualified** name — the segment after the last dot of a dotted path.
    ///
    /// Two carets, one answer, because they are the same question asked in two places:
    ///
    /// - `import org.springframework.b|`, where nothing was offered at all. An import is the one
    ///   line in a Java file written entirely in fully-qualified names, and it was the only kind
    ///   of name completion could not help with — you had to know it already, which is exactly
    ///   what an editor is for;
    /// - `org.springframework.boot.Sprin|` written out in code, where member completion cannot
    ///   help either: the receiver is a package, and a package is not a value with members.
    ///
    /// One segment at a time (see [`ClassNameIndex::segments_under`]): `import org.|` offers
    /// `springframework`, not every class beneath it. That is also what the editor can insert
    /// without rewriting the line — the token it replaces is the word under the caret, and a
    /// whole dotted name pasted there would be appended to the qualifier already written.
    ///
    /// Outside an `import`, a bare word with no qualifier is left to
    /// [`type_completions`](Self::type_completions): offering `javax` where a class name is being
    /// written would be answering a question nobody asked.
    fn qualified_completions(&self, text: &str, offset: usize) -> Vec<CompletionItem> {
        const MAX: usize = 50;
        let (ident_start, typed) = ident_prefix(text, offset);
        let qualifier = dotted_qualifier(text, ident_start);
        if qualifier.is_empty() && !in_import_statement(text, ident_start) {
            return Vec::new();
        }
        self.class_names
            .segments_under(&qualifier, &typed, MAX)
            .into_iter()
            .map(|seg| CompletionItem {
                label: seg.name,
                kind: if seg.is_class { "class" } else { "package" }.to_string(),
                // The full name a class row lands on. A package row says nothing extra: its own
                // label plus the qualifier already on screen is the whole of what it is.
                detail: seg.fqn,
                // Never an auto-import: the name being written IS the import, or is already
                // qualified at the point of use.
                ..Default::default()
            })
            .collect()
    }

    /// How many distinct type names completion can offer — the JDK's, every dependency jar's and
    /// the project's own.
    ///
    /// Reported by the index inspector because it is the one number that separates "Bennu does not
    /// complete my library classes" from "Bennu never loaded them": the two look identical from
    /// the popup, and only one of them is about completion.
    pub fn class_name_count(&self) -> usize {
        self.class_names.len()
    }

    /// Build a provider for a project: open the persisted index at `index_dir`, resolve
    /// the JDK for `jdk_version` (`"1.8"` / `"8"` / `"21"` / …), and seed the project's
    /// own declared simple names. `Err` when the index can't be opened or the JDK isn't
    /// installed — the caller then serves the empty provider.
    ///
    /// `jdk_index_path` (when `Some`) makes the JDK member index **persistent**: it loads the
    /// shared, cross-session memo from that path and writes warmed classes back, so a JDK class is
    /// parsed from bytecode at most once ever. The be layer keys the path by the resolved JDK.
    ///
    /// `deps` (when `Some`) adds the project's **dependency tier**: a `(dep-jars source, per-project
    /// memo path)` pair the be layer resolves from Maven's `~/.m2` classpath. With it, member /
    /// argument / cast / inheritance checks resolve **library** types (Spring, servlet, Hibernate, …)
    /// too, not just the JDK + project. `None` degrades to JDK + project, exactly as before.
    pub fn for_project(
        index_dir: &Path,
        jdk_version: &str,
        project_simple_names: &[(String, String)],
        jdk_index_path: Option<PathBuf>,
        deps: Option<(Box<dyn ClassSource>, PathBuf)>,
    ) -> Result<Self, String> {
        use bennu_classpath::prelude::resolve_jdk_classpath;
        use bennu_index::prelude::PersistedIndex;

        let blob = index_dir.join("symbols.blob");
        let fst = index_dir.join("names.fst");
        let project = PersistedIndex::open(&blob, &fst).map_err(|e| e.to_string())?;
        let source = resolve_jdk_classpath(jdk_version)?;
        // The JDK's `.java` sources, when present (`src.zip`). Opened once per build; a bare-JRE /
        // no-sources install yields `None` and go-to-into-JDK falls back to the decompiled stub.
        let jdk_sources = bennu_classpath::prelude::resolve_jdk_sources(jdk_version);

        // Build the "Import class" name index from the classpath + project types. Enumerate the JDK
        // (and, below, the dependency) `.class` names BEFORE the sources are moved into the member
        // index; this runs once per build on the background index thread.
        let mut class_names = ClassNameIndex::new();
        class_names.add_binaries(source.class_names());
        for (simple, binary) in project_simple_names {
            class_names.add_fqn(simple, &binary.replace('/', "."));
        }

        let jdk = Arc::new(match jdk_index_path {
            Some(path) => JdkMemberIndex::persistent(source, path),
            None => JdkMemberIndex::new(source),
        });
        let classpath = match deps {
            Some((dep_source, dep_memo_path)) => {
                class_names.add_binaries(dep_source.class_names());
                ClasspathIndex::with_deps(Arc::clone(&jdk), dep_source, dep_memo_path)
            }
            None => ClasspathIndex::jdk_only(Arc::clone(&jdk)),
        };
        // Snapshot the prefix-search axis now that every JDK / dependency / project class is in.
        class_names.finalize();

        let mut resolver = IndexResolver::new(project, classpath);
        for (simple, binary) in project_simple_names {
            resolver.add_simple_hint(simple, binary);
        }

        // A second view for the reference walk: project + JDK, and NOT the dependency tier.
        //
        // The walk needs library types as *conduits* — `list.stream().map(x -> x.foo())` types `x`
        // only by substituting through `List`/`Stream`/`Function` — but the tier that makes that
        // expensive is the dependency one. Its classes are decoded lazily and kept in memory only,
        // so a walk that touches thousands of them pays for thousands of jar reads every session;
        // the JDK tier is memoized and persisted, so it is expensive once, ever, and is shared with
        // the resolver above rather than decoded twice.
        //
        // The cost of leaving deps out is a conduit that runs through a LIBRARY generic (Guava's
        // `FluentIterable`, say) — still missed. The JDK ones are the ones real code is full of.
        let walk = PersistedIndex::open(&blob, &fst).ok().map(|index| {
            let mut r = IndexResolver::new(index, ClasspathIndex::jdk_only(jdk));
            for (simple, binary) in project_simple_names {
                r.add_simple_hint(simple, binary);
            }
            Arc::new(r)
        });

        Ok(Self {
            resolver: Some(Arc::new(resolver)),
            walk_resolver: walk,
            class_names,
            jdk_sources,
        })
    }

    /// Persist the classpath member index's memos now (best-effort; no-op for the empty provider or
    /// an in-memory index). Flushes BOTH tiers — the shared JDK memo and, when present, the
    /// per-project dependency memo — so a session's warmed JDK **and** library classes survive.
    pub fn flush_jdk_index(&self) {
        if let Some(resolver) = self.resolver.as_deref() {
            resolver.jdk_index().flush();
        }
    }

    /// Resolve the type `name` under the caret (a simple name via the file's `imports`, or a dotted
    /// FQCN) to its binary name — the shared front of the "go to source / decompile" flow. `None`
    /// when it doesn't resolve, or is a PROJECT type (real source exists — the normal go-to opens it,
    /// never a stub). The be then serves, in order: JDK `src.zip` source, a dependency `-sources.jar`,
    /// or a decompiled stub.
    pub fn library_binary(&self, source: &str, name: &str) -> Option<String> {
        use bennu_java::prelude::TypeResolver; // brings `resolve_simple_name`/`is_project_type` into scope
        let resolver = self.resolver.as_deref()?;
        let binary = if name.contains('.') {
            name.replace('.', "/")
        } else {
            let imports = bennu_java::prelude::extract_symbols(source).imports;
            resolver.resolve_simple_name(name, &imports)?
        };
        if resolver.is_project_type(&binary) {
            return None;
        }
        Some(binary)
    }

    /// The REAL `.java` source for `binary` from the JDK's `src.zip` (method bodies, loops, locals,
    /// lambdas, anonymous classes), when the JDK ships sources and holds this type. `None` on a
    /// bare-JRE install or a non-JDK type — the be then tries dependency sources, then a stub.
    pub fn jdk_source_text(&self, binary: &str) -> Option<String> {
        self.jdk_sources
            .as_ref()
            .and_then(|z| z.source_text(binary))
    }

    /// A signatures-only **decompiled-from-bytecode stub** for `binary` — the fallback when no real
    /// source is available (a bare JRE, or a dependency whose `-sources.jar` isn't downloaded).
    /// `None` on the pre-index provider or when the bytecode isn't decodable.
    pub fn stub_for(&self, binary: &str) -> Option<String> {
        use bennu_java::prelude::TypeResolver; // brings `members_of` into scope
        let resolver = self.resolver.as_deref()?;
        let cm = resolver.members_of(binary)?;
        Some(render_stub(binary, &cm))
    }

    /// The resolved members of `binary` — the class's own declared fields and methods, plus the
    /// links to its supertypes.
    ///
    /// The raw answer, deliberately: callers that want a *rendering* of it have one
    /// ([`stub_for`](Self::stub_for)), and callers that want to walk it — "what is inside this
    /// DTO" — need the structure rather than a page of Java to parse back apart.
    pub fn members_of(
        &self,
        binary: &str,
    ) -> Option<std::sync::Arc<bennu_java::prelude::ClassMembers>> {
        use bennu_java::prelude::TypeResolver;
        self.resolver.as_deref()?.members_of(binary)
    }

    /// The binary name of the static type of the expression spanning `[start, end)` in `source`,
    /// against this provider's full (JDK + dependency + project) resolver. For navigation/hover
    /// INSIDE a library source view — e.g. inferring `list` in `list.add(x)` to know which type
    /// declares `add`. `None` when the expression can't be typed. Works on any `.java` text.
    pub fn infer_type_binary(&self, source: &str, start: usize, end: usize) -> Option<String> {
        let resolver = self.resolver.as_deref()?;
        let tr = bennu_java::prelude::infer_expression_type(source, start, end, resolver)?;
        Some(tr.binary_name)
    }

    /// The static type of the expression spanning `[start, end)`, **written the way source writes
    /// it** — `List<String>`, not `java/util/List` — plus the fully-qualified names an import is
    /// needed for.
    ///
    /// The other half of [`infer_type_binary`](Self::infer_type_binary), and a different question:
    /// that one answers *which class is this* for a lookup, this one answers *what do I type* for a
    /// declaration a refactoring is about to write. Generic arguments are rendered because dropping
    /// them turns a correct refactoring into a raw-type warning, and a nested class comes out as
    /// `Map.Entry` with `java.util.Map` imported, which is how a person would write it.
    ///
    /// `None` when the expression cannot be typed — the caller then refuses rather than guessing,
    /// which is the whole reason this returns an `Option` instead of a `var`.
    pub fn infer_type_source(
        &self,
        source: &str,
        start: usize,
        end: usize,
    ) -> Option<(String, Vec<String>)> {
        let resolver = self.resolver.as_deref()?;
        let tr = bennu_java::prelude::infer_expression_type(source, start, end, resolver)?;
        let mut imports = Vec::new();
        let written = render_type_for_source(&tr, &mut imports);
        (!written.is_empty()).then_some((written, imports))
    }

    /// The **AST** of `source`, typed against this provider's resolver.
    ///
    /// Without a resolver — the pre-index provider — the tree is still complete, just untyped:
    /// the structure comes from the parse and only the type annotations need the classpath. That
    /// is what lets the panel draw something useful on a project that is still indexing.
    pub fn ast_of(&self, source: &str) -> bennu_java::prelude::AstNode {
        bennu_java::prelude::lower_ast(
            source,
            self.resolver
                .as_ref()
                .map(|r| r.as_ref() as &dyn bennu_java::prelude::TypeResolver),
        )
    }

    /// The binary name of the type that `name` **names** in `source`, or `None` when nothing does.
    ///
    /// The other half of [`Self::infer_type_binary`]. That one asks "what is the type *of* this
    /// expression"; this one asks "does this text name a type at all" — the question that
    /// separates `Files.copy(a, b)` from `files.copy(a, b)`, which are the same shape and
    /// different programs. Together they are what structural search's `@type` / `@value`
    /// constraint resolves against.
    ///
    /// Unlike [`Self::library_binary`], a **project** type answers yes: the caller is deciding
    /// what a name denotes, not where to find source for it.
    ///
    /// Guarded by shape first — anything that is not a bare or dotted identifier is not a type
    /// name, and handing an arbitrary expression's text to the resolver would be asking it a
    /// question it has no way to refuse.
    pub fn type_named(&self, source: &str, name: &str) -> Option<String> {
        use bennu_java::prelude::TypeResolver; // brings `resolve_simple_name`/`members_of` into scope
        let resolver = self.resolver.as_deref()?;
        if !reads_as_type_name(name) {
            return None;
        }
        if name.contains('.') {
            // Already qualified: it names a type exactly when the classpath holds one.
            let binary = name.replace('.', "/");
            return resolver.members_of(&binary).is_some().then_some(binary);
        }
        let imports = bennu_java::prelude::extract_symbols(source).imports;
        resolver.resolve_simple_name(name, &imports)
    }

    /// Whether `candidate` is `wanted`, or extends/implements it — both **binary** names.
    ///
    /// Walks superclasses and interfaces breadth-first through this provider's resolver, so it
    /// reaches through the JDK and the dependency jars, not only the project's own sources.
    ///
    /// **`false` on an unknown class**, unlike [`bennu_check`]'s conservative hierarchy walks. The
    /// two want opposite defaults and it is worth being explicit about why: a *check* that cannot
    /// see a supertype must stay silent rather than accuse, so an unknown class satisfies
    /// everything. A *search* filter that did the same would answer "yes" for every type it could
    /// not read, and a count of "uses of OrderService" would quietly include everything on the
    /// classpath. Here, not-known is not-a-match — and the caller reports it as undecided rather
    /// than as an absence (see `bennu-ssr`'s `TypeOracle`).
    ///
    /// Depth-bounded: a malformed index with a cycle in it must not spin.
    pub fn is_subtype_of(&self, candidate: &str, wanted: &str) -> bool {
        use bennu_java::prelude::TypeResolver; // brings `members_of` into scope
        const MAX_DEPTH: usize = 40;

        let normalise = |n: &str| n.replace('.', "/");
        let wanted = normalise(wanted);
        let Some(resolver) = self.resolver.as_deref() else {
            return false;
        };

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut queue = vec![(normalise(candidate), 0usize)];
        while let Some((binary, depth)) = queue.pop() {
            if depth > MAX_DEPTH || !seen.insert(binary.clone()) {
                continue;
            }
            if binary == wanted {
                return true;
            }
            let Some(cm) = resolver.members_of(&binary) else {
                continue;
            };
            if let Some(sup) = &cm.superclass {
                queue.push((sup.clone(), depth + 1));
            }
            for iface in &cm.interfaces {
                queue.push((iface.clone(), depth + 1));
            }
        }
        false
    }

    /// Whether the project declares `binary` — the guard that keeps a *library* navigation from
    /// serving code the user wrote.
    pub fn owns_type(&self, binary: &str) -> bool {
        use bennu_java::prelude::TypeResolver;
        self.resolver
            .as_ref()
            .is_some_and(|r| r.is_project_type(binary))
    }

    /// This provider's fully-resolving (project + JDK + dependency) resolver, type-erased and
    /// shareable — `None` before a project index exists.
    ///
    /// Handed to the [`SemanticEngine`](crate::engine::SemanticEngine) so its reference walk can type
    /// receivers that only a LIBRARY generic can carry: in `list.stream().map(x -> x.foo())` the
    /// lambda parameter `x` is typed by substituting through `List`/`Stream`/`Function`, so with no
    /// JDK those `x.foo()` edges are never recorded and a rename silently misses them. Shared
    /// rather than rebuilt: one classpath index, one warmed memo, one set of decoded classes for
    /// both completion and find-usages/rename.
    pub fn shared_resolver(
        &self,
    ) -> Option<Arc<dyn bennu_java::prelude::TypeResolver + Send + Sync>> {
        self.resolver
            .as_ref()
            .map(|r| Arc::clone(r) as Arc<dyn bennu_java::prelude::TypeResolver + Send + Sync>)
    }

    /// The resolver the REFERENCE WALK should use: project + JDK, without the dependency tier.
    ///
    /// The walk is parallel and runs over every file, so what it resolves has to be bounded. The
    /// JDK tier is decoded at most once ever (memoized in process, persisted across sessions) and
    /// is shared with the full resolver; the dependency tier is decoded lazily and kept only in
    /// memory, so a walk through it re-reads hundreds of jars every session — which is what made a
    /// large project's index crawl with every core busy.
    ///
    /// `None` before a project index exists, and then the caller falls back to project-only.
    pub fn walk_resolver(
        &self,
    ) -> Option<Arc<dyn bennu_java::prelude::TypeResolver + Send + Sync>> {
        self.walk_resolver
            .as_ref()
            .map(|r| Arc::clone(r) as Arc<dyn bennu_java::prelude::TypeResolver + Send + Sync>)
    }

    /// Classify the caret at `offset` in a library source view `source` into a go-to [`LibraryTarget`]
    /// — the type to open + (for a member access) the member to land on. Resolves against this
    /// provider's full resolver, using the library file's OWN imports. Handles: a type reference
    /// (`Supplier` → its type), an instance member access (`recv.foo()` / `recv.bar` → the receiver's
    /// type + member), and a static member access (`Foo.bar()` / `Foo.CONST`). `None` when the caret
    /// isn't on a resolvable navigable anchor (e.g. a bare same-class call, a local — the be handles
    /// those, or they stay in-file). Works on any `.java` text (project or library).
    pub fn library_target_at(&self, source: &str, offset: usize) -> Option<LibraryTarget> {
        let tree = bennu_java::prelude::parse_java(source)?;
        let bytes = source.as_bytes();
        let node = tree
            .root_node()
            .named_descendant_for_byte_range(offset, offset)?;
        if !matches!(node.kind(), "identifier" | "type_identifier") {
            return None;
        }
        let text = node.utf8_text(bytes).ok()?;

        // The binary name of a member-access RECEIVER: infer its value type, else (a static access
        // like `Foo.bar()`) resolve the receiver as a type name.
        let receiver_binary = |obj: tree_sitter::Node| -> Option<String> {
            self.infer_type_binary(source, obj.start_byte(), obj.end_byte())
                .or_else(|| {
                    obj.utf8_text(bytes)
                        .ok()
                        .and_then(|t| self.library_binary(source, t))
                })
        };

        if let Some(p) = node.parent() {
            match p.kind() {
                // `recv.foo(...)` — caret on the method name.
                "method_invocation" if p.child_by_field_name("name") == Some(node) => {
                    let obj = p.child_by_field_name("object")?; // bare same-class call → not resolved here
                    let binary = receiver_binary(obj)?;
                    return Some(LibraryTarget {
                        binary,
                        member: Some(LibraryMember {
                            name: text.to_string(),
                            is_field: false,
                        }),
                    });
                }
                // `recv.field` — caret on the field name.
                "field_access" if p.child_by_field_name("field") == Some(node) => {
                    let obj = p.child_by_field_name("object")?;
                    let binary = receiver_binary(obj)?;
                    return Some(LibraryTarget {
                        binary,
                        member: Some(LibraryMember {
                            name: text.to_string(),
                            is_field: true,
                        }),
                    });
                }
                _ => {}
            }
        }

        // Otherwise a TYPE reference (a `type_identifier`, or a bare name used as a type / scope).
        let binary = self.library_binary(source, text)?;
        Some(LibraryTarget {
            binary,
            member: None,
        })
    }

    /// Validate a Java `source` (AST checks always; the resolver-backed unknown-member check when a
    /// resolver is built + a JDK is available). `ctx` carries the file location + target Java version
    /// the be layer computed. Runs against THIS provider's own resolver.
    pub fn validate(
        &self,
        source: &str,
        ctx: &bennu_check::prelude::FileContext,
        jdk_available: bool,
    ) -> Vec<Diagnostic> {
        match self.resolver.as_deref() {
            Some(resolver) => {
                bennu_check::prelude::check_file_resolved(source, ctx, resolver, jdk_available)
            }
            None => bennu_check::prelude::check_file(source, ctx),
        }
    }

    /// [`validate`](Self::validate), but reusing cached diagnostics for the method / constructor bodies
    /// whose text didn't change since the last run against this provider — the out-of-code-block
    /// incremental pass. `resolver_rev` is an opaque revision the caller bumps whenever this provider's
    /// answers could change (project re-index, or another file's buffer edited) so a stale body is
    /// never replayed; `cache` is the caller-owned per-file state. Result is the same multiset as a
    /// full [`validate`](Self::validate).
    pub fn validate_incremental(
        &self,
        source: &str,
        ctx: &bennu_check::prelude::FileContext,
        jdk_available: bool,
        resolver_rev: u64,
        cache: &mut bennu_check::prelude::IncrementalCache,
    ) -> Vec<Diagnostic> {
        match self.resolver.as_deref() {
            Some(resolver) => bennu_check::prelude::check_file_resolved_incremental(
                source,
                ctx,
                resolver,
                jdk_available,
                resolver_rev,
                cache,
            ),
            None => bennu_check::prelude::check_file(source, ctx),
        }
    }

    /// A hover card for a **local variable / parameter** at `file`:`offset` — the piece the
    /// reference-index classifier (fields/methods/types) deliberately doesn't key, so the be layer
    /// falls back here.
    ///
    /// It answers three questions at once, because hovering a name is always all three: what it is
    /// (`ArrayList<Foo> rows`), *which* one (the dotted FQCN — four `Order`s on the classpath is the
    /// normal case in a legacy project), and whether that type is a class, an interface, an enum or
    /// a record. A `var` / Lombok `val` never shows as `var`: the whole point of hovering one is the
    /// type the compiler deduced, so the initializer is inferred — with THIS provider's full,
    /// JDK-aware resolver, so `var list = new ArrayList<Foo>()` reads as `ArrayList<Foo>`.
    ///
    /// `None` on the empty provider, an unparseable buffer, or a caret that isn't on a resolvable
    /// identifier.
    pub fn var_hover(&self, source: &str, offset: usize) -> Option<crate::rename::HoverInfo> {
        use bennu_java::prelude::infer_expression_type;
        let resolver = self.resolver.as_deref()?;

        let tree = bennu_java::prelude::parse_java(source)?;
        let bytes = source.as_bytes();

        // The identifier leaf under the caret.
        let node = tree
            .root_node()
            .named_descendant_for_byte_range(offset, offset)?;
        if node.kind() != "identifier" {
            return None;
        }
        let name = node.utf8_text(bytes).ok()?.to_string();

        // The declaration forms whose binding the ordinary inference can't see from the name
        // itself, then the ordinary path — which covers locals (`var` included), parameters,
        // `catch` and try-with-resources, at their declaration AND at every use.
        let ty = self.declared_binding_type(source, bytes, node).or_else(|| {
            infer_expression_type(source, node.start_byte(), node.end_byte(), resolver)
        });

        let Some(ty) = ty else {
            // Nothing resolved. Returning `None` here — which is what this used to do — makes
            // the tooltip simply not appear, and an absent tooltip is indistinguishable from a
            // broken one: the user cannot tell "Bennu could not type this" from "hover doesn't
            // work on `val`". So a local always gets a card, saying what is certain (it is a
            // local, this is how it was declared) and admitting the rest.
            return unresolved_local_hover(bytes, node, &name);
        };

        let (container, kind) = self.describe_type(&ty);
        Some(crate::rename::HoverInfo {
            signature: format!("{} {name}", render_type_ref(&ty)),
            kind,
            container,
            doc: None,
        })
    }

    /// The type a declaration binds when the caret is on the NAME it declares and the use-site
    /// inference cannot see it from there:
    ///   * the enhanced-`for` variable — its scope begins *after* the iterable, precisely so that
    ///     `for (Foo x : x.getKids())` reads the outer `x` in the iterable, which also means the
    ///     name itself sits outside its own scope;
    ///   * an `instanceof` pattern variable — bound by a flow fact rather than by a statement.
    ///
    /// Every other declaration form resolves through the ordinary path, so it is not repeated here.
    fn declared_binding_type(
        &self,
        source: &str,
        bytes: &[u8],
        node: tree_sitter::Node,
    ) -> Option<bennu_java::prelude::TypeRef> {
        use bennu_java::prelude::infer_expression_type;
        let resolver = self.resolver.as_deref()?;
        let parent = node.parent()?;
        if parent.child_by_field_name("name").map(|n| n.id()) != Some(node.id()) {
            return None;
        }
        match parent.kind() {
            "enhanced_for_statement" => {
                let written = parent.child_by_field_name("type")?.utf8_text(bytes).ok()?;
                if written == "var" || written == "val" {
                    let value = parent.child_by_field_name("value")?;
                    let it = infer_expression_type(
                        source,
                        value.start_byte(),
                        value.end_byte(),
                        resolver,
                    )?;
                    // `List<Foo>` → `Foo`. A raw or multi-argument iterable says nothing about the
                    // element, and a guess here would be shown to the user as fact.
                    (it.type_args.len() == 1).then(|| it.type_args[0].clone())
                } else {
                    self.type_of_written(source, written)
                }
            }
            "instanceof_expression" => {
                let written = parent.child_by_field_name("right")?.utf8_text(bytes).ok()?;
                self.type_of_written(source, written)
            }
            _ => None,
        }
    }

    /// A written type (`Foo`, `com.acme.Foo`, `List<Foo>`, `Foo[]`) resolved to its binary name via
    /// the file's imports. Type arguments are dropped — the caller renders the written text when it
    /// wants them; this exists to answer "which type is this, exactly".
    fn type_of_written(&self, source: &str, written: &str) -> Option<bennu_java::prelude::TypeRef> {
        use bennu_java::prelude::{TypeRef, TypeResolver};
        let resolver = self.resolver.as_deref()?;
        let base = written.split('<').next()?.trim().trim_end_matches("[]");
        if base.is_empty() {
            return None;
        }
        let binary = if base.contains('.') {
            base.replace('.', "/")
        } else {
            let imports = bennu_java::prelude::extract_symbols(source).imports;
            resolver.resolve_simple_name(base, &imports)?
        };
        Some(TypeRef {
            binary_name: binary,
            type_args: Vec::new(),
        })
    }

    /// `(dotted FQCN, what the type IS)` for the hover's meta line. A primitive or a type variable
    /// has no FQCN and no declaration to read, so it carries neither; an unresolvable type falls
    /// back to `variable`, which is at least true.
    fn describe_type(&self, ty: &bennu_java::prelude::TypeRef) -> (Option<String>, String) {
        use bennu_java::prelude::TypeResolver;
        if !ty.binary_name.contains('/') {
            return (None, "variable".to_string());
        }
        let kind = self
            .resolver
            .as_ref()
            .and_then(|r| r.members_of(&ty.binary_name))
            .map(|cm| {
                if cm.flags.is_annotation {
                    "annotation"
                } else if cm.flags.is_interface {
                    "interface"
                } else if cm.flags.is_enum {
                    "enum"
                } else if cm.flags.is_record {
                    "record"
                } else {
                    "class"
                }
            })
            .unwrap_or("variable");
        (
            Some(ty.binary_name.replace('/', ".").replace('$', ".")),
            kind.to_string(),
        )
    }

    /// Validate `source` while RECORDING the project types the validation reads — the fingerprint
    /// inputs the incremental diagnostic cache stores. Returns the diagnostics paired with the
    /// recorded dependencies. On the empty (pre-index) provider it runs the pure-AST checks and
    /// records nothing (the caller then skips caching, since there's no resolver to check
    /// freshness against).
    pub fn validate_recording(
        &self,
        source: &str,
        ctx: &bennu_check::prelude::FileContext,
        jdk_available: bool,
    ) -> (Vec<Diagnostic>, bennu_query::prelude::RecordedDeps) {
        match self.resolver.as_deref() {
            Some(resolver) => bennu_query::prelude::record(|| {
                bennu_check::prelude::check_file_resolved(source, ctx, resolver, jdk_available)
            }),
            None => (
                bennu_check::prelude::check_file(source, ctx),
                bennu_query::prelude::RecordedDeps::default(),
            ),
        }
    }

    /// The read-only project view for the diagnostic cache's freshness check, or `None` on the
    /// empty (pre-index) provider (the caller then can't cache — it just validates fresh).
    pub fn project_view(&self) -> Option<&(dyn bennu_query::prelude::ProjectView + '_)> {
        self.resolver
            .as_ref()
            .map(|r| r.as_ref() as &dyn bennu_query::prelude::ProjectView)
    }

    /// Apply one edited `file`'s freshly-extracted [`Symbol`](bennu_index::prelude::Symbol)
    /// records to the resolver's **in-memory overlay** — no disk write, no JDK re-resolve,
    /// no new provider. Completion on the edited file reflects the edit immediately while
    /// the memory-mapped index files stay untouched (they're only rewritten on a full
    /// build, which swaps in a brand-new provider). A no-op on the empty (pre-index)
    /// provider. The overlay tracks each file's prior contributions internally (keyed by
    /// `file`), so a rename/remove drops the stale entries; an empty `records` (a deleted
    /// file) just clears the file's overlay.
    pub fn apply_file_patch(&self, file: &str, records: &[bennu_index::prelude::Symbol]) {
        if let Some(resolver) = self.resolver.as_deref() {
            resolver.apply_file_patch(file, records);
        }
    }

    /// Enumerate the project's members (methods + fields) from the built index, for the
    /// index inspector's members list. A read-only view of the persisted index (the
    /// analyzer owns how a member symbol maps to a [`ProjectMember`]). An empty vec on the
    /// pre-index (empty) provider — the FE shows the "building" state.
    pub fn project_members(&self) -> Vec<ProjectMember> {
        let Some(resolver) = self.resolver.as_deref() else {
            return Vec::new();
        };
        resolver
            .member_symbols()
            .into_iter()
            .map(|s| ProjectMember {
                name: s.simple_name,
                owner_binary: s.fqn,
                signature: s.signature,
                file: s.loc_file,
                is_method: matches!(s.kind, SymbolKind::Method),
            })
            .collect()
    }
}

/// The identifier prefix ending at `caret`: scan back over `[A-Za-z0-9_]`; returns `(start, prefix)`.
/// Mirrors `bennu_query`'s member-completion prefix split (ASCII identifier chars).
fn ident_prefix(text: &str, caret: usize) -> (usize, String) {
    let caret = caret.min(text.len());
    let bytes = text.as_bytes();
    let mut start = caret;
    while start > 0 {
        let c = bytes[start - 1];
        if c == b'_' || c.is_ascii_alphanumeric() {
            start -= 1;
        } else {
            break;
        }
    }
    (start, text[start..caret].to_string())
}

/// The largest offset at or before `caret` that a `&str` may be sliced at.
///
/// The editor's caret arrives over IPC and the buffer may have moved on since it was taken, so it
/// is neither guaranteed to be in range nor to land on a character boundary. Every reader below
/// slices with it.
fn char_boundary_at_or_before(text: &str, caret: usize) -> usize {
    let mut at = caret.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The dotted path written immediately before `ident_start`, trailing dot included
/// (`"org.springframework."`), or empty when there is none.
///
/// No whitespace is crossed, unlike [`is_member_access`]: a qualified name is written in one
/// piece, and tolerating a gap would read `foo() . Bar` as a package walk.
fn dotted_qualifier(text: &str, ident_start: usize) -> String {
    let bytes = text.as_bytes();
    if ident_start == 0 || bytes[ident_start - 1] != b'.' {
        return String::new();
    }
    let mut start = ident_start;
    while start > 0 {
        let c = bytes[start - 1];
        if c == b'.' || c == b'_' || c == b'$' || c.is_ascii_alphanumeric() {
            start -= 1;
        } else {
            break;
        }
    }
    // A chain that opens with its own dot is the tail of an expression the walk could not see the
    // start of (`a.b().c.`), not a package.
    //
    // `start` is always a char boundary: the walk only steps over bytes it accepts, all of which
    // are ASCII, so it can never come to rest inside a multi-byte character.
    match bytes.get(start) {
        Some(b'.') => String::new(),
        _ => text[start..ident_start].to_string(),
    }
}

/// Whether `pos` sits in the qualified name of an `import` declaration.
///
/// Read off the line rather than the parse tree: a half-written import is a syntax error, which is
/// the only state this is ever asked about.
fn in_import_statement(text: &str, pos: usize) -> bool {
    let pos = pos.min(text.len());
    let line_start = text[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let head = text[line_start..pos].trim_start();
    let Some(rest) = head.strip_prefix("import") else { return false };
    // `import` alone is the keyword being typed, not a name after it.
    rest.starts_with(|c: char| c.is_whitespace())
}

/// Whether the identifier starting at `ident_start` is a member access — the nearest non-whitespace
/// char before it is a `.` (`recv.Foo`), so it's a member, not a bare type reference.
fn is_member_access(text: &str, ident_start: usize) -> bool {
    let bytes = text.as_bytes();
    let mut i = ident_start;
    while i > 0 {
        match bytes[i - 1] {
            b' ' | b'\t' | b'\r' | b'\n' => i -= 1,
            b'.' => return true,
            _ => return false,
        }
    }
    false
}

/// The single FQN to auto-import for a type completion, or `None` when it shouldn't auto-import: an
/// ambiguous name (several candidate packages → leave it to the Alt+Enter picker) or a `java.lang`
/// type (needs no import). The same-package / already-imported cases are filtered at accept time by
/// the be `bennu_import_edit` handler, which knows the file's package.
fn single_import_candidate(fqns: &[String]) -> Option<String> {
    let [only] = fqns else { return None };
    let pkg = only.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
    (pkg != "java.lang").then(|| only.clone())
}

/// The `detail` line for a type completion: its FQN (preferring `java`/`javax`), plus a `(+N more)`
/// hint when the simple name is declared in several packages.
fn type_detail(fqns: &[String]) -> Option<String> {
    let best = fqns
        .iter()
        .find(|f| f.starts_with("java.") || f.starts_with("javax."))
        .or_else(|| fqns.first())?;
    if fqns.len() == 1 {
        Some(best.clone())
    } else {
        Some(format!("{best} (+{} more)", fqns.len() - 1))
    }
}

impl IntelProvider for NativeJavaProvider {
    fn completion(
        &self,
        at: &Position,
        source: Option<&str>,
    ) -> Result<Vec<CompletionItem>, IntelError> {
        // No index yet (pre-open / still building) → benign empty, not an error.
        let Some(resolver) = self.resolver.as_deref() else {
            return Ok(Vec::new());
        };
        // Prefer the live buffer the caller hands in: the caret `offset` is in the editor's
        // coordinates, and the `.` the user just typed to trigger completion is unsaved — it exists
        // ONLY in that buffer. Parsing the on-disk file at a live offset would land mid-token and the
        // receiver before the dot would never be found (empty completions after every edit). Fall
        // back to a tolerant disk read (UTF-8-first, recovering via Windows-1252) only when no buffer
        // is supplied — a programmatic query with nothing open.
        let disk;
        let text: &str = match source {
            Some(src) => src,
            None => {
                let Some(decoded) = crate::java_index::read_source_for_index(
                    std::path::Path::new(&at.file),
                    &bennu_project::prelude::EncodingPlan::uniform("UTF-8"),
                ) else {
                    return Ok(Vec::new());
                };
                disk = decoded.text;
                &disk
            }
        };
        // The caret, made safe to slice at, ONCE. `completion_in` guards its own copy — a stale or
        // out-of-range offset would panic on the first `&text[..]` — but it kept the clamped value
        // to itself, so the two paths below were still indexing with the raw one.
        let offset = char_boundary_at_or_before(text, at.offset);
        // The classpath's type-name catalog rides along: a receiver you have not imported yet
        // (`Arrays.`) is one you are in the middle of writing, and refusing it is refusing the very
        // gesture that adds the import. See `TypeNameCatalog`.
        let member =
            bennu_query::prelude::completion_in(text, offset, resolver, Some(&self.class_names));
        if !member.is_empty() {
            return Ok(member);
        }
        // No member candidates. A dotted path is the next thing it could be — an `import`, or a
        // name written out qualified — and that is a question about the classpath's *names*,
        // which is the one thing member inference cannot answer: a package has no members.
        let qualified = self.qualified_completions(text, offset);
        if !qualified.is_empty() {
            return Ok(qualified);
        }
        // Otherwise TYPE-NAME completion, when the caret sits on a bare, capitalised identifier
        // prefix (not a member access after a `.`). Selecting a name inserts it; the "Import class"
        // intention (Alt+Enter) then adds the import.
        Ok(self.type_completions(text, offset))
    }

    fn hover(&self, _at: &Position) -> Result<Option<String>, IntelError> {
        Ok(None)
    }

    fn definition(&self, _at: &Position) -> Result<Vec<Location>, IntelError> {
        Ok(Vec::new())
    }

    fn references(&self, _at: &Position) -> Result<Vec<Location>, IntelError> {
        Ok(Vec::new())
    }

    fn diagnostics(&self, _file: &str) -> Result<Vec<Diagnostic>, IntelError> {
        // Phase-0: syntactic diagnostics land with tree-sitter in a later wave.
        Ok(Vec::new())
    }

    fn rename(&self, _at: &Position, _new_name: &str) -> Result<Vec<TextEdit>, IntelError> {
        Err(IntelError::Unimplemented("rename"))
    }

    fn format(&self, _file: &str) -> Result<Vec<TextEdit>, IntelError> {
        Err(IntelError::Unimplemented("format"))
    }

    fn symbols(&self, _file: &str) -> Result<Vec<DocumentSymbol>, IntelError> {
        Ok(Vec::new())
    }
}

/// The **predisposed** LSP-client provider (rust-analyzer, post-MVP — docs §2/§4).
/// Present so the seam is complete: the FE speaks the same protocol, and a language
/// bound to this provider forwards to an external LSP server. **Not implemented in
/// the MVP** — every method returns [`IntelError::Unimplemented`]. Wiring the LSP
/// transport later is a fill-in of these bodies, not a new shape (docs §2: "this is
/// the prestabilisci-LSP").
#[derive(Debug, Default)]
pub struct LspClientProvider {
    // Phase (post-MVP) holds the LSP server handle / transport here. Empty for now.
    _private: (),
}

impl LspClientProvider {
    /// Construct the (unimplemented) LSP-client provider slot.
    pub fn new() -> Self {
        Self::default()
    }
}

impl IntelProvider for LspClientProvider {
    fn completion(
        &self,
        _at: &Position,
        _source: Option<&str>,
    ) -> Result<Vec<CompletionItem>, IntelError> {
        Err(IntelError::Unimplemented("lsp completion"))
    }

    fn hover(&self, _at: &Position) -> Result<Option<String>, IntelError> {
        Err(IntelError::Unimplemented("lsp hover"))
    }

    fn definition(&self, _at: &Position) -> Result<Vec<Location>, IntelError> {
        Err(IntelError::Unimplemented("lsp definition"))
    }

    fn references(&self, _at: &Position) -> Result<Vec<Location>, IntelError> {
        Err(IntelError::Unimplemented("lsp references"))
    }

    fn diagnostics(&self, _file: &str) -> Result<Vec<Diagnostic>, IntelError> {
        Err(IntelError::Unimplemented("lsp diagnostics"))
    }

    fn rename(&self, _at: &Position, _new_name: &str) -> Result<Vec<TextEdit>, IntelError> {
        Err(IntelError::Unimplemented("lsp rename"))
    }

    fn format(&self, _file: &str) -> Result<Vec<TextEdit>, IntelError> {
        Err(IntelError::Unimplemented("lsp format"))
    }

    fn symbols(&self, _file: &str) -> Result<Vec<DocumentSymbol>, IntelError> {
        Err(IntelError::Unimplemented("lsp symbols"))
    }
}

#[cfg(test)]
mod local_hover_tests {
    use super::{ellipsize, local_declaration_of, unresolved_local_hover};

    /// Parse `src` and return the identifier node at the first occurrence of `needle`.
    fn ident_at(src: &str, needle: &str) -> (tree_sitter::Tree, usize) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        (tree, src.find(needle).unwrap())
    }

    fn declaration(src: &str, needle: &str, name: &str) -> Option<(String, Option<String>)> {
        let (tree, at) = ident_at(src, needle);
        let node = tree
            .root_node()
            .named_descendant_for_byte_range(at, at)
            .unwrap();
        let (written, init) = local_declaration_of(node, src.as_bytes(), name)?;
        Some((
            written,
            init.map(|n| n.utf8_text(src.as_bytes()).unwrap().to_string()),
        ))
    }

    /// The initializer of `name`, rendered as the shape the hover card shows.
    fn shape(src: &str, needle: &str, name: &str) -> String {
        let (tree, at) = ident_at(src, needle);
        let node = tree
            .root_node()
            .named_descendant_for_byte_range(at, at)
            .unwrap();
        let (_, init) = local_declaration_of(node, src.as_bytes(), name).expect("declared");
        super::summarize_expr(init.expect("has an initializer"), src.as_bytes())
    }

    #[test]
    fn a_lombok_val_is_found_from_its_own_name() {
        let src = "class C { void m() { val properties = Retriever.properties(svc); } }";
        let (written, init) =
            declaration(src, "properties =", "properties").expect("declared here");
        assert_eq!(written, "val");
        assert_eq!(init.as_deref(), Some("Retriever.properties(svc)"));
    }

    #[test]
    fn it_is_found_from_a_later_use_too() {
        let src = "class C { void m() { val rows = dao.find(); use(rows); } }";
        let (written, init) = declaration(src, "rows)", "rows").expect("found by walking out");
        assert_eq!(written, "val");
        assert_eq!(init.as_deref(), Some("dao.find()"));
    }

    #[test]
    fn an_enhanced_for_variable_carries_its_iterable() {
        let src = "class C { void m() { for (val row : dao.all()) { use(row); } } }";
        let (written, init) = declaration(src, "row :", "row").expect("loop variable");
        assert_eq!(written, "val");
        assert_eq!(init.as_deref(), Some("dao.all()"));
    }

    #[test]
    fn a_name_that_is_not_a_local_yields_nothing() {
        let src = "class C { int field; void m() { use(field); } }";
        assert!(
            declaration(src, "field);", "field").is_none(),
            "a field is not this function's business"
        );
    }

    #[test]
    fn the_card_says_what_is_certain_and_admits_the_rest() {
        let src = "class C { void m() { val properties = Retriever.properties(svc); } }";
        let (tree, at) = ident_at(src, "properties =");
        let node = tree
            .root_node()
            .named_descendant_for_byte_range(at, at)
            .unwrap();
        let info = unresolved_local_hover(src.as_bytes(), node, "properties").expect("a card");
        assert_eq!(info.signature, "val properties");
        assert_eq!(info.kind, "variable");
        // The shape, not the source text: the arguments are elided on purpose.
        assert!(info.doc.unwrap().contains("Retriever.properties(…)"));
    }

    /// The case the card used to be useless on: a builder chain whose *shape* is the answer and
    /// whose *text* is a page. Everything that decides the type — the search, the `map`, the
    /// `orElseGet` — has to survive; the arguments must not.
    #[test]
    fn a_builder_chain_reads_as_its_shape() {
        let src = "class C { void m() {\n\
                     val pair =\n\
                       service.search(\n\
                           Filter.builder()\n\
                               .applicativo(root.getId().getIdprg())\n\
                               .chiave1(exact(root.getComkey1()))\n\
                           .build()\n\
                       ).map(it -> Pair.of(it.getId(), factory.builder(it).get()))\n\
                        .orElseGet(() -> create(root));\n\
                   } }";
        assert_eq!(
            shape(src, "pair =", "pair"),
            "service.search(…).map(…).orElseGet(…)"
        );
    }

    /// A chain that stands on something other than a name still reads, and a chain longer than the
    /// budget loses its HEAD — the type comes out of the last call.
    #[test]
    fn a_shape_keeps_the_end_it_is_about() {
        let src = "class C { void m() { val x = new Builder().a().b(); } }";
        assert_eq!(shape(src, "x =", "x"), "new Builder().a().b()");

        let long = format!(
            "class C {{ void m() {{ val y = seed{}.last(); }} }}",
            (0..30)
                .map(|i| format!(".step{i}(arg)"))
                .collect::<String>()
        );
        let cut = shape(&long, "y =", "y");
        assert!(cut.starts_with('…'), "the head is what was dropped: {cut}");
        assert!(
            cut.ends_with(".last()"),
            "the call that decides the type survived: {cut}"
        );
    }

    #[test]
    fn a_long_initializer_is_shortened_and_flattened() {
        assert_eq!(ellipsize("a  \n  b", 40), "a b");
        let long = "x".repeat(200);
        let cut = ellipsize(&long, 10);
        assert_eq!(cut.chars().count(), 11, "10 characters plus the ellipsis");
        assert!(cut.ends_with('…'));
    }
}

/// Render a resolved type as source would spell it, collecting the imports it needs.
///
/// Three shapes, and each of them is a way a naive rendering goes wrong:
///   * a **primitive** (`int`) has no slashes and is written as it is — importing it would be
///     nonsense;
///   * a **nested class** is `java/util/Map$Entry`, which a person writes `Map.Entry` and imports
///     as `java.util.Map`;
///   * a **generic** carries its arguments, because a declaration written without them is a raw
///     type and a warning where the original was neither.
fn render_type_for_source(tr: &bennu_java::prelude::TypeRef, imports: &mut Vec<String>) -> String {
    let binary = tr.binary_name.trim();
    if binary.is_empty() {
        return String::new();
    }
    let written = if binary.contains('/') {
        let dotted = binary.replace('/', ".");
        let (outer, nested) = match dotted.split_once('$') {
            Some((outer, rest)) => (outer.to_string(), rest.replace('$', ".")),
            None => (dotted.clone(), String::new()),
        };
        // `java.lang` is implicit; importing it is noise the compiler already has.
        if !outer.starts_with("java.lang.") || outer.matches('.').count() > 2 {
            imports.push(outer.clone());
        }
        let simple = outer.rsplit('.').next().unwrap_or(&outer).to_string();
        match nested.is_empty() {
            true => simple,
            false => format!("{simple}.{nested}"),
        }
    } else {
        binary.to_string()
    };
    if tr.type_args.is_empty() {
        return written;
    }
    let args: Vec<String> =
        tr.type_args.iter().map(|a| render_type_for_source(a, imports)).collect();
    format!("{written}<{}>", args.join(", "))
}

#[cfg(test)]
mod tests {
    use super::{reads_as_type_name, render_stub, render_type_for_source};
    use bennu_java::prelude::{ClassFlags, ClassMembers, Member, TypeRef, Visibility};

    /// The guard exists so an arbitrary matched fragment never reaches the resolver as a
    /// question. Its job is to say no to everything that is not a name.
    #[test]
    fn only_a_bare_or_dotted_identifier_reads_as_a_type_name() {
        for yes in [
            "Files",
            "java.nio.file.Files",
            "_x",
            "Outer.Inner",
            "Map$Entry",
        ] {
            assert!(reads_as_type_name(yes), "{yes}");
        }
        for no in [
            "",
            "\"hello\"",
            "a.b()",
            "1",
            "a + b",
            "a..b",
            "a.",
            "new Foo()",
            "x[0]",
        ] {
            assert!(!reads_as_type_name(no), "{no}");
        }
    }

    /// The three shapes a naive rendering gets wrong, and the imports each one needs.
    #[test]
    fn a_type_is_rendered_the_way_source_writes_it() {
        use bennu_java::prelude::TypeRef;
        let mut imports = Vec::new();
        // A primitive is written as it is, and importing it would be nonsense.
        assert_eq!(render_type_for_source(&TypeRef::simple("int"), &mut imports), "int");
        assert!(imports.is_empty());

        // A generic carries its arguments — dropping them is a raw type where the original was not.
        let list = TypeRef {
            binary_name: "java/util/List".into(),
            type_args: vec![TypeRef::simple("java/lang/String")],
        };
        assert_eq!(render_type_for_source(&list, &mut imports), "List<String>");
        // `java.lang` is implicit; `java.util` is not.
        assert_eq!(imports, ["java.util.List"]);

        // A nested class is written `Map.Entry` and imported as its outer.
        imports.clear();
        let entry = TypeRef::simple("java/util/Map$Entry");
        assert_eq!(render_type_for_source(&entry, &mut imports), "Map.Entry");
        assert_eq!(imports, ["java.util.Map"]);
    }

    #[test]
    fn stub_renders_package_decl_fields_and_methods() {
        let cm = ClassMembers {
            type_params: Vec::new(),
            superclass: Some("java/lang/Object".to_string()),
            interfaces: vec!["java/lang/Iterable".to_string()],
            methods: vec![
                Member::method(
                    "get",
                    TypeRef::simple("com/acme/Item"),
                    vec![TypeRef::simple("int")],
                )
                .vis(Visibility::Public),
                Member::method("<init>", TypeRef::simple("void"), Vec::new())
                    .vis(Visibility::Public),
            ],
            fields: vec![Member::field("MAX", TypeRef::simple("int"))
                .vis(Visibility::Public)
                .stat()],
            flags: ClassFlags::default(),
        };
        let s = render_stub("com/acme/Registry", &cm);
        assert!(
            s.contains("Decompiled from bytecode"),
            "header warning: {s}"
        );
        assert!(s.contains("package com.acme;"), "package: {s}");
        assert!(s.contains("public class Registry"), "class decl: {s}");
        assert!(s.contains("implements Iterable"), "interfaces: {s}");
        assert!(!s.contains("extends Object"), "Object super is elided: {s}");
        assert!(s.contains("public static int MAX;"), "field: {s}");
        assert!(
            s.contains("public Item get(int arg0)"),
            "method w/ synthesized arg name: {s}"
        );
        assert!(
            s.contains("public Registry("),
            "constructor rendered by simple name: {s}"
        );
    }

    #[test]
    fn stub_renders_throws_clause() {
        // A method's declared checked exceptions must appear as a `throws` clause in the stub (by
        // simple name). Regression for decompiled stubs losing the throwables.
        let cm = ClassMembers {
            type_params: Vec::new(),
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods: vec![Member::method("read", TypeRef::simple("int"), Vec::new())
                .vis(Visibility::Public)
                .throws(vec!["java/io/IOException".to_string()])],
            fields: Vec::new(),
            flags: ClassFlags::default(),
        };
        let s = render_stub("com/acme/Reader", &cm);
        assert!(
            s.contains("int read() throws IOException"),
            "throws clause rendered: {s}"
        );
    }

    #[test]
    fn stub_renders_generic_signature_like_intellij() {
        // `Optional.orElseThrow`'s bytecode `Signature`. The stub must render the method type
        // parameter (`<X extends Throwable>`), the wildcard argument (`Supplier<? extends X>`) and the
        // type-variable `throws X` — NOT the erased `Supplier<X> … throws Throwable` the seam fields
        // carry. (The erased seam `return_type`/`params`/`throws` here are intentionally wrong to prove
        // the generic `Signature` wins.)
        let cm = ClassMembers {
            type_params: vec!["T".to_string()],
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods: vec![Member::method(
                "orElseThrow",
                TypeRef::simple("T"),
                vec![TypeRef::simple("java/util/function/Supplier")],
            )
            .vis(Visibility::Public)
            .throws(vec!["java/lang/Throwable".to_string()])
            .sig("<X:Ljava/lang/Throwable;>(Ljava/util/function/Supplier<+TX;>;)TT;^TX;")],
            fields: Vec::new(),
            flags: ClassFlags::default(),
        };
        let s = render_stub("java/util/Optional", &cm);
        assert!(
            s.contains("<X extends Throwable> T orElseThrow(Supplier<? extends X> arg0) throws X"),
            "generic signature rendered like IntelliJ: {s}"
        );
        assert!(
            !s.contains("throws Throwable"),
            "erased Throwable must not appear: {s}"
        );
    }

    #[test]
    fn interface_methods_are_bodyless() {
        let cm = ClassMembers {
            type_params: Vec::new(),
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods: vec![
                Member::method("run", TypeRef::simple("void"), Vec::new()).vis(Visibility::Public)
            ],
            fields: Vec::new(),
            flags: ClassFlags {
                is_interface: true,
                ..Default::default()
            },
        };
        let s = render_stub("com/acme/Task", &cm);
        assert!(s.contains("public interface Task"), "{s}");
        assert!(
            s.contains("void run();"),
            "interface method has no body: {s}"
        );
        assert!(
            !s.contains("throw new RuntimeException"),
            "no placeholder body in an interface: {s}"
        );
    }

    // ── Type-name completion helpers ─────────────────────────────────────────────

    #[test]
    fn ident_prefix_scans_identifier_chars() {
        assert_eq!(super::ident_prefix("new Opti", 8), (4, "Opti".to_string()));
        assert_eq!(super::ident_prefix("x.foo", 5), (2, "foo".to_string()));
        // Caret at a non-identifier boundary → empty prefix.
        assert_eq!(super::ident_prefix("List<", 5), (5, String::new()));
    }

    #[test]
    fn is_member_access_detects_a_preceding_dot() {
        // `recv.Foo` — the char before `Foo` (start index 5) is `.`.
        assert!(super::is_member_access("recv.Foo", 5));
        // `new Foo` — before `Foo` is a space then `w`, not a dot.
        assert!(!super::is_member_access("new Foo", 4));
        // whitespace between the dot and the name is tolerated.
        assert!(super::is_member_access("recv.  Foo", 7));
        // start of buffer → not a member access.
        assert!(!super::is_member_access("Foo", 0));
    }

    #[test]
    fn a_qualified_name_is_read_back_off_the_line() {
        use super::dotted_qualifier as q;
        assert_eq!(q("import org.springframework.boot.Spring", 32), "org.springframework.boot.");
        assert_eq!(q("import org.", 11), "org.");
        // A bare word has no qualifier, and neither has one written after a space.
        assert_eq!(q("new Foo", 4), "");
        assert_eq!(q("recv . Foo", 7), "");
        // The tail of an expression the walk cannot see the start of is not a package.
        assert_eq!(q("a.b().c.Foo", 8), "");
        assert_eq!(q("Foo", 0), "");
        // A non-ASCII identifier stops the walk on the dot in front of it, which reads as the
        // opens-with-its-own-dot case: no qualifier, and nothing sliced mid-character.
        assert_eq!(q("a.café.Foo", "a.café.".len()), "");
    }

    #[test]
    fn an_import_line_is_recognised_while_it_is_being_typed() {
        use super::in_import_statement as imp;
        assert!(imp("import org.spring", 11));
        assert!(imp("package a;\nimport java.", 22));
        assert!(imp("  import  javax.", 16), "indented, and spaced how it likes");
        assert!(imp("import static java.util.Arrays.", 31));
        // The keyword itself is not a name after it.
        assert!(!imp("import", 6));
        // And an ordinary line is not an import however it starts.
        assert!(!imp("importantThing.foo", 15));
        assert!(!imp("class Foo {", 10));
    }

    #[test]
    fn type_detail_prefers_java_and_counts_extras() {
        assert_eq!(
            super::type_detail(&["java.util.List".to_string()]),
            Some("java.util.List".to_string())
        );
        // Multiple packages: prefer java.*, note the rest.
        let d = super::type_detail(&["com.acme.List".to_string(), "java.util.List".to_string()]);
        assert_eq!(d, Some("java.util.List (+1 more)".to_string()));
        assert_eq!(super::type_detail(&[]), None);
    }
}

