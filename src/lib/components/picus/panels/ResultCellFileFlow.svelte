<script lang="ts">
  /**
   * Getting a value into — or out of — one cell, when the value is a file.
   *
   * Three entry points, all of them called by the grid's context menu and all of
   * them addressing a **row**, which is why they live together: reading a masked
   * large object, replacing one, and loading a file's text into an ordinary column
   * all need the same answer to "which row is this", and two copies of that would
   * be two chances to address it differently — on the operations where that matters
   * most.
   *
   * Renders nothing until asked. The component is the modals plus the state that
   * decides which of them is up.
   *
   * ## One picker and one confirmation, two destinations
   *
   * Which destination is decided by what the column holds, never by a preference:
   *
   *  • a **large object** takes the file's bytes, written immediately. Bytes cannot
   *    go through the pending-edit batch, which carries text: they would be stored
   *    as the base64 *of* the file, so the cell would look written and the document
   *    would be broken.
   *  • a **text column** takes the file's text as an ordinary pending change. Text
   *    is exactly what the batch carries, so it is marked, reviewable, written by
   *    Store and undone by Restore, like everything else in the grid.
   *
   * The confirmation is there for both, but it earns its place differently: for the
   * bytes it is the only review there will be, and for the text it is where the
   * decoding and the column's length get stated before the value is in the cell.
   */
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import LobViewerModal from './LobViewerModal.svelte';
  import { asCell } from './result-cells';
  import { decode, declaredLength } from './result-files';
  import { fsReadBytes } from '$lib/ipc/fs';
  import { writeLob } from '$lib/ipc/picus/db';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';
  import { resultEditStore, type Editability } from '$lib/stores/picus/result-edit.svelte';
  import type { PicusResult } from '$lib/stores/picus/result.svelte';
  import type { Connection } from '$lib/types/picus';

  interface Props {
    tabId: string;
    conn: Connection | null;
    result: PicusResult | null;
    editable: Editability;
    /** The relation the rows are from — what a large-object write addresses. */
    sourceTable: string;
  }

  let { tabId, conn, result, editable, sourceTable }: Props = $props();

  /**
   * The key one row is addressed by, or `null` with the reason already told.
   *
   * Shared by reading a large object and by replacing one, because they address the
   * same row the same way.
   */
  function rowKeysFor(rowIndex: number): Record<string, string | null> | null {
    if (!result) return null;
    // The key the backend addressed this read by, when it gave one — the primary key,
    // or the `ctid` it spliced in for a table that has none. It falls back to the
    // key `editability` derives for older results that carry none. Either way, no key
    // means nothing to address the value *by*, so we say why instead.
    const keyColumns = result.rowKey.length ? result.rowKey : editable.keys;
    if (!keyColumns.length) {
      toastStore.show(`This value cannot be addressed. ${editable.reason}`, 'warning');
      return null;
    }
    const row = result.rowAt(rowIndex);
    if (!row) {
      toastStore.show('That row is no longer loaded — scroll back to it and try again.', 'warning');
      return null;
    }
    const keys: Record<string, string | null> = {};
    for (const name of keyColumns) {
      const at = result.columns.findIndex((c) => c.name.toUpperCase() === name.toUpperCase());
      const value = at < 0 ? null : row[at];
      keys[name] = value === null || value === undefined ? null : String(value);
    }
    return keys;
  }

  // ── What is on screen ───────────────────────────────────────────────────────

  type PendingFile =
    | { kind: 'bytes'; path: string; base64: string; bytes: number }
    | { kind: 'text'; path: string; text: string; bytes: number; encoding: string; overflow: number | null };

  /** The large object being read, when one is open. */
  let opened = $state<{ column: string; keys: Record<string, string | null> } | null>(null);
  /** The cell a file is being put into, while the picker is up. */
  let filing = $state<
    | { rowIndex: number; column: string; kind: 'bytes'; keys: Record<string, string | null> }
    | { rowIndex: number; column: string; kind: 'text'; declared: string }
    | null
  >(null);
  /** The chosen file, read and measured, waiting for the confirmation. */
  let staged = $state<PendingFile | null>(null);
  let writingLob = $state(false);

  // ── The three entry points ──────────────────────────────────────────────────
  //
  // Exported so the grid can call them through `bind:this`. Imperative on purpose:
  // each is a *verb* the user has just chosen from a context menu, and modelling
  // that as a piece of state the parent sets would put an extra representable
  // state — "asked for, not yet acted on" — between the click and the dialog.

  /**
   * Read one masked value.
   *
   * The row is addressed by the key it was **read** with, exactly as an edit is —
   * and for the same reason the masking is only ever applied to a relation that has
   * one. A row that has scrolled out of memory cannot be addressed and says so
   * rather than fetching something arbitrary.
   */
  export function reveal(rowIndex: number, column: string) {
    const keys = rowKeysFor(rowIndex);
    if (keys) opened = { column, keys };
  }

  export function replaceLob(rowIndex: number, column: string) {
    const keys = rowKeysFor(rowIndex);
    if (keys) filing = { rowIndex, column, kind: 'bytes', keys };
  }

  export function loadText(rowIndex: number, column: string) {
    const declared = result?.columns.find((c) => c.name === column)?.type ?? '';
    filing = { rowIndex, column, kind: 'text', declared };
  }

  // ── Picking, reading, confirming ────────────────────────────────────────────

  /** Read the picked file. Nothing is written and nothing is staged as an edit yet. */
  async function stageFile(path: string) {
    const target = filing;
    if (!target) return;
    try {
      const base64 = await fsReadBytes(path);
      const raw = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
      if (target.kind === 'bytes') {
        staged = { kind: 'bytes', path, base64, bytes: raw.length };
        return;
      }
      const { text, encoding } = decode(raw);
      const limit = declaredLength(target.declared);
      staged = {
        kind: 'text',
        path,
        text,
        bytes: raw.length,
        encoding,
        // Stated rather than refused: the server is the authority on whether it
        // fits, and it gets the chance to say so at Store. What this avoids is
        // finding out only then.
        overflow: limit !== null && text.length > limit ? limit : null,
      };
    } catch (e) {
      filing = null;
      toastStore.show(`That file could not be read — ${e}`, 'error');
    }
  }

  async function commitFile() {
    const target = filing;
    const file = staged;
    if (!target || !file) return;

    if (target.kind === 'text' && file.kind === 'text') {
      const row = result?.rowAt(target.rowIndex);
      const at = result?.columns.findIndex((c) => c.name === target.column) ?? -1;
      resultEditStore.change(
        target.rowIndex,
        target.column,
        at >= 0 && row ? asCell(row[at]) : null,
        file.text,
      );
      filing = null;
      staged = null;
      return;
    }

    // Narrowed on the **target**, not on the file: `keys` lives on the target, and
    // it is the target's kind that decides which write this is.
    if (target.kind !== 'bytes' || file.kind !== 'bytes' || !conn || !sourceTable) return;
    writingLob = true;
    try {
      await writeLob(conn.id, sourceTable, target.keys, target.column, file.base64);
      toastStore.show(`${target.column} replaced with ${file.path.split(/[\\/]/).pop()}.`, 'success');
      // Re-read, for the reason every write here re-reads: the stored value is the
      // server's answer, and a grid showing the size of the file we sent would be
      // reporting our side of the exchange as if it were theirs.
      void queryStore.rerun(tabId, conn.id);
    } catch (e) {
      toastStore.show(`${target.column} was not written — ${e}`, 'error');
    } finally {
      writingLob = false;
      filing = null;
      staged = null;
    }
  }
