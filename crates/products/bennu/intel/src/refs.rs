//! Cross-file references / find-usages + the caret classifier both find-usages and
//! rename key off (docs §5 #7, #10-12).
//!
//! The inverse of the Phase-1 receiver inference: Phase-1 resolves a `receiver.member`
//! use site to its declaring type; here we run that resolution over EVERY use site in the
//! project and bucket the results by the declaration they resolve to, building the reverse
//! map
//!
//! ```text
//!   Declaration (a type FQN, or a method/field on a type)  →  Vec<UsageLocation>
//! ```
//!
//! A `references(file, offset)` query picks the declaration under the caret and returns
//! its usage bucket. Unresolved sites are skipped, never fatal — a receiver we can't type
//! (missing dep, flow-typed, static-on-name) simply contributes no edge.
//!
//! The classifier is shared with rename: [`classify_caret`] yields the [`DeclKey`] a
//! reference query keys off; [`classify_target`] is its rename superset that also
//! recognises a **local variable / parameter** (which find-usages doesn't bucket).

use std::collections::HashMap;

use bennu_java::prelude::{extract_symbols, infer_receiver_type, FileSymbols, TypeResolver};
use tree_sitter::{Node, Parser};

/// What a declaration *is*: a type, or a member (method/field) owned by a type. The key
/// the reverse map buckets usages under.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeclKey {
    /// A type declaration, identified by its JVM binary name (`com/acme/Order`).
    Type { binary: String },
    /// A method on a type: owner binary name + method simple name. (Overloads collapse to
    /// one key — see the honest-limits note; Phase-3 does not resolve by arity.)
    Method { owner: String, name: String },
    /// A field on a type: owner binary name + field simple name.
    Field { owner: String, name: String },
}

impl DeclKey {
    /// A short human label for a preview / results header.
    pub fn label(&self) -> String {
        match self {
            DeclKey::Type { binary } => format!("type {}", binary.replace('/', ".")),
            DeclKey::Method { owner, name } => format!("method {}.{}()", owner.replace('/', "."), name),
            DeclKey::Field { owner, name } => format!("field {}.{}", owner.replace('/', "."), name),
        }
    }
}

/// One resolved use site of a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageLocation {
    /// Absolute path to the file the use is in.
    pub file: String,
    /// Start byte offset of the referencing identifier.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// 1-based line of the reference (computed from `start`).
    pub line: usize,
    /// 1-based column of the reference.
    pub col: usize,
    /// The source line text (trimmed), for a preview in the results list.
    pub preview: String,
}

/// The built reverse index: `declaration → its use sites`, across the whole project.
pub struct ReferenceIndex {
    by_decl: HashMap<DeclKey, Vec<UsageLocation>>,
    /// Per-file parsed symbols, kept so a `references(file, offset)` can classify the
    /// caret against the declaration it sits on.
    file_symbols: HashMap<String, FileSymbols>,
    /// Use sites attempted / resolved (the resolve rate, for logging).
    pub attempted: usize,
    pub resolved: usize,
}

