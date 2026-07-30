/**
 * The SQL editor's right-click menu — one definition, both editors.
 *
 * A query tab and a script tab are the same editor with different things around
 * it, and a menu that offered different verbs depending on which one you happened
 * to be in would be a bug met by switching tabs. So the edit trio and Generate are
 * defined here once, and each view contributes only the one action that is
 * genuinely its own: Run for a query, Save for a file.
 *
 * Generate is a **submenu**: six shapes flat in the menu would put the three verbs
 * everybody actually right-clicks for — cut, copy, paste — at the top of a list of
 * nine, and "Generate" is one intent, not six.
 */

import { ClipboardPaste, Copy, Play, Save, Scissors, Wand2 } from 'lucide-svelte';
import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
import { picusContextMenuStore } from '$lib/stores/picus/contextmenu.svelte';
import { OBJECT_KIND_ICONS } from './PicusObjectKindIcon.svelte';
import { SKELETON_LABELS, SKELETON_ORDER, skeleton, type SkeletonKind } from './generate-skeleton';
import type { Dialect } from '$lib/types/picus';

/**
 * What the menu needs from the editor under the pointer.
 *
 * Structural rather than the component's type: this is the surface being used,
 * and naming it here is what stops the menu from quietly depending on the rest of
 * `CodeEditor`.
 */
export interface EditorTarget {
  cutSelection: () => void;
  copySelection: () => void;
  pasteClipboard: () => Promise<void> | void;
  insertAtCursor: (text: string) => void;
  setCaretAtCoords: (x: number, y: number) => boolean;
}

export interface EditorMenuOptions {
  editor: EditorTarget | null;
  /** The buffer's dialect — decides which skeleton Generate writes. */
  dialect: Dialect | null;
  /** A query tab's own verb. */
  onRun?: () => void;
  /** A script tab's own verb. */
  onSave?: () => void;
}

/** `generate:` + the kind, so one handler covers all six. */
const GENERATE_PREFIX = 'generate:';

export function openEditorContextMenu(e: MouseEvent, opts: EditorMenuOptions) {
  const { editor, dialect, onRun, onSave } = opts;
  if (!editor) return;
  e.preventDefault();

  // Move the caret to where the click landed BEFORE building the menu: a
  // right-click does not move it on its own, so Generate would otherwise insert
  // its skeleton wherever the caret happened to be — very possibly off-screen.
  editor.setCaretAtCoords(e.clientX, e.clientY);

  const items: MenuItem[] = [
    { id: 'cut', label: 'Cut', icon: Scissors, shortcut: 'Ctrl+X' },
    { id: 'copy', label: 'Copy', icon: Copy, shortcut: 'Ctrl+C' },
    { id: 'paste', label: 'Paste', icon: ClipboardPaste, shortcut: 'Ctrl+V' },
    { id: 'sep-generate', label: '', separator: true },
    {
      id: 'generate',
      label: 'Generate',
      icon: Wand2,
      // The same icon the sidebar tree, the inventory and the go-to list draw for
      // that kind of object — `OBJECT_KIND_ICONS`, not a second set chosen here.
      // A sequence has to look like a sequence everywhere in the window, or the
      // badge stops being something you can read at a glance.
      children: SKELETON_ORDER.map((kind) => ({
        id: `${GENERATE_PREFIX}${kind}`,
        label: SKELETON_LABELS[kind],
        icon: OBJECT_KIND_ICONS[kind],
      })),
    },
  ];

  if (onRun || onSave) {
    items.push({ id: 'sep-verb', label: '', separator: true });
    if (onRun) items.push({ id: 'run', label: 'Run', icon: Play, shortcut: 'Ctrl+Enter' });
    if (onSave) items.push({ id: 'save', label: 'Save', icon: Save, shortcut: 'Ctrl+S' });
  }

  picusContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
    if (id.startsWith(GENERATE_PREFIX)) {
      const kind = id.slice(GENERATE_PREFIX.length) as SkeletonKind;
      editor.insertAtCursor(skeleton(kind, dialect));
      return;
    }
    switch (id) {
      case 'cut': editor.cutSelection(); break;
      case 'copy': editor.copySelection(); break;
      case 'paste': void editor.pasteClipboard(); break;
      case 'run': onRun?.(); break;
      case 'save': onSave?.(); break;
    }
  });
}
