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

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use bennu_java::prelude::{find_type_name_span, TypeResolver};
use bennu_web::prelude::bean_class_value_spans;
use tree_sitter::{Node, Parser};

use bennu_query::prelude::{
    inherited_members, IndexResolver, InheritedMember, JdkMemberIndex, PlanFile,
};

use crate::refs::{
    build_reference_index_incremental, build_reference_index_with_progress, classify_caret,
    classify_target, references, DeclKey, LangLevel, ReferenceIndex, ReferencesResult,
    RenameTarget, SourceFile,
};

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
    /// The source file this rename also has to move, if any — see [`FileRename`].
    pub file_rename: Option<FileRename>,
    /// Why this rename must NOT be applied, when it must not be.
    ///
    /// The edits are still computed and still shown — seeing what it *would* do is how the reason
    /// makes sense — but a caller has to treat this as a refusal, not a warning to click past.
    /// The one case today is a method that overrides a member of a library type: the jar cannot be
    /// edited to follow, so renaming here produces a class that stops compiling.
    pub blocked: Option<String>,
}

/// A source file that has to be renamed along with the type it holds.
///
/// Java requires a public top-level type and its file to share a name, so renaming the type without
/// the file leaves code that does not compile. Only ever set when the file is named after the type
/// being renamed, which is exactly the top-level case: a nested type's file is named after its
/// outer type and must stay put.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRename {
    /// The file's current path (forward slashes, as the FE keys them).
    pub from: String,
    /// The path it must take — same directory, new basename.
    pub to: String,
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

/// Compute the rename plan for the symbol at `file`:`offset`. `java_files` are all the
/// project's `.java` sources; `xml_files` are the config fragments. Returns `None` when
/// the caret isn't on a renameable identifier.
///
/// `resolver` classifies the caret and walks the project hierarchy — it is the same (cheap) view
/// the reference walk ran with. `policy` answers the one question that needs the FULL classpath:
/// whether a supertype in a dependency jar declares this method, which decides whether the rename
/// is allowed at all. Pass the same resolver for both when there is only one.
#[allow(clippy::too_many_arguments)]
pub fn rename_plan(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    offset: usize,
    new_name: &str,
    resolver: &dyn TypeResolver,
    policy: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
    java_files: &[PlanFile],
    xml_files: &[PlanFile],
    level: LangLevel,
) -> Option<RenamePlan> {
    let target = classify_target(index, file, source, offset, resolver, project_types, level)?;
    let mut file_rename: Option<FileRename> = None;
    let mut blocked: Option<String> = None;

    let (old_name, label, edits) = match &target {
        RenameTarget::Local { name, def_start, def_end } => {
            let edits = plan_local(source, file, *def_start, *def_end, name, new_name);
            (name.clone(), format!("local `{name}`"), edits)
        }
        RenameTarget::Member { key } => {
            // The declaration lives in the file that DECLARES the member's owner (walked up the
            // hierarchy), which is not necessarily the caret's file. Renaming from a use site in
            // another file must still rewrite the declaration — otherwise the plan renamed the
            // call sites but left `int foo()` untouched. Fall back to the caret file only when
            // the declaring file isn't a known project source (a JDK/dep owner).
            // A method can be declared at several levels of one hierarchy, and to every caller
            // that is ONE method — so the whole override family moves together. A field has no
            // such family (hiding a field is not overriding it, and a shadowing field of the same
            // name is a different field), so it stays alone.
            let owners: Vec<String> = match key {
                DeclKey::Method { owner, name } => {
                    // A library supertype declaring the same method makes this an override of code
                    // we cannot edit. Plan it anyway — the edit list is what makes the refusal
                    // legible — but mark it unappliable.
                    blocked = library_override(policy, owner, name).map(|lib| {
                        format!(
                            "`{name}` overrides {} — a library type, which cannot be renamed with it. \
                             Renaming only this side would stop the class implementing what it declares.",
                            lib.replace('/', ".")
                        )
                    });
                    override_family(resolver, project_types, owner, name)
                }
                _ => vec![key.owner_binary().to_string()],
            };
            let mut edits = Vec::new();
            for owner in &owners {
                let member = match key {
                    DeclKey::Method { name, .. } => {
                        DeclKey::Method { owner: owner.clone(), name: name.clone() }
                    }
                    other => other.clone(),
                };
                let decl_file = index.file_declaring(owner).unwrap_or(file);
                let decl_source = project_source(java_files, decl_file).unwrap_or(source);
                edits.extend(plan_member(index, decl_file, decl_source, &member, new_name));
            }
            // Two levels of the family can land on the same bytes (a file declaring both). Keep
            // one edit per range, preferring the declaration — the same rule the type pass uses.
            edits.sort_by(|a, b| {
                (a.file.as_str(), a.start, a.end, reason_rank(&a.reason))
                    .cmp(&(b.file.as_str(), b.start, b.end, reason_rank(&b.reason)))
            });
            edits.dedup_by(|a, b| a.file == b.file && a.start == b.start && a.end == b.end);
            // A member plan with no DECLARATION edit would rewrite every caller to a name that
            // nothing declares — worse than doing nothing. It happens when the caret is on a
            // member NOBODY WROTE DOWN: a Lombok accessor has no source to edit, so renaming
            // `getCustomerName` here would leave the generated method named after the field it
            // still comes from. Refuse, and let the user rename the FIELD — the generated
            // accessors follow it (see `generated_accessors`).
            if !edits.iter().any(|e| e.reason == EditReason::Declaration) {
                return None;
            }
            (member_name(key), key.label(), edits)
        }
        RenameTarget::Type { binary, .. } => {
            let old = simple_of(binary);
            let edits = plan_type(index, binary, &old, new_name, java_files, xml_files, project_types);
            file_rename = index
                .file_declaring(binary)
                .and_then(|decl| file_rename_for(decl, &old, new_name));
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
        blocked,
        file_rename,
    })
}

