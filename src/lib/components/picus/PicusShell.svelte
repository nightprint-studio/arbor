<script lang="ts">
  /**
   * PicusShell — the SQL studio window.
   *
   * Arbor's standard layout, unchanged: title bar · activity rail · sidebar ·
   * centre column (tabs + contextual toolbar + document) · bottom dock · status
   * bar, all on the shared `WorkspaceShell` / `ActivityBar` / `PanelCard`
   * chrome. Someone coming from Corvus or Bennu should not notice they changed
   * application.
   *
   * The one Picus-specific arrangement: the consistency indicator sits at the
   * BOTTOM of the rail, separated from the sections, because it does not open a
   * sidebar — it reveals the bottom dock on the Consistency tab. It carries a
   * dot while blocking findings are open.
   *
   * Every action here is reachable from the keyboard; the canonical list lives
   * in `picus-shortcuts.ts` and this file's `onKeyDown` must stay in step with it.
   */
  import {
    Database, FolderTree, FormInput, Layers, TriangleAlert,
    Command, Play, Table2, FileCode2, Settings, Keyboard, BookOpen, Plus,
    RefreshCw, PanelLeft, PanelBottom, Check, Wrench,
  } from 'lucide-svelte';
  import WorkspaceShell from '$lib/components/shared/ui/WorkspaceShell.svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';
  import ActivityBar, { type ActivityRailItem } from '$lib/components/shared/ui/ActivityBar.svelte';
  import CommandPaletteShell, { type PaletteSection } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import FeedbackHost from '$lib/feedback/FeedbackHost.svelte';
  import FeedbackStatusButtons from '$lib/feedback/FeedbackStatusButtons.svelte';
  import type { IconComponent } from '$lib/types/icon';

  import PicusTitleBar from './shell/PicusTitleBar.svelte';
  import PicusStatusBar from './shell/PicusStatusBar.svelte';
  import PicusTabBar from './shell/PicusTabBar.svelte';
  import PicusToolbar from './shell/PicusToolbar.svelte';
  import ConnectionsPanel from './panels/ConnectionsPanel.svelte';
  import ScriptsPanel from './panels/ScriptsPanel.svelte';
  import GeneratePanel from './panels/GeneratePanel.svelte';
  import InventoryPanel from './panels/InventoryPanel.svelte';
  import PicusBottomDock from './panels/PicusBottomDock.svelte';
  import GenerateView from './views/GenerateView.svelte';
  import QueryView from './views/QueryView.svelte';
  import TableView from './views/TableView.svelte';
  import FileView from './views/FileView.svelte';
  import InventoryView from './views/InventoryView.svelte';
  import PicusSettingsModal from './PicusSettingsModal.svelte';
  import PicusShortcutsModal from './PicusShortcutsModal.svelte';
  import PicusAboutModal from './PicusAboutModal.svelte';
  import PicusConnectionModal from './PicusConnectionModal.svelte';
  import PicusDocsPanel from './PicusDocsPanel.svelte';
  import AddDestinationModal from './generate/AddDestinationModal.svelte';

  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusUiStore, type SidebarSection } from '$lib/stores/picus/ui.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { consistencyStore } from '$lib/stores/picus/consistency.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';
  import { picusSettingsStore } from '$lib/stores/picus/settings.svelte';

  let sidebarWidth = $state(280);
  let paletteQuery = $state('');
  /** Set while the write confirmation is up. */
  let confirmWrite = $state(false);

  const tab = $derived(picusTabsStore.active);

  // ── Activity rail ───────────────────────────────────────────────────────────
  const SECTIONS: { id: SidebarSection; icon: any; label: string; shortcut: string }[] = [
    { id: 'connections', icon: Database, label: 'Connections', shortcut: 'Ctrl+1' },
    { id: 'scripts', icon: FolderTree, label: 'Scripts on disk', shortcut: 'Ctrl+2' },
    { id: 'generate', icon: FormInput, label: 'Generate DML', shortcut: 'Ctrl+3' },
    { id: 'inventory', icon: Layers, label: 'Inventory', shortcut: 'Ctrl+4' },
  ];

  const railTop = $derived<ActivityRailItem[]>(
    SECTIONS.map((s) => ({
      id: s.id,
      icon: s.icon,
      tooltip: s.label,
      shortcut: s.shortcut,
      active: picusUiStore.sidebarOpen && picusUiStore.sidebarSection === s.id,
      onclick: () => picusUiStore.selectSection(s.id),
    })),
  );

  // Consistency lives apart, at the bottom: it opens the DOCK, not a sidebar.
  const railBottom = $derived<ActivityRailItem[]>([
    {
      id: 'consistency',
      icon: TriangleAlert,
      tooltip: consistencyStore.blockingCount
        ? `Consistency — ${consistencyStore.blockingCount} blocking`
        : consistencyStore.reviewCount
          ? `Consistency — ${consistencyStore.reviewCount} to check`
          : 'Consistency — no problem found',
      shortcut: 'Ctrl+J',
      active: picusUiStore.bottomOpen && picusUiStore.bottomTab === 'consistency',
      // Blockers are red, "worth checking" amber, clean shows nothing.
      dot: consistencyStore.blockingCount
        ? 'error'
        : consistencyStore.reviewCount
          ? 'warning'
          : false,
      onclick: () => picusUiStore.showBottom('consistency'),
    },
  ]);

  // ── Write flow ──────────────────────────────────────────────────────────────
  function requestWrite() {
    if (!dmlStore.generated || dmlStore.applied) return;
    if (!picusSettingsStore.confirmBeforeWrite) { applyWrite(); return; }
    confirmWrite = true;
  }

  function applyWrite() {
    confirmWrite = false;
    dmlStore.markApplied();
    toastStore.show(
      `${dmlStore.enabledTargets.length} file(s) written — encoding and line endings preserved.`,
      'success',
    );
  }

  function generate() {
    if (!dmlStore.canGenerate) {
      toastStore.show('Nothing to generate: check the values and enable at least one destination.', 'warning');
      return;
    }
    picusTabsStore.openGenerate();
    dmlStore.markGenerated();
  }

  /** The confirmation says exactly what will change — and what will not. */
  const writeDetail = $derived(
    dmlStore.enabledTargets.map((t) => t.file).join('\n') +
      '\n\nEncoding and line endings stay as they are.' +
      (picusSettingsStore.backupBeforeWrite
        ? '\nOriginals are copied to .arbor/backup first; if any file fails, all of them are rolled back.'
        : '\nBackups are disabled: a failed write cannot be rolled back.'),
  );

  // ── Command palette ─────────────────────────────────────────────────────────
  const ICONS: Record<string, IconComponent> = {
    command: Command as unknown as IconComponent,
    database: Database as unknown as IconComponent,
    folder: FolderTree as unknown as IconComponent,
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
  };
  function iconResolver(name: string): IconComponent { return ICONS[name] ?? ICONS.command; }

  function run(fn: () => void) { picusUiStore.closePalette(); queueMicrotask(fn); }

  const paletteSections = $derived.by<PaletteSection[]>(() => {
    const q = paletteQuery.trim().toLowerCase();

    const generateItems = [
      { id: 'gen', title: 'Generate DML', icon: 'form', shortcut: 'Ctrl+G', when: true, action: () => run(generate) },
      { id: 'write', title: 'Write the generated SQL to the scripts', icon: 'check', shortcut: 'Ctrl+Shift+W', when: dmlStore.generated && !dmlStore.applied, action: () => run(requestWrite) },
      { id: 'src-form', title: 'Source: guided form', icon: 'form', shortcut: 'Alt+1', when: true, action: () => run(() => { dmlStore.setSource('form'); picusTabsStore.openGenerate(); }) },
      { id: 'src-paste', title: 'Source: paste SQL', icon: 'form', shortcut: 'Alt+2', when: true, action: () => run(() => { dmlStore.setSource('paste'); picusTabsStore.openGenerate(); }) },
      { id: 'src-csv', title: 'Source: CSV', icon: 'form', shortcut: 'Alt+3', when: true, action: () => run(() => { dmlStore.setSource('csv'); picusTabsStore.openGenerate(); }) },
    ];

    const databaseItems = [
      { id: 'newquery', title: 'New query', icon: 'play', shortcut: 'Ctrl+T', when: true, action: () => run(() => picusTabsStore.openQuery()) },
      { id: 'runquery', title: 'Run the current query', icon: 'play', shortcut: 'Ctrl+Enter', when: tab?.kind === 'query', action: () => run(() => { if (tab && connectionsStore.active) queryStore.run(tab.id, connectionsStore.active.id); }) },
      { id: 'newconn', title: 'Add a connection…', icon: 'plus', shortcut: 'Ctrl+Shift+N', when: true, action: () => run(() => picusUiStore.openConnectionEditor(null)) },
      { id: 'cycleconn', title: 'Switch to the next connection', icon: 'database', shortcut: 'Ctrl+Shift+D', when: connectionsStore.connections.length > 1, action: () => run(() => connectionsStore.cycle(1)) },
      ...connectionsStore.connections.map((c) => ({
        id: `conn:${c.id}`,
        title: `Connect to ${c.name}`,
        subtitle: `${c.alias} · ${c.schema}@${c.host}`,
        icon: 'database',
        when: true,
        action: () => run(() => connectionsStore.setActive(c.id)),
      })),
      // Every schema object is reachable by name, whatever kind it is: the
      // palette is where "I know what it's called" turns into "it's open".
      ...schemaStore.relations.map((t) => ({
        id: `object:${t.name}`,
        title: `Open ${t.kind} ${t.name}`,
        icon: 'table',
        when: true,
        action: () => run(() => picusTabsStore.openObject(t.name, t.kind)),
      })),
      ...schemaStore.sequences.map((s) => ({
        id: `sequence:${s.name}`,
        title: `Open sequence ${s.name}`,
        icon: 'table',
        when: true,
        action: () => run(() => picusTabsStore.openObject(s.name, 'sequence')),
      })),
      ...schemaStore.triggers.map((t) => ({
        id: `trigger:${t.name}`,
        title: `Open trigger ${t.name}`,
        subtitle: `on ${t.table}`,
        icon: 'table',
        when: true,
        action: () => run(() => picusTabsStore.openObject(t.name, 'trigger')),
      })),
    ];

    const scriptItems = [
      ...picusProjectStore.allFiles.map((f) => ({
        id: `file:${f.path}`,
        title: `Open ${f.name}`,
        subtitle: f.path,
        icon: 'file',
        when: true,
        action: () => run(() => picusTabsStore.openFile(f.path, f.name, picusProjectStore.dialectOfFile(f.path))),
      })),
      { id: 'rescan', title: 'Re-scan the project', icon: 'refresh', when: true, action: () => run(() => toastStore.show('Project re-scanned.', 'success')) },
    ];

    const checkItems = [
      { id: 'check', title: 'Run the consistency check', icon: 'alert', shortcut: 'Ctrl+Shift+K', when: true, action: () => run(() => { picusUiStore.showBottom('consistency'); consistencyStore.run(); }) },
      { id: 'findings', title: 'Show the consistency report', icon: 'alert', when: true, action: () => run(() => picusUiStore.showBottom('consistency')) },
      { id: 'changes', title: 'Show pending changes', icon: 'wrench', when: true, action: () => run(() => picusUiStore.showBottom('changes')) },
      { id: 'inventory', title: 'Open the inventory', icon: 'layers', shortcut: 'Ctrl+4', when: true, action: () => run(() => picusTabsStore.openInventory()) },
    ];

    const viewItems = [
      { id: 'sidebar', title: 'Toggle the sidebar', icon: 'panelLeft', shortcut: 'Ctrl+B', when: true, action: () => run(() => picusUiStore.toggleSidebar()) },
      { id: 'bottom', title: 'Toggle the bottom panel', icon: 'panelBottom', shortcut: 'Ctrl+J', when: true, action: () => run(() => picusUiStore.toggleBottom()) },
      ...SECTIONS.map((s) => ({
        id: `sec:${s.id}`,
        title: `Show ${s.label}`,
        icon: 'folder',
        shortcut: s.shortcut,
        when: true,
        action: () => run(() => picusUiStore.showSection(s.id)),
      })),
    ];

    const appItems = [
      { id: 'settings', title: 'Settings…', icon: 'settings', shortcut: 'Ctrl+,', when: true, action: () => run(() => picusUiStore.openSettings()) },
      { id: 'shortcuts', title: 'Keyboard shortcuts…', icon: 'keyboard', shortcut: 'Shift+F1', when: true, action: () => run(() => picusUiStore.openShortcuts()) },
      { id: 'docs', title: 'Documentation', icon: 'docs', shortcut: 'F1', when: true, action: () => run(() => picusUiStore.toggleDocs()) },
      { id: 'about', title: 'About Picus', icon: 'command', when: true, action: () => run(() => picusUiStore.openAbout()) },
    ];

    type Raw = { id: string; title: string; subtitle?: string; icon: string; shortcut?: string; when: boolean; action: () => void };
    const pack = (items: Raw[]) =>
      items
        .filter((c) => c.when && (!q || c.title.toLowerCase().includes(q) || (c.subtitle ?? '').toLowerCase().includes(q)))
        .map((c) => ({ id: c.id, title: c.title, subtitle: c.subtitle, icon: c.icon, shortcut: c.shortcut, action: c.action }));

    const out: PaletteSection[] = [];
    const gen = pack(generateItems); if (gen.length) out.push({ id: 'generate', label: 'Generate', items: gen });
    const db = pack(databaseItems); if (db.length) out.push({ id: 'database', label: 'Database', items: db });
    const sc = pack(scriptItems); if (sc.length) out.push({ id: 'scripts', label: 'Scripts', items: sc });
    const ck = pack(checkItems); if (ck.length) out.push({ id: 'consistency', label: 'Consistency', items: ck });
    const vw = pack(viewItems); if (vw.length) out.push({ id: 'view', label: 'View', items: vw });
    const ap = pack(appItems); if (ap.length) out.push({ id: 'app', label: 'Application', items: ap });
    return out;
  });

  // ── Keyboard ────────────────────────────────────────────────────────────────
  function onKeyDown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();

    // Help toggles work even with a panel open (F1 closes the docs again).
    if (e.key === 'F1' && !e.shiftKey) { picusUiStore.toggleDocs(); e.preventDefault(); return; }
    if (e.key === 'F1' && e.shiftKey) { picusUiStore.openShortcuts(); e.preventDefault(); return; }

    // Behind a dialog, let it own the keyboard (its own Esc, Tab, Enter).
    if (picusUiStore.anyModalOpen || confirmWrite) {
      if (mod && key === 'k' && !picusUiStore.anyModalOpen) { picusUiStore.togglePalette(); e.preventDefault(); }
      return;
    }

    if (mod && key === 'k') { picusUiStore.togglePalette(); e.preventDefault(); return; }
    if (mod && key === ',') { picusUiStore.openSettings(); e.preventDefault(); return; }
    if (mod && key === 'b' && !e.shiftKey) { picusUiStore.toggleSidebar(); e.preventDefault(); return; }
    if (mod && key === 'j') { picusUiStore.toggleBottom(); e.preventDefault(); return; }

    // Sections — e.code so the digits survive non-US layouts.
    if (mod && !e.shiftKey && e.code === 'Digit1') { picusUiStore.selectSection('connections'); e.preventDefault(); return; }
    if (mod && !e.shiftKey && e.code === 'Digit2') { picusUiStore.selectSection('scripts'); e.preventDefault(); return; }
    if (mod && !e.shiftKey && e.code === 'Digit3') { picusUiStore.selectSection('generate'); e.preventDefault(); return; }
    if (mod && !e.shiftKey && e.code === 'Digit4') { picusUiStore.selectSection('inventory'); e.preventDefault(); return; }

    // Tabs.
    if (mod && key === 'tab') { picusTabsStore.cycle(e.shiftKey ? -1 : 1); e.preventDefault(); return; }
    if (mod && key === 'w' && !e.shiftKey) { if (tab) picusTabsStore.close(tab.id); e.preventDefault(); return; }
    if (mod && key === 't') { picusTabsStore.openQuery(); e.preventDefault(); return; }

    // Database.
    if (mod && key === 'enter') {
      if (tab?.kind === 'query' && connectionsStore.active) queryStore.run(tab.id, connectionsStore.active.id);
      e.preventDefault();
      return;
    }
    if (mod && e.shiftKey && key === 'c') { if (tab) queryStore.cancel(tab.id); e.preventDefault(); return; }
    if (mod && e.shiftKey && key === 'd') { connectionsStore.cycle(1); e.preventDefault(); return; }
    if (mod && e.shiftKey && key === 'n') { picusUiStore.openConnectionEditor(null); e.preventDefault(); return; }

    // Generation.
    if (mod && !e.shiftKey && key === 'g') { generate(); e.preventDefault(); return; }
    if (mod && e.shiftKey && key === 'w') { requestWrite(); e.preventDefault(); return; }
    if (e.altKey && e.code === 'Digit1') { dmlStore.setSource('form'); picusTabsStore.openGenerate(); e.preventDefault(); return; }
    if (e.altKey && e.code === 'Digit2') { dmlStore.setSource('paste'); picusTabsStore.openGenerate(); e.preventDefault(); return; }
    if (e.altKey && e.code === 'Digit3') { dmlStore.setSource('csv'); picusTabsStore.openGenerate(); e.preventDefault(); return; }
    if (e.altKey && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
      const list = dmlStore.enabledTargets;
      if (list.length) {
        const i = list.findIndex((t) => t.id === dmlStore.previewTargetId);
        const next = (i + (e.key === 'ArrowRight' ? 1 : -1) + list.length) % list.length;
        dmlStore.setPreviewTarget(list[next].id);
      }
      e.preventDefault();
      return;
    }

    // Consistency.
    if (mod && e.shiftKey && key === 'k') {
      picusUiStore.showBottom('consistency');
      consistencyStore.run();
      e.preventDefault();
      return;
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="shell">
  <PicusTitleBar />

  <div class="content-area">
    <WorkspaceShell>
      {#snippet leftRail()}
        <ActivityBar side="left" ariaLabel="Picus sections" topItems={railTop} bottomItems={railBottom} />
      {/snippet}

      {#snippet panels()}
        {#if picusUiStore.sidebarOpen}
          <PanelCard
            orientation="left"
            initialSize={sidebarWidth}
            minSize={200}
            maxSize={480}
            onResize={(px) => (sidebarWidth = px)}
          >
            {#if picusUiStore.sidebarSection === 'connections'}<ConnectionsPanel />
            {:else if picusUiStore.sidebarSection === 'scripts'}<ScriptsPanel />
            {:else if picusUiStore.sidebarSection === 'generate'}<GeneratePanel />
            {:else}<InventoryPanel />{/if}
          </PanelCard>
        {/if}

        <div class="main-col">
          <div class="card grow">
            <div class="doc">
              <PicusTabBar />
              <PicusToolbar onGenerate={generate} onWrite={requestWrite} />
              <div class="doc-body">
                {#if !tab}
                  <StateBlock tone="info" label="No document open. Ctrl+T opens a query, Ctrl+3 the generator." />
                {:else if tab.kind === 'generate'}
                  <GenerateView onWrite={requestWrite} />
                {:else if tab.kind === 'query'}
                  <QueryView {tab} />
                {:else if tab.kind === 'table'}
                  <TableView {tab} />
                {:else if tab.kind === 'file'}
                  <FileView {tab} />
                {:else}
                  <InventoryView />
                {/if}
              </div>
            </div>
          </div>

          {#if picusUiStore.bottomOpen}
            <PanelCard orientation="bottom" initialSize={240} minSize={120} maxSize={560}>
              <PicusBottomDock />
            </PanelCard>
          {/if}
        </div>
      {/snippet}
    </WorkspaceShell>
  </div>

  <PicusStatusBar>
    {#snippet footerExtra()}
      <FeedbackStatusButtons />
    {/snippet}
  </PicusStatusBar>
</div>

{#if picusUiStore.paletteOpen}
  <CommandPaletteShell
    onClose={() => picusUiStore.closePalette()}
    {iconResolver}
    sections={paletteSections}
    bind:query={paletteQuery}
    placeholder="Search a command, a table or a file…"
  />
{/if}

{#if confirmWrite}
  <ConfirmModal
    title="Write to the scripts"
    message={`${dmlStore.enabledTargets.length} file(s) will be rewritten.`}
    detail={writeDetail}
    variant="warning"
    confirmLabel="Write"
    onConfirm={applyWrite}
    onCancel={() => (confirmWrite = false)}
  />
{/if}

{#if picusUiStore.settingsOpen}
  <PicusSettingsModal onClose={() => picusUiStore.closeSettings()} />
{/if}

{#if picusUiStore.shortcutsOpen}
  <PicusShortcutsModal onClose={() => picusUiStore.closeShortcuts()} />
{/if}

{#if picusUiStore.aboutOpen}
  <PicusAboutModal onClose={() => picusUiStore.closeAbout()} />
{/if}

{#if picusUiStore.addDestinationOpen}
  <!-- Mounted on the shell, not on the generator view: the sidebar can open it
       while another tab is on screen. -->
  <AddDestinationModal onClose={() => picusUiStore.closeAddDestination()} />
{/if}

{#if picusUiStore.connectionEditorOpen}
  <PicusConnectionModal
    connectionId={picusUiStore.connectionEditorId}
    onClose={() => picusUiStore.closeConnectionEditor()}
  />
{/if}

{#if picusUiStore.docsOpen}
  <PicusDocsPanel onClose={() => picusUiStore.closeDocs()} />
{/if}

<Tooltip />

<!-- Toasts / notifications / progress addressed to this window. -->
<FeedbackHost id="picus" />

<style>
  .shell {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-base);
    overflow: hidden;
  }

  /* A few px of bg-elevated under the titlebar so the floating panel cards read
     as detached from the chrome (IntelliJ New UI). WorkspaceShell has no top
     padding by design; the window adds it. */
  .content-area {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding-top: 5px;
    background: var(--bg-elevated);
  }

  .main-col {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    gap: 4px;
  }

  .card {
    display: flex;
    flex-shrink: 0;
    min-width: 0;
    min-height: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  .card.grow { flex: 1; }
  .card.grow > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  .doc { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  /* The document area only ever FILLS — it never scrolls itself. Scrolling
     belongs to each view: the fill views (query, table, file) have their own
     inner scrollers, and the document-flow views (generate, inventory) scroll
     their own body. A scrolling flex container here would instead squash every
     card down to the viewport height, which is exactly what it did. */
  .doc-body { flex: 1; min-height: 0; min-width: 0; display: flex; overflow: hidden; }
  .doc-body > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
