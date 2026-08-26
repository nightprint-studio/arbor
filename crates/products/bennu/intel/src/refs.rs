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

use bennu_java::prelude::{
    extract_symbols_from_root, infer_receiver_type_at, FileSymbols,
    TypeResolver,
};
use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

/// The project's Java language level — gates recognition of version-specific binding forms
/// (a Java-8 project has no records or pattern variables). Level `0` means "unknown" (the
/// project JDK wasn't detected) and ENABLES every construct, so go-to never silently breaks
/// on valid source just because the level couldn't be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LangLevel(pub u32);

impl LangLevel {
    /// Parse a Maven/JDK version (`"1.8"`, `"8"`, `"11"`, `"17.0.2"`, `"21-ea"`) to its Java
    /// feature number (8, 11, 17, 21). Unparseable / empty → `0` ("unknown", all enabled).
    pub fn from_version(v: &str) -> Self {
        let major = v
            .trim()
            .trim_start_matches("1.") // "1.8" → "8"; "17" is untouched
            .split(['.', '-', '_'])
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        LangLevel(major)
    }

    /// Whether the level is at least `min` (an unknown level `0` counts as "yes" — never gate
    /// OUT a construct when the level wasn't detected).
    fn at_least(self, min: u32) -> bool {
        self.0 == 0 || self.0 >= min
    }

    /// Lambda inferred parameters `(x, y) -> …` — Java 8.
    fn lambda_inferred(self) -> bool {
        self.at_least(8)
    }

    // No `records()` gate any more. It existed to decide whether a record component counted as a
    // *local* binding, and a component is never a local at any language level — it is a member.
    // Renaming one has to reach its accessor, which a local rename cannot do.

    /// `instanceof` / `switch` pattern variables (`o instanceof String s`) — Java 16.
    fn patterns(self) -> bool {
        self.at_least(16)
    }
}

/// What a declaration *is*: a type, or a member (method/field) owned by a type. The key
/// the reverse map buckets usages under.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    /// The binary name of the TYPE this key references — the type itself, or the owner of a
    /// member. The incremental cache keys reverse-dependencies off this: a file "depends on"
    /// every type binary its edges resolve to, so when that type's file changes the referring
    /// file is re-walked.
    pub fn owner_binary(&self) -> &str {
        match self {
            DeclKey::Type { binary } => binary,
            DeclKey::Method { owner, .. } => owner,
            DeclKey::Field { owner, .. } => owner,
        }
    }
}

