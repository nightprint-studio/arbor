/**
 * The Garrulus command palette, as data.
 *
 * Lives apart from the shell because it is the part of the product that grows
 * every time a verb is added, and a window shell that doubles in length each
 * release stops being readable as a layout. The shell keeps the layout and the
 * keyboard; this file keeps the catalogue of what can be done.
 *
 * Two rules the entries follow:
 *  • **everything actionable is here**, by name — a feature only reachable by
 *    clicking the right panel does not exist for someone working at the keyboard;
 *  • **every object is addressable**: vaults and note types are enumerated one by
 *    one, so "I know what it's called" turns into "it's open" without a detour
 *    through a tree.
 *
 * The shell passes the actions rather than defining them here: opening a dialog,
 * writing a note and reaching the backend are the shell's business, and this file
 * only decides how they are found and what they are called. It reads no store for
 * the same reason — what is true right now arrives as {@link GarrulusPaletteContext},
 * so the catalogue stays a pure function of what it is told.
 */

import {
  Activity,
  ArrowDownToLine, ArrowUpFromLine, BookOpen, Bug, CalendarDays, Command, FileText, FolderOpen,
  FolderTree, Hash, History as HistoryIcon, Keyboard, Layers, Link, ListTodo, PanelBottom,
  PanelLeft, Pencil, Plus, RefreshCw, RotateCcw, Search, Settings, Share2, Table2, Trash2,
  TriangleAlert, Upload, Zap,
} from 'lucide-svelte';

import type { IconComponent } from '$lib/types/icon';
import type { SidebarSection } from '$lib/stores/garrulus/ui.svelte';

const ICONS: Record<string, IconComponent> = {
  command: Command as unknown as IconComponent,
  note: FileText as unknown as IconComponent,
  notes: FolderTree as unknown as IconComponent,
  plus: Plus as unknown as IconComponent,
  search: Search as unknown as IconComponent,
  sync: RefreshCw as unknown as IconComponent,
  pull: ArrowDownToLine as unknown as IconComponent,
  push: ArrowUpFromLine as unknown as IconComponent,
  commit: Upload as unknown as IconComponent,
  alert: TriangleAlert as unknown as IconComponent,
  history: HistoryIcon as unknown as IconComponent,
  link: Link as unknown as IconComponent,
  graph: Share2 as unknown as IconComponent,
  table: Table2 as unknown as IconComponent,
  tasks: ListTodo as unknown as IconComponent,
  tags: Hash as unknown as IconComponent,
  types: Layers as unknown as IconComponent,
  type: Bug as unknown as IconComponent,
  daily: CalendarDays as unknown as IconComponent,
  rename: Pencil as unknown as IconComponent,
  trash: Trash2 as unknown as IconComponent,
  activity: Activity as unknown as IconComponent,
  restore: RotateCcw as unknown as IconComponent,
  folder: FolderOpen as unknown as IconComponent,
  export: FileText as unknown as IconComponent,
  zap: Zap as unknown as IconComponent,
  panelLeft: PanelLeft as unknown as IconComponent,
  panelBottom: PanelBottom as unknown as IconComponent,
  settings: Settings as unknown as IconComponent,
  keyboard: Keyboard as unknown as IconComponent,
  docs: BookOpen as unknown as IconComponent,
};

/** Resolve a palette entry's icon key. Unknown keys fall back rather than throw —
 *  a missing glyph must never be the reason a verb cannot be reached. */
export function garrulusPaletteIcon(name: string): IconComponent {
  return ICONS[name] ?? ICONS.command;
}

/**
 * The sidebar's sections, shared by the palette, the activity rail that draws
 * them and the panel that says what each one holds.
 *
 * One list rather than three: a label that reads "Tags" in the rail and "Tags
 * and fields" in the palette is two names for one thing, and whichever the user
 * learned is the one that will not be there next time.
 */
export interface GarrulusSection {
  id: SidebarSection;
  label: string;
  /** Icon key, resolved through {@link garrulusPaletteIcon}. */
  icon: string;
  shortcut: string;
  /** What the section holds — the palette's subtitle and the panel's own copy
   *  while it is still empty. */
  description: string;
}

