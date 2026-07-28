/**
 * The Picus command palette, as data.
 *
 * Lives apart from `PicusShell` because it is the one part of the shell that
 * grows every time the product gains a verb, and a window shell that doubles in
 * length each release stops being readable as a layout. The shell keeps the
 * layout and the keyboard; this file keeps the catalogue of what can be done.
 *
 * Two rules the entries follow:
 *  • **everything actionable is here**, by name — a feature only reachable by
 *    clicking the right panel does not exist for someone working at the keyboard;
 *  • **every object is addressable**: connections, schema objects and script files
 *    are enumerated individually, so "I know what it's called" turns into "it's
 *    open" without a detour through a tree.
 *
 * The shell passes the actions rather than having them defined here: opening a
 * dialog, confirming a write and running a query are the shell's business, and
 * this file only decides how they are found and what they are called.
 */

import {
  BookOpen, Check, Command, Database, FileCode2, FolderOpen, FolderTree, FormInput,
  Info, Keyboard, Layers, PanelBottom, PanelLeft, Pencil, Play, Plus, RefreshCw,
  Settings, Table2, Trash2, TriangleAlert, Wrench,
} from 'lucide-svelte';

import type { IconComponent } from '$lib/types/icon';
import { connectionsStore } from '$lib/stores/picus/connections.svelte';
import { consistencyStore } from '$lib/stores/picus/consistency.svelte';
import { dmlStore } from '$lib/stores/picus/dml.svelte';
import { picusProjectStore } from '$lib/stores/picus/project.svelte';
import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
import { picusUiStore, type SidebarSection } from '$lib/stores/picus/ui.svelte';
import { schemaStore } from '$lib/stores/picus/schema.svelte';

const ICONS: Record<string, IconComponent> = {
  command: Command as unknown as IconComponent,
  database: Database as unknown as IconComponent,
  folder: FolderTree as unknown as IconComponent,
  folderOpen: FolderOpen as unknown as IconComponent,
  form: FormInput as unknown as IconComponent,
  layers: Layers as unknown as IconComponent,
  alert: TriangleAlert as unknown as IconComponent,
  play: Play as unknown as IconComponent,
  table: Table2 as unknown as IconComponent,
  file: FileCode2 as unknown as IconComponent,
  settings: Settings as unknown as IconComponent,
  keyboard: Keyboard as unknown as IconComponent,
  docs: BookOpen as unknown as IconComponent,
  plus: Plus as unknown as IconComponent,
  refresh: RefreshCw as unknown as IconComponent,
  panelLeft: PanelLeft as unknown as IconComponent,
  panelBottom: PanelBottom as unknown as IconComponent,
  check: Check as unknown as IconComponent,
  wrench: Wrench as unknown as IconComponent,
  pencil: Pencil as unknown as IconComponent,
  info: Info as unknown as IconComponent,
  trash: Trash2 as unknown as IconComponent,
};

export function picusPaletteIcon(name: string): IconComponent {
  return ICONS[name] ?? ICONS.command;
}

/** The rail's sections, shared by the palette and the shell that draws them. */
export const PICUS_SECTIONS: { id: SidebarSection; label: string; shortcut: string }[] = [
  { id: 'connections', label: 'Connections', shortcut: 'Ctrl+1' },
  { id: 'scripts', label: 'Scripts on disk', shortcut: 'Ctrl+2' },
  { id: 'generate', label: 'Generate DML', shortcut: 'Ctrl+3' },
  { id: 'inventory', label: 'Inventory', shortcut: 'Ctrl+4' },
];