/// The file move a type rename implies, or `None` when the file is not named after the type.
///
/// The name check is the whole test: Java ties a public top-level type to its filename, so a file
/// called `Order.java` holding `Order` must become `Bar.java` when `Order` becomes `Bar` — while a
/// nested type lives in a file named after its OUTER type, where the basename won't match and
/// nothing moves. `None` too when the new name would not change the path at all.
pub fn file_rename_for(decl_file: &str, old_simple: &str, new_name: &str) -> Option<FileRename> {
    let (dir, base) = match decl_file.rfind('/') {
        Some(i) => (&decl_file[..=i], &decl_file[i + 1..]),
        None => ("", decl_file),
    };
    let stem = base.strip_suffix(".java")?;
    if stem != old_simple || new_name == old_simple {
        return None;
    }
    Some(FileRename { from: decl_file.to_string(), to: format!("{dir}{new_name}.java") })
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
    level: LangLevel,
) -> Option<DeclarationLocation> {
    let target = classify_target(index, file, source, offset, resolver, project_types, level)?;
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
            // Resolve the member in the file that DECLARES ITS OWNER — `declaring_owner`
            // already walked the hierarchy to the type that actually declares it. Scanning
            // every file for a same-named member (the old behaviour) jumped to a random other
            // class. `None` when the owner isn't project source (a JDK/dep type) or the span
            // isn't found there.
            let decl_file = index.file_declaring(key.owner_binary())?.to_string();
            let decl_src = project_source(java_files, &decl_file)?;
            if let Some((s, e)) = find_member_name_span(decl_src, &key) {
                let (line, col) = line_col_1based(decl_src, s);
                return Some(DeclarationLocation {
                    file: decl_file,
                    start: s,
                    end: e,
                    line,
                    col,
                    label: key.label(),
                });
            }
            // No source method with that name: a Lombok-generated accessor (`getId`/`setId`/
            // `isShipped`, a fluent `customer`) has no name token to open — redirect to the BACKING
            // FIELD it wraps. Several candidate names are tried because the accessor→field mapping
            // isn't injective (see `backing_field_candidates`); the first that is a real field wins,
            // and a candidate that isn't simply doesn't match.
            if let DeclKey::Method { owner, name } = &key {
                for field in crate::lombok::backing_field_candidates(name) {
                    let field_key = DeclKey::Field { owner: owner.clone(), name: field };
                    if let Some((s, e)) = find_member_name_span(decl_src, &field_key) {
                        let (line, col) = line_col_1based(decl_src, s);
                        return Some(DeclarationLocation {
                            file: decl_file,
                            start: s,
                            end: e,
                            line,
                            col,
                            label: field_key.label(),
                        });
                    }
                }
            }
            None
        }
        RenameTarget::Type { binary, .. } => {
            // The declaring file is the one whose symbols carry this exact binary — not merely
            // a file with a same-simple-named type in another package.
            let decl_file = index.file_declaring(&binary)?.to_string();
            let decl_src = project_source(java_files, &decl_file)?;
            let simple = simple_of(&binary);
            let (s, e) = find_type_name_span(decl_src, &simple)?;
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
/// `Send + Sync` so it lives in the shared project slot across the dispatcher. The resolver is
/// type-erased behind an `Arc` because in production it is the **provider's own** fully-resolving
/// one, shared rather than duplicated (see [`for_project`](Self::for_project)).
pub struct RenameEngine {
    index: ReferenceIndex,
    resolver: Arc<dyn TypeResolver + Send + Sync>,
    /// The resolver POLICY questions are asked of — see [`RenameEngine::with_policy_resolver`].
    /// Defaults to `resolver`, so an engine built without one behaves exactly as before.
    policy: Arc<dyn TypeResolver + Send + Sync>,
    project_types: HashMap<String, String>,
    java_files: Vec<PlanFile>,
    xml_files: Vec<PlanFile>,
    /// The project's Java language level — gates recognition of version-specific binding forms
    /// (records, pattern variables, lambda inferred params) during caret classification.
    lang_level: LangLevel,
}

/// A resolver over the persisted project index ALONE — no JDK, no dependencies. The fallback for
/// [`RenameEngine::for_project`] when the caller has no shared resolver to lend it (the provider
/// failed to build, or no JDK is installed): rename over project symbols keeps working, it just
/// cannot follow a library generic back to a project type. An empty member source stands in for
/// the unused classpath slot.
fn project_only_resolver(
    index_dir: &Path,
    project_simple_names: &[(String, String)],
) -> Result<IndexResolver<JdkMemberIndex>, String> {
    use bennu_classpath::prelude::MultiSource;
    use bennu_index::prelude::PersistedIndex;

    let blob = index_dir.join("symbols.blob");
    let fst = index_dir.join("names.fst");
    let project = PersistedIndex::open(&blob, &fst).map_err(|e| e.to_string())?;
    let jdk = JdkMemberIndex::new(Box::new(MultiSource::new(Vec::new())));
    let mut resolver = IndexResolver::new(project, jdk).project_only();
    for (simple, binary) in project_simple_names {
        resolver.add_simple_hint(simple, binary);
    }
    Ok(resolver)
}

impl RenameEngine {
    /// Build the engine from the project's `.java` sources (path, text) + `.xml` config
    /// fragments (path, text), a type resolver, and the project-wide simple→binary type map.
    /// The reference index is walked here (the O(N) step) so `plan` is cheap.
    pub fn new(
        java_sources: Vec<(String, String)>,
        xml_sources: Vec<(String, String)>,
        resolver: Arc<dyn TypeResolver + Send + Sync>,
        project_types: HashMap<String, String>,
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Self {
        let ref_input: Vec<SourceFile> = java_sources
            .iter()
            .map(|(p, s)| SourceFile { path: p.clone(), source: s.clone() })
            .collect();
        let index =
            build_reference_index_with_progress(&ref_input, &*resolver, &project_types, on_progress);
        let java_files =
            java_sources.into_iter().map(|(path, source)| PlanFile { path, source }).collect();
        let xml_files =
            xml_sources.into_iter().map(|(path, source)| PlanFile { path, source }).collect();
        // No project version here (the test/plain constructor) → unknown level enables all
        // binding forms.
        let policy = Arc::clone(&resolver);
        Self { index, resolver, policy, project_types, java_files, xml_files, lang_level: LangLevel(0) }
    }

    /// Lend the engine a SECOND resolver, used only to answer policy questions — today, "does this
    /// method override something declared in a dependency jar?".
    ///
    /// The two differ in cost, not in kind. The walk resolver is deliberately cheap (JDK-only by
    /// default, see [`RenameEngine::for_project`]) because it is consulted once per reference in
    /// every file; a policy question is asked **once per rename**, so it can afford the full
    /// classpath — and it has to, because the interface whose contract a rename would break lives
    /// exactly in the tier the cheap resolver drops. Without this, a method implementing
    /// `jakarta.validation.ConstraintValidator` renamed clean and stopped compiling: the ancestor
    /// was named in the project's own record, but nothing could read its members to see the
    /// method declared there.
    pub fn with_policy_resolver(mut self, policy: Arc<dyn TypeResolver + Send + Sync>) -> Self {
        self.policy = policy;
        self
    }

    /// Build the engine over the given source sets, resolving types with `shared_resolver` when
    /// the caller has one. `Err` only when no resolver is supplied AND the index can't be opened.
    ///
    /// ## Why the resolver is shared, not built here
    /// This engine used to build its own **project-only** resolver: find-usages / rename target
    /// project symbols, so resolving JDK types looked like pure waste — and back when every
    /// `members_of` re-parsed the bytecode, it really did make the walk crawl for minutes.
    ///
    /// That reasoning has one hole. A library type is not only a destination, it is a **conduit**:
    /// in `failures.stream().map(f -> f.getPath())` the lambda parameter `f` is typed by
    /// substituting the project type through `List` → `Stream` → `Function`. With no JDK the
    /// substitution dead-ends, the walk records no edge for `f.getPath()`, and a rename of that
    /// member silently misses every such call — the symptom being an edit list that covers the
    /// declaration and the plain call sites but skips the ones inside stream/optional chains.
    ///
    /// ## Why it is still project-only by DEFAULT
    ///
    /// Lending the engine the provider's full resolver fixes that, and costs too much to be the
    /// default. The walk is parallel, and every JDK / dependency lookup funnels through
    /// `JdkMemberIndex`'s single mutex — the JDK-8 `JarSource` is `!Sync`, so serializing is not
    /// an implementation detail to tune away. Turning it on made a 700-file project's index take
    /// far longer with every core busy, and until that walk finishes there is no rename engine at
    /// all: every name comes back "cannot be renamed".
    ///
    /// So the trade is explicit. Off: indexing is fast, and a rename misses call sites whose
    /// receiver is typed only through a library generic. On (`BENNU_RENAME_FULL_RESOLVER=1`): those
    /// call sites are found, and the first walk pays for decoding the classpath.
    ///
    /// `jdk_version` is read as the project's Java **language level**, to gate version-specific
    /// binding forms (records, pattern variables, lambda inferred params) during classification.
    pub fn for_project(
        index_dir: &Path,
        jdk_version: &str,
        project_simple_names: &[(String, String)],
        java_sources: Vec<(String, String)>,
        xml_sources: Vec<(String, String)>,
        shared_resolver: Option<Arc<dyn TypeResolver + Send + Sync>>,
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Result<Self, String> {
        let mut project_types = HashMap::new();
        for (simple, binary) in project_simple_names {
            project_types.insert(simple.clone(), binary.clone());
        }
        // Whether to accept the loan is the CALLER's decision — it knows what the walk will cost on
        // this machine and this project. Here we only honour it.
        let resolver: Arc<dyn TypeResolver + Send + Sync> = match shared_resolver {
            Some(shared) => shared,
            None => Arc::new(project_only_resolver(index_dir, project_simple_names)?),
        };

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
            build_reference_index_incremental(&ref_input, &*resolver, &project_types, prior, on_progress);
        let index = built.index;
        if let (Some(path), Some(cache)) = (&cache_path, &built.cache_to_save) {
            crate::refcache::save(path, cache);
        }

        let java_files =
            java_sources.into_iter().map(|(path, source)| PlanFile { path, source }).collect();
        let xml_files =
            xml_sources.into_iter().map(|(path, source)| PlanFile { path, source }).collect();
        // Same view for both until the caller lends a fuller one (`with_policy_resolver`).
        let policy = Arc::clone(&resolver);
        Ok(Self {
            index,
            resolver,
            policy,
            project_types,
            java_files,
            xml_files,
            lang_level: LangLevel::from_version(jdk_version),
        })
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
            &*self.resolver,
            &*self.policy,
            &self.project_types,
            &self.java_files,
            &self.xml_files,
            self.lang_level,
        )
    }

    /// The binary name of the **type** declared at `file`:`offset`, if the caret is on one.
    ///
    /// The classification half of a rename, on its own. It costs an index + resolver lookup and
    /// touches no project sources, which is what lets a batch caller resolve a hundred carets and
    /// then plan them all in one pass — see [`plan_types`].
    pub fn classify_type(&self, file: &str, source: &str, offset: usize) -> Option<String> {
        let target = classify_target(
            &self.index,
            file,
            source,
            offset,
            &*self.resolver,
            &self.project_types,
            self.lang_level,
        )?;
        match target {
            RenameTarget::Type { binary, .. } => Some(binary),
            _ => None,
        }
    }

    /// Plan several **type** renames in one pass over the project's sources — see [`plan_types`].
    ///
    /// The batch entry point exists because the per-type cost is a pass over every project file,
    /// so a bulk fix that calls [`plan`](Self::plan) once per type pays that pass once per type.
    /// Returns one bucket of edits per input rename, in order, plus whether the pass **completed**
    /// — see [`plan_types`], whose `on_file` this forwards.
    pub fn plan_types(
        &self,
        renames: &[TypeRename],
        on_file: &dyn Fn(usize, usize) -> bool,
    ) -> (Vec<Vec<Edit>>, bool) {
        plan_types(
            &self.index,
            renames,
            &self.java_files,
            &self.xml_files,
            &self.project_types,
            on_file,
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
            &*self.resolver,
            &self.project_types,
            &self.java_files,
            self.lang_level,
        )
    }

    /// Find all usages of the symbol at `file`:`offset` (byte offset), for find-usages.
    /// Shares the engine's reference index + resolver with rename (same off-thread build).
    /// `source` is the current (possibly-unsaved) buffer. `None` when the caret isn't on a
    /// referenceable symbol (a local/param is scope-exact and not bucketed here).
    pub fn find_usages(&self, file: &str, source: &str, offset: usize) -> Option<ReferencesResult> {
        references(&self.index, file, source, offset, &*self.resolver, &self.project_types)
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
    ) -> Vec<InheritedMember> {
        inherited_members(&*self.resolver, &self.java_files, file, type_name, line)
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
            &*self.resolver,
            &self.project_types,
        )?;
        let mut info = hover_for_key(&key, &*self.resolver);
        // Best-effort: attach the leading Javadoc of the PROJECT declaration this key
        // resolves to (None for a classpath-only / JDK symbol we can't read the source of).
        info.doc = self.project_doc_for_key(&key);
        Some(info)
    }

    /// Extract the leading Javadoc (`/** … */`) of the project declaration `key` names. `None` when
    /// the declaration isn't in a project source (a JDK / dep-jar symbol) or carries no Javadoc.
    /// Best-effort — a parse/lookup miss just yields `None`.
    ///
    /// It asks the reference index which file declares the owning type and parses **that one**.
    /// The previous version walked every `.java` in the project, running a full tree-sitter parse
    /// per file — and, when the declaration it found carried no Javadoc (the common case in a
    /// legacy codebase), kept going through the rest anyway. On a 1300-file project that is 1300
    /// parses for one tooltip, which is why hovering a method took an age while hovering a local
    /// variable (one parse, on the fallback path) was instant.
    fn project_doc_for_key(&self, key: &DeclKey) -> Option<String> {
        let file = self.index.file_declaring(key.owner_binary())?;
        let source = project_source(&self.java_files, file)?;
        let decl_start = decl_site_for_key(source, key)?;
        leading_javadoc(source, decl_start)
    }
}

/// The byte offset where the *declaration* of `key` begins in `source` (the start of the
/// `class`/`interface`/`enum`/method/field declaration node, NOT just its name token — so
/// a preceding Javadoc comment can be found immediately above it). `None` when `source`
/// doesn't declare `key`.
fn decl_site_for_key(source: &str, key: &DeclKey) -> Option<usize> {
    let tree = bennu_java::prelude::parse_java(source)?;
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
        DeclKey::Type { binary } => {
            // What the type IS, not "class" for everything — an interface reported as a class
            // is the card stating something false about the thing you are pointing at. The
            // signature reads like the declaration; the package goes on the meta line.
            let simple = simple_of(binary).replace('$', ".");
            let kind = resolver
                .members_of(binary)
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
                .unwrap_or("class");
            let dotted = binary.replace('/', ".");
            let package = dotted.rsplit_once('.').map(|(p, _)| p.to_string());
            HoverInfo {
                signature: format!("{kind} {simple}"),
                kind: kind.to_string(),
                container: package,
                doc: None,
            }
        }
        DeclKey::Method { owner, name } => {
            let found = member_signature(resolver, owner, name, true);
            let (signature, declaring) = found.unwrap_or_else(|| (format!("{name}(…)"), owner.clone()));
            HoverInfo {
                signature,
                kind: "method".to_string(),
                // The type that DECLARES it, not the one it was reached through: hovering an
                // inherited method and being told the subclass owns it is a wrong answer.
                container: Some(declaring.replace('/', ".")),
                doc: None,
            }
        }
        DeclKey::Field { owner, name } => {
            let found = member_signature(resolver, owner, name, false);
            let (signature, declaring) = found.unwrap_or_else(|| (name.clone(), owner.clone()));
            HoverInfo {
                signature,
                kind: "field".to_string(),
                container: Some(declaring.replace('/', ".")),
                doc: None,
            }
        }
    }
}