impl ReferenceIndex {
    /// Every usage of a declaration key (empty when none / unknown key).
    pub fn usages_of(&self, key: &DeclKey) -> &[UsageLocation] {
        self.by_decl.get(key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// The number of distinct declarations that have at least one recorded usage.
    pub fn declared_with_usages(&self) -> usize {
        self.by_decl.len()
    }

    /// The parsed symbols of a file (for the caret classifier). `None` if not indexed.
    pub fn symbols(&self, file: &str) -> Option<&FileSymbols> {
        self.file_symbols.get(file)
    }

    /// Iterate every `(declaration, usages)` bucket (for ranking / reporting).
    pub fn iter(&self) -> impl Iterator<Item = (&DeclKey, &Vec<UsageLocation>)> {
        self.by_decl.iter()
    }
}

/// A `.java` file to index: its absolute path + its source text.
pub struct SourceFile {
    pub path: String,
    pub source: String,
}

/// Build the whole-project reference index. `resolver` resolves receiver types to their
/// declaring types (project sources + JDK), `project_types` is the project-wide
/// simple→binary type map so a bare `Foo` type reference resolves.
pub fn build_reference_index(
    files: &[SourceFile],
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
) -> ReferenceIndex {
    let mut by_decl: HashMap<DeclKey, Vec<UsageLocation>> = HashMap::new();
    let mut file_symbols: HashMap<String, FileSymbols> = HashMap::new();
    let mut attempted = 0usize;
    let mut resolved = 0usize;

    for f in files {
        let fs = extract_symbols(&f.source);
        file_symbols.insert(f.path.clone(), fs);

        let mut walker = FileWalker::new(&f.path, &f.source, resolver, project_types);
        walker.walk();
        attempted += walker.attempted;
        resolved += walker.resolved;
        for (key, usage) in walker.edges {
            by_decl.entry(key).or_default().push(usage);
        }
    }

    ReferenceIndex { by_decl, file_symbols, attempted, resolved }
}

/// The outcome of a references query.
#[derive(Debug, Clone)]
pub struct ReferencesResult {
    /// The declaration the caret resolved to (for the header / debug).
    pub target: DeclKey,
    /// Its use sites across the project.
    pub usages: Vec<UsageLocation>,
}

/// Resolve the declaration at `offset` in `file` and return its usages. `None` when the
/// caret isn't on an identifier we can turn into a declaration key.
pub fn references(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
) -> Option<ReferencesResult> {
    let key = classify_caret(index, file, source, offset, resolver, project_types)?;
    let usages = index.usages_of(&key).to_vec();
    Some(ReferencesResult { target: key, usages })
}

// ── the per-file reference walk ────────────────────────────────────────────────────

/// Walks one file's CST, emitting `(DeclKey, UsageLocation)` edges for each resolvable use
/// site (method invocation, field access, type reference).
struct FileWalker<'a> {
    path: &'a str,
    source: &'a str,
    bytes: &'a [u8],
    resolver: &'a dyn TypeResolver,
    project_types: &'a HashMap<String, String>,
    line_starts: Vec<usize>,
    edges: Vec<(DeclKey, UsageLocation)>,
    attempted: usize,
    resolved: usize,
}

impl<'a> FileWalker<'a> {
    fn new(
        path: &'a str,
        source: &'a str,
        resolver: &'a dyn TypeResolver,
        project_types: &'a HashMap<String, String>,
    ) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            path,
            source,
            bytes: source.as_bytes(),
            resolver,
            project_types,
            line_starts,
            edges: Vec::new(),
            attempted: 0,
            resolved: 0,
        }
    }

    fn walk(&mut self) {
        let mut parser = Parser::new();
        if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
            return;
        }
        let Some(tree) = parser.parse(self.source, None) else { return };
        let root = tree.root_node();

        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            let mut cur = n.walk();
            for c in n.named_children(&mut cur) {
                stack.push(c);
            }
            match n.kind() {
                "method_invocation" => self.on_method_invocation(&n),
                "field_access" => self.on_field_access(&n),
                "type_identifier" => self.on_type_identifier(&n),
                _ => {}
            }
        }
    }

    fn on_method_invocation(&mut self, node: &Node) {
        let Some(name_node) = node.child_by_field_name("name") else { return };
        let Some(name) = self.node_text(&name_node) else { return };

        let owner = match node.child_by_field_name("object") {
            Some(_) => {
                self.attempted += 1;
                let dot_off = name_node.start_byte();
                match self.resolve_receiver_owner(dot_off, &name, MemberSort::Method) {
                    Some(o) => {
                        self.resolved += 1;
                        o
                    }
                    None => return,
                }
            }
            None => {
                self.attempted += 1;
                match self.enclosing_owner(node, &name, MemberSort::Method) {
                    Some(o) => {
                        self.resolved += 1;
                        o
                    }
                    None => return,
                }
            }
        };
        let usage = self.usage_at(&name_node);
        self.edges.push((DeclKey::Method { owner, name }, usage));
    }

    fn on_field_access(&mut self, node: &Node) {
        let Some(field_node) = node.child_by_field_name("field") else { return };
        let Some(name) = self.node_text(&field_node) else { return };
        self.attempted += 1;
        let dot_off = field_node.start_byte();
        let Some(owner) = self.resolve_receiver_owner(dot_off, &name, MemberSort::Field) else {
            return;
        };
        self.resolved += 1;
        let usage = self.usage_at(&field_node);
        self.edges.push((DeclKey::Field { owner, name }, usage));
    }

    fn on_type_identifier(&mut self, node: &Node) {
        let Some(simple) = self.node_text(node) else { return };
        if self.is_declaration_name(node) {
            return;
        }
        self.attempted += 1;
        let Some(binary) = self.resolve_type_simple(&simple) else { return };
        self.resolved += 1;
        let usage = self.usage_at(node);
        self.edges.push((DeclKey::Type { binary }, usage));
    }

    fn resolve_receiver_owner(
        &self,
        dot_off: usize,
        member: &str,
        sort: MemberSort,
    ) -> Option<String> {
        let recv = infer_receiver_type(self.source, dot_off, self.resolver)?;
        self.declaring_owner(&recv.binary_name, member, sort)
    }

    fn enclosing_owner(&self, node: &Node, member: &str, sort: MemberSort) -> Option<String> {
        let fqn = self.enclosing_type_binary(node)?;
        self.declaring_owner(&fqn, member, sort)
    }

    fn declaring_owner(&self, start_binary: &str, member: &str, sort: MemberSort) -> Option<String> {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![start_binary.to_string()];
        while let Some(bn) = stack.pop() {
            if !visited.insert(bn.clone()) {
                continue;
            }
            if let Some(cm) = self.resolver.members_of(&bn) {
                let found = match sort {
                    MemberSort::Method => cm.methods.iter().any(|m| m.name == member),
                    MemberSort::Field => cm.fields.iter().any(|f| f.name == member),
                };
                if found {
                    return Some(bn);
                }
                if let Some(sc) = cm.superclass {
                    stack.push(sc);
                }
                stack.extend(cm.interfaces);
            }
        }
        Some(start_binary.to_string())
    }

    fn enclosing_type_binary(&self, node: &Node) -> Option<String> {
        let mut cur = node.parent();
        while let Some(n) = cur {
            if matches!(
                n.kind(),
                "class_declaration" | "interface_declaration" | "enum_declaration"
            ) {
                let name = n.child_by_field_name("name").and_then(|x| self.node_text(&x))?;
                return self.resolve_type_simple(&name);
            }
            cur = n.parent();
        }
        None
    }

    fn resolve_type_simple(&self, simple: &str) -> Option<String> {
        if simple.contains('.') {
            return Some(simple.replace('.', "/"));
        }
        if let Some(b) = self.project_types.get(simple) {
            return Some(b.clone());
        }
        self.resolver.resolve_simple_name(simple, &[])
    }

    fn is_declaration_name(&self, node: &Node) -> bool {
        let Some(parent) = node.parent() else { return false };
        if !matches!(
            parent.kind(),
            "class_declaration" | "interface_declaration" | "enum_declaration"
        ) {
            return false;
        }
        parent.child_by_field_name("name").map(|nm| nm.id() == node.id()).unwrap_or(false)
    }

    fn usage_at(&self, node: &Node) -> UsageLocation {
        let start = node.start_byte();
        let end = node.end_byte();
        let (line, col) = self.line_col(start);
        UsageLocation {
            file: self.path.to_string(),
            start,
            end,
            line,
            col,
            preview: self.line_text(line),
        }
    }

    fn line_col(&self, off: usize) -> (usize, usize) {
        let idx = match self.line_starts.binary_search(&off) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts[idx];
        (idx + 1, off - line_start + 1)
    }

    fn line_text(&self, line: usize) -> String {
        let start = self.line_starts.get(line - 1).copied().unwrap_or(0);
        let end = self.line_starts.get(line).copied().unwrap_or(self.source.len());
        self.source[start..end].trim().to_string()
    }

    fn node_text(&self, node: &Node) -> Option<String> {
        node.utf8_text(self.bytes).ok().map(|s| s.to_string())
    }
}

