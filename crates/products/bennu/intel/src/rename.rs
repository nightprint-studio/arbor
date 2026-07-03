//! RENAME refactoring — best-effort, PREVIEW-first (docs §5 #10-12).
//!
//! [`rename_plan`] classifies the symbol under the caret ([`crate::refs::classify_target`])
//! and computes **all** edit sites, returned as a per-file preview. [`rename_apply`]
//! flattens that plan into the concrete edits the FE applies (so CodeMirror's undo works
//! and the FE — not the backend — writes the buffers).
//!
//! The three caret kinds (docs §5 #10-12):
//!   * **Local variable / parameter** → scope-exact, single file. Pure tree-sitter scope
//!     walk; NO index needed, and NO unrelated same-named symbol is touched.
//!   * **Field / method** → the declaration + every cross-file reference from the
//!     [`crate::refs::ReferenceIndex`].
//!   * **Class / interface** → the declaration + references (simple-name use sites) +
//!     `import` statements + Spring bean XML `<bean class="oldFQCN">` occurrences
//!     ([`bennu_web::prelude::bean_class_value_spans`]). A Struts `<action class="beanId">`
//!     uses a bean-**id**, not the FQCN, so it is correctly NOT edited (honest limit).
//!
//! Conservative: an edit is emitted only where we can justify it. Method use-sites are
//! flagged `inferred` (overloads collapse to one key) so the FE surfaces them for review,
//! never silently applies them as if exact.

use std::collections::HashMap;
use std::path::Path;

use bennu_java::prelude::TypeResolver;
use bennu_web::prelude::bean_class_value_spans;
use tree_sitter::{Node, Parser};

use crate::jdk::JdkMemberIndex;
use crate::refs::{
    build_reference_index, classify_caret, classify_target, references, DeclKey, ReferenceIndex,
    ReferencesResult, RenameTarget, SourceFile,
};
use crate::resolver::IndexResolver;

/// Why an edit was planned (drives the preview grouping + the honest-limits surfacing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditReason {
    Declaration,
    Reference,
    Import,
    SpringBean,
    Local,
}

impl EditReason {
    pub fn label(&self) -> &'static str {
        match self {
            EditReason::Declaration => "declaration",
            EditReason::Reference => "reference",
            EditReason::Import => "import",
            EditReason::SpringBean => "spring-bean",
            EditReason::Local => "local",
        }
    }
}

/// One concrete edit: replace `[start, end)` in `file` with `new_text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub new_text: String,
    /// The exact text currently at `[start, end)` — the FE shows old→new and can assert
    /// the buffer still matches before applying (a stale-buffer guard).
    pub old: String,
    /// Why this edit exists.
    pub reason: EditReason,
    /// True when the edit is inferred/heuristic (a method use-site where an overload could
    /// collapse). The FE surfaces these for review.
    pub inferred: bool,
}

/// The edits for one file (the preview list the FE renders per file).
#[derive(Debug, Clone)]
pub struct FileEdits {
    pub file: String,
    pub edits: Vec<Edit>,
}

/// The rename PREVIEW: what the caret resolved to + the per-file edit sites.
#[derive(Debug, Clone)]
pub struct RenamePlan {
    pub old_name: String,
    pub new_name: String,
    /// A short human label of the target (`"method com.x.Foo.bar()"`, `"local `x`"`, …).
    pub target_label: String,
    /// The edits, grouped by file (preserves per-file preview rendering).
    pub files: Vec<FileEdits>,
    /// Whether the plan carries any `inferred` edit (the FE nudges review).
    pub has_inferred: bool,
}

impl RenamePlan {
    pub fn total_edits(&self) -> usize {
        self.files.iter().map(|f| f.edits.len()).sum()
    }
}

/// A source file available to the planner (path + text). The planner needs every project
/// `.java` file (for `import` rewrites + local-scope walks) and every config `.xml` file
/// (for Spring bean edits).
pub struct PlanFile {
    pub path: String,
    pub source: String,
}

