//! The project's **semantic engine** — everything the editor asks about Java that needs the whole
//! project rather than the file in front of you.
//!
//! It was called `RenameEngine` and lived in `rename.rs`, which was true when rename was the only
//! thing that needed a whole-project reference index. Find-usages needed the same index, then
//! go-to, hover, inherited members and the hierarchies; each was added where the index already was,
//! and the name went on describing one of its seven answers. A name that has to be explained is a
//! name that will mislead someone, and there is nothing here a rename owns.
//!
//! What it holds is the answer to "what does this project mean": the reference index, the sources
//! those offsets refer to, the simple→binary type map and the subtype map — all four behind one
//! lock, because an edit invalidates them together (see [`Live`]) — plus the resolvers and the
//! language level.
//!
//! Everything on it is a **query**. The work of answering lives in the modules that own each
//! question (`rename`, `refs`, `hierarchy`, `bennu-query`); this type is where they all get the
//! same consistent view of the project to answer from, from one read of the lock.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, RwLock};

use bennu_java::prelude::TypeResolver;
use bennu_query::prelude::{
    inherited_members, IndexResolver, InheritedMember, JdkMemberIndex, PlanFile,
};

use crate::hierarchy::{HierarchyCtx, HierarchyDirection, HierarchyHandle, HierarchyItem};
use crate::refs::{
    build_reference_index_incremental, build_reference_index_with_progress, classify_caret,
    classify_target, references, AliasUsages, DeclKey, LangLevel, ReferenceIndex, ReferencesResult,
    RenameTarget, SourceFile,
};
use crate::rename::{
    decl_site_for_key, generated_aliases, hover_for_key, leading_javadoc, plan_types,
    project_source, rename_plan, resolve_declaration, DeclarationLocation, Edit, HoverInfo,
    RenamePlan, SubtypeMap, TypeRename,
};

/// A ready-to-query semantic model of one project, built once (on the index background thread,
/// alongside the completion provider) and cached behind an `Arc` in the be layer.
///
/// `Send + Sync` so it lives in the shared project slot across the dispatcher. The resolver is
/// type-erased behind an `Arc` because in production it is the **provider's own** fully-resolving
/// one, shared rather than duplicated (see [`for_project`](Self::for_project)).
pub struct SemanticEngine {
    /// What an edit changes — see [`Live`].
    live: RwLock<Live>,
    resolver: Arc<dyn TypeResolver + Send + Sync>,
    /// The resolver POLICY questions are asked of — see [`SemanticEngine::with_policy_resolver`].
    /// Defaults to `resolver`, so an engine built without one behaves exactly as before.
    policy: Arc<dyn TypeResolver + Send + Sync>,
    xml_files: Vec<PlanFile>,
    /// The project's Java language level — gates recognition of version-specific binding forms
    /// (records, pattern variables, lambda inferred params) during caret classification.
    lang_level: LangLevel,
}

/// The two halves of the engine an edit invalidates, behind **one** lock.
///
/// They have to move together. A rename plan is built by looking a use site up in the index and
/// then rewriting the identifier at that offset in the file's text — so an index that has caught up
/// with an edit, paired with source text that hasn't, produces edits aimed at offsets that no
/// longer mean anything. One lock makes "the index and the sources agree" an invariant of the type
/// rather than a rule someone has to remember.
///
/// Everything else in the engine is genuinely build-time: the resolver has its own live overlay,
/// and `lang_level` changes only when the project's shape does.
struct Live {
    index: ReferenceIndex,
    java_files: Vec<PlanFile>,
    /// The project's simple→binary type map, which the walk resolves bare type names through. In
    /// here because a file that declares a *new* type has to make it resolvable to every other
    /// file's walk, and the map is how they find it.
    project_types: HashMap<String, String>,
    /// Who extends/implements whom, so an override family descends instead of scanning the project
    /// once per rename. In here because an `extends` clause is something an edit changes, and a
    /// stale answer here is not a missing feature but a **wrong rename**: a method's override family
    /// is what carries the rename to its implementations, so a subtype the map has not heard of
    /// keeps the old name and stops overriding what it declares.
    subtypes: SubtypeMap,
}

