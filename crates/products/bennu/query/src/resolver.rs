//! [`IndexResolver`] — the [`bennu_java`] [`TypeResolver`] backed by the persisted
//! project index (source types) + a JDK member index (bytecode types).
//!
//! This is the boundary the design flagged: `bennu-classpath` produces its OWN
//! `ClassMembers`/`Member`/`TypeRef` (the bytecode-decoded seam), and `bennu-java`
//! consumes ITS OWN `ClassMembers` (the resolver seam) — same *shape*, distinct
//! *types*. The resolver converts one into the other in [`convert_members`].
//!
//! Resolution order for `members_of(binary)`:
//!   1. the project index (a `.java`-declared type, `members_json` off the record) —
//!      mutable, patched per file;
//!   2. the JDK member index (rt.jar / jimage) — immutable, resolved live.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::dep_record;
use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_index::prelude::{PersistedIndex, Symbol, SymbolKind};
use bennu_java::prelude::{
    ClassFlags as JClassFlags, ClassMembers as JClassMembers, Import, Member as JMember,
    MemberKind as JMemberKind, TypeRef as JTypeRef, TypeResolver, Visibility as JVisibility,
};

/// A [`TypeResolver`] composing the persisted project index and a JDK `MemberIndex`,
/// with an **in-memory overlay** for files edited since the last full build. The overlay
/// is consulted before the persisted mmap so a keystroke's fresh members are visible to
/// completion without re-persisting the (memory-mapped) index files.
pub struct IndexResolver<M: CpMemberIndex> {
    project: PersistedIndex,
    jdk: M,
    /// Simple-name → binary-name hints (the project's own types + common JDK names),
    /// so `resolve_simple_name` works even without an explicit import.
    simple_hints: HashMap<String, String>,
    /// The in-memory patch overlay for files edited since the last full build
    /// (interior-mutable so a patch mutates the live, `Arc`-shared provider in place).
    overlay: RwLock<Overlay>,
    /// Memoized `members_of` results (incl. negative — `None` — hits). The reference-index
    /// walk resolves the same types (`String`, `List`, project base classes, …) tens of
    /// thousands of times; without this each call re-parses the members JSON or re-reads the
    /// class bytecode, which made the walk take many minutes on a large project. Cleared when
    /// the overlay changes ([`apply_file_patch`](Self::apply_file_patch)), since an edit can
    /// change a type's members.
    members_cache: RwLock<HashMap<String, Option<Arc<JClassMembers>>>>,
    /// When set, `members_of` resolves ONLY project types — it never decodes JDK / library
    /// bytecode. Used by the reference / rename engine: a use site on a JDK member is never
    /// queried by find-usages / rename (you can't rename it), so decoding the JDK for it is
    /// pure waste that made the whole reference walk crawl. The provider (completion) keeps
    /// full JDK resolution (a separate resolver instance).
    project_only: bool,
}

/// The edited-file overlay: a binary→members-JSON lookup for the resolver, plus a
/// simple→binary hint map, plus a per-file record of what each file contributed (so a
/// re-patch of the same file drops that file's stale entries — rename/remove correctness).
#[derive(Default)]
struct Overlay {
    /// binary-name → resolved members JSON. `Some` overrides the persisted record; an
    /// absent key falls through to the mmap.
    members: HashMap<String, String>,
    /// simple-name → binary-name for edited-file types (a renamed/added type).
    simple: HashMap<String, String>,
    /// file key → the (binary, simple) names that file currently contributes, so the next
    /// patch of the same file drops exactly its prior overlay entries.
    by_file: HashMap<String, Vec<(String, String)>>,
}

impl<M: CpMemberIndex> IndexResolver<M> {
    /// Build the resolver over a persisted project index + a JDK member index.
    pub fn new(project: PersistedIndex, jdk: M) -> Self {
        let mut simple_hints = HashMap::new();
        for (s, b) in COMMON_SIMPLE {
            simple_hints.insert((*s).to_string(), (*b).to_string());
        }
        Self {
            project,
            jdk,
            simple_hints,
            overlay: RwLock::new(Overlay::default()),
            members_cache: RwLock::new(HashMap::new()),
            project_only: false,
        }
    }

