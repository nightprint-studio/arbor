<script lang="ts">
  /**
   * MarketplaceModal — browse plugins + themes from the `arbor-registry` repo
   * and from user-added custom git URLs.
   *
   * Phase 1 wiring: catalog data flows through Tauri commands (`marketplace_*`)
   * backed by an in-memory seeded registry. The fetcher, installer and
   * permission-confirm dialog land in later phases (see project plan).
   */
  import { onMount, untrack } from 'svelte';
  import { Package, Palette, Search, Filter as FilterIcon, Globe, Shield, Tag, X, Plus, Upload, ExternalLink, Check, CheckCircle2, FolderGit2, GitBranch, ChevronRight, ChevronDown, Trash2, Eye, Pin, Store, RefreshCw, Link as LinkIcon, FlaskConical } from 'lucide-svelte';
  import Modal           from '$lib/components/shared/Modal.svelte';
  import ModalHeader     from '$lib/components/shared/ModalHeader.svelte';
  import Tabs, { type TabItem }       from '$lib/components/shared/ui/Tabs.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Monogram        from '$lib/components/shared/ui/Monogram.svelte';
  import Toggle          from '$lib/components/shared/ui/Toggle.svelte';
  import Button          from '$lib/components/shared/ui/Button.svelte';
  import Alert           from '$lib/components/shared/ui/Alert.svelte';
  import Badge           from '$lib/components/shared/ui/Badge.svelte';
  import Input           from '$lib/components/shared/ui/Input.svelte';
  import Select          from '$lib/components/shared/ui/Select.svelte';
  import Spinner         from '$lib/components/shared/ui/Spinner.svelte';
  import ModalFooter     from '$lib/components/shared/ModalFooter.svelte';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import MarketplaceInstallConfirm from './MarketplaceInstallConfirm.svelte';
  import { tooltip }     from '$lib/actions/tooltip';
  import { uiStore }     from '$lib/stores/ui.svelte';
  import { importPluginZipFromPath, reloadPlugins } from '$lib/ipc/plugin';
  import {
    listInstalled, fetchRegistry, refreshRegistry,
    installPlugin    as ipcInstallPlugin,
    uninstallPlugin  as ipcUninstallPlugin,
    setPluginEnabled as ipcSetPluginEnabled,
    installTheme     as ipcInstallTheme,
    uninstallTheme   as ipcUninstallTheme,
    addCustomSource    as ipcAddCustomSource,
    removeCustomSource as ipcRemoveCustomSource,
    getMarketplaceRefreshHours,
    setMarketplaceRefreshHours,
  } from '$lib/ipc/marketplace';
  import {
    pluginCategories, pluginTags, themeTags,
  } from '$lib/marketplace/catalog-helpers';
  import type {
    MarketplacePlugin, MarketplaceTheme, MarketplaceTab,
    MarketplaceSource, MarketplaceFilter,
  } from '$lib/types/marketplace';
  import { emptyFilter } from '$lib/types/marketplace';

  let {
    onClose,
    initialTab = 'plugins',
  }: {
    onClose:     () => void;
    initialTab?: MarketplaceTab;
  } = $props();

  // ── State ────────────────────────────────────────────────────────────────
  // NOTE: variable name avoids `tab` deliberately. With `tab` we saw a
  // bizarre reactivity glitch where mutating it from inside an event
  // handler (the Tabs `onSelect` callback) failed to re-render the
  // `{#if tab === 'plugins'}` block until something else forced a tick.
  // Renaming to `activeTab` made the issue go away — almost certainly a
  // shadowing collision with some implicit Svelte / HTML identifier the
  // compiler was tripping over.
  let activeTab = $state<MarketplaceTab>(untrack(() => initialTab));
  let filter   = $state<MarketplaceFilter>(emptyFilter());
  // The local arrays start empty: `listInstalled` (fast, synchronous on the
  // host) seeds them on mount, and `loadRegistry` (potentially networked)
  // merges the community catalog in afterwards.
  let plugins  = $state<MarketplacePlugin[]>([]);
  let themes   = $state<MarketplaceTheme[]>([]);
  let selected = $state<string | null>(null);
  /** Plugin name / theme id currently being installed (mock spinner). */
  let busyId   = $state<string | null>(null);

  let addCustomOpen = $state(false);
  let zipPickerOpen = $state(false);
  /** Plugin currently awaiting permission confirmation before install. */
  let confirmInstall = $state<MarketplacePlugin | null>(null);

  // ── Registry load state ──────────────────────────────────────────────────
  // The modal NEVER blocks on the registry fetch. We render whatever we know
  // locally, then merge community entries in. When the network is down we
  // surface a banner + Retry but keep the Installed list fully usable.
  type RegistryState = 'loading' | 'ready' | 'error';
  let registryState = $state<RegistryState>('loading');
  let registryError = $state<string | null>(null);
  /** Plain `$state` (not a derived from `registryState`) for the Refresh
   *  button + spinner. We saw a flaky case where the `$derived` didn't
   *  re-evaluate after `registryState` was mutated inside an async fn;
   *  setting an independent flag explicitly inside `loadRegistry`'s
   *  try/finally bypasses whatever the issue was. */
  let isLoading = $state(true);
  /**
   * Auto-refresh interval in hours, mirrored from `AppConfig.marketplace.refresh_hours`.
   * `null` = scheduler disabled. The footer dropdown writes through to the
   * backend, which picks up the change within ~60s.
   */
  let refreshHours = $state<number | null>(24);

  // Reset selection + filter on tab switch so the right pane doesn't show a
  // stale plugin while the left list has been swapped to themes.
  function switchTab(next: MarketplaceTab) {
    if (activeTab === next) return;
    activeTab = next;
    selected  = null;
  }

  // ── Derived: filter + sort ───────────────────────────────────────────────
  const allCategories = $derived(pluginCategories(plugins));
  const allPluginTags = $derived(pluginTags(plugins));
  const allThemeTags  = $derived(themeTags(themes));

  function matchesText(haystack: string[], q: string): boolean {
    if (!q) return true;
    const lo = q.toLowerCase();
    return haystack.some(h => h.toLowerCase().includes(lo));
  }
  // (passesPluginFilter / passesThemeFilter inlined inside `$derived.by`
  // below — see the comment there for the reactivity rationale.)

  // Use `$derived.by` so the filter property reads happen inside the
  // tracked closure. Calling helper functions with `filter` as an arg
  // works in Svelte 5 most of the time, but in this surface the
  // installation tab switch wasn't re-deriving consistently — the
  // explicit closure makes the dependency on each accessed field
  // unambiguous to the tracker.
  const filteredPlugins = $derived.by(() => {
    const f = filter;
    return plugins
      .filter((p) => {
        if (f.search) {
          const hay = [p.name, p.description, p.author, ...(p.tags ?? [])];
          if (!matchesText(hay, f.search)) return false;
        }
        if (f.categories.length > 0 && !f.categories.includes(p.category ?? 'other')) return false;
        if (f.sources.length    > 0 && !f.sources.includes(p.source)) return false;
        if (f.tags.length       > 0) {
          const has = (p.tags ?? []).some((t) => f.tags.includes(t));
          if (!has) return false;
        }
        if (f.installation === 'installed' && !p.installed) return false;
        if (f.installation === 'available' &&  p.installed) return false;
        return true;
      })
      .sort((a, b) => a.name.localeCompare(b.name));
  });

  const filteredThemes = $derived.by(() => {
    const f = filter;
    return themes
      .filter((t) => {
        if (f.search) {
          const hay = [t.name, t.description, t.author ?? '', ...(t.tags ?? [])];
          if (!matchesText(hay, f.search)) return false;
        }
        if (f.sources.length > 0 && !f.sources.includes(t.source)) return false;
        if (f.tags.length    > 0) {
          const has = (t.tags ?? []).some((tag) => f.tags.includes(tag));
          if (!has) return false;
        }
        if (f.themeVariant !== 'all' && t.variant !== f.themeVariant) return false;
        if (f.installation === 'installed' && !t.installed) return false;
        if (f.installation === 'available' &&  t.installed) return false;
        return true;
      })
      .sort((a, b) => a.name.localeCompare(b.name));
  });

  const installedPlugins = $derived(filteredPlugins.filter(p =>  p.installed));
  const availablePlugins = $derived(filteredPlugins.filter(p => !p.installed));
  const installedThemes  = $derived(filteredThemes .filter(t =>  t.installed));
  const availableThemes  = $derived(filteredThemes .filter(t => !t.installed));

  const totalShown = $derived(activeTab === 'plugins' ? filteredPlugins.length : filteredThemes.length);
  const totalAll   = $derived(activeTab === 'plugins' ? plugins.length        : themes.length);

  // Resolve the currently-selected card across both lists. Returned as a
  // tagged union so the detail pane can branch on its shape.
  const selectedEntry = $derived.by(() => {
    if (!selected) return null;
    if (activeTab === 'plugins') {
      const p = plugins.find(x => x.name === selected);
      return p ? { kind: 'plugin' as const, plugin: p } : null;
    }
    const t = themes.find(x => x.id === selected);
    return t ? { kind: 'theme' as const, theme: t } : null;
  });

  // ── Mock actions ─────────────────────────────────────────────────────────
  async function fakeDelay(ms = 700) {
    await new Promise(r => setTimeout(r, ms));
  }

  onMount(() => { void bootstrap(); });

  /**
   * Two-step load:
   *   1. `listInstalled` — fast, synchronous on the host. Populates the
   *      Installed tabs immediately so the modal never opens empty.
   *   2. `loadRegistry`  — potentially networked. Brings in the community
   *      catalog; failures here only suppress the available list and leave
   *      the installed slice untouched.
   */
  async function bootstrap() {
    try {
      const local = await listInstalled();
      plugins = local.plugins;
      themes  = local.themes;
    } catch (err) {
      // Failing here would mean the host is broken, not the network — surface
      // it via toast but still try the registry fetch so the user gets *some*
      // content if the IPC recovers.
      uiStore.showToast(`Could not read installed plugins: ${err}`, 'error');
    }
    // Pull the persisted auto-refresh interval so the footer dropdown
    // reflects the actual config (defaults to 24h on first boot).
    try {
      refreshHours = await getMarketplaceRefreshHours();
    } catch { /* footer dropdown is non-critical */ }
    await loadRegistry();
  }

  /**
   * Fetch the community catalog and merge it into the in-memory lists,
   * preserving any local `installed` / `enabled` state the user has changed
   * since opening the modal. Idempotent — safe to call repeatedly (Retry).
   *
   * `force === true` bypasses the host's 1h disk cache (wired to the
   * footer's Refresh button). The initial bootstrap uses the cached path
   * so the modal opens instantly when the cache is fresh.
   */
  async function loadRegistry(force = false) {
    isLoading     = true;
    registryState = 'loading';
    registryError = null;
    try {
      const remote = force ? await refreshRegistry() : await fetchRegistry();
      // The backend is authoritative: it already reconciles installed /
      // enabled from `marketplace_installed.json` + the on-disk manifest
      // scan. The old merge that preserved frontend state on top of the
      // network response is gone — it was a Phase-1 leftover and would
      // mask legitimate updates from the catalog refresh.
      plugins = remote.plugins;
      themes  = remote.themes;
      registryState = 'ready';
    } catch (err) {
      registryError = `${err instanceof Error ? err.message : err}`;
      registryState = 'error';
    } finally {
      // `finally` guarantees the button + spinner unblock even if the
      // try/catch above re-throws or a subsequent edit forgets to reset.
      isLoading = false;
    }
  }

  /** Replace `plugins[i]` (matching by name) with the freshly-returned entry. */
  function patchPlugin(updated: MarketplacePlugin) {
    plugins = plugins.map(p => (p.name === updated.name ? updated : p));
  }
  function patchTheme(updated: MarketplaceTheme) {
    themes = themes.map(t => (t.id === updated.id ? updated : t));
  }

  /** Entry point from the row / detail Install buttons — opens the
   *  permission confirmation modal first. The actual install runs through
   *  `runInstall` once the user confirms. */
  function installPlugin(p: MarketplacePlugin) {
    if (busyId) return;
    confirmInstall = p;
  }

  async function runInstall(p: MarketplacePlugin) {
    confirmInstall = null;
    if (busyId) return;
    busyId = p.name;
    try {
      const updated = await ipcInstallPlugin(p.name);
      patchPlugin(updated);
      uiStore.showToast(
        `Installed "${p.name}" — enable it from the detail pane when ready.`,
        'success',
      );
    } catch (err) {
      uiStore.showToast(`Install failed: ${err}`, 'error');
    } finally {
      busyId = null;
    }
  }

  async function uninstallPlugin(p: MarketplacePlugin) {
    if (busyId) return;
    busyId = p.name;
    try {
      const updated = await ipcUninstallPlugin(p.name);
      patchPlugin(updated);
      uiStore.showToast(`Uninstalled "${p.name}".`, 'success');
    } catch (err) {
      uiStore.showToast(`Uninstall failed: ${err}`, 'error');
    } finally {
      busyId = null;
    }
  }

  /** Flip the plugin's enable state from the detail pane — same effect as
   *  toggling the per-row switch in the Plugin Manager. The backend mirrors
   *  the change in its own state; Phase 3 will also drive the live plugin
   *  host via `pluginStore.togglePlugin`. */
  async function togglePluginEnabled(p: MarketplacePlugin, next: boolean) {
    if (!p.installed) return;
    try {
      const updated = await ipcSetPluginEnabled(p.name, next);
      patchPlugin(updated);
      uiStore.showToast(
        next ? `Enabled "${p.name}".` : `Disabled "${p.name}".`,
        'success',
      );
    } catch (err) {
      uiStore.showToast(`Could not toggle "${p.name}": ${err}`, 'error');
      // Re-sync local state from server in case the optimistic toggle was wrong.
      void loadRegistry();
    }
  }

  async function installTheme(t: MarketplaceTheme) {
    if (busyId) return;
    busyId = t.id;
    try {
      const updated = await ipcInstallTheme(t.id);
      patchTheme(updated);
      uiStore.showToast(
        `Theme "${t.name}" added — apply it from Settings → Appearance.`,
        'success',
      );
    } catch (err) {
      uiStore.showToast(`Install failed: ${err}`, 'error');
    } finally {
      busyId = null;
    }
  }

  async function uninstallTheme(t: MarketplaceTheme) {
    if (busyId) return;
    busyId = t.id;
    try {
      const updated = await ipcUninstallTheme(t.id);
      patchTheme(updated);
      uiStore.showToast(`Theme "${t.name}" removed.`, 'success');
    } catch (err) {
      uiStore.showToast(`Uninstall failed: ${err}`, 'error');
    } finally {
      busyId = null;
    }
  }

  // ── Filter chip toggling helpers ─────────────────────────────────────────
  function toggleArr<T>(arr: T[], v: T): T[] {
    return arr.includes(v) ? arr.filter(x => x !== v) : [...arr, v];
  }
  function toggleCategory(c: string) { filter.categories = toggleArr(filter.categories, c); }
  function toggleTag(t: string)      { filter.tags       = toggleArr(filter.tags, t); }
  function resetFilters() {
    const search = filter.search;
    filter = { ...emptyFilter(), search };
  }
  function clearAll() { filter = emptyFilter(); }

  const hasActiveFilters = $derived(
    filter.categories.length > 0
    || filter.tags.length > 0
    || filter.sources.length > 0
    || filter.installation !== 'all'
    || filter.themeVariant !== 'all',
  );

  // ── Tab + dropdown bindings ───────────────────────────────────────────────
  const topTabs = $derived<TabItem[]>([
    { id: 'plugins', label: 'Plugins', icon: Package, badge: plugins.length },
    { id: 'themes',  label: 'Themes',  icon: Palette, badge: themes.length  },
  ]);
  const installTabs: TabItem[] = [
    { id: 'all',       label: 'All' },
    { id: 'available', label: 'Available' },
    { id: 'installed', label: 'Installed' },
  ];

  // Source filter — single-select. Only the marketplace-tracked origins
  // (Community / Custom) get their own chip; "Local" isn't a category, it's
  // just the visual marker for plugins the user installed by hand. Such
  // plugins surface naturally in the Installed tab regardless.
  const sourceTabs: TabItem[] = [
    { id: 'all',       label: 'All sources' },
    { id: 'community', label: 'Community' },
    { id: 'custom',    label: 'Custom' },
  ];
  const sourceTabValue = $derived(
    filter.sources.length === 0 ? 'all' : (filter.sources[0] as string),
  );
  function setSourceTab(id: string) {
    filter.sources = id === 'all' ? [] : [id as MarketplaceSource];
  }

  const variantTabs: TabItem[] = [
    { id: 'all',   label: 'All'   },
    { id: 'dark',  label: 'Dark'  },
    { id: 'light', label: 'Light' },
  ];

  const categoryItems = $derived<DropdownItem[]>(
    allCategories.map(c => ({
      kind: 'item' as const, id: c, label: c,
      active: filter.categories.includes(c),
      onclick: () => toggleCategory(c),
    })),
  );
  const pluginTagItems = $derived<DropdownItem[]>(
    allPluginTags.map(t => ({
      kind: 'item' as const, id: t, label: t,
      active: filter.tags.includes(t),
      onclick: () => toggleTag(t),
    })),
  );
  const themeTagItems = $derived<DropdownItem[]>(
    allThemeTags.map(t => ({
      kind: 'item' as const, id: t, label: t,
      active: filter.tags.includes(t),
      onclick: () => toggleTag(t),
    })),
  );

  function summarizeSelection(label: string, selected: string[], total: number): string {
    if (selected.length === 0) return `${label}: any`;
    if (selected.length === 1) return `${label}: ${selected[0]}`;
    return `${label}: ${selected.length} of ${total}`;
  }

  // ── Add custom source — async resolve via Phase-4 3-mode resolver ───────
  let customRepo     = $state('');
  let customRef      = $state('');
  let customSubpath  = $state('');
  /** True while the backend is hitting GitHub to resolve the URL. */
  let customResolving = $state(false);

  function resetCustomForm() {
    customRepo     = '';
    customRef      = '';
    customSubpath  = '';
    customResolving = false;
  }
  async function submitCustomSource() {
    if (!customRepo.trim()) {
      uiStore.showToast('Repository URL is required.', 'error');
      return;
    }
    if (customResolving) return;
    customResolving = true;
    try {
      const resolved = await ipcAddCustomSource({
        repo:    customRepo.trim(),
        ref:     customRef.trim()     || undefined,
        subpath: customSubpath.trim() || undefined,
      });
      // Merge resolved entries into the in-memory list — overwrite by name
      // when the source already had a placeholder.
      const byName = new Map(plugins.map(p => [p.name, p]));
      for (const r of resolved) byName.set(r.name, r);
      plugins = Array.from(byName.values());

      const summary = resolved.length === 1
        ? `Custom source "${resolved[0].name}" added.`
        : `Custom source added — resolved ${resolved.length} plugins.`;
      uiStore.showToast(summary, 'success');
      addCustomOpen = false;
      resetCustomForm();
    } catch (err) {
      uiStore.showToast(`Could not add custom source: ${err}`, 'error');
    } finally {
      customResolving = false;
    }
  }

  // ── Auto-refresh interval ────────────────────────────────────────────────
  async function changeRefreshHours(next: number | null) {
    const prev = refreshHours;
    refreshHours = next;
    try {
      await setMarketplaceRefreshHours(next);
    } catch (err) {
      refreshHours = prev;
      uiStore.showToast(`Could not update auto-refresh: ${err}`, 'error');
    }
  }

  // ── Remove custom source ─────────────────────────────────────────────────
  async function removeCustomSource(p: MarketplacePlugin) {
    if (p.source !== 'custom') return;
    try {
      const removed = await ipcRemoveCustomSource({
        repo:    p.entry.repo,
        subpath: p.entry.subpath ?? undefined,
      });
      if (!removed) {
        uiStore.showToast(`No custom source matched ${p.entry.repo}.`, 'warning');
        return;
      }
      // Drop every plugin that came from the same pointer.
      plugins = plugins.filter(x => !(
        x.source === 'custom' &&
        x.entry.repo === p.entry.repo &&
        (x.entry.subpath ?? null) === (p.entry.subpath ?? null)
      ));
      if (selected === p.name) selected = null;
      uiStore.showToast(`Removed custom source for ${p.entry.repo}.`, 'success');
    } catch (err) {
      uiStore.showToast(`Could not remove custom source: ${err}`, 'error');
    }
  }

  // ── Import ZIP — real flow (kept as the only entry point for sideload). ──
  let zipImporting = $state(false);
  async function runZipImport(picked: string) {
    zipPickerOpen = false;
    if (zipImporting) return;
    zipImporting = true;
    try {
      const result = await importPluginZipFromPath(picked);
      uiStore.showToast(`Imported "${result.plugin_name}" (${result.files} files)`, 'success');
      // Backend already emits `arbor://plugins-reloaded`; trigger a reload here
      // too so the host PluginPanel refreshes immediately even if the listener
      // is racy.
      await reloadPlugins();
    } catch (err) {
      uiStore.showToast(`Import failed: ${err}`, 'error');
    } finally {
      zipImporting = false;
    }
  }

  // ── UI helpers ───────────────────────────────────────────────────────────
  function permissionChips(p: MarketplacePlugin): { icon: typeof Globe; label: string; tone: 'safe' | 'warn' | 'danger' }[] {
    const out: { icon: typeof Globe; label: string; tone: 'safe' | 'warn' | 'danger' }[] = [];
    const perms = p.permissions;
    if (!perms) return out;
    if (perms.network?.length)    out.push({ icon: Globe,   label: `net: ${perms.network.join(', ')}`, tone: 'warn' });
    if (perms.fs && perms.fs !== 'none') {
      const unrestricted = perms.fs_scope?.includes('*');
      out.push({ icon: Shield, label: `fs: ${perms.fs}${unrestricted ? ' *' : ''}`, tone: unrestricted ? 'danger' : 'warn' });
    }
    if (perms.git && perms.git !== 'none')
      out.push({ icon: FolderGit2, label: `git: ${perms.git}`, tone: perms.git === 'history_rewrite' ? 'danger' : 'warn' });
    if (perms.terminal && perms.terminal !== 'none')
      out.push({ icon: Tag, label: `term: ${perms.terminal}`, tone: perms.terminal === 'any' ? 'danger' : 'warn' });
    return out;
  }

  function sourceBadgeLabel(s: MarketplaceSource): string {
    switch (s) {
      case 'community': return 'Community';
      case 'custom':    return 'Custom source';
      case 'local':     return 'Local';
    }
  }
  function sourceBadgeTooltip(s: MarketplaceSource): string {
    switch (s) {
      case 'community':
        return 'Listed on the arbor-extensions registry — vetted via PR review.';
      case 'custom':
        return 'Third-party git URL you added by hand. Inspect before enabling.';
      case 'local':
        return 'Manually installed plugin (zip import or dev folder). Not tied to a marketplace entry.';
    }
  }
  function sourceBadgeClass(s: MarketplaceSource): string {
    return `mk-badge mk-badge-${s}`;
  }
  /** Icon component picked by source — used for the compact row marker
   *  that replaces the verbose Community/Custom/Local pills inline. */
  function sourceIcon(s: MarketplaceSource): typeof Globe {
    switch (s) {
      case 'community': return Globe;
      case 'custom':    return LinkIcon;
      case 'local':     return Package;
    }
  }

  /** True when the icon value is an inline SVG string (Phase 1 mock); false
   *  when it's a URL we should hand to `<img src>` (Phase 2+). Matters because
   *  inline SVGs need `{@html}` so `currentColor` inherits the parent's tint —
   *  `<img>` would render the SVG in an isolated context and ignore our CSS. */
  function isInlineSvg(s: string): boolean {
    return s.trimStart().startsWith('<svg');
  }