/// One resolved use site of a declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    /// The project file that DECLARES the type with JVM binary name `type_binary`
    /// (`it/acme/Foo`, or `it/acme/Outer.Inner` for a nested type). `None` when no indexed
    /// project source declares it. Go-to keys the member/type declaration lookup off this so a
    /// same-named member in a *different* class can't hijack the jump.
    /// The project file declaring `type_binary`, if a project source declares it.
    ///
    /// Accepts **both spellings of a nested type**. The project model names one from source —
    /// `p.Outer.Inner`, all dots — while the JVM names it `p/Outer$Inner`, and a binary that came
    /// back from bytecode (a generic signature, a classpath member lookup) carries the `$` form.
    /// Comparing only the dotted form meant a nested type of the project's own, once compiled into
    /// `target/classes`, was not recognised as a project type at all: go-to opened a decompiled
    /// stub of the user's own record, and a rename could find no source to edit.
    ///
    /// The exact spelling is tried first, so a top-level class genuinely named `A$B` still wins
    /// over a nested reading of it.
    pub fn file_declaring(&self, type_binary: &str) -> Option<&str> {
        let dotted = type_binary.replace('/', ".");
        if let Some(hit) = self.declaring_fqn(&dotted) {
            return Some(hit);
        }
        if !dotted.contains('$') {
            return None;
        }
        self.declaring_fqn(&dotted.replace('$', "."))
    }

    fn declaring_fqn(&self, fqn: &str) -> Option<&str> {
        self.file_symbols
            .iter()
            .find(|(_, fs)| fs.types.iter().any(|t| t.fqn == fqn))
            .map(|(path, _)| path.as_str())
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
/// declaring types, `project_types` is the project-wide simple→binary type map so a bare
/// `Foo` type reference resolves. Progress-free — see [`build_reference_index_with_progress`].
pub fn build_reference_index(
    files: &[SourceFile],
    resolver: &(dyn TypeResolver + Sync),
    project_types: &HashMap<String, String>,
) -> ReferenceIndex {
    build_reference_index_with_progress(files, resolver, project_types, &|_, _| {})
}

/// [`build_reference_index`] that reports `on_progress(files_done, total)` as it walks — so
/// the be layer can surface the walk (the O(N) phase that dominates a large-project index) as
/// live progress in the "Indexing" operation card. A full walk is just
/// [`build_reference_index_incremental`] with no prior cache.
pub fn build_reference_index_with_progress(
    files: &[SourceFile],
    resolver: &(dyn TypeResolver + Sync),
    project_types: &HashMap<String, String>,
    on_progress: &(dyn Fn(usize, usize) + Sync),
) -> ReferenceIndex {
    build_reference_index_incremental(files, resolver, project_types, None, on_progress).index
}

/// The outcome of an incremental build: the queryable index, plus the cache to persist — or
/// `None` when nothing was re-walked (the on-disk cache is already current, so the caller
/// skips a pointless multi-MB rewrite).
pub struct IncrementalBuild {
    pub index: ReferenceIndex,
    pub cache_to_save: Option<crate::refcache::RefCache>,
}

/// One file's contribution to the reference index: its resolved edges + parsed symbols + the
/// resolve stats. The unit the incremental cache stores and reuses.
struct FileContribution {
    symbols: FileSymbols,
    edges: Vec<(DeclKey, UsageLocation)>,
    attempted: usize,
    resolved: usize,
}

/// Parse + walk ONE file (parse once, reuse the tree for both the symbol map and the walk —
/// see [`FileWalker::walk`]).
fn walk_file(
    path: &str,
    source: &str,
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
) -> FileContribution {
    let mut walker = FileWalker::new(path, source, resolver, project_types);
    let symbols = {
        let mut parser = Parser::new();
        if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_ok() {
            if let Some(tree) = parser.parse(source, None) {
                let root = tree.root_node();
                let symbols = extract_symbols_from_root(&root, source);
                walker.walk(&root, &symbols);
                symbols
            } else {
                FileSymbols::default()
            }
        } else {
            FileSymbols::default()
        }
    };
    FileContribution {
        symbols,
        edges: walker.edges,
        attempted: walker.attempted,
        resolved: walker.resolved,
    }
}

/// Fold the per-file cache map into the queryable [`ReferenceIndex`], and hand back the cache
/// (same data) to persist. The clone is cheap next to the walk, and keeps the two owners
/// independent (the index is queried live; the cache is written to disk).
fn assemble(
    files_map: HashMap<String, crate::refcache::CachedFile>,
    tm_hash: u64,
) -> (ReferenceIndex, crate::refcache::RefCache) {
    let mut by_decl: HashMap<DeclKey, Vec<UsageLocation>> = HashMap::new();
    let mut file_symbols: HashMap<String, FileSymbols> = HashMap::new();
    let mut attempted = 0usize;
    let mut resolved = 0usize;
    for (path, cf) in &files_map {
        attempted += cf.attempted;
        resolved += cf.resolved;
        file_symbols.insert(path.clone(), cf.symbols.clone());
        for (key, usage) in &cf.edges {
            by_decl.entry(key.clone()).or_default().push(usage.clone());
        }
    }
    let index = ReferenceIndex { by_decl, file_symbols, attempted, resolved };
    let cache = crate::refcache::RefCache {
        version: crate::refcache::CACHE_VERSION,
        type_map_hash: tm_hash,
        files: files_map,
    };
    (index, cache)
}

/// Build the reference index, REUSING a prior on-disk cache where valid: walk only the files
/// whose source changed, plus (dependency-aware) any file that references a type declared by a
/// changed file. Returns the index + the refreshed cache to persist. `prior = None` (or a
/// cache whose version / project type-set no longer matches) → a full walk. See
/// [`crate::refcache`] for the invalidation model.
pub fn build_reference_index_incremental(
    files: &[SourceFile],
    resolver: &(dyn TypeResolver + Sync),
    project_types: &HashMap<String, String>,
    prior: Option<crate::refcache::RefCache>,
    on_progress: &(dyn Fn(usize, usize) + Sync),
) -> IncrementalBuild {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let tm_hash = crate::refcache::type_map_hash(project_types);
    // Global type-set guard: any type added / removed / renamed / moved shifts resolution
    // project-wide → drop the whole cache and walk fully.
    let prior =
        prior.filter(|c| c.version == crate::refcache::CACHE_VERSION && c.type_map_hash == tm_hash);
    let prior_valid = prior.is_some();

    let hashes: Vec<u64> = files.iter().map(|f| crate::refcache::content_hash(&f.source)).collect();
    let cur_paths: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();

    // Which files must be (re)walked?
    let walk_indices: Vec<usize> = match &prior {
        None => (0..files.len()).collect(),
        Some(prior) => {
            // Changed: a new file, or one whose content hash no longer matches the cache.
            let mut changed: Vec<usize> = Vec::new();
            for (i, f) in files.iter().enumerate() {
                match prior.files.get(&f.path) {
                    Some(cf) if cf.hash == hashes[i] => {}
                    _ => changed.push(i),
                }
            }
            // The types declared by the changed files (their names are unchanged — the type-set
            // guard passed — so read them off the cached symbols). Anything referencing one of
            // these must re-resolve against the (possibly new) signatures.
            let mut changed_types: HashSet<String> = HashSet::new();
            for &i in &changed {
                if let Some(cf) = prior.files.get(&files[i].path) {
                    changed_types.extend(crate::refcache::defined_types(&cf.symbols));
                }
            }
            // Dependents: unchanged files whose recorded deps name a changed type.
            let changed_set: HashSet<usize> = changed.iter().copied().collect();
            let mut walk = changed;
            if !changed_types.is_empty() {
                for (i, f) in files.iter().enumerate() {
                    if changed_set.contains(&i) {
                        continue;
                    }
                    if let Some(cf) = prior.files.get(&f.path) {
                        let deps = crate::refcache::deps_of(&cf.edges, &cf.symbols);
                        if deps.iter().any(|b| changed_types.contains(b)) {
                            walk.push(i);
                        }
                    }
                }
            }
            walk
        }
    };

    let total = walk_indices.len();
    let reused = files.len().saturating_sub(total);
    eprintln!(
        "bennu-be: reference walk starting — {total} to walk, {reused} reused (of {} files)",
        files.len()
    );
    let walk_start = std::time::Instant::now();
    on_progress(0, total);

    // Walk the selected files in parallel; a shared counter drives throttled progress.
    let walk_items: Vec<(usize, &SourceFile)> =
        walk_indices.iter().map(|&i| (i, &files[i])).collect();
    let done = AtomicUsize::new(0);
    let walked: Vec<(String, u64, FileContribution)> =
        crate::java_index::parallel_map(&walk_items, |(i, f)| {
            let contrib = walk_file(&f.path, &f.source, resolver, project_types);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 100 == 0 || n == total {
                on_progress(n, total);
            }
            (f.path.clone(), hashes[*i], contrib)
        });

    // New per-file map: reuse prior entries for files we didn't walk (still present), then
    // insert the freshly-walked ones.
    let mut files_map: HashMap<String, crate::refcache::CachedFile> = HashMap::new();
    if let Some(prior) = prior {
        for (path, cf) in prior.files {
            if cur_paths.contains(path.as_str()) {
                files_map.insert(path, cf);
            }
        }
    }
    for (path, hash, contrib) in walked {
        files_map.insert(
            path,
            crate::refcache::CachedFile {
                hash,
                edges: contrib.edges,
                symbols: contrib.symbols,
                attempted: contrib.attempted,
                resolved: contrib.resolved,
            },
        );
    }

    let (index, cache) = assemble(files_map, tm_hash);
    eprintln!(
        "bennu-be: reference walk done in {:?} — {} decls, {} attempted, {} resolved ({total} walked, {reused} reused)",
        walk_start.elapsed(),
        index.by_decl.len(),
        index.attempted,
        index.resolved
    );
    // Nothing re-walked and the prior cache was valid → the on-disk copy is already current;
    // don't rewrite it (it can be tens of MB).
    let cache_to_save = if prior_valid && total == 0 { None } else { Some(cache) };
    IncrementalBuild { index, cache_to_save }
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
    // A local variable / parameter is scope-exact and not reference-bucketed: `classify_caret`
    // now returns `None` for a local (so a local shadowing a field never reports the field's
    // uses), so no separate guard is needed here.
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
    /// Memo for [`records_owner`](FileWalker::records_owner). A file names only a handful of
    /// distinct owners but calls into them thousands of times, and the project-membership probe
    /// goes to the persisted index — unmemoized it doubled the walk on a call-heavy file.
    owner_is_project: HashMap<String, bool>,
    attempted: usize,
    resolved: usize,
    /// The file's `import`s, set by [`walk`](Self::walk) before it starts. A dependency type
    /// is reachable by its simple name **only** through these — see `resolve_type_simple`.
    imports: Vec<bennu_java::prelude::Import>,
    /// Every name bound by a local, parameter, catch, resource or pattern **anywhere in this
    /// file**, set by [`walk`](Self::walk). The cheap half of the shadowing test in
    /// [`on_bare_identifier`](Self::on_bare_identifier): a name nothing in the file binds cannot
    /// be shadowed, so the precise scope walk never runs for it.
    local_names: std::collections::HashSet<String>,
    /// Per enclosing type, `field name → the type that declares it`. See
    /// [`field_owners`](Self::field_owners).
    field_owners: HashMap<String, HashMap<String, String>>,
    /// The fields each type in THIS file declares, by simple type name — what the buffer says,
    /// independently of what the built index knows. The fallback half of
    /// [`field_owner`](Self::field_owner).
    file_fields: HashMap<String, std::collections::HashSet<String>>,
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
            owner_is_project: HashMap::new(),
            attempted: 0,
            resolved: 0,
            imports: Vec::new(),
            local_names: std::collections::HashSet::new(),
            field_owners: HashMap::new(),
            file_fields: HashMap::new(),
        }
    }

    /// Whether a member edge whose owner is `binary` is worth recording — i.e. the project
    /// declares that type.
    ///
    /// The walk *resolves* library types, because a library type is a **conduit**: in
    /// `list.stream().map(x -> x.foo())` the project type only reaches `x` by being substituted
    /// through `List` → `Stream` → `Function`, and without resolving those the `x.foo()` edge is
    /// never found at all. But an edge INTO a library member is never queried — find-usages and
    /// rename only ever target project symbols — so recording one would grow the in-memory index
    /// and the on-disk reference cache by every `list.add(…)` in the project, for nothing.
    ///
    /// Resolve widely, record narrowly.
    fn records_owner(&mut self, binary: &str) -> bool {
        if let Some(known) = self.owner_is_project.get(binary) {
            return *known;
        }
        let is_project = self.resolver.is_project_type(binary);
        self.owner_is_project.insert(binary.to_string(), is_project);
        is_project
    }

    /// Walk the file's CST emitting reference edges. `root`/`symbols` are the ALREADY-parsed
    /// tree + extracted symbols for this file — the caller parses once and shares both here
    /// and for the file-symbols map, so a file is never re-parsed per concern. Every
    /// receiver-type inference below reuses `root` + `symbols` (via `infer_receiver_type_at`)
    /// instead of re-parsing the whole file per call site — linear, not quadratic.
    fn walk(&mut self, root: &Node, symbols: &FileSymbols) {
        // The file's imports, for the whole walk: they are what turns a bare `SharedService`
        // into `com/acme/SharedService` when the class lives in a dependency.
        self.imports = symbols.imports.clone();
        self.local_names = collect_bound_names(root, self.bytes);
        // What the BUFFER declares, which is not the same question as what the index holds.
        self.file_fields = symbols
            .types
            .iter()
            .map(|t| (t.name.clone(), t.fields.iter().map(|f| f.name.clone()).collect()))
            .collect();
        self.walk_static_imports(symbols);
        let mut stack = vec![*root];
        while let Some(n) = stack.pop() {
            let mut cur = n.walk();
            for c in n.named_children(&mut cur) {
                stack.push(c);
            }
            match n.kind() {
                "method_invocation" => self.on_method_invocation(&n, root, symbols),
                "method_reference" => self.on_method_reference(&n, root, symbols),
                "element_value_pair" => self.on_annotation_element(&n),
                "field_access" => self.on_field_access(&n, root, symbols),
                "identifier" => self.on_bare_identifier(&n),
                "type_identifier" => self.on_type_identifier(&n),
                _ => {}
            }
        }
    }

    /// `Type::method` / `expr::method` — a method reference names a method as surely as a call
    /// does, and a rename that moves the calls but not these leaves the reference pointing at a
    /// name that no longer exists.
    ///
    /// The qualifier resolves exactly like a call's receiver: a type for `Failure::source_path` or
    /// `Reports::helper`, an expression for `obj::run`. `Foo::new` is skipped — a constructor
    /// reference names no method of its own, and the type it names is already recorded as a type
    /// use by the walk.
    fn on_method_reference(&mut self, node: &Node, root: &Node, symbols: &FileSymbols) {
        let Some((qualifier, name_node)) = method_reference_parts(node) else { return };
        let Some(name) = self.node_text(&name_node) else { return };
        self.attempted += 1;
        let Some(owner) = self.resolve_receiver_owner(
            &qualifier,
            name_node.start_byte(),
            &name,
            MemberSort::Method,
            root,
            symbols,
        ) else {
            return;
        };
        self.resolved += 1;
        let usage = self.usage_at(&name_node);
        if self.records_owner(&owner) {
            self.edges.push((DeclKey::Method { owner, name }, usage));
        }
    }

    /// `@Customizer(with_check_utenza = true)` — the KEY names an element of the annotation type,
    /// and an annotation element is a method (JLS §9.6.1: `@interface` members compile to public
    /// abstract no-arg methods, which is exactly how `bennu-java` indexes them).
    ///
    /// It is recorded because of what happens otherwise. `is_bound_name` correctly refuses to treat
    /// the key as a use of a *field* — it isn't one — but nothing else claimed it either, so the
    /// element had uses that the index could not see. Every rename of an `@interface` member was
    /// then refused for having no declaration to edit, which on a real project was 239 of 629
    /// refusals, the single largest reason. Recording the use and finding the declaration have to
    /// land together: either alone renames one half of a pair and stops the code compiling.
    fn on_annotation_element(&mut self, node: &Node) {
        let Some(key) = node.child_by_field_name("key") else { return };
        let Some(name) = self.node_text(&key) else { return };
        let Some(owner) = self.enclosing_annotation_binary(node) else { return };
        self.attempted += 1;
        self.resolved += 1;
        let usage = self.usage_at(&key);
        if self.records_owner(&owner) {
            self.edges.push((DeclKey::Method { owner, name }, usage));
        }
    }

    /// The binary name of the annotation type whose argument list `node` sits in.
    fn enclosing_annotation_binary(&self, node: &Node) -> Option<String> {
        let mut cur = node.parent();
        while let Some(n) = cur {
            if n.kind() == "annotation" {
                let name = n.child_by_field_name("name")?;
                let text = self.node_text(&name)?;
                // `@a.b.Ann(…)` is written as a scoped identifier: the last segment is the type.
                let simple = text.rsplit('.').next().unwrap_or(&text).to_string();
                return self.resolve_type_simple(&simple);
            }
            cur = n.parent();
        }
        None
    }

    fn on_method_invocation(&mut self, node: &Node, root: &Node, symbols: &FileSymbols) {
        let Some(name_node) = node.child_by_field_name("name") else { return };
        let Some(name) = self.node_text(&name_node) else { return };

        let owner = match node.child_by_field_name("object") {
            Some(obj) => {
                self.attempted += 1;
                let dot_off = name_node.start_byte();
                match self.resolve_receiver_owner(&obj, dot_off, &name, MemberSort::Method, root, symbols) {
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
        if self.records_owner(&owner) {
            self.edges.push((DeclKey::Method { owner, name }, usage));
        }
    }

    fn on_field_access(&mut self, node: &Node, root: &Node, symbols: &FileSymbols) {
        let Some(field_node) = node.child_by_field_name("field") else { return };
        let Some(name) = self.node_text(&field_node) else { return };
        let Some(obj) = node.child_by_field_name("object") else { return };
        self.attempted += 1;
        let dot_off = field_node.start_byte();
        let Some(owner) = self.resolve_receiver_owner(&obj, dot_off, &name, MemberSort::Field, root, symbols)
        else {
            return;
        };
        self.resolved += 1;
        let usage = self.usage_at(&field_node);
        if self.records_owner(&owner) {
            self.edges.push((DeclKey::Field { owner, name }, usage));
        }
    }

    /// A bare `identifier` standing for `this.<field>` — `count` where the source could equally
    /// have written `this.count`.
    ///
    /// Without this arm a field was indexed **only** at the sites that spell a receiver
    /// (`this.count`, `other.count`, `Config.MAX`). In ordinary Java that is the minority of
    /// them, and for a `private static final` constant it is usually none at all: find-usages
    /// answered "no usages" while the declaring class read it five lines down, and rename
    /// quietly left every one of those behind.
    ///
    /// Mirrors [`classify_caret_at`]'s bare-identifier arm — the index and the query must
    /// produce the same [`DeclKey`] or the lookup finds an empty bucket. The order of the tests
    /// is about cost rather than correctness: the field lookup is one hashed probe and rejects
    /// nearly every identifier in a method body (they are locals), so the scope walk that
    /// decides shadowing only runs for the few names that really are fields.
    fn on_bare_identifier(&mut self, node: &Node) {
        if is_member_selector_node(node) || is_bound_name(node) {
            return;
        }
        let Some(name) = self.node_text(node) else { return };
        let Some(enclosing) = self.enclosing_type_binary(node) else { return };
        let Some(owner) = self.field_owner(&enclosing, &name, node) else { return };
        self.attempted += 1;
        // A local or parameter of the same name shadows the field. `classify_caret` refuses to
        // classify those at all, so indexing them here would file a local's reads under a field
        // nobody touched.
        if self.local_names.contains(&name)
            && find_local_binding(node, self.bytes, &name, LangLevel(0)).is_some()
        {
            return;
        }
        self.resolved += 1;
        let usage = self.usage_at(node);
        if self.records_owner(&owner) {
            self.edges.push((DeclKey::Field { owner, name }, usage));
        }
    }

    /// The type that declares field `name`, for a bare read sitting inside `binary`.
    ///
    /// Two sources, and the second is the one that makes this work on a real project:
    ///
    /// 1. the **resolver**, walked up the supertypes — the only thing that can find an inherited
    ///    field, and the answer the query side computes for the same caret;
    /// 2. failing that, the **file's own parsed declarations**. The resolver answers from the
    ///    built index, which is a different thing from the buffer: a class the index has not
    ///    reached yet, one whose members did not survive a partial build, a nested type it holds
    ///    under another name — in every one of those `members_of` says nothing, and the field
    ///    three lines below the method reading it does not exist as far as this walk is
    ///    concerned. The query side never noticed because its own lookup is *lenient*: when
    ///    nothing declares the name it hands back the enclosing type anyway, so the caret builds
    ///    a key for a bucket the index never filled. That gap is the whole bug — a field with no
    ///    usages that <kbd>Ctrl</kbd>+click navigates from correctly — and the fallback closes it
    ///    on the one thing always available here, the file being walked.
    ///
    /// Still **strict overall**: a name neither the resolver nor the file knows as a field
    /// produces nothing, so an unresolvable identifier is not filed under whichever class it
    /// happened to sit in. Both branches name the same owner the query side would, which is the
    /// invariant that matters — a different answer is a key that matches nothing.
    ///
    /// Memoized per type: this is asked once per identifier in the project.
    fn field_owner(&mut self, binary: &str, name: &str, node: &Node) -> Option<String> {
        if !self.field_owners.contains_key(binary) {
            let table = self.build_field_owners(binary);
            self.field_owners.insert(binary.to_string(), table);
        }
        if let Some(owner) = self.field_owners.get(binary).and_then(|t| t.get(name)) {
            return Some(owner.clone());
        }
        let declared = enclosing_type_simple(node, self.bytes)
            .and_then(|simple| self.file_fields.get(&simple))
            .is_some_and(|fields| fields.contains(name));
        declared.then(|| binary.to_string())
    }

    /// `field name → declaring type` for `start` and its supertypes.
    ///
    /// Walks in the same order [`declaring_owner`](Self::declaring_owner) does, keeping the
    /// first declaration of each name — so the owner this names is byte-identical to the one the
    /// query side computes for the same caret. A different answer would be a key that matches
    /// nothing, which is the failure mode this whole arm exists to fix.
    fn build_field_owners(&self, start: &str) -> HashMap<String, String> {
        let mut out: HashMap<String, String> = HashMap::new();
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![start.to_string()];
        while let Some(bn) = stack.pop() {
            if !visited.insert(bn.clone()) {
                continue;
            }
            let Some(cm) = self.resolver.members_of(&bn) else { continue };
            for f in &cm.fields {
                out.entry(f.name.clone()).or_insert_with(|| bn.clone());
            }
            if let Some(sc) = cm.superclass.clone() {
                stack.push(sc);
            }
            stack.extend(cm.interfaces.iter().cloned());
        }
        out
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
        obj: &Node,
        dot_off: usize,
        member: &str,
        sort: MemberSort,
        root: &Node,
        symbols: &FileSymbols,
    ) -> Option<String> {
        // Reuse the file's already-parsed tree + symbols — NOT a per-call-site re-parse.
        if let Some(recv) = infer_receiver_type_at(root, self.source, symbols, dot_off, self.resolver) {
            if let Some(owner) = self.declaring_owner(&recv.binary_name, member, sort) {
                return Some(owner);
            }
        }
        // Static access via a TYPE name (`Util.create()`, `Config.CONST`): the receiver is a
        // type, not a value, so `infer` can't type it. Resolve the object text as a type and
        // look the member up there — otherwise static call/field USE SITES are never indexed and
        // find-usages / rename on a static member silently report nothing. The interactive query
        // path (`receiver_owner`) already has this fallback, so the two MUST agree.
        let obj_text = self.node_text(obj)?;
        let binary = self.resolve_type_simple(&obj_text)?;
        self.declaring_owner(&binary, member, sort)
    }

    /// Record the member each `import static a.b.C.member;` names, at the span of the member
    /// segment itself.
    ///
    /// The import is a use site like any other, and the only one a rename cannot afford to miss:
    /// leaving `import static p.Util.join_all;` behind after renaming `join_all` is not a cosmetic
    /// miss, it is a file that no longer compiles. Wildcard imports name no member and so have
    /// nothing to rewrite.
    ///
    /// Whether the name is a method or a field is not in the import — `import static p.Util.FOO;`
    /// reads the same either way — so the owner is asked, and a name that is both (legal in Java)
    /// is recorded under both keys. Only one of them is ever the target of a given rename.
    fn walk_static_imports(&mut self, symbols: &FileSymbols) {
        for target in bennu_java::prelude::static_import_targets(&symbols.imports) {
            let Some(member) = target.member.clone() else { continue };
            if !self.records_owner(&target.owner_binary) {
                continue;
            }
            let Some(cm) = self.resolver.members_of(&target.owner_binary) else { continue };
            let is_method = cm.methods.iter().any(|m| m.name == member);
            let is_field = cm.fields.iter().any(|f| f.name == member);
            if !is_method && !is_field {
                continue;
            }
            let Some(span) = self.static_import_member_span(&symbols.imports, &member) else {
                continue;
            };
            let usage = self.usage_span(span.0, span.1);
            if is_method {
                self.edges.push((
                    DeclKey::Method { owner: target.owner_binary.clone(), name: member.clone() },
                    usage.clone(),
                ));
            }
            if is_field {
                self.edges.push((
                    DeclKey::Field { owner: target.owner_binary.clone(), name: member.clone() },
                    usage,
                ));
            }
        }
    }

    /// The byte span of `member` inside the `import static …` declaration that names it. The member
    /// is the path's last segment, so the LAST occurrence within the declaration is it.
    fn static_import_member_span(
        &self,
        imports: &[bennu_java::prelude::Import],
        member: &str,
    ) -> Option<(usize, usize)> {
        for imp in imports.iter().filter(|i| i.static_ && !i.star) {
            let span = imp.span.as_ref()?;
            let text = self.source.get(span.start..span.end)?;
            if !imp.path.ends_with(member) {
                continue;
            }
            let at = text.rfind(member)?;
            return Some((span.start + at, span.start + at + member.len()));
        }
        None
    }

    fn enclosing_owner(&self, node: &Node, member: &str, sort: MemberSort) -> Option<String> {
        let fqn = self.enclosing_type_binary(node)?;
        if let Some(found) = self.declaring_owner_strict(&fqn, member, sort) {
            return Some(found);
        }
        // Not on the enclosing type or anything it inherits from. A bare name can also be bound by
        // an `import static`, which is precisely how a statically-imported helper is called — and
        // without this the call was filed under the CALLER's own type, a key no rename looks up, so
        // renaming the helper left every such call spelling the old name.
        if let Some(owner) = self.static_import_owner(member, sort) {
            return Some(owner);
        }
        Some(fqn)
    }

    /// The static-import owner that binds a bare `member` into this file, if one does.
    fn static_import_owner(&self, member: &str, sort: MemberSort) -> Option<String> {
        for target in bennu_java::prelude::static_import_targets(&self.imports) {
            match &target.member {
                // `import static a.b.C.member;` — named outright.
                Some(named) if named == member => return Some(target.owner_binary),
                Some(_) => continue,
                // `import static a.b.C.*;` — binds it only if the owner declares it.
                None => {
                    let Some(cm) = self.resolver.members_of(&target.owner_binary) else { continue };
                    let declares = match sort {
                        MemberSort::Method => cm.methods.iter().any(|m| m.name == member),
                        MemberSort::Field => cm.fields.iter().any(|f| f.name == member),
                    };
                    if declares {
                        return Some(target.owner_binary);
                    }
                }
            }
        }
        None
    }

    fn declaring_owner(&self, start_binary: &str, member: &str, sort: MemberSort) -> Option<String> {
        self.declaring_owner_strict(start_binary, member, sort)
            .or_else(|| Some(start_binary.to_string()))
    }

    /// The type in `start_binary`'s hierarchy that declares `member`, or `None` when none does.
    ///
    /// Separate from [`declaring_owner`], which falls back to the starting type: that fallback is
    /// right for a member whose supertypes we simply cannot resolve, and wrong for a name that is
    /// not a member of this hierarchy at all — a statically-imported one, say, which belongs to
    /// somebody else entirely. Telling the two apart is what lets the caller look elsewhere.
    fn declaring_owner_strict(
        &self,
        start_binary: &str,
        member: &str,
        sort: MemberSort,
    ) -> Option<String> {
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
                // `cm` is a shared `Arc` — clone the (small) supertype links, don't move.
                if let Some(sc) = cm.superclass.clone() {
                    stack.push(sc);
                }
                stack.extend(cm.interfaces.iter().cloned());
            }
        }
        None
    }

    /// The binary name of the type `node` sits in.
    ///
    /// Delegates to the FREE function of the same name — the one the caret classifier uses — and
    /// that is the whole point. This used to have its own copy that asked the resolver to turn
    /// the simple name into a binary one, where the query's falls back to the buffer's `package`
    /// line. For a type the project map holds they agree; for one it does not — a **nested**
    /// class, a file the index has not reached — the copy gave up and the query did not, so a
    /// member's own-class uses were filed under a key nothing ever looked up. Two spellings of
    /// one question is how an index and the query that reads it drift apart silently.
    fn enclosing_type_binary(&self, node: &Node) -> Option<String> {
        enclosing_type_binary(node, self.bytes, self.project_types)
    }

    fn resolve_type_simple(&self, simple: &str) -> Option<String> {
        if simple.contains('.') {
            return Some(simple.replace('.', "/"));
        }
        if let Some(b) = self.project_types.get(simple) {
            return Some(b.clone());
        }
        // WITH the file's imports. A project type resolves off `project_types` above, but a
        // **dependency** type only ever resolves through the `import` that named it — the
        // resolver's simple-name hints hold the project's own types and common JDK names, not
        // every class on the classpath. Passing an empty list here meant no use of a library
        // class was ever indexed, so find-usages on one reported nothing at all.
        self.resolver.resolve_simple_name(simple, &self.imports)
    }

    fn is_declaration_name(&self, node: &Node) -> bool {
        let Some(parent) = node.parent() else { return false };
        // Must list EVERY kind that declares a type. A kind missing here is indexed as a *use* of
        // itself, and the rename planner adds the declaration edit separately — so the same byte
        // range would be rewritten twice, once from the index and once from the walk.
        if !matches!(
            parent.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            return false;
        }
        parent.child_by_field_name("name").map(|nm| nm.id() == node.id()).unwrap_or(false)
    }

    fn usage_at(&self, node: &Node) -> UsageLocation {
        self.usage_span(node.start_byte(), node.end_byte())
    }

    /// A use site given as a raw byte range — for the places a use is NOT a node of its own, such
    /// as the member segment inside an `import static` declaration.
    fn usage_span(&self, start: usize, end: usize) -> UsageLocation {
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
    let tree = bennu_java::prelude::parse_java(source)?;
    classify_caret_at(index, file, source, &tree.root_node(), offset, resolver, project_types)
}

/// The core of [`classify_caret`] over an ALREADY-PARSED `root`, so a caller that has already
/// parsed `source` (rename's [`classify_target`]) doesn't re-parse it, and the receiver-type
/// inference reuses the same tree + a single symbol extraction (via [`infer_receiver_type_at`])
/// instead of re-parsing AND re-extracting the whole file per query. This is the interactive
/// go-to / find-usages / hover HOT PATH: the old per-query re-parse of a large legacy file cost
/// hundreds of ms and read to the user as a UI freeze.
#[allow(clippy::too_many_arguments)]
fn classify_caret_at(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    root: &Node,
    offset: usize,
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
) -> Option<DeclKey> {
    let bytes = source.as_bytes();

    let ident = smallest_named_at(root, offset)?;
    let ident_text = ident.utf8_text(bytes).ok()?.to_string();

    // A local variable / parameter is scope-exact and NOT a bucketed `DeclKey` — never classify
    // it to a same-named field (the bare-identifier fallback below would otherwise resolve a
    // local that shadows a field to that field). Callers that navigate locals use
    // `classify_target` (which has a `Local` variant); every `classify_caret` consumer
    // (find-usages, hover) wants nothing for a local. Reuses the already-parsed node.
    if ident.kind() == "identifier"
        && !is_member_selector_node(&ident)
        && find_local_binding(&ident, bytes, &ident_text, LangLevel(0)).is_some()
    {
        return None;
    }

    if let Some(key) = decl_name_key(&ident, bytes, file, index, project_types) {
        return Some(key);
    }

    let parent = ident.parent()?;
    match parent.kind() {
        "method_invocation" => {
            let name_node = parent.child_by_field_name("name")?;
            if name_node.id() != ident.id() {
                let symbols = extract_symbols_from_root(root, source);
                return receiver_side_key(
                    &ident,
                    &ident_text,
                    source,
                    resolver,
                    project_types,
                    &symbols.imports,
                );
            }
            let owner = match parent.child_by_field_name("object") {
                Some(obj) => {
                    // Extract symbols from the SHARED tree once (only when a receiver is present)
                    // — no re-parse, no re-extract per query.
                    let symbols = extract_symbols_from_root(root, source);
                    receiver_owner(
                        &obj,
                        name_node.start_byte(),
                        &ident_text,
                        true,
                        source,
                        bytes,
                        root,
                        &symbols,
                        resolver,
                        project_types,
                    )?
                }
                None => {
                    let fqn = enclosing_type_binary(&parent, bytes, project_types)?;
                    declaring_owner(resolver, &fqn, &ident_text, true)?
                }
            };
            Some(DeclKey::Method { owner, name: ident_text })
        }
        "field_access" => {
            let field_node = parent.child_by_field_name("field")?;
            if field_node.id() != ident.id() {
                let symbols = extract_symbols_from_root(root, source);
                return receiver_side_key(
                    &ident,
                    &ident_text,
                    source,
                    resolver,
                    project_types,
                    &symbols.imports,
                );
            }
            let obj = parent.child_by_field_name("object")?;
            let symbols = extract_symbols_from_root(root, source);
            let owner = receiver_owner(
                &obj,
                field_node.start_byte(),
                &ident_text,
                false,
                source,
                bytes,
                root,
                &symbols,
                resolver,
                project_types,
            )?;
            Some(DeclKey::Field { owner, name: ident_text })
        }
        "type_identifier" | "scoped_type_identifier" | "generic_type" => {
            // Use the FULL type expression (the parent), not just the clicked segment, so a
            // fully-qualified `alpha.Widget` resolves by its package (never the ambiguous bare
            // `Widget` shared with another package). `type_key` strips generics.
            let text = parent.utf8_text(bytes).map(str::to_string).unwrap_or_else(|_| ident_text.clone());
            let symbols = extract_symbols_from_root(root, source);
            type_key(&text, project_types, resolver, &symbols.imports)
        }
        _ => {
            if ident.kind() == "type_identifier" {
                let symbols = extract_symbols_from_root(root, source);
                return type_key(&ident_text, project_types, resolver, &symbols.imports);
            }
            // A bare `identifier` that isn't a declaration name, a member selector
            // (`x.foo` / `foo.bar()` — handled above), or a local/param (filtered before
            // this in `classify_target`): resolve it as a FIELD of the enclosing type and
            // its supertypes — a `this`-less field reference like `foo` standing for
            // `this.foo`. Without this a bare field usage classified to nothing, so go-to
            // silently did nothing (and the FE mis-fell-back to a same-named class).
            if ident.kind() == "identifier" && !is_member_selector_node(&ident) {
                let fqn = enclosing_type_binary(&ident, bytes, project_types)?;
                let owner = declaring_owner(resolver, &fqn, &ident_text, false)?;
                return Some(DeclKey::Field { owner, name: ident_text });
            }
            None
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
        "class_declaration"
        | "interface_declaration"
        | "enum_declaration"
        | "record_declaration"
        | "annotation_type_declaration" => {
            if parent.child_by_field_name("name")?.id() != node.id() {
                return None;
            }
            let binary = index
                .symbols(file)
                .and_then(|fs| fs.types.iter().find(|t| t.name == name))
                .map(|t| t.fqn.replace('.', "/"))
                .or_else(|| project_types.get(&name).cloned())
                // Neither indexed nor a project type: a library source view, where the caret
                // is on the dependency class's own declaration. Its own `package` line names
                // it, and asking the file is the only thing that can work — there is no index
                // entry for a file that is under no project.
                .or_else(|| {
                    buffer_package(bytes).map(|pkg| format!("{}/{name}", pkg.replace('.', "/")))
                })?;
            Some(DeclKey::Type { binary })
        }
        "method_declaration" => {
            if parent.child_by_field_name("name")?.id() != node.id() {
                return None;
            }
            let owner = enclosing_type_binary(&parent, bytes, project_types)?;
            Some(DeclKey::Method { owner, name })
        }
        // An `@interface` element is a METHOD — `bennu-java` indexes it as one (a public abstract
        // no-arg method, which is what it compiles to), and the walk records its use sites under a
        // method key. Without this arm the caret fell through to the bare-identifier path and came
        // back as a FIELD of the annotation type: a key with no declaration to edit and no use
        // sites recorded against it, so both find-usages and rename answered empty.
        "annotation_type_element_declaration" => {
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
    imports: &[bennu_java::prelude::Import],
) -> Option<DeclKey> {
    // The receiver of a member access (`obj` in `obj.foo()` / `obj.field`) is usually a
    // VARIABLE, not a type. Resolve it as a FIELD of the enclosing type first — a `this`-less
    // field like `stepper` for `this.stepper` — so go-to lands on the FIELD's declaration
    // instead of collapsing the variable onto the enclosing class. (Locals are already
    // resolved by `classify_target` before this.) Only when it isn't a field do we treat it
    // as a bare TYPE name, e.g. the static receiver in `Foo.staticMethod()`.
    let bytes = source.as_bytes();
    if let Some(fqn) = enclosing_type_binary(ident, bytes, project_types) {
        if let Some(owner) = declaring_owner(resolver, &fqn, ident_text, false) {
            return Some(DeclKey::Field { owner, name: ident_text.to_string() });
        }
    }
    type_key(ident_text, project_types, resolver, imports)
}

/// The owner type of `member` accessed on `obj` in `obj.member` (`obj.foo()` / `obj.field`).
/// First infers `obj`'s VALUE type and finds the member there (walking supertypes); if that
/// fails, treats `obj` as a TYPE name — a STATIC access `Type.member` (`Util.helper()`,
/// `Config.CONST`, `Registry.size`) whose receiver is a type, not a value. `None` otherwise.
#[allow(clippy::too_many_arguments)]
fn receiver_owner(
    obj: &Node,
    dot_off: usize,
    member: &str,
    is_method: bool,
    source: &str,
    bytes: &[u8],
    root: &Node,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
) -> Option<String> {
    // Reuse the caret's already-parsed tree + once-extracted symbols (NOT a per-call re-parse).
    if let Some(recv) = infer_receiver_type_at(root, source, symbols, dot_off, resolver) {
        if let Some(owner) = declaring_owner(resolver, &recv.binary_name, member, is_method) {
            return Some(owner);
        }
    }
    let obj_text = obj.utf8_text(bytes).ok()?;
    if let Some(DeclKey::Type { binary }) =
        type_key(obj_text, project_types, resolver, &symbols.imports)
    {
        return declaring_owner(resolver, &binary, member, is_method);
    }
    None
}

fn type_key(
    simple: &str,
    project_types: &HashMap<String, String>,
    resolver: &dyn TypeResolver,
    imports: &[bennu_java::prelude::Import],
) -> Option<DeclKey> {
    let base = simple.split('<').next().unwrap_or(simple).trim();
    if base.contains('.') {
        // A dotted type expression is EITHER a nested-type reference (`Outer.Inner`) OR a
        // package-qualified FQN (`alpha.Widget`). If the FIRST segment is a known project TYPE
        // it's nested → resolve the trailing simple name (nested types are indexed by it).
        // Otherwise the prefix is a PACKAGE → the binary is the dotted path itself, which
        // disambiguates two same-simple-name types in different packages (`alpha.Widget` vs
        // `beta.Widget`) that the simple→binary map alone cannot.
        let first = base.split('.').next().unwrap_or(base);
        if project_types.contains_key(first) {
            let last = base.rsplit('.').next().unwrap_or(base);
            if let Some(b) = project_types.get(last) {
                return Some(DeclKey::Type { binary: b.clone() });
            }
        }
        return Some(DeclKey::Type { binary: base.replace('.', "/") });
    }
    if let Some(b) = project_types.get(base) {
        return Some(DeclKey::Type { binary: b.clone() });
    }
    // With the file's imports — the only route by which a bare `SharedService` reaches a class
    // that lives in a dependency. The key produced here has to be byte-identical to the one the
    // walker indexed the use sites under, so this and `resolve_type_simple` must resolve the
    // same way; an empty list here made find-usages silent even once the edges existed.
    resolver.resolve_simple_name(base, imports).map(|binary| DeclKey::Type { binary })
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
            // `cm` is a shared `Arc` — clone the (small) supertype links, don't move.
            if let Some(sc) = cm.superclass.clone() {
                stack.push(sc);
            }
            stack.extend(cm.interfaces.iter().cloned());
        }
    }
    Some(start.to_string())
}

fn enclosing_type_binary(
    node: &Node,
    bytes: &[u8],
    project_types: &HashMap<String, String>,
) -> Option<String> {
    // The whole chain of enclosing type declarations, innermost first — not just the nearest one.
    //
    // `project_types` is keyed by SIMPLE name, which is unique for top-level types and emphatically
    // not for nested ones: eleven test classes in one real project each declared a nested
    // `JakartaValidationTest`, so every caret in any of them resolved to whichever binary happened
    // to win the map. The key named a type in another file, so nothing was declared there and
    // nothing used it — 237 refusals reading "nothing here can be renamed". The file is the
    // authority on what it declares, so when it says which package it is in, the binary is built
    // from the chain and the map is only a fallback.
    let mut chain: Vec<String> = Vec::new();
    let mut cur = Some(*node);
    while let Some(n) = cur {
        // An anonymous class body is a type too, and the FIRST one going up: everything written
        // inside `new Runnable() { … }` belongs to it, not to the class the `new` appears in.
        // Climbing past it filed an anonymous `run()` under the enclosing class — so find-usages on
        // that class's own `run()` reported a method nobody had called.
        if bennu_java::prelude::is_anonymous_body(&n) {
            if let Some(name) = bennu_java::prelude::anonymous_type_name(&n, bytes) {
                let outer = n.parent().and_then(|p| enclosing_type_binary(&p, bytes, project_types));
                if let Some(outer) = outer {
                    return Some(join_binary(&outer, &name, &chain));
                }
            }
        }
        if matches!(
            n.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                // A record and an `@interface` declare types like any other. Missing them here sent
                // every member declared in one to the type ABOVE it — or to nothing at all for a
                // top-level record.
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            let name = n.child_by_field_name("name")?.utf8_text(bytes).ok()?.to_string();
            chain.push(name);
        }
        cur = n.parent();
    }
    let innermost = chain.first()?.clone();
    // The buffer's own package + the chain it declares. Also the only thing that can work inside a
    // LIBRARY source view, which is under no project and so in no map at all.
    if let Some(pkg) = buffer_package(bytes) {
        let outermost = chain.last().cloned().unwrap_or_else(|| innermost.clone());
        let rest = &chain[..chain.len().saturating_sub(1)];
        return Some(join_binary(&format!("{}/{outermost}", pkg.replace('.', "/")), "", rest));
    }
    if let Some(b) = project_types.get(&innermost) {
        return Some(b.clone());
    }
    Some(innermost)
}

/// `outer` + an optional next segment + the remaining chain (which is stored innermost-first, so it
/// is appended in reverse). The index spells a nested type `p/Outer/Inner`, so the separator is `/`.
fn join_binary(outer: &str, next: &str, rest_innermost_first: &[String]) -> String {
    let mut out = outer.to_string();
    if !next.is_empty() {
        out.push('/');
        out.push_str(next);
    }
    for seg in rest_innermost_first.iter().rev() {
        out.push('/');
        out.push_str(seg);
    }
    out
}

/// The `package` a buffer declares, read off its own text.
///
/// The file is the authority on what it declares — more so than an index, which may not hold
/// it at all (a library source view is under no project). Scans only the head: a package
/// declaration precedes every type, so anything after the first `{` is past the point.
fn buffer_package(bytes: &[u8]) -> Option<String> {
    let head_len = bytes.iter().position(|b| *b == b'{').unwrap_or(bytes.len()).min(4096);
    let head = std::str::from_utf8(&bytes[..head_len]).ok()?;
    for line in head.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("package ") else { continue };
        // Cut at the `;`, not at the end of the line: the declaration ends at the semicolon, and
        // `package p; class C {}` on one line is legal Java (and is how fixtures are written).
        // Trimming only a trailing `;` swallowed the rest of the line into the package name.
        let pkg = rest.split(';').next().unwrap_or("").trim();
        if !pkg.is_empty() {
            return Some(pkg.to_string());
        }
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
    level: LangLevel,
) -> Option<RenameTarget> {
    let tree = bennu_java::prelude::parse_java(source)?;
    let root = tree.root_node();
    let bytes = source.as_bytes();

    let ident = smallest_named_at(&root, offset)?;
    if ident.kind() == "identifier" && !is_member_selector_node(&ident) {
        let name = ident.utf8_text(bytes).ok()?.to_string();
        // A real local wins first: Java lets a local inside a record's method shadow a component,
        // and that one genuinely is scope-exact.
        if let Some((ds, de)) = find_local_binding(&ident, bytes, &name, level) {
            return Some(RenameTarget::Local { name, def_start: ds, def_end: de });
        }
        // Then a record component — its declaration in the header, or a bare reference to it from
        // the record's body. Neither reaches the member classifier below on its own: that one
        // reads *uses on a receiver*, and both of these are unqualified.
        if let Some(owner) = record_component_owner(&ident, source, &name) {
            return Some(RenameTarget::Member { key: DeclKey::Field { owner, name } });
        }
    }

    // Reuse the tree this function already parsed — no second parse on the go-to hot path.
    let key = classify_caret_at(index, file, source, &root, offset, resolver, project_types)?;
    match &key {
        DeclKey::Type { binary } => {
            Some(RenameTarget::Type { key: key.clone(), binary: binary.clone() })
        }
        DeclKey::Method { .. } | DeclKey::Field { .. } => Some(RenameTarget::Member { key }),
    }
}

/// Whether an `identifier` is a name being **declared** (or a label, or an annotation element)
/// rather than a reference to something.
///
/// Java exposes every declared name as its parent's `name` field, so that one test covers a
/// field's own declarator, a parameter, a type, a method, an enum constant and an annotation's
/// own type in one line. The three exceptions below are the shapes where a bare identifier is
/// not an expression at all — and each of them is a place a legacy codebase reuses a field's
/// name freely.
///
/// The two member-selector shapes (`x.f`, `f()`) also carry a `name` field; they are filtered
/// ahead of this by [`is_member_selector_node`].
fn is_bound_name(node: &Node) -> bool {
    let Some(parent) = node.parent() else { return false };
    match parent.kind() {
        // Labels are a namespace of their own: `outer:`, `break outer`, `continue outer`.
        "labeled_statement" | "break_statement" | "continue_statement" => true,
        // Package / import / qualified names: every segment is an `identifier`, and a field
        // called `com` would otherwise collect every import in the file.
        "scoped_identifier" | "package_declaration" | "import_declaration" => true,
        // `@Retention(value = RUNTIME)` — an annotation element is not a field of the enclosing
        // class, however identically the two are named.
        "element_value_pair" => {
            parent.child_by_field_name("key").map(|n| n.id() == node.id()).unwrap_or(false)
        }
        _ => parent.child_by_field_name("name").map(|n| n.id() == node.id()).unwrap_or(false),
    }
}

/// The SIMPLE name of the type declaration enclosing `node`.
///
/// The same walk `enclosing_type_binary` does and stopping at the same declaration kinds, so the
/// two always speak about one type — it is the binary form of exactly this name that a bare
/// field's key is built from.
fn enclosing_type_simple(node: &Node, bytes: &[u8]) -> Option<String> {
    let mut cur = Some(*node);
    while let Some(n) = cur {
        if matches!(n.kind(), "class_declaration" | "interface_declaration" | "enum_declaration") {
            return n.child_by_field_name("name")?.utf8_text(bytes).ok().map(str::to_string);
        }
        cur = n.parent();
    }
    None
}

/// Every name bound by a local, parameter, catch, resource, loop variable or pattern anywhere in
/// a file.
///
/// A coarse over-approximation on purpose — it ignores scope entirely, so a name bound in ONE
/// method counts as bound for the whole file. That is the right shape for what it is used for:
/// deciding whether the precise, per-scope [`find_local_binding`] walk is worth running at all.
/// A name in this set gets the exact answer; a name outside it cannot be shadowed anywhere, and
/// skipping the walk for it is what keeps indexing a field read 200 times in a 2000-line method
/// from re-scanning that method 200 times.
fn collect_bound_names(root: &Node, bytes: &[u8]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let mut stack = vec![*root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        let bound = match n.kind() {
            // A declarator under a `field_declaration` is a field, not a local.
            "variable_declarator" => match n.parent().map(|p| p.kind()) {
                Some("field_declaration") => None,
                _ => n.child_by_field_name("name"),
            },
            "formal_parameter"
            | "spread_parameter"
            | "catch_formal_parameter"
            | "resource"
            | "enhanced_for_statement"
            | "type_pattern"
            | "instanceof_expression" => n.child_by_field_name("name"),
            // `(x, y) -> …` and `x -> …`: inferred lambda parameters are bare identifiers with
            // no `name` field to ask for.
            "inferred_parameters" => {
                let mut cw = n.walk();
                for c in n.named_children(&mut cw) {
                    if let Ok(t) = c.utf8_text(bytes) {
                        out.insert(t.to_string());
                    }
                }
                None
            }
            "lambda_expression" => n
                .child_by_field_name("parameters")
                .filter(|p| p.kind() == "identifier"),
            _ => None,
        };
        if let Some(nm) = bound {
            if let Ok(t) = nm.utf8_text(bytes) {
                out.insert(t.to_string());
            }
        }
    }
    out
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
        // `Failure::source_path` — the name after `::` selects a member of the qualifier, exactly
        // like the name after a `.`. The grammar gives it no field name, so it is the LAST named
        // child; the first is the qualifier. Without this it read as a bare identifier and was
        // filed as a field of whatever class the expression appeared in — a use of something
        // nobody wrote, and one a rename of that field would have rewritten.
        "method_reference" => {
            let mut cur = parent.walk();
            let children: Vec<Node> = parent.named_children(&mut cur).collect();
            children.len() >= 2
                && children.last().map(|n| n.id() == node.id()).unwrap_or(false)
                && children.first().map(|n| n.id() != node.id()).unwrap_or(false)
        }
        _ => false,
    }
}

/// The `(qualifier, member name)` of a method reference — `Failure::source_path`.
///
/// `None` for `Foo::new`: a constructor reference names no method, and `new` is not even a named
/// node, so the "last named child" would be the qualifier itself.
pub(crate) fn method_reference_parts<'a>(node: &Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
    let mut cur = node.walk();
    let children: Vec<Node<'a>> = node.named_children(&mut cur).collect();
    if children.len() < 2 {
        return None;
    }
    let name = *children.last()?;
    let qualifier = *children.first()?;
    if name.kind() != "identifier" || name.id() == qualifier.id() {
        return None;
    }
    Some((qualifier, name))
}

