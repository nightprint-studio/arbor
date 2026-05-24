<script lang="ts">
  import { onMount } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { RefreshCw, ChevronDown, ChevronRight, Package, Power, Globe, HardDrive, GitBranch, Zap, TerminalSquare, Settings, Info, Trash, AlertTriangle, Network, FolderOpen, Wand2, Store } from 'lucide-svelte';
  import PluginDepGraphModal           from './manager/PluginDepGraphModal.svelte';
  import PluginDisableConfirmModal     from './manager/PluginDisableConfirmModal.svelte';
  import PluginEnableConfirmModal      from './manager/PluginEnableConfirmModal.svelte';
  import PluginUninstallConfirmModal   from './manager/PluginUninstallConfirmModal.svelte';
  import PluginExportTemplateModal     from './manager/PluginExportTemplateModal.svelte';
  import PluginInfoModal               from './manager/PluginInfoModal.svelte';
  import type { PluginInfo } from '$lib/types/plugin';
  import {
    reloadPlugins, listPluginInfo, pluginDependents, deletePlugin,
    pluginEnablePreview, pluginDisablePreview,
    getPluginsEnabled, setPluginsEnabled,
    type EnableBlocker,
  } from '$lib/ipc/plugin';
  import { listMarketplaceInstalledNames } from '$lib/ipc/marketplace';
  import { tooltip } from '$lib/actions/tooltip';
  import { invoke } from '@tauri-apps/api/core';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { containerStore } from '$lib/stores/container.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { setupTauriListeners } from '$lib/utils/tauri-listeners';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import ExperimentalBadge from '$lib/components/shared/ui/ExperimentalBadge.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let pluginInfos = $state<PluginInfo[]>([]);
  let loading    = $state(false);
  let reloading  = $state(false);
  let selected   = $state<string | null>(null);
  /** Set of plugin names installed through the marketplace â€” used to paint
   *  the "Marketplace" badge next to those rows so dev / hand-copied plugins
   *  remain visually distinguishable. */
  let marketplaceNames = $state<Set<string>>(new Set());

  /** Master kill-switch â€” when false the runtime is empty and we render
   *  neither the list nor any per-plugin state.  Loaded once on mount and
   *  flipped via the toggle at the top of the body. */
  let systemEnabled = $state(false);
  /** True while we don't know the persisted value yet (first paint). */
  let systemLoading = $state(true);
  /** True while a toggle round-trip is in flight. */
  let systemBusy    = $state(false);

  onMount(() => {
    // Resolve the kill-switch state asynchronously and only load the list
    // once we know the system is on. The listener is attached synchronously
    // so onMount can return its cleanup directly.
    (async () => {
      try {
        systemEnabled = await getPluginsEnabled();
      } catch (err) {
        uiStore.showToast(`${err}`, 'error');
      } finally {
        systemLoading = false;
      }
      if (systemEnabled) loadPlugins();
    })();

    return setupTauriListeners([{
      event: 'arbor://plugins-reloaded',
      handler: () => { if (systemEnabled) loadPlugins(); },
    }]);
  });

  async function toggleSystem(next: boolean) {
    if (systemBusy) return;
    systemBusy = true;
    try {
      await setPluginsEnabled(next);
      systemEnabled = next;
      if (next) {
        // Backend just reloaded everything from disk â€” pull the fresh list.
        await loadPlugins();
      } else {
        // Pretend no plugins exist: empty local + shared caches and drop
        // any selected/expanded row so the body re-renders cleanly when the
        // toggle flips back on. Also close any plugin-owned sidebar / panel
        // that was open â€” its contributions just disappeared.
        pluginInfos = [];
        pluginStore.syncFromInfos([]);
        uiStore.closePluginSections();
        selected = null;
        pendingDisable = null;
        pendingEnable = null;
        pendingUninstall = null;
        infoOpenFor = null;
        uiStore.showToast('Plugin system disabled', 'success');
      }
    } catch (err) {
      // Roll back the visible toggle position on failure so it matches the
      // backend state.
      systemEnabled = !next;
      uiStore.showToast(`${err}`, 'error');
    } finally {
      systemBusy = false;
    }
  }

  // Reveal the plugins directory in the OS file manager. The backend
  // command ensures the directory exists first (since a fresh install
  // has none), so opening never fails with "not found".
  async function openPluginsDirectory() {
    try {
      const dir = await invoke<string>('get_plugin_directory');
      await openPath(dir);
    } catch (err) {
      uiStore.showToast(`Impossibile aprire la cartella plugins: ${err}`, 'error');
    }
  }

  async function loadPlugins() {
    loading = true;
    try {
      // Sort alphabetically by name so the panel matches the ActivityBar
      // ordering (items from the same plugin are grouped by name there).
      pluginInfos = (await listPluginInfo())
        .slice()
        .sort((a, b) => a.name.localeCompare(b.name));
      pluginStore.syncFromInfos(pluginInfos);
      // Refresh the marketplace-name set so newly installed/uninstalled
      // marketplace plugins get/lose their badge without a full panel reload.
      try {
        marketplaceNames = new Set(await listMarketplaceInstalledNames());
      } catch { /* badge is decorative; ignore */ }
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      loading = false;
    }
  }

  async function reload() {
    if (reloading) return;
    reloading = true;
    try {
      // Backend reloads all plugins from disk, starts schedulers, then emits
      // "arbor://plugins-reloaded". That event triggers:
      //   â€¢ contributionStore.reloadAll() â†’ refreshes the unified registry
      //     so ActivityBar / Sidebar / Command Palette / Keybindings / etc.
      //     all see the new plugin shapes.
      //   â€¢ loadPlugins() â†’ updates this panel's list (via onMount listener above)
      await reloadPlugins();
      uiStore.showToast('Plugins reloaded', 'success');
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      reloading = false;
    }
  }

  // â”€â”€ Disable / enable cascade confirmations â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

  /** When non-null, PluginDisableConfirmModal is shown with the cascade plan
   *  (leaves-first, target last) returned by `plugin_disable_preview`. */
  let pendingDisable = $state<{ plugin: string; cascade: string[] } | null>(null);
  /** When non-null, PluginEnableConfirmModal is shown. `cascade` is the plan
   *  (deps first, target last); `blockers` is non-empty when required deps
   *  are missing/unloadable, in which case the modal switches to its
   *  "cannot enable" variant. */
  let pendingEnable  = $state<{ plugin: string; cascade: string[]; blockers: EnableBlocker[] } | null>(null);
  /** True while the Plugin Dependency Graph modal is open. */
  let depGraphOpen   = $state(false);
  /** True while the Export Template modal is open. */
  let exportTemplateOpen = $state(false);

  async function togglePlugin(name: string) {
    const info = pluginInfos.find(p => p.name === name);
    if (!info) return;

    if (info.enabled) {
      // Disable flow â€” preview the cascade so the modal can list every
      // dependent that will be turned off alongside the explicit click.
      try {
        const cascade = await pluginDisablePreview(name);
        // cascade always ends with `name`. Anything before it is a transitively-
        // required dependent â€” surface those in a confirm modal.
        if (cascade.length > 1) {
          pendingDisable = { plugin: name, cascade };
          return;
        }
      } catch { /* fall through to direct toggle on error */ }
    } else {
      // Enable flow â€” preview blockers AND cascade. Open the modal when
      // either is non-empty so the user knows what's happening.
      try {
        const preview = await pluginEnablePreview(name);
        if (preview.blockers.length > 0 || preview.plan.length > 1) {
          pendingEnable = {
            plugin:   name,
            cascade:  preview.plan,
            blockers: preview.blockers,
          };
          return;
        }
      } catch { /* fall through to direct toggle on error */ }
    }
    await performToggle(name);
  }

  async function performToggle(name: string) {
    try {
      const wasEnabled = pluginInfos.find(p => p.name === name)?.enabled ?? false;
      try {
        await pluginStore.togglePlugin(name);
      } catch (err) {
        // Toggle failed (e.g. enable blocker the user dismissed). The store
        // already rolled back its optimistic state â€” surface a toast and
        // bail before the list refresh below repaints with stale info.
        uiStore.showToast(`${err instanceof Error ? err.message : err}`, 'error');
        return;
      }
      pluginInfos = (await listPluginInfo())
        .slice()
        .sort((a, b) => a.name.localeCompare(b.name));
      pluginStore.syncFromInfos(pluginInfos);
      // Refresh the marketplace-name set so newly installed/uninstalled
      // marketplace plugins get/lose their badge without a full panel reload.
      try {
        marketplaceNames = new Set(await listMarketplaceInstalledNames());
      } catch { /* badge is decorative; ignore */ }
      // If we just disabled the plugin, its sidebar / bottom panel
      // contributions are gone â€” close any section it had open.
      if (wasEnabled) uiStore.closePluginSections(name);
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    }
  }

  // Scheduler start/stop is now per-action; the panel just shows counts.
  // Individual control is available through the plugin's settings form.

  // Per-plugin: registered container keys (`<plugin>::<id>`). A plugin "has
  // settings" iff it registered at least one container via either
  // `arbor.ui.container.register` or its sugar `arbor.ui.settings.panel`.
  function containerKeysFor(pluginName: string): string[] {
    return containerStore.defsForPlugin(pluginName).map(d => d.key);
  }

  function pluginHasSettings(plugin: PluginInfo): boolean {
    return containerKeysFor(plugin.name).length > 0;
  }

  function openPluginSettings(plugin: PluginInfo) {
    const keys = containerKeysFor(plugin.name);
    if (keys.length === 0) {
      uiStore.showToast(`${plugin.name} did not register a settings panel`, 'warning');
      return;
    }
    // First container wins â€” the gear is a single button. A multi-container
    // chooser (rare) would go into a dropdown next to the gear.
    containerStore.open(keys[0]);
  }

  // â”€â”€ Uninstall plugin (folder + cache + per-repo data) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  let pendingUninstall = $state<{ plugin: string; dependents: string[] } | null>(null);
  let uninstalling     = $state(false);

  async function requestUninstall(plugin: PluginInfo) {
    let deps: string[] = [];
    try {
      // Look up dependents so the modal can warn when other plugins rely on
      // this one. Failure here is non-fatal â€” we just open the modal without
      // a dependent list.
      deps = await pluginDependents(plugin.name);
    } catch { /* non-fatal */ }
    pendingUninstall = { plugin: plugin.name, dependents: deps };
  }

  async function confirmUninstall() {
    if (!pendingUninstall || uninstalling) return;
    const name = pendingUninstall.plugin;
    uninstalling = true;
    try {
      const warnings = await deletePlugin(name);
      // The plugin's contributions are gone â€” close any sidebar / panel it
      // owned so the user isn't staring at a now-empty section. Done before
      // refreshing the list so the UI doesn't flash through an inconsistent
      // state where the section is still active.
      uiStore.closePluginSections(name);
      if (warnings.length > 0) {
        uiStore.showToast(
          `Uninstalled "${name}" with ${warnings.length} warning(s) â€” see logs`,
          'warning',
        );
        for (const w of warnings) console.warn('[plugin uninstall]', w);
      } else {
        uiStore.showToast(`Uninstalled "${name}"`, 'success');
      }
      // Backend already emitted `arbor://plugins-reloaded`, but reload here
      // too so the list refreshes immediately even if the listener is racy.
      await loadPlugins();
      if (selected === name) selected = null;
    } catch (err) {
      uiStore.showToast(`Uninstall failed: ${err}`, 'error');
    } finally {
      uninstalling = false;
      pendingUninstall = null;
    }
  }

  /** When non-null, the PluginInfoModal is open for this plugin name. */
  let infoOpenFor = $state<string | null>(null);

  function toggleSelected(name: string) {
    selected = selected === name ? null : name;
  }

  /** Return the active hooks for a plugin (now simple booleans). */
  function activeHooks(p: PluginInfo): string[] {
    const h = p.hooks;
    const out: string[] = [];
    if (h.on_repo_open)   out.push('on_repo_open');
    if (h.on_repo_close)  out.push('on_repo_close');
    if (h.on_plugin_load) out.push('on_plugin_load');
    if (h.on_commit)      out.push('on_commit');
    if (h.on_push)        out.push('on_push');
    if (h.on_checkout)    out.push('on_checkout');
    if (h.on_fetch)       out.push('on_fetch');
    if (h.on_tab_switch)  out.push('on_tab_switch');
    return out;
  }

  function fsClass(fs: string, unrestricted: boolean): string {
    if (fs === 'none') return 'safe';
    if (unrestricted)  return 'danger';
    return 'warn';
  }
  function fsScopeIsUnrestricted(scope: string[] | undefined): boolean {
    return Array.isArray(scope) && scope.some(s => s === '*');
  }
  function fsLabel(fs: string, scope: string[] | undefined): string {
    if (fs === 'none') return 'no fs';
    return fsScopeIsUnrestricted(scope) ? `fs:${fs} (unrestricted)` : `fs:${fs}`;
  }
