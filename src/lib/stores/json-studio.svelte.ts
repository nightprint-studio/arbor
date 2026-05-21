/**
 * jsonStudioStore — single open JSON document at a time, modal-driven.
 *
 * Mirrors `ronStudioStore`: tracks original / current text as first-class
 * state, exposes mutations + save + undo/redo. Phase 3.b promoted the JSON
 * backend from read-only to position-preserving editor; Phase 3.b.2 wires
 * those host-side capabilities through this store so the modal can edit
 * via tree mutations or raw textarea, save to disk preserving encoding
 * (FROZEN F16), and step undo/redo.
 *
 * Single-doc by design. JSON Studio is a focused inspector — multi-tab
 * workspace lives on RON only (no JSON-side workspace store).
 */

import {
  studioBackend,
  type EncodingInfo,
  type SchemaHint,
  type StudioMutateResult,
  type StudioPrimitiveValue,
} from '$lib/ipc/studio-format';

export type JsonNodeKind = 'object' | 'array' | 'string' | 'number' | 'bool' | 'null';

const JSON_BACKEND = studioBackend<JsonNodeKind>('json');

function basename(p: string): string {
  const norm = p.replace(/\\/g, '/');
  const i = norm.lastIndexOf('/');
  return i >= 0 ? norm.slice(i + 1) : norm;
}

/** localStorage key for the per-host "Don't show jsonc-in-json banner
 *  again" flag. The user dismisses globally — re-opens of the same or
 *  different `.json` files don't re-prompt. */
const JSONC_BANNER_DISMISSED_KEY = 'arbor:json-studio:jsonc-banner-dismissed';
/** localStorage key for the streaming-mode informational banner. */
const STREAM_BANNER_DISMISSED_KEY = 'arbor:json-studio:stream-banner-dismissed';

function readDismissed(key: string): boolean {
  if (typeof localStorage === 'undefined') return false;
  return localStorage.getItem(key) === '1';
}
function writeDismissed(key: string, on: boolean): void {
  if (typeof localStorage === 'undefined') return;
  if (on) localStorage.setItem(key, '1');
  else    localStorage.removeItem(key);
}