/// Compute the rename plan for the symbol at `file`:`offset`. `java_files` are all the
/// project's `.java` sources; `xml_files` are the config fragments. Returns `None` when
/// the caret isn't on a renameable identifier.
#[allow(clippy::too_many_arguments)]
pub fn rename_plan(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    offset: usize,
    new_name: &str,
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
    java_files: &[PlanFile],
    xml_files: &[PlanFile],
) -> Option<RenamePlan> {
    let target = classify_target(index, file, source, offset, resolver, project_types)?;

    let (old_name, label, edits) = match &target {
        RenameTarget::Local { name, def_start, def_end } => {
            let edits = plan_local(source, file, *def_start, *def_end, name, new_name);
            (name.clone(), format!("local `{name}`"), edits)
        }
        RenameTarget::Member { key } => {
            let edits = plan_member(index, file, source, key, new_name);
            (member_name(key), key.label(), edits)
        }
        RenameTarget::Type { binary, .. } => {
            let old = simple_of(binary);
            let edits = plan_type(index, binary, &old, new_name, java_files, xml_files, project_types);
            (old, format!("type {}", binary.replace('/', ".")), edits)
        }
    };

    let mut order: Vec<String> = Vec::new();
    let mut by_file: HashMap<String, Vec<Edit>> = HashMap::new();
    let mut has_inferred = false;
    for e in edits {
        has_inferred |= e.inferred;
        if !by_file.contains_key(&e.file) {
            order.push(e.file.clone());
        }
        by_file.entry(e.file.clone()).or_default().push(e);
    }
    let files = order
        .into_iter()
        .map(|f| {
            let mut edits = by_file.remove(&f).unwrap_or_default();
            edits.sort_by_key(|e| e.start);
            FileEdits { file: f, edits }
        })
        .collect();

    Some(RenamePlan {
        old_name,
        new_name: new_name.to_string(),
        target_label: label,
        files,
        has_inferred,
    })
}

/// Flatten a plan to the concrete edits the FE applies. Kept separate from the preview so
/// the two stages are distinct on the wire — the FE previews, the user confirms, the FE
/// applies. Sorted per file already.
pub fn rename_apply(plan: &RenamePlan) -> Vec<Edit> {
    plan.files.iter().flat_map(|f| f.edits.iter().cloned()).collect()
}

// ── the cached rename engine (built once per project, on the index thread) ────────

/// A ready-to-query rename engine for one project: the whole-project reference index +
/// the resolver + the project-wide simple→binary map + the java/xml source sets. Built
/// once (on the index background thread, alongside the completion provider) and cached
/// behind an `Arc` in the be layer — `plan` then answers a rename request off it.
///
/// `Send + Sync` (the [`IndexResolver`] is, via [`JdkMemberIndex`]) so it lives in the
/// shared project slot across the dispatcher.
pub struct RenameEngine {
    index: ReferenceIndex,
    resolver: IndexResolver<JdkMemberIndex>,
    project_types: HashMap<String, String>,
    java_files: Vec<PlanFile>,
    xml_files: Vec<PlanFile>,
}

impl RenameEngine {
    /// Build the engine from the project's `.java` sources (path, text) + `.xml` config
    /// fragments (path, text), a persisted-index-backed [`IndexResolver`], and the
    /// project-wide simple→binary type map. The reference index is walked here (the O(N)
    /// step) so `plan` is cheap.
    pub fn new(
        java_sources: Vec<(String, String)>,
        xml_sources: Vec<(String, String)>,
        resolver: IndexResolver<JdkMemberIndex>,
        project_types: HashMap<String, String>,
    ) -> Self {
        let ref_input: Vec<SourceFile> = java_sources
            .iter()
            .map(|(p, s)| SourceFile { path: p.clone(), source: s.clone() })
            .collect();
        let index = build_reference_index(&ref_input, &resolver, &project_types);
        let java_files =
            java_sources.into_iter().map(|(path, source)| PlanFile { path, source }).collect();
        let xml_files =
            xml_sources.into_iter().map(|(path, source)| PlanFile { path, source }).collect();
        Self { index, resolver, project_types, java_files, xml_files }
    }