/// Look up a member's `raw_signature` on `owner` (walking supertypes, like the reference
/// walk's `declaring_owner`), with the binary name of the type that actually declares it.
/// `None` when the class isn't resolvable or the member is nowhere in the hierarchy (the
/// caller then synthesizes a fallback).
fn member_signature(
    resolver: &dyn TypeResolver,
    owner: &str,
    name: &str,
    is_method: bool,
) -> Option<(String, String)> {
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![owner.to_string()];
    while let Some(bn) = stack.pop() {
        if !visited.insert(bn.clone()) {
            continue;
        }
        // A supertype we can't resolve ends THAT branch of the walk, not the whole search —
        // an un-indexed base class must not hide a member the subclass declares itself.
        let Some(cm) = resolver.members_of(&bn) else { continue };
        let pool = if is_method { &cm.methods } else { &cm.fields };
        if let Some(m) = pool.iter().find(|m| m.name == name) {
            if !m.raw_signature.is_empty() {
                return Some((m.raw_signature.clone(), bn.clone()));
            }
            // No recorded signature: synthesize a minimal one from the name (+ empty
            // param list for a method) so the hover still shows something meaningful.
            let sig = if is_method { format!("{name}()") } else { name.to_string() };
            return Some((sig, bn.clone()));
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
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
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
        if n.kind() == "identifier"
            && !crate::refs::is_member_selector_node(&n)
            && !is_callable_own_name(&n)
        {
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
/// Whether `node` is the **name** of the method or constructor it belongs to.
///
/// Now that a parameter's scope is the whole declaration rather than its body, the walk passes over
/// the callable's own name. Java allows `void foo(int foo)`, and renaming the parameter there must
/// not rename the method with it.
fn is_callable_own_name(node: &Node) -> bool {
    node.parent()
        .filter(|p| matches!(p.kind(), "method_declaration" | "constructor_declaration"))
        .and_then(|p| p.child_by_field_name("name"))
        .map(|name| name.id() == node.id())
        .unwrap_or(false)
}

fn enclosing_scope<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            // The WHOLE declaration, header included — not just the body.
            //
            // A parameter is declared in the header, so a scope that starts at the body renames
            // every use of it and leaves `final Path source_directory` untouched: uses rewritten,
            // declaration not, which does not compile. A local declared inside the body is
            // unaffected by the wider scope — Java forbids one from shadowing a parameter, so the
            // extra ground holds no other binding of the same name.
            "method_declaration" | "constructor_declaration" => return Some(n),
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

    // Every declaration of the name in the owner — overloads are several declarations of one key.
    for (ds, de) in find_member_name_spans(decl_source, key) {
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

    // Accessors nobody wrote down, but callers use — see [`generated_accessors`].
    if let DeclKey::Field { owner, .. } = key {
        for accessor in generated_accessors(decl_source, owner, &name, new_name) {
            let old = member_name(&accessor.key);
            for u in index.usages_of(&accessor.key) {
                edits.push(Edit {
                    file: u.file.clone(),
                    start: u.start,
                    end: u.end,
                    new_text: accessor.new_name.clone(),
                    old: old.clone(),
                    reason: EditReason::Reference,
                    inferred: false,
                });
            }
        }
    }
    edits
}

/// A generated accessor to carry along with the field it belongs to: its key in the reference
/// index, and what it must be called after the rename.
struct GeneratedAccessor {
    key: DeclKey,
    new_name: String,
}

/// The accessors that exist for `field` without anyone having written them, whose call sites a
/// rename of the field must therefore carry: a **record component's** accessor (JLS §8.10 — a
/// component declares a private final field AND a public accessor of the same name) and **Lombok's**
/// generated getter / setter / wither.
///
/// Nothing else in the plan would move them. The declaration half of a rename edits source text,
/// and for these there is no source text to edit — but `f.source_path()` and `order.getId()` are
/// written down at every call site, and they read a method that will no longer exist.
///
/// The old and new names coincide for a record component and differ for Lombok
/// (`getSource_path` → `getSourcePath`), which is why the new name comes from re-running Lombok's
/// own naming rule ([`crate::lombok::PlannedAccessor::name_for`]) rather than from a second
/// implementation of it here.
///
/// Both halves are gated on the declaring source actually declaring the thing: an ordinary class
/// may hold a field `foo` and an unrelated method `foo()`, and renaming one must never take the
/// other with it.
fn generated_accessors(
    decl_source: &str,
    owner: &str,
    field: &str,
    new_name: &str,
) -> Vec<GeneratedAccessor> {
    let mut out = Vec::new();
    if declares_record_component(decl_source, owner, field) {
        out.push(GeneratedAccessor {
            key: DeclKey::Method { owner: owner.to_string(), name: field.to_string() },
            new_name: new_name.to_string(),
        });
    }
    let symbols = bennu_java::prelude::extract_symbols(decl_source);
    if let Some(td) = symbols.types.iter().find(|t| t.fqn.replace('.', "/") == owner) {
        for acc in crate::lombok::accessors_of_field(td, &symbols.imports, field) {
            out.push(GeneratedAccessor {
                key: DeclKey::Method { owner: owner.to_string(), name: acc.name.clone() },
                new_name: acc.name_for(new_name),
            });
        }
    }
    out
}

/// Whether `source` declares `name` as a component of the record whose binary name is `owner`.
fn declares_record_component(source: &str, owner: &str, name: &str) -> bool {
    let simple = simple_of(owner);
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return false };
    let bytes = source.as_bytes();

    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if n.kind() != "record_declaration" {
            continue;
        }
        if n.child_by_field_name("name").and_then(|nm| nm.utf8_text(bytes).ok()) != Some(&simple) {
            continue;
        }
        let Some(params) = n.child_by_field_name("parameters") else { continue };
        let mut pc = params.walk();
        for p in params.named_children(&mut pc) {
            let component = p.child_by_field_name("name").and_then(|nm| nm.utf8_text(bytes).ok());
            if component == Some(name) {
                return true;
            }
        }
    }
    false
}

/// Whether this `formal_parameter` is a record component rather than a method parameter — the
/// grammar spells both the same way, and only the grandparent tells them apart.
fn is_record_component(node: &Node) -> bool {
    node.parent()
        .and_then(|params| params.parent())
        .map(|owner| owner.kind() == "record_declaration")
        .unwrap_or(false)
}

/// Find the byte span of a member declaration's NAME token in `source`. Shared by rename
/// (the declaration edit site) and go-to-declaration (the navigation target). `None` for a
/// [`DeclKey::Type`] (use [`bennu_java::prelude::find_type_name_span`]) or when `source` doesn't
/// declare the member.
pub fn find_member_name_span(source: &str, key: &DeclKey) -> Option<(usize, usize)> {
    find_member_name_spans(source, key).into_iter().next()
}

/// Every byte span in `source` where this member's name is DECLARED.
///
/// More than one for overloads: `foo(int)` and `foo(String)` are two declarations of one name, and
/// the reference index collapses them to a single key. Renaming from that key has to move both —
/// stopping at the first (which is what returning a single span meant) renamed one overload and
/// left the other declaring the old name, silently splitting one method into two.
pub fn find_member_name_spans(source: &str, key: &DeclKey) -> Vec<(usize, usize)> {
    let (name, want_field) = match key {
        DeclKey::Method { name, .. } => (name, false),
        DeclKey::Field { name, .. } => (name, true),
        DeclKey::Type { .. } => return Vec::new(),
    };
    let owner_simple = simple_of(key.owner_binary());
    // A declaration's name appears textually in the file that declares it — skip the
    // tree-sitter parse when the token isn't even present (a cheap early-out for callers that
    // probe more than one file, e.g. rename's edit-site search). A substring false-positive
    // just parses one file that then yields no match — correct, only slightly slower.
    if !source.contains(name.as_str()) {
        return Vec::new();
    }
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let mut found: Vec<(usize, usize)> = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        let hit = match n.kind() {
            "method_declaration" if !want_field => n.child_by_field_name("name"),
            // An `@interface` element (`boolean withCheck() default false;`) is a method — that is
            // what it compiles to and how the index records it. Its declaration lives under its own
            // node kind, so without this arm the member had uses and no declaration, and the rename
            // refused itself rather than edit half a pair.
            "annotation_type_element_declaration" if !want_field => n.child_by_field_name("name"),
            "variable_declarator" if want_field => {
                // A declarator under a `field_declaration` (a class field) OR a
                // `constant_declaration` (an interface's `int MAX = …;`) is the field decl.
                let is_field = n
                    .parent()
                    .map(|p| matches!(p.kind(), "field_declaration" | "constant_declaration"))
                    .unwrap_or(false);
                if is_field { n.child_by_field_name("name") } else { None }
            }
            // A record component IS the field's declaration — the JLS says the header declares a
            // `private final` field, and there is nowhere else in the source it is written.
            "formal_parameter" if want_field && is_record_component(&n) => {
                n.child_by_field_name("name")
            }
            _ => None,
        };
        if let Some(nm) = hit {
            if nm.utf8_text(bytes).ok() == Some(name.as_str())
                && declared_in_type(&n, bytes, &owner_simple)
            {
                found.push((nm.start_byte(), nm.end_byte()));
            }
        }
    }
    // The walk is a stack, so it arrives in no useful order; source order is what a preview wants.
    found.sort_unstable();
    found
}

/// Whether the declaration at `node` sits inside the type named `owner_simple`.
///
/// One file can declare several types — a nested class, a second top-level one — and any of them
/// may hold a member of the same name. Without this the span search took whichever the tree walk
/// reached first, so a rename could edit an unrelated member of an unrelated type.
///
/// A declaration with no enclosing named type is accepted: an anonymous class body or a shape the
/// grammar spells differently should not silently lose its declaration edit, and the caller is
/// already looking in the file that declares the owner.
fn declared_in_type(node: &Node, bytes: &[u8], owner_simple: &str) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        // An anonymous body is the enclosing type, and the first one going up. Climbing past it
        // would compare the member against the name of the class the `new` sits in — which never
        // matches the anonymous owner's synthetic name, so no declaration span was ever found for
        // a member of one.
        if bennu_java::prelude::is_anonymous_body(&n) {
            return bennu_java::prelude::anonymous_type_name(&n, bytes).as_deref()
                == Some(owner_simple);
        }
        if matches!(
            n.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            return match n.child_by_field_name("name").and_then(|nm| nm.utf8_text(bytes).ok()) {
                Some(found) => found == owner_simple,
                None => true,
            };
        }
        cur = n.parent();
    }
    true
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