/** What the shell owns and the palette borrows. */
export interface PicusPaletteActions {
  /** Close the palette, then perform — every entry goes through it. */
  run: (fn: () => void) => void;
  generate: () => void;
  requestWrite: () => void;
  runQuery: () => void;
  /** Step to the next / previous finding and open it. */
  stepFinding: (delta: number) => void;
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
 * Returns the shell's `PaletteSection[]` structurally rather than by importing the
 * type out of a `.svelte` module into a `.ts` one — the shell annotates the call
 * site, so any drift in the shape is still a type error, at the place that
 * actually feeds the component.
 */
export function buildPicusPalette(query: string, a: PicusPaletteActions): Section[] {
  const q = query.trim().toLowerCase();
  const tab = picusTabsStore.active;
  const activeConnectionId = connectionsStore.activeId;
  const attached = picusProjectStore.attached;

  const generateItems: Raw[] = [
    { id: 'gen', title: 'Generate DML', icon: 'form', shortcut: 'Ctrl+G', when: true, action: () => a.run(a.generate) },
    { id: 'write', title: 'Write the generated SQL to the scripts', icon: 'check', shortcut: 'Ctrl+Shift+W', when: dmlStore.generated && !dmlStore.applied, action: () => a.run(a.requestWrite) },
    { id: 'preview', title: 'Show what would change on disk', icon: 'wrench', when: dmlStore.generated, action: () => a.run(() => { picusUiStore.showBottom('changes'); void dmlStore.ensurePreview(); }) },
    { id: 'dest', title: 'Add a destination…', icon: 'plus', when: attached, action: () => a.run(() => picusUiStore.openAddDestination()) },
    { id: 'src-form', title: 'Source: guided form', icon: 'form', shortcut: 'Alt+1', when: true, action: () => a.run(() => { dmlStore.setSource('form'); picusTabsStore.openGenerate(); }) },
    { id: 'src-paste', title: 'Source: paste SQL', icon: 'form', shortcut: 'Alt+2', when: true, action: () => a.run(() => { dmlStore.setSource('paste'); picusTabsStore.openGenerate(); }) },
    { id: 'src-csv', title: 'Source: CSV', icon: 'form', shortcut: 'Alt+3', when: true, action: () => a.run(() => { dmlStore.setSource('csv'); picusTabsStore.openGenerate(); }) },
  ];

  const databaseItems: Raw[] = [
    { id: 'newquery', title: 'New query', icon: 'play', shortcut: 'Ctrl+T', when: true, action: () => a.run(() => picusTabsStore.openQuery()) },
    // `picusTabsStore.activeConnection`, NOT `connectionsStore.active`: the tab's
    // own binding, falling back to the sidebar only when it has none. They differ
    // the moment a tab is rebound to another database, and using the sidebar
    // highlight here would run the statement against a different server than the
    // one named above the editor.
    { id: 'runquery', title: 'Run the current query', icon: 'play', shortcut: 'Ctrl+Enter', when: tab?.kind === 'query', action: () => a.run(a.runQuery) },
    { id: 'newconn', title: 'Add a connection…', icon: 'plus', shortcut: 'Ctrl+Shift+N', when: true, action: () => a.run(() => picusUiStore.openConnectionEditor(null)) },
    { id: 'cycleconn', title: 'Switch to the next connection', icon: 'database', shortcut: 'Ctrl+Shift+D', when: connectionsStore.connections.length > 1, action: () => a.run(() => connectionsStore.cycle(1)) },
    { id: 'editconn', title: 'Edit the active connection…', icon: 'pencil', shortcut: 'F4', when: !!connectionsStore.active, action: () => a.run(() => picusUiStore.openConnectionEditor(activeConnectionId)) },
    // Every connection is addressable by name for each of the things you can do to
    // it — the keyboard path to what the sidebar row's menu offers to the mouse.
    ...connectionsStore.connections.flatMap((c): Raw[] => {
      const subtitle = `${c.alias} · ${c.schema}@${c.host}`;
      const root = connectionsStore.scriptRootFor(c.id);
      return [
        { id: `conn:${c.id}`, title: `Switch to ${c.name}`, subtitle, icon: 'database', when: true, action: () => a.run(() => connectionsStore.setActive(c.id)) },
        { id: `conn-edit:${c.id}`, title: `Edit connection ${c.name}…`, subtitle, icon: 'pencil', when: true, action: () => a.run(() => picusUiStore.openConnectionEditor(c.id)) },
        { id: `conn-info:${c.id}`, title: `Connection details: ${c.name}`, subtitle, icon: 'info', when: true, action: () => a.run(() => picusUiStore.openConnectionDetails(c.id)) },
        {
          id: `conn-root:${c.id}`,
          title: root ? `Change the scripts of ${c.name}…` : `Attach scripts to ${c.name}…`,
          subtitle: root || 'No script repository attached',
          icon: 'folderOpen',
          when: true,
          action: () => a.run(() => picusUiStore.openScriptRootPicker(c.id)),
        },
        { id: `conn-del:${c.id}`, title: `Delete connection ${c.name}…`, subtitle, icon: 'trash', when: true, action: () => a.run(() => picusUiStore.requestConnectionDelete(c.id)) },
      ];
    }),
    // Every schema object is reachable by name, whatever kind it is: the palette is
    // where "I know what it's called" turns into "it's open".
    ...schemaStore.relations.map((t): Raw => ({
      id: `object:${t.name}`,
      title: `Open ${t.kind} ${t.name}`,
      icon: 'table',
      when: true,
      action: () => a.run(() => picusTabsStore.openObject(t.name, t.kind)),
    })),
    ...schemaStore.sequences.map((s): Raw => ({
      id: `sequence:${s.name}`,
      title: `Open sequence ${s.name}`,
      icon: 'table',
      when: true,
      action: () => a.run(() => picusTabsStore.openObject(s.name, 'sequence')),
    })),
    ...schemaStore.triggers.map((t): Raw => ({
      id: `trigger:${t.name}`,
      title: `Open trigger ${t.name}`,
      subtitle: `on ${t.table}`,
      icon: 'table',
      when: true,
      action: () => a.run(() => picusTabsStore.openObject(t.name, 'trigger')),
    })),
  ];

  const scriptItems: Raw[] = [
    ...picusProjectStore.allFiles.map((f): Raw => ({
      id: `file:${f.path}`,
      title: `Open ${f.name}`,
      subtitle: f.path,
      icon: 'file',
      when: true,
      action: () => a.run(() => picusTabsStore.openFile(f.path, f.name, picusProjectStore.dialectOfFile(f.path))),
    })),
    {
      id: 'attach',
      title: attached ? 'Point this connection at another script folder…' : 'Attach a script repository to this connection…',
      subtitle: picusProjectStore.root || undefined,
      icon: 'folderOpen',
      when: !!activeConnectionId,
      action: () => a.run(() => picusUiStore.openScriptRootPicker(activeConnectionId)),
    },
    {
      id: 'rescan',
      title: 'Re-read the scripts from disk',
      icon: 'refresh',
      shortcut: 'F5',
      when: attached,
      action: () => a.run(() => void picusProjectStore.refresh()),
    },
  ];

  const checkItems: Raw[] = [
    { id: 'check', title: 'Run the consistency check', icon: 'alert', shortcut: 'Ctrl+Shift+K', when: attached, action: () => a.run(() => { picusUiStore.showBottom('consistency'); void picusProjectStore.analyze(); }) },
    { id: 'findings', title: 'Show the consistency report', icon: 'alert', when: true, action: () => a.run(() => picusUiStore.showBottom('consistency')) },
    { id: 'next-finding', title: 'Go to the next finding', icon: 'alert', shortcut: 'F8', when: consistencyStore.visible.length > 0, action: () => a.run(() => a.stepFinding(1)) },
    { id: 'prev-finding', title: 'Go to the previous finding', icon: 'alert', shortcut: 'Shift+F8', when: consistencyStore.visible.length > 0, action: () => a.run(() => a.stepFinding(-1)) },
    {
      id: 'suppressed',
      title: consistencyStore.showSuppressed ? 'Hide silenced findings' : 'Show silenced findings',
      subtitle: consistencyStore.suppressedCount
        ? `${consistencyStore.suppressedCount} silenced by a declared suppression`
        : 'Nothing is silenced in this repository',
      icon: 'alert',
      when: true,
      action: () => a.run(() => { picusUiStore.showBottom('consistency'); consistencyStore.toggleSuppressed(); }),
    },
    { id: 'changes', title: 'Show pending changes', icon: 'wrench', when: true, action: () => a.run(() => picusUiStore.showBottom('changes')) },
    { id: 'inventory', title: 'Open the inventory', icon: 'layers', shortcut: 'Ctrl+4', when: true, action: () => a.run(() => picusTabsStore.openInventory()) },
  ];

  const viewItems: Raw[] = [
    { id: 'sidebar', title: 'Toggle the sidebar', icon: 'panelLeft', shortcut: 'Ctrl+B', when: true, action: () => a.run(() => picusUiStore.toggleSidebar()) },
    { id: 'bottom', title: 'Toggle the bottom panel', icon: 'panelBottom', shortcut: 'Ctrl+J', when: true, action: () => a.run(() => picusUiStore.toggleBottom()) },
    ...PICUS_SECTIONS.map((s): Raw => ({
      id: `sec:${s.id}`,
      title: `Show ${s.label}`,
      icon: 'folder',
      shortcut: s.shortcut,
      when: true,
      action: () => a.run(() => picusUiStore.showSection(s.id)),
    })),
  ];

  const appItems: Raw[] = [
    { id: 'settings', title: 'Settings…', icon: 'settings', shortcut: 'Ctrl+,', when: true, action: () => a.run(() => picusUiStore.openSettings()) },
    { id: 'shortcuts', title: 'Keyboard shortcuts…', icon: 'keyboard', shortcut: 'Shift+F1', when: true, action: () => a.run(() => picusUiStore.openShortcuts()) },
    { id: 'docs', title: 'Documentation', icon: 'docs', shortcut: 'F1', when: true, action: () => a.run(() => picusUiStore.toggleDocs()) },
    { id: 'about', title: 'About Picus', icon: 'command', when: true, action: () => a.run(() => picusUiStore.openAbout()) },
  ];

  const pack = (items: Raw[]) =>
    items
      .filter((c) => c.when && (!q || c.title.toLowerCase().includes(q) || (c.subtitle ?? '').toLowerCase().includes(q)))
      .map((c) => ({ id: c.id, title: c.title, subtitle: c.subtitle, icon: c.icon, shortcut: c.shortcut, action: c.action }));

  const out: Section[] = [];
  const gen = pack(generateItems); if (gen.length) out.push({ id: 'generate', label: 'Generate', items: gen });
  const db = pack(databaseItems); if (db.length) out.push({ id: 'database', label: 'Database', items: db });
  const sc = pack(scriptItems); if (sc.length) out.push({ id: 'scripts', label: 'Scripts', items: sc });
  const ck = pack(checkItems); if (ck.length) out.push({ id: 'consistency', label: 'Consistency', items: ck });
  const vw = pack(viewItems); if (vw.length) out.push({ id: 'view', label: 'View', items: vw });
  const ap = pack(appItems); if (ap.length) out.push({ id: 'app', label: 'Application', items: ap });
  return out;
}