/// The binary name of the record `ident` is a **component** of, when it is one.
///
/// A component looks exactly like a method parameter to the grammar — `formal_parameter` under
/// `formal_parameters` — and only the grandparent tells them apart. `None` for anything else,
/// including a real parameter, so the caller falls through unchanged.
fn record_component_owner(ident: &Node, source: &str, name: &str) -> Option<String> {
    // Walk out to the nearest type. A `record` on the way means `ident` is either the component's
    // own declaration or an unqualified reference to it from the record's body — the two places a
    // component's name appears without a receiver.
    let mut cur = ident.parent();
    while let Some(n) = cur {
        if n.kind() == "record_declaration" {
            return has_component(&n, source.as_bytes(), name)
                .then(|| bennu_java::prelude::enclosing_type_binary(source, ident.start_byte()))
                .flatten();
        }
        // Any other type boundary: whatever this is, it is not a component of a record.
        if matches!(
            n.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "annotation_type_declaration"
        ) {
            return None;
        }
        cur = n.parent();
    }
    None
}

/// Whether `record_declaration` declares a component called `name`.
fn has_component(record: &Node, bytes: &[u8], name: &str) -> bool {
    let Some(params) = record.child_by_field_name("parameters") else { return false };
    let mut cursor = params.walk();
    // Bound rather than returned directly: the iterator borrows `cursor`, and as a tail expression
    // its temporary would outlive it.
    let found = params.named_children(&mut cursor).any(|p| {
        p.child_by_field_name("name").and_then(|nm| nm.utf8_text(bytes).ok()) == Some(name)
    });
    found
}