/// One type rename in a batch — see [`plan_types`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRename {
    /// The type's binary name (`com/acme/OrderService`).
    pub binary: String,
    /// The new SIMPLE name. The package is kept.
    pub new_name: String,
}

/// Plan several type renames in **one pass** over the project's sources.
///
/// The declaration name and the `import` statements can only be found by reading every file, and
/// reading means parsing. Done one rename at a time that is `renames × files` parses — which is
/// invisible for a single Shift+F6 and is minutes on a bulk fix over a legacy tree, with the
/// caller's request blocked for all of it. Here each file is parsed **once** and matched against
/// every rename in the batch, so the cost is `files`, whatever the batch holds.
///
/// Returns one bucket of edits per input rename, in the same order, so a caller that has to
/// attribute an edit back to the name that caused it still can.
///
/// `on_file(done, total)` is called as the pass advances, and **returning `false` stops it**.
///
/// Both halves of that exist for the same reason: this is the slow step, and from the outside it is
/// one indivisible call. Without progress, a caller can only report "started" and "finished" — on a
/// large project that is a full progress bar sitting still, indistinguishable from a hang. Without
/// the stop, a Cancel button elsewhere does nothing at all for the whole time this runs, which is
/// worse than not offering one.
///
/// A stopped pass returns `(buckets, false)`. Those buckets are **incomplete and must not be
/// applied**: the reference edits come from the index but the declaration and `import` edits come
/// from this walk, so a half-done pass renames the call sites of a type and leaves its declaration
/// alone. The caller is told so it can discard them, not so it can use what it got.
pub fn plan_types(
    index: &ReferenceIndex,
    renames: &[TypeRename],
    java_files: &[PlanFile],
    xml_files: &[PlanFile],
    project_types: &HashMap<String, String>,
    on_file: &dyn Fn(usize, usize) -> bool,
) -> (Vec<Vec<Edit>>, bool) {
    let mut out: Vec<Vec<Edit>> = vec![Vec::new(); renames.len()];
    if renames.is_empty() {
        return (out, true);
    }
    let targets: Vec<TypeTarget> = renames.iter().map(TypeTarget::of).collect();

    // (1) simple-name use sites (the reference index buckets these under DeclKey::Type; the
    // builder EXCLUDES the declaration name, added separately in (2)). An index lookup per
    // rename — cheap, and unrelated to how many files the project has.
    for (i, target) in targets.iter().enumerate() {
        for u in index.usages_of(&DeclKey::Type { binary: target.binary.clone() }) {
            out[i].push(Edit {
                file: u.file.clone(),
                start: u.start,
                end: u.end,
                new_text: target.new_name.clone(),
                old: target.old_simple.clone(),
                reason: EditReason::Reference,
                inferred: false,
            });
        }
    }

    // (2) the declaration name + import statements. ONE parse per file, matched against the
    // whole batch — this is the loop the batching exists for, and the one worth reporting on.
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_ok() {
        let total = java_files.len();
        for (done, f) in java_files.iter().enumerate() {
            if !on_file(done, total) {
                return (out, false);
            }
            collect_type_decls_and_imports(
                &mut parser,
                &f.source,
                &f.path,
                &targets,
                project_types,
                &mut out,
            );
        }
        on_file(total, total);
    }

    // (3) Spring bean XML `<bean class="oldFQCN">` → rewrite the FQCN (package kept). A string
    // scan over a handful of config files, so it stays per-rename.
    for f in xml_files {
        for (i, target) in targets.iter().enumerate() {
            for span in bean_class_value_spans(&f.source, &target.old_fqcn) {
                out[i].push(Edit {
                    file: f.path.clone(),
                    start: span.start,
                    end: span.end,
                    new_text: target.new_fqcn.clone(),
                    old: target.old_fqcn.clone(),
                    reason: EditReason::SpringBean,
                    inferred: false,
                });
            }
        }
    }

    // One byte range, one edit. The references come from the index and the declaration from the
    // walk above, and the two agree only as long as the index correctly excludes declaration names
    // from the usages it buckets. That is a property of another module, and if it ever slips the
    // failure here is two splices over the same bytes — corruption, not a missing rename. Deduping
    // costs a sort per bucket and removes the coupling.
    // Ordered so the DECLARATION sorts first within an identical range, because `dedup_by` keeps
    // the first: the two candidates replace the same bytes with the same text, but a caller may
    // well check that a plan contains a declaration edit before trusting it, and dropping that one
    // in favour of a reference would fail such a check on a plan that is in fact complete.
    for bucket in &mut out {
        bucket.sort_by(|a, b| {
            (a.file.as_str(), a.start, a.end, reason_rank(&a.reason))
                .cmp(&(b.file.as_str(), b.start, b.end, reason_rank(&b.reason)))
        });
        bucket.dedup_by(|a, b| a.file == b.file && a.start == b.start && a.end == b.end);
    }

    (out, true)
}

