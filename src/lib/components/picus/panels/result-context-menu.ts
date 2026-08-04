/**
 * The result grid's right-click menu.
 *
 * The verbs everybody arrives from pgAdmin and DBeaver expecting: copy what is
 * under the pointer, and set a value without typing it. They are here rather than
 * in the panel because the panel is already long, and because deciding *which*
 * entries a cell earns is a rule worth reading on its own.
 *
 * ## Nothing is offered that cannot happen
 *
 * Every entry below is gated on the state that makes it possible — the grid is
 * editable, the cell holds a large object, this cell has a pending edit. A menu
 * that lists **Set NULL** on a read-only connection and then explains itself in a
 * toast has taught the user to read the menu as decoration. The one thing worse
 * than a missing verb is a verb that does nothing, which this product has already
 * been caught doing once.
 *
 * ## Why NULL is in here when a shortcut already writes it
 *
 * `Ctrl`+`Enter` in the cell editor has always written NULL, and it stays the fast
 * way. But it is invisible: a cell editor takes text, no text means NULL, and
 * nothing on screen says which key does. A shortcut nobody can discover is a
 * feature only the person who wrote it has.
 *
 * So the menu names it, beside `Empty text` — because those two being different
 * values is the distinction the whole grid is careful about, and putting them
 * side by side is the clearest way to say so.
 */

import { Ban, ClipboardCopy, Copy, FileDigit, FileUp, Pencil, Rows3, Undo2 } from 'lucide-svelte';
import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
import { picusContextMenuStore } from '$lib/stores/picus/contextmenu.svelte';
import { resultEditStore } from '$lib/stores/picus/result-edit.svelte';
import { copyToClipboard } from '$lib/utils/clipboard';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import type { CellValue, Column } from '$lib/types/picus';

/** What the menu needs to know about the grid it was raised from. */
export interface ResultMenuTarget {
  rowIndex: number;
  columnIndex: number;
  columns: Column[];
  /** The row as it was read — the values a write is addressed by. */
  row: readonly CellValue[] | undefined;
  /** The columns whose values were not fetched, by name. */
  maskedColumns: readonly string[];
  /** Cells can be written at all — one source table, a key, and not read-only. */
  editable: boolean;
  /** Open the large-object viewer for this cell. */
  onReveal: (rowIndex: number, column: string) => void;
  /** Replace this cell's large object with the contents of a file. */
  onReplaceLob: (rowIndex: number, column: string) => void;
  /** Put a file's text into this cell, as a pending change. */
  onLoadText: (rowIndex: number, column: string) => void;
}

/**
 * Does this column hold text?
 *
 * Decided from the type the server reported, and deliberately narrow: loading a
 * file into a `numeric` is a mistake with no useful reading, so the entry is simply
 * not offered there. The length in `varchar(255)` is ignored here — whether the
 * text fits is checked against the file, later, where the number can be shown.
 */
function isTextual(type: string): boolean {
  const base = type.trim().toLowerCase().split('(')[0].trim();
  return [
    'text', 'citext', 'varchar', 'varchar2', 'nvarchar', 'nvarchar2', 'character varying',
    'char', 'nchar', 'character', 'clob', 'nclob', 'xml', 'json', 'jsonb',
    'longtext', 'mediumtext', 'tinytext',
  ].includes(base);
}

/**
 * A cell as text, for the clipboard.
 *
 * NULL copies as **nothing**. Copying the word `NULL` would paste a four-letter
 * string into whatever it lands in — a spreadsheet cell, a `WHERE` clause, another
 * grid — and every one of those reads it as data. The grid draws NULL differently
 * precisely because the distinction matters; the clipboard cannot draw, so it
 * carries the emptier of the two rather than a word that lies.
 */
function asText(value: CellValue | undefined): string {
  return value === null || value === undefined ? '' : String(value);
}