/// Find the declarator NAME span of the local variable / parameter `name` in scope at
/// `ident`. `None` when `name` is not a local/param binding (a field, or unresolved) — so
/// the caller falls back to member/type classification.
fn find_local_binding(
    ident: &Node,
    bytes: &[u8],
    name: &str,
    level: LangLevel,
) -> Option<(usize, usize)> {
    let mut cur = ident.parent();
    while let Some(n) = cur {
        if matches!(
            n.kind(),
            "method_declaration"
                | "constructor_declaration"
                | "compact_constructor_declaration"
                | "lambda_expression"
        ) {
            if let Some(p) = find_param_decl(&n, bytes, name, level) {
                return Some(p);
            }
        }
        // Any body/statement scope that can bind a local. NB: a method body is a `block` but a
        // CONSTRUCTOR body is a `constructor_body`; `for`/enhanced-`for`/`catch`/try-with-
        // resources bind their own variables; static/instance initializers are `block`. Deeper
        // nested locals are reached because `find_local_decl` descends the scanned scope.
        if matches!(
            n.kind(),
            "block"
                | "constructor_body"
                | "for_statement"
                | "enhanced_for_statement"
                | "catch_clause"
                | "try_with_resources_statement"
        ) {
            if let Some(d) = find_local_decl(&n, bytes, name, level) {
                return Some(d);
            }
        }
        // A `record`'s components are NOT locals, however much the grammar makes them look like
        // parameters. Per JLS §8.10 each one declares a `private final` field *and* a public
        // accessor of the same name, and callers reach it through the accessor — `f.source_path()`.
        // Classified as a local, a rename walked only the record's own scope: the component was
        // renamed and every caller was left reading a method that no longer exists. So this is a
        // type boundary like any other, and the member classifier takes it from here.
        if n.kind() == "record_declaration" {
            break;
        }
        // Stop at the enclosing TYPE boundary — a field of the type is NOT a local.
        if matches!(
            n.kind(),
            "class_declaration" | "interface_declaration" | "enum_declaration" | "annotation_type_declaration"
        ) {
            break;
        }
        cur = n.parent();
    }
    None
}