/// Every PROJECT type whose declaration of `name` must move when `owner`'s does — `owner` itself,
/// the supertypes it overrides the method from, and every subtype that overrides it.
///
/// A method that exists at several levels of a hierarchy is ONE method to every caller: renaming
/// `Base.doWork` while leaving `Impl.doWork` behind doesn't rename anything, it silently turns an
/// override into an unrelated method and an `@Override` into a compile error. And because the
/// reference index keys a call by the owner its receiver resolved to, calls made through an
/// `Impl`-typed variable live under `Impl`'s key — so missing the subtype loses ordinary-looking
/// call sites too, which is what "some usages were renamed and some weren't" looks like from
/// outside.
///
/// Only project types: library source can't be edited, and a family rooted at a JDK type
/// (everything "declares" `toString`) would drag in every unrelated class in the project.
fn override_family(
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
    owner: &str,
    name: &str,
) -> Vec<String> {
    // The topmost project types declaring `name` above `owner` — the roots the family hangs from.
    let mut roots: Vec<String> = vec![owner.to_string()];
    for anc in project_ancestors(resolver, owner) {
        if declares_method(resolver, &anc, name) {
            roots.push(anc);
        }
    }

    let mut family: Vec<String> = Vec::new();
    for r in &roots {
        if !family.contains(r) {
            family.push(r.clone());
        }
    }

    // Anything below a root that declares the same name is an override of it.
    let mut candidates: Vec<&String> = project_types.values().collect();
    candidates.sort_unstable();
    candidates.dedup();
    for cand in candidates {
        if family.contains(cand) || !declares_method(resolver, cand, name) {
            continue;
        }
        let ancestry = project_ancestors(resolver, cand);
        if roots.iter().any(|r| ancestry.contains(r)) {
            family.push(cand.clone());
        }
    }
    family
}

