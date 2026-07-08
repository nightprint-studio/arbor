//! Persisted, dependency-aware cache of per-file validation **diagnostics** — the incremental
//! layer that makes re-validating an unchanged project (or the unchanged part of an edited one)
//! instant, the way an IDE serves warm results without re-analyzing everything.
//!
//! It mirrors the reference-index cache ([`crate::refcache`]) — same content-hash + guarded-reuse
//! shape, persisted under the stable index base — but keyed on the PRECISE per-file dependency set
//! recorded during validation ([`bennu_query::prelude::RecordedDeps`]) rather than a coarse
//! type-map hash. A cached entry is reused iff, against the live resolver:
//!
//!  1. the file's own bytes are unchanged (`own_hash`), AND
//!  2. every project type it read still has the same members (`members`), AND
//!  3. every bare name it resolved to a project type still does (`simple_hits`), AND
//!  4. every name it probed and found absent is still absent (`misses`).
//!
//! Under those four conditions a fresh validation is *guaranteed* to produce the identical
//! diagnostics — the recorded set is a superset of everything validation reads from the mutable
//! project surface (see [`bennu_query::dep_record`]) — so serving the cached list can never
//! surface a stale (false-positive) diagnostic. That property is the whole point: correctness is
//! preserved by construction, the cache only removes redundant work.
//!
//! ## Epoch (classpath / JDK)
//! JDK and library-jar types are not fingerprinted per file (they're immutable within a fixed
//! classpath). Instead the whole cache carries an `epoch` derived from the JDK + resolved
//! classpath; a change to either drops the cache wholesale. The manual "Rebuild index" also
//! clears it ([`clear`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bennu_proto::prelude::Diagnostic;
use bennu_query::prelude::{ProjectView, RecordedDeps};
use serde::{Deserialize, Serialize};

/// Bumped whenever the on-disk shape (or the freshness semantics) changes — a mismatch drops the
/// cache and re-validates from scratch, since there is no migration.
pub const CACHE_VERSION: u32 = 1;

/// A deterministic content hash of a source buffer — the file's `own_hash`. Shares the
/// reference cache's FNV-1a so the whole product hashes source the same way.
pub fn source_hash(s: &str) -> u64 {
    crate::refcache::content_hash(s)
}

/// The recorded project dependencies of one file's validation, in a stable (sorted) on-disk form.
/// Built from a [`RecordedDeps`] plus the file's own content hash; re-checked against the live
/// resolver by [`is_fresh`](FileDeps::is_fresh).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDeps {
    /// FNV-1a of the file's own source bytes.
    pub own_hash: u64,
    /// Project types whose members were read: `(binary, members-JSON hash)`, sorted by binary.
    pub members: Vec<(String, u64)>,
    /// Bare names that resolved to a project type: `(simple, binary)`, sorted by simple.
    pub simple_hits: Vec<(String, String)>,
    /// Names probed against the project and found ABSENT (must stay absent), sorted.
    pub misses: Vec<String>,
}

impl FileDeps {
    /// Fold a validation's [`RecordedDeps`] (+ the file's `own_hash`) into the sorted on-disk form.
    pub fn from_recorded(own_hash: u64, deps: &RecordedDeps) -> Self {
        let mut members: Vec<(String, u64)> =
            deps.members.iter().map(|(k, v)| (k.clone(), *v)).collect();
        members.sort_unstable();
        let mut simple_hits: Vec<(String, String)> =
            deps.simple_hits.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        simple_hits.sort_unstable();
        let mut misses: Vec<String> = deps.misses.iter().cloned().collect();
        misses.sort_unstable();
        Self { own_hash, members, simple_hits, misses }
    }

    /// Whether a cached entry with these dependencies is still valid for the current `own_hash`,
    /// given the live project `view`. All four conditions must hold (own bytes, members, simple
    /// resolutions, absences) — the exact superset validation would re-read.
    pub fn is_fresh(&self, own_hash: u64, view: &dyn ProjectView) -> bool {
        if self.own_hash != own_hash {
            return false;
        }
        for (binary, hash) in &self.members {
            if view.dep_signature(binary) != Some(*hash) {
                return false; // a dependency's members changed (or it was removed)
            }
        }
        for (simple, binary) in &self.simple_hits {
            if view.project_simple(simple).as_deref() != Some(binary.as_str()) {
                return false; // a bare name now resolves elsewhere (or nowhere)
            }
        }
        for key in &self.misses {
            if view.project_contains(key) {
                return false; // a previously-absent type now exists → recompute (negative dep)
            }
        }
        true
    }
}

