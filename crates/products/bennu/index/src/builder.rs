//! The index **builder** and its **incremental patch** — plus the persisted
//! project-symbol **reader** used by the completion query.
//!
//! Leaf-clean: the builder knows only [`Symbol`](crate::schema::Symbol) records and
//! the fst keys each should be reachable under. The analyzer above (`bennu-intel`)
//! turns Java sources into these records; this crate only groups them per source file,
//! flattens them to the mmap'd store, and re-flattens on a single-file edit.
//!
//! ## Per-file grouping = cheap incremental patch
//!
//! Records are grouped by their **owning source file** (an [`IndexRecord`]'s file
//! key). A per-file edit is "drop this file's records, re-add the freshly extracted
//! ones, re-persist" ([`patch_file`](IndexBuilder::patch_file)) — no whole-project
//! re-parse. Persisting is fast (the disposable measured a full-project persist in a
//! couple ms), so re-flattening the whole record set on each patch is the simplest
//! correct strategy and stays well under the UI budget.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::query::serialize_symbol;
use crate::schema::{ArchivedSymbol, Symbol};
use crate::store::{open_fst_map, BlobReader, BlobWriter, StoreError};

/// One record to index: a [`Symbol`] plus every fst key it should be reachable under
/// (e.g. a class record under both its simple name and its binary name). `keys` must
/// be non-empty; each key maps to this same record in the fst.
#[derive(Debug, Clone)]
pub struct IndexRecord {
    /// The symbol payload written into the blob.
    pub symbol: Symbol,
    /// The fst keys this record is reachable under (simple name, binary name, …).
    pub keys: Vec<String>,
}

impl IndexRecord {
    /// A record reachable under a single key (the common case: a member under its
    /// simple name).
    pub fn new(symbol: Symbol, key: impl Into<String>) -> Self {
        Self { symbol, keys: vec![key.into()] }
    }

    /// Add an extra fst alias key (e.g. a class's binary name in addition to its
    /// simple name). Chainable.
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.keys.push(key.into());
        self
    }
}

/// Accumulates [`IndexRecord`]s grouped by source file, then flattens them to the
/// mmap'd store. Owns the paths of the two on-disk files it writes.
pub struct IndexBuilder {
    /// file path → its records. `BTreeMap` for a deterministic persist order.
    files: BTreeMap<PathBuf, Vec<IndexRecord>>,
    blob_path: PathBuf,
    fst_path: PathBuf,
}

impl IndexBuilder {
    /// Start a builder writing `<index_dir>/symbols.blob` + `<index_dir>/names.fst`.
    pub fn new(index_dir: &Path) -> Self {
        Self {
            files: BTreeMap::new(),
            blob_path: index_dir.join("symbols.blob"),
            fst_path: index_dir.join("names.fst"),
        }
    }

    /// Set (or replace) the records contributed by one source `file`. Replacing is how
    /// a re-ingest of a changed file stays consistent — the prior rows are dropped.
    pub fn set_file(&mut self, file: PathBuf, records: Vec<IndexRecord>) {
        if records.is_empty() {
            self.files.remove(&file);
        } else {
            self.files.insert(file, records);
        }
    }

    /// Incremental PATCH: replace one file's records (or, with `None`, drop them — a
    /// deleted file), then re-persist. Returns the persist result.
    pub fn patch_file(
        &mut self,
        file: PathBuf,
        records: Option<Vec<IndexRecord>>,
    ) -> Result<(), StoreError> {
        match records {
            Some(recs) => self.set_file(file, recs),
            None => {
                self.files.remove(&file);
            }
        }
        self.persist()
    }

    /// The number of source files currently contributing records.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The total number of records across all files.
    pub fn record_count(&self) -> usize {
        self.files.values().map(|v| v.len()).sum()
    }

    /// Flatten every file's records to the fst+blob store on disk. Each record is
    /// appended once and aliased under its extra keys, so a class is reachable by both
    /// its simple and binary name against one payload.
    pub fn persist(&self) -> Result<(), StoreError> {
        let mut writer = BlobWriter::new();
        for recs in self.files.values() {
            for rec in recs {
                let bytes = serialize_symbol(&rec.symbol).map_err(StoreError::Io)?;
                let primary = rec.keys.first().map(String::as_str).unwrap_or("");
                let off = writer.append(primary, &bytes);
                for extra in rec.keys.iter().skip(1) {
                    writer.append_alias(extra, off);
                }
            }
        }
        writer.finish(&self.blob_path, &self.fst_path)
    }

    /// The blob path this builder writes.
    pub fn blob_path(&self) -> &Path {
        &self.blob_path
    }
    /// The fst path this builder writes.
    pub fn fst_path(&self) -> &Path {
        &self.fst_path
    }
}

// ── read side ──────────────────────────────────────────────────────────────────

/// A read-only view of a persisted index: exact lookup by key + prefix scan, returning
/// the full [`Symbol`] records. The completion query holds one of these (the analyzer
/// reads `members_json` off the record); it never loads the blob onto the heap.
pub struct PersistedIndex {
    blob: BlobReader,
    map: fst::Map<memmap2::Mmap>,
}