/// The LIBRARY type whose method `name` this owner overrides, if any.
///
/// A method that implements a library interface or overrides a library base class is not free to be
/// renamed: the name IS the contract, and the jar cannot be edited to follow. Renaming it produces
/// a class that no longer implements what it claims to — `@Override` stops overriding and the file
/// stops compiling — which is exactly the outcome a rename must never quietly produce.
///
/// Matched by NAME, like everything else in this engine (overloads collapse to one key). That can
/// name a library supertype whose same-named method takes different parameters and is therefore
/// not really overridden — refusing there costs a rename that would have been safe, which is the
/// cheaper of the two mistakes.
fn library_override(resolver: &dyn TypeResolver, owner: &str, name: &str) -> Option<String> {
    all_ancestors(resolver, owner)
        .into_iter()
        .find(|a| !resolver.is_project_type(a) && declares_method(resolver, a, name))
}

/// Whether `binary` declares a method called `name` **itself** (not inherited).
fn declares_method(resolver: &dyn TypeResolver, binary: &str, name: &str) -> bool {
    resolver
        .members_of(binary)
        .map(|cm| cm.methods.iter().any(|m| m.name == name))
        .unwrap_or(false)
}

/// The PROJECT supertypes of `binary`, superclass and interfaces. Library supertypes end the walk
/// on that branch — nothing above them can be edited, so nothing above them joins a rename.
fn project_ancestors(resolver: &dyn TypeResolver, binary: &str) -> Vec<String> {
    ancestors(resolver, binary, true)
}