/// One file's cached validation result: its dependency fingerprint + the diagnostics to serve
/// while that fingerprint holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub deps: FileDeps,
    pub diagnostics: Vec<Diagnostic>,
}

/// The whole persisted diagnostic cache: a version + the classpath/JDK epoch + one entry per file
/// (keyed by the forward-slashed file path).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagCache {
    pub version: u32,
    pub epoch: u64,
    pub files: HashMap<String, CacheEntry>,
}

impl DiagCache {
    /// A fresh, empty cache for `epoch`.
    pub fn new(epoch: u64) -> Self {
        Self { version: CACHE_VERSION, epoch, files: HashMap::new() }
    }

    /// Load the cache for `epoch` from `path`, or start empty. An entry set is only reused when the
    /// on-disk `version` AND `epoch` match — a JDK / classpath change (new `epoch`), or a shape
    /// change (new `version`), yields an empty cache so nothing stale is ever served.
    pub fn load_or_new(path: &Path, epoch: u64) -> Self {
        match load(path) {
            Some(c) if c.version == CACHE_VERSION && c.epoch == epoch => c,
            _ => Self::new(epoch),
        }
    }

    /// The cached diagnostics for `file` when its entry is still fresh against `view` for the
    /// current `own_hash`; `None` on a miss (absent / stale entry → the caller re-validates).
    pub fn get_fresh(&self, file: &str, own_hash: u64, view: &dyn ProjectView) -> Option<&[Diagnostic]> {
        let entry = self.files.get(file)?;
        entry.deps.is_fresh(own_hash, view).then(|| entry.diagnostics.as_slice())
    }

    /// Store (or replace) `file`'s freshly-computed diagnostics + the deps they were computed
    /// under, so the next unchanged run serves them without re-validating.
    pub fn put(&mut self, file: &str, deps: FileDeps, diagnostics: Vec<Diagnostic>) {
        self.files.insert(file.to_string(), CacheEntry { deps, diagnostics });
    }
}

