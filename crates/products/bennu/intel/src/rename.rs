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
    build_reference_index_incremental, build_reference_index_with_progress, classify_caret,
    classify_target, references, DeclKey, ReferenceIndex, ReferencesResult, RenameTarget,
    SourceFile,
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

/// A resolved go-to-declaration target: the declaration NAME span in the owning **project**
/// file, plus 1-based line/col (computed from that file's source) and a human label. The be
/// layer maps this onto the wire `DeclarationTarget` field-for-field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationLocation {
    /// Absolute path (forward slashes) of the project file declaring the symbol.
    pub file: String,
    /// Start byte offset of the declaration NAME token in `file`.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// 1-based line of the declaration name in `file`.
    pub line: u32,
    /// 1-based column of the declaration name in `file`.
    pub col: u32,
    /// A short human label of the target (`"method com.x.Foo.bar()"`, `"class com.x.Order"`,
    /// `"local `x`"`) — the same style [`crate::refs::DeclKey::label`] uses.
    pub label: String,
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

/// Resolve the caret at `file`:`offset` to its DECLARATION site (go-to-declaration). Runs
/// the same caret classification find-usages / rename share, then returns the declaration
/// NAME span + owning project file (+ 1-based line/col from the declaring file's source).
/// The free-function core [`RenameEngine::declaration`] wraps — kept separate so it's
/// testable with an in-memory resolver (no live JDK), like [`rename_plan`] / [`references`].
///
/// `None` when the caret isn't on a resolvable symbol, or the declaration lives in a JDK /
/// dep-jar (no project source in `java_files` declares it → nothing to open).
pub fn resolve_declaration(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
    java_files: &[PlanFile],
) -> Option<DeclarationLocation> {
    let target = classify_target(index, file, source, offset, resolver, project_types)?;
    match target {
        RenameTarget::Local { name, def_start, def_end } => {
            // A local/param declaration is in the CURRENT buffer (scope-exact).
            let (line, col) = line_col_1based(source, def_start);
            Some(DeclarationLocation {
                file: file.to_string(),
                start: def_start,
                end: def_end,
                line,
                col,
                label: format!("local `{name}`"),
            })
        }
        RenameTarget::Member { key } => {
            // The declaration lives on the OWNER type. Scan project sources for its name
            // span; no project source declaring it ⇒ a JDK/dep symbol ⇒ None.
            let (decl_file, s, e) =
                first_hit(java_files, |src| find_member_name_span(src, &key))?;
            let decl_src = project_source(java_files, &decl_file)?;
            let (line, col) = line_col_1based(decl_src, s);
            Some(DeclarationLocation { file: decl_file, start: s, end: e, line, col, label: key.label() })
        }
        RenameTarget::Type { binary, .. } => {
            let simple = simple_of(&binary);
            let (decl_file, s, e) =
                first_hit(java_files, |src| find_type_name_span(src, &simple))?;
            let decl_src = project_source(java_files, &decl_file)?;
            let (line, col) = line_col_1based(decl_src, s);
            Some(DeclarationLocation {
                file: decl_file,
                start: s,
                end: e,
                line,
                col,
                label: format!("class {}", binary.replace('/', ".")),
            })
        }
    }
}

/// Scan `java_files` for the first source whose `find` yields a span, returning
/// `(file_path, start, end)`. `None` when no project source matches (a JDK / dep symbol).
fn first_hit(
    java_files: &[PlanFile],
    find: impl Fn(&str) -> Option<(usize, usize)>,
) -> Option<(String, usize, usize)> {
    for f in java_files {
        if let Some((s, e)) = find(&f.source) {
            return Some((f.path.clone(), s, e));
        }
    }
    None
}

