//! [`JdkMemberIndex`] — a `Send + Sync`, **persistent, lazy** member index over a JDK classpath
//! source.
//!
//! The JDK bytecode is immutable, and the same JDK is shared by every project and every session —
//! so parsing a class's members should happen **at most once, ever**, not on every `members_of`
//! call. This index memoizes each lookup (hits *and* definitive misses) and, when given a path,
//! persists that memo to a single JSON file keyed by the JDK identity. On the next session the memo
//! is loaded back, so a warm JDK index resolves without touching the jar/jimage at all — the
//! IntelliJ "shared JDK index" model, scoped to what's actually been queried.
//!
//! ### Why the wrapper (and the one narrowly-scoped `unsafe`)
//! `resolve_jdk_classpath` returns a `Box<dyn ClassSource>`. On **JDK 8** the concrete source is a
//! `JarSource` holding a `RefCell<ZipArchive<File>>` — `Send` but **`!Sync`**. We restore `Sync`
//! the standard way: **serialize every access through a `Mutex`**. The compiler can't see through
//! the boxed `dyn ClassSource` that the concrete is `Send`, so we assert it with a documented
//! `unsafe impl`; every concrete source `resolve_jdk_classpath` yields is `Send`, and all access
//! goes through the `Mutex`.
//!
//! ### Persistence
//! `persistent(source, path)` loads the memo from `path` (JSON `binary_name -> ClassMembers`) and,
//! once `FLUSH_EVERY` fresh **resolved** classes accumulate, writes it back atomically (temp +
//! rename) — the write is done **outside** the lock (on a cloned snapshot) so a flush never stalls a
//! concurrent `members_of`. `new(source)` is the in-memory-only variant (no path, no flush) used by
//! the empty / `project_only` resolvers. The path is chosen by the caller (the be layer) and keyed
//! by the resolved JDK — two JDKs never share a memo.
//!
//! **Only resolved JDK classes are persisted, never misses.** A miss is memoized in-memory (so the
//! source is never re-touched within a session), but it's almost always a project DEPENDENCY type
//! that isn't on the unindexed classpath — not a JDK class. Persisting misses bloated the shared,
//! cross-session JDK file without bound and made every flush re-serialize a growing map: O(K²) disk
//! churn in the number of distinct types K seen, which is why per-file validation slowed down over a
//! large legacy project. Filtering the snapshot to resolved classes keeps the file bounded to the
//! (finite) JDK surface a project touches, so flushes stop once the JDK is warm.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bennu_classpath::prelude::ClassSource;
use bennu_classpath::prelude::{ClassMembers, MemberIndex, SourceMemberIndex};

/// Flush the memo to disk once this many fresh entries have accumulated since the last write. A
/// coarse threshold: the flush clones + serializes the whole memo, so we amortise it over many
/// lookups rather than writing on every miss. Small enough that a modest project still persists its
/// warmed JDK classes (an explicit [`JdkMemberIndex::flush`] at a checkpoint catches the tail).
const FLUSH_EVERY: usize = 128;

/// The mutable interior, guarded by the index's `Mutex`.
struct Inner {
    /// The parse-on-miss source (re-reads bytecode only for entries not yet memoized).
    source: SourceMemberIndex<Box<dyn ClassSource>>,
    /// Memoized lookups: `Some(members)` for a resolved class, `None` for a definitive miss (the
    /// class is absent from this JDK). Both are authoritative — a `None` is a real "not here".
    memo: HashMap<String, Option<ClassMembers>>,
    /// Fresh entries added since the last flush (drives the coarse write threshold).
    unsaved: usize,
}

/// A mutex-serialized, `Send + Sync`, persistent lazy JDK member index over a boxed classpath source.
pub struct JdkMemberIndex {
    inner: Mutex<Inner>,
    /// Where the memo is persisted, or `None` for an in-memory-only index (empty / test resolvers).
    path: Option<PathBuf>,
}

// SAFETY: the concrete boxed source is always `Send`; the `Mutex` serializes every access, so the
// `!Sync` `RefCell` inside a JDK-8 `JarSource` is never borrowed concurrently. No `&`-shared
// interior mutation escapes the lock.
unsafe impl Sync for JdkMemberIndex {}
unsafe impl Send for JdkMemberIndex {}

impl JdkMemberIndex {
    /// An **in-memory-only** index over a boxed classpath source (no persistence). Used by the
    /// empty and `project_only` resolvers, which never resolve the JDK.
    pub fn new(source: Box<dyn ClassSource>) -> Self {
        Self {
            inner: Mutex::new(Inner { source: SourceMemberIndex::new(source), memo: HashMap::new(), unsaved: 0 }),
            path: None,
        }
    }

    /// A **persistent** index: load the memo from `path` (if it exists) and write it back as fresh
    /// entries accumulate. `path` must be keyed by the resolved JDK identity so a cached miss is
    /// only ever consulted for the JDK that produced it.
    pub fn persistent(source: Box<dyn ClassSource>, path: PathBuf) -> Self {
        let memo = load_memo(&path);
        Self {
            inner: Mutex::new(Inner { source: SourceMemberIndex::new(source), memo, unsaved: 0 }),
            path: Some(path),
        }
    }

    /// Persist the memo now (best-effort, no-op when in-memory or nothing changed). Called at safe
    /// checkpoints (e.g. project close) in addition to the automatic threshold flush.
    pub fn flush(&self) {
        let Some(path) = &self.path else { return };
        let snapshot = {
            let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            if g.unsaved == 0 {
                return;
            }
            g.unsaved = 0;
            positive_snapshot(&g.memo)
        };
        write_memo(path, &snapshot);
    }
}

