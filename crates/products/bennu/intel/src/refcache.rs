//! Persisted, incremental cache for the whole-project reference index.
//!
//! The reference walk (find-usages / rename / go-to backbone) is the O(N) phase of an index
//! build. It is a MERGE of per-file contributions — each file's resolved edges are
//! independent — so it caches naturally: persist every file's contribution keyed by a hash of
//! its source, and on the next open re-walk only the files whose source changed.
//!
//! ## Invalidation (dependency-aware)
//! A file's edges also depend on the TYPES of other files (`b.getThing().foo()` resolves
//! `foo` against the return type of `getThing` in *another* file). So two levels guard reuse:
//!
//! 1. **Global type-set guard** — [`type_map_hash`] over the project's simple→binary map. Any
//!    type added / removed / renamed / moved changes it → the whole cache is dropped and the
//!    walk runs fully. This covers every *structural* change cheaply.
//! 2. **Per-file dependency guard** — when the type set is unchanged but some files' CONTENT
//!    changed (a method body / signature / added member), we re-walk the changed files AND
//!    every file whose recorded dependencies ([`deps_of`]) name a type declared by a changed
//!    file. So a referrer picks up a callee's signature change.
//!
//! The manual "Rebuild index" deletes the cache file, forcing a clean full walk.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bennu_java::prelude::FileSymbols;
use serde::{Deserialize, Serialize};

use crate::refs::{DeclKey, UsageLocation};

/// Bumped whenever the on-disk shape (or the walk's edge semantics) changes — a mismatch
/// drops the cache and rebuilds, since there is no migration.
///
/// 6: the walk resolves four things it used to drop — a method reference qualified by an
/// EXPRESSION (`this::run`, `service::fetch`: the receiver search knew `a.b()` and `a.b` and not
/// `a::b`, so none of them were ever indexed), a qualifier that names a NESTED type
/// (`Outer.Nested::method`), and a lambda parameter target-typed from a static factory / a `return`
/// / a declared variable, and a STATIC field read as a receiver (`Headers.USERNAME.name()` — an
/// enum constant is a static field, and field access had no type-name fallback where a static call
/// already did). All four add edges for files whose bytes never changed, so without a
/// bump an existing project keeps serving the old, incomplete index and the fix looks like it did
/// nothing.
///
/// 12: `Outer.this` types to the enclosing class, so an inner class's calls on it are indexed.
///
/// 11: `@Ann` is recorded as a use of the type `Ann`. An annotation's name is not a
/// `type_identifier`, so every annotation use in every project was missing from the index.
///
/// 10: an enum constant's body is an anonymous subclass of its own enum, so the overrides written
/// there move with the method they override.
///
/// 9: a VARARGS parameter is finally a binding. tree-sitter gives `T... xs` no `name` field, so
/// every scope lookup in the walk answered `None` for one — and a bare use of it was attributed to
/// whatever else carried the name, typically a field of the enclosing class.
///
/// 8: the walk reads a written type name through the workspace's shared reader
/// (`bennu_java::typename`) in Java's own order — the file's own types, then inherited member
/// types, then imports, then the package — instead of consulting the project-wide simple→binary map
/// first. That map keeps ONE binary per simple name, so every file in a package that declares both
/// a top-level `Builder` and a nested one had bound the wrong one.
///
/// 7: each file also records the qualified member accesses whose receiver the walk could NOT type,
/// which the rename planner needs to refuse a rename it cannot prove complete. A cache without them
/// would let a rename through on exactly the evidence it is missing.
/// 13: a type named as the QUALIFIER of a static access (`Holder.VALUE`) is recorded as a use of
/// that type, and a reference written inside an ANONYMOUS class body is recorded at all — its
/// owner key (`p/Outer/1`) did not read as a project type, so every one of them was dropped. Both
/// change what the walk emits, so a cache written before this holds an index that is missing them
/// and looks complete.
pub const CACHE_VERSION: u32 = 13;

/// One file's cached contribution to the reference index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFile {
    /// Content hash of the file's source ([`content_hash`]) — reuse iff it still matches.
    pub hash: u64,
    /// The `(declaration, use-site)` edges this file emitted.
    pub edges: Vec<(DeclKey, UsageLocation)>,
    /// The file's parsed symbols (kept so the caret classifier works without a re-parse).
    pub symbols: FileSymbols,
    pub attempted: usize,
    pub resolved: usize,
    /// Qualified member accesses whose receiver the walk could not type — `(member name, where)`.
    /// The rename planner reads these to refuse a rename it cannot prove complete.
    /// `#[serde(default)]` so a cache written before they existed still loads.
    #[serde(default)]
    pub unresolved: Vec<(String, UsageLocation)>,
}

/// The whole persisted cache: a version + the type-set guard + one entry per file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefCache {
    pub version: u32,
    pub type_map_hash: u64,
    pub files: HashMap<String, CachedFile>,
}

/// FNV-1a over the source bytes — a fast, deterministic (cross-run stable) content hash. A
/// collision only ever causes a missed rebuild of one file; the manual Rebuild is the escape
/// hatch, so a non-cryptographic hash is fine.
pub fn content_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A stable hash of the project's simple→binary type map — the global guard. Sorted so it's
/// order-independent (the map iteration order is not).
pub fn type_map_hash(map: &HashMap<String, String>) -> u64 {
    let mut pairs: Vec<(&str, &str)> = map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    pairs.sort_unstable();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for (k, v) in pairs {
        feed(k.as_bytes());
        feed(b"=");
        feed(v.as_bytes());
        feed(b";");
    }
    h
}

/// The set of type binary names a file DEPENDS ON: every type its edges resolved to (member
/// owners + type refs) plus its explicit single-type imports. When any of these types is
/// declared by a file that changed, this file must be re-walked.
pub fn deps_of(edges: &[(DeclKey, UsageLocation)], symbols: &FileSymbols) -> HashSet<String> {
    let mut deps = HashSet::new();
    for (key, _) in edges {
        deps.insert(key.owner_binary().to_string());
    }
    for imp in &symbols.imports {
        // A single-type import (`import a.b.C;`) is a direct type dependency. Star imports
        // (a package) and static imports (a member) don't name a type binary usefully here —
        // the edge owners above already capture what actually resolved.
        if !imp.star && !imp.static_ {
            deps.insert(imp.path.replace('.', "/"));
        }
    }
    deps
}

/// The binary names of the types a file DECLARES (for the dependency guard's "changed types"
/// set). Derived from its parsed symbols.
pub fn defined_types(symbols: &FileSymbols) -> impl Iterator<Item = String> + '_ {
    symbols.types.iter().map(|t| t.fqn.replace('.', "/"))
}

/// Load the cache from `path`. `None` (→ full rebuild) on any error — a missing, truncated,
/// or version-incompatible file is never fatal.
pub fn load(path: &Path) -> Option<RefCache> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Persist the cache to `path`. Best-effort: a write failure is logged, never fatal (the next
/// open just rebuilds).
pub fn save(path: &Path, cache: &RefCache) {
    match serde_json::to_vec(cache) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, bytes) {
                eprintln!("bennu-be: ref cache write failed ({}): {e}", path.display());
            }
        }
        Err(e) => eprintln!("bennu-be: ref cache serialize failed: {e}"),
    }
}

/// Delete the cache at `path` (the manual "Rebuild index" path → force a clean full walk).
/// Ignores a missing file.
pub fn clear(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// The canonical cache location for a project — a STABLE path under the index base (not the
/// per-build generation dir, which is recreated each open), so it survives across opens.
pub fn cache_path(index_base: &Path) -> PathBuf {
    index_base.join("references-cache.json")
}
