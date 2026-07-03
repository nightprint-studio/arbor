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
use std::sync::RwLock;

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_index::prelude::{PersistedIndex, Symbol};
use bennu_java::prelude::{
    ClassMembers as JClassMembers, Import, Member as JMember, MemberKind as JMemberKind,
    TypeRef as JTypeRef, TypeResolver, Visibility as JVisibility,
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
        Self { project, jdk, simple_hints, overlay: RwLock::new(Overlay::default()) }
    }

    /// Seed a simple→binary hint (e.g. the project's own declared types).
    pub fn add_simple_hint(&mut self, simple: &str, binary: &str) {
        self.simple_hints.insert(simple.to_string(), binary.to_string());
    }

    /// The persisted project index (for the completion query's prefix search).
    pub fn project(&self) -> &PersistedIndex {
        &self.project
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

impl<M: CpMemberIndex> TypeResolver for IndexResolver<M> {
    fn members_of(&self, binary_name: &str) -> Option<JClassMembers> {
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
        // 2) JDK bytecode type (converted from the classpath seam).
        let cp = self.jdk.members_of(binary_name)?;
        Some(convert_members(&cp))
    }

    fn resolve_simple_name(&self, name: &str, imports: &[Import]) -> Option<String> {
        // Imports win (a `java.util.List` import binds `List`).
        for imp in imports {
            if imp.simple_name() == Some(name) {
                return Some(imp.path.replace('.', "/"));
            }
        }
        // An edited file's own (possibly renamed/added) type overrides the stale mmap.
        {
            let ov = self.overlay.read().unwrap_or_else(|p| p.into_inner());
            if let Some(binary) = ov.simple.get(name) {
                return Some(binary.clone());
            }
        }
        // Then a project type of that simple name, then the common-JDK table.
        if let Some(sym) = self.project.get(name) {
            if !sym.fqn.is_empty() {
                return Some(sym.fqn.clone());
            }
        }
        self.simple_hints.get(name).cloned()
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
                visibility: JVisibility::Public,
                raw_signature: format!("int {field}"),
            }],
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
}