export const GARRULUS_SECTIONS: GarrulusSection[] = [
  { id: 'notes', label: 'Notes', icon: 'notes', shortcut: 'Ctrl+1',
    description: 'The vault tree, pinned notes and recents.' },
  { id: 'search', label: 'Search', icon: 'search', shortcut: 'Ctrl+2',
    description: 'Full text plus structured filters — type:bug status:open and free text together.' },
  { id: 'tags', label: 'Tags and fields', icon: 'tags', shortcut: 'Ctrl+3',
    description: 'The tag vocabulary and the frontmatter fields the vault filters by.' },
  { id: 'types', label: 'Note types', icon: 'types', shortcut: 'Ctrl+4',
    description: 'Note types and their templates, read from inside the vault so they travel with it.' },
];

/** What the bottom dock holds, in the order its tabs appear. */
export const GARRULUS_DOCK_TABS: { id: string; label: string; icon: string }[] = [
  { id: 'tasks', label: 'Tasks', icon: 'tasks' },
  { id: 'problems', label: 'Problems', icon: 'alert' },
  { id: 'conflicts', label: 'Conflicts', icon: 'alert' },
  { id: 'history', label: 'History', icon: 'history' },
];

/**
 * What is true right now, as far as the catalogue needs to know.
 *
 * Passed in rather than read from a store so this file has no opinion about
 * where that state lives, and so an entry's `when` is decided by one readable
 * object instead of by six imports.
 */
export interface GarrulusPaletteContext {
  /** Is a vault open at all? Almost everything is gated on this. */
  vaultOpen: boolean;
  /** Vault-relative path of the note in front, or `null` when none is. */
  notePath: string | null;
  /** The `SyncState` tag — `syncStateTag()` from `$lib/ipc/garrulus`. */
  syncTag: string;
  /** Unresolved conflicts sitting in the vault. */
  conflicts: number;
  /** Can the configured destination answer for past revisions? A folder mirror
   *  cannot, and the history verbs are hidden rather than offered broken. */
  history: boolean;
  /** Note types declared inside the open vault — one "New note of type…" each. */
  types: { id: string; name: string }[];
  /** Every known vault, so each is addressable by name. */
  vaults: { id: string; displayName: string; path: string }[];
}

/**
 * What the shell owns and the palette borrows.
 *
 * **An optional action is a verb this window cannot perform yet**, and the
 * entries that need it are filtered out — never listed and greyed, never listed
 * and inert. That is the same doctrine as `when`, applied to the half of the
 * product whose surfaces have not landed: a palette whose every line works is
 * the only kind worth typing into, and a stub passed to keep the type happy
 * would put a lie in it. The shell passes exactly what it can do; the day the
 * editor mounts, passing `saveNote` is the whole of what makes "Save this note
 * now" appear.
 *
 * The required ones are the verbs the shell can always perform: they need only
 * a store and a dialog it already owns.
 */
export interface GarrulusPaletteActions {
  /** Close the palette, then perform — every entry goes through it. */
  run: (fn: () => void) => void;

  // Notes — all of these need an editor, and wait for one.
  newNote?: () => void;
  /** Open the type picker, or go straight to one when the entry named it. */
  newTypedNote?: (typeId?: string) => void;
  openDailyNote?: () => void;
  quickSwitch?: () => void;
  saveNote?: () => void;
  renameNote?: () => void;
  deleteNote?: () => void;
  /** Tag the open note as being of a type — the "promote to Bug" flow. */
  applyType?: (typeId?: string) => void;
  /** Intentions on the selection or the line (`Alt+Enter`). */
  intentions?: () => void;
  /** The unnamed, unfiled capture buffer. */
  inboxScratch?: () => void;
  exportNote?: (format: 'html' | 'pdf') => void;

  // Finding things
  search?: () => void;
  showSection: (id: SidebarSection) => void;
  /** One of {@link GARRULUS_DOCK_TABS}' ids. Absent until the dock exists. */
  showDock?: (id: string) => void;
  showGraph?: () => void;
  showTable?: () => void;
  showBacklinks?: () => void;

  // Sync — every one of these is a write, and every one is a click away by design
  syncNow: () => void;
  pull: () => void;
  push: () => void;
  /** Commit the dirty notes with a message the user writes. */
  commitWithMessage: () => void;
  showPending?: () => void;
  showConflicts?: () => void;
  showNoteHistory?: () => void;
  configureRemote: () => void;
  /** Create a private repository and adopt it. Present only while the
   *  destination form on screen actually offers that flow. */
  createRemoteRepo?: () => void;

  // Vault
  openVault: () => void;
  createVault: () => void;
  switchVault: (id: string) => void;
  closeVault: () => void;
  rebuildIndex: () => void;
  showTrash?: () => void;