</script>

{#if opened && conn}
  <LobViewerModal
    connectionId={conn.id}
    table={sourceTable}
    column={opened.column}
    keys={opened.keys}
    onClose={() => (opened = null)}
  />
{/if}

<!-- Pick the file. Nothing is read until it is chosen, and nothing is written or
     staged until the dialog below is answered. -->
{#if filing && !staged}
  <FileExplorerModal
    mode="file"
    title={filing.kind === 'bytes' ? `Replace ${filing.column}` : `Load into ${filing.column}`}
    onConfirm={(path) => void stageFile(path)}
    onCancel={() => (filing = null)}
  />
{/if}

<!-- One dialog, two outcomes. It is worded from what the file turned out to be:
     for bytes it is the only review there will be, and for text it is where the
     encoding and the column's length are stated before the value is in the cell. -->
{#if filing && staged}
  <ConfirmModal
    title={staged.kind === 'bytes'
      ? `Replace ${filing.column} with this file?`
      : `Load this file into ${filing.column}?`}
    message={staged.kind === 'bytes'
      ? `The value stored in ${sourceTable}.${filing.column} for this row will be `
        + 'overwritten. This is written straight away — it is not held with the other '
        + 'pending changes, and Restore does not undo it.'
      : "The file's text becomes a pending change on this cell, like any other edit: "
        + 'nothing reaches the database until Store, and Restore puts it back.'}
    detail={[
      staged.path,
      `${staged.bytes.toLocaleString()} bytes`,
      staged.kind === 'text' ? `read as ${staged.encoding}` : '',
      staged.kind === 'text' && staged.overflow !== null
        ? `${staged.text.length.toLocaleString()} characters — longer than the `
          + `${staged.overflow} this column declares. The server will refuse it at Store.`
        : '',
    ].filter(Boolean).join('\n')}
    variant={staged.kind === 'bytes' || staged.overflow !== null ? 'warning' : 'info'}
    confirmLabel={staged.kind === 'bytes' ? 'Replace' : 'Load'}
    cancelLabel="Keep the current value"
    busy={writingLob}
    onConfirm={() => void commitFile()}
    onCancel={() => { filing = null; staged = null; }}
  />
{/if}