fn find_param_decl(node: &Node, bytes: &[u8], name: &str, level: LangLevel) -> Option<(usize, usize)> {
    let params = node.child_by_field_name("parameters")?;
    // Lambda with INFERRED parameters — `(x, y) -> …` (`inferred_parameters` of bare
    // identifiers) or a single unparenthesized `x -> …` (the `parameters` field IS the
    // identifier). Java 8+ — gated. Typed lambda params `(int x) -> …` fall through to the
    // `formal_parameter` loop below.
    if node.kind() == "lambda_expression" && level.lambda_inferred() {
        if params.kind() == "identifier" {
            if params.utf8_text(bytes).ok() == Some(name) {
                return Some((params.start_byte(), params.end_byte()));
            }
        } else {
            let mut cw = params.walk();
            for c in params.named_children(&mut cw) {
                if c.kind() == "identifier" && c.utf8_text(bytes).ok() == Some(name) {
                    return Some((c.start_byte(), c.end_byte()));
                }
            }
        }
    }
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

fn find_local_decl(node: &Node, bytes: &[u8], name: &str, level: LangLevel) -> Option<(usize, usize)> {
    let mut stack: Vec<Node> = vec![*node];
    while let Some(n) = stack.pop() {
        let mut cw = n.walk();
        for c in n.named_children(&mut cw) {
            // Don't descend into a nested TYPE or CALLABLE: its bindings (a nested class's
            // field, a lambda's own parameter) belong to a different scope and must not match.
            if c.id() != node.id()
                && matches!(
                    c.kind(),
                    "class_declaration"
                        | "interface_declaration"
                        | "enum_declaration"
                        | "record_declaration"
                        | "annotation_type_declaration"
                        | "method_declaration"
                        | "constructor_declaration"
                        | "compact_constructor_declaration"
                        | "lambda_expression"
                )
            {
                continue;
            }
            stack.push(c);
        }
        // Every shape that BINDS a simple name in a body scope. A `variable_declarator` under a
        // `field_declaration` is a field (not a local); every other shape exposes its binding
        // through a `name` field: `for (X x : …)` (enhanced-for), `catch (E e)`, a
        // try-with-resources `resource`, a stray formal/spread parameter, and — Java 16+,
        // gated — an `instanceof` / `switch` pattern variable (`o instanceof String s`).
        let bound = match n.kind() {
            "variable_declarator" => {
                if n.parent().map(|p| p.kind()) == Some("field_declaration") {
                    None
                } else {
                    n.child_by_field_name("name")
                }
            }
            "enhanced_for_statement"
            | "catch_formal_parameter"
            | "resource"
            | "formal_parameter"
            | "spread_parameter" => n.child_by_field_name("name"),
            // A pattern variable is a `type_pattern` in some grammar versions and a `name`
            // field on the `instanceof_expression` in others — accept both. (No `name` field
            // when there's no binding, e.g. a plain `o instanceof String`, so this is inert.)
            "type_pattern" | "instanceof_expression" if level.patterns() => {
                n.child_by_field_name("name")
            }
            _ => None,
        };
        if let Some(nm) = bound {
            if nm.utf8_text(bytes).ok() == Some(name) {
                return Some((nm.start_byte(), nm.end_byte()));
            }
        }
    }
    None
}

/// The smallest named node the caret at `offset` sits on — the identifier to classify.
///
/// A name spans `[start, end)`, so a caret at a name's START byte IS covered by it, but a
/// caret at its END byte is not — there we fall back to `offset - 1`. The identifier on
/// either adjacent byte wins over an enclosing expression, so a click anywhere on the token
/// resolves. (The old code biased to `offset - 1` UNCONDITIONALLY, so a click on the left
/// edge of a short name — a one/two-char local like `i`/`id` — landed on the previous token
/// and go-to silently missed ~half the time.) O(tree depth) via direct descent, not a full
/// scan.
fn smallest_named_at<'t>(root: &Node<'t>, offset: usize) -> Option<Node<'t>> {
    // Identifier directly under the caret (name start / mid-token).
    let on = root.named_descendant_for_byte_range(offset, offset);
    if let Some(n) = on {
        if is_ident_like(&n) {
            return Some(n);
        }
    }
    // Caret at a token's end byte (or between tokens) — the identifier is one byte left.
    if offset > 0 {
        if let Some(n) = root.named_descendant_for_byte_range(offset - 1, offset - 1) {
            if is_ident_like(&n) {
                return Some(n);
            }
        }
    }
    // Not on a name either side — hand back whatever covers the caret (the caret byte first,
    // then one left) so type/expression classification still has a node to work with.
    on.or_else(|| offset.checked_sub(1).and_then(|o| root.named_descendant_for_byte_range(o, o)))
}

