<script lang="ts">
  /**
   * MarketplaceModal â€” browse plugins + themes from the `arbor-registry` repo
   * and from user-added custom git URLs.
   *
   * Phase 1 wiring: catalog data flows through Tauri commands (`marketplace_*`)
   * backed by an in-memory seeded registry. The fetcher, installer and
   * permission-confirm dialog land in later phases (see project plan).
   */
  import { onMount, untrack } from 'svelte';
  import { Package, Palette, Search, Filter as FilterIcon, Tag, Plus, Upload, ChevronDown, Store, RefreshCw } from 'lucide-svelte';
  import Modal           from '$lib/components/shared/Modal.svelte';
  import ModalHeader     from '$lib/components/shared/ModalHeader.svelte';
  import Tabs, { type TabItem }       from '$lib/components/shared/ui/Tabs.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Button          from '$lib/components/shared/ui/Button.svelte';
  import Alert           from '$lib/components/shared/ui/Alert.svelte';
  import Input           from '$lib/components/shared/ui/Input.svelte';
  import Select          from '$lib/components/shared/ui/Select.svelte';
  import Spinner         from '$lib/components/shared/ui/Spinner.svelte';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import MarketplaceInstallConfirm        from './marketplace/MarketplaceInstallConfirm.svelte';
  import MarketplaceAddCustomSourceModal  from './marketplace/MarketplaceAddCustomSourceModal.svelte';
  import MarketplacePluginRow             from './marketplace/MarketplacePluginRow.svelte';
  import MarketplaceThemeRow              from './marketplace/MarketplaceThemeRow.svelte';
  import MarketplaceDetailEmpty           from './marketplace/MarketplaceDetailEmpty.svelte';
  import MarketplacePluginDetail          from './marketplace/MarketplacePluginDetail.svelte';
  import MarketplaceThemeDetail           from './marketplace/MarketplaceThemeDetail.svelte';
  import ContextMenu from '$lib/components/shared/ContextMenu.svelte';
  import { openUrl, openPath } from '@tauri-apps/plugin-opener';
  import { tooltip }     from '$lib/actions/tooltip';
  import { uiStore }     from '$lib/stores/ui.svelte';
  import { importPluginZipFromPath, reloadPlugins, pluginEnablePreview, pluginDisablePreview, getInstalledPluginPath } from '$lib/ipc/plugin';
  import PluginUninstallConfirmModal from './manager/PluginUninstallConfirmModal.svelte';
  import PluginDisableConfirmModal   from './manager/PluginDisableConfirmModal.svelte';
  import PluginEnableConfirmModal    from './manager/PluginEnableConfirmModal.svelte';
  import type { EnableBlocker } from '$lib/ipc/plugin';
  import {
    listInstalled, fetchRegistry, refreshRegistry,
    installPlugin    as ipcInstallPlugin,
    uninstallPlugin  as ipcUninstallPlugin,
    setPluginEnabled as ipcSetPluginEnabled,
    installTheme     as ipcInstallTheme,
    uninstallTheme   as ipcUninstallTheme,
    removeCustomSource as ipcRemoveCustomSource,
    getMarketplaceRefreshHours,
    setMarketplaceRefreshHours,
  } from '$lib/ipc/marketplace';
  import {
    pluginCategories, pluginTags, themeTags,
  } from '$lib/marketplace/catalog-helpers';
  import {
    summarizeSelection,
    pluginCtxItems, themeCtxItems,
  } from '$lib/marketplace/ui-helpers';
  import { resolveInstallCascade } from '$lib/marketplace/install-cascade';
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

  // â”€â”€ State â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  // NOTE: variable name avoids `tab` deliberately. With `tab` we saw a
  // bizarre reactivity glitch where mutating it from inside an event
  // handler (the Tabs `onSelect` callback) failed to re-render the
  // `{#if tab === 'plugins'}` block until something else forced a tick.
  // Renaming to `activeTab` made the issue go away â€” almost certainly a
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
  /** Resolved dep cascade for the plugin pending install: plugins that exist
   *  in the catalog but aren't on disk yet, dep-first order. */
  let confirmPendingDeps = $state<MarketplacePlugin[]>([]);
  /** Required deps that aren't in the catalog at all â€” surfaced as a hard
   *  error in the confirm modal so the user knows to install them manually. */
  let confirmMissingDeps = $state<{ name: string; version: string }[]>([]);

  // â”€â”€ Registry load state â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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

  // â”€â”€ Derived: filter + sort â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  const allCategories = $derived(pluginCategories(plugins));
  const allPluginTags = $derived(pluginTags(plugins));
  const allThemeTags  = $derived(themeTags(themes));

  function matchesText(haystack: string[], q: string): boolean {
    if (!q) return true;
    const lo = q.toLowerCase();
    return haystack.some(h => h.toLowerCase().includes(lo));
  }
  // (passesPluginFilter / passesThemeFilter inlined inside `$derived.by`
  // below â€” see the comment there for the reactivity rationale.)

  // Use `$derived.by` so the filter property reads happen inside the
  // tracked closure. Calling helper functions with `filter` as an arg
  // works in Svelte 5 most of the time, but in this surface the
  // installation tab switch wasn't re-deriving consistently â€” the
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

  // â”€â”€ Mock actions â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  async function fakeDelay(ms = 700) {
    await new Promise(r => setTimeout(r, ms));
  }

  onMount(() => { void bootstrap(); });

  /**
   * Two-step load:
   *   1. `listInstalled` â€” fast, synchronous on the host. Populates the
   *      Installed tabs immediately so the modal never opens empty.
   *   2. `loadRegistry`  â€” potentially networked. Brings in the community
   *      catalog; failures here only suppress the available list and leave
   *      the installed slice untouched.
   */
  async function bootstrap() {
    try {
      const local = await listInstalled();
      plugins = local.plugins;
      themes  = local.themes;
    } catch (err) {
      // Failing here would mean the host is broken, not the network â€” surface
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
   * since opening the modal. Idempotent â€” safe to call repeatedly (Retry).
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
      // network response is gone â€” it was a Phase-1 leftover and would
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

  /** Entry point from the row / detail Install buttons â€” opens the
   *  permission confirmation modal first. The actual install runs through
   *  `runInstall` once the user confirms. The cascade walker is a pure
   *  function in `marketplace/install-cascade.ts`. */
  function installPlugin(p: MarketplacePlugin) {
    if (busyId) return;
    const { pending, missing } = resolveInstallCascade(p, plugins);
    confirmPendingDeps = pending;
    confirmMissingDeps = missing;
    confirmInstall = p;
  }

  /** Install a single plugin via the existing IPC + patch the catalog row.
   *  Throws on failure so the caller's cascade can abort cleanly. */
  async function installOne(p: MarketplacePlugin): Promise<void> {
    const updated = await ipcInstallPlugin(p.name);
    patchPlugin(updated);
  }

  async function runInstall(p: MarketplacePlugin) {
    const deps = confirmPendingDeps.slice();
    confirmInstall = null;
    confirmPendingDeps = [];
    confirmMissingDeps = [];
    if (busyId) return;
    busyId = p.name;
    try {
      // Install transitive required deps first (already topologically ordered
      // by `resolveInstallCascade`). A failure here aborts the target install
      // so we don't end up with a half-broken graph.
      for (const dep of deps) {
        await installOne(dep);
      }
      await installOne(p);
      const total = deps.length + 1;
      uiStore.showToast(
        total > 1
          ? `Installed "${p.name}" + ${deps.length} required ${deps.length === 1 ? 'dependency' : 'dependencies'} â€” enable from the detail pane when ready.`
          : `Installed "${p.name}" â€” enable it from the detail pane when ready.`,
        'success',
      );
      return;
    } catch (err) {
      uiStore.showToast(`Install failed: ${err}`, 'error');
    } finally {
      busyId = null;
    }
  }

  /** Entry point from the row / detail Uninstall buttons â€” opens the
   *  uninstall confirmation modal first (which lists dependents the
   *  backend will cascade-disable). The actual uninstall runs through
   *  `runUninstall` once the user confirms. */
  let pendingUninstall = $state<{ plugin: MarketplacePlugin; dependents: string[] } | null>(null);

  async function uninstallPlugin(p: MarketplacePlugin) {
    if (busyId) return;
    let deps: string[] = [];
    try {
      // Look up the FULL transitive cascade â€” the same set the backend will
      // disable when uninstalling. `pluginDisablePreview` ends with the
      // target itself; strip it so the modal lists only the dependents.
      // Failure here is non-fatal â€” we still open the modal so the user
      // can confirm even without a dependent list.
      const cascade = await pluginDisablePreview(p.name);
      deps = cascade.filter(n => n !== p.name);
    } catch { /* non-fatal */ }
    pendingUninstall = { plugin: p, dependents: deps };
  }

  async function runUninstall() {
    if (!pendingUninstall || busyId) return;
    const p = pendingUninstall.plugin;
    // `dependents` was resolved up front in `uninstallPlugin` â€” the backend
    // cascade-disables exactly that set. Patch them locally to avoid a full
    // registry re-fetch (visible lag in the sidebar otherwise).
    const cascaded = pendingUninstall.dependents.slice();
    pendingUninstall = null;
    busyId = p.name;
    try {
      const updated = await ipcUninstallPlugin(p.name);
      patchPlugin(updated);
      if (cascaded.length > 0) {
        const set = new Set(cascaded);
        plugins = plugins.map(x =>
          set.has(x.name) ? { ...x, enabled: false } : x,
        );
      }
      uiStore.showToast(`Uninstalled "${p.name}".`, 'success');
    } catch (err) {
      uiStore.showToast(`Uninstall failed: ${err}`, 'error');
    } finally {
      busyId = null;
    }
  }

  /** Flip the plugin's enable state from the detail pane â€” same effect as
   *  toggling the per-row switch in the Plugin Manager. The backend mirrors
   *  the change in its own state; Phase 3 will also drive the live plugin
   *  host via `pluginStore.togglePlugin`. */
  /** Confirm modals for cascade-aware toggles inside the marketplace â€” same
   *  UX as the Plugin Manager so users can't be surprised by a hidden
   *  cascade. Each holds the resolved plan + (for enable) blocker list. */
  let pendingDisable = $state<{ plugin: string; cascade: string[] } | null>(null);
  let pendingEnable  = $state<{ plugin: string; cascade: string[]; blockers: EnableBlocker[] } | null>(null);
  /** Tick incremented when a cascade-confirm modal is cancelled. The Toggle
   *  in the detail pane keys off this so its internal `bind:checked` resets
   *  to the (unchanged) `p.enabled` prop â€” without this, the visual switch
   *  stays in the just-clicked position even though the backend never ran. */
  let toggleResetTick = $state(0);

  async function togglePluginEnabled(p: MarketplacePlugin, next: boolean) {
    if (!p.installed) return;

    // Preview the cascade first so the user sees what the toggle will actually
    // affect before anything changes on disk. Both preview IPCs are host-local
    // (no network), so the lookup stays instant. A non-trivial cascade or any
    // enable blocker â†’ open the confirm modal; otherwise the toggle goes
    // straight to the backend.
    try {
      if (next) {
        const preview = await pluginEnablePreview(p.name);
        if (preview.blockers.length > 0 || preview.plan.length > 1) {
          pendingEnable = {
            plugin:   p.name,
            cascade:  preview.plan,
            blockers: preview.blockers,
          };
          return;
        }
      } else {
        const cascade = await pluginDisablePreview(p.name);
        if (cascade.length > 1) {
          pendingDisable = { plugin: p.name, cascade };
          return;
        }
      }
    } catch { /* fall through to direct toggle on preview failure */ }

    await runTogglePluginEnabled(p.name, next);
  }

  /** Carry out the toggle once the user has either confirmed the cascade or
   *  the cascade was trivial enough to skip the modal. Patches the affected
   *  rows locally â€” no registry re-fetch â€” so the sidebar stays smooth. */
  async function runTogglePluginEnabled(name: string, next: boolean) {
    let cascaded: string[] = [];
    try {
      if (next) {
        const preview = await pluginEnablePreview(name);
        cascaded = preview.plan.filter(n => n !== name);
      } else {
        const list = await pluginDisablePreview(name);
        cascaded = list.filter(n => n !== name);
      }
    } catch { /* non-fatal â€” we'll still update the target */ }

    try {
      const updated = await ipcSetPluginEnabled(name, next);
      patchPlugin(updated);
      if (cascaded.length > 0) {
        const set = new Set(cascaded);
        plugins = plugins.map(x =>
          set.has(x.name) ? { ...x, enabled: next } : x,
        );
      }
      uiStore.showToast(
        next ? `Enabled "${name}".` : `Disabled "${name}".`,
        'success',
      );
    } catch (err) {
      uiStore.showToast(`Could not toggle "${name}": ${err}`, 'error');
      // Re-sync from server only when the optimistic toggle was wrong â€” that's
      // the one path where a stale local state is worse than the brief flicker.
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
        `Theme "${t.name}" added â€” apply it from Settings â†’ Appearance.`,
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

  // â”€â”€ Filter chip toggling helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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

  // â”€â”€ Tab + dropdown bindings â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  const topTabs = $derived<TabItem[]>([
    { id: 'plugins', label: 'Plugins', icon: Package, badge: plugins.length },
    { id: 'themes',  label: 'Themes',  icon: Palette, badge: themes.length  },
  ]);
  const installTabs: TabItem[] = [
    { id: 'all',       label: 'All' },
    { id: 'available', label: 'Available' },
    { id: 'installed', label: 'Installed' },
  ];

  // Source filter â€” single-select. Only the marketplace-tracked origins
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

  // â”€â”€ Add custom source â€” resolved by the secondary modal; host only
  //    merges the result + picks the success-toast copy. â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  function onCustomSourceResolved(resolved: MarketplacePlugin[]) {
    // Merge resolved entries into the in-memory list â€” overwrite by name
    // when the source already had a placeholder.
    const byName = new Map(plugins.map(p => [p.name, p]));
    for (const r of resolved) byName.set(r.name, r);
    plugins = Array.from(byName.values());

    uiStore.showToast(
      resolved.length === 1
        ? `Custom source "${resolved[0].name}" added.`
        : `Custom source added â€” resolved ${resolved.length} plugins.`,
      'success',
    );
    addCustomOpen = false;
  }

  // â”€â”€ Auto-refresh interval â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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

  // â”€â”€ Remove custom source â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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

  // â”€â”€ Import ZIP â€” real flow (kept as the only entry point for sideload). â”€â”€
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

  // â”€â”€ Context menu â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  // Right-click on a row opens an actions menu mirroring the buttons in the
  // detail pane. Selecting an item also marks the row as selected so the
  // detail pane is in sync.
  type RowCtx =
    | { x: number; y: number; kind: 'plugin'; plugin: MarketplacePlugin }
    | { x: number; y: number; kind: 'theme';  theme:  MarketplaceTheme  };
  let rowCtx = $state<RowCtx | null>(null);

  function openPluginCtx(e: MouseEvent, p: MarketplacePlugin) {
    e.preventDefault();
    e.stopPropagation();
    selected = p.name;
    rowCtx = { x: e.clientX, y: e.clientY, kind: 'plugin', plugin: p };
  }
  function openThemeCtx(e: MouseEvent, t: MarketplaceTheme) {
    e.preventDefault();
    e.stopPropagation();
    selected = t.id;
    rowCtx = { x: e.clientX, y: e.clientY, kind: 'theme', theme: t };
  }

  /** Reveal the plugin's on-disk folder in Explorer/Finder. Path is resolved
   *  via a backend command because the folder name can differ from the
   *  manifest's `name` (zip imports preserve the archive root). */
  async function revealPluginFolder(p: MarketplacePlugin) {
    try {
      const dir = await getInstalledPluginPath(p.name);
      await openPath(dir);
    } catch (err) {
      uiStore.showToast(`Could not open plugin folder: ${err}`, 'error');
    }
  }

  async function handleRowCtxSelect(id: string) {
    const ctx = rowCtx;
    rowCtx = null;
    if (!ctx) return;
    if (ctx.kind === 'plugin') {
      const p = ctx.plugin;
      switch (id) {
        case 'install':
        case 'update':       installPlugin(p);                       return;
        case 'uninstall':    await uninstallPlugin(p);               return;
        case 'enable':       await togglePluginEnabled(p, true);     return;
        case 'disable':      await togglePluginEnabled(p, false);    return;
        case 'homepage':     if (p.homepage) openUrl(p.homepage).catch(() => {}); return;
        case 'repo':         openUrl(p.entry.repo).catch(() => {}); return;
        case 'explorer':     await revealPluginFolder(p);            return;
        case 'remove_source':await removeCustomSource(p);            return;
      }
    } else {
      const t = ctx.theme;
      switch (id) {
        case 'install_theme':   await installTheme(t);   return;
        case 'uninstall_theme': await uninstallTheme(t); return;
        case 'repo':            openUrl(t.entry.repo).catch(() => {}); return;
      }
    }
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

  <!-- â”€â”€ Body â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ -->
  <div class="mk-root">

    <!-- LEFT: filters + list -->
    <aside class="mk-left mk-card">
      <!-- Installation tabs â€” split installed / available into distinct slots -->
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
            ? 'Search plugins by name, tag, authorâ€¦'
            : 'Search themes by name or tagâ€¦'}
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
              searchPlaceholder="Filter categoriesâ€¦"
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
              searchPlaceholder="Filter tagsâ€¦"
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
              searchPlaceholder="Filter tagsâ€¦"
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

      <!-- Registry status banner â€” non-blocking. Only shown when the
           community catalog is loading or failed; the Installed list above
           keeps working regardless of network state. -->
      {#if registryState === 'loading'}
        <div class="mk-status-slot">
          <Alert variant="info" compact noIcon>
            <Spinner size="xs" label="Fetching marketplace catalogâ€¦" />
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

      <!-- LIST â€” single flat list, sorted alphabetically. Visibility of installed
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
              <MarketplacePluginRow
                plugin={p}
                selected={selected === p.name}
                onSelect={() => (selected = p.name)}
                onContextMenu={(e) => openPluginCtx(e, p)}
              />
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
              <MarketplaceThemeRow
                theme={t}
                selected={selected === t.id}
                onSelect={() => (selected = t.id)}
                onContextMenu={(e) => openThemeCtx(e, t)}
              />
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
        <button class="mk-link" use:tooltip={'Force re-fetch â€” bypasses the 1h cache'}
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
        <MarketplaceDetailEmpty tab={activeTab} />
      {:else if selectedEntry.kind === 'plugin'}
        <MarketplacePluginDetail
          plugin={selectedEntry.plugin}
          {busyId}
          {toggleResetTick}
          onInstall={installPlugin}
          onUninstall={uninstallPlugin}
          onToggle={togglePluginEnabled}
          onRemoveSource={removeCustomSource}
        />
      {:else}
        <MarketplaceThemeDetail
          theme={selectedEntry.theme}
          {busyId}
          onInstall={installTheme}
          onUninstall={uninstallTheme}
        />
      {/if}
    </section>
  </div>
</Modal>

{#if addCustomOpen}
  <MarketplaceAddCustomSourceModal
    onResolved={onCustomSourceResolved}
    onClose={() => { addCustomOpen = false; }}
  />
{/if}

{#if confirmInstall}
  <MarketplaceInstallConfirm
    plugin={confirmInstall}
    pendingDeps={confirmPendingDeps}
    missingDeps={confirmMissingDeps}
    onCancel={() => {
      confirmInstall = null;
      confirmPendingDeps = [];
      confirmMissingDeps = [];
    }}
    onConfirm={() => { const p = confirmInstall!; void runInstall(p); }}
  />
{/if}

{#if pendingUninstall}
  <PluginUninstallConfirmModal
    pluginName={pendingUninstall.plugin.name}
    dependents={pendingUninstall.dependents}
    busy={busyId === pendingUninstall.plugin.name}
    onConfirm={runUninstall}
    onCancel={() => { if (busyId === null) pendingUninstall = null; }}
  />
{/if}

{#if pendingDisable}
  <PluginDisableConfirmModal
    pluginName={pendingDisable.plugin}
    cascade={pendingDisable.cascade}
    onConfirm={() => {
      const n = pendingDisable!.plugin;
      pendingDisable = null;
      void runTogglePluginEnabled(n, false);
    }}
    onCancel={() => { pendingDisable = null; toggleResetTick++; }}
  />
{/if}

{#if pendingEnable}
  <PluginEnableConfirmModal
    pluginName={pendingEnable.plugin}
    cascade={pendingEnable.cascade}
    blockers={pendingEnable.blockers}
    onConfirm={() => {
      const n = pendingEnable!.plugin;
      pendingEnable = null;
      void runTogglePluginEnabled(n, true);
    }}
    onCancel={() => { pendingEnable = null; toggleResetTick++; }}
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

{#if rowCtx}
  <ContextMenu
    x={rowCtx.x}
    y={rowCtx.y}
    items={rowCtx.kind === 'plugin' ? pluginCtxItems(rowCtx.plugin) : themeCtxItems(rowCtx.theme)}
    onSelect={handleRowCtxSelect}
    onClose={() => (rowCtx = null)}
  />
{/if}

<style>
  /* â”€â”€ Layout â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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

  /* Top tab badge â€” flips foreground/background when its tab is active so
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

  /* â”€â”€ Search â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */
  /* Outer gutter â€” the inner field is rendered by the shared `Input`
     widget so the box / focus / clear chrome stays consistent app-wide. */
  .mk-search-wrap {
    padding: 8px 10px;
  }

  /* â”€â”€ Filters: compact toolbar with multi-select dropdowns + mini chip
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

  /* Auto-refresh dropdown wrapper â€” keeps the label + shared Select on the
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

  /* Slot around the registry-status Alert â€” pulls it into the same gutter
     as the list. The loading row is now the shared `<Spinner>` widget
     with a label prop, so no custom row CSS is needed. */
  .mk-status-slot { padding: 6px 10px 0; }

  /* â”€â”€ List â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */
  .mk-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 6px 12px;
  }
  /* â”€â”€ Left footer â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */
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

  /* â”€â”€ Empty list â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */
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

  /* Spinner styles moved to the shared `<Spinner>` widget â€” no local
     `.spinning` / @keyframes spin rules needed here anymore. */
</style>
