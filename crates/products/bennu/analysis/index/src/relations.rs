//! The **relation** store: config-graph (and type-hierarchy) edges in their own
//! framed blob + fst, keyed by `from_id`, so an edge query slices only one node's
//! out-edges rather than scanning the symbol table (docs §3).
//!
//! Layout mirrors the symbol store's hardenings (16-byte-aligned runs, the same
//! format-version header, rebuild-on-mismatch) but the unit is a **run** of edges for a
//! single `from_id` — several edges share a node, so the fst value is the offset of a
//! `[u32 count][ (u32 len)(rkyv Relation bytes) ]*` frame. This is the seam the
//! config-graph (`bennu-web`) ingests onto and the resolver (`bennu-intel`) walks for
//! `action → class` (`ActionToClass` → `BeanIdToImpl`) and `result → view`
//! (`ResultToView`).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use fst::{Map, MapBuilder};
use memmap2::Mmap;
use rkyv::rancor::Error as RkyvError;

use crate::schema::{ArchivedRelation, Relation};
use crate::store::{open_fst_map, BlobReader, StoreError, FORMAT_VERSION, HEADER_LEN, MAGIC, RECORD_ALIGN};

/// fst key for a `from_id` — 10-digit zero-padded so the map's byte order matches the
/// numeric order (keeps a future range scan of a node-id window well-defined).
fn rel_key(from_id: u32) -> String {
    format!("{from_id:010}")
}

/// Round `n` up to the next multiple of `align` (local copy — `store::align_up` is
/// private; a 4-line helper isn't worth widening that crate-internal API).
fn align_up(n: usize, align: usize) -> usize {
    (n + align - 1) & !(align - 1)
}

/// Serialize one [`Relation`] to rkyv bytes.
pub fn serialize_relation(rel: &Relation) -> Result<Vec<u8>, String> {
    rkyv::to_bytes::<RkyvError>(rel).map(|b| b.to_vec()).map_err(|e| format!("rkyv serialize rel: {e}"))
}

/// Accumulates edges grouped by `from_id`, then writes the relation blob + its
/// `from_id → run-offset` fst. `BTreeMap` for a deterministic persist order.
pub struct RelationWriter {
    by_from: BTreeMap<u32, Vec<Relation>>,
}

impl Default for RelationWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl RelationWriter {
    /// Start an empty relation writer.
    pub fn new() -> Self {
        Self { by_from: BTreeMap::new() }
    }

    /// Add one edge. Grouped by its `from_id`.
    pub fn add(&mut self, rel: Relation) {
        self.by_from.entry(rel.from_id).or_default().push(rel);
    }

    /// The total number of edges.
    pub fn edge_count(&self) -> usize {
        self.by_from.values().map(|v| v.len()).sum()
    }

    /// The number of distinct `from` nodes.
    pub fn node_count(&self) -> usize {
        self.by_from.len()
    }

    /// Whether any edge was added.
    pub fn is_empty(&self) -> bool {
        self.by_from.is_empty()
    }

    /// Write the relation blob to `blob_path` and the `from_id → run-offset` fst to
    /// `fst_path`. Each node's edges are emitted as one 16-byte-aligned run.
    pub fn finish(self, blob_path: &Path, fst_path: &Path) -> Result<(), StoreError> {
        let mut blob: Vec<u8> = Vec::with_capacity(HEADER_LEN);
        blob.extend_from_slice(MAGIC);
        blob.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        debug_assert_eq!(blob.len(), HEADER_LEN);

        let mut entries: Vec<(String, u64)> = Vec::with_capacity(self.by_from.len());
        for (from_id, rels) in &self.by_from {
            let start = align_up(blob.len(), RECORD_ALIGN);
            blob.resize(start, 0);
            blob.extend_from_slice(&(rels.len() as u32).to_le_bytes());
            for r in rels {
                let bytes = serialize_relation(r).map_err(StoreError::Io)?;
                blob.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                blob.extend_from_slice(&bytes);
            }
            entries.push((rel_key(*from_id), start as u64));
        }

        {
            let mut f =
                BufWriter::new(File::create(blob_path).map_err(|e| StoreError::Io(e.to_string()))?);
            f.write_all(&blob).map_err(|e| StoreError::Io(e.to_string()))?;
            f.flush().map_err(|e| StoreError::Io(e.to_string()))?;
        }

        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let wtr =
            BufWriter::new(File::create(fst_path).map_err(|e| StoreError::Io(e.to_string()))?);
        let mut mb = MapBuilder::new(wtr).map_err(|e| StoreError::Fst(e.to_string()))?;
        for (k, off) in &entries {
            mb.insert(k.as_bytes(), *off).map_err(|e| StoreError::Fst(e.to_string()))?;
        }
        mb.finish().map_err(|e| StoreError::Fst(e.to_string()))?;
        Ok(())
    }
}

