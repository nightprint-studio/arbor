//! The mmap'd store: a framed `rkyv` blob + an `fst::Map` (name → offset).
//!
//! Ported from `bennu-spike-index` (docs §10) with the two production hardenings:
//!
//! 1. **Format-version header** (16 bytes) at the head of the blob: magic
//!    (`b"BNNUIDX0"`) + a `u64` version. [`BlobReader::open`] rejects a mismatch with
//!    [`StoreError::VersionMismatch`] so the caller rebuilds from sources (rkyv has
//!    no migration — docs §4).
//! 2. **16-byte-aligned records** (docs §10): the writer pads every record so its
//!    payload starts on a 16-byte boundary. rkyv's archived form has alignment
//!    requirements; a misaligned slice fails `bytecheck` loudly. The header is itself
//!    16 bytes, so the first record is already aligned.
//!
//! Frame layout per record, at a 16-byte-aligned offset:
//! ```text
//!   [u32 len][record bytes ..len][pad to next 16-byte boundary]
//! ```
//! The `u32` len prefix means the reader slices exactly one record; the offset
//! stored in the fst is the offset of the `len` prefix (itself 16-byte-aligned, so
//! `offset + 4` — where the rkyv payload begins — is 4-aligned; rkyv is copied into
//! an aligned scratch buffer on access, so payload alignment is satisfied there).

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use fst::{Map, MapBuilder};
use memmap2::Mmap;

/// Blob format magic + version. Bump [`FORMAT_VERSION`] on any schema/layout change
/// so an old on-disk index is rebuilt rather than misread.
pub const MAGIC: &[u8; 8] = b"BNNUIDX0";
/// Current on-disk format version. Bump on any [`crate::schema`] change.
///
/// v2: added `Symbol::members_json` (the analyzer-owned resolved member surface for a
/// type symbol) so a project type resolves from the index without a source re-parse.
/// v3: added the relation store (config-graph edges) + `Relation::inferred` (candidate
/// edges from wildcards / Tiles indirection).
/// v4: added the Struts interceptor + validation config sources (`StrutsInterceptor` /
/// `StrutsValidation`) and their edges (`InterceptorRefToDef` / `InterceptorToClass`).
/// v5: added the MyBatis mapper config source (`MyBatisMapper`) and its edge
/// (`MethodToStatement`).
/// v6: `members_json` now records a type's supertypes as written — `Range<Double>` rather
/// than `Range` — so an inherited generic member can be resolved against the arguments the
/// subtype actually passes up. An index written before this carries names only, which
/// deserializes into the new shape as a raw type: silently wrong rather than absent, so
/// the version has to reject it.
/// v7: a type reached through a JDK star import (`import java.util.*;`) is recorded under its
/// binary name. Before, `Optional<Foo> find()` in such a file was baked with the bare word
/// `Optional`, which reads as an unresolved type variable — so a `var` initialised from the call
/// had no type. An older index still carries the bare names and must be rebuilt.
/// v8: `members_json` records whether a top-level type is package-private. An older index reads as
/// "not package-private" for every type, which is safe but leaves the access check blind; rebuilding
/// is what gives it something to judge.
pub const FORMAT_VERSION: u64 = 9;
/// The header is exactly 16 bytes (8 magic + 8 version), so the first record starts
/// 16-byte-aligned.
pub const HEADER_LEN: usize = 16;
/// Record alignment (docs §10).
pub const RECORD_ALIGN: usize = 16;

