<script lang="ts">
  /**
   * FileExplorerSettings — the in-explorer settings page (Windows-Terminal
   * style: it fills the explorer body in place of the file listing). Opened by
   * typing `arbor://settings` in the address bar, the sidebar Settings item, or
   * Ctrl+,. Edits ExplorerConfig directly through `explorerStore` (persisted to
   * `~/.config/arbor/config.toml`); the same host-level switches are also in the
   * main SettingsPanel → File Explorer section. Reset actions are owned by the
   * parent explorer (it holds the ephemeral localStorage state) and passed in.
   */
  import { ArrowLeft, GitCompare, LayoutGrid, Keyboard, RotateCcw, PanelLeft, Eye, EyeOff, ChevronUp, ChevronDown, Link2 } from 'lucide-svelte';
  import { explorerStore, mergeSidebarSections, EXPLORER_SECTIONS, MAX_RECENTS_MIN, MAX_RECENTS_MAX } from '$lib/stores/explorer.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { ExplorerView, ExplorerSort, ExplorerStartup } from '$lib/types/config';
  import SectionHeader from './ui/SectionHeader.svelte';
  import FormRow from './ui/FormRow.svelte';
  import Toggle from './ui/Toggle.svelte';
  import RadioGroup from './ui/RadioGroup.svelte';
  import Button from './ui/Button.svelte';
  import NumberStepper from './ui/NumberStepper.svelte';
  import GlobalShortcutCapture from './internal/GlobalShortcutCapture.svelte';

  let {
    onExit,
    onResetViewMemory,
    onResetRecents,
    onResetLayout,
    viewMemoryCount = 0,
    recentsCount = 0,
  }: {
    onExit?: () => void;
    onResetViewMemory?: () => void;
    onResetRecents?: () => void;
    onResetLayout?: () => void;
    viewMemoryCount?: number;
    recentsCount?: number;
  } = $props();

  // Plain toggles/selects — setters are no-ops when unchanged, so the initial
  // effect pass (after the store loads) doesn't write back.
  let gitAwareness    = $state(explorerStore.gitAwareness);
  let defaultView     = $state<string>(explorerStore.defaultView);
  let showHidden      = $state(explorerStore.showHidden);
  let recursiveSearch = $state(explorerStore.recursiveSearch);
  let defaultSort     = $state<string>(explorerStore.defaultSort);
  let sortDir         = $state(explorerStore.sortAscending ? 'asc' : 'desc');
  let startup         = $state<string>(explorerStore.startup);
  let alwaysNewWindow = $state(explorerStore.alwaysNewWindow);
  let openExternalLinks = $state(explorerStore.openExternalLinks);
  let openWebLinks      = $state(explorerStore.openWebLinks);

  $effect(() => { explorerStore.setGitAwareness(gitAwareness); });
  $effect(() => { explorerStore.setDefaultView(defaultView as ExplorerView); });
  $effect(() => { explorerStore.setShowHidden(showHidden); });
  $effect(() => { explorerStore.setRecursiveSearch(recursiveSearch); });
  $effect(() => { explorerStore.setDefaultSort(defaultSort as ExplorerSort); });
  $effect(() => { explorerStore.setSortAscending(sortDir === 'asc'); });
  $effect(() => { explorerStore.setStartup(startup as ExplorerStartup); });
  $effect(() => { explorerStore.setAlwaysNewWindow(alwaysNewWindow); });
  $effect(() => { explorerStore.setOpenExternalLinks(openExternalLinks); });
  $effect(() => { explorerStore.setOpenWebLinks(openWebLinks); });

  // Global shortcut goes through async setters (the backend register can fail
  // on a taken combo); read from the store and toast + revert on error.
  async function toggleShortcut(on: boolean) {
    try { await explorerStore.setGlobalShortcut(on); }
    catch (e) { uiStore.showToast(`Shortcut: ${e}`, 'error'); }
  }
  async function rebind(accel: string) {
    try { await explorerStore.setGlobalShortcutAccel(accel); }
    catch (e) { uiStore.showToast(`Shortcut: ${e}`, 'error'); }
  }

  const viewOptions: { value: string; label: string; description: string }[] = [
    { value: 'details', label: 'Details', description: 'List with columns' },
    { value: 'medium',  label: 'Medium',  description: 'Medium icons' },
    { value: 'large',   label: 'Large',   description: 'Large icons + thumbnails' },
    { value: 'xlarge',  label: 'X-Large', description: 'Extra large previews' },
  ];
  const sortOptions: { value: string; label: string }[] = [
    { value: 'name',     label: 'Name' },
    { value: 'modified', label: 'Date modified' },
    { value: 'size',     label: 'Size' },
  ];
  const dirOptions: { value: string; label: string }[] = [
    { value: 'asc',  label: 'Ascending' },
    { value: 'desc', label: 'Descending' },
  ];
  const startupOptions: { value: string; label: string; description: string }[] = [
    { value: 'overview', label: 'Overview', description: 'The dashboard' },
    { value: 'last',     label: 'Last folder', description: 'Re-open the most recent folder' },
  ];

  // ── Sidebar sections (order + visibility) ──────────────────────────────────
  // Derived straight from the store so a right-click "hide" in the sidebar
  // (which the page sits beside) stays in sync. Mutations persist immediately.
  const sections = $derived(mergeSidebarSections(explorerStore.sidebarSections));
  const labelFor = (id: string) => EXPLORER_SECTIONS.find(s => s.id === id)?.label ?? id;
  function moveSection(i: number, dir: -1 | 1) {
    const j = i + dir;
    if (j < 0 || j >= sections.length) return;
    const a = sections.map(s => ({ ...s }));
    [a[i], a[j]] = [a[j], a[i]];
    explorerStore.setSidebarSections(a);
  }
  function toggleSectionVis(i: number) {
    const a = sections.map(s => ({ ...s }));
    a[i] = { ...a[i], visible: !a[i].visible };
    explorerStore.setSidebarSections(a);
  }