    /// Restrict this resolver to PROJECT types only — `members_of` returns `None` for a JDK /
    /// library type instead of decoding its bytecode. For the reference / rename engine, where
    /// resolving JDK receivers is wasted work (their edges are never queried). The provider
    /// (completion) does NOT call this, so it keeps full JDK resolution.
    pub fn project_only(mut self) -> Self {
        self.project_only = true;
        self
    }

    /// Seed a simple→binary hint (e.g. the project's own declared types).
    pub fn add_simple_hint(&mut self, simple: &str, binary: &str) {
        self.simple_hints.insert(simple.to_string(), binary.to_string());
    }

    /// The persisted project index (for the completion query's prefix search).
    pub fn project(&self) -> &PersistedIndex {
        &self.project
    }

    /// The JDK member index (e.g. to flush its persistent memo at a checkpoint).
    pub fn jdk_index(&self) -> &M {
        &self.jdk
    }

    /// Every project member (`Method` / `Field`) symbol in the persisted index, deduped
    /// by symbol id (a member is reachable under one fst key, but this stays robust to a
    /// future alias). For the index inspector's members list — a read-only enumeration of
    /// the already-built index, no source re-parse. The overlay (unsaved edits) is NOT
    /// consulted: the inspector reflects the last full build, like the symbol counts.
    pub fn member_symbols(&self) -> Vec<Symbol> {
        let mut seen = std::collections::HashSet::new();
        self.project
            .prefix("")
            .into_iter()
            .filter(|s| matches!(s.kind, SymbolKind::Method | SymbolKind::Field))
            .filter(|s| seen.insert(s.id))
            .collect()
    }

    /// The project-only resolved members-JSON for `binary` (an edited-file overlay override wins,
    /// else the persisted project record). `None` when `binary` isn't a PROJECT type — the JDK /
    /// library bytecode is **never** consulted here. This is the mutable surface the diagnostic
    /// cache fingerprints: two validations of a file agree iff this string is unchanged for every
    /// project type the file read.
    fn project_members_json(&self, binary: &str) -> Option<String> {
        {
            let ov = self.overlay.read().unwrap_or_else(|p| p.into_inner());
            if let Some(json) = ov.members.get(binary) {
                return Some(json.clone());
            }
        }
        let sym = self.project.get(binary)?;
        (!sym.members_json.is_empty()).then_some(sym.members_json)
    }

    /// A stable hash of `binary`'s project members-JSON, or `None` when it isn't a project type —
    /// the per-dependency fingerprint the diagnostic cache stores and re-checks. Mirrors exactly
    /// the project branch of [`members_of`](TypeResolver::members_of) (overlay → persisted, no
    /// JDK), so a recorded dependency and its freshness check read the same source of truth.
    pub fn dep_signature(&self, binary: &str) -> Option<u64> {
        self.project_members_json(binary).map(|j| dep_record::fnv1a(j.as_bytes()))
    }

    /// The project binary a bare `simple` name resolves to (an overlay-added type wins, else a
    /// persisted project type of that simple name). `None` when no PROJECT type has that simple
    /// name. Mirrors the project branch of
    /// [`resolve_simple_name`](TypeResolver::resolve_simple_name) (minus imports / JDK), so a
    /// recorded "simple hit" and its freshness check agree by construction.
    pub fn project_simple(&self, simple: &str) -> Option<String> {
        {
            let ov = self.overlay.read().unwrap_or_else(|p| p.into_inner());
            if let Some(binary) = ov.simple.get(simple) {
                return Some(binary.clone());
            }
        }
        let sym = self.project.get(simple)?;
        (!sym.fqn.is_empty()).then_some(sym.fqn)
    }

    /// Whether `key` names a PROJECT type — as a binary name OR a simple name. The diagnostic
    /// cache's negative-dependency check: a recorded miss on `key` stays valid only while this is
    /// `false` (a project type appearing under that name invalidates the cached file).
    pub fn project_contains(&self, key: &str) -> bool {
        self.dep_signature(key).is_some() || self.project_simple(key).is_some()
    }

