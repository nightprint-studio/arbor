/**
 * tomlStudioStore — single open TOML document at a time, modal-driven.
 *
 * Mirrors `jsonStudioStore`: tracks original / current text, exposes
 * mutations + save + undo/redo. Phase 4.b promotes the TOML backend to
 * a position-preserving editor via `toml_edit` (preserves comments +
 * formatting natively, no manual byte-splice).
 *
 * Single-doc by design. TOML Studio is a focused inspector — multi-tab
 * workspace lives on RON only.
 */

import {
  studioBackend,
  type EncodingInfo,
  type SchemaHint,
  type StudioMutateResult,
  type StudioPrimitiveValue,
} from '$lib/ipc/studio-format';

export type TomlNodeKind =
  | 'table'
  | 'inline_table'
  | 'array'
  | 'array_of_tables'
  | 'string'
  | 'integer'
  | 'float'
  | 'bool'
  | 'datetime';

const TOML_BACKEND = studioBackend<TomlNodeKind>('toml');

function basename(p: string): string {
  const norm = p.replace(/\\/g, '/');
  const i = norm.lastIndexOf('/');
  return i >= 0 ? norm.slice(i + 1) : norm;
}

function createTomlStudioStore() {
  let open       = $state(false);
  let docId      = $state<string | null>(null);
  let title      = $state<string | null>(null);
  let sourcePath = $state<string | null>(null);
  let sizeBytes  = $state<number | null>(null);
  let loading    = $state(false);
  let error      = $state<string | null>(null);

  let original = $state<string>('');
  let current  = $state<string>('');

  let parseError     = $state<string | null>(null);
  let rootKind       = $state<TomlNodeKind | null>(null);
  let rootChildCount = $state<number>(0);

  let canUndo = $state(false);
  let canRedo = $state(false);

  // FROZEN F16 — sniffed from file bytes at open. Drives save round-trip
  // for windows-1252 / UTF-16 BOM legacy files (rare in .toml but free).
  let encoding = $state<EncodingInfo>({ label: 'UTF-8', had_bom: false });

  // Sidecar / cfg-keyed schema binding (populated by `parse` when opened
  // with `tabId + relativePath`). Modal observes + auto-loads schema.
  let schemaHint = $state<SchemaHint | null>(null);

  async function openDoc(opts: {
    text?:         string;
    path?:         string;
    title?:        string | null;
    tabId?:        string;
    relativePath?: string;
  }): Promise<void> {
    if (docId) {
      try { await TOML_BACKEND.close(docId); } catch { /* best-effort */ }
      docId = null;
    }
    open       = true;
    loading    = true;
    error      = null;
    title      = opts.title ?? (opts.path ? basename(opts.path) : 'TOML Studio');
    sourcePath = opts.path ?? null;
    sizeBytes  = null;
    original   = '';
    current    = '';
    parseError = null;
    rootKind   = null;
    rootChildCount = 0;
    canUndo = false;
    canRedo = false;
    schemaHint = null;
    try {
      const r = await TOML_BACKEND.parse({
        text:         opts.text,
        path:         opts.path,
        tabId:        opts.tabId,
        relativePath: opts.relativePath,
      });
      sizeBytes      = r.size_bytes;
      parseError     = r.parse_error;
      rootKind       = r.root_kind as TomlNodeKind | null;
      rootChildCount = r.child_count;
      encoding       = r.encoding;
      schemaHint     = r.schema_hint ?? null;
      const [orig, curr, sp] = await Promise.all([
        TOML_BACKEND.rawOriginal(r.doc_id),
        TOML_BACKEND.rawCurrent(r.doc_id),
        TOML_BACKEND.sourcePath(r.doc_id),
      ]);
      original   = orig;
      current    = curr;
      sourcePath = sp;
      docId      = r.doc_id;
      if (!opts.title && sp) title = basename(sp);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function closeDoc(): Promise<void> {
    if (docId) {
      try { await TOML_BACKEND.close(docId); } catch { /* best-effort */ }
    }
    docId = null;
    title = null;
    sourcePath = null;
    sizeBytes  = null;
    error      = null;
    loading    = false;
    open       = false;
    original   = '';
    current    = '';
    parseError = null;
    rootKind   = null;
    rootChildCount = 0;
    canUndo = false;
    canRedo = false;
    encoding = { label: 'UTF-8', had_bom: false };
    schemaHint = null;
  }

  async function setText(text: string): Promise<void> {
    current = text;
    if (!docId) return;
    try {
      const r = await TOML_BACKEND.setText(docId, text);
      parseError     = r.parse_error;
      rootKind       = r.root_kind as TomlNodeKind | null;
      rootChildCount = r.child_count;
      canUndo        = r.can_undo;
      canRedo        = r.can_redo;
    } catch (e) {
      parseError = String(e);
    }
  }

  async function applyMutateResult(r: StudioMutateResult): Promise<void> {
    current        = r.text;
    parseError     = r.parse_error;
    rootKind       = r.root_kind as TomlNodeKind | null;
    rootChildCount = r.child_count;
    canUndo        = r.can_undo;
    canRedo        = r.can_redo;
  }

  async function mutatePrimitive(path: string[], value: StudioPrimitiveValue): Promise<void> {
    if (!docId) return;
    const r = await TOML_BACKEND.applyMutation(docId, { kind: 'set_primitive', path, value });
    await applyMutateResult(r);
  }
  async function replaceAt(path: string[], snippet: string): Promise<void> {
    if (!docId) return;
    const r = await TOML_BACKEND.applyMutation(docId, { kind: 'replace_at', path, text: snippet });
    await applyMutateResult(r);
  }
  async function removeAt(path: string[]): Promise<void> {
    if (!docId) return;
    const r = await TOML_BACKEND.applyMutation(docId, { kind: 'remove_at', path });
    await applyMutateResult(r);
  }
  async function insertField(path: string[], name: string, snippet: string): Promise<void> {
    if (!docId) return;
    const r = await TOML_BACKEND.applyMutation(docId, { kind: 'insert_field', path, name, text: snippet });
    await applyMutateResult(r);
  }
  async function insertItem(path: string[], snippet: string): Promise<void> {
    if (!docId) return;
    const r = await TOML_BACKEND.applyMutation(docId, { kind: 'insert_item', path, text: snippet });
    await applyMutateResult(r);
  }
  async function duplicateAt(path: string[]): Promise<void> {
    if (!docId) return;
    const r = await TOML_BACKEND.applyMutation(docId, { kind: 'duplicate_at', path });
    await applyMutateResult(r);
  }
  async function moveItem(path: string[], delta: number): Promise<void> {
    if (!docId) return;
    const r = await TOML_BACKEND.applyMutation(docId, { kind: 'move_item', path, delta });
    await applyMutateResult(r);
  }

  async function undo(): Promise<boolean> {
    if (!docId || !canUndo) return false;
    try {
      const r = await TOML_BACKEND.undo(docId);
      await applyMutateResult(r);
      return true;
    } catch (e) {
      console.warn('toml-studio: undo failed', e);
      return false;
    }
  }
  async function redo(): Promise<boolean> {
    if (!docId || !canRedo) return false;
    try {
      const r = await TOML_BACKEND.redo(docId);
      await applyMutateResult(r);
      return true;
    } catch (e) {
      console.warn('toml-studio: redo failed', e);
      return false;
    }
  }

  async function save(opts: { path?: string | null; bindToDoc: boolean }): Promise<void> {
    if (!docId) throw new Error('No open document.');
    const target = opts.path ?? sourcePath;
    if (!target) throw new Error('No path bound — use Save As to pick one.');
    await TOML_BACKEND.save({
      docId:     docId,
      path:      target,
      contents:  current,
      bindToDoc: opts.bindToDoc,
    });
    original = current;
    if (opts.bindToDoc) {
      sourcePath = target;
      title      = basename(target);
    }
  }

  return {
    get open()           { return open; },
    get docId()          { return docId; },
    get title()          { return title; },
    get sourcePath()     { return sourcePath; },
    get sizeBytes()      { return sizeBytes; },
    get parseError()     { return parseError; },
    get loading()        { return loading; },
    get error()          { return error; },
    get original()       { return original; },
    get current()        { return current; },
    get rootKind()       { return rootKind; },
    get rootChildCount() { return rootChildCount; },
    get dirty()          { return original !== current; },
    get canUndo()        { return canUndo; },
    get canRedo()        { return canRedo; },
    get encoding()       { return encoding; },
    get schemaHint()     { return schemaHint; },
    openDoc,
    closeDoc,
    setText,
    applyExternalMutate: applyMutateResult,
    mutatePrimitive,
    replaceAt,
    removeAt,
    insertField,
    insertItem,
    duplicateAt,
    moveItem,
    undo,
    redo,
    save,
  };
}

export const tomlStudioStore = createTomlStudioStore();