    /// Open the persisted project index at `index_dir`, resolve the JDK for `jdk_version`,
    /// seed the project's simple names, then build the engine over the given source sets.
    /// `Err` when the index can't be opened or the JDK isn't installed.
    pub fn for_project(
        index_dir: &Path,
        jdk_version: &str,
        project_simple_names: &[(String, String)],
        java_sources: Vec<(String, String)>,
        xml_sources: Vec<(String, String)>,
    ) -> Result<Self, String> {
        use bennu_classpath::prelude::resolve_jdk_classpath;
        use bennu_index::prelude::PersistedIndex;

        let blob = index_dir.join("symbols.blob");
        let fst = index_dir.join("names.fst");
        let project = PersistedIndex::open(&blob, &fst).map_err(|e| e.to_string())?;
        let jdk = JdkMemberIndex::new(resolve_jdk_classpath(jdk_version)?);
        let mut resolver = IndexResolver::new(project, jdk);
        let mut project_types = HashMap::new();
        for (simple, binary) in project_simple_names {
            resolver.add_simple_hint(simple, binary);
            project_types.insert(simple.clone(), binary.clone());
        }
        Ok(Self::new(java_sources, xml_sources, resolver, project_types))
    }

    /// Plan a rename at `file`:`offset` → the new name. `None` when the caret isn't on a
    /// renameable identifier. `source` is the (possibly-unsaved) current buffer text.
    pub fn plan(&self, file: &str, source: &str, offset: usize, new_name: &str) -> Option<RenamePlan> {
        rename_plan(
            &self.index,
            file,
            source,
            offset,
            new_name,
            &self.resolver,
            &self.project_types,
            &self.java_files,
            &self.xml_files,
        )
    }

    /// The reference index (for a find-usages query sharing the same build).
    pub fn index(&self) -> &ReferenceIndex {
        &self.index
    }

    /// Find all usages of the symbol at `file`:`offset` (byte offset), for find-usages.
    /// Shares the engine's reference index + resolver with rename (same off-thread build).
    /// `source` is the current (possibly-unsaved) buffer. `None` when the caret isn't on a
    /// referenceable symbol (a local/param is scope-exact and not bucketed here).
    pub fn find_usages(&self, file: &str, source: &str, offset: usize) -> Option<ReferencesResult> {
        references(&self.index, file, source, offset, &self.resolver, &self.project_types)
    }

    /// Resolve the symbol at `file`:`offset` to a hover card (signature + kind + owner).
    /// Shares the engine's classifier + resolver with rename/find-usages (same off-thread
    /// build). `source` is the current (possibly-unsaved) buffer. `None` when the caret
    /// isn't on a symbol we can classify (a local variable / parameter isn't keyed here).
    pub fn hover(&self, file: &str, source: &str, offset: usize) -> Option<HoverInfo> {
        let key = classify_caret(
            &self.index,
            file,
            source,
            offset,
            &self.resolver,
            &self.project_types,
        )?;
        let mut info = hover_for_key(&key, &self.resolver);
        // Best-effort: attach the leading Javadoc of the PROJECT declaration this key
        // resolves to (None for a classpath-only / JDK symbol we can't read the source of).
        info.doc = self.project_doc_for_key(&key);
        Some(info)
    }

    /// Extract the leading Javadoc (`/** … */`) of the project declaration `key` names, by
    /// locating its declaration site in one of the engine's `.java` sources. `None` when
    /// the declaration isn't in a project source (a JDK / dep-jar symbol) or carries no
    /// Javadoc. Best-effort — a parse/lookup miss just yields `None`.
    fn project_doc_for_key(&self, key: &DeclKey) -> Option<String> {
        for f in &self.java_files {
            if let Some(decl_start) = decl_site_for_key(&f.source, key) {
                if let Some(doc) = leading_javadoc(&f.source, decl_start) {
                    return Some(doc);
                }
                // The declaration is in THIS file but has no Javadoc — a same-named type in
                // another package could still carry one, so keep scanning rather than
                // returning early. (Rare; the extra files are already parsed cheaply.)
            }
        }
        None
    }
}