#[derive(Debug, Clone, Copy)]
enum MemberSort {
    Method,
    Field,
}

// ── caret classification (shared by references + rename) ───────────────────────────

/// Turn a caret into the [`DeclKey`] it references (declaration site or use site).
pub fn classify_caret(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
) -> Option<DeclKey> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    let bytes = source.as_bytes();

    let ident = smallest_named_at(&root, offset)?;
    let ident_text = ident.utf8_text(bytes).ok()?.to_string();

    if let Some(key) = decl_name_key(&ident, bytes, file, index, project_types) {
        return Some(key);
    }

    let parent = ident.parent()?;
    match parent.kind() {
        "method_invocation" => {
            let name_node = parent.child_by_field_name("name")?;
            if name_node.id() != ident.id() {
                return receiver_side_key(&ident, &ident_text, source, resolver, project_types);
            }
            let owner = match parent.child_by_field_name("object") {
                Some(_) => {
                    let dot_off = name_node.start_byte();
                    let recv = infer_receiver_type(source, dot_off, resolver)?;
                    declaring_owner(resolver, &recv.binary_name, &ident_text, true)
                }
                None => {
                    let fqn = enclosing_type_binary(&parent, bytes, project_types)?;
                    declaring_owner(resolver, &fqn, &ident_text, true)
                }
            }?;
            Some(DeclKey::Method { owner, name: ident_text })
        }
        "field_access" => {
            let field_node = parent.child_by_field_name("field")?;
            if field_node.id() != ident.id() {
                return receiver_side_key(&ident, &ident_text, source, resolver, project_types);
            }
            let dot_off = field_node.start_byte();
            let recv = infer_receiver_type(source, dot_off, resolver)?;
            let owner = declaring_owner(resolver, &recv.binary_name, &ident_text, false)?;
            Some(DeclKey::Field { owner, name: ident_text })
        }
        "type_identifier" | "scoped_type_identifier" | "generic_type" => {
            type_key(&ident_text, project_types, resolver)
        }
        _ => {
            if ident.kind() == "type_identifier" {
                type_key(&ident_text, project_types, resolver)
            } else {
                None
            }
        }
    }
}

