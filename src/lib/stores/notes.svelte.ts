import type { CommitNote, NoteRemoteStatus } from '$lib/types/git';
import { listCommitNotes, checkNoteRemoteStatus } from '$lib/ipc/notes';

// ---------------------------------------------------------------------------
// Notes store — keyed by commit OID, loaded on demand
// ---------------------------------------------------------------------------

/** notes per commit: oid → CommitNote[] */
let notesByOid = $state<Map<string, CommitNote[]>>(new Map());

/** OIDs that have at least one note (used for graph badge) */
let oidsWithNotes = $state<Set<string>>(new Set());

/** Loading state for a specific oid */
let loadingOids = $state<Set<string>>(new Set());

function setNotes(oid: string, notes: CommitNote[]) {
  const m = new Map(notesByOid);
  m.set(oid, notes);
  notesByOid = m;

  const s = new Set(oidsWithNotes);
  if (notes.length > 0) s.add(oid);
  else s.delete(oid);
  oidsWithNotes = s;
}

function removeNote(oid: string, namespace: string) {
  const existing = notesByOid.get(oid) ?? [];
  const updated  = existing.filter(n => n.namespace !== namespace);
  setNotes(oid, updated);
}

function upsertNote(oid: string, namespace: string, content: string) {
  const existing = notesByOid.get(oid) ?? [];
  const idx = existing.findIndex(n => n.namespace === namespace);
  let updated: CommitNote[];
  if (idx >= 0) {
    updated = existing.map((n, i) =>
      i === idx ? { ...n, content, remote_status: 'local_only' as NoteRemoteStatus } : n
    );
  } else {
    updated = [...existing, { namespace, content, remote_status: 'local_only', created_at: Math.floor(Date.now() / 1000) }];
  }
  setNotes(oid, updated);
}

function updateRemoteStatus(oid: string, namespace: string, status: NoteRemoteStatus) {
  const existing = notesByOid.get(oid) ?? [];
  const updated  = existing.map(n =>
    n.namespace === namespace ? { ...n, remote_status: status } : n
  );
  setNotes(oid, updated);
}

export const notesStore = {
  // ── Reactive getters ──────────────────────────────────────────────────────

  get notesByOid() { return notesByOid; },
  get oidsWithNotes() { return oidsWithNotes; },

  /** Returns notes for a commit, or [] if not yet loaded. */
  getNotes(oid: string): CommitNote[] {
    return notesByOid.get(oid) ?? [];
  },

  noteCount(oid: string): number {
    return notesByOid.get(oid)?.length ?? 0;
  },

  hasNotes(oid: string): boolean {
    return oidsWithNotes.has(oid);
  },

  isLoading(oid: string): boolean {
    return loadingOids.has(oid);
  },

  // ── Data loading ─────────────────────────────────────────────────────────

  /** Load notes for a commit if not already cached. */
  async load(tabId: string, oid: string): Promise<CommitNote[]> {
    if (loadingOids.has(oid)) return notesByOid.get(oid) ?? [];

    const s = new Set(loadingOids);
    s.add(oid);
    loadingOids = s;

    try {
      const notes = await listCommitNotes(tabId, oid);
      setNotes(oid, notes);
      return notes;
    } finally {
      const s2 = new Set(loadingOids);
      s2.delete(oid);
      loadingOids = s2;
    }
  },

  /** Force-reload notes for a commit (called after save/delete). */
  async reload(tabId: string, oid: string): Promise<CommitNote[]> {
    const s = new Set(loadingOids);
    s.add(oid);
    loadingOids = s;
    try {
      const notes = await listCommitNotes(tabId, oid);
      setNotes(oid, notes);
      return notes;
    } finally {
      const s2 = new Set(loadingOids);
      s2.delete(oid);
      loadingOids = s2;
    }
  },

  /** Check remote status for one namespace and update the store. */
  async checkRemoteStatus(tabId: string, oid: string, namespace: string): Promise<NoteRemoteStatus> {
    const status = await checkNoteRemoteStatus(tabId, oid, namespace);
    updateRemoteStatus(oid, namespace, status);
    return status;
  },

  // ── Optimistic updates (applied before server round-trip) ─────────────────

  optimisticUpsert(oid: string, namespace: string, content: string) {
    upsertNote(oid, namespace, content);
  },

  optimisticRemove(oid: string, namespace: string) {
    removeNote(oid, namespace);
  },

  /** Invalidate cached notes for a commit (clears the OID from cache). */
  invalidate(oid: string) {
    const m = new Map(notesByOid);
    m.delete(oid);
    notesByOid = m;
    const s = new Set(oidsWithNotes);
    s.delete(oid);
    oidsWithNotes = s;
  },

  /** Clear everything (e.g. on tab switch). */
  clear() {
    notesByOid   = new Map();
    oidsWithNotes = new Set();
    loadingOids  = new Set();
  },
};