/// Load the cache from `path`. `None` (→ start empty) on any error — a missing, truncated, or
/// version-incompatible file is never fatal.
pub fn load(path: &Path) -> Option<DiagCache> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Persist the cache to `path`. Best-effort: a write failure is logged, never fatal (the next run
/// just re-validates).
pub fn save(path: &Path, cache: &DiagCache) {
    match serde_json::to_vec(cache) {
        Ok(bytes) => {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(path, bytes) {
                eprintln!("bennu-be: diagnostic cache write failed ({}): {e}", path.display());
            }
        }
        Err(e) => eprintln!("bennu-be: diagnostic cache serialize failed: {e}"),
    }
}

/// Delete the cache at `path` (the manual "Rebuild index" path → force clean re-validation).
/// Ignores a missing file.
pub fn clear(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// The canonical cache location for a project — a STABLE path under the index base (not the
/// per-build generation dir, which is recreated each open), so it survives across opens like the
/// reference cache.
pub fn cache_path(index_base: &Path) -> PathBuf {
    index_base.join("diagnostics-cache.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// A hand-built project view: types present (binary → members hash) + simple→binary names.
    /// Absent from both maps ⇒ not a project type. Lets the freshness logic be tested precisely
    /// without a live index / resolver.
    #[derive(Default)]
    struct MockView {
        members: HashMap<String, u64>,
        simples: HashMap<String, String>,
    }
    impl MockView {
        fn with_type(mut self, binary: &str, simple: &str, hash: u64) -> Self {
            self.members.insert(binary.to_string(), hash);
            self.simples.insert(simple.to_string(), binary.to_string());
            self
        }
    }
    impl ProjectView for MockView {
        fn dep_signature(&self, binary: &str) -> Option<u64> {
            self.members.get(binary).copied()
        }
        fn project_simple(&self, simple: &str) -> Option<String> {
            self.simples.get(simple).cloned()
        }
        fn project_contains(&self, key: &str) -> bool {
            self.members.contains_key(key) || self.simples.contains_key(key)
        }
    }

    fn recorded(members: &[(&str, u64)], simple_hits: &[(&str, &str)], misses: &[&str]) -> RecordedDeps {
        RecordedDeps {
            members: members.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            simple_hits: simple_hits.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            misses: misses.iter().map(|s| s.to_string()).collect::<HashSet<_>>(),
        }
    }

    fn diag(msg: &str) -> Diagnostic {
        Diagnostic { message: msg.to_string(), severity: "error".to_string(), code: String::new(), start: 0, end: 1 }
    }

    #[test]
    fn from_recorded_sorts_for_determinism() {
        let d = recorded(&[("b/B", 2), ("a/A", 1)], &[("Z", "z/Z"), ("A", "a/A")], &["y", "x"]);
        let fd = FileDeps::from_recorded(99, &d);
        assert_eq!(fd.own_hash, 99);
        assert_eq!(fd.members, vec![("a/A".to_string(), 1), ("b/B".to_string(), 2)]);
        assert_eq!(fd.simple_hits, vec![("A".to_string(), "a/A".to_string()), ("Z".to_string(), "z/Z".to_string())]);
        assert_eq!(fd.misses, vec!["x".to_string(), "y".to_string()]);
        // Same deps in a different iteration order → identical FileDeps (stable serialization).
        let d2 = recorded(&[("a/A", 1), ("b/B", 2)], &[("A", "a/A"), ("Z", "z/Z")], &["x", "y"]);
        assert_eq!(fd, FileDeps::from_recorded(99, &d2));
    }

    #[test]
    fn fresh_when_nothing_changed() {
        let view = MockView::default().with_type("a/A", "A", 1).with_type("b/B", "B", 2);
        let fd = FileDeps::from_recorded(7, &recorded(&[("a/A", 1)], &[("B", "b/B")], &["Ghost"]));
        assert!(fd.is_fresh(7, &view), "same content + same deps + still-absent miss ⇒ reuse");
    }

    #[test]
    fn stale_when_own_content_changed() {
        let view = MockView::default().with_type("a/A", "A", 1);
        let fd = FileDeps::from_recorded(7, &recorded(&[("a/A", 1)], &[], &[]));
        assert!(!fd.is_fresh(8, &view), "different own_hash ⇒ re-validate");
    }

    #[test]
    fn stale_when_a_dependency_members_changed() {
        // Cached under A's members hash = 1; the live view now reports A with hash = 999.
        let view = MockView::default().with_type("a/A", "A", 999);
        let fd = FileDeps::from_recorded(7, &recorded(&[("a/A", 1)], &[], &[]));
        assert!(!fd.is_fresh(7, &view), "a dependency's members changed ⇒ re-validate");
    }

    #[test]
    fn stale_when_a_dependency_was_removed() {
        // A is gone from the project entirely.
        let view = MockView::default();
        let fd = FileDeps::from_recorded(7, &recorded(&[("a/A", 1)], &[], &[]));
        assert!(!fd.is_fresh(7, &view), "a removed dependency ⇒ re-validate");
    }

    #[test]
    fn stale_when_a_simple_hit_now_resolves_elsewhere() {
        // The file resolved `A` → a/A; now `A` maps to a DIFFERENT binary (a moved/renamed type).
        let view = MockView::default().with_type("other/A", "A", 5);
        let fd = FileDeps::from_recorded(7, &recorded(&[], &[("A", "a/A")], &[]));
        assert!(!fd.is_fresh(7, &view), "a bare name now binds a different type ⇒ re-validate");
    }

    #[test]
    fn stale_when_a_negative_dependency_becomes_present() {
        // THE false-positive-critical case: the file had an error because `Widget` didn't exist.
        // A project type `Widget` is now added → the cached (error) diagnostics MUST be dropped.
        let view = MockView::default().with_type("w/Widget", "Widget", 3);
        let fd = FileDeps::from_recorded(7, &recorded(&[], &[], &["Widget"]));
        assert!(!fd.is_fresh(7, &view), "a formerly-absent type now exists ⇒ re-validate (no stale error)");
    }

    #[test]
    fn cache_get_put_roundtrip_and_epoch_reset() {
        let view = MockView::default().with_type("a/A", "A", 1);
        let mut cache = DiagCache::new(100);
        let fd = FileDeps::from_recorded(7, &recorded(&[("a/A", 1)], &[], &[]));
        cache.put("src/F.java", fd, vec![diag("boom")]);
        // Hit while fresh.
        let hit = cache.get_fresh("src/F.java", 7, &view).expect("fresh entry");
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].message, "boom");
        // Miss when the content changed.
        assert!(cache.get_fresh("src/F.java", 8, &view).is_none());
        // Miss for an unknown file.
        assert!(cache.get_fresh("src/G.java", 7, &view).is_none());

        // Persist + reload at the SAME epoch → entry survives (cross-session warm start).
        let dir = std::env::temp_dir().join(format!("bennu-diagcache-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = cache_path(&dir);
        save(&path, &cache);
        let reloaded = DiagCache::load_or_new(&path, 100);
        assert!(reloaded.get_fresh("src/F.java", 7, &view).is_some(), "same epoch reloads entries");
        // Reload at a DIFFERENT epoch (classpath/JDK changed) → empty (nothing stale served).
        let reset = DiagCache::load_or_new(&path, 101);
        assert!(reset.files.is_empty(), "epoch change drops the whole cache");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_or_new_handles_missing_and_corrupt_files() {
        let dir = std::env::temp_dir().join(format!("bennu-diagcache-corrupt-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = cache_path(&dir);
        // Missing file → empty.
        assert!(DiagCache::load_or_new(&path, 1).files.is_empty());
        // Corrupt file → empty (never fatal).
        std::fs::write(&path, b"not json at all").unwrap();
        assert!(DiagCache::load_or_new(&path, 1).files.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// END-TO-END: drive the WHOLE mechanism over a real [`IndexResolver`] and real validation
/// (`check_file_resolved`) — the recording, the `FileDeps`, and the freshness re-check against the
/// live resolver — to prove the incremental cache invalidates exactly when a re-validation would
/// differ, and NEVER serves a stale (false-positive) diagnostic. No JDK needed: a stub member
/// index resolves nothing (only PROJECT types matter for the fingerprint).
#[cfg(test)]
mod integration {
    use super::{source_hash, FileDeps};

    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use bennu_check::prelude::{check_file_resolved, FileContext};
    use bennu_classpath::prelude::{ClassMembers as CpClassMembers, MemberIndex};
    use bennu_index::prelude::{PersistedIndex, Symbol};
    use bennu_proto::prelude::Diagnostic;
    use bennu_query::prelude::{record, IndexResolver, ProjectView};

    use crate::java_index::{build_project_index_from_sources, file_records_from_source};

    /// A JDK stub that resolves nothing — the cache fingerprint only tracks the mutable PROJECT
    /// surface, so the test needs no live JDK.
    struct NoJdk;
    impl MemberIndex for NoJdk {
        fn members_of(&self, _binary_name: &str) -> Option<CpClassMembers> {
            None
        }
    }

    /// Build a real resolver over a persisted index of `sources`. Returns the resolver, the
    /// project's simple→binary type map (to seed incremental patches), and the temp dir (kept for
    /// the mmap's lifetime; the caller removes it at the end).
    fn build(sources: &[(PathBuf, String)]) -> (IndexResolver<NoJdk>, BTreeMap<String, String>, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "bennu-diagcache-e2e-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let built = build_project_index_from_sources(sources, &dir);
        built.builder.persist().unwrap();
        let project = PersistedIndex::open(built.builder.blob_path(), built.builder.fst_path()).unwrap();
        let type_map = built.type_map;
        let mut resolver = IndexResolver::new(project, NoJdk);
        for (simple, binary) in &type_map {
            resolver.add_simple_hint(simple, binary);
        }
        (resolver, type_map, dir)
    }

    /// Validate `src` while recording its project dependencies → `(diagnostics, FileDeps)`, exactly
    /// as the be layer's parallel whole-project validation does per file.
    fn validate(resolver: &IndexResolver<NoJdk>, src: &str, stem: &str) -> (Vec<Diagnostic>, FileDeps) {
        let ctx = FileContext {
            file_stem: Some(stem.to_string()),
            expected_package: None,
            java_major: Some(8),
            classpath_complete: false,
        };
        let (diags, recorded) = record(|| check_file_resolved(src, &ctx, resolver, true));
        (diags, FileDeps::from_recorded(source_hash(src), &recorded))
    }

    /// Apply an edited file to the resolver's overlay (mirrors `IndexService::patch_file`).
    fn patch(resolver: &IndexResolver<NoJdk>, file: &str, src: &str, type_map: &BTreeMap<String, String>) {
        let symbols: Vec<Symbol> = file_records_from_source(Path::new(file), src, type_map, u32::MAX / 2)
            .into_iter()
            .map(|r| r.symbol)
            .collect();
        resolver.apply_file_patch(file, &symbols);
    }

    #[test]
    fn dependency_member_change_invalidates_the_dependent() {
        let b_src = "package p;\npublic class B { public int foo() { return 1; } }\n";
        let a_src = "package p;\npublic class A { void m(B b) { b.foo(); } }\n";
        let sources = vec![
            (PathBuf::from("/p/B.java"), b_src.to_string()),
            (PathBuf::from("/p/A.java"), a_src.to_string()),
        ];
        let (resolver, type_map, dir) = build(&sources);

        let (_diags, a_deps) = validate(&resolver, a_src, "A");
        // A resolved + read B → B is a recorded dependency (a members hit or a simple hit).
        assert!(
            a_deps.members.iter().any(|(bin, _)| bin == "p/B")
                || a_deps.simple_hits.iter().any(|(s, _)| s == "B"),
            "A must depend on B: {a_deps:?}",
        );
        assert!(a_deps.is_fresh(source_hash(a_src), &resolver), "fresh immediately after validation");

        // Edit B's members (foo gains a parameter) → A's cached result must go stale.
        let b_src2 = "package p;\npublic class B { public int foo(int x) { return x; } }\n";
        patch(&resolver, "/p/B.java", b_src2, &type_map);
        assert!(
            !a_deps.is_fresh(source_hash(a_src), &resolver),
            "a dependency's member change invalidates the dependent",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unrelated_edit_keeps_the_dependent_fresh() {
        let b_src = "package p;\npublic class B { public int foo() { return 1; } }\n";
        let a_src = "package p;\npublic class A { void m(B b) { b.foo(); } }\n";
        let sources = vec![
            (PathBuf::from("/p/B.java"), b_src.to_string()),
            (PathBuf::from("/p/A.java"), a_src.to_string()),
        ];
        let (resolver, type_map, dir) = build(&sources);
        let (_d, a_deps) = validate(&resolver, a_src, "A");
        assert!(a_deps.is_fresh(source_hash(a_src), &resolver));

        // A brand-new, unrelated type C appears — A never mentions it, so A stays reusable (this is
        // the incremental win: editing one file doesn't re-validate the whole project).
        let c_src = "package p;\npublic class C { public void bar() {} }\n";
        patch(&resolver, "/p/C.java", c_src, &type_map);
        assert!(
            a_deps.is_fresh(source_hash(a_src), &resolver),
            "an unrelated new type must NOT invalidate A",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn adding_a_previously_missing_type_invalidates_via_negative_dep() {
        // THE false-positive-critical case: A2 references `Widget`, which doesn't exist → whatever
        // diagnostic that produces is recorded against a NEGATIVE dependency on `Widget`. When a
        // project type `Widget` later appears, the cached (stale) result MUST be dropped — otherwise
        // a resolved reference would keep showing a phantom "unknown type" error.
        let a2_src = "package p;\npublic class A2 { void m() { Widget w = null; } }\n";
        let sources = vec![(PathBuf::from("/p/A2.java"), a2_src.to_string())];
        let (resolver, type_map, dir) = build(&sources);

        let (_diags, a2_deps) = validate(&resolver, a2_src, "A2");
        assert!(
            a2_deps.misses.iter().any(|m| m == "Widget"),
            "A2 must record a negative dependency on the absent Widget: {a2_deps:?}",
        );
        assert!(a2_deps.is_fresh(source_hash(a2_src), &resolver), "fresh while Widget is absent");

        // A project type `Widget` now exists → A2 is no longer reusable.
        let w_src = "package p;\npublic class Widget { }\n";
        patch(&resolver, "/p/Widget.java", w_src, &type_map);
        assert!(
            !a2_deps.is_fresh(source_hash(a2_src), &resolver),
            "a formerly-absent type appearing invalidates the dependent (no stale error served)",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn own_source_edit_invalidates_regardless_of_dependencies() {
        let a_src = "package p;\npublic class A { int x = 1; }\n";
        let sources = vec![(PathBuf::from("/p/A.java"), a_src.to_string())];
        let (resolver, _tm, dir) = build(&sources);
        let (_d, a_deps) = validate(&resolver, a_src, "A");
        assert!(a_deps.is_fresh(source_hash(a_src), &resolver));
        // A different buffer for the same file → the own-hash guard alone forces re-validation.
        let a_src2 = "package p;\npublic class A { int x = 2; }\n";
        assert!(!a_deps.is_fresh(source_hash(a_src2), &resolver), "own content change ⇒ re-validate");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