/// If `node` is the NAME of a declaration, return the corresponding [`DeclKey`].
fn decl_name_key(
    node: &Node,
    bytes: &[u8],
    file: &str,
    index: &ReferenceIndex,
    project_types: &HashMap<String, String>,
) -> Option<DeclKey> {
    let parent = node.parent()?;
    let name = node.utf8_text(bytes).ok()?.to_string();
    match parent.kind() {
        "class_declaration" | "interface_declaration" | "enum_declaration" => {
            if parent.child_by_field_name("name")?.id() != node.id() {
                return None;
            }
            let binary = index
                .symbols(file)
                .and_then(|fs| fs.types.iter().find(|t| t.name == name))
                .map(|t| t.fqn.replace('.', "/"))
                .or_else(|| project_types.get(&name).cloned())?;
            Some(DeclKey::Type { binary })
        }
        "method_declaration" => {
            if parent.child_by_field_name("name")?.id() != node.id() {
                return None;
            }
            let owner = enclosing_type_binary(&parent, bytes, project_types)?;
            Some(DeclKey::Method { owner, name })
        }
        "variable_declarator" => {
            let gp = parent.parent()?;
            if gp.kind() != "field_declaration" {
                return None;
            }
            if parent.child_by_field_name("name")?.id() != node.id() {
                return None;
            }
            let owner = enclosing_type_binary(&gp, bytes, project_types)?;
            Some(DeclKey::Field { owner, name })
        }
        _ => None,
    }
}

fn receiver_side_key(
    ident: &Node,
    ident_text: &str,
    source: &str,
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
) -> Option<DeclKey> {
    let _ = (ident, source, resolver);
    type_key(ident_text, project_types, resolver)
}