/// A leaf the caret can "sit on" for go-to: a name token, `this`/`super`.
fn is_ident_like(n: &Node) -> bool {
    matches!(n.kind(), "identifier" | "type_identifier" | "this" | "super")
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
                            is_abstract: false,
                            is_default: false,
                            is_final: m.is_final,
                            visibility: Visibility::Public,
                            raw_signature: String::new(),
                            throws: Vec::new(),
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
                            is_abstract: false,
                            is_default: false,
                            is_final: f.is_final,
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
                            fields,
                            flags: Default::default(),
                        },
                    );
                }
            }
            simple.insert("String".into(), "java/lang/String".into());
            (Self { project, simple }, project_types)
        }
    }

    impl TypeResolver for SrcResolver {
        fn members_of(&self, binary: &str) -> Option<std::sync::Arc<bennu_java::prelude::ClassMembers>> {
            self.project.get(binary).cloned().map(std::sync::Arc::new)
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
        let t = classify_target(&index, "C.java", src, off, &resolver, &pt, LangLevel(0)).expect("classified");
        assert!(matches!(t, RenameTarget::Local { ref name, .. } if name == "x"));
    }

    #[test]
    fn goto_local_usage_at_name_start_byte() {
        // Caret at the START byte of a short local's USAGE — the case the old unconditional
        // `offset - 1` bias sent to the previous token, so go-to on a one/two-char local
        // silently missed. classify_target must resolve the local from its exact start.
        let src = "package p; public class C { int f() { int id = 1; return id; } }";
        let files = [("C.java", src)];
        let (index, resolver, pt) = index_of(&files);
        let usage = src.rfind("id").unwrap(); // the `id` in `return id`
        let t = classify_target(&index, "C.java", src, usage, &resolver, &pt, LangLevel(0)).expect("classified");
        assert!(matches!(t, RenameTarget::Local { ref name, .. } if name == "id"));
    }

    #[test]
    fn classify_bare_field_reference_resolves_to_field() {
        // A `this`-less field usage (`count` standing for `this.count`) must classify to the
        // FIELD, so go-to-declaration lands on the field decl instead of doing nothing / the
        // FE mis-jumping to a same-named class.
        let src = "package p; public class C { int count; int get() { return count; } }";
        let files = [("C.java", src)];
        let (index, resolver, pt) = index_of(&files);
        let off = src.find("return count").unwrap() + "return ".len();
        let key = classify_caret(&index, "C.java", src, off, &resolver, &pt).expect("classified");
        assert!(matches!(key, DeclKey::Field { ref owner, ref name } if owner == "p/C" && name == "count"));
    }

    #[test]
    fn goto_local_in_constructor_body_shadows_field() {
        // A CONSTRUCTOR body is a `constructor_body`, not a `block`. A local declared there
        // must resolve as a LOCAL (and shadow a same-named field) — the missing scope kind
        // made go-to fall through to the field for every constructor-local.
        let src = "package p; public class C { int ctx; C() { int ctx = 1; foo(ctx); } void foo(int x){} }";
        let files = [("C.java", src)];
        let (index, resolver, pt) = index_of(&files);
        let usage = src.rfind("ctx)").unwrap(); // the `ctx` in `foo(ctx)`
        let t = classify_target(&index, "C.java", src, usage, &resolver, &pt, LangLevel(0)).expect("classified");
        assert!(matches!(t, RenameTarget::Local { ref name, .. } if name == "ctx"));
    }

    #[test]
    fn goto_resolves_locals_in_all_body_scopes() {
        // enhanced-for, catch, and try-with-resources each bind a local through a `name` field
        // (NOT a `variable_declarator`) — all must still resolve as a local, like a plain block
        // local. (Also validates the tree-sitter node names for these shapes.)
        fn assert_local(src: &str, needle: &str, want: &str) {
            let files = [("C.java", src)];
            let (index, resolver, pt) = index_of(&files);
            let off = src.rfind(needle).unwrap();
            let t = classify_target(&index, "C.java", src, off, &resolver, &pt, LangLevel(0))
                .unwrap_or_else(|| panic!("not classified: {want}"));
            assert!(
                matches!(t, RenameTarget::Local { ref name, .. } if name == want),
                "want local {want}, got {t:?}"
            );
        }
        // enhanced-for variable
        assert_local(
            "package p; public class C { void m() { for (String s : x()) { echo(s); } } String[] x(){return null;} void echo(String a){} }",
            "s)",
            "s",
        );
        // catch parameter
        assert_local(
            "package p; public class C { void m() { try { r(); } catch (Exception e) { echo(e); } } void r(){} void echo(Object a){} }",
            "e)",
            "e",
        );
        // try-with-resources variable
        assert_local(
            "package p; public class C { void m() { try (java.io.Reader rd = o()) { echo(rd); } catch (Exception e) {} } java.io.Reader o(){return null;} void echo(Object a){} }",
            "rd)",
            "rd",
        );
    }

    #[test]
    fn goto_version_gated_bindings() {
        // Lambda inferred params, instanceof pattern variables, and record components resolve
        // as locals ONLY at the JDK level that introduced them (a lower level falls through —
        // e.g. so a same-named field can't be mistaken for a "component" in a Java-8 project).
        fn local_at(src: &str, needle: &str, level: LangLevel) -> Option<String> {
            let files = [("C.java", src)];
            let (index, resolver, pt) = index_of(&files);
            let off = src.rfind(needle).unwrap();
            match classify_target(&index, "C.java", src, off, &resolver, &pt, level) {
                Some(RenameTarget::Local { name, .. }) => Some(name),
                _ => None,
            }
        }

        // Lambda inferred parameter `(a, b) -> …` — Java 8+.
        let lam = "package p; public class C { java.util.function.BiFunction<Integer,Integer,Integer> f = (a, b) -> a + b; }";
        assert_eq!(local_at(lam, "a +", LangLevel(8)).as_deref(), Some("a"));
        assert_eq!(local_at(lam, "a +", LangLevel(7)), None);

        // instanceof pattern variable `o instanceof String s` — Java 16+.
        let pat = "package p; public class C { void m(Object o) { if (o instanceof String s) { echo(s); } } void echo(Object a){} }";
        assert_eq!(local_at(pat, "s)", LangLevel(17)).as_deref(), Some("s"));
        assert_eq!(local_at(pat, "s)", LangLevel(11)), None);

        // A record component is NOT a local, at any level. It used to classify as one, and that is
        // what made renaming it produce broken code: a local rename is bounded by the record's own
        // scope, while a component also declares an accessor that callers use as `r.x()`. It is a
        // member, so the member classifier answers for it and the rename reaches the accessor.
        let rec = "package p; public record R(int x) { R { echo(x); } static void echo(int a){} }";
        assert_eq!(local_at(rec, "x)", LangLevel(17)), None);
        assert_eq!(local_at(rec, "x)", LangLevel(11)), None);

        // Unknown level (0) enables everything — never break go-to when the JDK wasn't detected.
        assert_eq!(local_at(pat, "s)", LangLevel(0)).as_deref(), Some("s"));
    }

    #[test]
    fn unresolved_receiver_never_panics() {
        let files = [("X.java", "package p; public class X { void m(Unknown u) { u.frob(); } }")];
        let (index, _r, _pt) = index_of(&files);
        let _ = index.declared_with_usages();
    }

    #[test]
    fn incremental_cache_reuses_unchanged_and_rewalks_changed() {
        let key = DeclKey::Method { owner: "p/A".into(), name: "val".into() };
        let v1 = [
            ("A.java", "package p; public class A { public int val() { return 1; } }"),
            ("B.java", "package p; public class B { public int use(A a) { return a.val(); } }"),
        ];
        let (resolver, pt) = SrcResolver::build(&v1);
        let src1: Vec<SourceFile> =
            v1.iter().map(|(p, s)| SourceFile { path: p.to_string(), source: s.to_string() }).collect();

        // First build: no cache → full walk, one usage of A.val (in B), cache produced.
        let b1 = build_reference_index_incremental(&src1, &resolver, &pt, None, &|_, _| {});
        assert_eq!(b1.index.usages_of(&key).len(), 1);
        let cache = b1.cache_to_save.expect("first build yields a cache");

        // Rebuild, nothing changed → nothing re-walked, and no rewrite of the on-disk cache.
        let b2 = build_reference_index_incremental(&src1, &resolver, &pt, Some(cache.clone()), &|_, _| {});
        assert!(b2.cache_to_save.is_none(), "unchanged project must not rewrite the cache");
        assert_eq!(b2.index.usages_of(&key).len(), 1);

        // Change B (now calls val() twice); A untouched. Same type set → incremental path:
        // B is re-walked, A reused, and the merged index reflects B's two usages.
        let v2 = [
            ("A.java", "package p; public class A { public int val() { return 1; } }"),
            ("B.java", "package p; public class B { public int use(A a) { return a.val() + a.val(); } }"),
        ];
        let src2: Vec<SourceFile> =
            v2.iter().map(|(p, s)| SourceFile { path: p.to_string(), source: s.to_string() }).collect();
        let b3 = build_reference_index_incremental(&src2, &resolver, &pt, Some(cache), &|_, _| {});
        assert!(b3.cache_to_save.is_some(), "a changed file must refresh the cache");
        assert_eq!(b3.index.usages_of(&key).len(), 2);
    }

    #[test]
    fn project_type_map_seeds_classification() {
        // sanity: the java_index helper produces the same simple→binary shape the walk uses
        let _ = project_type_map(
            std::path::Path::new("."),
            &bennu_project::prelude::EncodingPlan::uniform("UTF-8"),
        );
    }

    #[test]
    fn file_declaring_accepts_the_jvm_spelling_of_a_nested_type() {
        // The project model spells a nested type `p.Outer.Inner`; the JVM spells it
        // `p/Outer$Failure`, and any binary that came back from bytecode carries the `$`.
        // Rejecting that form meant a compiled project's OWN nested type was not recognised as
        // project code: go-to opened a decompiled stub of the user's record instead of the record,
        // and a rename found no source to edit.
        let files = [(
            "Outer.java",
            "package p; public class Outer { private record Failure(String source_path) {} }",
        )];
        let (index, _r, _pt) = index_of(&files);

        assert_eq!(index.file_declaring("p/Outer/Failure"), Some("Outer.java"), "the source form");
        assert_eq!(index.file_declaring("p/Outer$Failure"), Some("Outer.java"), "the JVM form");
        // A type nobody declares is still unknown, whichever way it is spelled.
        assert_eq!(index.file_declaring("p/Outer$Missing"), None);
        assert_eq!(index.file_declaring("java/util/Map$Entry"), None);
    }
}