/// A resolver over the persisted project index ALONE — no JDK, no dependencies. The fallback for
/// [`SemanticEngine::for_project`] when the caller has no shared resolver to lend it (the provider
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

impl SemanticEngine {
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
            .map(|(p, s)| SourceFile {
                path: p.clone(),
                source: s.clone(),
            })
            .collect();
        let index = build_reference_index_with_progress(
            &ref_input,
            &*resolver,
            &project_types,
            on_progress,
        );
        let java_files = java_sources
            .into_iter()
            .map(|(path, source)| PlanFile { path, source })
            .collect();
        let xml_files = xml_sources
            .into_iter()
            .map(|(path, source)| PlanFile { path, source })
            .collect();
        // No project version here (the test/plain constructor) → unknown level enables all
        // binding forms.
        let policy = Arc::clone(&resolver);
        let subtypes = SubtypeMap::build(&index, &*resolver);
        Self {
            live: RwLock::new(Live { index, java_files, project_types, subtypes }),
            resolver,
            policy,
            xml_files,
            lang_level: LangLevel(0),
        }
    }

    /// Lend the engine a SECOND resolver, used only to answer policy questions — today, "does this
    /// method override something declared in a dependency jar?".
    ///
    /// The two differ in cost, not in kind. The walk resolver is deliberately cheap (JDK-only by
    /// default, see [`SemanticEngine::for_project`]) because it is consulted once per reference in
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
    /// far longer with every core busy, and until that walk finishes there is no engine at
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
            .map(|(p, s)| SourceFile {
                path: p.clone(),
                source: s.clone(),
            })
            .collect();
        let built = build_reference_index_incremental(
            &ref_input,
            &*resolver,
            &project_types,
            prior,
            on_progress,
        );
        let index = built.index;
        if let (Some(path), Some(cache)) = (&cache_path, &built.cache_to_save) {
            crate::refcache::save(path, cache);
        }

        let java_files = java_sources
            .into_iter()
            .map(|(path, source)| PlanFile { path, source })
            .collect();
        let xml_files = xml_sources
            .into_iter()
            .map(|(path, source)| PlanFile { path, source })
            .collect();
        // Same view for both until the caller lends a fuller one (`with_policy_resolver`).
        let policy = Arc::clone(&resolver);
        let subtypes = SubtypeMap::build(&index, &*resolver);
        Ok(Self {
            live: RwLock::new(Live { index, java_files, project_types, subtypes }),
            resolver,
            policy,
            xml_files,
            lang_level: LangLevel::from_version(jdk_version),
        })
    }

    /// Read the live half of the engine, recovering from a panicked writer rather than propagating
    /// it: a poisoned lock here would take find-usages, go-to and hover down for the rest of the
    /// session over one bad file.
    fn live(&self) -> std::sync::RwLockReadGuard<'_, Live> {
        self.live.read().unwrap_or_else(|p| p.into_inner())
    }

    /// Bring the reference index up to date with an edit to `file`, and with the edits it implies
    /// elsewhere. `source == None` is a delete.
    ///
    /// A file's edges depend on the *types other files declare*, so re-walking only the edited file
    /// is right exactly when the edit didn't change what that file declares. When it did — a method
    /// added, renamed or removed — every file resolving against those types is holding edges that
    /// were computed against the old shape, and they are re-walked too. That set comes from the
    /// index's own record of which buckets each file is in, so finding it costs no parsing.
    ///
    /// Returns how many files were re-walked (the edited one included), for logging.
    pub fn refresh_file(&self, file: &str, source: Option<&str>) -> usize {
        let mut guard = self.live.write().unwrap_or_else(|p| p.into_inner());
        // Reborrow as a plain `&mut Live` so the fields below can be borrowed apart — the index
        // mutably while the type map is read. Through the guard's `DerefMut` they cannot be.
        let live = &mut *guard;

        // Keep the plan sources in step with the index — see [`Live`]. Done FIRST because the walk
        // below reads `java_files` for the dependents, and one of them could be this file.
        match source {
            Some(text) => match live.java_files.iter_mut().find(|f| f.path == file) {
                Some(pf) => pf.source = text.to_string(),
                None => live
                    .java_files
                    .push(PlanFile { path: file.to_string(), source: text.to_string() }),
            },
            None => live.java_files.retain(|f| f.path != file),
        }

        let before_types: Vec<String> = live.index.types_declared_in(file);
        let before_print = live.index.declaration_fingerprint(file);
        live.index.refresh_file(file, source, &*self.resolver, &live.project_types);
        let after_print = live.index.declaration_fingerprint(file);

        // A body-only edit — which is nearly every edit — changes nothing another file resolves
        // against, so it costs exactly one walk. Only a moved fingerprint earns the sweep.
        if before_print == after_print {
            return 1;
        }

        // A type this file declares may be new, and until the map knows it every other file's walk
        // resolves the bare name to nothing. Additive on purpose: a type that went away leaves its
        // entry behind until the next full build, which resolves a name to a type that no longer
        // exists — strictly better than the alternative, where removing one class from a file
        // silently unresolves the ones still in it.
        let after_types = live.index.types_declared_in(file);
        for binary in &after_types {
            if let Some(simple) = binary.rsplit(['/', '$']).next() {
                live.project_types.insert(simple.to_string(), binary.clone());
            }
        }

        // Re-file this file's types in the subtype map, so an `extends` or `implements` that just
        // changed is reflected in the override families a rename descends. Driven by the INDEX's
        // view of which types the file declares rather than by the resolver's: the resolver keeps
        // serving a deleted type out of the persisted index until a full rebuild, and a subtype the
        // map still believes in would have a rename plan edits into a class that is not there.
        for binary in &before_types {
            if !after_types.iter().any(|b| b == binary) {
                live.subtypes.withdraw_type(binary);
            }
        }
        for binary in &after_types {
            live.subtypes.refresh_type(binary, &*self.resolver);
        }

        // Whatever this file declared before or declares now: a type that disappeared invalidates
        // its referrers exactly as much as one that appeared.
        let mut touched: HashSet<String> = before_types.into_iter().collect();
        touched.extend(after_types);
        if touched.is_empty() {
            return 1;
        }
        let dependents = live.index.dependents_of(&touched, file);
        let mut walked = 1;
        for dep in dependents {
            let Some(text) = live.java_files.iter().find(|f| f.path == dep).map(|f| f.source.clone())
            else {
                continue; // not a project source we hold — nothing to re-walk it from
            };
            live.index.refresh_file(&dep, Some(&text), &*self.resolver, &live.project_types);
            walked += 1;
        }
        walked
    }

    /// Plan a rename at `file`:`offset` → the new name. `None` when the caret isn't on a
    /// renameable identifier. `source` is the (possibly-unsaved) current buffer text.
    pub fn plan(
        &self,
        file: &str,
        source: &str,
        offset: usize,
        new_name: &str,
    ) -> Option<RenamePlan> {
        let live = self.live();
        rename_plan(
            &live.index,
            file,
            source,
            offset,
            new_name,
            &*self.resolver,
            &*self.policy,
            &live.project_types,
            &live.subtypes,
            &live.java_files,
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
        let live = self.live();
        let target = classify_target(
            &live.index,
            file,
            source,
            offset,
            &*self.resolver,
            &live.project_types,
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
        let live = self.live();
        plan_types(
            &live.index,
            renames,
            &live.java_files,
            &self.xml_files,
            &live.project_types,
            on_file,
        )
    }

    /// The project file declaring the type `binary`, if any — the index's answer, without lending
    /// the index itself out (it lives behind a lock now, so a borrow of it can't outlive the guard).
    pub fn file_declaring(&self, binary: &str) -> Option<String> {
        self.live().index.file_declaring(binary).map(str::to_string)
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
    pub fn declaration(
        &self,
        file: &str,
        source: &str,
        offset: usize,
    ) -> Option<DeclarationLocation> {
        let live = self.live();
        resolve_declaration(
            &live.index,
            file,
            source,
            offset,
            &*self.resolver,
            &live.project_types,
            &live.java_files,
            self.lang_level,
        )
    }

    /// Find all usages of the symbol at `file`:`offset` (byte offset), for find-usages.
    /// Shares the engine's reference index + resolver with rename (same off-thread build).
    /// `source` is the current (possibly-unsaved) buffer. `None` when the caret isn't on a
    /// referenceable symbol (a local/param is scope-exact and not bucketed here).
    pub fn find_usages(&self, file: &str, source: &str, offset: usize) -> Option<ReferencesResult> {
        let live = self.live();
        let mut result = references(
            &live.index,
            file,
            source,
            offset,
            &*self.resolver,
            &live.project_types,
        )?;

        // A field's generated accessors are the field under another name, and on a class whose
        // accessors Lombok writes they are the ONLY way the field is used from outside — so without
        // this, find-usages on such a field reports a field nobody touches. The getter cannot be
        // asked about directly either: it has no declaration anywhere to put a caret on.
        //
        // Done here rather than in `references` because finding them needs the TEXT of the file
        // that declares the owner, which the index does not hold and the engine does.
        if let DeclKey::Field { owner, name } = result.target.clone() {
            if let Some(decl) = live
                .index
                .file_declaring(&owner)
                .and_then(|f| project_source(&live.java_files, f))
            {
                for alias in generated_aliases(decl, &owner, &name) {
                    let usages = live.index.usages_of(&alias.key).to_vec();
                    if !usages.is_empty() {
                        result.aliases.push(AliasUsages { label: alias.label, usages });
                    }
                }
            }
        }
        Some(result)
    }

    /// The root of a call (`calls`) or type hierarchy for the symbol at `file`:`offset`, or `[]`
    /// when the caret is not on one that can be built from. `source` is the current buffer.
    ///
    /// Shares the caret classifier with find-usages and rename, so the three agree about what the
    /// caret is on before they ever differ about what to do with it.
    pub fn prepare_hierarchy(
        &self,
        file: &str,
        source: &str,
        offset: usize,
        calls: bool,
    ) -> Vec<HierarchyItem> {
        let live = self.live();
        let Some(key) = classify_caret(
            &live.index,
            file,
            source,
            offset,
            &*self.resolver,
            &live.project_types,
        ) else {
            return Vec::new();
        };
        crate::hierarchy::prepare(&self.hierarchy_ctx(&live), &key, calls)
    }

    /// One level of a hierarchy, expanded from a node's own handle.
    pub fn hierarchy_step(
        &self,
        handle: &HierarchyHandle,
        direction: HierarchyDirection,
    ) -> Vec<HierarchyItem> {
        let live = self.live();
        crate::hierarchy::step(&self.hierarchy_ctx(&live), handle, direction)
    }

    /// The four things a hierarchy question is answered from, from ONE read of the live state —
    /// see [`HierarchyCtx`] for why they cannot be gathered separately.
    fn hierarchy_ctx<'a>(&'a self, live: &'a Live) -> HierarchyCtx<'a> {
        HierarchyCtx {
            index: &live.index,
            files: &live.java_files,
            resolver: &*self.resolver,
            subtypes: &live.subtypes,
        }
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
        inherited_members(&*self.resolver, &self.live().java_files, file, type_name, line)
    }

    /// Resolve the symbol at `file`:`offset` to a hover card (signature + kind + owner).
    /// Shares the engine's classifier + resolver with rename/find-usages (same off-thread
    /// build). `source` is the current (possibly-unsaved) buffer. `None` when the caret
    /// isn't on a symbol we can classify (a local variable / parameter isn't keyed here).
    pub fn hover(&self, file: &str, source: &str, offset: usize) -> Option<HoverInfo> {
        let key = {
            let live = self.live();
            classify_caret(
                &live.index,
                file,
                source,
                offset,
                &*self.resolver,
                &live.project_types,
            )?
        };
        // How many arguments the call under the caret passes — what tells two overloads apart.
        let argc = bennu_java::prelude::call_arity_at(source, offset);
        let mut info = hover_for_key(&key, &*self.resolver, argc);
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
        let live = self.live();
        let file = live.index.file_declaring(key.owner_binary())?;
        let source = project_source(&live.java_files, file)?;
        let decl_start = decl_site_for_key(source, key)?;
        leading_javadoc(source, decl_start)
    }
}