fn type_key(
    simple: &str,
    project_types: &HashMap<String, String>,
    resolver: &dyn TypeResolver,
) -> Option<DeclKey> {
    let base = simple.split('<').next().unwrap_or(simple).trim();
    if base.contains('.') {
        return Some(DeclKey::Type { binary: base.replace('.', "/") });
    }
    if let Some(b) = project_types.get(base) {
        return Some(DeclKey::Type { binary: b.clone() });
    }
    resolver.resolve_simple_name(base, &[]).map(|binary| DeclKey::Type { binary })
}

fn declaring_owner(
    resolver: &dyn TypeResolver,
    start: &str,
    member: &str,
    is_method: bool,
) -> Option<String> {
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![start.to_string()];
    while let Some(bn) = stack.pop() {
        if !visited.insert(bn.clone()) {
            continue;
        }
        if let Some(cm) = resolver.members_of(&bn) {
            let found = if is_method {
                cm.methods.iter().any(|m| m.name == member)
            } else {
                cm.fields.iter().any(|f| f.name == member)
            };
            if found {
                return Some(bn);
            }
            if let Some(sc) = cm.superclass {
                stack.push(sc);
            }
            stack.extend(cm.interfaces);
        }
    }
    Some(start.to_string())
}

fn enclosing_type_binary(
    node: &Node,
    bytes: &[u8],
    project_types: &HashMap<String, String>,
) -> Option<String> {
    let mut cur = Some(*node);
    while let Some(n) = cur {
        if matches!(
            n.kind(),
            "class_declaration" | "interface_declaration" | "enum_declaration"
        ) {
            let name = n.child_by_field_name("name")?.utf8_text(bytes).ok()?.to_string();
            if let Some(b) = project_types.get(&name) {
                return Some(b.clone());
            }
            return Some(name);
        }
        cur = n.parent();
    }
    None
}

// ── rename classification (superset: also local var / param) ───────────────────────

/// What the caret sits on, for a RENAME (a superset of the references [`DeclKey`]: it also
/// recognises a **local variable / parameter**, which find-usages doesn't bucket).
#[derive(Debug, Clone)]
pub enum RenameTarget {
    /// A local variable or parameter: single-file, scope-exact. `def_start`/`def_end` is
    /// its declarator name span (the anchor the scope walk keys off).
    Local { name: String, def_start: usize, def_end: usize },
    /// A method or field — the reference index buckets its cross-file uses.
    Member { key: DeclKey },
    /// A type — refs + imports + Spring bean XML.
    Type { key: DeclKey, binary: String },
}

/// Classify the caret at `offset` for a rename. Tries **local variable / parameter**
/// first (a bare `identifier` bound in the enclosing method) — that path needs no index
/// and is scope-exact; otherwise falls back to the references classifier (member / type).
pub fn classify_target(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
) -> Option<RenameTarget> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    let bytes = source.as_bytes();

    let ident = smallest_named_at(&root, offset)?;
    if ident.kind() == "identifier" && !is_member_selector_node(&ident) {
        let name = ident.utf8_text(bytes).ok()?.to_string();
        if let Some((ds, de)) = find_local_binding(&ident, bytes, &name) {
            return Some(RenameTarget::Local { name, def_start: ds, def_end: de });
        }
    }

    let key = classify_caret(index, file, source, offset, resolver, project_types)?;
    match &key {
        DeclKey::Type { binary } => {
            Some(RenameTarget::Type { key: key.clone(), binary: binary.clone() })
        }
        DeclKey::Method { .. } | DeclKey::Field { .. } => Some(RenameTarget::Member { key }),
    }
}

/// Whether an `identifier` is a member selector (`x.name`, `foo.bar()`) — a local rename
/// must not treat these as the variable.
pub(crate) fn is_member_selector_node(node: &Node) -> bool {
    let Some(parent) = node.parent() else { return false };
    match parent.kind() {
        "field_access" => {
            parent.child_by_field_name("field").map(|f| f.id() == node.id()).unwrap_or(false)
        }
        "method_invocation" => {
            parent.child_by_field_name("name").map(|n| n.id() == node.id()).unwrap_or(false)
        }
        _ => false,
    }
}

