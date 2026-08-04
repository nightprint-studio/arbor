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

    /// `record` components (readable in the compact constructor) — Java 16.
    fn records(self) -> bool {
        self.at_least(16)
    }

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
    pub fn file_declaring(&self, type_binary: &str) -> Option<&str> {
        let dotted = type_binary.replace('/', ".");
        self.file_symbols
            .iter()
            .find(|(_, fs)| fs.types.iter().any(|t| t.fqn == dotted))
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
    attempted: usize,
    resolved: usize,
    /// The file's `import`s, set by [`walk`](Self::walk) before it starts. A dependency type
    /// is reachable by its simple name **only** through these — see `resolve_type_simple`.
    imports: Vec<bennu_java::prelude::Import>,
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
            imports: Vec::new(),
        }
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
        let mut stack = vec![*root];
        while let Some(n) = stack.pop() {
            let mut cur = n.walk();
            for c in n.named_children(&mut cur) {
                stack.push(c);
            }
            match n.kind() {
                "method_invocation" => self.on_method_invocation(&n, root, symbols),
                "field_access" => self.on_field_access(&n, root, symbols),
                "type_identifier" => self.on_type_identifier(&n),
                _ => {}
            }
        }
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
        self.edges.push((DeclKey::Method { owner, name }, usage));
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
                // `cm` is a shared `Arc` — clone the (small) supertype links, don't move.
                if let Some(sc) = cm.superclass.clone() {
                    stack.push(sc);
                }
                stack.extend(cm.interfaces.iter().cloned());
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
        // WITH the file's imports. A project type resolves off `project_types` above, but a
        // **dependency** type only ever resolves through the `import` that named it — the
        // resolver's simple-name hints hold the project's own types and common JDK names, not
        // every class on the classpath. Passing an empty list here meant no use of a library
        // class was ever indexed, so find-usages on one reported nothing at all.
        self.resolver.resolve_simple_name(simple, &self.imports)
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
            // Not a project type — which is the normal state inside a LIBRARY source view,
            // where the file being read belongs to a dependency. The buffer says which
            // package it is in, so use it: the bare simple name this used to fall back to
            // could never match `com/acme/SharedService`, the key every use site was indexed
            // under, so a caret anywhere inside such a file classified to a key that existed
            // nowhere and find-usages reported nothing.
            return Some(match buffer_package(bytes) {
                Some(pkg) => format!("{}/{name}", pkg.replace('.', "/")),
                None => name,
            });
        }
        cur = n.parent();
    }
    None
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
        let pkg = rest.trim_end_matches(';').trim();
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
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    let bytes = source.as_bytes();

    let ident = smallest_named_at(&root, offset)?;
    if ident.kind() == "identifier" && !is_member_selector_node(&ident) {
        let name = ident.utf8_text(bytes).ok()?.to_string();
        if let Some((ds, de)) = find_local_binding(&ident, bytes, &name, level) {
            return Some(RenameTarget::Local { name, def_start: ds, def_end: de });
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
        // A `record`'s components (its header `formal_parameters`) are in scope in the record's
        // members and its compact constructor. Java 16+ — gated so a same-named field can't be
        // mistaken for a "component" in an older project. Then STOP (a type boundary).
        if n.kind() == "record_declaration" {
            if level.records() {
                if let Some(p) = find_param_decl(&n, bytes, name, level) {
                    return Some(p);
                }
            }
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

        // Record component used in the compact constructor — Java 16+.
        let rec = "package p; public record R(int x) { R { echo(x); } static void echo(int a){} }";
        assert_eq!(local_at(rec, "x)", LangLevel(17)).as_deref(), Some("x"));
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
        let _ = project_type_map(std::path::Path::new("."), "UTF-8");
    }
}
