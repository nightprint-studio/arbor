/**
 * ronStudioStore — single open RON document at a time, modal-driven.
 *
 * Mirrors the json-studio store but tracks the *current* edited text as a
 * first-class field — RON Studio is an editor, not just a viewer. Text
 * changes flow through `setText()` which updates the host's parse cache
 * (Tree/Errors views pull from there) and marks the document dirty.
 */

import {
  studioBackend,
  type EncodingInfo,
  type SchemaHint,
  type StudioMutateResult,
} from '$lib/ipc/studio-format';
import type { RonNodeKind, RonPrimitiveValue } from '$lib/types/ron-studio';

/** Pre-bound backend for the RON format. All host IPC for RON Studio
 *  flows through here; per FROZEN F17 there are no `ronStudio*`
 *  commands anymore — every call hits the format-agnostic
 *  `studio_*` Tauri commands with `format_id="ron"` baked in. */
const RON = studioBackend<RonNodeKind>('ron');

function basename(p: string): string {
  const norm = p.replace(/\\/g, '/');
  const i = norm.lastIndexOf('/');
  return i >= 0 ? norm.slice(i + 1) : norm;
}

function createRonStudioStore() {
  let open        = $state(false);
  let docId       = $state<string | null>(null);
  let title       = $state<string | null>(null);
  let sourcePath  = $state<string | null>(null);
  let sizeBytes   = $state<number | null>(null);
  let loading     = $state(false);
  let error       = $state<string | null>(null);

  // Text mirror — kept in sync with the host. `original` is the snapshot at
  // load (or last successful Save); `current` is the live edit state. Dirty
  // = original !== current (string compare; works for any text size that
  // makes sense in a RON file).
  let original    = $state<string>('');
  let current     = $state<string>('');

  // Parse status reflected to the UI.
  let parseError  = $state<string | null>(null);
  let rootKind    = $state<RonNodeKind | null>(null);
  let rootChildCount = $state<number>(0);

  // Undo / redo flags — refreshed from every mutation's result so the
  // footer buttons + keyboard shortcuts stay in lockstep with the host.
  let canUndo = $state(false);
  let canRedo = $state(false);

  // Schema auto-load hint surfaced by parse — populated either from a
  // `//! ron-studio:` directive at the top of the file or from a
  // sidecar `.ron-studio.toml` walking the folder hierarchy. Modal
  // consumes this in an $effect to pre-load the schema silently.
  let schemaHint = $state<SchemaHint | null>(null);

  // FROZEN F16 — encoding sniffed from the file bytes at open time.
  // Display-only in v1 (status-bar pill in the modal); driven by save
  // to round-trip windows-1252 / UTF-16 BOM legacy files losslessly.
  let encoding = $state<EncodingInfo>({ label: 'UTF-8', had_bom: false });

  async function openDoc(opts: {
    text?:         string;
    path?:         string;
    title?:        string | null;
    /** When the caller knows the active tab + the synthetic studio
     *  relative path (`external/<label>/foo.ron`, …), pass them
     *  through so the host can resolve schema bindings for files
     *  that live outside the repo tree. The walk-up sidecar lookup
     *  can't reach the repo's `.ron-studio.toml` for those. */
    tabId?:        string;
    relativePath?: string;
  }): Promise<void> {
    if (docId) {
      try { await RON.close(docId); } catch { /* best-effort */ }
      docId = null;
    }
    open       = true;
    loading    = true;
    error      = null;
    title      = opts.title ?? (opts.path ? basename(opts.path) : 'RON Studio');
    sourcePath = opts.path ?? null;
    sizeBytes  = null;
    original   = '';
    current    = '';
    parseError = null;
    rootKind   = null;
    rootChildCount = 0;
    try {
      const r = await RON.parse({
        text:         opts.text,
        path:         opts.path,
        tabId:        opts.tabId,
        relativePath: opts.relativePath,
      });
      sizeBytes   = r.size_bytes;
      parseError  = r.parse_error;
      rootKind    = r.root_kind as RonNodeKind | null;
      rootChildCount = r.child_count;
      schemaHint  = r.schema_hint;
      encoding    = r.encoding;
      // Host returns the raw text via a separate call (parse result keeps
      // the IPC payload small in case of multi-MB documents).
      const [orig, curr, sp] = await Promise.all([
        RON.rawOriginal(r.doc_id),
        RON.rawCurrent(r.doc_id),
        RON.sourcePath(r.doc_id),
      ]);
      original   = orig;
      current    = curr;
      sourcePath = sp;
      // Flip `docId` last — modal effects watch this and call
      // `syncTextFromStore()` inside `untrack(...)`, which would
      // capture an empty `current` if we'd set it earlier. By the
      // time the effect fires, the raw text is already in place
      // so the Text tab paints correctly on first open.
      docId       = r.doc_id;
      if (!opts.title && sp) title = basename(sp);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  /** Switch the active document to an already-open `docId` without
   *  closing the previous one. The host keeps every per-doc state
   *  (history, parse cache, indent), so the only thing we sync is the
   *  modal mirror — text, sourcePath, parse status, undo/redo flags.
   *  Returns true on success so the caller can decide whether to
   *  refresh ancillary UI (tree, diff). */
  async function switchTo(targetDocId: string): Promise<boolean> {
    if (docId === targetDocId) return false;
    try {
      const [snap, enc] = await Promise.all([
        RON.snapshot(targetDocId),
        RON.getEncoding(targetDocId),
      ]);
      // Reset BEFORE assigning the new docId — a stale schemaHint
      // carried over from the previous doc would otherwise let the
      // workspace-store stash $effect overwrite the new doc's
      // (correct) hint when the modal-side effect observes the docId
      // change.  Modal sets `schemaHint` again explicitly once it
      // re-loads the new tab's schema via `autoLoadSchemaFromHint`.
      schemaHint     = null;
      docId          = snap.doc_id;
      sourcePath     = snap.source_path;
      sizeBytes      = snap.size_bytes;
      original       = snap.original;
      current        = snap.current;
      parseError     = snap.parse_error;
      rootKind       = snap.root_kind;
      rootChildCount = snap.child_count;
      canUndo        = snap.can_undo;
      canRedo        = snap.can_redo;
      encoding       = enc;
      title          = basename(snap.source_path ?? 'Untitled');
      error          = null;
      loading        = false;
      open           = true;
      // Schema hint stays per-doc in the workspace store
      // (`schemaHintByDoc`); the modal queries it explicitly via
      // `getSchemaHint(docId)` after `switchTo` and re-syncs
      // `schemaHint` here if it loads a fresh schema.
      return true;
    } catch (e) {
      error = String(e);
      return false;
    }
  }

  async function closeDoc(): Promise<void> {
    if (docId) {
      try { await RON.close(docId); } catch { /* best-effort */ }
    }
    docId      = null;
    title      = null;
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
    canUndo    = false;
    canRedo    = false;
    schemaHint = null;
    encoding   = { label: 'UTF-8', had_bom: false };
  }

  /** Push the latest editor text back to the host. Updates the parse cache
   *  the Tree view reads from. Cheap enough to call on every keystroke —
   *  the caller (modal) debounces anyway. */
  async function setText(text: string): Promise<void> {
    current = text;
    if (!docId) return;
    try {
      const r = await RON.setText(docId, text);
      parseError     = r.parse_error;
      rootKind       = r.root_kind as RonNodeKind | null;
      rootChildCount = r.child_count;
      canUndo        = r.can_undo;
      canRedo        = r.can_redo;
    } catch (e) {
      // set_text shouldn't fail under normal use; surface as parse error
      // so the user sees the message rather than a silent stale tree.
      parseError = String(e);
    }
  }

  /** Apply a structured tree edit: the host mutates the parsed AST and
   *  regenerates the text. We mirror the new text locally so the textarea
   *  / highlight overlay redraw immediately, then return the result for
   *  the caller (modal) to refresh the tree from. */
  async function applyMutateResult(r: StudioMutateResult): Promise<void> {
    current        = r.text;
    parseError     = r.parse_error;
    rootKind       = r.root_kind as RonNodeKind | null;
    rootChildCount = r.child_count;
    canUndo        = r.can_undo;
    canRedo        = r.can_redo;
  }
  async function mutatePrimitive(path: string[], value: RonPrimitiveValue): Promise<void> {
    if (!docId) return;
    const r = await RON.applyMutation(docId, { kind: 'set_primitive', path, value });
    await applyMutateResult(r);
  }
  async function toggleOption(path: string[]): Promise<void> {
    if (!docId) return;
    const r = await RON.applyMutation(docId, { kind: 'toggle_option', path });
    await applyMutateResult(r);
  }
  async function replaceAt(path: string[], ronText: string): Promise<void> {
    if (!docId) return;
    const r = await RON.applyMutation(docId, { kind: 'replace_at', path, text: ronText });
    await applyMutateResult(r);
  }
  async function removeAt(path: string[]): Promise<void> {
    if (!docId) return;
    const r = await RON.applyMutation(docId, { kind: 'remove_at', path });
    await applyMutateResult(r);
  }
  async function insertField(path: string[], name: string, ronText: string): Promise<void> {
    if (!docId) return;
    const r = await RON.applyMutation(docId, { kind: 'insert_field', path, name, text: ronText });
    await applyMutateResult(r);
  }
  async function insertItem(path: string[], ronText: string): Promise<void> {
    if (!docId) return;
    const r = await RON.applyMutation(docId, { kind: 'insert_item', path, text: ronText });
    await applyMutateResult(r);
  }
  async function insertMapEntry(path: string[], keyText: string, valText: string): Promise<void> {
    if (!docId) return;
    const r = await RON.applyMutation(docId, { kind: 'insert_map_entry', path, key_text: keyText, val_text: valText });
    await applyMutateResult(r);
  }
  async function duplicateAt(path: string[]): Promise<void> {
    if (!docId) return;
    const r = await RON.applyMutation(docId, { kind: 'duplicate_at', path });
    await applyMutateResult(r);
  }
  async function moveItem(path: string[], delta: number): Promise<void> {
    if (!docId) return;
    const r = await RON.applyMutation(docId, { kind: 'move_item', path, delta });
    await applyMutateResult(r);
  }

  /** Step backward one entry in the document's history. Resolves to
   *  `false` when there's nothing to undo, `true` after a successful
   *  step (so the caller can decide whether to refresh ancillary UI). */
  async function undo(): Promise<boolean> {
    if (!docId || !canUndo) return false;
    try {
      const r = await RON.undo(docId);
      await applyMutateResult(r);
      return true;
    } catch (e) {
      console.warn('ron-studio: undo failed', e);
      return false;
    }
  }
  async function redo(): Promise<boolean> {
    if (!docId || !canRedo) return false;
    try {
      const r = await RON.redo(docId);
      await applyMutateResult(r);
      return true;
    } catch (e) {
      console.warn('ron-studio: redo failed', e);
      return false;
    }
  }

  /** Save current text to disk. When `path` is null, falls through to the
   *  bound source path; if there is none either, throws. Marks the
   *  document non-dirty by snapshotting `current → original` on success. */
  async function save(opts: { path?: string | null; bindToDoc: boolean }): Promise<void> {
    if (!docId) throw new Error('No open document.');
    const target = opts.path ?? sourcePath;
    if (!target) throw new Error('No path bound — use Save As to pick one.');
    await RON.save({
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
    get loading()        { return loading; },
    get error()          { return error; },
    get original()       { return original; },
    get current()        { return current; },
    get parseError()     { return parseError; },
    get rootKind()       { return rootKind; },
    get rootChildCount() { return rootChildCount; },
    get dirty()          { return original !== current; },
    get canUndo()        { return canUndo; },
    get canRedo()        { return canRedo; },
    get schemaHint()     { return schemaHint; },
    get encoding()       { return encoding; },
    openDoc,
    closeDoc,
    switchTo,
    setText,
    /** Public hook to sync the store from a post-mutation snapshot
     *  produced elsewhere (e.g. the F13 bulk-edit pipeline returns the
     *  MutateResult-shaped state directly so we skip a second IPC). */
    applyExternalMutate: applyMutateResult,
    mutatePrimitive,
    toggleOption,
    replaceAt,
    removeAt,
    insertField,
    insertItem,
    insertMapEntry,
    duplicateAt,
    moveItem,
    undo,
    redo,
    save,
  };
}

export const ronStudioStore = createRonStudioStore();