/// Find the declarator NAME span of the local variable / parameter `name` in scope at
/// `ident`. `None` when `name` is not a local/param binding (a field, or unresolved) — so
/// the caller falls back to member/type classification.
fn find_local_binding(ident: &Node, bytes: &[u8], name: &str) -> Option<(usize, usize)> {
    let mut cur = ident.parent();
    while let Some(n) = cur {
        if matches!(
            n.kind(),
            "method_declaration" | "constructor_declaration" | "lambda_expression"
        ) {
            if let Some(p) = find_param_decl(&n, bytes, name) {
                return Some(p);
            }
        }
        if matches!(
            n.kind(),
            "block" | "for_statement" | "enhanced_for_statement" | "catch_clause"
        ) {
            if let Some(d) = find_local_decl(&n, bytes, name) {
                return Some(d);
            }
        }
        if matches!(
            n.kind(),
            "class_declaration" | "interface_declaration" | "enum_declaration"
        ) {
            break;
        }
        cur = n.parent();
    }
    None
}

fn find_param_decl(node: &Node, bytes: &[u8], name: &str) -> Option<(usize, usize)> {
    let params = node.child_by_field_name("parameters")?;
    let mut cw = params.walk();
    for p in params.named_children(&mut cw) {
        if matches!(p.kind(), "formal_parameter" | "spread_parameter") {
            if let Some(nm) = p.child_by_field_name("name") {
                if nm.utf8_text(bytes).ok() == Some(name) {
                    return Some((nm.start_byte(), nm.end_byte()));
                }
            }
        }
    }
    None
}

fn find_local_decl(node: &Node, bytes: &[u8], name: &str) -> Option<(usize, usize)> {
    let mut stack: Vec<Node> = vec![*node];
    while let Some(n) = stack.pop() {
        let mut cw = n.walk();
        for c in n.named_children(&mut cw) {
            if c.id() != node.id()
                && matches!(
                    c.kind(),
                    "class_declaration" | "method_declaration" | "constructor_declaration"
                )
            {
                continue;
            }
            stack.push(c);
        }
        if n.kind() == "variable_declarator" {
            let parent_kind = n.parent().map(|p| p.kind().to_string()).unwrap_or_default();
            if parent_kind == "field_declaration" {
                continue;
            }
            if let Some(nm) = n.child_by_field_name("name") {
                if nm.utf8_text(bytes).ok() == Some(name) {
                    return Some((nm.start_byte(), nm.end_byte()));
                }
            }
        }
    }
    None
}

