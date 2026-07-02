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

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_index::prelude::PersistedIndex;
use bennu_java::prelude::{
    ClassMembers as JClassMembers, Import, Member as JMember, MemberKind as JMemberKind,
    TypeRef as JTypeRef, TypeResolver, Visibility as JVisibility,
};

/// A [`TypeResolver`] composing the persisted project index and a JDK `MemberIndex`.
pub struct IndexResolver<M: CpMemberIndex> {
    project: PersistedIndex,
    jdk: M,
    /// Simple-name → binary-name hints (the project's own types + common JDK names),
    /// so `resolve_simple_name` works even without an explicit import.
    simple_hints: HashMap<String, String>,
}

impl<M: CpMemberIndex> IndexResolver<M> {
    /// Build the resolver over a persisted project index + a JDK member index.
    pub fn new(project: PersistedIndex, jdk: M) -> Self {
        let mut simple_hints = HashMap::new();
        for (s, b) in COMMON_SIMPLE {
            simple_hints.insert((*s).to_string(), (*b).to_string());
        }
        Self { project, jdk, simple_hints }
    }

    /// Seed a simple→binary hint (e.g. the project's own declared types).
    pub fn add_simple_hint(&mut self, simple: &str, binary: &str) {
        self.simple_hints.insert(simple.to_string(), binary.to_string());
    }

    /// The persisted project index (for the completion query's prefix search).
    pub fn project(&self) -> &PersistedIndex {
        &self.project
    }
}

impl<M: CpMemberIndex> TypeResolver for IndexResolver<M> {
    fn members_of(&self, binary_name: &str) -> Option<JClassMembers> {
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
