//! The [`IntelProvider`] trait + its two impl slots.
//!
//! One protocol for every language (docs §2). The Phase-0 skeleton defines the trait
//! and both impls' *shapes*; the bodies are stubs that return empty / unimplemented,
//! so `bennu-be` can wire the seam now and later waves fill the native engine in
//! (and, post-MVP, the LSP client).

use std::path::{Path, PathBuf};

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

/// Render a [`bennu_java::prelude::TypeRef`] to a readable type string — the simple (last-segment)
/// name plus its generic arguments (`java/util/List<com/acme/Foo>` → `List<Foo>`). For the hover
/// card, which wants the written-Java shape, not the binary name.
fn render_type_ref(t: &bennu_java::prelude::TypeRef) -> String {
    let simple = t.binary_name.rsplit(['/', '$']).next().unwrap_or(&t.binary_name);
    if t.type_args.is_empty() {
        simple.to_string()
    } else {
        let args: Vec<String> = t.type_args.iter().map(render_type_ref).collect();
        format!("{simple}<{}>", args.join(", "))
    }
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
        Some(expr) => format!("Type not resolved from the initializer `{}`.", ellipsize(&expr, 120)),
        None => "Type not resolved.".to_string(),
    };
    Some(crate::rename::HoverInfo {
        signature: format!("{written} {name}"),
        kind: "variable".to_string(),
        container: None,
        doc: Some(doc),
    })
}