/// The byte offset where the *declaration* of `key` begins in `source` (the start of the
/// `class`/`interface`/`enum`/method/field declaration node, NOT just its name token — so
/// a preceding Javadoc comment can be found immediately above it). `None` when `source`
/// doesn't declare `key`.
fn decl_site_for_key(source: &str, key: &DeclKey) -> Option<usize> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let bytes = source.as_bytes();
    let root = tree.root_node();

    match key {
        DeclKey::Type { binary } => {
            let simple = simple_of(binary);
            find_decl_node_start(&root, bytes, &[
                "class_declaration",
                "interface_declaration",
                "enum_declaration",
            ], &simple, false)
        }
        DeclKey::Method { name, .. } => {
            find_decl_node_start(&root, bytes, &["method_declaration"], name, false)
        }
        DeclKey::Field { name, .. } => {
            find_decl_node_start(&root, bytes, &["variable_declarator"], name, true)
        }
    }
}

/// Walk `root` for a declaration node of one of `kinds` whose `name` child matches `name`,
/// returning the node's start byte. When `want_field`, the `variable_declarator` must sit
/// under a `field_declaration` (not a local) and the reported start is the enclosing
/// `field_declaration` (so its leading Javadoc is found, not the declarator's).
fn find_decl_node_start(
    root: &Node,
    bytes: &[u8],
    kinds: &[&str],
    name: &str,
    want_field: bool,
) -> Option<usize> {
    let mut stack = vec![*root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if kinds.contains(&n.kind()) {
            if want_field {
                let is_field = n.parent().map(|p| p.kind() == "field_declaration").unwrap_or(false);
                if !is_field {
                    continue;
                }
            }
            if let Some(nm) = n.child_by_field_name("name") {
                if nm.utf8_text(bytes).ok() == Some(name) {
                    // For a field, anchor on the enclosing `field_declaration` so the doc
                    // above `private int x;` (not above the bare declarator) is captured.
                    let anchor = if want_field {
                        n.parent().unwrap_or(n)
                    } else {
                        n
                    };
                    return Some(anchor.start_byte());
                }
            }
        }
    }
    None
}

/// Extract and clean the `/** … */` Javadoc block that ends immediately above the
/// declaration starting at `decl_start` in `source`. Returns the joined, trimmed doc text
/// (leading `*` and the `/**` / `*/` markers stripped, capped ~600 chars), or `None` when
/// the lines directly above the declaration aren't a Javadoc block.
fn leading_javadoc(source: &str, decl_start: usize) -> Option<String> {
    // Everything above the declaration. We look only at the whitespace/comment tail here —
    // a modifier keyword (`public`) between the comment and the node can't occur, since the
    // declaration node start already precedes modifiers.
    let head = &source[..decl_start];
    let trimmed = head.trim_end();
    if !trimmed.ends_with("*/") {
        return None;
    }
    // Find the matching `/**` opening the block that this `*/` closes.
    let open = trimmed.rfind("/**")?;
    let close = trimmed.len() - "*/".len();
    if open + "/**".len() > close {
        return None; // malformed / `/**/`
    }
    let inner = &trimmed[open + "/**".len()..close];

    let mut lines: Vec<String> = Vec::new();
    for raw in inner.lines() {
        let mut l = raw.trim();
        // Strip a leading `*` (the Javadoc gutter) and one following space.
        if let Some(rest) = l.strip_prefix('*') {
            l = rest.strip_prefix(' ').unwrap_or(rest);
        }
        lines.push(l.to_string());
    }
    // Drop leading/trailing empty lines, then join.
    while lines.first().map(|s| s.is_empty()).unwrap_or(false) {
        lines.remove(0);
    }
    while lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    let joined = lines.join("\n");
    let doc = joined.trim();
    if doc.is_empty() {
        return None;
    }
    Some(doc.chars().take(600).collect())
}

/// A resolved hover card for the symbol under the caret (the intel-level view the be layer
/// maps to the wire `HoverInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverInfo {
    /// The signature line: a member's `raw_signature` (or a synthesized `name(...)`
    /// fallback), or a type's dotted FQCN.
    pub signature: String,
    /// `"method"` | `"field"` | `"class"` (types are reported as `"class"`, best-effort —
    /// interface/enum aren't distinguished from the reference index).
    pub kind: String,
    /// The owning type's dotted FQCN for a member; `None` for a type.
    pub container: Option<String>,
    /// A best-effort leading Javadoc for a PROJECT declaration (the `/** … */` block
    /// immediately above it, markers stripped, capped ~600 chars). `None` for a JDK /
    /// dep-jar symbol (source not readable) or a declaration with no Javadoc.
    pub doc: Option<String>,
}