    /// Apply one edited `file`'s freshly-extracted [`Symbol`] records to the in-memory
    /// overlay — **no disk write**. The overlay shadows the persisted mmap so completion
    /// on the edited file reflects the edit immediately, while the (memory-mapped)
    /// `symbols.blob` / `names.fst` are left untouched until the next full build (which
    /// swaps in a brand-new provider and clears the overlay).
    ///
    /// The file's PRIOR overlay entries (tracked internally, keyed by `file`) are dropped
    /// first, so a renamed/removed type doesn't leave a stale entry. An empty `records`
    /// (a deleted / cleared file) just drops the file's prior overlay.
    pub fn apply_file_patch(&self, file: &str, records: &[Symbol]) {
        // An edit can change any type's members → drop the memoized resolutions. Patches are
        // rare (debounced per keystroke) and the walk is done, so a full clear is fine; it
        // repopulates lazily on the next resolution.
        self.members_cache.write().unwrap_or_else(|p| p.into_inner()).clear();
        let mut ov = self.overlay.write().unwrap_or_else(|p| p.into_inner());
        // Drop this file's previous contributions (rename/remove correctness).
        if let Some(prev) = ov.by_file.remove(file) {
            for (binary, simple) in prev {
                ov.members.remove(&binary);
                // Only clear the simple hint if it still points at this file's type.
                if ov.simple.get(&simple).map(String::as_str) == Some(binary.as_str()) {
                    ov.simple.remove(&simple);
                }
            }
        }
        // Add the fresh type records (only Class symbols carry members_json).
        let mut contributed = Vec::new();
        for sym in records {
            if sym.members_json.is_empty() {
                continue;
            }
            ov.members.insert(sym.fqn.clone(), sym.members_json.clone());
            if !sym.simple_name.is_empty() {
                ov.simple.insert(sym.simple_name.clone(), sym.fqn.clone());
            }
            contributed.push((sym.fqn.clone(), sym.simple_name.clone()));
        }
        if !contributed.is_empty() {
            ov.by_file.insert(file.to_string(), contributed);
        }
    }
}

impl<M: CpMemberIndex> IndexResolver<M> {
    /// Resolve a type's members from the overlay → persisted project index → JDK bytecode,
    /// WITHOUT the memo cache. The uncached core [`members_of`](TypeResolver::members_of)
    /// wraps for hot re-resolution of the same types.
    fn compute_members(&self, binary_name: &str) -> Option<JClassMembers> {
        // 0) in-memory overlay for a file edited since the last full build — wins over
        //    the persisted mmap so a keystroke's fresh members are visible without a
        //    re-persist of the memory-mapped index files.
        {
            let ov = self.overlay.read().unwrap_or_else(|p| p.into_inner());
            if let Some(json) = ov.members.get(binary_name) {
                if let Ok(cm) = serde_json::from_str::<JClassMembers>(json) {
                    return Some(cm);
                }
            }
        }
        // 1) project source type — its resolved members are baked into the record.
        if let Some(sym) = self.project.get(binary_name) {
            if !sym.members_json.is_empty() {
                if let Ok(cm) = serde_json::from_str::<JClassMembers>(&sym.members_json) {
                    return Some(cm);
                }
            }
        }
        // 2) JDK bytecode type (converted from the classpath seam) — skipped entirely for a
        //    project-only resolver (the reference/rename engine), which never needs it.
        if self.project_only {
            return None;
        }
        let cp = self.jdk.members_of(binary_name)?;
        Some(convert_members(&cp))
    }
}

impl<M: CpMemberIndex> TypeResolver for IndexResolver<M> {
    fn members_of(&self, binary_name: &str) -> Option<Arc<JClassMembers>> {
        // Record this file's dependency on the project type `binary_name` when a validation
        // recording scope is active — present → its members hash, absent → a negative dep. Done
        // here (not in `compute_members`) so a memo hit still records the dependency, and gated by
        // the cheap `recording()` flag so it's a no-op on the (hot) reference-walk path.
        if dep_record::recording() {
            dep_record::note_type(binary_name, self.dep_signature(binary_name));
        }
        // Memo hit (incl. a cached negative) — skips the JSON parse / bytecode read AND the
        // deep clone: on a hit we hand back a clone of the shared `Arc` (a refcount bump),
        // not a copy of every method/field. This is what makes the reference walk tractable.
        {
            let cache = self.members_cache.read().unwrap_or_else(|p| p.into_inner());
            if let Some(hit) = cache.get(binary_name) {
                return hit.clone();
            }
        }
        // Miss — resolve once, then memoize (the walk re-asks for the same types constantly).
        let computed = self.compute_members(binary_name).map(Arc::new);
        self.members_cache
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .insert(binary_name.to_string(), computed.clone());
        computed
    }