</script>

<Modal {onClose} width="780px" height="560px" padBody={false} ariaLabel="Plugin Manager">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Package size={14} />
      <span class="modal-title">Plugin Manager</span>
      {#if pluginInfos.length > 0}
        <span class="ps-count">{pluginInfos.length}</span>
      {/if}
      {#snippet actions()}
        <button class="ps-btn ps-btn-marketplace" use:tooltip={'Browse marketplace â€” discover plugins & themes'} disabled={!systemEnabled} onclick={() => uiStore.openMarketplace()}>
          <Store size={13} />
          <span class="ps-btn-label">Browse</span>
        </button>
        <button class="ps-btn" use:tooltip={'Export plugin template (.zip)'} disabled={!systemEnabled} onclick={() => { exportTemplateOpen = true; }}>
          <Wand2 size={13} />
        </button>
        <span class="ps-sep" aria-hidden="true"></span>
        <button class="ps-btn" use:tooltip={'Show dependency graph'} disabled={!systemEnabled} onclick={() => { depGraphOpen = true; }}>
          <Network size={13} />
        </button>
        <button class="ps-btn" use:tooltip={'Reload plugins from disk'} disabled={reloading || !systemEnabled} onclick={reload}>
          <RefreshCw size={13} class={reloading ? 'spinning' : ''} />
        </button>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="plugin-list">
    <!-- Security advisory â€” plugins run sandboxed Lua but can still touch the
         filesystem, network, terminal and git history with the permissions
         their author declares.  Treat them like any other third-party code. -->
    <Alert variant="warning" title="Importa solo plugin di cui ti fidi">
      I plugin sono codice di terze parti che gira con i permessi dichiarati
      nel loro <code>plugin.toml</code> (filesystem, network, terminale,
      gitâ€¦). Verifica sempre la fonte e ispeziona <code>main.lua</code> +
      la sezione <code>[permissions]</code> prima di abilitare un plugin
      che non hai scritto tu.
    </Alert>

    <!-- Master kill-switch.  When off the runtime is empty: nothing is
         loaded at startup, no schedulers fire, no list is rendered, and any
         cached plugin state is wiped.  Default off so a fresh install runs
         without surprises. -->
    <div class="ps-master">
      <Toggle
        bind:checked={systemEnabled}
        size="md"
        disabled={systemLoading || systemBusy}
        label="Abilita gestione plugin"
        description={systemEnabled
          ? 'I plugin installati sono caricati ed eseguiti.'
          : 'Nessun plugin viene caricato. Lo stato in cache Ã¨ vuoto.'}
        onchange={toggleSystem}
      />
    </div>

    {#if systemLoading}
      <div class="msg"><RefreshCw size={20} class="spinning" /><span>Loadingâ€¦</span></div>
    {:else if !systemEnabled}
      <div class="msg empty-msg">
        <Power size={32} class="empty-icon" />
        <p>Plugin system disabled</p>
        <p class="hint">Attiva l'interruttore qui sopra per scoprire e caricare i plugin presenti in
          <button class="plugins-link" onclick={openPluginsDirectory} use:tooltip={'Apri la cartella plugins nel file manager'}>
            <FolderOpen size={11} />
            plugins/
          </button>.
        </p>
      </div>
    {:else if loading}
      <div class="msg"><RefreshCw size={20} class="spinning" /><span>Loadingâ€¦</span></div>
    {:else if pluginInfos.length === 0}
      <div class="msg empty-msg">
        <Package size={32} class="empty-icon" />
        <p>No plugins found</p>
        <p class="hint">Place plugin folders in
          <button class="plugins-link" onclick={openPluginsDirectory} use:tooltip={'Apri la cartella plugins nel file manager'}>
            <FolderOpen size={11} />
            plugins/
          </button>
          (clicca per aprirla).
        </p>
      </div>
    {:else}
        {#each pluginInfos as plugin (plugin.name)}
          {@const enabled      = plugin.enabled}
          {@const hasScheduler = plugin.scheduler_count > 0}
          {@const schedRunning = plugin.schedulers_running > 0}
          {@const hooks        = activeHooks(plugin)}
          {@const perms        = plugin.permissions}
          {@const depFailed    = !!plugin.dep_error}

          <div
            class="plugin-item"
            class:selected={selected === plugin.name}
            class:disabled={!enabled}
            class:dep-failed={depFailed}
          >
            {#if depFailed}
              <div class="dep-banner" role="alert">
                <AlertTriangle size={12} />
                <span><strong>Failed to load:</strong> {plugin.dep_error}</span>
              </div>
            {/if}
            <div class="plugin-item-header">
              <!-- â”€â”€ Main row â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ -->
              <button class="plugin-main" onclick={() => toggleSelected(plugin.name)}>
                <Monogram
                  name={plugin.name}
                  initials={plugin.name[0].toUpperCase()}
                  color="var(--accent-subtle)"
                  fg="var(--accent)"
                  size={42}
                  disabled={!enabled}
                  class="plugin-monogram"
                />

                <div class="plugin-info">
                  <div class="plugin-name-row">
                    <span class="plugin-name">{plugin.name}</span>
                    <span class="plugin-version">v{plugin.version}</span>
                    {#if plugin.experimental}
                      <ExperimentalBadge
                        size="sm"
                        description="This plugin is flagged experimental in its manifest â€” its settings, hooks or storage format may change between releases."
                      />
                    {/if}
                    {#if marketplaceNames.has(plugin.name)}
                      <span use:tooltip={'Installed via the marketplace'}>
                        <Badge variant="tone" tone="accent" size="sm">
                          {#snippet icon()}<Store size={9} />{/snippet}
                          Marketplace
                        </Badge>
                      </span>
                    {/if}
                    {#if hasScheduler}
                      <span
                        class="sched-badge"
                        class:running={schedRunning}
                        use:tooltip={schedRunning
                          ? `${plugin.schedulers_running}/${plugin.scheduler_count} scheduler(s) running`
                          : 'All schedulers stopped'}
                      >{schedRunning ? 'â—' : 'â—‹'}</span>
                    {/if}
                  </div>
                  <span class="plugin-desc truncate">{plugin.description}</span>
                </div>

                <span class="expand-icon">
                  {#if selected === plugin.name}
                    <ChevronDown size={12} />
                  {:else}
                    <ChevronRight size={12} />
                  {/if}
                </span>
              </button>

              <!-- â”€â”€ Action buttons â”€â”€ -->
              <div class="item-actions">
                {#if pluginHasSettings(plugin)}
                  <button
                    class="action-btn settings-btn"
                    use:tooltip={'Plugin settings'}
                    disabled={!enabled}
                    onclick={() => openPluginSettings(plugin)}
                  >
                    <Settings size={12} />
                  </button>
                {:else}
                  <!-- Invisible placeholder so the trash+power column stays
                       at a consistent x position across every plugin row,
                       regardless of whether this plugin exposes a settings
                       form. -->
                  <span class="action-btn action-placeholder" aria-hidden="true"></span>
                {/if}
                <!-- Plugin info â€” opens a detailed modal that also hosts the
                     destructive "Clear settings cache" action and per-schedule
                     enable/disable toggles. -->
                <button
                  class="action-btn info-btn"
                  use:tooltip={'Plugin info & maintenance'}
                  onclick={() => { infoOpenFor = plugin.name; }}
                >
                  <Info size={12} />
                </button>
                <!-- Uninstall plugin â€” removes folder + global data + per-repo data -->
                <button
                  class="action-btn uninstall-btn"
                  use:tooltip={'Uninstall plugin'}
                  disabled={uninstalling}
                  onclick={() => requestUninstall(plugin)}
                >
                  <Trash size={12} />
                </button>
                <button
                  class="action-btn toggle-btn"
                  class:enabled
                  use:tooltip={enabled ? 'Disable plugin' : 'Enable plugin'}
                  onclick={() => togglePlugin(plugin.name)}
                  aria-pressed={enabled}
                >
                  <Power size={12} />
                </button>
              </div>
            </div>

            <!-- â”€â”€ Expanded detail â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ -->
            {#if selected === plugin.name}
              <div class="plugin-detail"
                   transition:slide={{ duration: animStore.dPanel, easing: cubicOut }}>

                <!-- Basic info -->
                <div class="detail-grid">
                  <span class="detail-label">Author</span>
                  <span>{plugin.author}</span>

                  <span class="detail-label">Status</span>
                  <span class:text-success={enabled} class:text-muted={!enabled}>
                    {enabled ? 'Enabled' : 'Disabled'}
                  </span>

                  <span class="detail-label">Arbor API</span>
                  <span class="text-muted">v{plugin.arbor_api}</span>

                  {#if pluginHasSettings(plugin)}
                    <span class="detail-label">Settings</span>
                    <span class="text-muted">
                      {containerKeysFor(plugin.name).join(', ')}
                    </span>
                  {/if}

                  {#if plugin.license}
                    <span class="detail-label">License</span>
                    <span class="text-muted">{plugin.license}</span>
                  {/if}

                  {#if hasScheduler}
                    <span class="detail-label">Schedulers</span>
                    <span class:text-success={schedRunning} class:text-muted={!schedRunning}>
                      {plugin.schedulers_running}/{plugin.scheduler_count} running
                    </span>
                  {/if}

                  {#if plugin.dependencies && plugin.dependencies.length > 0}
                    <span class="detail-label">Depends on</span>
                    <span class="dep-chip-row">
                      {#each plugin.dependencies as d (d.name)}
                        <span class="dep-chip" class:optional={d.optional}>
                          {d.name}{d.version ? ` ${d.version}` : ''}{d.optional ? ' (optional)' : ''}
                        </span>
                      {/each}
                    </span>
                  {/if}

                  {#if plugin.required_by && plugin.required_by.length > 0}
                    <span class="detail-label">Required by</span>
                    <span class="dep-chip-row">
                      {#each plugin.required_by as n (n)}
                        <span class="dep-chip">{n}</span>
                      {/each}
                    </span>
                  {/if}
                </div>

                <!-- Permissions -->
                <div class="detail-section">
                  <Zap size={10} />
                  Permissions
                </div>
                <div class="perms-list">
                  <!-- Filesystem -->
                  <span class="perm-tag {fsClass(perms.fs, fsScopeIsUnrestricted(perms.fs_scope))}">
                    <HardDrive size={9} style="display:inline;vertical-align:-1px" />
                    {fsLabel(perms.fs, perms.fs_scope)}
                  </span>

                  <!-- Network -->
                  {#if perms.network.length === 0}
                    <span class="perm-tag safe"><Globe size={9} style="display:inline;vertical-align:-1px" /> no network</span>
                  {:else}
                    {#each perms.network as host}
                      <span class="perm-tag warn"><Globe size={9} style="display:inline;vertical-align:-1px" /> {host}</span>
                    {/each}
                  {/if}

                  <!-- Git read/write/history-rewrite -->
                  {#if perms.git && perms.git !== 'none'}
                    {@const isRewrite = perms.git === 'history_rewrite'}
                    {@const isWrite   = perms.git === 'write'}
                    {@const isRead    = perms.git === 'read'}
                    <span class="perm-tag" class:danger={isRewrite} class:warn={isWrite} class:safe={isRead}>
                      <GitBranch size={9} style="display:inline;vertical-align:-1px" />
                      git:{perms.git === 'history_rewrite' ? 'rewrite' : perms.git}
                    </span>
                  {:else}
                    <span class="perm-tag safe"><GitBranch size={9} style="display:inline;vertical-align:-1px" /> no git</span>
                  {/if}

                  <!-- Terminal access -->
                  {#if !perms.terminal || perms.terminal === 'none'}
                    <span class="perm-tag safe">
                      <TerminalSquare size={9} style="display:inline;vertical-align:-1px" /> no terminal
                    </span>
                  {:else if perms.terminal === 'any'}
                    <span class="perm-tag danger">
                      <TerminalSquare size={9} style="display:inline;vertical-align:-1px" /> terminal:any
                    </span>
                  {:else}
                    <span class="perm-tag warn" use:tooltip={`Allowed: ${perms.terminal_scope?.join(', ')}`}>
                      <TerminalSquare size={9} style="display:inline;vertical-align:-1px" />
                      terminal:{perms.terminal_scope?.join(', ') ?? '?'}
                    </span>
                  {/if}
                </div>

                <!-- Hooks -->
                {#if hooks.length > 0}
                  <div class="detail-section">
                    <Zap size={10} />
                    Hooks
                  </div>
                  <div class="hooks-tags">
                    {#each hooks as h}
                      <code class="hook-fn">{h}</code>
                    {/each}
                  </div>
                {/if}

                <!-- API quick reference -->
                <div class="detail-section">
                  <Zap size={10} />
                  Arbor Lua API
                </div>
                <div class="api-list">
                  <div class="api-row"><code class="api-fn">arbor.notify{`{`}message,level?{`}`}</code><span class="api-desc">Show notification</span></div>
                  <div class="api-row"><code class="api-fn">arbor.ui.show_form(cfg)</code><span class="api-desc">Display input form modal</span></div>
                  <div class="api-row"><code class="api-fn">arbor.ui.confirm(msg, cfg)</code><span class="api-desc">Confirmation dialog</span></div>
                  <div class="api-row"><code class="api-fn">arbor.log.info(msg)</code><span class="api-desc">Log message (debug/info/warn/error)</span></div>
                  <div class="api-row"><code class="api-fn">arbor.settings.global.get(k)</code><span class="api-desc">Global persisted setting</span></div>
                  <div class="api-row"><code class="api-fn">arbor.settings.project.get(k)</code><span class="api-desc">Per-repo persisted setting</span></div>
                  <div class="api-row"><code class="api-fn">arbor.json.encode(v)</code><span class="api-desc">Lua â†’ JSON string</span></div>
                  <div class="api-row"><code class="api-fn">arbor.job.spawn(cfg)</code><span class="api-desc">Run background process</span></div>
                  {#if perms.terminal && perms.terminal !== 'none'}
                    <div class="api-row">
                      <code class="api-fn">arbor.terminal.exec(cmd)</code>
                      <span class="api-desc">
                        Blocking shell command
                        {#if perms.terminal === 'commands'}
                          â€” allowed: {perms.terminal_scope?.join(', ')}
                        {/if}
                      </span>
                    </div>
                  {/if}
                  {#if perms.git && perms.git !== 'none'}
                    <div class="api-row"><code class="api-fn">arbor.repo.current()</code><span class="api-desc">Active repo path</span></div>
                    <div class="api-row"><code class="api-fn">arbor.repo.branch()</code><span class="api-desc">Current branch name</span></div>
                  {/if}
                  {#if perms.network.length > 0}
                    <div class="api-row"><code class="api-fn">arbor.http.get(url)</code><span class="api-desc">HTTP request</span></div>
                  {/if}
                  {#if perms.fs && perms.fs !== 'none'}
                    <div class="api-row"><code class="api-fn">arbor.fs.read(path)</code><span class="api-desc">Read file</span></div>
                    {#if perms.fs === 'write'}
                      <div class="api-row"><code class="api-fn">arbor.fs.write(path, data)</code><span class="api-desc">Write file</span></div>
                    {/if}
                  {/if}
                </div>

              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

  {#snippet footer()}
    <div class="panel-footer">
      <span>Plugins directory:
        <button class="plugins-link" onclick={openPluginsDirectory} title="Apri la cartella plugins nel file manager">
          <FolderOpen size={11} />
          plugins/
        </button>
        Â· Reload to pick up disk changes
      </span>
    </div>
  {/snippet}
</Modal>

{#if depGraphOpen}
  <PluginDepGraphModal onClose={() => { depGraphOpen = false; }} />
{/if}

{#if exportTemplateOpen}
  <PluginExportTemplateModal
    onClose={() => { exportTemplateOpen = false; }}
  />
{/if}

{#if pendingDisable}
  <PluginDisableConfirmModal
    pluginName={pendingDisable.plugin}
    cascade={pendingDisable.cascade}
    onConfirm={() => {
      const n = pendingDisable!.plugin;
      pendingDisable = null;
      performToggle(n);
    }}
    onCancel={() => { pendingDisable = null; }}
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
      performToggle(n);
    }}
    onCancel={() => { pendingEnable = null; }}
  />
{/if}

{#if pendingUninstall}
  <PluginUninstallConfirmModal
    pluginName={pendingUninstall.plugin}
    dependents={pendingUninstall.dependents}
    busy={uninstalling}
    onConfirm={confirmUninstall}
    onCancel={() => { if (!uninstalling) pendingUninstall = null; }}
  />
{/if}

{#if infoOpenFor}
  {@const target = pluginInfos.find(p => p.name === infoOpenFor)}
  <PluginInfoModal
    pluginName={infoOpenFor}
    onClose={() => { infoOpenFor = null; }}
    onOpenSettings={target && pluginHasSettings(target)
      ? () => openPluginSettings(target!)
      : undefined}
  />
{/if}

<style>
  /* Body / chrome / borders / scrolling all live on <Modal> now â€”
     this stylesheet only owns the inner list + per-item rendering. */

  .ps-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 14px;
    padding: 0 4px;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .ps-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    padding: 0;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ps-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .ps-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  /* Primary CTA â€” the marketplace is the main entry point for discovering and
     installing plugins, so it gets a labelled button instead of icon-only. */
  .ps-btn-marketplace {
    width: auto;
    padding: 0 8px;
    gap: 5px;
    background: var(--accent-subtle);
    color: var(--accent);
  }
  .ps-btn-marketplace:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    color: var(--accent);
  }
  .ps-btn-label {
    font-size: var(--font-size-xs);
    font-weight: 500;
  }

  /* Visual divider between import/export pair and the existing
     dep-graph + reload pair so the action cluster stays readable. */
  .ps-sep {
    display: inline-block;
    width: 1px;
    height: 14px;
    background: var(--border-subtle);
    margin: 0 2px;
  }

  .plugin-item-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-right: 1.3em;
  }

  :global(.spinning) { animation: spin 1s linear infinite; }

  /* â”€â”€ Body / List â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */
  /* Modal-body is the scrollable card; this just lays out the plugin rows. */
  .plugin-list   { scrollbar-gutter: stable; padding: 8px; display: flex; flex-direction: column; gap: 6px; }

  /* Inline code chips inside the security alert â€” pick up tinted bg from
     the alert variant via inheritance, but we want monospace + padding. */
  .plugin-list :global(.alert code) {
    font-family: var(--font-code);
    font-size: 11px;
    background: rgba(255,255,255,0.06);
    border-radius: var(--radius-sm);
    padding: 0 4px;
  }

  /* Master kill-switch row.  Sits between the security alert and the list;
     visually separated by a soft divider so it reads as a top-level control,
     not just another list item. */
  .ps-master {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    padding: 8px 10px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    border-radius: var(--radius-md);
  }

  .msg {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 40px 20px;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    text-align: center;
  }
  .empty-msg .hint { font-size: var(--font-size-xs); color: var(--text-disabled); }
  :global(.empty-icon) { color: var(--text-disabled); }

  /* â”€â”€ Plugin item â€” card style â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */
  .plugin-item {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    position: relative;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .plugin-item:hover          { border-color: var(--border); }
  .plugin-item.selected       { border-color: rgba(77,120,204,0.5); box-shadow: 0 0 0 1px rgba(77,120,204,0.2); }
  .plugin-item.disabled       { opacity: 0.5; }
  .plugin-item.dep-failed     {
    border-color: color-mix(in srgb, var(--error) 55%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--error) 18%, transparent);
    opacity: 1;
  }

  .dep-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    background: color-mix(in srgb, var(--error) 12%, transparent);
    color: var(--error);
    font-size: 11px;
    line-height: 1.4;
    border-bottom: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
    border-radius: var(--radius-md) var(--radius-md) 0 0;
  }
  .dep-banner strong { font-weight: 600; }

  /* Main clickable row. flex:1 + min-width:0 lets the description truncate
     with ellipsis when it's too long; the fixed-width .item-actions sibling
     reserves the right-side slot so the actions don't drift left/right. */
  .plugin-main {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    min-width: 0;
    padding: 10px 12px;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
    font-family: var(--font-ui-sans);
    color: var(--text-secondary);
    transition: background var(--transition-fast);
    border-radius: var(--radius-md) 0 0 var(--radius-md);
  }
  .plugin-main:hover { background: rgba(255,255,255,0.03); }

  /* The plugin tile uses the shared Monogram (with class="plugin-monogram"). */
  :global(.plugin-monogram) {
    border: 1px solid rgba(77,120,204,0.25);
  }

  .plugin-info       { flex: 1; display: flex; flex-direction: column; gap: 3px; overflow: hidden; }
  .plugin-name-row   { display: flex; align-items: center; gap: 6px; }
  .plugin-name       { font-size: var(--font-size-sm); color: var(--text-primary); font-weight: 500; }
  .plugin-version    { font-size: 10px; color: var(--text-disabled); background: rgba(255,255,255,0.06); padding: 0 5px; border-radius: 999px; border: 1px solid rgba(255,255,255,0.06); }
  .plugin-desc       { font-size: var(--font-size-xs); color: var(--text-muted); }
  .expand-icon       { color: var(--text-disabled); flex-shrink: 0; display: flex; }

  /* Scheduler dot badge */
  .sched-badge         { font-size: 10px; color: var(--text-disabled); line-height: 1; flex-shrink: 0; }
  .sched-badge.running { color: var(--success); }


  /* â”€â”€ Action buttons â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */
  /* Fixed-width, non-shrinking container so the three action slots
     (settings / clear / toggle) live at the same x position on every
     plugin row â€” regardless of description length or whether the
     plugin exposes a settings form. */
  .item-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    flex-shrink: 0;
    width: 124px;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: 1px solid var(--border);
    background: var(--bg-base);
    color: var(--text-disabled);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .action-btn:hover:not(:disabled) { background: var(--bg-overlay); color: var(--text-primary); border-color: var(--border); }
  .action-btn:disabled              { opacity: 0.35; cursor: not-allowed; }

  /* Occupies the settings-gear slot so the trash+power buttons keep
     the same x position on plugins without a settings form. */
  .action-placeholder {
    visibility: hidden;
    pointer-events: none;
    border-color: transparent;
    background: transparent;
  }

  /* Scheduler â–¶/â–  button */
  .sched-btn.running                        { color: var(--success); border-color: rgba(95,173,86,0.5); background: rgba(95,173,86,0.1); }
  .sched-btn.running:hover:not(:disabled)   { background: rgba(199,84,80,0.1); color: var(--error); border-color: rgba(199,84,80,0.5); }
  .sched-btn:not(.running):hover:not(:disabled) { color: var(--success); border-color: rgba(95,173,86,0.5); }

  /* Settings gear */
  .settings-btn:hover:not(:disabled) { color: var(--accent); border-color: rgba(77,120,204,0.5); background: var(--accent-subtle); }

  /* Info button â€” opens the rich detail modal */
  .info-btn:hover:not(:disabled) { color: var(--accent); border-color: rgba(77,120,204,0.5); background: var(--accent-subtle); }

  /* Uninstall â€” destructive, opens confirmation modal on click */
  .uninstall-btn:hover:not(:disabled) {
    color: var(--error);
    border-color: rgba(199,84,80,0.5);
    background: rgba(199,84,80,0.1);
  }

  /* Power button â€” soft state: on = subtle green tint, off = muted */
  .toggle-btn.enabled                       { color: var(--success); border-color: color-mix(in srgb, var(--success) 30%, transparent); background: color-mix(in srgb, var(--success) 8%, transparent); }
  .toggle-btn.enabled:hover:not(:disabled)  { background: rgba(199,84,80,0.09); color: var(--error); border-color: rgba(199,84,80,0.4); }

  /* â”€â”€ Expanded detail panel â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */
  .plugin-detail {
    padding: 10px 14px 14px;
    background: rgba(0,0,0,0.15);
    border-top: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* Basic key-value grid */
  .detail-grid {
    display: grid;
    grid-template-columns: 86px 1fr;
    gap: 4px 8px;
    font-size: var(--font-size-xs);
  }
  .detail-label { color: var(--text-muted); }

  /* Inline list of dep chips inside the detail grid. Wraps on overflow so
     long dependency chains don't push the layout. */
  .dep-chip-row { display: flex; flex-wrap: wrap; gap: 4px; }
  .dep-chip {
    font-size: 10px;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--bg-overlay);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
  }
  .dep-chip.optional {
    color: var(--text-muted);
    font-style: italic;
  }

  .inline-code {
    background: var(--bg-overlay);
    padding: 1px 4px;
    border-radius: var(--radius-sm);
    color: var(--accent);
    font-family: var(--font-code);
    font-size: 10px;
  }

  /* Section divider */
  .detail-section {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
    padding-top: 6px;
    border-top: 1px solid var(--border-subtle);
  }

  /* Permissions pill list */
  .perms-list { display: flex; gap: 5px; flex-wrap: wrap; }

  .perm-tag {
    font-size: 10px;
    padding: 2px 7px;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .perm-tag.safe   { color: var(--success); border-color: rgba(95,173,86,0.4);  background: rgba(95,173,86,0.08); }
  .perm-tag.warn   { color: var(--warning); border-color: rgba(226,163,53,0.4); background: rgba(226,163,53,0.08); }
  .perm-tag.danger { color: var(--error);   border-color: rgba(199,84,80,0.4);  background: rgba(199,84,80,0.08); }

  /* Hooks tags */
  .hooks-tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .hook-fn   { color: var(--accent); background: var(--bg-overlay); padding: 2px 7px; border-radius: 999px; font-family: var(--font-code); font-size: 10px; border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent); }

  /* API reference list */
  .api-list  { display: flex; flex-direction: column; gap: 3px; }
  .api-row   { display: flex; align-items: baseline; gap: 8px; font-size: var(--font-size-xs); }
  .api-fn    { color: var(--accent); background: var(--bg-overlay); padding: 1px 5px; border-radius: var(--radius-sm); font-family: var(--font-code); font-size: 10px; white-space: nowrap; flex-shrink: 0; }
  .api-desc  { color: var(--text-disabled); font-size: 10px; }

  .text-success { color: var(--success); }
  .text-muted   { color: var(--text-muted); }
  .truncate     { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* â”€â”€ Footer â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
     Padding + chrome come from Modal's `.modal-footer`; inner content only
     owns its own typography. */
  .panel-footer {
    flex: 1;
    font-size: 10px;
    color: var(--text-disabled);
  }
  /* Inline pill that behaves like the old <code>plugins/</code> but is a
     real button opening the folder in the OS file manager. */
  .plugins-link {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    background: var(--bg-overlay);
    padding: 0 5px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--accent);
    font-family: var(--font-code);
    font-size: inherit;
    cursor: pointer;
    vertical-align: baseline;
    line-height: 1.4;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .plugins-link:hover {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .plugins-link :global(svg) { color: var(--accent); opacity: 0.85; }
</style>