</script>

<div class="fxs">
  <div class="fxs-inner">
    <div class="fxs-top">
      <SectionHeader title="Explorer Settings" description="Preferences for the built-in file explorer. Stored in your Arbor config, shared across both the in-app explorer and the standalone window." />
      {#if onExit}
        <Button variant="ghost" size="sm" onclick={onExit}>
          {#snippet iconStart()}<ArrowLeft size={14} />{/snippet}
          Back to files
        </Button>
      {/if}
    </div>

    <!-- ── Git ── -->
    <h3 class="fxs-group"><GitCompare size={13} /> Git</h3>
    <div class="fxs-card">
      <FormRow
        label="Git awareness"
        description="Show status overlays, repo-root markers, the Changes panel and branch switching while browsing. Off by default — when off, the explorer issues no git checks, so plain browsing stays fast.">
        <Toggle bind:checked={gitAwareness} />
      </FormRow>
    </div>

    <!-- ── Browsing ── -->
    <h3 class="fxs-group"><LayoutGrid size={13} /> Browsing</h3>
    <div class="fxs-card">
      <FormRow label="Default view" description="Layout applied to folders you haven't customised yet.">
        <RadioGroup bind:value={defaultView} options={viewOptions} appearance="segment" size="sm" />
      </FormRow>
      <FormRow label="Default sort" description="Column new folders are sorted by.">
        <RadioGroup bind:value={defaultSort} options={sortOptions} appearance="segment" size="sm" />
      </FormRow>
      <FormRow label="Sort direction" description="Order for the default sort.">
        <RadioGroup bind:value={sortDir} options={dirOptions} appearance="segment" size="sm" />
      </FormRow>
      <FormRow label="On open" description="What a freshly-opened explorer shows.">
        <RadioGroup bind:value={startup} options={startupOptions} appearance="segment" size="sm" />
      </FormRow>
      <FormRow label="Show hidden files" description="Reveal dot-prefixed entries by default.">
        <Toggle bind:checked={showHidden} />
      </FormRow>
      <FormRow label="Recursive search" description="When searching, match files in all subfolders instead of only the current one.">
        <Toggle bind:checked={recursiveSearch} />
      </FormRow>
      <FormRow label="Recent folders" description="Maximum number of recently visited folders kept in the sidebar.">
        <NumberStepper value={explorerStore.maxRecents} min={MAX_RECENTS_MIN} max={MAX_RECENTS_MAX} onchange={(v) => explorerStore.setMaxRecents(v)} />
      </FormRow>
    </div>

    <!-- ── Sidebar sections ── -->
    <h3 class="fxs-group"><PanelLeft size={13} /> Sidebar sections</h3>
    <div class="fxs-card">
      <div class="fxs-sec-note">Reorder and show / hide the sidebar sections. You can also right-click a section header in the sidebar to hide it.</div>
      {#each sections as s, i (s.id)}
        <div class="fxs-sec-row" class:hidden={!s.visible}>
          <button class="fxs-sec-vis" onclick={() => toggleSectionVis(i)} use:tooltip={s.visible ? 'Hide section' : 'Show section'} aria-label={s.visible ? 'Hide section' : 'Show section'}>
            {#if s.visible}<Eye size={14} />{:else}<EyeOff size={14} />{/if}
          </button>
          <span class="fxs-sec-label">{labelFor(s.id)}</span>
          <div class="fxs-sec-move">
            <button onclick={() => moveSection(i, -1)} disabled={i === 0} use:tooltip={'Move up'} aria-label="Move up"><ChevronUp size={14} /></button>
            <button onclick={() => moveSection(i, 1)} disabled={i === sections.length - 1} use:tooltip={'Move down'} aria-label="Move down"><ChevronDown size={14} /></button>
          </div>
        </div>
      {/each}
    </div>

    <!-- ── Window ── -->
    <h3 class="fxs-group"><Keyboard size={13} /> Window</h3>
    <div class="fxs-card">
      <FormRow
        label="Global shortcut"
        description="Register a system-wide hotkey that opens the dedicated explorer window even when Arbor isn't focused. Click the chord to rebind it.">
        <GlobalShortcutCapture accel={explorerStore.globalShortcutAccel} disabled={!explorerStore.globalShortcut} onChange={rebind} />
        <Toggle checked={explorerStore.globalShortcut} onchange={toggleShortcut} />
      </FormRow>
      <FormRow
        label="Always open a new window"
        description="When on, the shortcut and “Open File Explorer in New Window” always spawn a fresh window. When off (default), a single explorer window is reused and re-summoning just focuses it.">
        <Toggle bind:checked={alwaysNewWindow} />
      </FormRow>
    </div>

    <!-- ── Address bar ── -->
    <h3 class="fxs-group"><Link2 size={13} /> Address bar</h3>
    <div class="fxs-card">
      <FormRow
        label="Open external links"
        description="Let the address bar open generic external links (custom schemes like vscode://, mailto:, slack://) in the associated app. Each open asks for confirmation unless you choose to remember that scheme. arbor:// deep links are always handled.">
        <Toggle bind:checked={openExternalLinks} />
      </FormRow>
      <FormRow
        label="Open web links (http / https)"
        description="Also allow plain web URLs typed in the address bar to open in your default browser. Requires “Open external links”.">
        <Toggle bind:checked={openWebLinks} disabled={!openExternalLinks} />
      </FormRow>
    </div>

    <!-- ── Reset ── -->
    <h3 class="fxs-group"><RotateCcw size={13} /> Reset</h3>
    <div class="fxs-card">
      <FormRow label="Per-folder view memory" description="Forget the view mode (details / icons) remembered for individual folders. Your default view is kept.">
        <Button variant="secondary" size="sm" disabled={!onResetViewMemory || viewMemoryCount === 0} onclick={() => onResetViewMemory?.()}>
          Clear{viewMemoryCount ? ` (${viewMemoryCount})` : ''}
        </Button>
      </FormRow>
      <FormRow label="Recent folders" description="Clear the list of recently visited folders.">
        <Button variant="secondary" size="sm" disabled={!onResetRecents || recentsCount === 0} onclick={() => onResetRecents?.()}>
          Clear{recentsCount ? ` (${recentsCount})` : ''}
        </Button>
      </FormRow>
      <FormRow label="Sidebar & panel layout" description="Reset collapsed sidebar sections, expanded workspace groups, the sidebar collapse state and the right-panel width.">
        <Button variant="secondary" size="sm" disabled={!onResetLayout} onclick={() => onResetLayout?.()}>
          Reset
        </Button>
      </FormRow>
      <FormRow label="Remembered external links" description="Forget the link schemes you chose to always allow — they'll prompt again next time.">
        <Button variant="secondary" size="sm" disabled={explorerStore.rememberedSchemes.length === 0} onclick={() => explorerStore.forgetRememberedSchemes()}>
          Clear{explorerStore.rememberedSchemes.length ? ` (${explorerStore.rememberedSchemes.length})` : ''}
        </Button>
      </FormRow>
    </div>
  </div>
</div>

<style>
  .fxs {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }
  .fxs-inner {
    max-width: 720px;
    margin: 0 auto;
    padding: 22px 24px 32px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .fxs-top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 6px;
  }
  .fxs-group {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 16px 0 6px;
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }
  .fxs-group > :global(svg) { color: var(--accent); }
  .fxs-card {
    border: 1px solid var(--border-subtle, var(--border));
    border-radius: var(--radius-lg);
    background: var(--bg-elevated);
    overflow: hidden;
  }

  /* Sidebar-sections manager */
  .fxs-sec-note {
    padding: 10px 14px;
    font-size: 0.77rem;
    color: var(--text-muted);
    line-height: 1.5;
    border-bottom: 1px solid var(--border-subtle);
  }
  .fxs-sec-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .fxs-sec-row:last-child { border-bottom: none; }
  .fxs-sec-row.hidden .fxs-sec-label { color: var(--text-disabled); text-decoration: line-through; }
  .fxs-sec-label { flex: 1; min-width: 0; font-size: 0.82rem; font-weight: 600; color: var(--text-primary); }
  .fxs-sec-vis, .fxs-sec-move button {
    display: inline-flex; align-items: center; justify-content: center;
    width: 26px; height: 24px;
    background: transparent; border: 1px solid transparent; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .fxs-sec-vis:hover, .fxs-sec-move button:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .fxs-sec-move { display: inline-flex; gap: 2px; flex-shrink: 0; }
  .fxs-sec-move button:disabled { opacity: 0.3; cursor: default; }
</style>
