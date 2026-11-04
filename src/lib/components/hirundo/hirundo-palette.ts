/**
 * The Hirundo command palette, as data. Every user-facing verb is here by name; every request and every
 * environment is addressable, so "I know what it is called" is one box away.
 *
 * Pure: what is true now arrives as {@link HirundoPaletteContext}, what can be done as
 * {@link HirundoPaletteActions}. An entry whose `when` is false is filtered out — never listed and greyed.
 */

import { fuzzyMatch } from '$lib/utils/fuzzy';
import type { RightSection, SidebarSection } from '$lib/stores/hirundo/ui.svelte';
import type { FlatRequest } from './request-model';

export interface HirundoSection {
  id: SidebarSection;
  label: string;
  /** Lucide name, resolved through `paletteIcon`. */
  icon: string;
  shortcut: string;
  description: string;
}

/** The rail, the palette and the sidebar header share this one list. */
export const HIRUNDO_SECTIONS: HirundoSection[] = [
  { id: 'collections', label: 'Collections', icon: 'FolderTree', shortcut: 'Ctrl+1', description: 'Requests kept as files in the project, grouped in collections and folders.' },
  { id: 'detected', label: 'Detected from code', icon: 'Code2', shortcut: 'Ctrl+2', description: 'The endpoints Bennu finds in the Java sources of the projects you choose.' },
  { id: 'environments', label: 'Environments', icon: 'Globe', shortcut: 'Ctrl+3', description: 'Where requests go: local, dev, staging, prod and your own types.' },
];

export interface HirundoRightSection {
  id: RightSection;
  label: string;
  icon: string;
  shortcut: string;
  description: string;
}

/** The icons of the right rail; the palette and the shell share this list. */
export const HIRUNDO_RIGHT_SECTIONS: HirundoRightSection[] = [
  { id: 'runner', label: 'Runner', icon: 'Play', shortcut: 'Ctrl+4', description: 'Run a collection and see which tests pass.' },
  { id: 'history', label: 'History', icon: 'History', shortcut: 'Ctrl+5', description: 'What this window sent, and the runs of this project.' },
  { id: 'schedules', label: 'Schedules', icon: 'Clock', shortcut: 'Ctrl+6', description: 'Run a collection on a timetable.' },
  { id: 'cookies', label: 'Cookies', icon: 'Layers', shortcut: 'Ctrl+7', description: "The cookie jar of the active environment." },
  { id: 'stream', label: 'Event stream', icon: 'Send', shortcut: 'Ctrl+8', description: 'Server-Sent Events of the open request, live.' },
];

export interface HirundoPaletteContext {
  projectOpen: boolean;
  hasRequest: boolean;
  sending: boolean;
  requests: FlatRequest[];
  environments: { id: string; name: string }[];
  recents: string[];
  workspaces: { id: string; name: string }[];
  collections: { path: string; name: string }[];
}

export interface HirundoPaletteActions {
  run: (fn: () => void) => void;
  send: () => void;
  cancel: () => void;
  save: () => void;
  duplicate: () => void;
  newRequest: () => void;
  newCollection: () => void;
  curl: () => void;
  copyCurl: () => void;
  formatBody: () => void;
  bulkEdit: () => void;
  variables: () => void;
  requestSettings: () => void;
  diff: () => void;
  closeTab: () => void;
  findRequest: () => void;
  openRequest: (path: string) => void;
  environments: () => void;
  switchEnvironment: (id: string) => void;
  openProject: () => void;
  openRecent: (path: string) => void;
  showSection: (id: SidebarSection) => void;
  toggleSidebar: () => void;
  showRight: (id: RightSection) => void;
  newSoapRequest: () => void;
  importSoap: () => void;
  importOpenApi: () => void;
  importHttp: () => void;
  startStream: () => void;
  exportHttp: (path: string) => void;
  toggleBottom: () => void;
  askImber: () => void;
  goToController: () => void;
  copyLink: () => void;
  saveAs: () => void;
  workspaces: () => void;
  switchWorkspace: (id: string) => void;
  newSchedule: () => void;
  runCollection: (path: string) => void;
  openDocs: (section?: string) => void;
}

interface Raw {
  id: string;
  title: string;
  subtitle?: string;
  icon: string;
  shortcut?: string;
  when: boolean;
  action: () => void;
}

interface Section {
  id: string;
  label: string;
  items: Omit<Raw, 'when'>[];
}