  // View & app
  toggleSidebar: () => void;
  toggleDock?: () => void;
  openSettings?: () => void;
  openShortcuts: () => void;
  openDocs: (section?: string) => void;
  openAbout?: () => void;
}

type Raw = {
  id: string;
  title: string;
  subtitle?: string;
  icon: string;
  shortcut?: string;
  /** Whether the entry applies at all right now — filtered out, never greyed. */
  when: boolean;
  action: () => void;
};

/** One rendered section, structurally `PaletteSection` from `CommandPaletteShell`. */
interface Section {
  id: string;
  label: string;
  items: Omit<Raw, 'when'>[];
}

/**
 * The sections for the current query.
 *
 * Returns the shell's `PaletteSection[]` structurally rather than importing the
 * type out of a `.svelte` module into a `.ts` one — the shell annotates the call
 * site, so any drift in the shape is still a type error, at the place that
 * actually feeds the component.
 */
export function buildGarrulusPalette(
  query: string,
  ctx: GarrulusPaletteContext,
  a: GarrulusPaletteActions,
): Section[] {
  const q = query.trim().toLowerCase();
  const open = ctx.vaultOpen;
  const note = ctx.notePath;
  const hasRemote = ctx.syncTag !== 'no-remote';

  const noteItems: Raw[] = [
    { id: 'new', title: 'New note', icon: 'plus', shortcut: 'Ctrl+N', when: open && !!a.newNote, action: () => a.run(() => a.newNote?.()) },
    {
      id: 'new-typed',
      title: 'New note of a type…',
      subtitle: 'The type decides the folder, the filename and what the note starts with',
      icon: 'types',
      shortcut: 'Ctrl+Shift+N',
      when: open && ctx.types.length > 0 && !!a.newTypedNote,
      action: () => a.run(() => a.newTypedNote?.()),
    },
    // Every type is addressable by name: knowing it is a bug is the whole of the
    // decision, and going through a picker to say so is a step for nothing.
    ...ctx.types.map((t): Raw => ({
      id: `new-type:${t.id}`,
      title: `New ${t.name}`,
      icon: 'type',
      when: open && !!a.newTypedNote,
      action: () => a.run(() => a.newTypedNote?.(t.id)),
    })),
    {
      id: 'daily',
      title: "Today's daily note",
      subtitle: 'Created on the first line of the day, appended to after that',
      icon: 'daily',
      shortcut: 'Ctrl+D',
      when: open && !!a.openDailyNote,
      action: () => a.run(() => a.openDailyNote?.()),
    },
    {
      id: 'scratch',
      title: 'Inbox scratch',
      subtitle: 'An unnamed, unfiled buffer — file it later, or never',
      icon: 'zap',
      shortcut: 'Ctrl+Shift+Space',
      when: open && !!a.inboxScratch,
      action: () => a.run(() => a.inboxScratch?.()),
    },
    {
      id: 'switcher',
      title: 'Open a note by title…',
      subtitle: 'Matched loosely — type the words you remember, in any order',
      icon: 'note',
      shortcut: 'Ctrl+O',
      when: open && !!a.quickSwitch,
      action: () => a.run(() => a.quickSwitch?.()),
    },
    { id: 'save', title: 'Save this note now', icon: 'note', shortcut: 'Ctrl+S', when: !!note && !!a.saveNote, action: () => a.run(() => a.saveNote?.()) },
    {
      id: 'rename',
      title: 'Rename this note and update every link to it',
      subtitle: 'Shows what changes before it changes anything',
      icon: 'rename',
      shortcut: 'F2',
      when: !!note && !!a.renameNote,
      action: () => a.run(() => a.renameNote?.()),
    },
    {
      id: 'apply-type',
      title: 'Give this note a type…',
      subtitle: "Adds the type's missing headings and opens its fields — nothing already written is touched",
      icon: 'types',
      when: !!note && ctx.types.length > 0 && !!a.applyType,
      action: () => a.run(() => a.applyType?.()),
    },
    {
      id: 'intentions',
      title: 'Intentions on the selection…',
      subtitle: 'Extract to a new note, promote to a type, turn a line into a task, link a mention',
      icon: 'zap',
      shortcut: 'Alt+Enter',
      when: !!note && !!a.intentions,
      action: () => a.run(() => a.intentions?.()),
    },
    {
      id: 'delete',
      title: 'Delete this note',
      subtitle: "To the vault's own trash, where it can be restored without going through git",
      icon: 'trash',
      when: !!note && !!a.deleteNote,
      action: () => a.run(() => a.deleteNote?.()),
    },
    { id: 'export-html', title: 'Export this note as HTML…', subtitle: 'One self-contained file — styling and images travel with it', icon: 'export', shortcut: 'Alt+Shift+E', when: !!note && !!a.exportNote, action: () => a.run(() => a.exportNote?.('html')) },
    { id: 'export-pdf', title: 'Export this note as PDF…', icon: 'export', when: !!note && !!a.exportNote, action: () => a.run(() => a.exportNote?.('pdf')) },
  ];

  const findItems: Raw[] = [
    {
      id: 'search',
      title: 'Search the vault',
      subtitle: 'Full text and typed filters together — type:bug status:open and the words you remember',
      icon: 'search',
      shortcut: 'Ctrl+Shift+F',
      when: open && !!a.search,
      action: () => a.run(() => a.search?.()),
    },
    { id: 'backlinks', title: 'What links to this note', icon: 'link', when: !!note && !!a.showBacklinks, action: () => a.run(() => a.showBacklinks?.()) },
    { id: 'tasks', title: 'Tasks across the vault', icon: 'tasks', when: open && !!a.showDock, action: () => a.run(() => a.showDock?.('tasks')) },
    {
      id: 'problems',
      title: 'Problems in the vault',
      subtitle: 'Broken links, orphan notes, unreferenced attachments, notes with no type',
      icon: 'alert',
      when: open && !!a.showDock,
      action: () => a.run(() => a.showDock?.('problems')),
    },
    { id: 'graph', title: 'Show the link graph', icon: 'graph', when: open && !!a.showGraph, action: () => a.run(() => a.showGraph?.()) },
    { id: 'table', title: 'Show the notes as a table', subtitle: "Columns are the type's own fields, and the cells are editable", icon: 'table', when: open && !!a.showTable, action: () => a.run(() => a.showTable?.()) },
  ];

  const syncItems: Raw[] = [
    {
      id: 'sync',
      title: 'Sync now',
      subtitle: 'Commit what changed, pull, push — in that order, and only when asked',
      icon: 'sync',
      shortcut: 'Ctrl+Shift+S',
      when: hasRemote,
      action: () => a.run(a.syncNow),
    },
    { id: 'pull', title: 'Pull only', subtitle: 'Bring in what the other machine sent, send nothing', icon: 'pull', when: hasRemote, action: () => a.run(a.pull) },
    { id: 'push', title: 'Push only', icon: 'push', when: hasRemote, action: () => a.run(a.push) },
    {
      id: 'commit',
      title: 'Commit with a message…',
      subtitle: 'For the change that deserves one — the rest are described for you',
      icon: 'commit',
      when: hasRemote,
      action: () => a.run(a.commitWithMessage),
    },
    { id: 'pending', title: 'Show what would be sent', icon: 'commit', when: hasRemote && !!a.showPending, action: () => a.run(() => a.showPending?.()) },
    {
      id: 'conflicts',
      title: ctx.conflicts > 0 ? `Resolve the conflicts (${ctx.conflicts})` : 'Show the conflicts',
      subtitle: 'Both versions side by side — keep mine, take theirs, or merge by hand',
      icon: 'alert',
      when: hasRemote && !!a.showConflicts,
      action: () => a.run(() => a.showConflicts?.()),
    },
    {
      id: 'note-history',
      title: 'History of this note',
      subtitle: 'Every past version, with a diff and a restore',
      icon: 'history',
      when: !!note && ctx.history && !!a.showNoteHistory,
      action: () => a.run(() => a.showNoteHistory?.()),
    },
    {
      id: 'remote',
      title: hasRemote ? 'Change where this vault syncs…' : 'Choose where this vault syncs…',
      subtitle: 'A git remote, or a folder the vault is mirrored to',
      icon: 'sync',
      when: open,
      action: () => a.run(a.configureRemote),
    },
    {
      id: 'create-repo',
      title: 'Create a private repository for this vault…',
      subtitle: 'Private, with no public option anywhere — a note vault has no business being public',
      icon: 'sync',
      when: open && !hasRemote && !!a.createRemoteRepo,
      action: () => a.run(() => a.createRemoteRepo?.()),
    },
  ];

  const vaultItems: Raw[] = [
    { id: 'open-vault', title: 'Open a vault…', subtitle: 'A folder of markdown notes — Obsidian vaults open as they are', icon: 'folder', when: true, action: () => a.run(a.openVault) },
    { id: 'create-vault', title: 'Create a vault…', icon: 'plus', when: true, action: () => a.run(a.createVault) },
    ...ctx.vaults.map((v): Raw => ({
      id: `vault:${v.id}`,
      title: `Open vault ${v.displayName}`,
      subtitle: v.path,
      icon: 'folder',
      when: true,
      action: () => a.run(() => a.switchVault(v.id)),
    })),
    { id: 'trash', title: 'Show the vault trash', subtitle: 'Deleted notes, restorable to where they were', icon: 'restore', when: open && !!a.showTrash, action: () => a.run(() => a.showTrash?.()) },
    {
      id: 'rebuild',
      title: 'Rebuild the index',
      subtitle: 'Re-reads every note. The answer to a vault changed by something other than Garrulus',
      icon: 'sync',
      when: open,
      action: () => a.run(a.rebuildIndex),
    },
    { id: 'close-vault', title: 'Close the vault', icon: 'folder', when: open, action: () => a.run(a.closeVault) },
  ];

  const viewItems: Raw[] = [
    { id: 'sidebar', title: 'Toggle the sidebar', icon: 'panelLeft', shortcut: 'Ctrl+B', when: true, action: () => a.run(a.toggleSidebar) },
    { id: 'dock', title: 'Toggle the bottom dock', icon: 'panelBottom', shortcut: 'Ctrl+J', when: !!a.toggleDock, action: () => a.run(() => a.toggleDock?.()) },
    ...GARRULUS_SECTIONS.map((s): Raw => ({
      id: `sec:${s.id}`,
      title: `Show ${s.label}`,
      subtitle: s.description,
      icon: s.icon,
      shortcut: s.shortcut,
      when: true,
      action: () => a.run(() => a.showSection(s.id)),
    })),
    ...GARRULUS_DOCK_TABS.map((d): Raw => ({
      id: `dock:${d.id}`,
      title: `Show ${d.label}`,
      icon: d.icon,
      when: open && !!a.showDock,
      action: () => a.run(() => a.showDock?.(d.id)),
    })),
  ];

  const appItems: Raw[] = [
    { id: 'settings', title: 'Settings…', icon: 'settings', shortcut: 'Ctrl+,', when: !!a.openSettings, action: () => a.run(() => a.openSettings?.()) },
    { id: 'shortcuts', title: 'Keyboard shortcuts…', icon: 'keyboard', shortcut: 'Shift+F1', when: true, action: () => a.run(a.openShortcuts) },
    { id: 'docs', title: 'Documentation', icon: 'docs', shortcut: 'F1', when: true, action: () => a.run(() => a.openDocs()) },
    // The one page somebody goes looking for by name, because "what does the
    // button do when it says diverged" is a question asked at the button.
    {
      id: 'docs-sync',
      title: 'How syncing works',
      subtitle: 'What the button shows, what the background does, and what it never does',
      icon: 'docs',
      when: true,
      action: () => a.run(() => a.openDocs('sync')),
    },
    { id: 'ai-activity', title: 'AI activity…', subtitle: 'What an AI client is doing right now, and what it has done', icon: 'activity', when: true, action: () => a.run(() => window.dispatchEvent(new CustomEvent('arbor:open-mcp-activity'))) },
    { id: 'about', title: 'About Garrulus', icon: 'command', when: !!a.openAbout, action: () => a.run(() => a.openAbout?.()) },
  ];

  const pack = (items: Raw[]) =>
    items
      .filter((c) => c.when && (!q || c.title.toLowerCase().includes(q) || (c.subtitle ?? '').toLowerCase().includes(q)))
      .map((c) => ({ id: c.id, title: c.title, subtitle: c.subtitle, icon: c.icon, shortcut: c.shortcut, action: c.action }));

  const out: Section[] = [];
  const nt = pack(noteItems); if (nt.length) out.push({ id: 'notes', label: 'Notes', items: nt });
  const fd = pack(findItems); if (fd.length) out.push({ id: 'find', label: 'Find', items: fd });
  const sy = pack(syncItems); if (sy.length) out.push({ id: 'sync', label: 'Sync', items: sy });
  const vl = pack(vaultItems); if (vl.length) out.push({ id: 'vault', label: 'Vault', items: vl });
  const vw = pack(viewItems); if (vw.length) out.push({ id: 'view', label: 'View', items: vw });
  const ap = pack(appItems); if (ap.length) out.push({ id: 'app', label: 'Application', items: ap });
  return out;
}