/// Every supertype of `binary`, project and library alike — for deciding whether a rename is
/// allowed at all, which depends on supertypes that cannot be edited.
fn all_ancestors(resolver: &dyn TypeResolver, binary: &str) -> Vec<String> {
    ancestors(resolver, binary, false)
}

/// Supertypes of `binary`, superclass and interfaces. With `project_only`, a library supertype is
/// neither returned nor walked through.
fn ancestors(resolver: &dyn TypeResolver, binary: &str, project_only: bool) -> Vec<String> {
    /// A hierarchy this deep is a cycle in a malformed index, not a real one.
    const MAX_DEPTH: usize = 64;
    let mut out: Vec<String> = Vec::new();
    let mut queue: Vec<String> = vec![binary.to_string()];
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(binary.to_string());
    let mut steps = 0usize;
    while let Some(next) = queue.pop() {
        steps += 1;
        if steps > MAX_DEPTH {
            break;
        }
        let Some(cm) = resolver.members_of(&next) else { continue };
        let supers = cm.superclass.iter().cloned().chain(cm.interfaces.iter().cloned());
        for s in supers {
            if (project_only && !resolver.is_project_type(&s)) || !seen.insert(s.clone()) {
                continue;
            }
            out.push(s.clone());
            queue.push(s);
        }
    }
    out
}

/// Sort order for two edits over the same bytes: the declaration wins.
fn reason_rank(reason: &EditReason) -> u8 {
    match reason {
        EditReason::Declaration => 0,
        _ => 1,
    }
}

/// A [`TypeRename`] with the spellings the walk compares against precomputed.
struct TypeTarget {
    binary: String,
    old_simple: String,
    old_fqcn: String,
    new_name: String,
    new_fqcn: String,
}

