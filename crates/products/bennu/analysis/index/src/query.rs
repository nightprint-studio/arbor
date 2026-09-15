//! The query engine: exact / prefix / fuzzy lookup over the fst + blob.
//!
//! Ported from `bennu-spike-index`'s `query` (docs §10): exact lookup slices one
//! record and accesses it zero-copy with `bytecheck` validation; prefix uses an fst
//! `Str::starts_with` automaton; fuzzy uses a `Levenshtein` automaton (the
//! `levenshtein` feature). All three run against the mmap'd files — the low-RAM
//! profile the spike measured (~15 MiB working set for a 2M-symbol index).
//!
//! [`SymbolIndex`] binds one fst map + one blob into a typed [`crate::schema::Symbol`]
//! reader; a caller with several maps (per-simple-name, per-fqn, per-file — docs §3)
//! holds several of these over the same blob.

use fst::automaton::{Automaton, Levenshtein, Str};
use fst::{IntoStreamer, Map, Streamer};
use memmap2::Mmap;
use rkyv::rancor::Error as RkyvError;

use crate::schema::{ArchivedSymbol, Symbol};
use crate::store::{BlobReader, StoreError};

/// Serialize one [`Symbol`] to rkyv bytes for [`crate::store::BlobWriter::append`].
/// Kept here (next to the reader) so the round-trip type is obvious.
pub fn serialize_symbol(sym: &Symbol) -> Result<Vec<u8>, String> {
    rkyv::to_bytes::<RkyvError>(sym).map(|b| b.to_vec()).map_err(|e| format!("rkyv serialize: {e}"))
}

/// A typed view: one name→offset fst map bound to the blob that holds the records.
/// Cheap to hold several (one per lookup axis) over the same [`BlobReader`].
pub struct SymbolIndex<'a> {
    map: Map<Mmap>,
    blob: &'a BlobReader,
}

impl<'a> SymbolIndex<'a> {
    /// Bind an already-opened fst map + blob reader (both mmap'd — see
    /// [`crate::store`]).
    pub fn new(map: Map<Mmap>, blob: &'a BlobReader) -> Self {
        Self { map, blob }
    }

    /// Number of keys in the map.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// **Exact** lookup: the record for `name`, accessed zero-copy off the mmap with
    /// `bytecheck` validation, then fully deserialized into an owned [`Symbol`].
    /// `Ok(None)` when the key is absent.
    pub fn get_exact(&self, name: &str) -> Result<Option<Symbol>, StoreError> {
        let Some(off) = self.map.get(name.as_bytes()) else {
            return Ok(None);
        };
        let rec = self.blob.record_bytes(off);
        let archived = rkyv::access::<ArchivedSymbol, RkyvError>(rec)
            .map_err(|e| StoreError::Io(format!("bytecheck: {e}")))?;
        let owned = rkyv::deserialize::<Symbol, RkyvError>(archived)
            .map_err(|e| StoreError::Io(format!("deserialize: {e}")))?;
        Ok(Some(owned))
    }

    /// **Prefix** query: every key starting with `prefix`, as `(name, offset)`. The
    /// autocomplete hot path (docs §3) — fst gives it for free.
    pub fn prefix(&self, prefix: &str) -> Vec<(String, u64)> {
        let matcher = Str::new(prefix).starts_with();
        let mut stream = self.map.search(&matcher).into_stream();
        let mut out = Vec::new();
        while let Some((k, off)) = stream.next() {
            out.push((String::from_utf8_lossy(k).into_owned(), off));
        }
        out
    }

    /// **Fuzzy** query: keys within Levenshtein `distance` of `term`, as
    /// `(name, offset)`. Powers typo-tolerant "find everywhere". `Err` only when the
    /// automaton can't be built (e.g. an over-long term for the distance).
    pub fn fuzzy(&self, term: &str, distance: u32) -> Result<Vec<(String, u64)>, StoreError> {
        let lev = Levenshtein::new(term, distance)
            .map_err(|e| StoreError::Fst(format!("levenshtein: {e}")))?;
        let mut stream = self.map.search(&lev).into_stream();
        let mut out = Vec::new();
        while let Some((k, off)) = stream.next() {
            out.push((String::from_utf8_lossy(k).into_owned(), off));
        }
        Ok(out)
    }