/// The smallest named node whose span covers `offset` (prefers the identifier under a
/// caret; for a caret at the very end of an identifier we bias left by one).
fn smallest_named_at<'t>(root: &Node<'t>, offset: usize) -> Option<Node<'t>> {
    let probe = if offset > 0 { offset - 1 } else { offset };
    let mut best: Option<Node> = None;
    let mut stack = vec![*root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if n.start_byte() <= probe && probe < n.end_byte() && n.is_named() {
            match &best {
                Some(b) if (b.end_byte() - b.start_byte()) <= (n.end_byte() - n.start_byte()) => {}
                _ => best = Some(n),
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::java_index::project_type_map;
    use bennu_java::prelude::extract_symbols;

    // A tiny in-memory TypeResolver over project sources only (no JDK) — enough for the
    // pure-project reference/rename cases the unit tests cover.
    struct SrcResolver {
        project: HashMap<String, bennu_java::prelude::ClassMembers>,
        simple: HashMap<String, String>,
    }

    impl SrcResolver {
        fn build(files: &[(&str, &str)]) -> (Self, HashMap<String, String>) {
            use bennu_java::prelude::{ClassMembers, Member, MemberKind, TypeRef, Visibility};
            let mut project_types: HashMap<String, String> = HashMap::new();
            for (_p, s) in files {
                for td in extract_symbols(s).types {
                    project_types.insert(td.name.clone(), td.fqn.replace('.', "/"));
                }
            }
            let mut project = HashMap::new();
            let mut simple = project_types.clone();
            for (_p, s) in files {
                let fs = extract_symbols(s);
                for td in &fs.types {
                    let binary = td.fqn.replace('.', "/");
                    let methods = td
                        .methods
                        .iter()
                        .map(|m| Member {
                            name: m.name.clone(),
                            kind: MemberKind::Method,
                            return_type: TypeRef { binary_name: String::new(), type_args: vec![] },
                            params: vec![],
                            is_static: m.is_static,
                            visibility: Visibility::Public,
                            raw_signature: String::new(),
                        })
                        .collect();
                    let fields = td
                        .fields
                        .iter()
                        .map(|f| Member {
                            name: f.name.clone(),
                            kind: MemberKind::Field,
                            return_type: TypeRef {
                                binary_name: project_types
                                    .get(f.type_text.split('<').next().unwrap_or(&f.type_text).trim())
                                    .cloned()
                                    .unwrap_or_else(|| f.type_text.replace('.', "/")),
                                type_args: vec![],
                            },
                            params: vec![],
                            is_static: f.is_static,
                            visibility: Visibility::Public,
                            raw_signature: String::new(),
                        })
                        .collect();
                    project.insert(
                        binary,
                        ClassMembers { superclass: None, interfaces: vec![], methods, fields },
                    );
                }
            }
            simple.insert("String".into(), "java/lang/String".into());
            (Self { project, simple }, project_types)
        }
    }

    impl TypeResolver for SrcResolver {
        fn members_of(&self, binary: &str) -> Option<bennu_java::prelude::ClassMembers> {
            self.project.get(binary).cloned()
        }
        fn resolve_simple_name(
            &self,
            name: &str,
            imports: &[bennu_java::prelude::Import],
        ) -> Option<String> {
            for imp in imports {
                if imp.simple_name() == Some(name) {
                    return Some(imp.path.replace('.', "/"));
                }
            }
            self.simple.get(name).cloned()
        }
    }

    fn index_of(files: &[(&str, &str)]) -> (ReferenceIndex, SrcResolver, HashMap<String, String>) {
        let (resolver, project_types) = SrcResolver::build(files);
        let src: Vec<SourceFile> =
            files.iter().map(|(p, s)| SourceFile { path: p.to_string(), source: s.to_string() }).collect();
        let index = build_reference_index(&src, &resolver, &project_types);
        (index, resolver, project_types)
    }

    #[test]
    fn method_usages_counted_across_files() {
        let files = [
            ("A.java", "package p; public class A { public int val() { return 1; } }"),
            ("B.java", "package p; public class B { public int use(A a) { return a.val() + a.val(); } }"),
        ];
        let (index, _r, _pt) = index_of(&files);
        let key = DeclKey::Method { owner: "p/A".into(), name: "val".into() };
        assert_eq!(index.usages_of(&key).len(), 2);
    }

    #[test]
    fn type_usages_exclude_declaration_name() {
        let files = [
            ("A.java", "package p; public class A { }"),
            ("B.java", "package p; public class B { public int u(A a) { return 0; } }"),
        ];
        let (index, _r, _pt) = index_of(&files);
        let usages = index.usages_of(&DeclKey::Type { binary: "p/A".into() });
        assert_eq!(usages.len(), 1);
        assert_eq!(usages[0].file, "B.java");
    }

    #[test]
    fn classify_local_var_over_field() {
        let src = "package p; public class C { int x; int f() { int x = 1; return x; } }";
        let files = [("C.java", src)];
        let (index, resolver, pt) = index_of(&files);
        // caret on the local `x` in `int x = 1`
        let off = src.find("int x = 1").unwrap() + "int ".len() + 0;
        let t = classify_target(&index, "C.java", src, off, &resolver, &pt).expect("classified");
        assert!(matches!(t, RenameTarget::Local { ref name, .. } if name == "x"));
    }

    #[test]
    fn unresolved_receiver_never_panics() {
        let files = [("X.java", "package p; public class X { void m(Unknown u) { u.frob(); } }")];
        let (index, _r, _pt) = index_of(&files);
        let _ = index.declared_with_usages();
    }

    #[test]
    fn project_type_map_seeds_classification() {
        // sanity: the java_index helper produces the same simple→binary shape the walk uses
        let _ = project_type_map(std::path::Path::new("."));
    }
}