export function openResultContextMenu(event: MouseEvent, target: ResultMenuTarget) {
  const { rowIndex, columnIndex, columns, row, editable } = target;
  const column = columns[columnIndex];
  if (!column) return;
  event.preventDefault();

  const name = column.name;
  const value = row?.[columnIndex];
  const masked = target.maskedColumns.some((c) => c === name);
  const pending = resultEditStore.has(rowIndex, name);

  const items: MenuItem[] = [
    { id: 'copy', label: 'Copy', icon: Copy, shortcut: 'Ctrl+C' },
    { id: 'copy-row', label: 'Copy row', icon: Rows3 },
    { id: 'copy-column', label: 'Copy column name', icon: ClipboardCopy },
  ];

  if (editable) {
    items.push({ id: 'sep-edit', label: '', separator: true });
    items.push({ id: 'edit', label: 'Edit…', icon: Pencil, subtitle: 'or double-click' });
    // A submenu rather than four entries in the list: they are one intent — put a
    // value here without typing it — and the two that matter are at the top of a
    // short flyout instead of the middle of a long menu.
    items.push({
      id: 'set',
      label: 'Set',
      icon: Ban,
      children: [
        { id: 'set-null', label: 'NULL', subtitle: 'a real null, not an empty string' },
        { id: 'set-empty', label: 'Empty text', subtitle: "''" },
      ],
    });
    // A text column takes a file's *contents* as an ordinary pending change: text
    // is what the edit batch already carries, so this needs nothing special and
    // behaves like every other edit — marked, reviewable, written by Store and
    // undone by Restore. That is exactly why it is not offered on a large object,
    // where the value is bytes and the batch would store the encoding of them.
    if (!masked && isTextual(column.type)) {
      items.push({
        id: 'load-text',
        label: 'Load from file…',
        icon: FileUp,
        subtitle: "the file's text, as a pending change",
      });
    }
    if (pending) {
      items.push({
        id: 'revert-cell',
        label: 'Restore this cell',
        icon: Undo2,
        subtitle: 'back to the value it was read with',
      });
    }
  }

  // A large object was never fetched — the cell holds its size. Reading one is a
  // round trip for that single value, which is why it is an action rather than
  // something the grid did on its own.
  if (masked) {
    items.push({ id: 'sep-lob', label: '', separator: true });
    items.push({ id: 'open-lob', label: 'Open this value…', icon: FileDigit });
    // Writing one is not part of the pending-edit machinery: that batch carries
    // text values, and this replaces bytes in a single cell straight away. So it
    // is offered only where a write is possible at all, and it says in its own
    // subtitle that it does not wait for Store.
    if (editable) {
      items.push({
        id: 'replace-lob',
        label: 'Replace from file…',
        icon: FileUp,
        subtitle: 'written immediately, not with Store',
      });
    }
  }

  picusContextMenuStore.show(event.clientX, event.clientY, items, (id) => {
    switch (id) {
      case 'copy':
        void copyToClipboard(asText(value));
        break;
      case 'copy-row':
        // Tab-separated, which is what a spreadsheet reads as columns — the reason
        // anybody copies a whole row out of a grid.
        void copyToClipboard((row ?? []).map(asText).join('\t'));
        break;
      case 'copy-column':
        void copyToClipboard(name);
        break;
      case 'edit':
        resultEditStore.begin(rowIndex, name);
        break;
      case 'set-null':
        resultEditStore.change(rowIndex, name, value ?? null, null);
        break;
      case 'set-empty':
        resultEditStore.change(rowIndex, name, value ?? null, '');
        break;
      case 'revert-cell':
        resultEditStore.revertCell(rowIndex, name);
        break;
      case 'open-lob':
        target.onReveal(rowIndex, name);
        break;
      case 'replace-lob':
        target.onReplaceLob(rowIndex, name);
        break;
      case 'load-text':
        target.onLoadText(rowIndex, name);
        break;
      default:
        // A branch reached means an item was added above without a handler — say so
        // rather than doing nothing, which is the failure this menu is careful about.
        toastStore.show(`No action is wired for “${id}”.`, 'error');
        break;
    }
  });
}