impl MemberIndex for JdkMemberIndex {
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers> {
        // A flush snapshot is taken under the lock but written after releasing it, so the disk I/O
        // never blocks a concurrent lookup.
        let (result, flush_snapshot) = {
            // Poisoned lock (a prior panic in a member decode) is recoverable — the source is
            // immutable, so we take the inner guard and keep serving.
            let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());

            if let Some(hit) = g.memo.get(binary_name) {
                return hit.clone(); // memoized (resolved OR definitive miss) — no source touch
            }

            let parsed = g.source.members_of(binary_name);
            let resolved = parsed.is_some();
            g.memo.insert(binary_name.to_string(), parsed.clone());

            // Only a RESOLVED JDK class drives persistence. A miss is memoized in-memory but is
            // (overwhelmingly) a dependency type, not JDK — persisting misses grew the shared file
            // without bound and made each flush re-serialize a growing map (O(K²) disk churn, the
            // per-file slowdown). So misses never count toward the flush threshold nor reach disk.
            let snap = if resolved {
                g.unsaved += 1;
                if self.path.is_some() && g.unsaved >= FLUSH_EVERY {
                    g.unsaved = 0;
                    Some(positive_snapshot(&g.memo))
                } else {
                    None
                }
            } else {
                None
            };
            (parsed, snap)
        };

        if let (Some(path), Some(snapshot)) = (&self.path, flush_snapshot) {
            write_memo(path, &snapshot);
        }
        result
    }
}

/// Load a persisted memo, or an empty map on any error (a missing / corrupt cache just re-warms
/// lazily — never fatal).
fn load_memo(path: &Path) -> HashMap<String, Option<ClassMembers>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

/// The persistable subset of the memo: only resolved JDK classes. Definitive misses are kept
/// in-memory (so the source is never re-touched within a session) but never written — they're
/// overwhelmingly dependency types that don't belong in the shared JDK cache and would grow it
/// without bound, making every flush O(memo). Filtering here bounds the file to the JDK surface.
fn positive_snapshot(
    memo: &HashMap<String, Option<ClassMembers>>,
) -> HashMap<String, Option<ClassMembers>> {
    memo.iter()
        .filter(|(_, v)| v.is_some())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Write the memo atomically (temp file + rename), best-effort: a failed write just means the next
/// session re-warms from bytecode. Creates the parent dir if needed.
fn write_memo(path: &Path, memo: &HashMap<String, Option<ClassMembers>>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(bytes) = serde_json::to_vec(memo) else { return };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_classpath::prelude::{Member, MemberKind, TypeRef, Visibility};
    use std::sync::{Arc, Mutex as StdMutex};

    /// A fake `ClassSource` that COUNTS how many times each class's bytes were requested — so a
    /// test can assert the memo avoids re-touching the source. Returns `Ok(None)` (a definitive
    /// miss), which still exercises the cache-a-miss path.
    struct CountingSource {
        hits: Arc<StdMutex<HashMap<String, usize>>>,
    }

    impl ClassSource for CountingSource {
        fn class_bytes(&self, binary_name: &str) -> Result<Option<Vec<u8>>, String> {
            *self.hits.lock().unwrap().entry(binary_name.to_string()).or_insert(0) += 1;
            Ok(None)
        }
    }

    #[test]
    fn miss_is_memoized_not_reparsed() {
        let hits = Arc::new(StdMutex::new(HashMap::new()));
        let idx = JdkMemberIndex::new(Box::new(CountingSource { hits: hits.clone() }));

        assert!(idx.members_of("java/util/Nope").is_none());
        assert!(idx.members_of("java/util/Nope").is_none());
        assert!(idx.members_of("java/util/Nope").is_none());
        // The source was touched exactly once despite three lookups — the memo served the rest.
        assert_eq!(*hits.lock().unwrap().get("java/util/Nope").unwrap_or(&0), 1);
    }

    #[test]
    fn persistent_roundtrip_reloads_memo() {
        let dir = std::env::temp_dir().join(format!("bennu-jdk-index-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("jdk-memo.json");
        let _ = std::fs::remove_file(&path);

        // Hand-build a memo and persist it via the same writer the index uses.
        let mut memo: HashMap<String, Option<ClassMembers>> = HashMap::new();
        memo.insert(
            "com/acme/Foo".to_string(),
            Some(ClassMembers {
                superclass: Some("java/lang/Object".to_string()),
                interfaces: vec![],
                methods: vec![Member {
                    name: "bar".to_string(),
                    kind: MemberKind::Method,
                    return_type: TypeRef::plain("int"),
                    params: vec![],
                    is_static: false,
                    is_abstract: false,
                    is_default: false,
                    is_final: false,
                    visibility: Visibility::Public,
                    raw_signature: "int bar()".to_string(),
                }],
                fields: vec![],
                flags: Default::default(),
            }),
        );
        memo.insert("com/acme/Absent".to_string(), None);
        write_memo(&path, &memo);

        // Load it back — a persistent index reads this at construction.
        let loaded = load_memo(&path);
        assert_eq!(loaded.len(), 2);
        assert!(loaded.get("com/acme/Absent").unwrap().is_none());
        let foo = loaded.get("com/acme/Foo").unwrap().as_ref().expect("Foo present");
        assert_eq!(foo.methods[0].name, "bar");
        assert_eq!(foo.superclass.as_deref(), Some("java/lang/Object"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_cache_file_loads_empty() {
        let path = std::env::temp_dir().join("bennu-jdk-index-does-not-exist-xyz.json");
        let _ = std::fs::remove_file(&path);
        assert!(load_memo(&path).is_empty());
    }
}