    fn resolve_simple_name(&self, name: &str, imports: &[Import]) -> Option<String> {
        // Imports win (a `java.util.List` import binds `List`).
        for imp in imports {
            if imp.simple_name() == Some(name) {
                return Some(imp.path.replace('.', "/"));
            }
        }
        // From here on we probe the PROJECT for `name`: record the outcome (hit / miss) for the
        // diagnostic cache when recording. An import-bound name (above) is project-independent, so
        // it was deliberately not recorded. A miss recorded here means "a project type appearing
        // under this name must invalidate the file" (the negative dependency).
        let recording = dep_record::recording();
        // An edited file's own (possibly renamed/added) type overrides the stale mmap.
        {
            let ov = self.overlay.read().unwrap_or_else(|p| p.into_inner());
            if let Some(binary) = ov.simple.get(name) {
                if recording {
                    dep_record::note_simple_hit(name, binary);
                }
                return Some(binary.clone());
            }
        }
        // Then a project type of that simple name, then the common-JDK table.
        if let Some(sym) = self.project.get(name) {
            if !sym.fqn.is_empty() {
                if recording {
                    dep_record::note_simple_hit(name, &sym.fqn);
                }
                return Some(sym.fqn.clone());
            }
        }
        // The project has no type named `name` → a negative dependency (a future project type of
        // this name would resolve here first, shadowing the JDK fall-through below).
        if recording {
            dep_record::note_simple_miss(name);
        }
        if let Some(hint) = self.simple_hints.get(name) {
            return Some(hint.clone());
        }
        // Fall through to the JDK / library bytecode: `java.lang` is implicitly imported, and a
        // non-static star import (`import pkg.*;`) can supply the type. Probe the member index —
        // fast now (the resolver's per-name memo + the persistent JDK memo). A hit means the type
        // genuinely EXISTS, so `None` here is a real "cannot resolve" — the definitive answer the
        // validator's unresolved-type check needs. Skipped in `project_only` mode (the reference /
        // rename engine never resolves JDK receivers, so decoding bytecode for them is waste).
        if self.project_only {
            return None;
        }
        let java_lang = format!("java/lang/{name}");
        if self.jdk.members_of(&java_lang).is_some() {
            return Some(java_lang);
        }
        for imp in imports {
            if imp.star && !imp.static_ {
                let candidate = format!("{}/{name}", imp.path.replace('.', "/"));
                if self.jdk.members_of(&candidate).is_some() {
                    return Some(candidate);
                }
            }
        }
        None
    }
}

/// The read-only view of the mutable project surface the diagnostic cache checks a file's
/// recorded dependencies against. Implemented by [`IndexResolver`]; mockable in tests so the
/// cache's freshness logic is unit-testable without a live index. Every method is project-only
/// (never the JDK / library bytecode — those are guarded by the cache's classpath epoch).
pub trait ProjectView {
    /// A stable hash of `binary`'s project members-JSON, or `None` when it isn't a project type.
    fn dep_signature(&self, binary: &str) -> Option<u64>;
    /// The project binary a bare `simple` name resolves to, or `None` when no project type has it.
    fn project_simple(&self, simple: &str) -> Option<String>;
    /// Whether `key` names a project type as a binary OR a simple name.
    fn project_contains(&self, key: &str) -> bool;
}

impl<M: CpMemberIndex> ProjectView for IndexResolver<M> {
    fn dep_signature(&self, binary: &str) -> Option<u64> {
        IndexResolver::dep_signature(self, binary)
    }
    fn project_simple(&self, simple: &str) -> Option<String> {
        IndexResolver::project_simple(self, simple)
    }
    fn project_contains(&self, key: &str) -> bool {
        IndexResolver::project_contains(self, key)
    }
}