</script>

<Modal
  {onClose}
  width="min(1480px, 96vw)"
  height="min(880px, 92vh)"
  padBody={false}
  ariaLabel="Marketplace"
>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Store size={14} />
      <span class="modal-title">Marketplace</span>
      {#snippet actions()}
        <Tabs
          variant="solid"
          size="sm"
          value={activeTab}
          items={topTabs}
          onSelect={(id) => switchTab(id as MarketplaceTab)}
          ariaLabel="Marketplace section"
        >
          {#snippet itemContent({ item, active }: { item: TabItem; active: boolean })}
            {#if item.icon}{@const Ic = item.icon}<Ic size={12} />{/if}
            {#if item.label}<span class="tab-label">{item.label}</span>{/if}
            {#if item.badge !== undefined && item.badge !== null && item.badge !== ''}
              <span class="mk-tab-badge" class:active>{item.badge}</span>
            {/if}
          {/snippet}
        </Tabs>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <!-- ── Body ────────────────────────────────────────────────────────────── -->
  <div class="mk-root">

    <!-- LEFT: filters + list -->
    <aside class="mk-left mk-card">
      <!-- Installation tabs — split installed / available into distinct slots -->
      <div class="mk-install-tabs">
        <Tabs
          variant="pill"
          size="sm"
          value={filter.installation}
          items={installTabs}
          fill
          onSelect={(id) => (filter.installation = id as MarketplaceFilter['installation'])}
          ariaLabel="Installation state"
        />
      </div>

      <div class="mk-search-wrap">
        <Input
          type="search"
          bind:value={filter.search}
          placeholder={activeTab === 'plugins'
            ? 'Search plugins by name, tag, author…'
            : 'Search themes by name or tag…'}
          clearable
          ariaLabel="Marketplace search"
        >
          {#snippet iconStart()}<Search size={13} />{/snippet}
        </Input>
      </div>

      <!-- Filter toolbar: multi-select dropdowns + compact source / variant chips -->
      <div class="mk-filters">
        {#if activeTab === 'plugins'}
          {#if allCategories.length > 0}
            <Dropdown
              position="fixed"
              direction="down"
              selectionMode="multiple"
              width="220px"
              searchable
              searchPlaceholder="Filter categories…"
              items={categoryItems}
            >
              {#snippet trigger({ open, toggle })}
                <button class="mk-filter-trigger" class:active={filter.categories.length > 0}
                        onclick={toggle} aria-expanded={open}>
                  <span class="mk-filter-label">{summarizeSelection('Category', filter.categories, allCategories.length)}</span>
                  <ChevronDown size={11} />
                </button>
              {/snippet}
            </Dropdown>
          {/if}
          {#if allPluginTags.length > 0}
            <Dropdown
              position="fixed"
              direction="down"
              selectionMode="multiple"
              width="240px"
              searchable
              searchPlaceholder="Filter tags…"
              items={pluginTagItems}
            >
              {#snippet trigger({ open, toggle })}
                <button class="mk-filter-trigger" class:active={filter.tags.length > 0}
                        onclick={toggle} aria-expanded={open}>
                  <Tag size={10} />
                  <span class="mk-filter-label">{summarizeSelection('Tags', filter.tags, allPluginTags.length)}</span>
                  <ChevronDown size={11} />
                </button>
              {/snippet}
            </Dropdown>
          {/if}
        {:else}
          {#if allThemeTags.length > 0}
            <Dropdown
              position="fixed"
              direction="down"
              selectionMode="multiple"
              width="220px"
              searchable
              searchPlaceholder="Filter tags…"
              items={themeTagItems}
            >
              {#snippet trigger({ open, toggle })}
                <button class="mk-filter-trigger" class:active={filter.tags.length > 0}
                        onclick={toggle} aria-expanded={open}>
                  <Tag size={10} />
                  <span class="mk-filter-label">{summarizeSelection('Tags', filter.tags, allThemeTags.length)}</span>
                  <ChevronDown size={11} />
                </button>
              {/snippet}
            </Dropdown>
          {/if}
          <Tabs
            variant="pill"
            size="sm"
            value={filter.themeVariant}
            items={variantTabs}
            onSelect={(id) => (filter.themeVariant = id as MarketplaceFilter['themeVariant'])}
            ariaLabel="Theme variant"
          />
        {/if}

        <Tabs
          variant="pill"
          size="sm"
          value={sourceTabValue}
          items={sourceTabs}
          onSelect={setSourceTab}
          ariaLabel="Source"
        />

        {#if hasActiveFilters}
          <button class="mk-link mk-link-end" onclick={resetFilters} use:tooltip={'Clear category/tag/source filters'}>
            <FilterIcon size={10} /> Reset
          </button>
        {/if}
      </div>

      <!-- Registry status banner — non-blocking. Only shown when the
           community catalog is loading or failed; the Installed list above
           keeps working regardless of network state. -->
      {#if registryState === 'loading'}
        <div class="mk-status-slot">
          <Alert variant="info" compact noIcon>
            <Spinner size="xs" label="Fetching marketplace catalog…" />
          </Alert>
        </div>
      {:else if registryState === 'error'}
        <div class="mk-status-slot">
          <Alert variant="warning" compact title="Registry unavailable">
            {registryError}. Installed items are still listed below.
            {#snippet actions()}
              <Button size="xs" variant="secondary" onclick={() => loadRegistry()}>
                {#snippet iconStart()}<RefreshCw size={11} />{/snippet}
                Retry
              </Button>
            {/snippet}
          </Alert>
        </div>
      {/if}

      <!-- LIST — single flat list, sorted alphabetically. Visibility of installed
           vs available items is driven by the installation tabs above. -->
      <div class="mk-list">
        {#if activeTab === 'plugins'}
          {#if filteredPlugins.length === 0}
            <div class="mk-empty">
              <Package size={28} class="mk-empty-icon" />
              <p>No plugins match the current filters.</p>
              {#if hasActiveFilters || filter.search}
                <button class="mk-link" onclick={clearAll}>Clear all filters</button>
              {/if}
            </div>
          {:else}
            {#each filteredPlugins as p (p.name)}
              {@const SrcIcon = sourceIcon(p.source)}
              <button class="mk-row" class:selected={selected === p.name}
                      onclick={() => (selected = p.name)}>
                {#if p.icon}
                  {#if isInlineSvg(p.icon)}
                    <span class="mk-icon-art mk-icon-art-sm" class:dim={!p.installed} aria-hidden="true">{@html p.icon}</span>
                  {:else}
                    <img class="mk-icon-art mk-icon-art-sm" class:dim={!p.installed} src={p.icon} alt="" />
                  {/if}
                {:else}
                  <!-- Same chrome as the Plugin Manager row (PluginPanel) so the
                       two surfaces look like one product. No greyed-out variant
                       for uninstalled — the row's "Install" button + missing
                       "Installed" pill already signal availability. -->
                  <Monogram name={p.name} initials={p.name[0].toUpperCase()}
                            color="var(--accent-subtle)"
                            fg="var(--accent)"
                            size={42}
                            class="mk-card-icon" />
                {/if}
                <div class="mk-card-body">
                  <div class="mk-card-top">
                    <span class="mk-card-name">{p.name}</span>
                    <span class="mk-card-version">v{p.version}</span>
                    <!-- Compact status cluster — replaces the verbose
                         Installed / source / Experimental / Update pills.
                         Each glyph carries its own colour + tooltip; the
                         full labels live in the detail pane. The
                         `SrcIcon` const is hoisted to the {#each} above
                         because {@const} can't live inside a <div>. -->
                    <span class="mk-row-icons">
                      <span class="mk-rowicon mk-rowicon-source mk-rowicon-{p.source}"
                            use:tooltip={sourceBadgeTooltip(p.source)}>
                        <SrcIcon size={12} />
                      </span>
                      {#if p.installed}
                        <span class="mk-rowicon mk-rowicon-installed"
                              use:tooltip={'Installed'}>
                          <CheckCircle2 size={12} />
                        </span>
                      {/if}
                      {#if p.update_available}
                        <span class="mk-rowicon mk-rowicon-update"
                              use:tooltip={`Update available — installed v${p.installed_version ?? '?'} · catalog v${p.update_available}`}>
                          <RefreshCw size={12} />
                        </span>
                      {/if}
                      {#if p.experimental}
                        <span class="mk-rowicon mk-rowicon-experimental"
                              use:tooltip={'Flagged experimental in its manifest'}>
                          <FlaskConical size={12} />
                        </span>
                      {/if}
                    </span>
                  </div>
                  <span class="mk-card-desc">{p.description}</span>
                  <div class="mk-card-foot">
                    <span class="mk-card-author">by {p.author}</span>
                  </div>
                </div>
                <ChevronRight size={12} class="mk-card-chev" />
              </button>
            {/each}
          {/if}
        {:else}
          <!-- THEMES -->
          {#if filteredThemes.length === 0}
            <div class="mk-empty">
              <Palette size={28} class="mk-empty-icon" />
              <p>No themes match the current filters.</p>
              {#if hasActiveFilters || filter.search}
                <button class="mk-link" onclick={clearAll}>Clear all filters</button>
              {/if}
            </div>
          {:else}
            {#each filteredThemes as t (t.id)}
              {@const SrcIcon = sourceIcon(t.source)}
              <button class="mk-row mk-theme-row" class:selected={selected === t.id}
                      onclick={() => (selected = t.id)}>
                <div class="mk-theme-swatch" style="background: {t.preview.bg}; color: {t.preview.fg};">
                  <span class="mk-theme-letter" style="color: {t.preview.fg};">Aa</span>
                  <span class="mk-swatch-dot" style="background: {t.preview.accent};"></span>
                </div>
                <div class="mk-card-body">
                  <div class="mk-card-top">
                    <span class="mk-card-name">{t.name}</span>
                    {#if t.variant}<span class="mk-card-version">{t.variant}</span>{/if}
                    <span class="mk-row-icons">
                      <span class="mk-rowicon mk-rowicon-source mk-rowicon-{t.source}"
                            use:tooltip={sourceBadgeTooltip(t.source)}>
                        <SrcIcon size={12} />
                      </span>
                      {#if t.installed}
                        <span class="mk-rowicon mk-rowicon-installed"
                              use:tooltip={'Installed'}>
                          <CheckCircle2 size={12} />
                        </span>
                      {/if}
                    </span>
                  </div>
                  <span class="mk-card-desc">{t.description}</span>
                </div>
                <ChevronRight size={12} class="mk-card-chev" />
              </button>
            {/each}
          {/if}
        {/if}
      </div>

      <!-- LEFT footer -->
      <div class="mk-left-footer">
        <span class="mk-footer-counter">
          Showing <strong>{totalShown}</strong> of <strong>{totalAll}</strong>
          {activeTab === 'plugins' ? 'plugins' : 'themes'}
        </span>
        <button class="mk-link" use:tooltip={'Force re-fetch — bypasses the 1h cache'}
                onclick={() => loadRegistry(true)} disabled={isLoading}>
          {#if isLoading}
            <Spinner size="xs" ariaLabel="Refreshing marketplace" />
          {:else}
            <RefreshCw size={11} />
          {/if}
          Refresh
        </button>
        <span class="mk-auto-refresh"
              use:tooltip={'How often Arbor refreshes the catalog in the background'}>
          <span class="mk-auto-refresh-label">Auto</span>
          <Select
            value={refreshHours ?? 0}
            narrow
            options={[
              { value: 0,   label: 'Off' },
              { value: 6,   label: '6h'  },
              { value: 24,  label: '24h' },
              { value: 168, label: '7d'  },
            ]}
            onchange={(v) => changeRefreshHours(Number(v) || null)}
          />
        </span>
        <button class="mk-link" onclick={() => { addCustomOpen = true; }}>
          <Plus size={11} /> Add custom source
        </button>
        <button class="mk-link" onclick={() => { zipPickerOpen = true; }}>
          <Upload size={11} /> Import .zip
        </button>
      </div>
    </aside>

    <!-- RIGHT: detail pane -->
    <section class="mk-right mk-card">
      {#if !selectedEntry}
        <div class="mk-detail-empty">
          {#if activeTab === 'plugins'}
            <Package size={48} class="mk-empty-icon" />
            <h3>Select a plugin to see details</h3>
            <p>Browse the list on the left. Each plugin shows the permissions it asks for, its source repository and a description.</p>
          {:else}
            <Palette size={48} class="mk-empty-icon" />
            <h3>Select a theme to preview</h3>
            <p>Each theme ships as a JSON file; you can install several and switch between them from Settings → Appearance.</p>
          {/if}
          <div class="mk-detail-hints">
            <div class="mk-hint">
              <span class="mk-badge mk-badge-community">Community</span>
              Listed on the official <code>arbor-extensions</code> repo.
            </div>
            <div class="mk-hint">
              <span class="mk-badge mk-badge-custom">Custom source</span>
              Third-party git URL — inspect the source before enabling.
            </div>
            <div class="mk-hint">
              <span class="mk-badge mk-badge-local">Local</span>
              Manually installed (zip import or dev folder) — has no marketplace entry to update.
            </div>
          </div>
        </div>
      {:else if selectedEntry.kind === 'plugin'}
        {@const p = selectedEntry.plugin}
        <header class="mk-detail-head">
          {#if p.icon}
            {#if isInlineSvg(p.icon)}
              <span class="mk-icon-art mk-icon-art-lg" aria-hidden="true">{@html p.icon}</span>
            {:else}
              <img class="mk-icon-art mk-icon-art-lg" src={p.icon} alt="" />
            {/if}
          {:else}
            <Monogram name={p.name} initials={p.name[0].toUpperCase()}
                      color="var(--accent-subtle)" fg="var(--accent)" size={56}
                      class="mk-detail-icon" />
          {/if}
          <div class="mk-detail-headtext">
            <div class="mk-detail-name-row">
              <h2 class="mk-detail-name">{p.name}</h2>
              {#if p.update_available}
                <span class="mk-card-version mk-card-version-old">v{p.installed_version ?? p.version}</span>
                <span class="mk-version-arrow">→</span>
                <span class="mk-card-version mk-card-version-new">v{p.update_available}</span>
              {:else}
                <span class="mk-card-version">v{p.installed_version ?? p.version}</span>
              {/if}
              <span class={sourceBadgeClass(p.source)}>{sourceBadgeLabel(p.source)}</span>
              {#if p.experimental}<span class="mk-badge mk-badge-experimental">Experimental</span>{/if}
            </div>
            <div class="mk-detail-meta">
              <span>by <strong>{p.author}</strong></span>
              {#if p.category}<span class="mk-dot">·</span><span>{p.category}</span>{/if}
              {#if p.min_arbor_version}<span class="mk-dot">·</span><span>requires Arbor ≥ {p.min_arbor_version}</span>{/if}
            </div>
          </div>
          <div class="mk-detail-actions">
            {#if p.installed}
              <div class="mk-enable-toggle">
                <Toggle
                  checked={p.enabled ?? false}
                  size="md"
                  label={p.enabled ? 'Enabled' : 'Disabled'}
                  labelPosition="before"
                  onchange={(v) => togglePluginEnabled(p, v)}
                />
              </div>
              {#if p.update_available}
                <span use:tooltip={`Re-install at v${p.update_available}`}>
                  <Button
                    variant="primary" size="md"
                    disabled={busyId === p.name}
                    loading={busyId === p.name}
                    onclick={() => installPlugin(p)}
                  >
                    {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
                    {busyId === p.name ? 'Updating…' : `Update to v${p.update_available}`}
                  </Button>
                </span>
              {/if}
              <Button
                variant="secondary" size="md"
                disabled={busyId === p.name}
                onclick={() => uninstallPlugin(p)}
              >
                {#snippet iconStart()}<Trash2 size={13} />{/snippet}
                {busyId === p.name ? 'Removing…' : 'Uninstall'}
              </Button>
            {:else}
              <Button
                variant="primary" size="md"
                disabled={busyId === p.name}
                loading={busyId === p.name}
                onclick={() => installPlugin(p)}
              >
                {#snippet iconStart()}<Plus size={13} />{/snippet}
                {busyId === p.name ? 'Installing…' : 'Install'}
              </Button>
            {/if}
            {#if p.source === 'custom'}
              <span use:tooltip={'Forget this custom source (installed plugins stay)'}>
                <Button
                  variant="ghost" size="md"
                  onclick={() => removeCustomSource(p)}
                >
                  {#snippet iconStart()}<X size={13} />{/snippet}
                  Remove source
                </Button>
              </span>
            {/if}
          </div>
        </header>

        <div class="mk-detail-body">
          <p class="mk-detail-desc">{p.description}</p>

          <!-- Screenshots (mock placeholder) -->
          <section class="mk-detail-section">
            <h4><Eye size={11} /> Preview</h4>
            <div class="mk-screenshots">
              <div class="mk-shot-placeholder">
                <span>Screenshots ship with the plugin's repo.</span>
                <small>Set <code>screenshots = ["docs/1.png", …]</code> in <code>plugin.toml</code> to surface them here.</small>
              </div>
            </div>
          </section>

          <!-- Permissions -->
          {#if p.permissions}
            {@const chips = permissionChips(p)}
            <section class="mk-detail-section">
              <h4><Shield size={11} /> Permissions requested</h4>
              {#if chips.length === 0}
                <p class="mk-detail-muted">This plugin requests no elevated permissions.</p>
              {:else}
                <div class="mk-perm-list">
                  {#each chips as c, i (i)}
                    <span class="mk-perm-chip mk-perm-{c.tone}">
                      <c.icon size={10} />{c.label}
                    </span>
                  {/each}
                </div>
                <p class="mk-detail-muted small">
                  Arbor will show a confirmation dialog with the resolved list before installing.
                  Plugins are <strong>disabled by default</strong> after install — you'll need to enable them manually
                  from the Plugin Manager.
                </p>
              {/if}
            </section>
          {/if}

          <!-- Tags -->
          {#if (p.tags ?? []).length > 0}
            <section class="mk-detail-section">
              <h4><Tag size={11} /> Tags</h4>
              <div class="mk-card-tags mk-card-tags-expanded">
                {#each p.tags ?? [] as tg (tg)}
                  <span class="mk-tag-mini">{tg}</span>
                {/each}
              </div>
            </section>
          {/if}

          <!-- Source links -->
          <section class="mk-detail-section">
            <h4><FolderGit2 size={11} /> Source</h4>
            <div class="mk-source-rows">
              <div class="mk-src-row">
                <span class="mk-src-key">Repository</span>
                <a href={p.entry.repo} target="_blank" rel="noreferrer" class="mk-src-link">
                  {p.entry.repo}
                  <ExternalLink size={10} />
                </a>
              </div>
              {#if p.entry.subpath}
                <div class="mk-src-row">
                  <span class="mk-src-key">Subpath</span>
                  <code>{p.entry.subpath}</code>
                </div>
              {/if}
              {#if p.entry.ref}
                <div class="mk-src-row">
                  <span class="mk-src-key">Ref</span>
                  <code><GitBranch size={9} /> {p.entry.ref}</code>
                </div>
              {:else}
                <div class="mk-src-row">
                  <span class="mk-src-key">Ref</span>
                  <span class="mk-detail-muted small">latest tag (fallback: <code>main</code>)</span>
                </div>
              {/if}
              {#if p.entry.pinned_sha}
                <div class="mk-src-row">
                  <span class="mk-src-key">Pinned SHA</span>
                  <code><Pin size={9} /> {p.entry.pinned_sha.slice(0, 12)}</code>
                </div>
              {/if}
              {#if p.homepage}
                <div class="mk-src-row">
                  <span class="mk-src-key">Homepage</span>
                  <a href={p.homepage} target="_blank" rel="noreferrer" class="mk-src-link">
                    {p.homepage}<ExternalLink size={10} />
                  </a>
                </div>
              {/if}
            </div>
          </section>

          {#if p.source === 'custom'}
            <section class="mk-detail-section">
              <Alert variant="warning" title="Third-party source">
                This plugin lives outside the curated registry. Review its <code>plugin.toml</code>
                and <code>main.lua</code> on GitHub before enabling it — declared permissions
                describe what the plugin <em>can</em> do once enabled.
              </Alert>
            </section>
          {/if}

          {#if p.doc}
            <!-- Authored HTML from plugin.toml's doc_file. Lives at the
                 bottom of the detail body because the doc is reference
                 material — permissions, source and install actions are the
                 primary decision surface above. Styles mirror DocsPanel
                 (`.docs-content`) so plugins authored once look the same in
                 both surfaces. -->
            <section class="mk-detail-section mk-doc-section">
              <h4><Eye size={11} /> Documentation</h4>
              <div class="docs-content mk-doc-card">
                <div class="docs-content-inner">{@html p.doc}</div>
              </div>
            </section>
          {/if}
        </div>
      {:else}
        {@const t = selectedEntry.theme}
        <header class="mk-detail-head mk-theme-head" style="background: {t.preview.bg};">
          <div class="mk-theme-preview-lg" style="background: {t.preview.bg}; color: {t.preview.fg}; border-color: {t.preview.accent};">
            <span class="mk-theme-letter-lg" style="color: {t.preview.fg};">Aa</span>
            <div class="mk-theme-swatches-lg">
              <span class="sw" style="background: {t.preview.accent};"  use:tooltip={'accent'}></span>
              <span class="sw" style="background: {t.preview.success};" use:tooltip={'success'}></span>
              <span class="sw" style="background: {t.preview.warning};" use:tooltip={'warning'}></span>
              <span class="sw" style="background: {t.preview.error};"   use:tooltip={'error'}></span>
            </div>
          </div>
          <div class="mk-detail-headtext mk-theme-headtext" style="color: {t.preview.fg};">
            <div class="mk-detail-name-row">
              <h2 class="mk-detail-name" style="color: {t.preview.fg};">{t.name}</h2>
              {#if t.variant}<span class="mk-badge mk-badge-variant">{t.variant}</span>{/if}
            </div>
            <div class="mk-detail-meta" style="color: {t.preview.fg}; opacity: 0.75;">
              {#if t.author}<span>by <strong>{t.author}</strong></span>{/if}
            </div>
          </div>
          <div class="mk-detail-actions">
            {#if t.installed}
              <Button variant="secondary" size="md"
                      disabled={busyId === t.id}
                      onclick={() => uninstallTheme(t)}>
                {#snippet iconStart()}<Trash2 size={13} />{/snippet}
                {busyId === t.id ? 'Removing…' : 'Remove'}
              </Button>
            {:else}
              <Button variant="primary" size="md"
                      disabled={busyId === t.id}
                      loading={busyId === t.id}
                      onclick={() => installTheme(t)}>
                {#snippet iconStart()}<Plus size={13} />{/snippet}
                {busyId === t.id ? 'Installing…' : 'Install'}
              </Button>
            {/if}
          </div>
        </header>

        <div class="mk-detail-body">
          <p class="mk-detail-desc">{t.description}</p>

          <section class="mk-detail-section">
            <h4><Eye size={11} /> Live preview</h4>
            <!-- Slice-of-Arbor mock: title bar + branches sidebar + mini
                 graph + sample diff + status bar — all rendered with the
                 theme's six preview colours so the user can judge fit
                 before installing. -->
            <div
              class="mk-theme-mock-frame"
              style="
                --pv-bg:      {t.preview.bg};
                --pv-fg:      {t.preview.fg};
                --pv-accent:  {t.preview.accent};
                --pv-success: {t.preview.success};
                --pv-warning: {t.preview.warning};
                --pv-error:   {t.preview.error};
              "
            >
              <!-- Title bar -->
              <div class="mk-pv-titlebar">
                <span class="mk-pv-traffic mk-pv-dot-error"></span>
                <span class="mk-pv-traffic mk-pv-dot-warn"></span>
                <span class="mk-pv-traffic mk-pv-dot-ok"></span>
                <span class="mk-pv-title">arbor — <span class="mk-pv-strong">arbor-extensions</span></span>
              </div>

              <div class="mk-pv-body">
                <!-- Sidebar -->
                <aside class="mk-pv-sidebar">
                  <div class="mk-pv-sect-h">BRANCHES</div>
                  <div class="mk-pv-item mk-pv-active">
                    <span class="mk-pv-dot mk-pv-dot-accent"></span>main
                  </div>
                  <div class="mk-pv-item">
                    <span class="mk-pv-dot mk-pv-dot-muted"></span>feat/marketplace
                  </div>
                  <div class="mk-pv-item">
                    <span class="mk-pv-dot mk-pv-dot-muted"></span>fix/oauth-refresh
                  </div>
                  <div class="mk-pv-sect-h mk-pv-spaced">TAGS</div>
                  <div class="mk-pv-item">
                    <span class="mk-pv-tag-glyph">▸</span>v1.4.0
                  </div>
                </aside>

                <!-- Main content -->
                <div class="mk-pv-main">
                  <!-- Graph -->
                  <div class="mk-pv-graph">
                    <div class="mk-pv-row">
                      <span class="mk-pv-node mk-pv-node-accent"></span>
                      <span class="mk-pv-msg mk-pv-strong">feat: add marketplace auto-refresh</span>
                      <span class="mk-pv-meta">a1b2c3d</span>
                    </div>
                    <div class="mk-pv-row">
                      <span class="mk-pv-node mk-pv-node-secondary"></span>
                      <span class="mk-pv-msg">refactor: extract Select widget</span>
                      <span class="mk-pv-meta">e4f5g6h</span>
                    </div>
                    <div class="mk-pv-row">
                      <span class="mk-pv-node mk-pv-node-secondary"></span>
                      <span class="mk-pv-msg">fix: dropdown flipUp positioning</span>
                      <span class="mk-pv-meta">i7j8k9l</span>
                    </div>
                  </div>

                  <!-- Diff -->
                  <div class="mk-pv-diff">
                    <div class="mk-pv-diff-h">+ src/marketplace/scheduler.rs</div>
                    <div class="mk-pv-diff-add">+ const POLL_SECS: u64 = 600;</div>
                    <div class="mk-pv-diff-rem">- const POLL_SECS: u64 = 60;</div>
                    <div class="mk-pv-diff-ctx">  let interval = read_config();</div>
                  </div>
                </div>
              </div>

              <!-- Status bar -->
              <div class="mk-pv-statusbar">
                <span class="mk-pv-status-chip"><span class="mk-pv-dot mk-pv-dot-accent"></span>main</span>
                <span class="mk-pv-status-chip mk-pv-success">✓ 12</span>
                <span class="mk-pv-status-chip mk-pv-warning">⚠ 2</span>
                <span class="mk-pv-status-chip mk-pv-error">✕ 1</span>
                <span class="mk-pv-status-chip mk-pv-muted mk-pv-status-spacer">accent</span>
              </div>
            </div>
          </section>

          <!-- Swatch row — labelled colour chips so users can grab the
               exact hex / role at a glance even when the mock looks
               busy. -->
          <section class="mk-detail-section">
            <h4><Palette size={11} /> Palette</h4>
            <div class="mk-pv-swatch-row">
              {#each [
                { label: 'Background',  color: t.preview.bg      },
                { label: 'Foreground',  color: t.preview.fg      },
                { label: 'Accent',      color: t.preview.accent  },
                { label: 'Success',     color: t.preview.success },
                { label: 'Warning',     color: t.preview.warning },
                { label: 'Error',       color: t.preview.error   },
              ] as sw (sw.label)}
                <div class="mk-pv-swatch">
                  <span class="mk-pv-swatch-chip" style="background: {sw.color};"></span>
                  <span class="mk-pv-swatch-label">{sw.label}</span>
                  <span class="mk-pv-swatch-hex">{sw.color}</span>
                </div>
              {/each}
            </div>
          </section>

          {#if (t.tags ?? []).length > 0}
            <section class="mk-detail-section">
              <h4><Tag size={11} /> Tags</h4>
              <div class="mk-card-tags mk-card-tags-expanded">
                {#each t.tags ?? [] as tg (tg)}
                  <span class="mk-tag-mini">{tg}</span>
                {/each}
              </div>
            </section>
          {/if}

          <section class="mk-detail-section">
            <h4><FolderGit2 size={11} /> Source</h4>
            <div class="mk-source-rows">
              <div class="mk-src-row">
                <span class="mk-src-key">Repository</span>
                <a href={t.entry.repo} target="_blank" rel="noreferrer" class="mk-src-link">
                  {t.entry.repo}<ExternalLink size={10} />
                </a>
              </div>
              {#if t.entry.subpath}
                <div class="mk-src-row">
                  <span class="mk-src-key">File</span>
                  <code>{t.entry.subpath}</code>
                </div>
              {/if}
            </div>
          </section>
        </div>
      {/if}
    </section>
  </div>
</Modal>

<!-- ── Add custom source — secondary modal ─────────────────────────────── -->
{#if addCustomOpen}
  <Modal
    onClose={() => { addCustomOpen = false; }}
    width="560px"
    height="auto"
    ariaLabel="Add custom source"
  >
    {#snippet header()}
      <ModalHeader title="Add custom source" onClose={() => { addCustomOpen = false; }}>
        <Plus size={14} />
        <span class="modal-title">Add custom source</span>
      </ModalHeader>
    {/snippet}

    <div class="mk-form">
      <p class="mk-form-hint">
        Point Arbor at any GitHub repo. The resolver looks for a <code>plugin.toml</code> at the root
        first, then for an <code>arbor-registry.toml</code> manifest (multi-plugin repos), and finally
        for a plugin at the subpath you specify below.
      </p>

      <label class="mk-form-row">
        <span>Repository URL <span class="required">*</span></span>
        <Input
          bind:value={customRepo}
          placeholder="https://github.com/owner/repo"
          ariaLabel="Repository URL"
        />
      </label>

      <div class="mk-form-grid">
        <label class="mk-form-row">
          <span>Ref <small>(tag, branch or SHA — optional)</small></span>
          <Input
            bind:value={customRef}
            placeholder="defaults to main"
            ariaLabel="Git ref"
          />
        </label>
        <label class="mk-form-row">
          <span>Subpath <small>(optional, for monorepos)</small></span>
          <Input
            bind:value={customSubpath}
            placeholder="plugins/my-plugin"
            ariaLabel="Subpath"
          />
        </label>
      </div>

      <Alert variant="warning" compact>
        Custom sources are unverified — review the plugin's source on GitHub before enabling.
        Plugins are disabled by default after install.
      </Alert>
    </div>

    {#snippet footer()}
      <ModalFooter>
        <Button variant="ghost"
                disabled={customResolving}
                onclick={() => { addCustomOpen = false; resetCustomForm(); }}>
          Cancel
        </Button>
        <Button variant="primary"
                disabled={customResolving || !customRepo.trim()}
                onclick={submitCustomSource}>
          {#snippet iconStart()}
            {#if customResolving}<Spinner size="xs" ariaLabel="Resolving source" />{:else}<Plus size={14} />{/if}
          {/snippet}
          {customResolving ? 'Resolving…' : 'Add source'}
        </Button>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

{#if confirmInstall}
  <MarketplaceInstallConfirm
    plugin={confirmInstall}
    onCancel={() => (confirmInstall = null)}
    onConfirm={() => { const p = confirmInstall!; confirmInstall = null; void runInstall(p); }}
  />
{/if}

{#if zipPickerOpen}
  <FilePickerModal
    mode="file"
    title="Import plugin (.zip)"
    extensions={['zip']}
    onConfirm={runZipImport}
    onCancel={() => { zipPickerOpen = false; }}
  />
{/if}

<style>
  /* ── Layout ────────────────────────────────────────────────────────────
     Studio-style two-card body: outer band is --bg-elevated (4px padding
     + gap), inner cards are --bg-base with rounded corners. */
  .mk-root {
    display: grid;
    grid-template-columns: minmax(420px, 520px) 1fr;
    height: 100%;
    min-height: 0;
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
  }
  .mk-card {
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .mk-left, .mk-right { min-height: 0; }

  /* Installation tabs strip at the top of the left card. */
  .mk-install-tabs {
    padding: 8px 10px 0;
  }

  /* Top tab badge — flips foreground/background when its tab is active so
     the count stays readable on the accent-filled pill (the default Tabs
     badge would read accent-on-accent and disappear). Mirrors the
     StudioModal pattern. */
  .mk-tab-badge {
    font-size: 10px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-subtle);
    border-radius: 8px;
    padding: 1px 5px;
    min-width: 14px;
    text-align: center;
    line-height: 1.3;
    flex-shrink: 0;
  }
  .mk-tab-badge.active {
    background: var(--text-on-accent);
    color: var(--accent);
  }

  /* ── Search ────────────────────────────────────────────────────────── */
  /* Outer gutter — the inner field is rendered by the shared `Input`
     widget so the box / focus / clear chrome stays consistent app-wide. */
  .mk-search-wrap {
    padding: 8px 10px;
  }

  /* ── Filters: compact toolbar with multi-select dropdowns + mini chip
       groups for source / variant. Single horizontal row, wraps when narrow. */
  .mk-filters {
    padding: 4px 10px 8px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .mk-filter-trigger {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    font-size: 11px;
    padding: 5px 8px;
    cursor: pointer;
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .mk-filter-trigger:hover { background: var(--bg-hover); color: var(--text-primary); }
  .mk-filter-trigger.active { border-color: var(--accent); color: var(--accent); }
  .mk-filter-trigger[aria-expanded='true'] { border-color: var(--border-focus); }
  .mk-filter-label { white-space: nowrap; }

  .mk-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: none;
    color: var(--accent);
    font-size: 11px;
    cursor: pointer;
    padding: 2px 4px;
  }
  .mk-link:hover { text-decoration: underline; }
  .mk-link:disabled { opacity: 0.5; cursor: not-allowed; text-decoration: none; }
  .mk-link-end { margin-left: auto; }

  /* Auto-refresh dropdown wrapper — keeps the label + shared Select on the
     same baseline as the other footer links. */
  .mk-auto-refresh {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .mk-auto-refresh-label {
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 9.5px;
  }

  /* Slot around the registry-status Alert — pulls it into the same gutter
     as the list. The loading row is now the shared `<Spinner>` widget
     with a label prop, so no custom row CSS is needed. */
  .mk-status-slot { padding: 6px 10px 0; }

  /* ── List ──────────────────────────────────────────────────────────── */
  .mk-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 6px 12px;
  }
  .mk-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    text-align: left;
    padding: 10px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    cursor: pointer;
    margin-bottom: 2px;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .mk-row:hover { background: var(--bg-hover); }
  .mk-row.selected {
    background: var(--accent-subtle);
    border-color: var(--accent);
  }
  .mk-card-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .mk-card-top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .mk-card-name {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
  }
  .mk-card-version {
    font-size: 10px;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }
  .mk-card-desc {
    font-size: 11.5px;
    color: var(--text-secondary);
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .mk-card-tags {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin-top: 2px;
  }
  .mk-card-tags-expanded { gap: 5px; }
  /* Use the dedicated `--color-tag` palette (same one the shared Badge
     uses via `tone="tag"`) so chips read at a glance and stay coherent
     with the rest of the app. The old `--text-muted` on `--bg-overlay`
     was nearly invisible against the card background. */
  .mk-tag-mini {
    font-size: 9.5px;
    background: color-mix(in srgb, var(--color-tag) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-tag) 32%, transparent);
    color: var(--color-tag);
    padding: 1px 7px;
    border-radius: 999px;
    text-transform: lowercase;
    font-weight: 500;
    letter-spacing: 0.02em;
  }
  .mk-card-author {
    font-size: 10px;
    color: var(--text-muted);
    margin-left: auto;
    font-style: italic;
  }

  /* New compact row footer — replaces the (now removed) tag chips.
     Only carries the author name; everything else moved to the
     detail pane to keep the row free of visual noise. */
  .mk-card-foot {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    margin-top: 2px;
  }

  /* Compact icon cluster that replaces the old verbose pills
     (Installed / Local / Custom / Community / Experimental / Update).
     Each glyph is small, semantic, and carries its own tooltip — the
     full labels live in the detail pane. */
  .mk-row-icons {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: 2px;
  }
  .mk-rowicon {
    display: inline-flex;
    align-items: center;
    cursor: help;
    color: var(--text-disabled);
    line-height: 0;
  }
  .mk-rowicon-installed    { color: var(--success);     }
  .mk-rowicon-update       { color: var(--color-stash); }
  .mk-rowicon-experimental { color: var(--warning);     }
  .mk-rowicon-community    { color: var(--accent);      }
  .mk-rowicon-custom       { color: var(--warning);     }
  .mk-rowicon-local        { color: var(--info);        }
  .mk-row :global(.mk-card-chev) {
    flex-shrink: 0;
    color: var(--text-disabled);
  }
  .mk-row.selected :global(.mk-card-chev) { color: var(--accent); }

  /* Plugin icon — same slot as the Monogram fallback. Inline SVGs inherit
     `color` so the strokes pick up the accent palette; `<img>` icons stay
     untouched (Phase 2+ uses URLs and can't be retinted). */
  .mk-icon-art {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    background: var(--accent-subtle);
    color: var(--accent);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .mk-icon-art-sm { width: 36px; height: 36px; }
  .mk-icon-art-sm :global(svg) { width: 22px; height: 22px; display: block; }
  .mk-icon-art-lg { width: 56px; height: 56px; }
  .mk-icon-art-lg :global(svg) { width: 32px; height: 32px; display: block; }
  /* `<img class="mk-icon-art-*">` variants stretch to fill the container — no
     explicit sizing needed; the container width/height already pin them. */
  /* Dim the icon for entries the user hasn't installed yet — matches the
     softer Monogram tint used in the same slot. */
  .mk-icon-art.dim {
    background: var(--bg-overlay);
    color: var(--text-secondary);
  }

  /* ── Badges ────────────────────────────────────────────────────────── */
  .mk-badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 9.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }
  .mk-badge-installed {
    background: rgba(var(--success-rgb, 0, 180, 90), 0.15);
    color: var(--success);
    border: 1px solid color-mix(in srgb, var(--success) 30%, transparent);
  }
  /* Version pills in the detail header — when an update is available we
     render `installed → catalog` with the old version dimmed. */
  .mk-card-version-old {
    color: var(--text-disabled);
    text-decoration: line-through;
    text-decoration-thickness: 1px;
  }
  .mk-card-version-new {
    color: var(--warning);
    border-color: color-mix(in srgb, var(--warning) 40%, transparent);
  }
  .mk-version-arrow {
    color: var(--text-disabled);
    font-size: 10px;
  }
  .mk-badge-community {
    background: var(--accent-subtle);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .mk-badge-custom {
    background: color-mix(in srgb, var(--warning) 15%, transparent);
    color: var(--warning);
    border: 1px solid color-mix(in srgb, var(--warning) 35%, transparent);
  }
  /* Local — info tone. Sideloaded / dev plugins aren't trusted by us but
     they're not third-party either: the user put them there themselves.
     Painting them with the `--info` palette makes them visually distinct
     from community (accent) and custom (warning) without flagging them
     as "warn" or "danger". */
  .mk-badge-local {
    background: color-mix(in srgb, var(--info) 16%, transparent);
    color: var(--info);
    border: 1px solid color-mix(in srgb, var(--info) 38%, transparent);
  }
  .mk-badge-experimental {
    background: color-mix(in srgb, var(--warning) 20%, transparent);
    color: var(--warning);
    border: 1px solid color-mix(in srgb, var(--warning) 40%, transparent);
  }
  .mk-badge-variant {
    background: var(--bg-overlay);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    text-transform: capitalize;
  }

  /* ── Theme cards / preview ─────────────────────────────────────────── */
  .mk-theme-row { align-items: stretch; }
  .mk-theme-swatch {
    position: relative;
    width: 48px;
    height: 48px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .mk-theme-letter {
    font-size: 16px;
    font-weight: 700;
    font-family: var(--font-mono);
  }
  .mk-swatch-dot {
    position: absolute;
    right: 4px;
    bottom: 4px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.25);
  }

  /* ── Left footer ───────────────────────────────────────────────────── */
  .mk-left-footer {
    border-top: 1px solid var(--border-subtle);
    padding: 6px 10px 8px;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 12px;
  }
  .mk-footer-counter {
    flex: 1 0 100%;
    font-size: 11px;
    color: var(--text-muted);
  }
  .mk-footer-counter strong { color: var(--text-secondary); font-weight: 600; }

  /* ── Right pane — empty state ──────────────────────────────────────── */
  .mk-detail-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 36px;
    gap: 8px;
    color: var(--text-muted);
  }
  .mk-detail-empty :global(.mk-empty-icon) { color: var(--text-disabled); }
  .mk-detail-empty h3 {
    margin: 8px 0 0;
    font-size: var(--font-size-lg);
    color: var(--text-secondary);
    font-weight: 500;
  }
  .mk-detail-empty p {
    margin: 0;
    max-width: 460px;
    line-height: 1.5;
    font-size: var(--font-size-sm);
  }
  .mk-detail-hints {
    margin-top: 20px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    max-width: 460px;
  }
  .mk-hint {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    text-align: left;
    line-height: 1.4;
  }

  /* ── Right pane — populated state ──────────────────────────────────── */
  .mk-detail-head {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    padding: 18px 22px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .mk-detail-headtext { flex: 1; min-width: 0; }
  .mk-detail-name-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }
  .mk-detail-name {
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: 600;
    color: var(--text-primary);
  }
  .mk-detail-meta {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .mk-detail-meta strong { color: var(--text-secondary); font-weight: 500; }
  .mk-dot { opacity: 0.5; }

  .mk-detail-actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }
  .mk-enable-toggle {
    display: inline-flex;
    align-items: center;
    padding: 4px 12px;
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
  }

  .mk-detail-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 18px 22px 28px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .mk-detail-desc {
    margin: 0;
    font-size: var(--font-size-sm);
    line-height: 1.55;
    color: var(--text-primary);
  }

  .mk-detail-section h4 {
    margin: 0 0 8px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
    font-weight: 600;
  }
  .mk-detail-muted { color: var(--text-muted); font-size: var(--font-size-xs); margin: 0; }
  .mk-detail-muted.small { font-size: 11px; margin-top: 6px; }

  /* Permissions */
  .mk-perm-list {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .mk-perm-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-family: var(--font-mono);
    border: 1px solid var(--border-subtle);
  }
  .mk-perm-safe   { background: color-mix(in srgb, var(--success) 12%, transparent); color: var(--success); border-color: color-mix(in srgb, var(--success) 30%, transparent); }
  .mk-perm-warn   { background: color-mix(in srgb, var(--warning) 12%, transparent); color: var(--warning); border-color: color-mix(in srgb, var(--warning) 30%, transparent); }
  .mk-perm-danger { background: color-mix(in srgb, var(--error) 12%, transparent);   color: var(--error);   border-color: color-mix(in srgb, var(--error) 35%, transparent); }

  /* Source rows */
  .mk-source-rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 12px;
  }
  .mk-src-row {
    display: grid;
    grid-template-columns: 100px 1fr;
    gap: 12px;
    align-items: center;
    font-size: var(--font-size-xs);
  }
  .mk-src-key {
    color: var(--text-muted);
    text-transform: uppercase;
    font-size: 10px;
    letter-spacing: 0.5px;
  }
  .mk-src-link {
    color: var(--accent);
    text-decoration: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mk-src-link:hover { text-decoration: underline; }
  .mk-src-row code {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    background: var(--bg-overlay);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
  }

  /* Authored plugin documentation. Styles mirror DocsPanel's `.docs-content`
     so a plugin authored once renders identically here and in the docs
     pane. Scoped via :global() because the body is `{@html}`-rendered.
     The `.mk-doc-section` wrapper itself needs no rules — children below
     (`.mk-doc-card`, `:global(...)` selectors) carry the styling. */
  /* Doc block grows to its full content height — the modal's detail
     pane (`.mk-detail-body`) is the one and only scroll container in
     this region. Without this, dragging the page down with the mouse
     wheel scrolls the *inner* doc-card and leaves the rest of the
     detail (Source / Permissions / etc.) inaccessible. */
  .mk-doc-card {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
  }
  .mk-doc-card :global(.docs-content-inner) {
    padding: 18px 22px 22px;
    user-select: text;
  }
  .mk-doc-card :global(.docs-content-inner *) { user-select: text; }

  .mk-doc-card :global(h1) {
    font-size: 19px;
    font-weight: 700;
    color: var(--text-primary);
    margin: 0 0 14px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border-subtle);
    letter-spacing: -0.2px;
  }
  .mk-doc-card :global(h2) {
    font-size: 11px;
    font-weight: 700;
    color: var(--text-secondary);
    margin: 22px 0 10px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border-subtle);
    text-transform: uppercase;
    letter-spacing: 0.7px;
  }
  .mk-doc-card :global(h2 code) {
    text-transform: none;
    letter-spacing: 0;
    font-size: 11px;
  }
  .mk-doc-card :global(h3) {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-primary);
    margin: 16px 0 6px;
    letter-spacing: 0.2px;
  }
  .mk-doc-card :global(h4) {
    font-size: 10px;
    font-weight: 700;
    color: var(--text-muted);
    margin: 12px 0 4px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .mk-doc-card :global(h1):first-child,
  .mk-doc-card :global(h2):first-child,
  .mk-doc-card :global(h3):first-child { margin-top: 0; }
  .mk-doc-card :global(p) {
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    line-height: 1.65;
    margin: 0 0 10px;
  }
  .mk-doc-card :global(ul),
  .mk-doc-card :global(ol) {
    margin: 0 0 12px;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .mk-doc-card :global(li) {
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    line-height: 1.5;
  }
  .mk-doc-card :global(strong) { color: var(--text-primary); font-weight: 600; }
  .mk-doc-card :global(kbd) {
    display: inline-block;
    font-family: var(--font-code);
    font-size: 10px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    white-space: nowrap;
  }
  .mk-doc-card :global(code) {
    font-family: var(--font-code);
    font-size: 11px;
    background: var(--bg-overlay);
    padding: 1px 4px;
    border-radius: var(--radius-sm);
    color: var(--accent);
  }
  .mk-doc-card :global(pre) {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 12px 14px;
    overflow-x: auto;
    margin: 0 0 14px;
  }
  .mk-doc-card :global(pre code) {
    background: none;
    padding: 0;
    font-size: 11px;
    color: var(--text-secondary);
    border-radius: 0;
  }
  .mk-doc-card :global(table) {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--font-size-xs);
    margin: 10px 0 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .mk-doc-card :global(th) {
    text-align: left;
    padding: 7px 12px;
    font-size: 9.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    background: rgba(255, 255, 255, 0.02);
    border-bottom: 1px solid var(--border);
  }
  .mk-doc-card :global(td) {
    padding: 6px 12px;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
    vertical-align: top;
    line-height: 1.55;
  }
  .mk-doc-card :global(tbody tr:last-child td) { border-bottom: none; }
  .mk-doc-card :global(a) { color: var(--accent); }

  /* Screenshots */
  .mk-screenshots { display: flex; gap: 8px; }
  .mk-shot-placeholder {
    flex: 1;
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 18px;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    text-align: center;
  }
  .mk-shot-placeholder small { color: var(--text-disabled); font-size: 10.5px; }

  /* Theme detail header — coloured strip */
  .mk-theme-head {
    position: relative;
    border-bottom: 1px solid var(--border-subtle);
  }
  .mk-theme-headtext { z-index: 1; }
  .mk-theme-preview-lg {
    width: 100px;
    height: 80px;
    border-radius: var(--radius-md);
    border: 2px solid;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 10px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
  }
  .mk-theme-letter-lg {
    font-size: 24px;
    font-weight: 700;
    font-family: var(--font-mono);
    line-height: 1;
  }
  .mk-theme-swatches-lg { display: flex; gap: 4px; }
  .mk-theme-swatches-lg .sw {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.25);
  }
  /* ── Theme live preview — slice-of-Arbor mock UI ──────────────────────
     A small but information-dense replica of Arbor's chrome (title bar +
     branches sidebar + graph + diff + status bar) rendered with the
     theme's 6 preview colours via the `--pv-*` custom properties set
     inline on `.mk-theme-mock-frame`. Helper tints are derived with
     `color-mix` from `--pv-bg`/`--pv-fg` so we get readable elevated
     surfaces and muted text without needing extra catalog fields. */
  .mk-theme-mock-frame {
    /* Derived helpers — kept inside the scope so themes don't leak. */
    --pv-elevated: color-mix(in srgb, var(--pv-fg)  6%, var(--pv-bg));
    --pv-overlay:  color-mix(in srgb, var(--pv-fg) 10%, var(--pv-bg));
    --pv-border:   color-mix(in srgb, var(--pv-fg) 18%, transparent);
    --pv-muted:    color-mix(in srgb, var(--pv-fg) 60%, transparent);
    --pv-faint:    color-mix(in srgb, var(--pv-fg) 42%, transparent);

    background: var(--pv-bg);
    color: var(--pv-fg);
    border: 1px solid var(--pv-border);
    border-radius: var(--radius-md);
    overflow: hidden;
    font-family: var(--font-ui-sans);
    font-size: 11px;
    line-height: 1.4;
    user-select: none;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.22);
  }

  /* Title bar */
  .mk-pv-titlebar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: var(--pv-elevated);
    border-bottom: 1px solid var(--pv-border);
  }
  .mk-pv-traffic {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .mk-pv-dot-error { background: var(--pv-error);   }
  .mk-pv-dot-warn  { background: var(--pv-warning); }
  .mk-pv-dot-ok    { background: var(--pv-success); }
  .mk-pv-title {
    margin-left: 6px;
    color: var(--pv-muted);
    font-size: 10.5px;
  }
  .mk-pv-strong { color: var(--pv-fg); font-weight: 600; }

  /* Body — sidebar + main */
  .mk-pv-body {
    display: grid;
    grid-template-columns: 140px 1fr;
    min-height: 200px;
  }

  /* Sidebar */
  .mk-pv-sidebar {
    background: var(--pv-elevated);
    border-right: 1px solid var(--pv-border);
    padding: 8px 6px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .mk-pv-sect-h {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--pv-faint);
    padding: 4px 6px 2px;
  }
  .mk-pv-sect-h.mk-pv-spaced { margin-top: 8px; }
  .mk-pv-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 4px;
    color: var(--pv-fg);
    font-size: 10.5px;
  }
  .mk-pv-item.mk-pv-active {
    background: color-mix(in srgb, var(--pv-accent) 18%, transparent);
    color: var(--pv-accent);
  }
  .mk-pv-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .mk-pv-dot-accent { background: var(--pv-accent); }
  .mk-pv-dot-muted  { background: var(--pv-muted);  }
  .mk-pv-tag-glyph {
    color: var(--pv-warning);
    width: 8px;
    text-align: center;
    font-size: 9px;
  }

  /* Main column — graph + diff */
  .mk-pv-main {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  /* Graph */
  .mk-pv-graph {
    background: var(--pv-overlay);
    border: 1px solid var(--pv-border);
    border-radius: 4px;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .mk-pv-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 10.5px;
    min-width: 0;
  }
  .mk-pv-node {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    flex-shrink: 0;
    box-shadow: inset 0 0 0 1.5px var(--pv-bg);
  }
  .mk-pv-node-accent    { background: var(--pv-accent); }
  .mk-pv-node-secondary { background: var(--pv-muted);  }
  .mk-pv-msg {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mk-pv-meta {
    color: var(--pv-faint);
    font-family: var(--font-mono);
    font-size: 9.5px;
    flex-shrink: 0;
  }

  /* Diff */
  .mk-pv-diff {
    background: var(--pv-overlay);
    border: 1px solid var(--pv-border);
    border-radius: 4px;
    padding: 6px 8px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .mk-pv-diff-h   { color: var(--pv-accent);  font-weight: 600; margin-bottom: 2px; }
  .mk-pv-diff-add { color: var(--pv-success); }
  .mk-pv-diff-rem { color: var(--pv-error);   }
  .mk-pv-diff-ctx { color: var(--pv-muted);   }

  /* Status bar */
  .mk-pv-statusbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 10px;
    background: var(--pv-elevated);
    border-top: 1px solid var(--pv-border);
    font-size: 10px;
  }
  .mk-pv-status-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--pv-fg);
  }
  .mk-pv-status-chip.mk-pv-success { color: var(--pv-success); }
  .mk-pv-status-chip.mk-pv-warning { color: var(--pv-warning); }
  .mk-pv-status-chip.mk-pv-error   { color: var(--pv-error);   }
  .mk-pv-status-chip.mk-pv-muted   { color: var(--pv-accent);  }
  .mk-pv-status-spacer { margin-left: auto; }

  /* Palette swatch row — labelled chips for each preview colour. */
  .mk-pv-swatch-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 6px;
  }
  .mk-pv-swatch {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .mk-pv-swatch-chip {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    /* Theme-agnostic outline — works on light + dark host chrome. */
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--text-primary) 14%, transparent);
    flex-shrink: 0;
  }
  .mk-pv-swatch-label {
    font-size: 11px;
    color: var(--text-secondary);
    flex: 1;
  }
  .mk-pv-swatch-hex {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-muted);
  }

  /* ── Empty list ───────────────────────────────────────────────────── */
  .mk-empty {
    text-align: center;
    padding: 30px 12px;
    color: var(--text-muted);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .mk-empty p { margin: 0; font-size: var(--font-size-sm); }

  /* ── Custom-source modal form ─────────────────────────────────────── */
  .mk-form {
    padding: 16px 20px 8px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .mk-form-hint {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    line-height: 1.45;
  }
  .mk-form-hint code {
    font-family: var(--font-mono);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    font-size: 11px;
  }
  .mk-form-row { display: flex; flex-direction: column; gap: 4px; }
  .mk-form-row > span {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-muted);
    font-weight: 600;
  }
  .mk-form-row > span small {
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-disabled);
    font-weight: 400;
    margin-left: 4px;
  }
  .mk-form-row > span .required { color: var(--error); margin-left: 2px; }
  /* The inner field is the shared `Input` widget — its own styles handle
     the box / focus state. Nothing else needed at this level. */
  .mk-form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  /* Spinner styles moved to the shared `<Spinner>` widget — no local
     `.spinning` / @keyframes spin rules needed here anymore. */
</style>
