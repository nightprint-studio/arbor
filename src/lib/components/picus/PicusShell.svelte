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
  import { Database, FolderTree, FormInput, Layers, TriangleAlert } from 'lucide-svelte';
  import WorkspaceShell from '$lib/components/shared/ui/WorkspaceShell.svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';
  import ActivityBar, { type ActivityRailItem } from '$lib/components/shared/ui/ActivityBar.svelte';
  import CommandPaletteShell, { type PaletteSection } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
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
  import PicusConnectionDetailsModal from './PicusConnectionDetailsModal.svelte';
  import PicusDocsPanel from './PicusDocsPanel.svelte';
  import AddDestinationModal from './generate/AddDestinationModal.svelte';
  import ClassifyFolderModal from './ClassifyFolderModal.svelte';
  import { aliasOfferDetail } from './folder-classify';
  import {
    PICUS_SECTIONS,
    buildPicusPalette,
    picusPaletteIcon,
  } from './picus-palette';
  import { FOLDER_ROLE_LABELS, engineLabel } from '$lib/types/picus';

  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusUiStore, type SidebarSection } from '$lib/stores/picus/ui.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
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
  const SECTION_ICONS: Record<SidebarSection, IconComponent> = {
    connections: Database as unknown as IconComponent,
    scripts: FolderTree as unknown as IconComponent,
    generate: FormInput as unknown as IconComponent,
    inventory: Layers as unknown as IconComponent,
  };

  const railTop = $derived<ActivityRailItem[]>(
    PICUS_SECTIONS.map((s) => ({
      id: s.id,
      icon: SECTION_ICONS[s.id],
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
      tooltip: consistencyStore.running
        ? 'Consistency — checking the repository…'
        : consistencyStore.blockingCount
          ? `Consistency — ${consistencyStore.blockingCount} blocking`
          : consistencyStore.reviewCount
            ? `Consistency — ${consistencyStore.reviewCount} to check`
            : consistencyStore.hasRun
              ? 'Consistency — no problem found'
              : 'Consistency — not checked yet',
      shortcut: 'Ctrl+J',
      active: picusUiStore.bottomOpen && picusUiStore.bottomTab === 'consistency',
      // Blockers are red, "worth checking" amber, clean shows nothing. A run in
      // flight keeps the previous dot rather than flickering to none — the status
      // bar is where "it is working" is stated.
      dot: consistencyStore.blockingCount
        ? 'error'
        : consistencyStore.reviewCount
          ? 'warning'
          : false,
      onclick: () => picusUiStore.showBottom('consistency'),
    },
  ]);

  // ── Write flow ──────────────────────────────────────────────────────────────
  //
  // Two steps, always in this order: compute the exact bytes, show them, and only
  // then write — handing the digests back so the backend can refuse if any of
  // those files moved since they were reviewed. "Skip the confirmation" is a
  // setting about the DIALOG, never about the preview: nothing is written that
  // was not first computed and checked against disk.

  /** The preview is being built because the user asked to write, not to browse. */
  let preparingWrite = $state(false);

  async function requestWrite() {
    if (!dmlStore.generated || dmlStore.applied || preparingWrite) return;
    if (!picusProjectStore.attached) {
      toastStore.show('This connection has no script repository attached.', 'warning');
      return;
    }
    preparingWrite = true;
    picusUiStore.showBottom('changes');
    await dmlStore.ensurePreview();
    preparingWrite = false;

    if (dmlStore.previewError) {
      toastStore.show(`Nothing was written — ${dmlStore.previewError}`, 'error');
      return;
    }
    if (!dmlStore.changedFiles.length) {
      toastStore.show('Nothing to write — the destinations already contain this change.', 'info');
      return;
    }
    if (!picusSettingsStore.confirmBeforeWrite) { void applyWrite(); return; }
    confirmWrite = true;
  }

  async function applyWrite() {
    confirmWrite = false;
    const res = await dmlStore.apply();
    if (!res) {
      // The backend's refusal names the file that changed underneath the preview.
      // That sentence IS the useful part — it is passed through untouched, and the
      // Changes dock keeps it on screen next to the button that rebuilds the patch.
      picusUiStore.showBottom('changes');
      toastStore.show(dmlStore.applyError ?? 'The write was refused.', 'error');
      return;
    }
    // The backend answers with the paths, not with counts — so the message can be
    // specific about what happened rather than "3 files written".
    const parts = [`${res.written.length} file(s) written`];
    if (res.created.length) parts.push(`${res.created.length} created`);
    if (res.unchanged.length) parts.push(`${res.unchanged.length} already up to date`);
    toastStore.show(`${parts.join(', ')} — encoding and line endings preserved.`, 'success');
    // What is on disk changed, so the tree and the verdict both describe the past.
    void picusProjectStore.refresh();
  }

  /**
   * Bind a folder of scripts to a connection.
   *
   * The binding is saved with the connection, so the repository is back the next
   * time that database is opened — including in another window. The window's own
   * effect notices the new root and reads it; nothing here has to.
   */
  async function attachScriptRoot(path: string) {
    const id = picusUiStore.scriptRootPickerId;
    picusUiStore.closeScriptRootPicker();
    if (!id || !path) return;
    try {
      await connectionsStore.setScriptRoot(id, path);
      // Attaching is always "show me these scripts" — including from the palette,
      // where the connection picked may not be the one currently selected. The
      // window reads the repository off the ACTIVE connection, so making it active
      // is what turns the choice into something on screen.
      connectionsStore.setActive(id);
      picusUiStore.showSection('scripts');
    } catch (e) {
      toastStore.show(`The folder could not be attached — ${e}`, 'error');
    }
  }

  /** Step to a finding and open it where it lives — the F8 pair. */
  function stepFinding(delta: number) {
    const finding = delta > 0 ? consistencyStore.next() : consistencyStore.previous();
    if (!finding) {
      toastStore.show('No finding to step to.', 'info');
      return;
    }
    picusUiStore.showBottom('consistency');
    const file = picusProjectStore.fileByPath(finding.file);
    if (!file) return;
    picusTabsStore.openFile(
      file.path,
      file.name,
      picusProjectStore.dialectOfFile(file.path),
      finding.line,
    );
  }

  function runActiveQuery() {
    const conn = picusTabsStore.activeConnection;
    if (tab?.kind === 'query' && conn) void queryStore.run(tab.id, conn.id);
  }

  function generate() {
    // Reached from the keyboard, where the disabled button that would have said
    // the same thing is not in the way — so the toast has to carry the reason.
    const blocked = dmlStore.generateBlockedReason;
    if (blocked) {
      toastStore.show(`Nothing to generate — ${blocked.replace(/\.$/, '').toLowerCase()}.`, 'warning');
      return;
    }
    picusTabsStore.openGenerate();
    dmlStore.markGenerated();
  }

  // ── "…and every folder named POS" ───────────────────────────────────────────
  //
  // Raised by `folder-classify.ts` right after a folder is classified, and owned
  // here for the same reason the delete confirmation is: classifying is reachable
  // from the tree row, the dialog and the palette, and the follow-up question must
  // not depend on which of the three is on screen.
  //
  // It is deliberately a **second** dialog rather than a checkbox on the first.
  // Declaring what one folder is and declaring what a name means across the whole
  // repository are different decisions with different blast radii — one folder
  // versus eleven and counting — and a user who reached the second by pressing a
  // button that named the first would be right to feel misled.
  const aliasOffer = $derived(picusUiStore.aliasOffer);

  const aliasOfferMessage = $derived.by(() => {
    const offer = aliasOffer;
    if (!offer) return '';
    const said = [
      offer.engine ? engineLabel(offer.engine) : null,
      offer.role ? FOLDER_ROLE_LABELS[offer.role] : null,
    ].filter(Boolean).join(' · ');
    const others = offer.paths.length - 1;
    return (
      `${offer.origin} is ${said}. ` +
      (others === 1
        ? `One other folder is called ${offer.name} — should it mean the same thing?`
        : `${others} other folders are called ${offer.name} — should they all mean the same thing?`)
    );
  });

  async function acceptAliasOffer() {
    const offer = aliasOffer;
    if (!offer) return;
    const message = await picusProjectStore.setAlias(offer.name, offer.engine, offer.role);
    picusUiStore.closeFolderAlias();
    if (message) {
      toastStore.show(`${offer.name} could not be declared — ${message}`, 'error');
      return;
    }
    toastStore.show(
      `Every folder named ${offer.name} is now classified — ${offer.paths.length} of them, ` +
        'and any added later.',
      'success',
    );
  }

  // ── Deleting a connection ───────────────────────────────────────────────────
  //
  // Owned by the shell rather than by the connections panel: deleting is offered
  // from the sidebar, from the details dialog and from the palette, and a
  // destructive confirmation must not depend on which of the three is on screen.
  const pendingDelete = $derived(
    picusUiStore.connectionDeleteId
      ? connectionsStore.specById(picusUiStore.connectionDeleteId)
      : null,
  );
  let deleting = $state(false);

  /**
   * Say what goes with the connection — before the user agrees, not after.
   *
   * Deleting one is not only forgetting a row: the open session is closed and the
   * password Arbor keeps for it is removed from the keychain. Neither comes back,
   * and a user who expected "remove it from the list" would find out by having to
   * type a password they may no longer have.
   */
  const deleteDetail = $derived.by(() => {
    const c = pendingDelete;
    if (!c) return '';
    const lines = [
      c.state === 'disconnected'
        ? 'It is not open, so nothing in flight is interrupted.'
        : 'Its open session is closed first — anything still running on it is cut off.',
      c.hasSecret
        ? "The password saved for it in Arbor's keychain is deleted with it. Configuring this connection again later means typing that password again."
        : "No password is stored for it, so there is nothing to remove from Arbor's keychain.",
      'Scripts on disk, and anything already generated, are untouched.',
    ];
    return lines.join('\n\n');
  });

  async function confirmDelete() {
    const c = pendingDelete;
    if (!c || deleting) return;
    deleting = true;
    try {
      await connectionsStore.remove(c.id);
      picusUiStore.cancelConnectionDelete();
      toastStore.show(`${c.name} deleted.`, 'success');
    } catch (e) {
      toastStore.show(`${c.name} could not be deleted — ${e}`, 'error');
    } finally {
      deleting = false;
    }
  }

  /**
   * The confirmation names exactly the files the PREVIEW says would change — not
   * the enabled destinations, which is a different list the moment one of them
   * already contains the change.
   */
  const writeDetail = $derived(
    dmlStore.changedFiles
      .map((f) => `${f.path}${f.createsFile ? '  (new file)' : ''}`)
      .join('\n') +
      '\n\nEncoding and line endings stay as they are.' +
      '\nThe write is refused if any of these files changed since the diff above was computed.' +
      (picusSettingsStore.backupBeforeWrite
        ? '\nOriginals are copied to .arbor/backup first; if any file fails, all of them are rolled back.'
        : '\nBackups are disabled: a failed write cannot be rolled back.'),
  );

  // ── Command palette ─────────────────────────────────────────────────────────
  //
  // The catalogue itself lives in `picus-palette.ts`: it grows with every verb the
  // product gains, and a window shell that doubles in length each release stops
  // reading as a layout. What stays here is what the shell owns — closing the
  // palette before acting, and the actions the entries borrow.
  function run(fn: () => void) { picusUiStore.closePalette(); queueMicrotask(fn); }

  const paletteSections = $derived<PaletteSection[]>(
    buildPicusPalette(paletteQuery, {
      run,
      generate,
      requestWrite: () => void requestWrite(),
      runQuery: runActiveQuery,
      stepFinding,
    }),
  );

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

    // Database. The TAB's connection, never the sidebar's selection: a query tab
    // can be bound to another database than the one highlighted in the sidebar, and
    // the bar above the editor names the former. Running a statement — or
    // cancelling one — against a connection the user is not looking at is the worst
    // kind of wrong, so the keyboard resolves it exactly as the buttons do.
    if (mod && key === 'enter') {
      runActiveQuery();
      e.preventDefault();
      return;
    }
    if (mod && e.shiftKey && key === 'c') {
      const conn = picusTabsStore.activeConnection;
      if (tab && conn) void queryStore.cancel(tab.id, conn.id);
      e.preventDefault();
      return;
    }
    if (mod && e.shiftKey && key === 'd') { connectionsStore.cycle(1); e.preventDefault(); return; }
    if (mod && e.shiftKey && key === 'n') { picusUiStore.openConnectionEditor(null); e.preventDefault(); return; }
    if (e.key === 'F4' && !mod && !e.shiftKey && !e.altKey) {
      // Nothing selected means nothing to edit — silently, because the palette
      // and the sidebar both already say there is no connection.
      if (connectionsStore.activeId) picusUiStore.openConnectionEditor(connectionsStore.activeId);
      e.preventDefault();
      return;
    }

    // Scripts on disk. F5 is the universal "re-read", and Picus has a genuine use
    // for it: files change under the tool constantly (a colleague's pull, an
    // external editor), and the tree is a snapshot until asked otherwise.
    if (e.key === 'F5' && !mod && !e.shiftKey && !e.altKey) {
      if (picusProjectStore.attached) void picusProjectStore.refresh();
      e.preventDefault();
      return;
    }
    // Saying what a folder is — the step that makes a repository work at all when
    // its engine sits several levels down and nothing in the name gives it away.
    if (mod && e.shiftKey && key === 'f') {
      if (picusProjectStore.folderCount) picusUiStore.openFolderClassify();
      else toastStore.show('No repository is attached — there is no folder to classify.', 'warning');
      e.preventDefault();
      return;
    }

    // Generation.
    if (mod && !e.shiftKey && key === 'g') { generate(); e.preventDefault(); return; }
    if (mod && e.shiftKey && key === 'w') { void requestWrite(); e.preventDefault(); return; }
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
      void picusProjectStore.analyze();
      e.preventDefault();
      return;
    }
    // F8 / Shift+F8 walk the report and open each finding where it lives — the
    // pair the shortcuts reference has always listed, now that findings have a
    // file and a line to go to.
    if (e.key === 'F8' && !mod && !e.altKey) {
      stepFinding(e.shiftKey ? -1 : 1);
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
    iconResolver={picusPaletteIcon}
    sections={paletteSections}
    bind:query={paletteQuery}
    placeholder="Search a command, a table or a file…"
  />
{/if}

{#if confirmWrite}
  <!-- The counts and the file list come from the PREVIEW, which is already on
       screen in the Changes dock: the dialog confirms what was reviewed, it does
       not describe something else. -->
  <ConfirmModal
    title="Write to the scripts"
    message={`${dmlStore.changedFiles.length} file(s) will be written, exactly as shown in Changes.`}
    detail={writeDetail}
    variant="warning"
    confirmLabel="Write"
    busy={dmlStore.applying}
    onConfirm={() => void applyWrite()}
    onCancel={() => (confirmWrite = false)}
  />
{/if}

{#if picusUiStore.settingsOpen}
  <PicusSettingsModal
    initialSection={picusUiStore.settingsSection}
    onClose={() => picusUiStore.closeSettings()}
  />
{/if}

{#if aliasOffer}
  <!-- The second decision, and visibly a second one: the folder the user
       classified is already saved, and this asks whether the same answer should
       hold for every folder of that name. Cancelling costs them nothing they
       just did — which is the property that makes it safe to offer at all. -->
  <ConfirmModal
    title={`Every folder named ${aliasOffer.name}`}
    message={aliasOfferMessage}
    detail={aliasOfferDetail(aliasOffer.paths, aliasOffer.origin, picusProjectStore.configPath)}
    variant="info"
    confirmLabel={`Apply to all ${aliasOffer.paths.length}`}
    cancelLabel="Just this folder"
    busy={picusProjectStore.classifying}
    onConfirm={() => void acceptAliasOffer()}
    onCancel={() => picusUiStore.declineFolderAlias()}
  />
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

{#if picusUiStore.folderClassifyPath !== null}
  <!-- Saying what a folder is. Mounted on the shell because it is opened from the
       tree row, from the palette and from the destination picker alike. -->
  <ClassifyFolderModal
    path={picusUiStore.folderClassifyPath}
    onClose={() => picusUiStore.closeFolderClassify()}
  />
{/if}

{#if picusUiStore.scriptRootPickerId}
  <!-- Attaching a repository to a connection. Arbor's own folder picker, never a
       native dialog and never an <input type="file">; the shell owns it because
       the same action is offered from the scripts panel, the connection list and
       the palette. -->
  <FileExplorerModal
    mode="folder"
    title="Choose the folder of SQL scripts"
    initialPath={connectionsStore.scriptRootFor(picusUiStore.scriptRootPickerId) || undefined}
    onConfirm={(path) => void attachScriptRoot(path)}
    onCancel={() => picusUiStore.closeScriptRootPicker()}
    onClose={() => picusUiStore.closeScriptRootPicker()}
  />
{/if}

{#if picusUiStore.connectionEditorOpen}
  <PicusConnectionModal
    connectionId={picusUiStore.connectionEditorId}
    onClose={() => picusUiStore.closeConnectionEditor()}
  />
{/if}

{#if picusUiStore.connectionDetailsId}
  <PicusConnectionDetailsModal
    connectionId={picusUiStore.connectionDetailsId}
    onClose={() => picusUiStore.closeConnectionDetails()}
  />
{/if}

{#if pendingDelete}
  <ConfirmModal
    title="Delete this connection"
    message={`“${pendingDelete.name}” is removed from Picus.`}
    detail={deleteDetail}
    variant="danger"
    confirmLabel="Delete"
    busy={deleting}
    onConfirm={() => void confirmDelete()}
    onCancel={() => picusUiStore.cancelConnectionDelete()}
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
