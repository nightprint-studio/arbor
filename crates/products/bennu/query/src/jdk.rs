//! [`JdkMemberIndex`] — a `Send + Sync` wrapper around the JDK classpath member
//! index, so the native provider (which must be `Send + Sync`, per [`IntelProvider`])
//! can hold it in the backend state across the multi-threaded dispatcher.
//!
//! Why the wrapper (and the one narrowly-scoped `unsafe`): `resolve_jdk_classpath`
//! returns a `Box<dyn ClassSource>`. On **JDK 8** the concrete source is a `JarSource`
//! holding a `RefCell<ZipArchive<File>>` — which is `Send` but **`!Sync`** (a jimage
//! source, JDK 9+, is `Send + Sync`, but we don't get to pick statically). The rest of
//! the state is `Send + Sync`, so this one non-`Sync` piece would poison the whole
//! provider.
//!
//! We restore `Sync` the standard way: **serialize every access through a `Mutex`**.
//! `Mutex<T>` makes a `Send` `T` usable as `Sync` because it hands out `&mut` behind a
//! lock — exactly the single-borrow discipline the `RefCell` needs. The compiler can't
//! *see* through the boxed `dyn ClassSource` that the concrete is `Send`, so we assert
//! it with a documented `unsafe impl`. The safety argument:
//!   1. every concrete source `resolve_jdk_classpath` yields (`JarSource`,
//!      `JimageSource`, `MultiSource`, `DirSource`) is `Send`;
//!   2. all access to the inner source goes through [`members_of`], which takes the
//!      `Mutex` first — no concurrent borrow of the `RefCell` is possible.
//!
//! (The alternative — reopening rt.jar's ZIP directory on every keystroke — is far too
//! slow; the mutex serializes the rare-contended, sub-millisecond lookups instead.)

use std::sync::Mutex;

use bennu_classpath::prelude::{ClassMembers, MemberIndex, SourceMemberIndex};
use bennu_classpath::prelude::ClassSource;

/// A mutex-serialized, `Send + Sync` JDK member index over a boxed classpath source.
pub struct JdkMemberIndex {
    inner: Mutex<SourceMemberIndex<Box<dyn ClassSource>>>,
}

// SAFETY: see the module docs. The concrete boxed source is always `Send`; the `Mutex`
// serializes every access, so the `!Sync` `RefCell` inside a JDK-8 `JarSource` is never
// borrowed concurrently. No `&`-shared interior mutation escapes the lock.
unsafe impl Sync for JdkMemberIndex {}
unsafe impl Send for JdkMemberIndex {}

impl JdkMemberIndex {
    /// Wrap a boxed classpath source (typically `resolve_jdk_classpath(version)?`).
    pub fn new(source: Box<dyn ClassSource>) -> Self {
        Self { inner: Mutex::new(SourceMemberIndex::new(source)) }
    }
}

impl MemberIndex for JdkMemberIndex {
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers> {
        // Poisoned lock (a prior panic in a member decode) is recoverable — the source
        // is immutable, so we take the inner guard and keep serving.
        let guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.members_of(binary_name)
    }
}