impl PersistedIndex {
    /// Open the two mmap'd files. `Err(VersionMismatch)`/`Err(BadMagic)` → the caller
    /// rebuilds from sources.
    pub fn open(blob_path: &Path, fst_path: &Path) -> Result<Self, StoreError> {
        let blob = BlobReader::open(blob_path)?;
        let map = open_fst_map(fst_path)?;
        Ok(Self { blob, map })
    }

    /// The record stored under `key` (exact), or `None`. When several records share a
    /// key (fst keeps the first on a dup — see [`BlobWriter::finish`]), the first wins.
    pub fn get(&self, key: &str) -> Option<Symbol> {
        let off = self.map.get(key.as_bytes())?;
        self.symbol_at(off)
    }

    /// Every record whose key starts with `prefix`. Powers the "search everywhere"
    /// axis and, combined with member walking, member-access completion.
    pub fn prefix(&self, prefix: &str) -> Vec<Symbol> {
        use fst::automaton::{Automaton, Str};
        use fst::{IntoStreamer, Streamer};
        let matcher = Str::new(prefix).starts_with();
        let mut stream = self.map.search(&matcher).into_stream();
        let mut out = Vec::new();
        while let Some((_k, off)) = stream.next() {
            if let Some(s) = self.symbol_at(off) {
                out.push(s);
            }
        }
        out
    }

    /// Number of fst keys (not distinct records — a class contributes two keys).
    pub fn key_count(&self) -> usize {
        self.map.len()
    }

    fn symbol_at(&self, off: u64) -> Option<Symbol> {
        use rkyv::rancor::Error as RkyvError;
        let rec = self.blob.record_bytes(off);
        let archived = rkyv::access::<ArchivedSymbol, RkyvError>(rec).ok()?;
        rkyv::deserialize::<Symbol, RkyvError>(archived).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Source, SymbolKind};

    fn class_sym(id: u32, simple: &str, binary: &str, members_json: &str, file: &str) -> Symbol {
        Symbol {
            id,
            kind: SymbolKind::Class,
            simple_name: simple.to_string(),
            fqn: binary.to_string(),
            owner_id: u32::MAX,
            source: Source::ProjectSource,
            signature: format!("class {simple}"),
            modifiers: String::new(),
            loc_file: file.to_string(),
            loc_start: 0,
            loc_end: 0,
            loc_container: String::new(),
            loc_class: String::new(),
            members_json: members_json.to_string(),
        }
    }

    fn tmp() -> PathBuf {
        // Unique PER TEST: cargo runs tests concurrently, each on its own thread, so keying
        // only on the process id would share one dir across tests — they'd clobber each
        // other's blob/fst (wrong records read back) and, on Windows, one test's live mmap
        // would block another's rewrite (os error 1224). The thread id isolates them.
        let d = std::env::temp_dir().join(format!(
            "bennu-idx-build-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn persist_dual_key_and_read_back() {
        let dir = tmp();
        let mut b = IndexBuilder::new(&dir);
        let rec = IndexRecord::new(
            class_sym(0, "Customer", "com/acme/Customer", "{\"m\":1}", "Customer.java"),
            "Customer",
        )
        .with_key("com/acme/Customer");
        b.set_file(PathBuf::from("Customer.java"), vec![rec]);
        b.persist().unwrap();

        let idx = PersistedIndex::open(b.blob_path(), b.fst_path()).unwrap();
        // Reachable by BOTH the simple name and the binary name → same record.
        let by_simple = idx.get("Customer").expect("by simple");
        let by_binary = idx.get("com/acme/Customer").expect("by binary");
        assert_eq!(by_simple.id, 0);
        assert_eq!(by_binary.id, 0);
        assert_eq!(by_binary.members_json, "{\"m\":1}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn incremental_patch_replaces_one_file() {
        let dir = tmp();
        let mut b = IndexBuilder::new(&dir);
        b.set_file(
            PathBuf::from("A.java"),
            vec![IndexRecord::new(class_sym(0, "A", "p/A", "{}", "A.java"), "A")],
        );
        b.set_file(
            PathBuf::from("B.java"),
            vec![IndexRecord::new(class_sym(1, "B", "p/B", "{}", "B.java"), "B")],
        );
        b.persist().unwrap();
        assert_eq!(b.file_count(), 2);

        // Patch A.java: rename its type to A2. A must vanish, A2 must appear, B stays.
        b.patch_file(
            PathBuf::from("A.java"),
            Some(vec![IndexRecord::new(class_sym(0, "A2", "p/A2", "{}", "A.java"), "A2")]),
        )
        .unwrap();

        // Scope the mmap: it must drop BEFORE the next `patch_file` rewrites the same files,
        // or Windows refuses the rewrite (os error 1224 — a user-mapped section is open).
        {
            let idx = PersistedIndex::open(b.blob_path(), b.fst_path()).unwrap();
            assert!(idx.get("A").is_none(), "old symbol gone after patch");
            assert!(idx.get("A2").is_some(), "new symbol present after patch");
            assert!(idx.get("B").is_some(), "untouched file's symbol preserved");
        }

        // Delete B.java: its symbol must vanish.
        b.patch_file(PathBuf::from("B.java"), None).unwrap();
        {
            let idx = PersistedIndex::open(b.blob_path(), b.fst_path()).unwrap();
            assert!(idx.get("B").is_none(), "deleted file's symbol gone");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
