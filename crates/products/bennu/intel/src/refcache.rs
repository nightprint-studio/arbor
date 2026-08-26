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
pub const CACHE_VERSION: u32 = 5;

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