/// Errors from building or opening a store.
#[derive(Debug)]
pub enum StoreError {
    /// Underlying I/O failure.
    Io(String),
    /// The blob header magic did not match — not a Bennu index blob.
    BadMagic,
    /// The blob format version did not match [`FORMAT_VERSION`] — rebuild required.
    VersionMismatch { found: u64, expected: u64 },
    /// An fst build/load failure.
    Fst(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "store io: {e}"),
            StoreError::BadMagic => write!(f, "store: blob magic mismatch (not a bennu index)"),
            StoreError::VersionMismatch { found, expected } => {
                write!(f, "store: format version {found} != {expected} — rebuild required")
            }
            StoreError::Fst(e) => write!(f, "store fst: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// Round `n` up to the next multiple of `align`.
fn align_up(n: usize, align: usize) -> usize {
    (n + align - 1) & !(align - 1)
}

// ── Writer ───────────────────────────────────────────────────────────────────

/// Accumulates framed, 16-byte-aligned records + their fst keys, then writes the
/// blob (with header) and the fst map. Keys are collected then sorted + deduped at
/// [`finish`](BlobWriter::finish) (fst requires unique, sorted keys).
pub struct BlobWriter {
    blob: Vec<u8>,
    /// `(key, offset)` — the fst key for a record → the offset of its `len` prefix.
    entries: Vec<(String, u64)>,
}

impl Default for BlobWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BlobWriter {
    /// Start a fresh writer with the 16-byte format header already laid down, so the
    /// first record is 16-byte-aligned.
    pub fn new() -> Self {
        let mut blob = Vec::with_capacity(HEADER_LEN);
        blob.extend_from_slice(MAGIC);
        blob.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        debug_assert_eq!(blob.len(), HEADER_LEN);
        Self { blob, entries: Vec::new() }
    }

    /// Append one already-serialized rkyv record under fst `key`, framed as
    /// `[u32 len][bytes]` and padded to the next 16-byte boundary. Returns the
    /// record's offset (the offset of the `len` prefix).
    ///
    /// The caller serializes with `rkyv::to_bytes` (kept out of this crate's public
    /// surface so the writer stays schema-agnostic); [`prelude`](crate::prelude)
    /// re-exports a `serialize_record` helper for the common case.
    pub fn append(&mut self, key: &str, record_bytes: &[u8]) -> u64 {
        // Pad the blob so THIS record's frame starts 16-byte-aligned.
        let start = align_up(self.blob.len(), RECORD_ALIGN);
        self.blob.resize(start, 0);

        let len = record_bytes.len() as u32;
        self.blob.extend_from_slice(&len.to_le_bytes());
        self.blob.extend_from_slice(record_bytes);

        self.entries.push((key.to_string(), start as u64));
        start as u64
    }

    /// Add a second fst key pointing at an already-appended record `offset`. Lets one
    /// record be reachable under two names (e.g. a class by its simple name *and* its
    /// binary name) without duplicating the payload.
    pub fn append_alias(&mut self, key: &str, offset: u64) {
        self.entries.push((key.to_string(), offset));
    }

    /// Write the blob to `blob_path` and the fst map to `fst_path`. Keys are sorted
    /// and de-duplicated (keep-first) as fst requires.
    ///
    /// Ensures the target directory exists first: the store owns its files, so it can't assume the
    /// caller created (and kept) the parent — a concurrent generation GC could have removed it,
    /// which surfaced as `os error 3` (path not found) during index persist.
    pub fn finish(mut self, blob_path: &Path, fst_path: &Path) -> Result<(), StoreError> {
        for dir in [blob_path.parent(), fst_path.parent()].into_iter().flatten() {
            std::fs::create_dir_all(dir).map_err(|e| StoreError::Io(e.to_string()))?;
        }
        write_atomic(blob_path, &self.blob).map_err(|e| StoreError::Io(e.to_string()))?;

        self.entries.sort_by(|a, b| a.0.cmp(&b.0));
        self.entries.dedup_by(|a, b| a.0 == b.0);

        // Built in memory so the file only ever appears whole (see [`write_atomic`]).
        let mut mb = MapBuilder::memory();
        for (name, off) in &self.entries {
            mb.insert(name.as_bytes(), *off).map_err(|e| StoreError::Fst(e.to_string()))?;
        }
        let fst_bytes = mb.into_inner().map_err(|e| StoreError::Fst(e.to_string()))?;
        write_atomic(fst_path, &fst_bytes).map_err(|e| StoreError::Io(e.to_string()))
    }
}

/// Write `bytes` to `path` all-or-nothing: into a sibling `<path>.tmp`, flushed to disk, then
/// renamed over the target.
///
/// A plain `fs::write` truncates the target first, so a crash or kill mid-write leaves a short
/// file that is still a *valid-looking* one — the blob header check passes on it and the damage
/// only surfaces later, at the first read past the end. With the rename, a reader sees either the
/// previous complete file or the new complete one, never a torn one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    let written = (|| -> std::io::Result<()> {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    let outcome = written.and_then(|()| std::fs::rename(&tmp, path));
    if outcome.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    outcome
}

// ── Reader ───────────────────────────────────────────────────────────────────

/// The mmap'd blob. Validates the format header on open (rebuild-on-mismatch), then
/// slices records zero-copy by offset. The caller accesses the rkyv payload via
/// `rkyv::access::<Archived…>` with `bytecheck` (see the `query` module).
#[derive(Debug)]
pub struct BlobReader {
    mmap: Mmap,
}

impl BlobReader {
    /// mmap `blob_path` and validate its 16-byte header. `Err(VersionMismatch)` /
    /// `Err(BadMagic)` tell the caller to rebuild.
    pub fn open(blob_path: &Path) -> Result<Self, StoreError> {
        let file = File::open(blob_path).map_err(|e| StoreError::Io(e.to_string()))?;
        // SAFETY: the file is a Bennu-owned index blob under our cache dir; we don't
        // mutate it while mapped. Same contract as the proven spike.
        let mmap = unsafe { Mmap::map(&file) }.map_err(|e| StoreError::Io(e.to_string()))?;
        if mmap.len() < HEADER_LEN || &mmap[0..8] != MAGIC {
            return Err(StoreError::BadMagic);
        }
        let version = u64::from_le_bytes(mmap[8..16].try_into().expect("8 bytes"));
        if version != FORMAT_VERSION {
            return Err(StoreError::VersionMismatch { found: version, expected: FORMAT_VERSION });
        }
        Ok(Self { mmap })
    }

