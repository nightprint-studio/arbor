//! The [`IntelProvider`] trait + its two impl slots.
//!
//! One protocol for every language (docs §2). The Phase-0 skeleton defines the trait
//! and both impls' *shapes*; the bodies are stubs that return empty / unimplemented,
//! so `bennu-be` can wire the seam now and later waves fill the native engine in
//! (and, post-MVP, the LSP client).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use bennu_complete::prelude::MatchCase;
use bennu_index::prelude::SymbolKind;
use bennu_java::prelude::{AnnotationSite, ElementTarget, TypeResolver};

use crate::import_census::ImportCensus;
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
        doc: Some(doc),
        ..Default::default()
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
    // A supertype now carries its arguments, and the decompiled view is where a reader sees them:
    // `extends AbstractList<E>` says which variable this class binds, `extends AbstractList` used to
    // say only that a binding existed somewhere.
    fn supertype_name(t: &bennu_java::prelude::TypeRef) -> String {
        let simple = t.binary_name.rsplit(['/', '$']).next().unwrap_or(&t.binary_name);
        if t.type_args.is_empty() {
            return simple.to_string();
        }
        let args: Vec<String> = t.type_args.iter().map(supertype_name).collect();
        format!("{simple}<{}>", args.join(", "))
    }

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
        if let Some(sc) =
            cm.superclass.as_ref().filter(|sc| sc.binary_name != "java/lang/Object")
        {
            s.push_str(&format!(" extends {}", supertype_name(sc)));
        }
    }
    if !cm.interfaces.is_empty() {
        let word = if cm.flags.is_interface {
            "extends"
        } else {
            "implements"
        };
        let list: Vec<String> = cm.interfaces.iter().map(supertype_name).collect();
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
        //
        // A stub is compiled against, so its parameters need names even though the class file has
        // none: `arg0` is the honest placeholder here, and the one place it is allowed.
        let core = bennu_classpath::prelude::render_method_core(
            &m.raw_signature,
            &m.name,
            is_ctor.then_some(simple),
            &bennu_classpath::prelude::placeholder_names(m.params.len()),
        )
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
    /// How often each type is imported across the project — the strongest ranking term for a
    /// simple name that resolves to several types, and the only one this index can supply.
    /// Empty for the pre-index provider, and empty is simply "no opinion". See [`ImportCensus`].
    imports: Arc<ImportCensus>,
    /// Simple type name → the **annotation types** among its candidates, decided once and kept.
    ///
    /// Deciding costs a bytecode decode per candidate, and the sweep behind an `@` visits names in
    /// ranked order until it has enough annotations — on a first letter that most of the classpath
    /// shares, that is thousands of decodes. Paying it once per name, rather than once per
    /// keystroke, is what makes the sweep affordable: `@S`, `@Se`, `@Ser` walk the same head of the
    /// same list, and after the first one every answer in it is already known.
    ///
    /// Emptied whenever the class-name axis is replaced, since a name's candidates changed with it.
    annotation_memo: RwLock<HashMap<String, Arc<Vec<String>>>>,
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

    /// Give the provider the project's import census — see [`ImportCensus`].
    ///
    /// Builder-style rather than a parameter of [`for_project`](Self::for_project), because it
    /// arrives from a different place: the census falls out of the index build, and the provider is
    /// constructed from the *persisted* index afterwards, twice. A parameter would have to be
    /// threaded through both constructions and every test that calls one.
    pub fn with_imports(mut self, census: Arc<ImportCensus>) -> Self {
        self.imports = census;
        self
    }

    /// Candidate importable FQNs (dotted, sorted) for a simple type name — the "Import class"
    /// intention's picker list. Empty for the pre-index provider or an unknown name.
    pub fn import_candidates(&self, simple: &str) -> &[String] {
        self.class_names.candidates(simple)
    }

    /// The **dependency jar** `binary` was decoded from, when a dependency declares it.
    ///
    /// What a hover card needs to say which library a type comes from. Cheap enough to ask on
    /// every tooltip and no cache is warranted: the jars are opened once when the classpath is
    /// resolved and each holds its central directory in memory from then on, so this is one hash
    /// probe per jar and no I/O — against a card that already reads a file from disk to find the
    /// declaration's Javadoc.
    ///
    /// A **path**, not a coordinate: turning `…/org/springframework/spring-core/6.1.14/…jar` into
    /// `org.springframework:spring-core:6.1.14` is a fact about a Maven repository's layout, and
    /// this crate does not know about Maven. The caller that does converts it.
    pub fn origin_jar(&self, binary: &str) -> Option<std::path::PathBuf> {
        self.resolver.as_deref()?.jdk_index().origin_of(binary)
    }

    /// What a type **is** — `class` / `interface` / `enum` / `record` / `annotation` — read off the
    /// class flags through whichever tier owns it. `None` when the name resolves to nothing.
    ///
    /// The kind of a JDK type has no other source: there is no jar to re-open on a modern JDK, and
    /// the member index this asks is already built and already holding the answer.
    pub fn type_kind(&self, binary: &str) -> Option<String> {
        use bennu_java::prelude::TypeResolver; // brings `members_of` into scope
        let resolver = self.resolver.as_deref()?;
        let name = resolver.bytecode_name(binary);
        resolver.members_of(&name)?; // resolves, or there is nothing honest to say
        // The SAME flags→kind rule the hover card uses, rather than a second reading of the same
        // five booleans — the two would drift the first time a language adds a kind.
        Some(
            crate::rename::hover_for_key(
                &crate::refs::DeclKey::Type { binary: name },
                resolver,
                None,
            )
            .kind,
        )
    }

    /// Type names on the classpath matching what has been typed, as dot-form FQNs, best first.
    ///
    /// The index behind it is the one "Import class" already uses — JDK, dependencies and the
    /// project's own types in one axis — so a caller does not have to know which tier a name lives
    /// in, which is exactly the question a user completing a class name does not want to answer.
    pub fn type_name_matches(&self, typed: &str, limit: usize) -> Vec<String> {
        // A qualified prefix (`org.springframework.web.`) matches on the WHOLE name; a bare one
        // matches on the simple name, which is what people type.
        let simple_typed = typed.rsplit('.').next().unwrap_or(typed);
        let mut out: Vec<String> = Vec::new();
        for simple in self.class_names.matches_for_prefix(simple_typed, limit * 4) {
            for fqn in self.class_names.candidates(simple) {
                if typed.contains('.') && !fqn.starts_with(typed) {
                    continue;
                }
                out.push(fqn.clone());
                if out.len() >= limit {
                    return out;
                }
            }
        }
        out
    }

    /// The binary name of the type of the expression immediately **left of the `.`** at `offset`.
    ///
    /// The one question "create this method in the receiver's class" turns on, and the reason that
    /// transform cannot live in the pure refactoring crate: `order.total()` says which method and
    /// which arguments, and says nothing whatsoever about which file. Resolved against the FULL
    /// resolver, so a receiver typed by a JDK or dependency class answers too — the caller then
    /// declines for a different reason (there is no source to write into), which is a better
    /// message than "could not resolve".
    pub fn receiver_type_at(&self, source: &str, offset: usize) -> Option<String> {
        let resolver = self.resolver.as_deref()?;
        bennu_java::prelude::infer_receiver_type(source, offset, resolver).map(|t| t.binary_name)
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
    fn type_completions(
        &self,
        text: &str,
        offset: usize,
        census: bool,
        case: MatchCase,
    ) -> Vec<CompletionItem> {
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
        // The name index ranks by how well the name matches. What it cannot know is which of two
        // equally-matching names is the one *this file* is likely to mean — see `proximity`, which
        // is the difference between `Order` offering your own `Order` and offering `java.awt`'s.
        //
        // A stable sort, so the index's match tiers survive intact and this only reorders inside
        // them. It is a tie-break, not a second opinion about the prefix.
        let here = Proximity::of(text, census.then_some(&*self.imports));
        // What the file can already reach by simple name, read once rather than per candidate.
        let symbols = bennu_java::prelude::extract_symbols(text);
        let site = bennu_java::prelude::enclosing_type_binary(text, offset);
        // The index searches case-insensitively — it is also what "Import class" and Go-to-class
        // read, where finding a name you half-remember is the whole point. The user's setting
        // applies to COMPLETION's answer, which is here.
        let mut ranked: Vec<&str> = self
            .class_names
            .matches_for_prefix(&prefix, MAX)
            .into_iter()
            .filter(|name| bennu_complete::prelude::match_tier(name, &prefix, case).is_some())
            .collect();
        ranked.sort_by_key(|simple| here.rank(self.class_names.candidates(simple)));
        ranked
            .into_iter()
            .map(|simple| {
                let candidates = self.class_names.candidates(simple);
                // A NESTED type written by its simple name does not compile on its own. See
                // `nested_form`: it answers the qualified spelling and the import that makes it
                // work, and `None` for everything that is already fine.
                let nested = match candidates {
                    [only] => self.nested_form(only, &symbols, site.as_deref()),
                    _ => None,
                };
                CompletionItem {
                    label: simple.to_string(),
                    kind: "class".to_string(),
                    detail: type_detail(candidates),
                    insert_text: nested.as_ref().map(|(written, _)| written.clone()),
                    // Auto-import ONLY when unambiguous — a single candidate that isn't `java.lang`
                    // (which needs no import). Ambiguous names (several packages) are left to the
                    // Alt+Enter picker, so we never silently import the wrong `List`.
                    //
                    // For a nested type it is the OUTER that gets imported: one import serves every
                    // nested type of that class, and it is the form people write by hand.
                    auto_import: match &nested {
                        Some((_, outer)) => Some(outer.clone()),
                        None => single_import_candidate(candidates),
                    },
                    // An ambiguous simple name owns nothing in particular: `List` is two classes,
                    // and documenting one of them would document the wrong one half the time.
                    owner: (candidates.len() == 1)
                        .then(|| candidates[0].replace('.', "/")),
                    ..Default::default()
                }
            })
            .collect()
    }

    /// The annotation types among the candidates for a simple name, decided once and remembered.
    ///
    /// Deciding is a bytecode decode per candidate ([`is_annotation_type`]), and the sweep behind
    /// an `@` asks about names in ranked order until it has enough annotations — which on a common
    /// first letter means thousands of questions. Asked once per name instead of once per
    /// keystroke, so the letters that follow walk a list whose head is already answered.
    fn annotations_named(&self, simple: &str, resolver: &dyn TypeResolver) -> Arc<Vec<String>> {
        if let Ok(memo) = self.annotation_memo.read() {
            if let Some(hit) = memo.get(simple) {
                return Arc::clone(hit);
            }
        }
        let annotations: Arc<Vec<String>> = Arc::new(
            self.class_names
                .candidates(simple)
                .iter()
                .filter(|fqn| is_annotation_type(resolver, fqn))
                .cloned()
                .collect(),
        );
        if let Ok(mut memo) = self.annotation_memo.write() {
            memo.insert(simple.to_string(), Arc::clone(&annotations));
        }
        annotations
    }

    /// The **annotation types** whose name starts with what has been typed after an `@`.
    ///
    /// Two narrowings, and the first is the one that matters. `@` says the name is an annotation
    /// type, which cuts the candidate set by two orders of magnitude — and until this existed the
    /// popup above an `@` offered every class in the project, which is a list with the answer
    /// buried in it at the one moment the answer was nearly knowable.
    ///
    /// The second is `@Target`: an annotation declares what it may be attached to, so above a field
    /// the ones that only go on a method are not candidates. That one **ranks** rather than filters,
    /// because "the resolver did not read a `@Target`" and "it has none" arrive as the same empty
    /// answer, and hiding on the strength of it would hide every annotation from a jar whose
    /// bytes were not decoded.
    fn annotation_completions(
        &self,
        text: &str,
        site: &AnnotationSite,
        census: bool,
        case: MatchCase,
    ) -> Vec<CompletionItem> {
        const MAX: usize = 40;
        let Some(resolver) = self.resolver.as_deref() else { return Vec::new() };
        if site.prefix.is_empty() && site.target == ElementTarget::Unknown {
            // A bare `@` with nothing else written is every annotation in the world, in no order
            // anyone would want. One letter is enough to make the list mean something.
            return Vec::new();
        }
        let here = Proximity::of(text, census.then_some(&*self.imports));
        // The cap counts annotations, not names looked at. Capping the sweep instead — three
        // hundred names, filtered afterwards — is what made this list start at the third letter:
        // the JDK alone has more than three hundred classes beginning with `S`, so the sweep behind
        // `@S` ended long before `SuppressWarnings`, and the popup fell through to the buffer's own
        // words. See [`ClassNameIndex::matches_for_prefix_where`].
        let mut kept: HashMap<&str, Arc<Vec<String>>> = HashMap::new();
        let matched = self.class_names.matches_for_prefix_where(&site.prefix, MAX, &mut |simple| {
            if bennu_complete::prelude::match_tier(simple, &site.prefix, case).is_none() {
                return false;
            }
            let annotations = self.annotations_named(simple, resolver);
            if annotations.is_empty() {
                return false;
            }
            kept.insert(simple, annotations);
            true
        });
        let mut scored: Vec<(u8, Rank, String, Option<String>)> = Vec::new();
        for simple in matched {
            let annotations = &kept[simple];
            let Some(first) = annotations.first() else { continue };
            // Legal here, by its own `@Target`. Ranked ahead rather than kept alone — see above.
            let fits = annotations.iter().any(|fqn| {
                let binary = fqn.replace('.', "/");
                let target = resolver
                    .class_annotations(&binary)
                    .into_iter()
                    .find(|a| a.name == "Target")
                    .map(target_text_of)
                    .unwrap_or_default();
                bennu_java::prelude::target_admits(&target, site.target)
            });
            // What you have actually picked, this session, above an `@`. The rest of the ranking
            // reads the project — this file's imports, the census's counts — and none of it can
            // tell an annotation you write every day from one you have never written, because
            // both are equally far away in package terms. It is subtracted from the weighed score
            // rather than given its own band: evidence, weighed with the other evidence, and never
            // able to outrank the file having already named the type it means.
            let (certainty, mut score) = here.rank(annotations);
            score -= bennu_query::prelude::pick_weight(
                bennu_query::prelude::ANNOTATION_CONTEXT,
                simple,
            );
            scored.push((
                u8::from(!fits),
                (certainty, score),
                simple.to_string(),
                single_import_candidate(annotations).or_else(|| Some(first.clone())),
            ));
        }
        scored.sort();
        scored.truncate(MAX);
        scored
            .into_iter()
            .map(|(_, _, simple, import)| CompletionItem {
                label: simple,
                kind: "annotation".to_string(),
                detail: import.clone(),
                owner: import.as_ref().map(|fqn| fqn.replace('.', "/")),
                auto_import: import.filter(|fqn| !fqn.starts_with("java.lang.")),
                ..Default::default()
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
                owner: seg.fqn.as_deref().map(|f| f.replace('.', "/")),
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
            // Empty until the caller hands one over — see `with_imports`. Empty is "no opinion",
            // which ranks exactly as the census being switched off does.
            imports: Arc::default(),
            annotation_memo: RwLock::default(),
        })
    }

    /// The same provider **plus a dependency tier**, sharing this one's decoded JDK.
    ///
    /// The project's dependency jars are resolved on their own thread, so the provider is built
    /// twice: once with the JDK and the project alone, so navigation comes up immediately, and again
    /// when the jars land. Building the second one through [`for_project`](Self::for_project) would
    /// redo the expensive half of the first: re-open the JVM image, re-enumerate every class name in
    /// it, and start a **second** `JdkMemberIndex` that decodes and memoises the very classes the
    /// first already holds — the duplication `ClasspathIndex` documents as the reason its JDK tier
    /// is an `Arc`.
    ///
    /// It is not only wasted work. Every dependency jar is held **open** for the session, so while
    /// the second provider is being built the process holds both sets at once, and macOS hands a
    /// bundled app 256 descriptors: on a project with 150 jars the second build failed with `Too
    /// many open files`, which surfaced as every library import going red.
    ///
    /// `None` when this provider has no resolver yet (the empty, pre-index one): there is nothing to
    /// share, and the caller falls back to a full build.
    pub fn with_dependency_tier(
        &self,
        index_dir: &Path,
        jdk_version: &str,
        project_simple_names: &[(String, String)],
        dep_source: Box<dyn ClassSource>,
        dep_memo_path: PathBuf,
    ) -> Option<Result<Self, String>> {
        use bennu_index::prelude::PersistedIndex;

        let jdk = self.resolver.as_deref()?.jdk_index().jdk_tier();
        Some((|| {
            let blob = index_dir.join("symbols.blob");
            let fst = index_dir.join("names.fst");
            let project = PersistedIndex::open(&blob, &fst).map_err(|e| e.to_string())?;

            // The JDK and project names are already in there; only the dependency ones are new.
            // `finalize` rebuilds the sorted axes from scratch, so re-running it is correct.
            let mut class_names = self.class_names.clone();
            class_names.add_binaries(dep_source.class_names());
            class_names.finalize();

            let mut resolver = IndexResolver::new(
                project,
                ClasspathIndex::with_deps(Arc::clone(&jdk), dep_source, dep_memo_path),
            );
            for (simple, binary) in project_simple_names {
                resolver.add_simple_hint(simple, binary);
            }
            // The walk's JDK-only view, over the same shared tier — see `for_project`.
            let walk = PersistedIndex::open(&blob, &fst).ok().map(|index| {
                let mut r = IndexResolver::new(index, ClasspathIndex::jdk_only(Arc::clone(&jdk)));
                for (simple, binary) in project_simple_names {
                    r.add_simple_hint(simple, binary);
                }
                Arc::new(r)
            });

            Ok(Self {
                resolver: Some(Arc::new(resolver)),
                walk_resolver: walk,
                class_names,
                // One small archive, re-opened rather than shared: `JavaSourceZip` is not behind an
                // `Arc`, and one file is not what the descriptor budget is spent on.
                jdk_sources: bennu_classpath::prelude::resolve_jdk_sources(jdk_version),
                // Carried, not recomputed: the census is a fact about the project's sources, and
                // the dependency tier changes what is on the classpath rather than what the
                // project imports. Rebuilding it here would mean re-reading every file.
                imports: Arc::clone(&self.imports),
                // NOT carried: the dependency tier adds classes, so a name that had no annotation
                // among its candidates may have one now. A memo built against the narrower
                // classpath would answer for a question that is no longer the same one.
                annotation_memo: RwLock::default(),
            })
        })())
    }

    /// Whether this provider's resolver can see the project's **libraries**, not just the JDK.
    ///
    /// The honest answer to "is the classpath complete", which decides whether an unresolvable
    /// `org.…` import is a real error or a gap in what we indexed. Inferring it from the resolved
    /// jar LIST instead is what marked every library import in a project red: the list is written
    /// when Maven answers, and the tier exists only once the provider that holds it has been built.
    pub fn has_dependency_tier(&self) -> bool {
        self.resolver.as_deref().is_some_and(|r| r.jdk_index().has_dependency_tier())
    }

/// The binary name of a type that is a **sibling in the buffer's own package** — the one kind of
/// name that is in scope with no import at all (JLS §7.3).
///
/// It costs nothing in a project file, where the siblings are project types the ordinary go-to
/// already opens. It is the difference between working and not inside a **library source view**,
/// where the neighbours are exactly what the code refers to most: `ApplicationContext extends
/// MessageSource, ApplicationEventPublisher` names two types in its own package, neither of them
/// imported, and reading only the import list made both a dead go-to.
fn same_package_type(
    resolver: &dyn bennu_java::prelude::TypeResolver,
    package: Option<&str>,
    name: &str,
) -> Option<String> {
    let package = package?;
    if package.is_empty() {
        return None;
    }
    let binary = format!("{}/{}", package.replace('.', "/"), name);
    // Confirmed against the classpath rather than assumed: the caret can be on any capitalised
    // word — a javadoc reference, a type parameter, a name from a package that does not exist —
    // and returning a binary nobody can decode would turn "go-to found nothing" into "go-to
    // opened an empty stub". `members_of` is the same lookup the stub itself is built from, so a
    // hit here is a view that will render.
    resolver.members_of(&binary).map(|_| binary)
}

/// The binary name of the type a **static import** names, for a `name` written inside one.
///
/// Two carets, one answer: on the type (`…handler.HandlerFunctions.http`, caret on
/// `HandlerFunctions`) it is that type, and on the member (caret on `http`) it is the type that
/// declares it — which is the only thing there is to open, since a member has no file of its own.
/// A star import (`import static a.b.C.*;`) binds no member name, so only its type matches.
///
/// Nested types are why the match is on the last SEGMENT rather than on the whole owner:
/// `import static a.b.Outer.Inner.of;` has owner `a/b/Outer/Inner`, and the caret can be on either
/// half of it.
fn static_import_type(imports: &[bennu_java::prelude::Import], name: &str) -> Option<String> {
    let targets = bennu_java::prelude::static_import_targets(imports);
    // The type first: a caret on a segment of the owner means that type, not the member's owner.
    for t in &targets {
        if let Some(at) = t.owner_binary.rfind(&format!("/{name}")) {
            // Everything up to and including the matched segment — `a/b/Outer` for a caret on
            // `Outer` in `a/b/Outer/Inner`.
            let end = at + 1 + name.len();
            if t.owner_binary[end..].is_empty() || t.owner_binary[end..].starts_with('/') {
                return Some(t.owner_binary[..end].to_string());
            }
        }
    }
    // Then the member: open the type that declares it.
    targets
        .iter()
        .find(|t| t.member.as_deref() == Some(name))
        .map(|t| t.owner_binary.clone())
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
            let symbols = bennu_java::prelude::extract_symbols(source);
            let imports = symbols.imports;
            match resolver.resolve_simple_name(name, &imports) {
                Some(b) => b,
                // A name written only inside a **static** import resolves through no ordinary
                // import entry: `import static a.b.C.http;` binds `http`, not `C`, so neither the
                // type nor the member is a simple name the resolver can look up — and go-to on
                // either did nothing at all. The import itself says what they are.
                None => Self::static_import_type(&imports, name)
                    // …and a sibling in the buffer's OWN package is imported by nobody, because
                    // JLS §7.3 already put it in scope. Reading only the imports made every such
                    // name unresolvable, which is invisible in a project file (its siblings are
                    // project types, resolved elsewhere) and constant inside a library source
                    // view: `ApplicationContext` extends `MessageSource` and
                    // `ApplicationEventPublisher`, both `org.springframework.context`, both with
                    // no import line and both a dead go-to. Last, so an explicit import still
                    // wins the name.
                    .or_else(|| Self::same_package_type(resolver, symbols.package.as_deref(), name))?,
            }
        };
        if resolver.is_project_type(&binary) {
            return None;
        }
        // Asked in this order on purpose. `is_project_type` wants the **project's** spelling of a
        // nested type (`pkg/Outer/Inner`, which is what its index files), and everything downstream
        // of here wants the **bytecode's** (`pkg/Outer$Inner`, which is what a jar holds): the
        // decompiled view writes this name into its `package` line, and "Download sources" probes
        // each jar's central directory for it literally. A dotted name reaches here with every
        // separator turned into a slash — right for a top-level type and wrong for every nested one
        // — and the mistake was invisible because reading a nested type's MEMBERS retries the `$`
        // form internally. So the members were real and the name was not: `DefaultParts.FluxContent`
        // opened a stub declaring `package …multipart.DefaultParts;`, and its Download sources said
        // no dependency jar contained the type. Truthfully, about a name nothing had.
        Some(resolver.bytecode_name(&binary))
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
        declarable_type_at(source, start, end, self.resolver.as_deref()?)
    }

    /// [`Self::infer_type_source`] keeping the two ways of failing apart — see [`Declarable`].
    ///
    /// No resolver at all is [`Declarable::Unknown`]: a provider that has not finished indexing has
    /// no opinion about this expression, which is a different thing from having one it must not
    /// write down.
    pub fn infer_type_detail(&self, source: &str, start: usize, end: usize) -> Declarable {
        match self.resolver.as_deref() {
            Some(r) => declarable_type_detail(source, start, end, r),
            None => Declarable::Unknown,
        }
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
            for sup in cm.superclass.iter().chain(cm.interfaces.iter()) {
                queue.push((sup.binary_name.clone(), depth + 1));
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
            // The card is about the variable, not about its type — a library doc pulled in here
            // would answer a question nobody asked. `owner` stays empty for the same reason.
            ..Default::default()
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
            // The brackets were trimmed off `base` two lines up; the depth they carried is the
            // difference between `Foo` and `Foo[]` and belongs on the reference.
            dims: written.matches("[]").count().min(u8::MAX as usize) as u8,
            wildcard: false,
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

impl NativeJavaProvider {
    /// How a **nested** type has to be written here, and the import that makes it work.
    ///
    /// `Inner` alone is not a name Java resolves: a nested type is reached through its outer, or
    /// through an `import pkg.Outer.Inner;` that names it exactly. So completing `MyProva` and
    /// inserting `MyProva` produced code that does not compile — the one thing a completion must
    /// never do — and the auto-import beside it named a class that is not importable by that name.
    ///
    /// The answer is `Outer.Inner` plus `import pkg.Outer`. It is what people write by hand, one
    /// import serves every nested type of that class, and it reads as what it is.
    ///
    /// `None` — meaning "the simple name is fine" — when the type is not nested, when the file
    /// already imports it outright, or when the caret is inside the outer class (where its own
    /// member types are in scope, JLS §6.5.5.1). That last case is the common one: a nested class
    /// used by the class that declares it.
    fn nested_form(
        &self,
        fqn: &str,
        symbols: &bennu_java::prelude::FileSymbols,
        site: Option<&str>,
    ) -> Option<(String, String)> {
        nested_form_of(fqn, symbols, site, &|b| self.is_project_type(b))
    }
}

/// [`NativeJavaProvider::nested_form`], with "is this a project type" passed in — so the rule can
/// be tested without an index behind it.
fn nested_form_of(
    fqn: &str,
    symbols: &bennu_java::prelude::FileSymbols,
    site: Option<&str>,
    is_project: &dyn Fn(&str) -> bool,
) -> Option<(String, String)> {
    {
        let binary = fqn.replace('.', "/");
        let (outer, inner) = binary.rsplit_once('/')?;
        // The segment before the last is a TYPE, not a package — which is what makes this nested.
        // Asked of the index rather than guessed from the capital letter, because a package
        // segment may be capitalised and a legacy tree has some.
        if !is_project(outer) {
            return None;
        }
        if symbols
            .imports
            .iter()
            .any(|i| !i.static_ && !i.star && i.path == fqn)
        {
            return None;
        }
        // Inside the outer, or inside something nested within it.
        if let Some(site) = site {
            if site == outer || site.starts_with(&format!("{outer}/")) {
                return None;
            }
        }
        let outer_simple = outer.rsplit('/').next()?;
        Some((format!("{outer_simple}.{inner}"), outer.replace('/', ".")))
    }
}

#[cfg(test)]
mod nested_form_tests {
    use super::nested_form_of;

    fn symbols(src: &str) -> bennu_java::prelude::FileSymbols {
        bennu_java::prelude::extract_symbols(src)
    }

    /// The reported case: a nested class completed from another file. `MyProva` alone does not
    /// compile, and the import beside it named a class that is not importable by that name.
    #[test]
    fn a_nested_type_is_written_through_its_outer() {
        let s = symbols("package other;\npublic class Use {}\n");
        let got = nested_form_of(
            "cfg.ConfigurazioneCors.MyProva",
            &s,
            Some("other/Use"),
            &|b| b == "cfg/ConfigurazioneCors",
        );
        assert_eq!(
            got,
            Some(("ConfigurazioneCors.MyProva".to_string(), "cfg.ConfigurazioneCors".to_string()))
        );
    }

    /// A top-level type is already a name Java resolves. The segment before it is a package, and
    /// asking the index is what tells the two apart — a package segment may be capitalised.
    #[test]
    fn a_top_level_type_needs_nothing() {
        let s = symbols("package other;\npublic class Use {}\n");
        assert_eq!(nested_form_of("cfg.Order", &s, None, &|_| false), None);
    }

    /// `import cfg.ConfigurazioneCors.MyProva;` names it exactly — the simple name is in scope and
    /// qualifying it would be noise.
    #[test]
    fn an_outright_import_leaves_the_simple_name_alone() {
        let s = symbols("package other;\nimport cfg.ConfigurazioneCors.MyProva;\npublic class Use {}\n");
        assert_eq!(
            nested_form_of("cfg.ConfigurazioneCors.MyProva", &s, None, &|b| b == "cfg/ConfigurazioneCors"),
            None
        );
    }

    /// The common case: a nested class used by the class that declares it. Its own member types
    /// are in scope (JLS §6.5.5.1).
    #[test]
    fn inside_the_outer_the_simple_name_is_in_scope() {
        let s = symbols("package cfg;\npublic class ConfigurazioneCors {}\n");
        assert_eq!(
            nested_form_of(
                "cfg.ConfigurazioneCors.MyProva",
                &s,
                Some("cfg/ConfigurazioneCors"),
                &|b| b == "cfg/ConfigurazioneCors",
            ),
            None
        );
    }

    /// …and from a SIBLING nested class, which is inside the outer too.
    #[test]
    fn inside_a_sibling_nested_class_the_simple_name_is_in_scope() {
        let s = symbols("package cfg;\npublic class ConfigurazioneCors {}\n");
        assert_eq!(
            nested_form_of(
                "cfg.ConfigurazioneCors.MyProva",
                &s,
                Some("cfg/ConfigurazioneCors/Altra"),
                &|b| b == "cfg/ConfigurazioneCors",
            ),
            None
        );
    }
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


// ── which of two equally-matching names this file probably meant ─────────────────────────────

/// What the buffer says about which package the caret lives in, and what the project says about
/// which candidate it usually means.
///
/// The ranking terms the name index cannot supply. It knows how well a name matches what was typed
/// and whether the project declares it — which is right, and which leaves `Order` in a Spring
/// project offering `java.awt`'s neighbours above the `Order` three files away.
///
/// ## Two questions, and they are not the same question
///
/// **Certainty** is what *this file* has already said. If it imports `it.acme.model.Order`, there
/// is nothing to rank: the file named the one it means. Its own package and a wildcard it imports
/// are the same kind of fact, one step weaker. Nothing may outrank these — not popularity, not
/// anything — because they are not evidence about the answer, they *are* the answer.
///
/// **Everything else** is weighed. Two terms:
///
/// * **distance**, by shared package segments, then the JDK, then the rest of the classpath;
/// * **popularity**, from [`ImportCensus`] — how many files in this project import that exact type.
///
/// They are combined rather than ordered, and that is the whole design decision. Ordering distance
/// first would mean a `it.acme.model.List` nobody has ever imported permanently outranking the
/// `java.util.List` written in four hundred files, purely for sharing two package segments —
/// which is the case the census exists for. Ordering popularity first would let one import
/// anywhere beat a sibling package. So popularity is worth [`POPULARITY_WEIGHT`] points a band,
/// which is enough for real evidence to cross the distance tiers and not enough for weak evidence
/// to.
///
/// With the census off, every popularity is zero and this degrades exactly to distance — the
/// counts are not consulted at all rather than consulted with a smaller weight.
struct Proximity<'a> {
    /// The package the buffer declares, dotted. Empty for the default package.
    package: String,
    /// The FQNs this file imports, plus its `.*` imports as written.
    imported: Vec<String>,
    /// The project's import census, or `None` when the setting is off.
    census: Option<&'a ImportCensus>,
}

/// What one band of [`ImportCensus::popularity`] is worth against distance.
///
/// Eight bands at four points each is a 28-point swing over a distance range of 10..40 — sized so
/// a type this project leans on can cross from the far end, and one imported once or twice cannot
/// cross from anywhere.
const POPULARITY_WEIGHT: i32 = 4;

/// How near a candidate is, lower first: the certainty tier, then the weighed score within it.
type Rank = (u8, i32);

impl<'a> Proximity<'a> {
    fn of(text: &str, census: Option<&'a ImportCensus>) -> Self {
        let symbols = bennu_java::prelude::extract_symbols(text);
        Self {
            package: symbols.package.unwrap_or_default(),
            imported: symbols.imports.into_iter().map(|i| i.path).collect(),
            census,
        }
    }

    /// A sort key over a simple name's candidate FQNs — **lower is nearer**. The name is ranked by
    /// its nearest candidate: one good reading is what makes a name worth offering.
    fn rank(&self, candidates: &[String]) -> Rank {
        candidates.iter().map(|fqn| self.rank_one(fqn)).min().unwrap_or((u8::MAX, i32::MAX))
    }

    fn rank_one(&self, fqn: &str) -> Rank {
        if self.imported.iter().any(|i| i == fqn) {
            return (0, 0);
        }
        let package = fqn.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
        // The file wildcard-imports the package — it named it, one step less specifically. The
        // import's `path` is already written without the `.*` (see `Import::star`).
        if self.imported.iter().any(|i| i == package) {
            return (1, 0);
        }
        if package == self.package {
            return (2, 0);
        }
        // Nearer the more of the package chain is shared, so a sibling beats a cousin. Bounded at
        // eight segments, which is deeper than any package anybody navigates.
        let shared = shared_segments(&self.package, package).min(8) as i32;
        let distance = if shared > 0 {
            10 + (8 - shared)
        } else if fqn.starts_with("java.") || fqn.starts_with("javax.") {
            // Not because the JDK is special, but because a name matching both a JDK type and
            // something in a jar nobody here has opened is, overwhelmingly, the JDK one.
            30
        } else {
            40
        };
        let popularity = self.census.map_or(0, |c| c.popularity(fqn)) as i32;
        (3, distance - popularity * POPULARITY_WEIGHT)
    }
}

/// How many leading dot-separated segments two package names share.
fn shared_segments(a: &str, b: &str) -> usize {
    a.split('.').zip(b.split('.')).take_while(|(x, y)| x == y && !x.is_empty()).count()
}

/// Whether `fqn` names an annotation type, according to the resolver that can read its flags.
///
/// `false` when the type cannot be resolved at all, which is the direction that shows *fewer*
/// wrong things: an unreadable type offered above an `@` is a name that will not compile there.
fn is_annotation_type(resolver: &dyn TypeResolver, fqn: &str) -> bool {
    resolver
        .members_of(&fqn.replace('.', "/"))
        .is_some_and(|m| m.flags.is_annotation)
}

/// The raw source of a `@Target(...)`'s argument, however it was written — `@Target(METHOD)`,
/// `@Target({METHOD, FIELD})` and `@Target(value = METHOD)` all reach here.
fn target_text_of(target: bennu_java::prelude::Annotation) -> String {
    if let Some(first) = target.positional.first() {
        return first.clone();
    }
    target
        .args
        .iter()
        .find(|(k, _)| k == "value")
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

impl NativeJavaProvider {
    /// Completion at `at`, saying whether the **import census** may order the answer.
    ///
    /// The flag is a parameter and not a field because it is a *setting*, read per request: a
    /// switch that took effect at the next index build would stop counting immediately in one
    /// direction and go on ranking by stale counts in the other. Off means the census is not
    /// consulted at all — not consulted with a lower weight.
    pub fn complete_at(
        &self,
        at: &Position,
        source: Option<&str>,
        opts: CompletionOptions,
    ) -> Result<Vec<CompletionItem>, IntelError> {
        let CompletionOptions { census, case } = opts;
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
            bennu_query::prelude::completion_in(text, offset, resolver, Some(&self.class_names), case);
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
        // An `@` narrows the legal names harder than anything else in Java — from every type on the
        // classpath to the annotation types on it — so it gets its own answer rather than being
        // left to the general type-name path, which offered every class in the project above an
        // `@`. See `annotation_completions`.
        if let Some(site) = bennu_java::prelude::annotation_site(text, offset) {
            return Ok(self.annotation_completions(text, &site, census, case));
        }
        // Otherwise the caret is on a BARE identifier, and two indexes can answer it: the scope
        // around the caret (locals, parameters, the enclosing type's own members, static imports)
        // and the class-name index (the JDK, the dependency jars and the project's own types).
        //
        // Which goes first is decided by Java's own naming law rather than by a score, because the
        // two lists are not comparable: `Str` is a type being written and `str` is a variable, and
        // no amount of ranking makes one of those the other. A SCREAMING_CASE prefix is a
        // constant, so it stays with the scope — that is where a constant of your own class is.
        let mut out = bennu_query::prelude::scope_completion(text, offset, resolver, case);
        // The members this class does not have YET — the accessors a field is missing, and the
        // methods the class calls without declaring. Offered first, and only at a member position:
        // there, `getNa` is a declaration being written, not a call — and a call is what every
        // other candidate in the list would be.
        let generated = crate::accessor_completion::generated_members(
            text,
            offset,
            case,
            Some(resolver as &dyn TypeResolver),
        );
        if !generated.is_empty() {
            let mut merged = generated;
            merged.extend(out);
            out = merged;
        }
        let types = self.type_completions(text, offset, census, case);
        let (_, prefix) = bennu_query::prelude::split_completion_prefix(text, offset);
        if looks_like_a_type_name(&prefix) {
            let mut typed = types;
            typed.extend(out);
            return Ok(typed);
        }
        out.extend(types);
        Ok(out)
    }
}

/// What a completion request was asked with, beyond where the caret is.
///
/// A struct rather than two more positional arguments, because that is what the second one would
/// have made it: `complete_at(at, source, true, MatchCase::All)` says nothing about which `true`
/// is which, and the next setting would say even less.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompletionOptions {
    /// Whether to rank type names by how often the PROJECT imports each candidate. A user setting
    /// (Settings → Completion), off meaning "do not consult the counts".
    pub census: bool,
    /// How strictly the typed letters must agree with a candidate's — see [`MatchCase`].
    pub case: MatchCase,
}

impl CompletionOptions {
    /// The defaults a programmatic caller gets: the census on, the case rule at its sensible
    /// middle. What the seam's own `completion` uses.
    pub fn with_census(census: bool) -> Self {
        Self { census, ..Self::default() }
    }
}

/// Whether a written prefix reads as a TYPE name rather than a value — PascalCase, an initial
/// capital with at least one lowercase letter after it.
///
/// The same test the editor's postfix templates use to decide whether `Foo.` is a type receiver,
/// and Java's own convention rather than a guess: `Order` is a class, `ORDER` is a constant and
/// `order` is a variable. It decides ORDER, never membership — both lists are always offered, so
/// a prefix read the "wrong" way costs a scroll, not an answer.
fn looks_like_a_type_name(prefix: &str) -> bool {
    let mut chars = prefix.chars();
    chars.next().is_some_and(|c| c.is_uppercase()) && chars.any(|c| c.is_lowercase())
}

impl IntelProvider for NativeJavaProvider {
    /// The seam's completion: the census on, which is its default. A caller that has read the
    /// setting calls [`complete_at`](NativeJavaProvider::complete_at) instead.
    fn completion(
        &self,
        at: &Position,
        source: Option<&str>,
    ) -> Result<Vec<CompletionItem>, IntelError> {
        self.complete_at(at, source, CompletionOptions::with_census(true))
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
/// The type to WRITE at a declaration for the expression in `[start, end)`, with the imports it
/// needs — or `None` when there is no type a declaration could carry.
///
/// The `None` cases are the point. A call that returns nothing infers as `void`, and `void x = f();`
/// is not a declaration with a bad type, it is a syntax error — measured as 685 of them on one
/// library before this existed. Every caller that writes a type into source goes through here, so
/// there is one answer to "may this be declared" rather than one per call site.
pub fn declarable_type_at(
    source: &str,
    start: usize,
    end: usize,
    resolver: &dyn bennu_java::prelude::TypeResolver,
) -> Option<(String, Vec<String>)> {
    match declarable_type_detail(source, start, end, resolver) {
        Declarable::Writable(written, imports) => Some((written, imports)),
        _ => None,
    }
}

/// What [`declarable_type_at`] found, with the two ways of failing kept apart.
///
/// They are not the same fact and a caller may need to act differently on them. Nothing inferred
/// means the engine has no opinion, and `var` — which is what javac infers anyway — is a perfectly
/// good stand-in. A type inferred and rejected means the engine DOES have an opinion and it is one
/// no declaration may carry: `void`, a type variable the class never declared, a captured wildcard
/// substituted down to `Object`. Where the surrounding code is what decides the type, that second
/// answer is the fingerprint of a poly expression re-inferred with nothing to infer from, and the
/// only one of the two worth refusing on.
pub enum Declarable {
    /// A type to write, with the imports it needs.
    Writable(String, Vec<String>),
    /// A type was inferred, and it is not one a declaration may carry.
    Unwritable,
    /// Nothing was inferred.
    Unknown,
}

pub fn declarable_type_detail(
    source: &str,
    start: usize,
    end: usize,
    resolver: &dyn bennu_java::prelude::TypeResolver,
) -> Declarable {
    let Some(tr) = bennu_java::prelude::infer_expression_type(source, start, end, resolver) else {
        return Declarable::Unknown;
    };
    let mut imports: Vec<String> = Vec::new();
    let written = render_type_for_source_with(&tr, &mut imports, &|binary| {
        resolver.members_of(binary).is_some()
    });
    if written.is_empty() || written == "void" {
        return Declarable::Unwritable;
    }
    // A BINARY name with no package is a TYPE VARIABLE — `T`, `E` — and writing it into a
    // declaration puts a name there the surrounding class has never heard of. Judged on the binary
    // name and not the rendered one: `java/lang/String` renders as plain `String`, which the first
    // version of this rule threw away along with two thirds of the constants it should have named.
    // Recursive: a type variable hides in the ARGUMENTS as readily as at the top — `List<T>` has a
    // perfectly good binary name and a `T` inside it that the target class never declared.
    if names_a_type_variable(&tr) {
        return Declarable::Unwritable;
    }
    // A type ARGUMENT that came out as `Object` is nearly always a captured wildcard the engine
    // substituted away: `cl.getClass()` is `Class<?>`, and `Class<Object> c = cl.getClass()` does
    // not compile. The two are indistinguishable once substituted, so this declines rather than
    // guesses — and declining is a good outcome for a local, which then keeps `var`, exactly what
    // javac itself would have inferred.
    // A captured wildcard has no name anyone can type. `a.annotationType()` is a
    // `Class<? extends Annotation>`; the decoder collapses that onto its bound, which is what every
    // member lookup needs and is NOT what `Class<Annotation> c = a.annotationType();` means to the
    // compiler. The bit says the bound is standing in for something unwritable — and where the
    // whole point of the declaration is to write it down, that is a refusal. A local then keeps
    // `var`, which is what javac infers there anyway.
    if tr.names_a_wildcard() {
        return Declarable::Unwritable;
    }
    if names_object_as_a_type_argument(&tr) {
        return Declarable::Unwritable;
    }
    // A type in the file's OWN package needs no import, and asking for one is not merely redundant:
    // the nested types of a class in this package are written `Outer.Inner`, and an import of one
    // that happens to be `private` does not compile — `Range.ComparableComparator has private
    // access`, inside `Range.java` itself. Same package, no import, question closed.
    let own = package_of(source);
    let imports = imports
        .into_iter()
        .filter(|fqn| !own.as_ref().is_some_and(|p| fqn.starts_with(&format!("{p}."))))
        .collect();
    Declarable::Writable(written, imports)
}

/// Whether `Object` appears anywhere as a type ARGUMENT of `tr` — at any depth, in any position.
///
/// Read off the tree rather than off the rendered string, which is where the first version of this
/// lived: `<Object>`, `<Object,` and `, Object>` between them miss the MIDDLE argument of a
/// three-parameter generic, and `Collector<CharSequence, Object, String>` — what
/// `Collectors.joining()` becomes once its capture is substituted away — went through as a type to
/// write.
fn names_object_as_a_type_argument(tr: &bennu_java::prelude::TypeRef) -> bool {
    tr.type_args
        .iter()
        .any(|a| a.binary_name == "java/lang/Object" || names_object_as_a_type_argument(a))
}

/// The package a Java source declares, if it declares one.
fn package_of(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("package ")
            .map(|rest| rest.trim_end_matches(';').trim().to_string())
            .filter(|p| !p.is_empty())
    })
}

/// Whether this type, anywhere in it, is a bare type variable.
fn names_a_type_variable(tr: &bennu_java::prelude::TypeRef) -> bool {
    let binary = tr.binary_name.trim().trim_end_matches("[]");
    if !binary.contains('/') && !is_primitive(binary) {
        return true;
    }
    tr.type_args.iter().any(names_a_type_variable)
}

fn is_primitive(written: &str) -> bool {
    matches!(
        written,
        "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double"
    )
}

pub fn render_type_for_source(
    tr: &bennu_java::prelude::TypeRef,
    imports: &mut Vec<String>,
) -> String {
    render_type_for_source_with(tr, imports, &|_| false)
}

/// [`render_type_for_source`], told which binary names are TYPES.
///
/// A library type spells its nesting with `$` (`java/util/Map$Entry`), and the split is free. A
/// PROJECT type does not: the index turns `p.Processor.Arch` into `p/Processor/Arch`, so nesting and
/// packaging look identical, and the last segment came out as the whole name — `Arch bIT_64 =
/// Processor.Arch.BIT_64;`, which does not compile, because `Arch` is not a name `ArchUtils` can
/// see. Asking the resolver whether the path one segment up is itself a type is what tells the two
/// apart, and it is the only thing that can.
pub fn render_type_for_source_with(
    tr: &bennu_java::prelude::TypeRef,
    imports: &mut Vec<String>,
    is_type: &dyn Fn(&str) -> bool,
) -> String {
    let binary = tr.binary_name.trim();
    if binary.is_empty() {
        return String::new();
    }
    let written = if binary.contains('/') {
        let (outer_binary, project_nested) = split_project_nesting(binary, is_type);
        let dotted = outer_binary.replace('/', ".");
        let (outer, nested) = match dotted.split_once('$') {
            Some((outer, rest)) => (outer.to_string(), rest.replace('$', ".")),
            None => (dotted.clone(), String::new()),
        };
        let nested = match (nested.is_empty(), project_nested.is_empty()) {
            (_, true) => nested,
            (true, false) => project_nested,
            (false, false) => format!("{nested}.{project_nested}"),
        };
        // `java.lang` is implicit; importing it is noise the compiler already has. The name is the
        // ELEMENT type — the depth lives in `dims` — so it is already the thing to import; the trim
        // stays for an index persisted before that was true.
        let importable = outer.trim_end_matches("[]").to_string();
        if !importable.starts_with("java.lang.") || importable.matches('.').count() > 2 {
            imports.push(importable);
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
        return tr.with_brackets(&written);
    }
    let args: Vec<String> = tr
        .type_args
        .iter()
        .map(|a| render_type_for_source_with(a, imports, is_type))
        .collect();
    tr.with_brackets(&format!("{written}<{}>", args.join(", ")))
}

/// Split a `/`-separated binary into the outermost type and the nested names under it, by asking
/// `is_type` about each prefix. `p/Processor/Arch` with `p/Processor` a known type gives
/// `("p/Processor", "Arch")`; a plain `p/Processor` gives `("p/Processor", "")`.
fn split_project_nesting(binary: &str, is_type: &dyn Fn(&str) -> bool) -> (String, String) {
    let mut segments: Vec<&str> = binary.split('/').collect();
    let mut nested: Vec<&str> = Vec::new();
    // Bounded by the segment count, which is what the loop consumes.
    while segments.len() > 1 {
        let cut = segments.len() - 1;
        let head = segments[..cut].join("/");
        if !is_type(&head) {
            break;
        }
        nested.insert(0, segments[cut]);
        segments.truncate(cut);
    }
    (segments.join("/"), nested.join("."))
}

#[cfg(test)]
mod tests {
    use super::{
        names_object_as_a_type_argument, reads_as_type_name, render_stub, render_type_for_source,
        render_type_for_source_with,
    };
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
    /// The depth is the difference between two programs, so it survives the round trip — and the
    /// import is of the element, because `import java.net.URL[];` does not parse.
    #[test]
    fn an_array_keeps_its_brackets_and_imports_its_element() {
        let mut imports = Vec::new();
        let urls = TypeRef::simple("java/net/URL").arrayed(1);
        assert_eq!(render_type_for_source(&urls, &mut imports), "URL[]");
        assert_eq!(imports, ["java.net.URL"]);

        let grid = TypeRef::simple("java/lang/String").arrayed(2);
        assert_eq!(render_type_for_source(&grid, &mut Vec::new()), "String[][]");
    }

    /// A generic array puts the brackets outside the arguments, where Java puts them.
    #[test]
    fn a_generic_array_brackets_the_whole_type() {
        let mut list_of_string = TypeRef::simple("java/util/List");
        list_of_string.type_args.push(TypeRef::simple("java/lang/String"));
        assert_eq!(
            render_type_for_source(&list_of_string.arrayed(1), &mut Vec::new()),
            "List<String>[]"
        );
    }

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
            dims: 0,
            wildcard: false,
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
            superclass: Some(TypeRef::simple("java/lang/Object")),
            interfaces: vec![TypeRef::simple("java/lang/Iterable")],
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
            superclass: Some(TypeRef::simple("java/lang/Object")),
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
            superclass: Some(TypeRef::simple("java/lang/Object")),
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
            superclass: Some(TypeRef::simple("java/lang/Object")),
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


#[cfg(test)]
mod static_import_tests {
    use super::*;

    fn imports_of(source: &str) -> Vec<bennu_java::prelude::Import> {
        bennu_java::prelude::extract_symbols(source).imports
    }

    const SRC: &str = r#"
import static org.springframework.cloud.gateway.server.mvc.handler.HandlerFunctions.http;
import static java.lang.String.format;
import static com.acme.Outer.Inner.of;
import static com.acme.Constants.*;
class C {}
"#;

    /// A caret on the TYPE half of a static import. Go-to did nothing here: the name is bound by no
    /// ordinary import, so `resolve_simple_name` — which reads the file's imports for a type — had
    /// nothing to answer with.
    #[test]
    fn the_type_of_a_static_import_resolves() {
        let imports = imports_of(SRC);
        assert_eq!(
            NativeJavaProvider::static_import_type(&imports, "HandlerFunctions").as_deref(),
            Some("org/springframework/cloud/gateway/server/mvc/handler/HandlerFunctions")
        );
        assert_eq!(
            NativeJavaProvider::static_import_type(&imports, "String").as_deref(),
            Some("java/lang/String")
        );
    }

    /// A caret on the MEMBER half opens the type that declares it — a method has no file of its own,
    /// so its owner is the only honest destination.
    #[test]
    fn the_member_of_a_static_import_resolves_to_its_owner() {
        let imports = imports_of(SRC);
        assert_eq!(
            NativeJavaProvider::static_import_type(&imports, "http").as_deref(),
            Some("org/springframework/cloud/gateway/server/mvc/handler/HandlerFunctions")
        );
        assert_eq!(
            NativeJavaProvider::static_import_type(&imports, "format").as_deref(),
            Some("java/lang/String")
        );
    }

    /// `import static com.acme.Outer.Inner.of;` — the caret can be on either half of a nested owner,
    /// and each names a different type. Matching the whole owner would answer only for `Inner`.
    #[test]
    fn either_half_of_a_nested_owner_resolves_to_that_half() {
        let imports = imports_of(SRC);
        assert_eq!(
            NativeJavaProvider::static_import_type(&imports, "Inner").as_deref(),
            Some("com/acme/Outer/Inner")
        );
        assert_eq!(
            NativeJavaProvider::static_import_type(&imports, "Outer").as_deref(),
            Some("com/acme/Outer")
        );
    }

    /// A star static import binds no member name, so only its type is a caret target.
    #[test]
    fn a_star_static_import_offers_its_type_and_nothing_else() {
        let imports = imports_of(SRC);
        assert_eq!(
            NativeJavaProvider::static_import_type(&imports, "Constants").as_deref(),
            Some("com/acme/Constants")
        );
        assert_eq!(NativeJavaProvider::static_import_type(&imports, "nothing"), None);
    }
}

#[cfg(test)]
mod same_package_tests {
    use std::sync::Arc;

    use bennu_java::prelude::{ClassFlags, ClassMembers, TypeResolver};

    use super::*;

    /// A resolver that knows exactly one set of classes and resolves nothing by import — enough to
    /// ask the only question this function has: does the buffer's own package plus this name name
    /// something on the classpath?
    struct Known(&'static [&'static str]);

    fn empty_members() -> Arc<ClassMembers> {
        Arc::new(ClassMembers {
            superclass: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags::default(),
            type_params: Vec::new(),
        })
    }

    impl TypeResolver for Known {
        fn members_of(&self, binary_name: &str) -> Option<Arc<ClassMembers>> {
            self.0.contains(&binary_name).then(empty_members)
        }
        fn resolve_simple_name(
            &self,
            _name: &str,
            _imports: &[bennu_java::prelude::Import],
        ) -> Option<String> {
            None
        }
    }

    const CLASSPATH: &[&str] = &[
        "org/springframework/context/MessageSource",
        "org/springframework/context/ApplicationEventPublisher",
    ];

    /// The reported bug, at its root: inside a library source view every sibling of the type being
    /// read is in scope with no import (JLS §7.3), and reading only the import list made each one
    /// resolve to nothing — so go-to on `MessageSource` from `ApplicationContext` did nothing at all.
    #[test]
    fn a_sibling_in_the_buffers_own_package_resolves_without_an_import() {
        let r = Known(CLASSPATH);
        assert_eq!(
            NativeJavaProvider::same_package_type(&r, Some("org.springframework.context"), "MessageSource")
                .as_deref(),
            Some("org/springframework/context/MessageSource")
        );
    }

    /// Confirmed against the classpath, never assumed. The caret can be on any capitalised word —
    /// a javadoc reference, a type parameter — and answering with a binary nobody can decode would
    /// turn "found nothing" into "opened an empty stub".
    #[test]
    fn a_name_the_classpath_does_not_have_is_not_invented() {
        let r = Known(CLASSPATH);
        assert_eq!(
            NativeJavaProvider::same_package_type(&r, Some("org.springframework.context"), "Nonexistent"),
            None
        );
        // Right name, wrong package.
        assert_eq!(
            NativeJavaProvider::same_package_type(&r, Some("com.acme"), "MessageSource"),
            None
        );
    }

    /// A buffer with no `package` line (the default package, and every unparseable buffer) has no
    /// sibling scope to consult.
    #[test]
    fn a_buffer_without_a_package_resolves_nothing() {
        let r = Known(CLASSPATH);
        assert_eq!(NativeJavaProvider::same_package_type(&r, None, "MessageSource"), None);
        assert_eq!(NativeJavaProvider::same_package_type(&r, Some(""), "MessageSource"), None);
    }

    /// A library type spells its nesting with `$`; a PROJECT type reaches the index as
    /// `p/Processor/Arch`, where nesting and packaging look identical. Written from the last
    /// segment alone it came out `Arch bIT_64 = Processor.Arch.BIT_64;` — and `Arch` is not a name
    /// `ArchUtils` can see. Asking which prefix is itself a type is the only thing that can tell
    /// the two apart.
    #[test]
    fn a_project_nested_type_is_written_through_its_outer_name() {
        let is_type = |b: &str| b == "org/apache/commons/lang3/Processor";
        let mut imports = Vec::new();
        let arch = bennu_java::prelude::TypeRef::simple("org/apache/commons/lang3/Processor/Arch");
        assert_eq!(
            render_type_for_source_with(&arch, &mut imports, &is_type),
            "Processor.Arch"
        );
        // The import is the OUTER type, which is the one a file can import.
        assert_eq!(imports, vec!["org.apache.commons.lang3.Processor".to_string()]);
        // And a path whose prefix is only a package is untouched.
        let plain = bennu_java::prelude::TypeRef::simple("org/apache/commons/lang3/Processor");
        assert_eq!(
            render_type_for_source_with(&plain, &mut Vec::new(), &is_type),
            "Processor"
        );
    }

    /// The refusal the whole `wildcard` bit exists for: a type that carries a capture at any depth
    /// is not one a declaration may be written with, and the outer name looks perfectly ordinary.
    #[test]
    fn a_captured_wildcard_anywhere_makes_a_type_unwritable() {
        use bennu_java::prelude::TypeRef;
        let capture = TypeRef {
            binary_name: "java/lang/Class".into(),
            type_args: vec![TypeRef::simple("java/lang/annotation/Annotation").captured()],
            dims: 0,
            wildcard: false,
        };
        assert!(capture.names_a_wildcard());
        // Nested one level deeper — the shape `Map<String, ? extends Factory>` has.
        let nested = TypeRef {
            binary_name: "java/util/Map".into(),
            type_args: vec![TypeRef::simple("java/lang/String"), capture.clone()],
            dims: 0,
            wildcard: false,
        };
        assert!(nested.names_a_wildcard());
        // And an ordinary generic stays writable.
        let plain = TypeRef {
            binary_name: "java/util/List".into(),
            type_args: vec![TypeRef::simple("java/lang/String")],
            dims: 0,
            wildcard: false,
        };
        assert!(!plain.names_a_wildcard());
    }

    /// The `Object`-as-a-type-argument rule is read off the tree, not off the rendered string: the
    /// string form missed the MIDDLE argument of a three-parameter generic, which is exactly where
    /// `Collectors.joining()` puts its captured accumulator.
    #[test]
    fn object_is_spotted_in_the_middle_argument_too() {
        use bennu_java::prelude::TypeRef;
        let collector = TypeRef {
            binary_name: "java/util/stream/Collector".into(),
            type_args: vec![
                TypeRef::simple("java/lang/CharSequence"),
                TypeRef::simple("java/lang/Object"),
                TypeRef::simple("java/lang/String"),
            ],
            dims: 0,
            wildcard: false,
        };
        assert!(names_object_as_a_type_argument(&collector));
        // A plain `Object` that is the type ITSELF is not a captured argument and stays writable.
        assert!(!names_object_as_a_type_argument(&TypeRef::simple("java/lang/Object")));
    }

}

// ── which of two equally-matching names this file probably meant ──────────────────────────

mod proximity_tests {
    use super::{shared_segments, Proximity};

    const FILE: &str = "package it.acme.web;\n\
                        import it.acme.model.Order;\n\
                        import java.util.*;\n\
                        class Ctl {}\n";

    fn nearer(file: &str, a: &str, b: &str) -> bool {
        let p = Proximity::of(file, None);
        p.rank(&[a.to_string()]) < p.rank(&[b.to_string()])
    }

    #[test]
    fn what_the_file_already_imports_wins_outright() {
        // The file said which `Order` it means. There is nothing left to rank.
        assert!(nearer(FILE, "it.acme.model.Order", "java.awt.Order"));
        assert_eq!(Proximity::of(FILE, None).rank(&["it.acme.model.Order".into()]).0, 0);
    }

    #[test]
    fn a_wildcard_import_is_the_file_naming_the_package() {
        assert!(nearer(FILE, "java.util.List", "org.other.List"));
    }

    #[test]
    fn the_file_s_own_package_needs_no_import_and_ranks_accordingly() {
        assert!(nearer(FILE, "it.acme.web.Helper", "org.apache.Helper"));
    }

    #[test]
    fn a_sibling_package_beats_a_cousin_and_both_beat_a_stranger() {
        assert!(nearer(FILE, "it.acme.web.dto.Row", "it.other.Row"));
        assert!(nearer(FILE, "it.acme.batch.Row", "org.apache.Row"));
    }

    #[test]
    fn the_jdk_beats_a_jar_nobody_here_has_named() {
        assert!(nearer(FILE, "java.time.Duration", "org.joda.Duration"));
    }

    #[test]
    fn a_name_is_ranked_by_its_nearest_candidate() {
        // `List` resolves to both `java.util` (wildcard-imported here) and somewhere far away.
        // The near one is what makes the name near.
        let p = Proximity::of(FILE, None);
        let both = ["org.far.List".to_string(), "java.util.List".to_string()];
        assert_eq!(p.rank(&both), p.rank(&["java.util.List".to_string()]));
    }

    #[test]
    fn a_file_in_the_default_package_shares_nothing_with_anyone() {
        // The empty package must not read as a prefix of every package there is.
        let p = Proximity::of("class A {}\n", None);
        assert_eq!(p.rank(&["org.apache.Thing".into()]), p.rank(&["it.acme.Thing".into()]));
    }

    #[test]
    fn shared_segments_counts_whole_segments_and_not_characters() {
        assert_eq!(shared_segments("it.acme.web", "it.acme.model"), 2);
        assert_eq!(shared_segments("it.acme", "it.acmerie"), 1);
        assert_eq!(shared_segments("", "it.acme"), 0);
    }
    // ── and when the project's own imports overrule the distance ──────────────────────────────

    /// A census in which `fqn` is imported by `times` files.
    fn used(fqn: &str, times: usize) -> crate::import_census::ImportCensus {
        let mut c = crate::import_census::ImportCensus::default();
        let file = bennu_java::prelude::extract_symbols(&format!("package p;\nimport {fqn};\n"));
        for _ in 0..times {
            c.add_file(&file);
        }
        c
    }

    fn nearer_with(
        census: &crate::import_census::ImportCensus,
        file: &str,
        a: &str,
        b: &str,
    ) -> bool {
        let p = Proximity::of(file, Some(census));
        p.rank(&[a.to_string()]) < p.rank(&[b.to_string()])
    }

    #[test]
    fn a_type_the_project_leans_on_beats_a_near_one_nobody_imports() {
        // The case the census exists for. Without it `it.acme.model.List` wins on a shared package
        // prefix alone — a class nobody has ever imported, above the `List` written in 400 files.
        //
        // A fixture with no `java.util` wildcard on purpose: with one, `java.util.List` wins on
        // certainty and the two terms being measured here never come into it.
        const PLAIN: &str = "package it.acme.web;\nclass Ctl {}\n";
        let c = used("java.util.List", 400);
        assert!(nearer_with(&c, PLAIN, "java.util.List", "it.acme.model.List"));
        // And with the census off the near one wins again — the counts are not consulted at all,
        // which is the whole promise of the switch.
        assert!(nearer(PLAIN, "it.acme.model.List", "java.util.List"));
    }

    #[test]
    fn a_jar_this_project_lives_in_beats_a_near_name_nobody_uses() {
        let c = used("org.apache.commons.lang3.StringUtils", 300);
        assert!(nearer_with(
            &c,
            FILE,
            "org.apache.commons.lang3.StringUtils",
            "it.acme.model.StringUtils",
        ));
    }

    #[test]
    fn one_import_somewhere_is_not_enough_to_cross_a_package() {
        // Weak evidence must not move anything: a type imported once may well be the mistake.
        let c = used("org.random.Row", 1);
        assert!(nearer_with(&c, FILE, "it.acme.model.Row", "org.random.Row"));
    }

    #[test]
    fn what_the_file_itself_imports_cannot_be_outranked_by_any_count() {
        // `FILE` imports `it.acme.model.Order`. The file has already said which one it means, and
        // that is not evidence about the answer — it IS the answer.
        let c = used("java.awt.Order", 5000);
        assert!(nearer_with(&c, FILE, "it.acme.model.Order", "java.awt.Order"));
    }

    #[test]
    fn the_file_s_own_package_also_survives_a_popular_stranger() {
        let c = used("org.far.Helper", 5000);
        assert!(nearer_with(&c, FILE, "it.acme.web.Helper", "org.far.Helper"));
    }

    #[test]
    fn between_two_names_nobody_imports_distance_still_decides() {
        let c = crate::import_census::ImportCensus::default();
        assert!(nearer_with(&c, FILE, "it.acme.model.Thing", "org.far.Thing"));
    }

}