/// Build a [`HoverInfo`] for a classified [`DeclKey`], resolving a member's signature from
/// the resolver's [`bennu_java::prelude::ClassMembers`] (falling back to a synthesized
/// `name(...)` when the class isn't on the resolvable classpath or carries no signature).
fn hover_for_key(key: &DeclKey, resolver: &dyn TypeResolver) -> HoverInfo {
    match key {
        DeclKey::Type { binary } => HoverInfo {
            signature: binary.replace('/', "."),
            kind: "class".to_string(),
            container: None,
            doc: None,
        },
        DeclKey::Method { owner, name } => {
            let signature = member_signature(resolver, owner, name, true)
                .unwrap_or_else(|| format!("{name}(…)"));
            HoverInfo {
                signature,
                kind: "method".to_string(),
                container: Some(owner.replace('/', ".")),
                doc: None,
            }
        }
        DeclKey::Field { owner, name } => {
            let signature =
                member_signature(resolver, owner, name, false).unwrap_or_else(|| name.clone());
            HoverInfo {
                signature,
                kind: "field".to_string(),
                container: Some(owner.replace('/', ".")),
                doc: None,
            }
        }
    }
}

/// Look up a member's `raw_signature` on `owner` (walking supertypes, like the reference
/// walk's `declaring_owner`). `None` when the class isn't resolvable or the member has no
/// recorded signature (the caller then synthesizes a fallback).
fn member_signature(
    resolver: &dyn TypeResolver,
    owner: &str,
    name: &str,
    is_method: bool,
) -> Option<String> {
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![owner.to_string()];
    while let Some(bn) = stack.pop() {
        if !visited.insert(bn.clone()) {
            continue;
        }
        let cm = resolver.members_of(&bn)?;
        let pool = if is_method { &cm.methods } else { &cm.fields };
        if let Some(m) = pool.iter().find(|m| m.name == name) {
            if !m.raw_signature.is_empty() {
                return Some(m.raw_signature.clone());
            }
            // No recorded signature: synthesize a minimal one from the name (+ empty
            // param list for a method) so the hover still shows something meaningful.
            return Some(if is_method { format!("{name}()") } else { name.to_string() });
        }
        if let Some(sc) = cm.superclass {
            stack.push(sc);
        }
        stack.extend(cm.interfaces);
    }
    None
}

// ── local variable / parameter: scope-exact single-file ──────────────────────────

fn plan_local(
    source: &str,
    file: &str,
    def_start: usize,
    def_end: usize,
    name: &str,
    new_name: &str,
) -> Vec<Edit> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else { return Vec::new() };
    let bytes = source.as_bytes();

    let root = tree.root_node();
    let Some(def_node) = smallest_named_covering(&root, def_start) else { return Vec::new() };
    let scope = enclosing_scope(&def_node).unwrap_or(root);

    let mut edits = Vec::new();
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if n.kind() == "identifier" && !crate::refs::is_member_selector_node(&n) {
            if let Ok(t) = n.utf8_text(bytes) {
                if t == name {
                    edits.push(Edit {
                        file: file.to_string(),
                        start: n.start_byte(),
                        end: n.end_byte(),
                        new_text: new_name.to_string(),
                        old: name.to_string(),
                        reason: if n.start_byte() == def_start && n.end_byte() == def_end {
                            EditReason::Declaration
                        } else {
                            EditReason::Local
                        },
                        inferred: false,
                    });
                }
            }
        }
    }
    edits
}

/// The nearest scope node that bounds a local binding: a method/constructor body block, a
/// `for`/`enhanced_for`/`catch` clause, or a lambda body.
fn enclosing_scope<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            "method_declaration" | "constructor_declaration" => {
                return n.child_by_field_name("body");
            }
            "for_statement" | "enhanced_for_statement" | "catch_clause" | "lambda_expression" => {
                return Some(n);
            }
            _ => {}
        }
        cur = n.parent();
    }
    None
}

// ── field / method: declaration + cross-file references ───────────────────────────