    /// The record bytes at `offset` (the offset of the `len` prefix). Reads the
    /// `u32` length then returns the `..len` slice. An offset or length that runs past the
    /// blob (a truncated or corrupt file — the header check cannot see that) yields an EMPTY
    /// slice, which every caller's `rkyv::access` rejects as an error, so damage costs a
    /// missing record rather than a panic in the backend.
    pub fn record_bytes(&self, offset: u64) -> &[u8] {
        let Ok(start) = usize::try_from(offset) else { return &[] };
        let Some(body) = start.checked_add(4) else { return &[] };
        let Some(prefix) = self.mmap.get(start..body) else { return &[] };
        let len = u32::from_le_bytes(prefix.try_into().expect("4 bytes")) as usize;
        let Some(end) = body.checked_add(len) else { return &[] };
        self.mmap.get(body..end).unwrap_or(&[])
    }

    /// The whole mmap. Used by the relation store's run decoder, whose frame is
    /// `[u32 count][ (u32 len)(bytes) ]*` (a run of edges) rather than a single
    /// `[u32 len][bytes]` symbol record.
    pub fn raw(&self) -> &[u8] {
        &self.mmap
    }
}

/// Open an `fst::Map` from `fst_path` via mmap (low working set — docs §3).
pub fn open_fst_map(fst_path: &Path) -> Result<Map<Mmap>, StoreError> {
    let file = File::open(fst_path).map_err(|e| StoreError::Io(e.to_string()))?;
    // SAFETY: Bennu-owned fst file under our cache dir, not mutated while mapped.
    let mmap = unsafe { Mmap::map(&file) }.map_err(|e| StoreError::Io(e.to_string()))?;
    Map::new(mmap).map_err(|e| StoreError::Fst(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_rounds_to_16() {
        assert_eq!(align_up(0, 16), 0);
        assert_eq!(align_up(1, 16), 16);
        assert_eq!(align_up(16, 16), 16);
        assert_eq!(align_up(17, 16), 32);
    }

    #[test]
    fn header_is_16_bytes_and_first_record_aligned() {
        let mut w = BlobWriter::new();
        // First record offset must be 16-byte-aligned (== HEADER_LEN here).
        let off = w.append("alpha", &[1, 2, 3]);
        assert_eq!(off as usize % RECORD_ALIGN, 0);
        assert_eq!(off as usize, HEADER_LEN);
        // Second record must also land on a 16-byte boundary despite the 3-byte payload.
        let off2 = w.append("beta", &[9]);
        assert_eq!(off2 as usize % RECORD_ALIGN, 0);
        assert!(off2 > off);
    }
}