/// Convert a [`bennu_classpath`] `ClassMembers` into a [`bennu_java`] seam
/// `ClassMembers`. Field-by-field over identical shapes across the crate boundary.
pub fn convert_members(cp: &bennu_classpath::prelude::ClassMembers) -> JClassMembers {
    JClassMembers {
        superclass: cp.superclass.clone(),
        interfaces: cp.interfaces.clone(),
        methods: cp.methods.iter().map(convert_member).collect(),
        fields: cp.fields.iter().map(convert_member).collect(),
        flags: convert_flags(&cp.flags),
    }
}

fn convert_flags(f: &bennu_classpath::prelude::ClassFlags) -> JClassFlags {
    JClassFlags {
        is_interface: f.is_interface,
        is_abstract: f.is_abstract,
        is_final: f.is_final,
        is_enum: f.is_enum,
        is_annotation: f.is_annotation,
        is_record: f.is_record,
        is_sealed: f.is_sealed,
    }
}

fn convert_member(m: &bennu_classpath::prelude::Member) -> JMember {
    JMember {
        name: m.name.clone(),
        kind: match m.kind {
            bennu_classpath::prelude::MemberKind::Method => JMemberKind::Method,
            bennu_classpath::prelude::MemberKind::Field => JMemberKind::Field,
        },
        return_type: convert_typeref(&m.return_type),
        params: m.params.iter().map(convert_typeref).collect(),
        is_static: m.is_static,
        is_abstract: m.is_abstract,
        is_default: m.is_default,
        is_final: m.is_final,
        visibility: match m.visibility {
            bennu_classpath::prelude::Visibility::Public => JVisibility::Public,
            bennu_classpath::prelude::Visibility::Protected => JVisibility::Protected,
            bennu_classpath::prelude::Visibility::Private => JVisibility::Private,
            bennu_classpath::prelude::Visibility::Package => JVisibility::Package,
        },
        raw_signature: m.raw_signature.clone(),
    }
}

fn convert_typeref(t: &bennu_classpath::prelude::TypeRef) -> JTypeRef {
    JTypeRef {
        binary_name: t.binary_name.clone(),
        type_args: t.type_args.iter().map(convert_typeref).collect(),
    }
}