impl TypeTarget {
    fn of(rename: &TypeRename) -> TypeTarget {
        let old_fqcn = rename.binary.replace('/', ".");
        TypeTarget {
            binary: rename.binary.clone(),
            old_simple: simple_of(&rename.binary),
            new_fqcn: replace_simple(&old_fqcn, &rename.new_name),
            old_fqcn,
            new_name: rename.new_name.clone(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_type(
    index: &ReferenceIndex,
    binary: &str,
    _old_simple: &str,
    new_name: &str,
    java_files: &[PlanFile],
    xml_files: &[PlanFile],
    project_types: &HashMap<String, String>,
) -> Vec<Edit> {
    // A batch of one. Keeping a second implementation for the single case is how the two would
    // eventually disagree about what renaming a type means.
    let renames =
        [TypeRename { binary: binary.to_string(), new_name: new_name.to_string() }];
    // One rename, one pass, nothing to report and nothing to stop.
    let (buckets, _) =
        plan_types(index, &renames, java_files, xml_files, project_types, &|_, _| true);
    buckets.into_iter().next().unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
/// One file, every rename in the batch. Parses once and pushes into each rename's bucket.
fn collect_type_decls_and_imports(
    parser: &mut Parser,
    source: &str,
    path: &str,
    targets: &[TypeTarget],
    project_types: &HashMap<String, String>,
    out: &mut [Vec<Edit>],
) {
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
            // Every kind that declares a type. `record_declaration` and
            // `annotation_type_declaration` were missing here, and the failure mode is the worst
            // one a rename has: the *uses* come from the reference index and were all rewritten,
            // while the declaration — which only this walk can find — was left alone. The result
            // is code that no longer compiles, produced by a refactor that reported success.
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => {
                let Some(nm) = n.child_by_field_name("name") else { continue };
                let Ok(text) = nm.utf8_text(bytes) else { continue };
                for (i, target) in targets.iter().enumerate() {
                    if text != target.old_simple {
                        continue;
                    }
                    // Confirm this really is the target type (same binary) so a same-named
                    // class in another package isn't hit.
                    let is_target = project_types
                        .get(&target.old_simple)
                        .map(|b| *b == target.binary)
                        .unwrap_or(true);
                    if is_target {
                        out[i].push(Edit {
                            file: path.to_string(),
                            start: nm.start_byte(),
                            end: nm.end_byte(),
                            new_text: target.new_name.clone(),
                            old: target.old_simple.clone(),
                            reason: EditReason::Declaration,
                            inferred: false,
                        });
                    }
                }
            }
            "import_declaration" => {
                let Some(pn) = n
                    .named_children(&mut n.walk())
                    .find(|c| matches!(c.kind(), "scoped_identifier" | "identifier"))
                else {
                    continue;
                };
                let Ok(text) = pn.utf8_text(bytes) else { continue };
                for (i, target) in targets.iter().enumerate() {
                    if text != target.old_fqcn {
                        continue;
                    }
                    // Replace only the trailing simple name (after the final `.`).
                    let path_end = pn.end_byte();
                    let simple_start = path_end - target.old_simple.len();
                    out[i].push(Edit {
                        file: path.to_string(),
                        start: simple_start,
                        end: path_end,
                        new_text: target.new_name.clone(),
                        old: target.old_simple.clone(),
                        reason: EditReason::Import,
                        inferred: false,
                    });
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
                        is_abstract: false,
                        is_default: false,
                        is_final: m.is_final,
                        visibility: Visibility::Public,
                        raw_signature: String::new(),
                        throws: Vec::new(),
                    })
                    .collect();
                project.insert(
                    binary,
                    ClassMembers {
                        type_params: Vec::new(),
                        superclass: None,
                        interfaces: vec![],
                        methods,
                        fields: vec![],
                        flags: Default::default(),
                    },
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
            &resolver,
            &project_types,
            &java_files,
            &xml_files,
            LangLevel(0),
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
        resolve_declaration(
            &index,
            target_file,
            source,
            offset,
            &resolver,
            &project_types,
            &java_files,
            LangLevel(0),
        )
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

    /// A record and a second file that reads it through the accessor the compiler generates.
    const RECORD_FILES: [(&str, &str); 2] = [
        (
            "Failure.java",
            "package p; public record Failure(String source_path, int code) {}",
        ),
        (
            "Report.java",
            "package p; public class Report { String show(Failure f) { return f.source_path(); } }",
        ),
    ];

    #[test]
    fn renaming_a_record_component_renames_its_accessor_uses() {
        // A record component is not a local: the JLS says it declares a private final field AND a
        // public accessor of the same name, and callers use the accessor (`f.source_path()`).
        // Classified as a local, the rename walked the record's own scope, renamed the component,
        // and left every caller reading a method that no longer exists.
        let src = RECORD_FILES[0].1;
        let off = src.find("String source_path").unwrap() + "String ".len();
        let p = plan(&RECORD_FILES, &[], "Failure.java", off, "sourcePath").expect("classified");

        let declarations: Vec<&Edit> =
            p.files.iter().flat_map(|f| &f.edits).filter(|e| e.reason == EditReason::Declaration).collect();
        assert_eq!(declarations.len(), 1, "the component itself: {:?}", p.files);
        assert_eq!(
            &src[declarations[0].start..declarations[0].end],
            "source_path",
            "the declaration edit must be the component in the record header"
        );

        // …and the accessor call in the other file.
        let report = p.files.iter().find(|f| f.file == "Report.java");
        let report = report.unwrap_or_else(|| panic!("the accessor's caller must be edited: {:?}", p.files));
        assert_eq!(report.edits.len(), 1, "{:?}", report.edits);
        assert_eq!(
            &RECORD_FILES[1].1[report.edits[0].start..report.edits[0].end],
            "source_path",
        );
    }

    #[test]
    fn renaming_a_nested_record_component_renames_its_accessor_uses() {
        // The shape real code has: the record is declared INSIDE another class, which is where the
        // binary name stops being `p/Failure` and becomes `p/Outer/Failure`. Two bugs today came
        // from exactly that difference, so the top-level case passing proves less than it looks.
        let files = [(
            "Outer.java",
            "package p; public class Outer { \
             private record Failure(String source_path) {} \
             String show(Failure f) { return f.source_path(); } }",
        )];
        let src = files[0].1;
        let off = src.find("String source_path)").unwrap() + "String ".len();
        let p = plan(&files, &[], "Outer.java", off, "sourcePath").expect("classified");

        let edits = &p.files[0].edits;
        assert_eq!(edits.len(), 2, "the component and the accessor call: {edits:?}");
        assert!(
            edits.iter().any(|e| e.reason == EditReason::Declaration),
            "the component's own declaration: {edits:?}"
        );
        let accessor = src.find("f.source_path()").unwrap() + "f.".len();
        assert!(
            edits.iter().any(|e| e.start == accessor),
            "the accessor call must be renamed: {edits:?}"
        );
    }

    #[test]
    fn a_record_component_is_not_classified_as_a_local() {
        // The classification itself is the thing under test: a local rename is scope-exact and is
        // applied without a preview, so mis-classifying a component is what made an incomplete
        // rename look safe.
        let src = RECORD_FILES[0].1;
        let off = src.find("String source_path").unwrap() + "String ".len();
        let (resolver, project_types) = build_resolver(&RECORD_FILES);
        let files: Vec<SourceFile> = RECORD_FILES
            .iter()
            .map(|(p, s)| SourceFile { path: p.to_string(), source: s.to_string() })
            .collect();
        let index = build_reference_index(&files, &resolver, &project_types);
        let target = crate::refs::classify_target(
            &index,
            "Failure.java",
            src,
            off,
            &resolver,
            &project_types,
            LangLevel(0),
        )
        .expect("classified");
        assert!(
            !matches!(target, crate::refs::RenameTarget::Local { .. }),
            "a record component must classify as a member, not a local: {target:?}"
        );
    }

    #[test]
    fn parameter_rename_includes_its_own_declaration() {
        // The regression this pins: a parameter is declared in the method HEADER, and the scope
        // walk used to start at the body — so every use was rewritten and `int source_count` was
        // left as it was. A rename that does that does not compile.
        let src = "package p; public class C { int f(int source_count) { return source_count + source_count; } }";
        let files = [("C.java", src)];
        let off = src.find("int source_count)").unwrap() + "int ".len();
        let p = plan(&files, &[], "C.java", off, "sourceCount").expect("classified");
        assert_eq!(p.total_edits(), 3, "declaration + two uses");
        let declarations: Vec<&Edit> = p.files[0]
            .edits
            .iter()
            .filter(|e| e.reason == EditReason::Declaration)
            .collect();
        assert_eq!(declarations.len(), 1, "exactly one declaration edit: {:?}", p.files[0].edits);
        // …and it is the one in the header, before the body opens.
        let body = src.find('{').and_then(|_| src.find(") {")).expect("body");
        assert!(declarations[0].start < body, "the declaration edit must be the header's");
    }

    #[test]
    fn renaming_a_parameter_leaves_a_method_of_the_same_name_alone() {
        // Java allows `void foo(int foo)`. Widening the scope to the whole declaration put the
        // method's own name in range, and renaming the parameter must not take it with it.
        let src = "package p; public class C { void foo(int foo) { System.out.println(foo); } }";
        let files = [("C.java", src)];
        let off = src.find("int foo)").unwrap() + "int ".len();
        let p = plan(&files, &[], "C.java", off, "count").expect("classified");
        assert_eq!(p.total_edits(), 2, "the parameter and its one use: {:?}", p.files[0].edits);
        let method_name = src.find("void foo(").unwrap() + "void ".len();
        assert!(
            p.files[0].edits.iter().all(|e| e.start != method_name),
            "the method's own name must not be renamed"
        );
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