/// The `(written type, initializer text)` of the local named `name`, searched outwards from `node`
/// through the enclosing scopes. Covers the two forms that carry an inferred type: an ordinary
/// declaration and an enhanced-`for` variable.
fn local_declaration_of(
    node: tree_sitter::Node,
    bytes: &[u8],
    name: &str,
) -> Option<(String, Option<String>)> {
    fn text(n: &tree_sitter::Node, bytes: &[u8]) -> Option<String> {
        n.utf8_text(bytes).ok().map(|s| s.to_string())
    }
    /// `(type, value)` of `decl` when it declares `name`.
    fn declares(
        decl: &tree_sitter::Node,
        bytes: &[u8],
        name: &str,
    ) -> Option<(String, Option<String>)> {
        let declared = decl.child_by_field_name("type").and_then(|t| text(&t, bytes))?;
        if decl.kind() == "enhanced_for_statement" {
            let n = decl.child_by_field_name("name").and_then(|n| text(&n, bytes))?;
            return (n == name)
                .then(|| (declared, decl.child_by_field_name("value").and_then(|v| text(&v, bytes))));
        }
        let mut w = decl.walk();
        for d in decl.named_children(&mut w) {
            if d.kind() != "variable_declarator" {
                continue;
            }
            if d.child_by_field_name("name").and_then(|n| text(&n, bytes)).as_deref() == Some(name) {
                return Some((declared, d.child_by_field_name("value").and_then(|v| text(&v, bytes))));
            }
        }
        None
    }

    let mut scope = Some(node);
    while let Some(s) = scope {
        if matches!(s.kind(), "local_variable_declaration" | "enhanced_for_statement") {
            if let Some(hit) = declares(&s, bytes, name) {
                return Some(hit);
            }
        }
        let mut w = s.walk();
        let mut found = None;
        for c in s.named_children(&mut w) {
            if matches!(c.kind(), "local_variable_declaration" | "enhanced_for_statement") {
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
    let Some(first) = chars.next() else { return false };
    if !(first.is_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    text.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.')
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
    s.push_str("// Decompiled from bytecode — no source attached. Signatures only (method bodies\n");
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
        if let Some(sc) = cm.superclass.as_deref().filter(|sc| *sc != "java/lang/Object") {
            s.push_str(&format!(" extends {}", type_name(sc)));
        }
    }
    if !cm.interfaces.is_empty() {
        let word = if cm.flags.is_interface { "extends" } else { "implements" };
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
                    (format!("{} ", render_type_ref(&m.return_type)), m.name.clone())
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
fn generic_method_core(raw_signature: &str, name: &str, ctor_simple: Option<&str>) -> Option<String> {
    let ms = bennu_classpath::prelude::parse_method_signature(raw_signature).ok()?;
    let type_params = if ms.type_params.is_empty() {
        String::new()
    } else {
        let ps: Vec<String> = ms.type_params.iter().map(render_sig_type_param).collect();
        format!("<{}> ", ps.join(", "))
    };
    let params: Vec<String> =
        ms.params.iter().enumerate().map(|(i, p)| format!("{} arg{i}", render_sig_type(p))).collect();
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
    resolver: Option<IndexResolver<ClasspathIndex>>,
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
        Self { resolver: Some(resolver), ..Default::default() }
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
        self.resolver.as_ref().is_some_and(|r| r.is_project_type(binary))
    }

    /// Type-name completion candidates at `offset` in `text`: distinct simple type names from the
    /// class-name index whose name starts with the capitalised prefix under the caret. Empty unless
    /// the caret is on a bare identifier (NOT after a `.`) whose first char is uppercase.
    fn type_completions(&self, text: &str, offset: usize) -> Vec<CompletionItem> {
        const MAX: usize = 50;
        let (ident_start, prefix) = ident_prefix(text, offset);
        // A type reference starts with an uppercase letter; requiring it keeps the list focused and
        // avoids firing on a variable / method prefix (which member completion, not this, serves).
        if prefix.is_empty() || !prefix.starts_with(|c: char| c.is_ascii_uppercase()) {
            return Vec::new();
        }
        // `recv.Prefix` is a member access, not a type reference — leave it to member completion.
        if is_member_access(text, ident_start) {
            return Vec::new();
        }
        self.class_names
            .simple_names_with_prefix(&prefix, MAX)
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
                }
            })
            .collect()
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

        let jdk = match jdk_index_path {
            Some(path) => JdkMemberIndex::persistent(source, path),
            None => JdkMemberIndex::new(source),
        };
        let classpath = match deps {
            Some((dep_source, dep_memo_path)) => {
                class_names.add_binaries(dep_source.class_names());
                ClasspathIndex::with_deps(jdk, dep_source, dep_memo_path)
            }
            None => ClasspathIndex::jdk_only(jdk),
        };
        // Snapshot the prefix-search axis now that every JDK / dependency / project class is in.
        class_names.finalize();

        let mut resolver = IndexResolver::new(project, classpath);
        for (simple, binary) in project_simple_names {
            resolver.add_simple_hint(simple, binary);
        }
        Ok(Self { resolver: Some(resolver), class_names, jdk_sources })
    }

    /// Persist the classpath member index's memos now (best-effort; no-op for the empty provider or
    /// an in-memory index). Flushes BOTH tiers — the shared JDK memo and, when present, the
    /// per-project dependency memo — so a session's warmed JDK **and** library classes survive.
    pub fn flush_jdk_index(&self) {
        if let Some(resolver) = &self.resolver {
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
        let resolver = self.resolver.as_ref()?;
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
        self.jdk_sources.as_ref().and_then(|z| z.source_text(binary))
    }

    /// A signatures-only **decompiled-from-bytecode stub** for `binary` — the fallback when no real
    /// source is available (a bare JRE, or a dependency whose `-sources.jar` isn't downloaded).
    /// `None` on the pre-index provider or when the bytecode isn't decodable.
    pub fn stub_for(&self, binary: &str) -> Option<String> {
        use bennu_java::prelude::TypeResolver; // brings `members_of` into scope
        let resolver = self.resolver.as_ref()?;
        let cm = resolver.members_of(binary)?;
        Some(render_stub(binary, &cm))
    }

    /// The resolved members of `binary` — the class's own declared fields and methods, plus the
    /// links to its supertypes.
    ///
    /// The raw answer, deliberately: callers that want a *rendering* of it have one
    /// ([`stub_for`](Self::stub_for)), and callers that want to walk it — "what is inside this
    /// DTO" — need the structure rather than a page of Java to parse back apart.
    pub fn members_of(&self, binary: &str) -> Option<std::sync::Arc<bennu_java::prelude::ClassMembers>> {
        use bennu_java::prelude::TypeResolver;
        self.resolver.as_ref()?.members_of(binary)
    }

    /// The binary name of the static type of the expression spanning `[start, end)` in `source`,
    /// against this provider's full (JDK + dependency + project) resolver. For navigation/hover
    /// INSIDE a library source view — e.g. inferring `list` in `list.add(x)` to know which type
    /// declares `add`. `None` when the expression can't be typed. Works on any `.java` text.
    pub fn infer_type_binary(&self, source: &str, start: usize, end: usize) -> Option<String> {
        let resolver = self.resolver.as_ref()?;
        let tr = bennu_java::prelude::infer_expression_type(source, start, end, resolver)?;
        Some(tr.binary_name)
    }

    /// The **AST** of `source`, typed against this provider's resolver.
    ///
    /// Without a resolver — the pre-index provider — the tree is still complete, just untyped:
    /// the structure comes from the parse and only the type annotations need the classpath. That
    /// is what lets the panel draw something useful on a project that is still indexing.
    pub fn ast_of(&self, source: &str) -> bennu_java::prelude::AstNode {
        bennu_java::prelude::lower_ast(
            source,
            self.resolver.as_ref().map(|r| r as &dyn bennu_java::prelude::TypeResolver),
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
        let resolver = self.resolver.as_ref()?;
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
        let Some(resolver) = self.resolver.as_ref() else { return false };

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut queue = vec![(normalise(candidate), 0usize)];
        while let Some((binary, depth)) = queue.pop() {
            if depth > MAX_DEPTH || !seen.insert(binary.clone()) {
                continue;
            }
            if binary == wanted {
                return true;
            }
            let Some(cm) = resolver.members_of(&binary) else { continue };
            if let Some(sup) = &cm.superclass {
                queue.push((sup.clone(), depth + 1));
            }
            for iface in &cm.interfaces {
                queue.push((iface.clone(), depth + 1));
            }
        }
        false
    }

    /// Classify the caret at `offset` in a library source view `source` into a go-to [`LibraryTarget`]
    /// — the type to open + (for a member access) the member to land on. Resolves against this
    /// provider's full resolver, using the library file's OWN imports. Handles: a type reference
    /// (`Supplier` → its type), an instance member access (`recv.foo()` / `recv.bar` → the receiver's
    /// type + member), and a static member access (`Foo.bar()` / `Foo.CONST`). `None` when the caret
    /// isn't on a resolvable navigable anchor (e.g. a bare same-class call, a local — the be handles
    /// those, or they stay in-file). Works on any `.java` text (project or library).
    pub fn library_target_at(&self, source: &str, offset: usize) -> Option<LibraryTarget> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).ok()?;
        let tree = parser.parse(source, None)?;
        let bytes = source.as_bytes();
        let node = tree.root_node().named_descendant_for_byte_range(offset, offset)?;
        if !matches!(node.kind(), "identifier" | "type_identifier") {
            return None;
        }
        let text = node.utf8_text(bytes).ok()?;

        // The binary name of a member-access RECEIVER: infer its value type, else (a static access
        // like `Foo.bar()`) resolve the receiver as a type name.
        let receiver_binary = |obj: tree_sitter::Node| -> Option<String> {
            self.infer_type_binary(source, obj.start_byte(), obj.end_byte()).or_else(|| {
                obj.utf8_text(bytes).ok().and_then(|t| self.library_binary(source, t))
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
                        member: Some(LibraryMember { name: text.to_string(), is_field: false }),
                    });
                }
                // `recv.field` — caret on the field name.
                "field_access" if p.child_by_field_name("field") == Some(node) => {
                    let obj = p.child_by_field_name("object")?;
                    let binary = receiver_binary(obj)?;
                    return Some(LibraryTarget {
                        binary,
                        member: Some(LibraryMember { name: text.to_string(), is_field: true }),
                    });
                }
                _ => {}
            }
        }

        // Otherwise a TYPE reference (a `type_identifier`, or a bare name used as a type / scope).
        let binary = self.library_binary(source, text)?;
        Some(LibraryTarget { binary, member: None })
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
        match &self.resolver {
            Some(resolver) => bennu_check::prelude::check_file_resolved(source, ctx, resolver, jdk_available),
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
        match &self.resolver {
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
        let resolver = self.resolver.as_ref()?;

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).ok()?;
        let tree = parser.parse(source, None)?;
        let bytes = source.as_bytes();

        // The identifier leaf under the caret.
        let node = tree.root_node().named_descendant_for_byte_range(offset, offset)?;
        if node.kind() != "identifier" {
            return None;
        }
        let name = node.utf8_text(bytes).ok()?.to_string();

        // The declaration forms whose binding the ordinary inference can't see from the name
        // itself, then the ordinary path — which covers locals (`var` included), parameters,
        // `catch` and try-with-resources, at their declaration AND at every use.
        let ty = self
            .declared_binding_type(source, bytes, node)
            .or_else(|| infer_expression_type(source, node.start_byte(), node.end_byte(), resolver));

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
        let resolver = self.resolver.as_ref()?;
        let parent = node.parent()?;
        if parent.child_by_field_name("name").map(|n| n.id()) != Some(node.id()) {
            return None;
        }
        match parent.kind() {
            "enhanced_for_statement" => {
                let written = parent.child_by_field_name("type")?.utf8_text(bytes).ok()?;
                if written == "var" || written == "val" {
                    let value = parent.child_by_field_name("value")?;
                    let it =
                        infer_expression_type(source, value.start_byte(), value.end_byte(), resolver)?;
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
        let resolver = self.resolver.as_ref()?;
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
        Some(TypeRef { binary_name: binary, type_args: Vec::new() })
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
        (Some(ty.binary_name.replace('/', ".").replace('$', ".")), kind.to_string())
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
        match &self.resolver {
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
            .map(|r| r as &dyn bennu_query::prelude::ProjectView)
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
        if let Some(resolver) = &self.resolver {
            resolver.apply_file_patch(file, records);
        }
    }

    /// Enumerate the project's members (methods + fields) from the built index, for the
    /// index inspector's members list. A read-only view of the persisted index (the
    /// analyzer owns how a member symbol maps to a [`ProjectMember`]). An empty vec on the
    /// pre-index (empty) provider — the FE shows the "building" state.
    pub fn project_members(&self) -> Vec<ProjectMember> {
        let Some(resolver) = &self.resolver else {
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
        let Some(resolver) = &self.resolver else {
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
                    "UTF-8",
                ) else {
                    return Ok(Vec::new());
                };
                disk = decoded.text;
                &disk
            }
        };
        let member = bennu_query::prelude::completion(text, at.offset, resolver);
        if !member.is_empty() {
            return Ok(member);
        }
        // No member candidates — offer TYPE-NAME completion when the caret sits on a bare, capitalised
        // identifier prefix (not a member access after a `.`). Selecting a name inserts it; the "Import
        // class" intention (Alt+Enter) then adds the import.
        Ok(self.type_completions(text, at.offset))
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
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        (tree, src.find(needle).unwrap())
    }

    fn declaration(src: &str, needle: &str, name: &str) -> Option<(String, Option<String>)> {
        let (tree, at) = ident_at(src, needle);
        let node = tree.root_node().named_descendant_for_byte_range(at, at).unwrap();
        local_declaration_of(node, src.as_bytes(), name)
    }

    #[test]
    fn a_lombok_val_is_found_from_its_own_name() {
        let src = "class C { void m() { val properties = Retriever.properties(svc); } }";
        let (written, init) = declaration(src, "properties =", "properties").expect("declared here");
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
        assert!(declaration(src, "field);", "field").is_none(), "a field is not this function's business");
    }

    #[test]
    fn the_card_says_what_is_certain_and_admits_the_rest() {
        let src = "class C { void m() { val properties = Retriever.properties(svc); } }";
        let (tree, at) = ident_at(src, "properties =");
        let node = tree.root_node().named_descendant_for_byte_range(at, at).unwrap();
        let info = unresolved_local_hover(src.as_bytes(), node, "properties").expect("a card");
        assert_eq!(info.signature, "val properties");
        assert_eq!(info.kind, "variable");
        assert!(info.doc.unwrap().contains("Retriever.properties(svc)"));
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

#[cfg(test)]
mod tests {
    use super::{reads_as_type_name, render_stub};
    use bennu_java::prelude::{ClassFlags, ClassMembers, Member, TypeRef, Visibility};

    /// The guard exists so an arbitrary matched fragment never reaches the resolver as a
    /// question. Its job is to say no to everything that is not a name.
    #[test]
    fn only_a_bare_or_dotted_identifier_reads_as_a_type_name() {
        for yes in ["Files", "java.nio.file.Files", "_x", "Outer.Inner", "Map$Entry"] {
            assert!(reads_as_type_name(yes), "{yes}");
        }
        for no in ["", "\"hello\"", "a.b()", "1", "a + b", "a..b", "a.", "new Foo()", "x[0]"] {
            assert!(!reads_as_type_name(no), "{no}");
        }
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
                Member::method("<init>", TypeRef::simple("void"), Vec::new()).vis(Visibility::Public),
            ],
            fields: vec![Member::field("MAX", TypeRef::simple("int"))
                .vis(Visibility::Public)
                .stat()],
            flags: ClassFlags::default(),
        };
        let s = render_stub("com/acme/Registry", &cm);
        assert!(s.contains("Decompiled from bytecode"), "header warning: {s}");
        assert!(s.contains("package com.acme;"), "package: {s}");
        assert!(s.contains("public class Registry"), "class decl: {s}");
        assert!(s.contains("implements Iterable"), "interfaces: {s}");
        assert!(!s.contains("extends Object"), "Object super is elided: {s}");
        assert!(s.contains("public static int MAX;"), "field: {s}");
        assert!(s.contains("public Item get(int arg0)"), "method w/ synthesized arg name: {s}");
        assert!(s.contains("public Registry("), "constructor rendered by simple name: {s}");
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
        assert!(s.contains("int read() throws IOException"), "throws clause rendered: {s}");
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
        assert!(!s.contains("throws Throwable"), "erased Throwable must not appear: {s}");
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
            flags: ClassFlags { is_interface: true, ..Default::default() },
        };
        let s = render_stub("com/acme/Task", &cm);
        assert!(s.contains("public interface Task"), "{s}");
        assert!(s.contains("void run();"), "interface method has no body: {s}");
        assert!(!s.contains("throw new RuntimeException"), "no placeholder body in an interface: {s}");
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
    fn type_detail_prefers_java_and_counts_extras() {
        assert_eq!(super::type_detail(&["java.util.List".to_string()]), Some("java.util.List".to_string()));
        // Multiple packages: prefer java.*, note the rest.
        let d = super::type_detail(&["com.acme.List".to_string(), "java.util.List".to_string()]);
        assert_eq!(d, Some("java.util.List (+1 more)".to_string()));
        assert_eq!(super::type_detail(&[]), None);
    }
}