/// The cached source text of a project java file by its (forward-slash) path.
fn project_source<'a>(java_files: &'a [PlanFile], file: &str) -> Option<&'a str> {
    java_files.iter().find(|f| f.path == file).map(|f| f.source.as_str())
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
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Self {
        let ref_input: Vec<SourceFile> = java_sources
            .iter()
            .map(|(p, s)| SourceFile { path: p.clone(), source: s.clone() })
            .collect();
        let index =
            build_reference_index_with_progress(&ref_input, &resolver, &project_types, on_progress);
        let java_files =
            java_sources.into_iter().map(|(path, source)| PlanFile { path, source }).collect();
        let xml_files =
            xml_sources.into_iter().map(|(path, source)| PlanFile { path, source }).collect();
        Self { index, resolver, project_types, java_files, xml_files }
    }

    /// Open the persisted project index at `index_dir`, seed the project's simple names, then
    /// build the engine over the given source sets. `Err` only when the index can't be opened.
    ///
    /// The engine is **project-only** — it never resolves JDK / library types — so it does NOT
    /// open the JDK classpath (`_jdk_version` is unused): find-usages / rename only target
    /// project symbols, and decoding JDK bytecode in the reference walk was pure waste that made
    /// the walk crawl for minutes on a large project. This also makes both work with no JDK
    /// installed. An empty member source stands in for the unused JDK slot.
    pub fn for_project(
        index_dir: &Path,
        _jdk_version: &str,
        project_simple_names: &[(String, String)],
        java_sources: Vec<(String, String)>,
        xml_sources: Vec<(String, String)>,
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Result<Self, String> {
        use bennu_classpath::prelude::MultiSource;
        use bennu_index::prelude::PersistedIndex;

        let blob = index_dir.join("symbols.blob");
        let fst = index_dir.join("names.fst");
        let project = PersistedIndex::open(&blob, &fst).map_err(|e| e.to_string())?;
        let jdk = JdkMemberIndex::new(Box::new(MultiSource::new(Vec::new())));
        let mut resolver = IndexResolver::new(project, jdk).project_only();
        let mut project_types = HashMap::new();
        for (simple, binary) in project_simple_names {
            resolver.add_simple_hint(simple, binary);
            project_types.insert(simple.clone(), binary.clone());
        }

        // Incremental, persisted reference walk: reuse the on-disk cache (keyed by per-file
        // content hash) where valid, re-walking only changed files + their dependents. The
        // cache lives at a STABLE path under the index base (the parent of the per-build gen
        // dir), so it survives across opens — a full walk only happens on the first open or a
        // structural type change.
        let cache_path = index_dir.parent().map(crate::refcache::cache_path);
        let prior = cache_path.as_deref().and_then(crate::refcache::load);

        let ref_input: Vec<SourceFile> = java_sources
            .iter()
            .map(|(p, s)| SourceFile { path: p.clone(), source: s.clone() })
            .collect();
        let built =
            build_reference_index_incremental(&ref_input, &resolver, &project_types, prior, on_progress);
        let index = built.index;
        if let (Some(path), Some(cache)) = (&cache_path, &built.cache_to_save) {
            crate::refcache::save(path, cache);
        }

        let java_files =
            java_sources.into_iter().map(|(path, source)| PlanFile { path, source }).collect();
        let xml_files =
            xml_sources.into_iter().map(|(path, source)| PlanFile { path, source }).collect();
        Ok(Self { index, resolver, project_types, java_files, xml_files })
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

    /// Resolve the symbol at `file`:`offset` to its DECLARATION site (go-to-declaration).
    /// Runs the same caret classification find-usages / rename share, then returns the
    /// declaration NAME span + the owning **project** file (with 1-based line/col computed
    /// from the declaring file's source). `source` is the current (possibly-unsaved) buffer.
    ///
    /// `None` (never an error) when the caret isn't on a resolvable symbol, or when the
    /// declaration lives in a JDK / dep-jar (no project source declares it → nothing to
    /// open). A **local variable / parameter** resolves to its declarator in the CURRENT
    /// file (scope-exact); a **method / field** to its name token on the owner type's
    /// declaration; a **class / interface / enum** to its type-declaration name token.
    pub fn declaration(&self, file: &str, source: &str, offset: usize) -> Option<DeclarationLocation> {
        resolve_declaration(
            &self.index,
            file,
            source,
            offset,
            &self.resolver,
            &self.project_types,
            &self.java_files,
        )
    }

    /// Find all usages of the symbol at `file`:`offset` (byte offset), for find-usages.
    /// Shares the engine's reference index + resolver with rename (same off-thread build).
    /// `source` is the current (possibly-unsaved) buffer. `None` when the caret isn't on a
    /// referenceable symbol (a local/param is scope-exact and not bucketed here).
    pub fn find_usages(&self, file: &str, source: &str, offset: usize) -> Option<ReferencesResult> {
        references(&self.index, file, source, offset, &self.resolver, &self.project_types)
    }

    /// The inherited ("super") members of the type declared at `file`:(`type_name`,`line`) —
    /// the Structure panel's lazy "Inherited" bucket. Resolves the type's binary name off its
    /// declaring source, then collects the members of its SUPERCLASS + INTERFACES recursively
    /// (NOT the type's own members), deduping overrides, tagging each with its declaring FQCN
    /// + visibility + (for a project supertype) a source file+line. `[]` when the type can't
    /// be resolved in `file`. Shares the engine's resolver + java sources (same off-thread
    /// build) with completion / rename.
    pub fn inherited_members(
        &self,
        file: &str,
        type_name: &str,
        line: i64,
    ) -> Vec<crate::inherited::InheritedMember> {
        crate::inherited::inherited_members(
            &self.resolver,
            &self.java_files,
            file,
            type_name,
            line,
        )
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
        // `cm` is a shared `Arc` — clone the (small) supertype links, don't move.
        if let Some(sc) = cm.superclass.clone() {
            stack.push(sc);
        }
        stack.extend(cm.interfaces.iter().cloned());
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

    if let Some((ds, de)) = find_member_name_span(decl_source, key) {
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

/// Find the byte span of a member declaration's NAME token in `source`. Shared by rename
/// (the declaration edit site) and go-to-declaration (the navigation target). `None` for a
/// [`DeclKey::Type`] (use [`find_type_name_span`]) or when `source` doesn't declare the
/// member.
pub fn find_member_name_span(source: &str, key: &DeclKey) -> Option<(usize, usize)> {
    let (name, want_field) = match key {
        DeclKey::Method { name, .. } => (name, false),
        DeclKey::Field { name, .. } => (name, true),
        DeclKey::Type { .. } => return None,
    };
    // A declaration's name appears textually in the file that declares it — skip the
    // tree-sitter parse of files that don't even contain the token. `first_hit` scans EVERY
    // project source; without this guard go-to-declaration re-parses the whole project on the
    // handler thread (seconds of freeze on a large project). A substring false-positive just
    // parses one extra file that then yields no match — correct, only slightly slower.
    if !source.contains(name.as_str()) {
        return None;
    }
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

/// Find the byte span of a type declaration's NAME token in `source` (class / interface /
/// enum matching `simple`). The go-to-declaration counterpart of [`find_member_name_span`]
/// for a [`DeclKey::Type`]. `None` when `source` doesn't declare a type with that simple
/// name. (A same-named type in another package could match; the caller scans the project's
/// sources and the first hit wins — good enough for navigation, and the type-map keying in
/// classification already narrowed the caret to this binary name.)
pub fn find_type_name_span(source: &str, simple: &str) -> Option<(usize, usize)> {
    // See `find_member_name_span`: skip the parse when the type name isn't even in the file,
    // so `first_hit` doesn't re-parse the whole project on the handler thread.
    if !source.contains(simple) {
        return None;
    }
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
        if matches!(
            n.kind(),
            "class_declaration" | "interface_declaration" | "enum_declaration"
        ) {
            if let Some(nm) = n.child_by_field_name("name") {
                if nm.utf8_text(bytes).ok() == Some(simple) {
                    return Some((nm.start_byte(), nm.end_byte()));
                }
            }
        }
    }
    None
}

/// 1-based `(line, col)` of byte `start` in `source`: line = 1 + count of `'\n'` before
/// `start`; col = 1 + bytes since the last `'\n'` (or from the start of the file). Byte
/// columns (not char columns) — the FE maps against the same byte buffer.
fn line_col_1based(source: &str, start: usize) -> (u32, u32) {
    let clamped = start.min(source.len());
    let head = &source.as_bytes()[..clamped];
    let line = 1 + head.iter().filter(|&&b| b == b'\n').count() as u32;
    let col = match head.iter().rposition(|&b| b == b'\n') {
        Some(i) => (clamped - (i + 1)) as u32 + 1,
        None => clamped as u32 + 1,
    };
    (line, col)
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
        fn members_of(&self, binary: &str) -> Option<std::sync::Arc<ClassMembers>> {
            self.project.get(binary).cloned().map(std::sync::Arc::new)
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

    fn decl(files: &[(&str, &str)], target_file: &str, offset: usize) -> Option<DeclarationLocation> {
        let (resolver, project_types) = build_resolver(files);
        let src: Vec<SourceFile> = files
            .iter()
            .map(|(p, s)| SourceFile { path: p.to_string(), source: s.to_string() })
            .collect();
        let index = build_reference_index(&src, &resolver, &project_types);
        let java_files: Vec<PlanFile> =
            files.iter().map(|(p, s)| PlanFile { path: p.to_string(), source: s.to_string() }).collect();
        let source = files.iter().find(|(p, _)| *p == target_file).unwrap().1;
        resolve_declaration(&index, target_file, source, offset, &resolver, &project_types, &java_files)
    }

    #[test]
    fn declaration_method_from_call_resolves_to_decl_name() {
        // A bare call inside the SAME class keys to its enclosing owner (p/A) — no receiver
        // inference needed, so this exercises the member-decl path deterministically.
        let src =
            "package p; public class A { int compute() { return 1; } int caller() { return compute(); } }";
        let files = [("A.java", src)];
        let off = src.rfind("compute()").unwrap() + 2; // caret inside the CALL
        let d = decl(&files, "A.java", off).expect("resolved");
        assert_eq!(d.file, "A.java");
        let decl_off = src.find("compute()").unwrap(); // the DECLARATION name
        assert_eq!((d.start, d.end), (decl_off, decl_off + "compute".len()));
        assert_eq!(d.label, "method p.A.compute()");
    }

    #[test]
    fn declaration_field_from_decl_name() {
        // Field with an initializer so the `variable_declarator` span exceeds the name span
        // and the classifier lands on the name identifier (the bare `int x;` tie is an
        // upstream classifier edge, not this feature's concern).
        let src = "package p; public class A { int count = 0; int m() { return count; } }";
        let files = [("A.java", src)];
        let off = src.find("count").unwrap() + 2; // caret on the field DECL name
        let d = decl(&files, "A.java", off).expect("resolved");
        assert_eq!(d.file, "A.java");
        let decl_off = src.find("count").unwrap();
        assert_eq!((d.start, d.end), (decl_off, decl_off + "count".len()));
        assert_eq!(d.label, "field p.A.count");
    }

    #[test]
    fn declaration_local_var_is_current_file() {
        let src = "package p; public class C { int f() { int x = 1; return x + x; } }";
        let files = [("C.java", src)];
        let off = src.rfind("x + x").unwrap() + 1; // caret on a USE of `x`
        let d = decl(&files, "C.java", off).expect("resolved");
        assert_eq!(d.file, "C.java");
        let decl_off = src.find("x = 1").unwrap(); // the declarator name
        assert_eq!((d.start, d.end), (decl_off, decl_off + 1));
        assert_eq!(d.label, "local `x`");
    }

    #[test]
    fn declaration_param_resolves_to_param_name() {
        let src = "package p; public class C { int f(int y) { return y * 2; } }";
        let files = [("C.java", src)];
        let off = src.rfind("y * 2").unwrap() + 1;
        let d = decl(&files, "C.java", off).expect("resolved");
        let decl_off = src.find("int y)").unwrap() + "int ".len();
        assert_eq!((d.start, d.end), (decl_off, decl_off + 1));
        assert_eq!(d.label, "local `y`");
    }

    #[test]
    fn declaration_type_from_cross_file_reference() {
        let widget = ("Widget.java", "package com.acme; public class Widget { }");
        let consumer = (
            "Consumer.java",
            "package com.app; import com.acme.Widget; public class Consumer { Widget w; }",
        );
        let files = [widget, consumer];
        let src = consumer.1;
        let off = src.rfind("Widget w").unwrap() + 1; // caret on the type USE
        let d = decl(&files, "Consumer.java", off).expect("resolved");
        assert_eq!(d.file, "Widget.java");
        let wsrc = widget.1;
        let decl_off = wsrc.find("Widget {").unwrap();
        assert_eq!((d.start, d.end), (decl_off, decl_off + "Widget".len()));
        assert_eq!(d.label, "class com.acme.Widget");
    }

    #[test]
    fn declaration_jdk_symbol_is_none() {
        // `String` isn't a project type — no project source declares it → None (not an
        // error): the FE simply doesn't navigate into an unopenable JDK class.
        let src = "package p; public class C { String s; }";
        let files = [("C.java", src)];
        let off = src.find("String s").unwrap() + 1;
        assert!(decl(&files, "C.java", off).is_none());
    }

    #[test]
    fn line_col_1based_counts_lines_and_bytes() {
        let src = "line1\nline2\nabcXdef";
        let start = src.find('X').unwrap();
        assert_eq!(line_col_1based(src, start), (3, 4));
        assert_eq!(line_col_1based("first", 0), (1, 1));
        // Clamps an out-of-range offset to the source length rather than panicking.
        assert_eq!(line_col_1based("ab", 999), (1, 3));
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