    /// Owned [`Symbol`] at a raw offset (e.g. one returned by [`prefix`](Self::prefix)
    /// / [`fuzzy`](Self::fuzzy)), zero-copy access + `bytecheck` + deserialize.
    pub fn record_at(&self, offset: u64) -> Result<Symbol, StoreError> {
        let rec = self.blob.record_bytes(offset);
        let archived = rkyv::access::<ArchivedSymbol, RkyvError>(rec)
            .map_err(|e| StoreError::Io(format!("bytecheck: {e}")))?;
        rkyv::deserialize::<Symbol, RkyvError>(archived)
            .map_err(|e| StoreError::Io(format!("deserialize: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Source, SymbolKind};
    use crate::store::{open_fst_map, BlobWriter};

    fn sym(id: u32, simple: &str, fqn: &str) -> Symbol {
        Symbol {
            id,
            kind: SymbolKind::Method,
            simple_name: simple.to_string(),
            fqn: fqn.to_string(),
            owner_id: u32::MAX,
            source: Source::ProjectSource,
            signature: format!("void {simple}()"),
            modifiers: "public".to_string(),
            loc_file: "Foo.java".to_string(),
            loc_start: 0,
            loc_end: 0,
            loc_container: String::new(),
            loc_class: String::new(),
            members_json: String::new(),
        }
    }

    #[test]
    fn round_trip_exact_prefix_fuzzy() {
        let dir = std::env::temp_dir().join(format!("bennu-index-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blob_path = dir.join("symbols.blob");
        let fst_path = dir.join("names.fst");

        let mut w = BlobWriter::new();
        for s in [
            sym(0, "emailAddress", "com.acme.Account.emailAddress"),
            sym(1, "getName", "com.acme.Account.getName"),
            sym(2, "getValue", "com.acme.Account.getValue"),
        ] {
            let bytes = serialize_symbol(&s).unwrap();
            w.append(&s.simple_name, &bytes);
        }
        w.finish(&blob_path, &fst_path).unwrap();

        let blob = BlobReader::open(&blob_path).unwrap();
        let map = open_fst_map(&fst_path).unwrap();
        let idx = SymbolIndex::new(map, &blob);

        // exact
        let hit = idx.get_exact("emailAddress").unwrap().expect("present");
        assert_eq!(hit.id, 0);
        assert_eq!(hit.fqn, "com.acme.Account.emailAddress");
        assert!(idx.get_exact("missing").unwrap().is_none());

        // prefix
        let mut names: Vec<String> = idx.prefix("get").into_iter().map(|(n, _)| n).collect();
        names.sort();
        assert_eq!(names, vec!["getName".to_string(), "getValue".to_string()]);

        // fuzzy (one deletion)
        let fuzzy: Vec<String> =
            idx.fuzzy("emailAdress", 2).unwrap().into_iter().map(|(n, _)| n).collect();
        assert!(fuzzy.contains(&"emailAddress".to_string()), "{fuzzy:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_mismatch_is_detected() {
        // A blob whose header version is bumped by hand must be rejected on open.
        let dir = std::env::temp_dir().join(format!("bennu-index-ver-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blob_path = dir.join("bad.blob");
        let fst_path = dir.join("bad.fst");

        let mut w = BlobWriter::new();
        let s = sym(0, "x", "x");
        w.append("x", &serialize_symbol(&s).unwrap());
        w.finish(&blob_path, &fst_path).unwrap();

        // Corrupt the version field (bytes 8..16) to a value != FORMAT_VERSION.
        let mut bytes = std::fs::read(&blob_path).unwrap();
        bytes[8] = 0xFF;
        std::fs::write(&blob_path, &bytes).unwrap();

        match BlobReader::open(&blob_path) {
            Err(StoreError::VersionMismatch { .. }) => {}
            other => panic!("expected VersionMismatch, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