function createJsonStudioStore() {
  let open       = $state(false);
  let docId      = $state<string | null>(null);
  let title      = $state<string | null>(null);
  let sourcePath = $state<string | null>(null);
  let sizeBytes  = $state<number | null>(null);
  let loading    = $state(false);
  let error      = $state<string | null>(null);

  // Text mirror — see ron-studio for the rationale. Original is the
  // baseline (snapshot at load + last successful save); current is the
  // live edit state. Dirty = string compare.
  let original = $state<string>('');
  let current  = $state<string>('');

  let parseError     = $state<string | null>(null);
  let rootKind       = $state<JsonNodeKind | null>(null);
  let rootChildCount = $state<number>(0);

  let canUndo = $state(false);
  let canRedo = $state(false);

  // FROZEN F16 — sniffed from file bytes at open. Drives save round-trip
  // for windows-1252 / UTF-16 BOM legacy files.
  let encoding = $state<EncodingInfo>({ label: 'UTF-8', had_bom: false });

  // Phase 3.d — JSONC mode flags. `streamMode` is sticky per-doc (decided
  // at open, doesn't change). `isJsonc` reflects the extension. `hasJsoncFeatures`
  // tracks the live buffer — re-set after every successful set_text /
  // mutation / undo so the banner stays in sync.
  let streamMode        = $state<boolean>(false);
  let isJsonc           = $state<boolean>(false);
  let hasJsoncFeatures  = $state<boolean>(false);
  let bannerDismissed   = $state<boolean>(false);
  let streamBannerDismissed = $state<boolean>(false);

  // Sidecar / cfg-keyed schema binding. Populated from `parse`'s
  // `schema_hint` when the doc was opened with `tabId + relativePath`.
  // The modal observes it and auto-loads the schema via the Schema
  // panel's normal probe/load flow. Mirrors RON's `schemaHint`.
  let schemaHint = $state<SchemaHint | null>(null);

  async function openDoc(opts: {
    text?:         string;
    path?:         string;
    title?:        string | null;
    tabId?:        string;
    relativePath?: string;
  }): Promise<void> {
    if (docId) {
      try { await JSON_BACKEND.close(docId); } catch { /* best-effort */ }
      docId = null;
    }
    open       = true;
    loading    = true;
    error      = null;
    title      = opts.title ?? (opts.path ? basename(opts.path) : 'JSON Studio');
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
      const r = await JSON_BACKEND.parse({
        text:         opts.text,
        path:         opts.path,
        tabId:        opts.tabId,
        relativePath: opts.relativePath,
      });
      sizeBytes        = r.size_bytes;
      parseError       = r.parse_error;
      rootKind         = r.root_kind as JsonNodeKind | null;
      rootChildCount   = r.child_count;
      encoding         = r.encoding;
      streamMode       = r.stream_mode;
      isJsonc          = r.is_jsonc;
      hasJsoncFeatures = r.has_jsonc_features;
      schemaHint       = r.schema_hint ?? null;
      bannerDismissed       = readDismissed(JSONC_BANNER_DISMISSED_KEY);
      streamBannerDismissed = readDismissed(STREAM_BANNER_DISMISSED_KEY);
      const [orig, curr, sp] = await Promise.all([
        JSON_BACKEND.rawOriginal(r.doc_id),
        JSON_BACKEND.rawCurrent(r.doc_id),
        JSON_BACKEND.sourcePath(r.doc_id),
      ]);
      original   = orig;
      current    = curr;
      sourcePath = sp;
      // Flip docId LAST — modal effects watch this and would otherwise
      // capture an empty `current` on the first synchronous read.
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
      try { await JSON_BACKEND.close(docId); } catch { /* best-effort */ }
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
    streamMode        = false;
    isJsonc           = false;
    hasJsoncFeatures  = false;
    schemaHint        = null;
  }

  /** Push the latest editor text back to the host. Updates the parse
   *  cache the Tree view reads from. Cheap to call on every keystroke;
   *  the modal debounces. */
  async function setText(text: string): Promise<void> {
    current = text;
    if (!docId) return;
    try {
      const r = await JSON_BACKEND.setText(docId, text);
      parseError       = r.parse_error;
      rootKind         = r.root_kind as JsonNodeKind | null;
      rootChildCount   = r.child_count;
      canUndo          = r.can_undo;
      canRedo          = r.can_redo;
      hasJsoncFeatures = r.has_jsonc_features;
    } catch (e) {
      parseError = String(e);
    }
  }

  /** Apply a structured tree edit: host mutates the AST + emits the new
   *  text. Mirror locally so the editor / overlay paint immediately,
   *  then return so the caller (modal) refreshes the tree. */
  async function applyMutateResult(r: StudioMutateResult): Promise<void> {
    current          = r.text;
    parseError       = r.parse_error;
    rootKind         = r.root_kind as JsonNodeKind | null;
    rootChildCount   = r.child_count;
    canUndo          = r.can_undo;
    canRedo          = r.can_redo;
    hasJsoncFeatures = r.has_jsonc_features;
  }

  async function mutatePrimitive(path: string[], value: StudioPrimitiveValue): Promise<void> {
    if (!docId) return;
    const r = await JSON_BACKEND.applyMutation(docId, { kind: 'set_primitive', path, value });
    await applyMutateResult(r);
  }
  async function replaceAt(path: string[], jsonText: string): Promise<void> {
    if (!docId) return;
    const r = await JSON_BACKEND.applyMutation(docId, { kind: 'replace_at', path, text: jsonText });
    await applyMutateResult(r);
  }
  async function removeAt(path: string[]): Promise<void> {
    if (!docId) return;
    const r = await JSON_BACKEND.applyMutation(docId, { kind: 'remove_at', path });
    await applyMutateResult(r);
  }
  async function insertField(path: string[], name: string, jsonText: string): Promise<void> {
    if (!docId) return;
    const r = await JSON_BACKEND.applyMutation(docId, { kind: 'insert_field', path, name, text: jsonText });
    await applyMutateResult(r);
  }
  async function insertItem(path: string[], jsonText: string): Promise<void> {
    if (!docId) return;
    const r = await JSON_BACKEND.applyMutation(docId, { kind: 'insert_item', path, text: jsonText });
    await applyMutateResult(r);
  }
  async function duplicateAt(path: string[]): Promise<void> {
    if (!docId) return;
    const r = await JSON_BACKEND.applyMutation(docId, { kind: 'duplicate_at', path });
    await applyMutateResult(r);
  }
  async function moveItem(path: string[], delta: number): Promise<void> {
    if (!docId) return;
    const r = await JSON_BACKEND.applyMutation(docId, { kind: 'move_item', path, delta });
    await applyMutateResult(r);
  }

  async function undo(): Promise<boolean> {
    if (!docId || !canUndo) return false;
    try {
      const r = await JSON_BACKEND.undo(docId);
      await applyMutateResult(r);
      return true;
    } catch (e) {
      console.warn('json-studio: undo failed', e);
      return false;
    }
  }
  async function redo(): Promise<boolean> {
    if (!docId || !canRedo) return false;
    try {
      const r = await JSON_BACKEND.redo(docId);
      await applyMutateResult(r);
      return true;
    } catch (e) {
      console.warn('json-studio: redo failed', e);
      return false;
    }
  }

  /** Phase 3.d — re-emit the buffer without JSONC features (comments
   *  + trailing commas). Lossy by design; routes through the doc's
   *  history so undo restores the JSONC-flavoured text. */
  async function stripJsoncFeatures(): Promise<boolean> {
    if (!docId) return false;
    try {
      const r = await JSON_BACKEND.stripFeatures(docId);
      await applyMutateResult(r);
      return true;
    } catch (e) {
      error = String(e);
      return false;
    }
  }

  /** Rebind the doc's on-disk path to a `.jsonc` neighbour. Just
   *  swaps the source_path locally — no disk action; the next Save
   *  writes the new path. Returns the rewritten path. */
  function renameSourceToJsonc(): string | null {
    if (!sourcePath) return null;
    const norm = sourcePath.replace(/\\/g, '/');
    if (norm.toLowerCase().endsWith('.jsonc')) return sourcePath;
    const next = sourcePath.endsWith('.json')
      ? sourcePath + 'c'
      : sourcePath.replace(/\.json$/i, '.jsonc');
    sourcePath = next;
    title      = basename(next);
    isJsonc    = true;
    return next;
  }

  function dismissJsoncBanner(): void {
    bannerDismissed = true;
    writeDismissed(JSONC_BANNER_DISMISSED_KEY, true);
  }
  function dismissStreamBanner(): void {
    streamBannerDismissed = true;
    writeDismissed(STREAM_BANNER_DISMISSED_KEY, true);
  }

  /** Save current text to disk. Falls through to the bound source path
   *  when `path` is null; throws when neither is available. Marks the
   *  document non-dirty on success. */
  async function save(opts: { path?: string | null; bindToDoc: boolean }): Promise<void> {
    if (!docId) throw new Error('No open document.');
    const target = opts.path ?? sourcePath;
    if (!target) throw new Error('No path bound — use Save As to pick one.');
    await JSON_BACKEND.save({
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
    get streamMode()       { return streamMode; },
    get isJsonc()          { return isJsonc; },
    get hasJsoncFeatures() { return hasJsoncFeatures; },
    get bannerDismissed()  { return bannerDismissed; },
    get streamBannerDismissed() { return streamBannerDismissed; },
    get schemaHint()       { return schemaHint; },
    openDoc,
    closeDoc,
    setText,
    /** Sync the store from a post-mutation snapshot produced elsewhere
     *  (e.g. F13 bulk-edit pipeline returns the MutateResult-shaped
     *  state directly to skip a second IPC). */
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
    stripJsoncFeatures,
    renameSourceToJsonc,
    dismissJsoncBanner,
    dismissStreamBanner,
  };
}

export const jsonStudioStore = createJsonStudioStore();