export function buildHirundoPalette(query: string, ctx: HirundoPaletteContext, a: HirundoPaletteActions): Section[] {
  const open = ctx.projectOpen;
  const req = ctx.hasRequest;
  const run = a.run;

  const request: Raw[] = [
    { id: 'send', title: 'Send the request', icon: 'Send', shortcut: 'Ctrl+Enter', when: req && !ctx.sending, action: () => run(a.send) },
    { id: 'cancel', title: 'Cancel the request in flight', icon: 'Square', shortcut: 'Esc', when: req && ctx.sending, action: () => run(a.cancel) },
    { id: 'save', title: 'Save the request', icon: 'Save', shortcut: 'Ctrl+S', when: req, action: () => run(a.save) },
    { id: 'duplicate', title: 'Duplicate the request', icon: 'Copy', shortcut: 'Ctrl+D', when: req, action: () => run(a.duplicate) },
    { id: 'new-soap', title: 'New SOAP request', subtitle: 'A POST with an envelope, SOAPAction and the right headers', icon: 'Code2', when: open, action: () => run(a.newSoapRequest) },
    { id: 'stream', title: 'Listen to the open request as an event stream (SSE)', subtitle: 'Esc stops it', icon: 'Send', when: req, action: () => run(a.startStream) },
    { id: 'import-http', title: 'Import a .http file…', subtitle: 'IntelliJ HTTP Client format, with its http-client.env.json', icon: 'Download', when: open, action: () => run(a.importHttp) },
    ...ctx.collections.map((c): Raw => ({ id: `export-http:${c.path}`, title: `Export ${c.name} as a .http file…`, icon: 'Upload', when: open, action: () => run(() => a.exportHttp(c.path)) })),
    { id: 'import-openapi', title: 'Import an OpenAPI spec…', subtitle: 'JSON, from a file, a URL or pasted: you choose the operations', icon: 'Download', shortcut: 'Ctrl+Shift+Y', when: open, action: () => run(a.importOpenApi) },
    { id: 'import-soap', title: 'Import a WSDL or a SOAP envelope…', subtitle: 'A pasted text or the URL of a WSDL', icon: 'Download', when: open, action: () => run(a.importSoap) },
    { id: 'new', title: 'New request', icon: 'Plus', shortcut: 'Ctrl+N', when: open, action: () => run(a.newRequest) },
    { id: 'curl-in', title: 'Paste a cURL command…', subtitle: 'Also Chrome\'s "Copy as cURL"', icon: 'Terminal', shortcut: 'Ctrl+Shift+U', when: open, action: () => run(a.curl) },
    { id: 'curl-out', title: 'Copy the request as cURL', icon: 'Terminal', when: req, action: () => run(a.copyCurl) },
    { id: 'format', title: 'Format the body', icon: 'Braces', shortcut: 'Ctrl+Shift+F', when: req, action: () => run(a.formatBody) },
    { id: 'bulk', title: 'Bulk edit params, headers and variables', icon: 'ListChecks', shortcut: 'Ctrl+Shift+B', when: req, action: () => run(a.bulkEdit) },
    { id: 'vars', title: 'Show the resolved variables', subtitle: 'What will actually be sent', icon: 'Variable', shortcut: 'Ctrl+Shift+V', when: req, action: () => run(a.variables) },
    { id: 'diff', title: 'Compare with the previous response', icon: 'FileCode', when: req, action: () => run(a.diff) },
    { id: 'settings', title: 'Request settings…', subtitle: 'Timeout, redirects, TLS', icon: 'Settings2', when: req, action: () => run(a.requestSettings) },
    { id: 'save-as', title: 'Save as…', icon: 'Save', when: req, action: () => run(a.saveAs) },
    { id: 'imber', title: 'Ask Imber about this request', subtitle: 'Copies a prompt with the request, the answer and the failed checks', icon: 'Sparkles', when: req, action: () => run(a.askImber) },
    { id: 'controller', title: 'Go to the controller', subtitle: 'Opens the source Bennu found for this URL', icon: 'Code2', when: req, action: () => run(a.goToController) },
    { id: 'link', title: 'Copy the request file path', icon: 'Link', when: req, action: () => run(a.copyLink) },
    { id: 'close', title: 'Close the tab', icon: 'Square', shortcut: 'Ctrl+W', when: req, action: () => run(a.closeTab) },
  ];

  const project: Raw[] = [
    { id: 'find', title: 'Find a request…', icon: 'Search', shortcut: 'Ctrl+P', when: open, action: () => run(a.findRequest) },
    { id: 'new-collection', title: 'New collection…', icon: 'FolderTree', when: open, action: () => run(a.newCollection) },
    { id: 'envs', title: 'Environments…', subtitle: 'Switch, edit, define types', icon: 'Globe', shortcut: 'Ctrl+E', when: open, action: () => run(a.environments) },
    ...ctx.environments.map((e): Raw => ({ id: `env:${e.id}`, title: `Use environment ${e.name}`, icon: 'Globe', when: open, action: () => run(() => a.switchEnvironment(e.id)) })),
    { id: 'workspaces', title: 'Manage workspaces…', icon: 'Layers', when: true, action: () => run(a.workspaces) },
    ...ctx.workspaces.map((w): Raw => ({ id: `ws:${w.id}`, title: `Switch to workspace ${w.name}`, icon: 'Layers', when: true, action: () => run(() => a.switchWorkspace(w.id)) })),
    { id: 'schedule', title: 'New schedule…', subtitle: 'Run a collection on a timetable', icon: 'Clock', when: open && ctx.collections.length > 0, action: () => run(a.newSchedule) },
    ...ctx.collections.map((c): Raw => ({ id: `run:${c.path}`, title: `Run ${c.name}`, subtitle: 'Send every request and run the tests', icon: 'Play', when: open, action: () => run(() => a.runCollection(c.path)) })),
    { id: 'open-project', title: 'Open a project…', icon: 'FolderTree', shortcut: 'Ctrl+Shift+O', when: true, action: () => run(a.openProject) },
    ...ctx.recents.map((p): Raw => ({ id: `recent:${p}`, title: `Open ${p.split(/[\\/]/).filter(Boolean).pop() ?? p}`, subtitle: p, icon: 'FolderTree', when: true, action: () => run(() => a.openRecent(p)) })),
  ];

  const view: Raw[] = [
    ...HIRUNDO_SECTIONS.map((s): Raw => ({ id: `section:${s.id}`, title: `Show ${s.label}`, subtitle: s.description, icon: s.icon, shortcut: s.shortcut, when: open, action: () => run(() => a.showSection(s.id)) })),
    ...HIRUNDO_RIGHT_SECTIONS.map((p): Raw => ({ id: `right:${p.id}`, title: `Show ${p.label}`, subtitle: p.description, icon: p.icon, shortcut: p.shortcut, when: open, action: () => run(() => a.showRight(p.id)) })),
    { id: 'bottom', title: 'Toggle the response panel', icon: 'Square', shortcut: 'Ctrl+J', when: true, action: () => run(a.toggleBottom) },
    { id: 'sidebar', title: 'Toggle the sidebar', icon: 'FolderTree', shortcut: 'Ctrl+B', when: true, action: () => run(a.toggleSidebar) },
    { id: 'docs', title: 'Documentation', icon: 'BookOpen', shortcut: 'F1', when: true, action: () => run(() => a.openDocs()) },
    { id: 'shortcuts', title: 'Keyboard shortcuts', icon: 'Command', shortcut: 'Shift+F1', when: true, action: () => run(() => a.openDocs('shortcuts')) },
  ];

  const requests: Raw[] = ctx.requests.map((r) => ({ id: `req:${r.path}`, title: r.name, subtitle: `${r.method} · ${r.where}`, icon: 'Send', when: open, action: () => run(() => a.openRequest(r.path)) }));

  const groups: { id: string; label: string; raw: Raw[] }[] = [
    { id: 'request', label: 'Request', raw: request },
    { id: 'project', label: 'Project', raw: project },
    { id: 'view', label: 'View', raw: view },
    { id: 'requests', label: 'Requests', raw: requests },
  ];

  const q = query.trim();
  const out: Section[] = [];
  for (const g of groups) {
    const items = g.raw
      .filter((r) => r.when)
      .map((r) => ({ r, m: fuzzyMatch(`${r.title} ${r.subtitle ?? ''}`, q) }))
      .filter((x) => x.m !== null)
      // Requests only appear once something is typed: a project can hold hundreds.
      .filter(() => g.id !== 'requests' || q.length > 0)
      .sort((x, y) => (y.m?.score ?? 0) - (x.m?.score ?? 0))
      .map(({ r }) => ({ id: r.id, title: r.title, subtitle: r.subtitle, icon: r.icon, shortcut: r.shortcut, action: r.action }));
    if (items.length) out.push({ id: g.id, label: g.label, items });
  }
  return out;
}