fn plan_member(
    index: &ReferenceIndex,
    decl_file: &str,
    decl_source: &str,
    key: &DeclKey,
    new_name: &str,
) -> Vec<Edit> {
    let name = member_name(key);
    let mut edits = Vec::new();

    if let Some((ds, de)) = find_member_decl(decl_source, key) {
        edits.push(Edit {
            file: decl_file.to_string(),
            start: ds,
            end: de,
            new_text: new_name.to_string(),
            old: name.clone(),
            reason: EditReason::Declaration,
            inferred: false,
        });
    }

    let is_method = matches!(key, DeclKey::Method { .. });
    for u in index.usages_of(key) {
        edits.push(Edit {
            file: u.file.clone(),
            start: u.start,
            end: u.end,
            new_text: new_name.to_string(),
            old: name.clone(),
            reason: EditReason::Reference,
            inferred: is_method,
        });
    }
    edits
}

/// Find the byte span of a member declaration's NAME token in its source file.
fn find_member_decl(source: &str, key: &DeclKey) -> Option<(usize, usize)> {
    let (name, want_field) = match key {
        DeclKey::Method { name, .. } => (name, false),
        DeclKey::Field { name, .. } => (name, true),
        DeclKey::Type { .. } => return None,
    };
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        let hit = match n.kind() {
            "method_declaration" if !want_field => n.child_by_field_name("name"),
            "variable_declarator" if want_field => {
                let is_field =
                    n.parent().map(|p| p.kind() == "field_declaration").unwrap_or(false);
                if is_field { n.child_by_field_name("name") } else { None }
            }
            _ => None,
        };
        if let Some(nm) = hit {
            if nm.utf8_text(bytes).ok() == Some(name.as_str()) {
                return Some((nm.start_byte(), nm.end_byte()));
            }
        }
    }
    None
}

// ── class / interface: decl + refs + imports + Spring bean XML ────────────────────

#[allow(clippy::too_many_arguments)]
fn plan_type(
    index: &ReferenceIndex,
    binary: &str,
    old_simple: &str,
    new_name: &str,
    java_files: &[PlanFile],
    xml_files: &[PlanFile],
    project_types: &HashMap<String, String>,
) -> Vec<Edit> {
    let mut edits = Vec::new();
    let old_fqcn = binary.replace('/', ".");
    let new_fqcn = replace_simple(&old_fqcn, new_name);

    // (1) simple-name use sites (the reference index buckets these under DeclKey::Type; the
    // builder EXCLUDES the declaration name, added separately in (2)).
    for u in index.usages_of(&DeclKey::Type { binary: binary.to_string() }) {
        edits.push(Edit {
            file: u.file.clone(),
            start: u.start,
            end: u.end,
            new_text: new_name.to_string(),
            old: old_simple.to_string(),
            reason: EditReason::Reference,
            inferred: false,
        });
    }

    // (2) the declaration name + import statements, scanned per java file.
    for f in java_files {
        collect_type_decl_and_imports(
            &f.source,
            &f.path,
            old_simple,
            &old_fqcn,
            new_name,
            project_types,
            binary,
            &mut edits,
        );
    }

    // (3) Spring bean XML `<bean class="oldFQCN">` → rewrite the FQCN (package kept).
    for f in xml_files {
        for span in bean_class_value_spans(&f.source, &old_fqcn) {
            edits.push(Edit {
                file: f.path.clone(),
                start: span.start,
                end: span.end,
                new_text: new_fqcn.clone(),
                old: old_fqcn.clone(),
                reason: EditReason::SpringBean,
                inferred: false,
            });
        }
    }

    edits
}