/// A read-only view of the relation store: the out-edges of a `from_id`. Held next to
/// the [`PersistedIndex`](crate::builder::PersistedIndex) by the resolver.
pub struct RelationReader {
    blob: BlobReader,
    map: Map<Mmap>,
}

impl RelationReader {
    /// Open the two mmap'd files. `Err(VersionMismatch)`/`Err(BadMagic)` → the caller
    /// rebuilds from sources (the symbol store carries the same version).
    pub fn open(blob_path: &Path, fst_path: &Path) -> Result<Self, StoreError> {
        let blob = BlobReader::open(blob_path)?;
        let map = open_fst_map(fst_path)?;
        Ok(Self { blob, map })
    }

    /// Every out-edge of `from_id` (its run in the blob), decoded. Empty when the node
    /// has no out-edges.
    pub fn out_edges(&self, from_id: u32) -> Vec<Relation> {
        let Some(off) = self.map.get(rel_key(from_id).as_bytes()) else {
            return Vec::new();
        };
        self.decode_run(off)
    }

    /// Out-edges of `from_id` of a given [`RelationKind`].
    pub fn out_edges_of_kind(&self, from_id: u32, kind: crate::schema::RelationKind) -> Vec<Relation> {
        self.out_edges(from_id).into_iter().filter(|r| r.kind == kind).collect()
    }

    /// The first out-edge of `from_id` matching `kind`, if any.
    pub fn first_out_edge(&self, from_id: u32, kind: crate::schema::RelationKind) -> Option<Relation> {
        self.out_edges(from_id).into_iter().find(|r| r.kind == kind)
    }

    /// The number of `from` nodes with at least one out-edge.
    pub fn node_count(&self) -> usize {
        self.map.len()
    }

    fn decode_run(&self, run_off: u64) -> Vec<Relation> {
        let mmap = self.blob.raw();
        let o = run_off as usize;
        if o + 4 > mmap.len() {
            return Vec::new();
        }
        let count = u32::from_le_bytes(mmap[o..o + 4].try_into().expect("4")) as usize;
        let mut cursor = o + 4;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            if cursor + 4 > mmap.len() {
                break;
            }
            let len = u32::from_le_bytes(mmap[cursor..cursor + 4].try_into().expect("4")) as usize;
            cursor += 4;
            if cursor + len > mmap.len() {
                break;
            }
            let rec = &mmap[cursor..cursor + len];
            cursor += len;
            if let Ok(archived) = rkyv::access::<ArchivedRelation, RkyvError>(rec) {
                if let Ok(r) = rkyv::deserialize::<Relation, RkyvError>(archived) {
                    out.push(r);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{RelationKind, Source};

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-rel-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn edge(from: u32, to: u32, kind: RelationKind, inferred: bool) -> Relation {
        Relation { from_id: from, to_id: to, kind, source: Source::StrutsAction, inferred }
    }

    #[test]
    fn writes_and_reads_out_edges_grouped_by_from() {
        let dir = tmp();
        let blob = dir.join("rel.blob");
        let fst = dir.join("rel.fst");

        let mut w = RelationWriter::new();
        // node 7 has two out-edges (one concrete, one inferred); node 3 has one.
        w.add(edge(7, 100, RelationKind::ActionToClass, false));
        w.add(edge(7, 101, RelationKind::ActionToResult, true));
        w.add(edge(3, 50, RelationKind::BeanIdToImpl, false));
        assert_eq!(w.edge_count(), 3);
        assert_eq!(w.node_count(), 2);
        w.finish(&blob, &fst).unwrap();

        let r = RelationReader::open(&blob, &fst).unwrap();
        let e7 = r.out_edges(7);
        assert_eq!(e7.len(), 2);
        assert!(e7.iter().any(|e| e.to_id == 100 && e.kind == RelationKind::ActionToClass && !e.inferred));
        assert!(e7.iter().any(|e| e.to_id == 101 && e.inferred));

        let e3 = r.out_edges_of_kind(3, RelationKind::BeanIdToImpl);
        assert_eq!(e3.len(), 1);
        assert_eq!(e3[0].to_id, 50);

        // an unknown node has no edges (not an error)
        assert!(r.out_edges(999).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