/// A small simple→binary table for the ubiquitous JDK names, so bare `String`/`List`/…
/// resolve even without an explicit import (java.lang is implicitly imported; the
/// common java.util collections are everywhere in the target stack).
const COMMON_SIMPLE: &[(&str, &str)] = &[
    ("String", "java/lang/String"),
    ("Object", "java/lang/Object"),
    ("Integer", "java/lang/Integer"),
    ("Long", "java/lang/Long"),
    ("Boolean", "java/lang/Boolean"),
    ("CharSequence", "java/lang/CharSequence"),
    ("List", "java/util/List"),
    ("ArrayList", "java/util/ArrayList"),
    ("Map", "java/util/Map"),
    ("HashMap", "java/util/HashMap"),
    ("Set", "java/util/Set"),
    ("Collection", "java/util/Collection"),
    ("Iterator", "java/util/Iterator"),
    ("Optional", "java/util/Optional"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_classpath::prelude::ClassMembers as CpClassMembers;
    use bennu_index::prelude::{IndexBuilder, IndexRecord, Source, SymbolKind};
    use std::path::PathBuf;

    /// A JDK stub that resolves nothing — the overlay/persisted-index precedence under
    /// test never needs the JDK fall-through, so this keeps the test free of a live JDK.
    struct NoJdk;
    impl CpMemberIndex for NoJdk {
        fn members_of(&self, _binary_name: &str) -> Option<CpClassMembers> {
            None
        }
    }

    /// A JDK stub that resolves a couple of well-known bytecode types (NOT in `COMMON_SIMPLE`), so
    /// the `resolve_simple_name` bytecode fall-through (java.lang + star imports) can be tested.
    struct FakeJdk;
    impl CpMemberIndex for FakeJdk {
        fn members_of(&self, binary_name: &str) -> Option<CpClassMembers> {
            matches!(binary_name, "java/lang/Runnable" | "java/util/LinkedHashMap").then(|| {
                CpClassMembers {
                    superclass: None,
                    interfaces: Vec::new(),
                    methods: Vec::new(),
                    fields: Vec::new(),
                    flags: Default::default(),
                }
            })
        }
    }

    /// A resolver with an EMPTY project index over a given JDK stub — for the simple-name
    /// resolution fall-through tests (no project types, so the JDK probe is what answers).
    fn empty_resolver_with_jdk<M: CpMemberIndex>(jdk: M) -> IndexResolver<M> {
        let dir = std::env::temp_dir().join(format!(
            "bennu-jdkprobe-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut b = IndexBuilder::new(&dir);
        b.persist().unwrap(); // empty index → project.get always misses
        let project = PersistedIndex::open(b.blob_path(), b.fst_path()).unwrap();
        IndexResolver::new(project, jdk)
    }

    #[test]
    fn resolves_java_lang_type_via_bytecode_probe() {
        let r = empty_resolver_with_jdk(FakeJdk);
        // `Runnable` isn't in COMMON_SIMPLE → only the java.lang bytecode probe resolves it.
        assert_eq!(r.resolve_simple_name("Runnable", &[]).as_deref(), Some("java/lang/Runnable"));
        // A genuinely unknown name → None. This is the DEFINITIVE answer the validator relies on.
        assert!(r.resolve_simple_name("Nope", &[]).is_none());
    }

    #[test]
    fn resolves_type_via_non_static_star_import() {
        let r = empty_resolver_with_jdk(FakeJdk);
        let imports = vec![Import { path: "java.util".into(), star: true, static_: false }];
        assert_eq!(
            r.resolve_simple_name("LinkedHashMap", &imports).as_deref(),
            Some("java/util/LinkedHashMap"),
        );
        // Without the star import, java.util.LinkedHashMap isn't implicitly available → None.
        assert!(r.resolve_simple_name("LinkedHashMap", &[]).is_none());
        // A STATIC star import doesn't bind a type name → still None.
        let static_star = vec![Import { path: "java.util".into(), star: true, static_: true }];
        assert!(r.resolve_simple_name("LinkedHashMap", &static_star).is_none());
    }

    #[test]
    fn project_only_skips_the_jdk_probe() {
        let r = empty_resolver_with_jdk(FakeJdk).project_only();
        // The reference/rename engine never resolves JDK receivers — even a real java.lang type
        // must not resolve here (it's wasted bytecode decoding for a use-site we can't rename).
        assert!(r.resolve_simple_name("Runnable", &[]).is_none());
    }

    /// A class `Symbol` carrying a members-JSON, reachable under its binary name.
    fn class_symbol(simple: &str, binary: &str, members_json: &str) -> Symbol {
        Symbol {
            id: 0,
            kind: SymbolKind::Class,
            simple_name: simple.to_string(),
            fqn: binary.to_string(),
            owner_id: u32::MAX,
            source: Source::ProjectSource,
            signature: format!("class {simple}"),
            modifiers: String::new(),
            loc_file: format!("{simple}.java"),
            loc_start: 0,
            loc_end: 0,
            loc_container: String::new(),
            loc_class: String::new(),
            members_json: members_json.to_string(),
        }
    }

    /// A members-JSON blob with a single field of the given name, so `members_of` returns
    /// a distinguishable `ClassMembers`.
    fn members_json_with_field(field: &str) -> String {
        serde_json::to_string(&JClassMembers {
            superclass: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: vec![JMember {
                name: field.to_string(),
                kind: JMemberKind::Field,
                return_type: JTypeRef { binary_name: "int".into(), type_args: Vec::new() },
                params: Vec::new(),
                is_static: false,
                is_abstract: false,
                is_default: false,
                is_final: false,
                visibility: JVisibility::Public,
                raw_signature: format!("int {field}"),
            }],
            flags: Default::default(),
        })
        .unwrap()
    }

    /// Build a persisted index with one class type, then a resolver over it + `NoJdk`.
    fn resolver_with(binary: &str, simple: &str, members_json: &str) -> IndexResolver<NoJdk> {
        let dir = std::env::temp_dir().join(format!(
            "bennu-overlay-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut b = IndexBuilder::new(&dir);
        b.set_file(
            PathBuf::from("Order.java"),
            vec![IndexRecord::new(class_symbol(simple, binary, members_json), simple.to_string())
                .with_key(binary.to_string())],
        );
        b.persist().unwrap();
        let project = PersistedIndex::open(b.blob_path(), b.fst_path()).unwrap();
        IndexResolver::new(project, NoJdk)
    }

    #[test]
    fn overlay_shadows_persisted_members() {
        // Persisted index says `com/acme/Order` has field `oldField`.
        let r = resolver_with("com/acme/Order", "Order", &members_json_with_field("oldField"));
        let before = r.members_of("com/acme/Order").expect("persisted members");
        assert_eq!(before.fields[0].name, "oldField");

        // A keystroke adds `newField` to Order (a new members-JSON) via the overlay.
        let patched = class_symbol("Order", "com/acme/Order", &members_json_with_field("newField"));
        r.apply_file_patch("src/Order.java", &[patched]);
        let after = r.members_of("com/acme/Order").expect("overlay members");
        assert_eq!(after.fields[0].name, "newField", "overlay wins over the mmap");
    }

    #[test]
    fn overlay_rename_drops_stale_entry() {
        let r = resolver_with("com/acme/Order", "Order", &members_json_with_field("f"));
        // First patch registers Order under the file.
        r.apply_file_patch(
            "src/Order.java",
            &[class_symbol("Order", "com/acme/Order", &members_json_with_field("f"))],
        );
        assert!(r.members_of("com/acme/Order").is_some());
        // Second patch of the SAME file renames the type to Invoice → Order's overlay
        // entry must be dropped (it now falls back to the persisted record, still present),
        // and the new binary resolves from the overlay.
        r.apply_file_patch(
            "src/Order.java",
            &[class_symbol("Invoice", "com/acme/Invoice", &members_json_with_field("g"))],
        );
        // Invoice resolves via overlay simple-name hint.
        assert_eq!(r.resolve_simple_name("Invoice", &[]).as_deref(), Some("com/acme/Invoice"));
        let inv = r.members_of("com/acme/Invoice").expect("renamed type in overlay");
        assert_eq!(inv.fields[0].name, "g");
    }

    /// A member `Symbol` (method or field) keyed by its simple name, mirroring what the
    /// java-index build emits for the search-everywhere axis.
    fn member_symbol(id: u32, kind: SymbolKind, name: &str, owner_binary: &str, sig: &str) -> Symbol {
        Symbol {
            id,
            kind,
            simple_name: name.to_string(),
            fqn: owner_binary.to_string(),
            owner_id: 0,
            source: Source::ProjectSource,
            signature: sig.to_string(),
            modifiers: String::new(),
            loc_file: "Order.java".to_string(),
            loc_start: 0,
            loc_end: 0,
            loc_container: String::new(),
            loc_class: String::new(),
            members_json: String::new(),
        }
    }

    /// `member_symbols()` enumerates only Method/Field records (not the Class type record),
    /// deduped by id — the members-list source for the index inspector.
    #[test]
    fn member_symbols_lists_methods_and_fields_only() {
        let dir = std::env::temp_dir().join(format!(
            "bennu-members-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut b = IndexBuilder::new(&dir);
        b.set_file(
            PathBuf::from("Order.java"),
            vec![
                // The type record (Class) — must NOT appear in member_symbols().
                IndexRecord::new(
                    class_symbol("Order", "com/acme/Order", "{}"),
                    "Order".to_string(),
                )
                .with_key("com/acme/Order".to_string()),
                // A method + a field record.
                IndexRecord::new(
                    member_symbol(1, SymbolKind::Method, "getId", "com/acme/Order", "long getId()"),
                    "getId".to_string(),
                ),
                IndexRecord::new(
                    member_symbol(2, SymbolKind::Field, "id", "com/acme/Order", "long id"),
                    "id".to_string(),
                ),
            ],
        );
        b.persist().unwrap();
        let project = PersistedIndex::open(b.blob_path(), b.fst_path()).unwrap();
        let r = IndexResolver::new(project, NoJdk);

        let mut members = r.member_symbols();
        members.sort_by(|a, b| a.simple_name.cmp(&b.simple_name));
        assert_eq!(members.len(), 2, "only the method + field, not the Class type");
        assert_eq!(members[0].simple_name, "getId");
        assert!(matches!(members[0].kind, SymbolKind::Method));
        assert_eq!(members[0].fqn, "com/acme/Order");
        assert_eq!(members[0].signature, "long getId()");
        assert_eq!(members[1].simple_name, "id");
        assert!(matches!(members[1].kind, SymbolKind::Field));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overlay_delete_clears_file_entries() {
        let r = resolver_with("com/acme/Order", "Order", &members_json_with_field("f"));
        r.apply_file_patch(
            "src/Order.java",
            &[class_symbol("Order2", "com/acme/Order2", &members_json_with_field("h"))],
        );
        assert!(r.members_of("com/acme/Order2").is_some());
        // Delete (empty records) drops the file's overlay contributions.
        r.apply_file_patch("src/Order.java", &[]);
        assert!(r.members_of("com/acme/Order2").is_none(), "deleted file's overlay cleared");
    }

    // ── project-view queries (the diagnostic cache's freshness source of truth) ──────────────

    #[test]
    fn project_queries_distinguish_project_types_from_absent() {
        let r = resolver_with("com/acme/Order", "Order", &members_json_with_field("f"));
        // A project type: dep_signature is Some, project_simple maps the bare name, contains true.
        assert!(r.dep_signature("com/acme/Order").is_some());
        assert_eq!(r.project_simple("Order").as_deref(), Some("com/acme/Order"));
        assert!(r.project_contains("com/acme/Order"));
        assert!(r.project_contains("Order"));
        // An absent type: all None / false.
        assert!(r.dep_signature("com/acme/Ghost").is_none());
        assert!(r.project_simple("Ghost").is_none());
        assert!(!r.project_contains("Ghost"));
    }

    #[test]
    fn dep_signature_tracks_member_changes_and_is_stable() {
        let r = resolver_with("com/acme/Order", "Order", &members_json_with_field("f"));
        let sig1 = r.dep_signature("com/acme/Order").expect("project type");
        // Same content → same signature (stable).
        assert_eq!(r.dep_signature("com/acme/Order"), Some(sig1));
        // An overlay edit that changes the members → the signature changes (the cache would then
        // re-validate any file depending on Order).
        r.apply_file_patch(
            "src/Order.java",
            &[class_symbol("Order", "com/acme/Order", &members_json_with_field("g"))],
        );
        let sig2 = r.dep_signature("com/acme/Order").expect("still a project type");
        assert_ne!(sig1, sig2, "changed members ⇒ changed dependency signature");
    }

    #[test]
    fn recording_captures_members_hit_and_miss() {
        let r = resolver_with("com/acme/Order", "Order", &members_json_with_field("f"));
        let (_out, deps) = crate::dep_record::record(|| {
            // A hit on a project type, and a miss on an absent one.
            let _ = r.members_of("com/acme/Order");
            let _ = r.members_of("com/acme/Ghost");
        });
        assert_eq!(
            deps.members.get("com/acme/Order").copied(),
            r.dep_signature("com/acme/Order"),
            "recorded members hash matches the live dep_signature",
        );
        assert!(deps.misses.contains("com/acme/Ghost"), "absent type recorded as a negative dep");
    }

    #[test]
    fn recording_captures_simple_hits_and_negative_deps() {
        let r = resolver_with("com/acme/Order", "Order", &members_json_with_field("f"));
        let (_out, deps) = crate::dep_record::record(|| {
            // Resolves to a project type → simple hit.
            assert_eq!(r.resolve_simple_name("Order", &[]).as_deref(), Some("com/acme/Order"));
            // No project type named `Widget` → a negative dep (adding one later must invalidate).
            assert!(r.resolve_simple_name("Widget", &[]).is_none());
        });
        assert_eq!(deps.simple_hits.get("Order").map(String::as_str), Some("com/acme/Order"));
        assert!(deps.misses.contains("Widget"));
        // An import-bound name is project-independent → NOT recorded (neither hit nor miss).
        let (_o2, deps2) = crate::dep_record::record(|| {
            let imports = vec![Import { path: "com.other.Order".into(), star: false, static_: false }];
            let _ = r.resolve_simple_name("Order", &imports);
        });
        assert!(deps2.simple_hits.is_empty(), "import-bound name not recorded as a project hit");
        assert!(deps2.misses.is_empty(), "import-bound name not recorded as a miss");
    }
}