#[allow(clippy::too_many_arguments)]
fn collect_type_decl_and_imports(
    source: &str,
    path: &str,
    old_simple: &str,
    old_fqcn: &str,
    new_name: &str,
    project_types: &HashMap<String, String>,
    binary: &str,
    out: &mut Vec<Edit>,
) {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return;
    }
    let Some(tree) = parser.parse(source, None) else { return };
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        match n.kind() {
            "class_declaration" | "interface_declaration" | "enum_declaration" => {
                if let Some(nm) = n.child_by_field_name("name") {
                    if nm.utf8_text(bytes).ok() == Some(old_simple) {
                        // Confirm this really is the target type (same binary) so a
                        // same-named class in another package isn't hit.
                        let is_target =
                            project_types.get(old_simple).map(|b| b == binary).unwrap_or(true);
                        if is_target {
                            out.push(Edit {
                                file: path.to_string(),
                                start: nm.start_byte(),
                                end: nm.end_byte(),
                                new_text: new_name.to_string(),
                                old: old_simple.to_string(),
                                reason: EditReason::Declaration,
                                inferred: false,
                            });
                        }
                    }
                }
            }
            "import_declaration" => {
                if let Some(pn) = n
                    .named_children(&mut n.walk())
                    .find(|c| matches!(c.kind(), "scoped_identifier" | "identifier"))
                {
                    if pn.utf8_text(bytes).ok() == Some(old_fqcn) {
                        // Replace only the trailing simple name (after the final `.`).
                        let path_end = pn.end_byte();
                        let simple_start = path_end - old_simple.len();
                        out.push(Edit {
                            file: path.to_string(),
                            start: simple_start,
                            end: path_end,
                            new_text: new_name.to_string(),
                            old: old_simple.to_string(),
                            reason: EditReason::Import,
                            inferred: false,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────────

fn member_name(key: &DeclKey) -> String {
    match key {
        DeclKey::Method { name, .. } | DeclKey::Field { name, .. } => name.clone(),
        DeclKey::Type { binary } => simple_of(binary),
    }
}

fn simple_of(binary: &str) -> String {
    binary.rsplit('/').next().unwrap_or(binary).to_string()
}

/// Replace the trailing simple name of a dotted FQCN (`com.x.Foo` + `Bar` → `com.x.Bar`).
fn replace_simple(fqcn: &str, new_simple: &str) -> String {
    match fqcn.rfind('.') {
        Some(i) => format!("{}.{}", &fqcn[..i], new_simple),
        None => new_simple.to_string(),
    }
}

fn smallest_named_covering<'t>(root: &Node<'t>, offset: usize) -> Option<Node<'t>> {
    let probe = offset;
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
    use crate::refs::{build_reference_index, SourceFile};
    use bennu_java::prelude::{
        extract_symbols, ClassMembers, Import, Member, MemberKind, TypeRef, TypeResolver, Visibility,
    };

    struct SrcResolver {
        project: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    fn build_resolver(files: &[(&str, &str)]) -> (SrcResolver, HashMap<String, String>) {
        let mut project_types: HashMap<String, String> = HashMap::new();
        for (_p, s) in files {
            for td in extract_symbols(s).types {
                project_types.insert(td.name.clone(), td.fqn.replace('.', "/"));
            }
        }
        let mut project = HashMap::new();
        for (_p, s) in files {
            for td in &extract_symbols(s).types {
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
                project.insert(
                    binary,
                    ClassMembers { superclass: None, interfaces: vec![], methods, fields: vec![] },
                );
            }
        }
        let mut simple = project_types.clone();
        simple.insert("String".into(), "java/lang/String".into());
        (SrcResolver { project, simple }, project_types)
    }

    impl TypeResolver for SrcResolver {
        fn members_of(&self, binary: &str) -> Option<ClassMembers> {
            self.project.get(binary).cloned()
        }
        fn resolve_simple_name(&self, name: &str, imports: &[Import]) -> Option<String> {
            for imp in imports {
                if imp.simple_name() == Some(name) {
                    return Some(imp.path.replace('.', "/"));
                }
            }
            self.simple.get(name).cloned()
        }
    }

    fn plan(
        files: &[(&str, &str)],
        xml: &[(&str, &str)],
        target_file: &str,
        offset: usize,
        new_name: &str,
    ) -> Option<RenamePlan> {
        let (resolver, project_types) = build_resolver(files);
        let src: Vec<SourceFile> = files
            .iter()
            .map(|(p, s)| SourceFile { path: p.to_string(), source: s.to_string() })
            .collect();
        let index = build_reference_index(&src, &resolver, &project_types);
        let java_files: Vec<PlanFile> =
            files.iter().map(|(p, s)| PlanFile { path: p.to_string(), source: s.to_string() }).collect();
        let xml_files: Vec<PlanFile> =
            xml.iter().map(|(p, s)| PlanFile { path: p.to_string(), source: s.to_string() }).collect();
        let source = files.iter().find(|(p, _)| *p == target_file).unwrap().1;
        rename_plan(
            &index,
            target_file,
            source,
            offset,
            new_name,
            &resolver,
            &project_types,
            &java_files,
            &xml_files,
        )
    }

    #[test]
    fn local_rename_is_scope_exact() {
        let src = "package p; public class C { int f() { int x = 1; return x + x; } int g() { int x = 2; return x; } }";
        let files = [("C.java", src)];
        let off = src.find("int x = 1").unwrap() + "int ".len();
        let p = plan(&files, &[], "C.java", off, "y").expect("classified");
        // f()'s x: decl + two uses = 3, none in g().
        assert_eq!(p.total_edits(), 3);
        let g = src.find("int g()").unwrap();
        assert!(p.files[0].edits.iter().all(|e| e.start < g));
    }

    #[test]
    fn method_rename_hits_decl_and_calls() {
        let files = [
            ("A.java", "package p; public class A { int v() { return 1; } }"),
            ("B.java", "package p; public class B { int u(A a) { return a.v() + a.v(); } }"),
        ];
        let src = files[0].1;
        let off = src.find("int v()").unwrap() + "int ".len();
        let p = plan(&files, &[], "A.java", off, "value").expect("classified");
        assert_eq!(p.total_edits(), 3);
        let decls = p
            .files
            .iter()
            .flat_map(|f| &f.edits)
            .filter(|e| e.reason == EditReason::Declaration)
            .count();
        assert_eq!(decls, 1);
    }

    #[test]
    fn class_rename_edits_decl_import_ref_and_bean() {
        let widget = ("Widget.java", "package com.acme; public class Widget { }");
        let consumer = (
            "Consumer.java",
            "package com.app; import com.acme.Widget; public class Consumer { Widget w; Widget make() { return new Widget(); } }",
        );
        let beans = (
            "beans.xml",
            r#"<beans><bean id="w" class="com.acme.Widget"/><package><action name="s" class="w"/></package></beans>"#,
        );
        let files = [widget, consumer];
        let src = widget.1;
        let off = src.find("class Widget").unwrap() + "class ".len();
        let p = plan(&files, &[beans], "Widget.java", off, "Gadget").expect("classified");
        let all: Vec<_> = p.files.iter().flat_map(|f| &f.edits).collect();
        let cnt = |r: EditReason| all.iter().filter(|e| e.reason == r).count();
        assert_eq!(cnt(EditReason::Declaration), 1);
        assert_eq!(cnt(EditReason::Import), 1);
        assert_eq!(cnt(EditReason::SpringBean), 1);
        assert!(cnt(EditReason::Reference) >= 2);
        // Spring edit rewrites the FQCN; the <action class="w"> bean-id is untouched.
        let bean = all.iter().find(|e| e.reason == EditReason::SpringBean).unwrap();
        assert_eq!(bean.old, "com.acme.Widget");
        assert_eq!(bean.new_text, "com.acme.Gadget");
        // rename_apply flattens the same set.
        assert_eq!(rename_apply(&p).len(), p.total_edits());
    }

    #[test]
    fn leading_javadoc_extracts_type_doc() {
        let src = "package p;\n\n/**\n * Represents an order.\n * Second line.\n */\npublic class Order { }\n";
        let start = decl_site_for_key(src, &DeclKey::Type { binary: "p/Order".into() })
            .expect("type decl found");
        let doc = leading_javadoc(src, start).expect("javadoc found");
        assert_eq!(doc, "Represents an order.\nSecond line.");
    }

    #[test]
    fn leading_javadoc_extracts_method_doc() {
        let src = "package p;\npublic class C {\n  /** Does the thing. */\n  public int go() { return 1; }\n}\n";
        let start = decl_site_for_key(src, &DeclKey::Method { owner: "p/C".into(), name: "go".into() })
            .expect("method decl found");
        let doc = leading_javadoc(src, start).expect("javadoc found");
        assert_eq!(doc, "Does the thing.");
    }

    #[test]
    fn no_javadoc_yields_none() {
        let src = "package p;\n// just a line comment\npublic class C { }\n";
        let start = decl_site_for_key(src, &DeclKey::Type { binary: "p/C".into() }).unwrap();
        assert!(leading_javadoc(src, start).is_none());
    }
}
