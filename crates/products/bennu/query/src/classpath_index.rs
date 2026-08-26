//! [`ClasspathIndex`] — the two-tier member index the completion/validation resolver runs over: a
//! shared, cross-session **JDK tier** plus an optional per-project **dependency tier**.
//!
//! The JDK bytecode is identical for every project on a machine, so its decoded members are memoized
//! in ONE shared file keyed by the resolved JDK ([`JdkMemberIndex::persistent`]). A project's
//! dependency jars (`~/.m2`, resolved by Maven) are project-specific, so they get their OWN memo file
//! keyed by the project + its resolved jar set — never mixed into the shared JDK cache (which
//! [`crate::jdk`]'s `positive_snapshot` deliberately keeps bounded to the JDK surface, so a
//! dependency type of project A can't leak into project B's shared JDK memo).
//!
//! `members_of` consults the JDK tier FIRST (the genuine `java.*`/`javax.*` core wins over any shaded
//! copy bundled in a dependency), then the dependency tier. Both tiers reuse the same memoizing,
//! `Send + Sync`, persistent [`JdkMemberIndex`] mechanism — the only difference is the classpath
//! source behind each and the memo file it persists to. A JDK-only [`ClasspathIndex`] (a project with
//! no resolvable dependencies, or dependency indexing turned off / failed) degrades resolution to
//! exactly JDK + project, as before dependency indexing existed.

use std::path::PathBuf;
use std::sync::Arc;

use bennu_classpath::prelude::{ClassMembers, ClassSource, MemberIndex};

use crate::jdk::JdkMemberIndex;

/// A JDK member tier + an optional dependency member tier, unified behind one
/// [`MemberIndex`](bennu_classpath::prelude::MemberIndex) so the [`IndexResolver`](crate::resolver::IndexResolver)
/// resolves JDK **and** library types through a single `members_of`.
pub struct ClasspathIndex {
    /// The shared, cross-session JDK tier (persistent memo keyed by the resolved JDK).
    ///
    /// Behind an `Arc` so two classpath views can share ONE decoded JDK: the full one completion
    /// and validation resolve against, and a JDK-only one for the reference walk. Sharing is the
    /// point — a second `JdkMemberIndex` would re-open the jimage and re-decode every class the
    /// first had already decoded, and would race the first for the same persistent memo file.
    jdk: Arc<JdkMemberIndex>,
    /// The optional per-project dependency tier (persistent memo keyed by the project + jar set).
    /// `None` for a project with no resolvable dependencies.
    deps: Option<JdkMemberIndex>,
}

impl ClasspathIndex {
    /// A JDK-only classpath index (no dependency tier). Resolution degrades to JDK + project — the
    /// behavior before dependency indexing, and the fallback when a project has no resolvable
    /// dependencies or dependency resolution failed.
    pub fn jdk_only(jdk: Arc<JdkMemberIndex>) -> Self {
        Self { jdk, deps: None }
    }

    /// A JDK tier plus a dependency tier built from the given dep-jars `source`.
    ///
    /// The dependency tier is **in-memory only** (its referenced classes are decoded lazily and
    /// memoized for the session, never flushed to disk). A persistent dep memo re-serialized the
    /// WHOLE growing dep map every 128 newly-decoded classes — O(K²) CPU + disk churn across the
    /// whole-project validation warm-up (the CPU-pegging regression; same pattern the shared JDK
    /// memo was fixed for). Re-decoding a project's referenced dep classes on the next open is
    /// linear and cheap by comparison, so the trade is worth it; the shared JDK tier stays
    /// persistent. `_dep_memo_path` is accepted (the be layer computes it) but intentionally unused
    /// now — kept so re-enabling a *deferred* (flush-once) persistence later is a one-line change.
    pub fn with_deps(
        jdk: Arc<JdkMemberIndex>,
        dep_source: Box<dyn ClassSource>,
        _dep_memo_path: PathBuf,
    ) -> Self {
        Self {
            jdk,
            deps: Some(JdkMemberIndex::new(dep_source)),
        }
    }

    /// Whether a dependency tier is present (a project with resolvable dep jars).
    pub fn has_deps(&self) -> bool {
        self.deps.is_some()
    }

    /// Persist both tiers' memos now (best-effort; each is a no-op when nothing changed). Called at a
    /// checkpoint (project close / warm-up finish) so a session's warmed JDK **and** dependency
    /// classes survive to the next session.
    pub fn flush(&self) {
        self.jdk.flush();
        if let Some(d) = &self.deps {
            d.flush();
        }
    }
}

impl MemberIndex for ClasspathIndex {
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers> {
        // JDK first (the real core wins over a shaded copy in a dependency), then the dependency tier.
        self.jdk
            .members_of(binary_name)
            .or_else(|| self.deps.as_ref().and_then(|d| d.members_of(binary_name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `ClassSource` that serves exactly one class's bytes (any real `.class` file's bytes), keyed
    /// by binary name — enough to prove tier precedence and fall-through without a live JDK.
    struct OneClass {
        binary: &'static str,
        bytes: Vec<u8>,
    }
    impl ClassSource for OneClass {
        fn class_bytes(&self, binary_name: &str) -> Result<Option<Vec<u8>>, String> {
            Ok((binary_name == self.binary).then(|| self.bytes.clone()))
        }
    }

    /// The bytes of a minimal valid class `p/T` (compiled once and embedded) would be ideal, but the
    /// decode path is already covered in `bennu-classpath`. Here we only need to prove the COMPOSITION:
    /// a `None` source for a name means "ask the other tier". A source that returns `Ok(None)` for a
    /// name never resolves, so `members_of` for an absent name is `None` regardless of tiers.
    #[test]
    fn absent_name_resolves_to_none_across_both_tiers() {
        let jdk = JdkMemberIndex::new(Box::new(OneClass {
            binary: "java/lang/X",
            bytes: vec![],
        }));
        let deps = JdkMemberIndex::new(Box::new(OneClass {
            binary: "dep/Y",
            bytes: vec![],
        }));
        let idx = ClasspathIndex {
            jdk: Arc::new(jdk),
            deps: Some(deps),
        };
        // Neither tier has `p/Z`, and the one-class sources return empty (undecodable) bytes for their
        // own name → every lookup is a definitive miss. The point: composition never panics and a
        // miss stays a miss.
        assert!(idx.members_of("p/Z").is_none());
        assert!(idx.has_deps());
    }

    #[test]
    fn jdk_only_has_no_dep_tier() {
        let jdk = JdkMemberIndex::new(Box::new(OneClass {
            binary: "java/lang/X",
            bytes: vec![],
        }));
        let idx = ClasspathIndex::jdk_only(Arc::new(jdk));
        assert!(!idx.has_deps());
        assert!(idx.members_of("dep/Y").is_none());
    }
}
