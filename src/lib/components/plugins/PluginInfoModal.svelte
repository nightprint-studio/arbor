<script lang="ts">
  /**
   * PluginInfoModal — detailed plugin info window.
   *
   * Opened from the (i) button on each plugin row in PluginPanel.  Shows the
   * full identity / permissions / hooks / scheduler picture for one plugin and
   * hosts maintenance actions:
   *   • Per-schedule enable/disable toggles + bulk Enable all / Disable all.
   *   • Clear settings cache (the destructive button used to live in the row).
   *   • Open plugin docs if the manifest declared `doc_file`.
   */
  import { onMount } from 'svelte';
  import { openUrl }  from '@tauri-apps/plugin-opener';
  import {
    Info, Globe, HardDrive, GitBranch, Zap, TerminalSquare, Settings,
    Trash2, Power, Clock, Eye, Play, Tag, FileText, ExternalLink, Repeat,
  } from 'lucide-svelte';
  import Modal       from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button      from '$lib/components/shared/ui/Button.svelte';
  import Toggle      from '$lib/components/shared/ui/Toggle.svelte';
  import Monogram    from '$lib/components/shared/ui/Monogram.svelte';
  import Alert       from '$lib/components/shared/ui/Alert.svelte';
  import {
    listPluginInfo, pluginSettingsSetAll,
    startPluginScheduler, stopPluginScheduler,
  } from '$lib/ipc/plugin';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { setupTauriListeners } from '$lib/utils/tauri-listeners';
  import type { PluginInfo, PluginScheduleStatus, ScheduleTrigger } from '$lib/types/plugin';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    pluginName,
    onClose,
    onOpenSettings,
  }: {
    pluginName:      string;
    onClose:         () => void;
    /** Optional handler for the "Open settings" shortcut — if the plugin
     *  registered any settings container the host wires this to its own
     *  containerStore.open() call. */
    onOpenSettings?: () => void;
  } = $props();

  let plugin = $state<PluginInfo | null>(null);
  let loading = $state(true);
  /** Per-action busy flag while a start/stop scheduler call is in-flight. */
  let schedBusy = $state<Record<string, boolean>>({});
  /** Two-step confirmation for the destructive Clear cache action. */
  let clearArmed = $state(false);
  let clearArmedTimer: ReturnType<typeof setTimeout> | null = null;
  let clearing   = $state(false);

  onMount(() => {
    refresh();
    return setupTauriListeners([{
      event:   'arbor://plugins-reloaded',
      handler: refresh,
    }]);
  });

  async function refresh() {
    loading = true;
    try {
      const all = await listPluginInfo();
      plugin = all.find(p => p.name === pluginName) ?? null;
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      loading = false;
    }
  }

  // ── Scheduler controls ────────────────────────────────────────────────────
  async function toggleSchedule(action: string, next: boolean) {
    if (schedBusy[action]) return;
    schedBusy[action] = true;
    try {
      if (next) await startPluginScheduler(pluginName, action);
      else      await stopPluginScheduler (pluginName, action);
      await refresh();
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      schedBusy[action] = false;
    }
  }

  async function setAllSchedules(next: boolean) {
    if (!plugin) return;
    // Run sequentially so backend mutex contention doesn't drop calls.
    for (const s of plugin.schedules) {
      if (s.running === next) continue;
      try {
        if (next) await startPluginScheduler(pluginName, s.action);
        else      await stopPluginScheduler (pluginName, s.action);
      } catch (err) {
        uiStore.showToast(`${err}`, 'error');
      }
    }
    await refresh();
  }

  function describeTrigger(t: ScheduleTrigger): string {
    switch (t.kind) {
      case 'fixed_rate':  return `every ${formatSecs(t.interval_sec)}`;
      case 'fixed_delay': return `${formatSecs(t.delay_sec)} delay`;
      case 'cron':        return `cron(${t.expr})`;
    }
  }

  function formatSecs(n: number): string {
    if (n < 60)    return `${n}s`;
    if (n < 3600)  return `${Math.round(n / 60)}m`;
    return `${(n / 3600).toFixed(n % 3600 === 0 ? 0 : 1)}h`;
  }

  // ── Clear settings cache ──────────────────────────────────────────────────
  function armClear() {
    if (clearArmed) {
      void doClear();
      return;
    }
    clearArmed = true;
    if (clearArmedTimer) clearTimeout(clearArmedTimer);
    clearArmedTimer = setTimeout(() => { clearArmed = false; }, 3000);
  }

  async function doClear() {
    if (!plugin || clearing) return;
    clearing   = true;
    clearArmed = false;
    if (clearArmedTimer) { clearTimeout(clearArmedTimer); clearArmedTimer = null; }
    try {
      await pluginSettingsSetAll(plugin.name, {});
      uiStore.showToast(`Settings cleared for ${plugin.name}`, 'success');
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      clearing = false;
    }
  }

  // ── Helpers (mirror PluginPanel rendering) ────────────────────────────────
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

  function activeHooks(p: PluginInfo): string[] {
    const out: string[] = [];
    if (p.hooks.on_repo_open)   out.push('on_repo_open');
    if (p.hooks.on_repo_close)  out.push('on_repo_close');
    if (p.hooks.on_plugin_load) out.push('on_plugin_load');
    if (p.hooks.on_commit)      out.push('on_commit');
    if (p.hooks.on_push)        out.push('on_push');
    if (p.hooks.on_checkout)    out.push('on_checkout');
    if (p.hooks.on_fetch)       out.push('on_fetch');
    if (p.hooks.on_tab_switch)  out.push('on_tab_switch');
    return out;
  }

  async function openRepo() {
    if (!plugin?.repository) return;
    try { await openUrl(plugin.repository); }
    catch (err) { uiStore.showToast(`${err}`, 'error'); }
  }
</script>

<Modal {onClose} width="620px" height="640px" padBody={false} ariaLabel="Plugin info">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Info size={14} />
      <span class="modal-title">Plugin info</span>
      {#if plugin}
        <span class="pi-version">v{plugin.version}</span>
      {/if}
    </ModalHeader>
  {/snippet}

  <div class="pi-body">
    {#if loading && !plugin}
      <div class="pi-msg">Loading…</div>
    {:else if !plugin}
      <div class="pi-msg">Plugin not found.</div>
    {:else}
      {@const hooks = activeHooks(plugin)}
      {@const perms = plugin.permissions}
      {@const allRunning = plugin.schedules.length > 0 && plugin.schedules.every(s => s.running)}
      {@const noneRunning = plugin.schedules.length > 0 && plugin.schedules.every(s => !s.running)}

      <!-- ── Identity card ──────────────────────────────────────────────── -->
      <div class="pi-identity">
        <Monogram
          name={plugin.name}
          initials={plugin.name[0].toUpperCase()}
          color="var(--accent-subtle)"
          fg="var(--accent)"
          size={56}
          disabled={!plugin.enabled}
        />
        <div class="pi-identity-text">
          <div class="pi-name-row">
            <span class="pi-name">{plugin.name}</span>
            <span class="pi-status" class:on={plugin.enabled}>
              <Power size={10} />
              {plugin.enabled ? 'Enabled' : 'Disabled'}
            </span>
          </div>
          <div class="pi-desc">{plugin.description}</div>
          <div class="pi-meta">
            <span><Tag size={10} /> v{plugin.version}</span>
            <span class="pi-dot">·</span>
            <span>by {plugin.author}</span>
            {#if plugin.license}
              <span class="pi-dot">·</span>
              <span><FileText size={10} /> {plugin.license}</span>
            {/if}
            <span class="pi-dot">·</span>
            <span>API v{plugin.arbor_api}</span>
          </div>
          {#if plugin.repository}
            <button class="pi-repo" onclick={openRepo} use:tooltip={'Open repository in browser'}>
              <ExternalLink size={10} />
              {plugin.repository}
            </button>
          {/if}
        </div>
      </div>

      {#if plugin.dep_error}
        <Alert variant="error" title="Failed to load">
          {plugin.dep_error}
        </Alert>
      {/if}

      {#if plugin.keywords && plugin.keywords.length > 0}
        <div class="pi-keywords">
          {#each plugin.keywords as k (k)}
            <span class="pi-kw">{k}</span>
          {/each}
        </div>
      {/if}

      <!-- ── Schedulers ─────────────────────────────────────────────────── -->
      <div class="pi-section">
        <div class="pi-section-head">
          <Clock size={11} />
          <span>Schedulers</span>
          <span class="pi-section-count">
            {plugin.schedulers_running}/{plugin.scheduler_count} running
          </span>
          {#if plugin.schedules.length > 0 && plugin.enabled}
            <div class="pi-section-actions">
              <Button
                variant="ghost" size="xs"
                disabled={allRunning}
                onclick={() => setAllSchedules(true)}
                title="Start every scheduler for this plugin"
              >Enable all</Button>
              <Button
                variant="ghost" size="xs"
                disabled={noneRunning}
                onclick={() => setAllSchedules(false)}
                title="Stop every scheduler for this plugin"
              >Disable all</Button>
            </div>
          {/if}
        </div>

        {#if plugin.schedules.length === 0}
          <div class="pi-empty">This plugin has no background schedulers.</div>
        {:else}
          <ul class="pi-sched-list">
            {#each plugin.schedules as s (s.action)}
              <li class="pi-sched" class:running={s.running}>
                <div class="pi-sched-info">
                  <code class="pi-sched-action">{s.action}</code>
                  <span class="pi-sched-trigger">
                    <Repeat size={9} />
                    {describeTrigger(s.trigger)}
                  </span>
                  <div class="pi-sched-flags">
                    {#if s.on_load}
                      <span class="pi-flag" use:tooltip={'Fires once on plugin load before the cadence kicks in'}><Play size={8} /> on load</span>
                    {/if}
                    {#if s.only_when_focused}
                      <span class="pi-flag" use:tooltip={'Skips firing when the app window is not focused'}><Eye size={8} /> focus-gated</span>
                    {/if}
                    {#if s.initial_delay_sec > 0}
                      <span class="pi-flag" use:tooltip={'Initial wait before the first fire'}>
                        <Clock size={8} /> wait {formatSecs(s.initial_delay_sec)}
                      </span>
                    {/if}
                  </div>
                </div>
                <Toggle
                  checked={s.running}
                  size="sm"
                  disabled={!plugin.enabled || schedBusy[s.action]}
                  ariaLabel={s.running ? `Disable ${s.action}` : `Enable ${s.action}`}
                  onchange={(v) => toggleSchedule(s.action, v)}
                />
              </li>
            {/each}
          </ul>
        {/if}
      </div>

      <!-- ── Permissions ───────────────────────────────────────────────── -->
      <div class="pi-section">
        <div class="pi-section-head">
          <Zap size={11} />
          <span>Permissions</span>
        </div>
        <div class="pi-perms">
          <span class="pi-perm {fsClass(perms.fs, fsScopeIsUnrestricted(perms.fs_scope))}">
            <HardDrive size={9} /> {fsLabel(perms.fs, perms.fs_scope)}
          </span>

          {#if perms.network.length === 0}
            <span class="pi-perm safe"><Globe size={9} /> no network</span>
          {:else}
            {#each perms.network as host (host)}
              <span class="pi-perm warn"><Globe size={9} /> {host}</span>
            {/each}
          {/if}

          {#if perms.git && perms.git !== 'none'}
            {@const isRewrite = perms.git === 'history_rewrite'}
            {@const isWrite   = perms.git === 'write'}
            {@const isRead    = perms.git === 'read'}
            <span class="pi-perm" class:danger={isRewrite} class:warn={isWrite} class:safe={isRead}>
              <GitBranch size={9} /> git:{perms.git === 'history_rewrite' ? 'rewrite' : perms.git}
            </span>
          {:else}
            <span class="pi-perm safe"><GitBranch size={9} /> no git</span>
          {/if}

          {#if !perms.terminal || perms.terminal === 'none'}
            <span class="pi-perm safe"><TerminalSquare size={9} /> no terminal</span>
          {:else if perms.terminal === 'any'}
            <span class="pi-perm danger"><TerminalSquare size={9} /> terminal:any</span>
          {:else}
            <span class="pi-perm warn" use:tooltip={`Allowed: ${perms.terminal_scope?.join(', ')}`}>
              <TerminalSquare size={9} />
              terminal:{perms.terminal_scope?.join(', ') ?? '?'}
            </span>
          {/if}
        </div>
      </div>

      <!-- ── Hooks ─────────────────────────────────────────────────────── -->
      {#if hooks.length > 0}
        <div class="pi-section">
          <div class="pi-section-head">
            <Zap size={11} />
            <span>Hooks</span>
          </div>
          <div class="pi-hooks">
            {#each hooks as h (h)}
              <code class="pi-hook">{h}</code>
            {/each}
          </div>
        </div>
      {/if}

      <!-- ── Maintenance ───────────────────────────────────────────────── -->
      <div class="pi-section">
        <div class="pi-section-head">
          <Settings size={11} />
          <span>Maintenance</span>
        </div>

        <div class="pi-row-action">
          <div class="pi-row-text">
            <div class="pi-row-title">Plugin settings</div>
            <div class="pi-row-desc">Open the settings panel exposed by this plugin (if any).</div>
          </div>
          <Button
            variant="secondary" size="sm"
            disabled={!onOpenSettings || !plugin.enabled}
            onclick={() => { onOpenSettings?.(); onClose(); }}
          >
            {#snippet iconStart()}<Settings size={11} />{/snippet}
            Open settings
          </Button>
        </div>

        <div class="pi-row-action">
          <div class="pi-row-text">
            <div class="pi-row-title">Clear settings cache</div>
            <div class="pi-row-desc">
              Remove every persisted setting written by this plugin (global +
              per-repo). The plugin's own data and folder are kept.
            </div>
          </div>
          <Button
            variant={clearArmed ? 'danger' : 'secondary'}
            size="sm"
            loading={clearing}
            disabled={clearing}
            onclick={armClear}
            title={clearArmed
              ? 'Click again to confirm — this clears every saved setting for this plugin'
              : 'Clear all persisted settings'}
          >
            {#snippet iconStart()}<Trash2 size={11} />{/snippet}
            {clearArmed ? 'Click to confirm' : 'Clear cache'}
          </Button>
        </div>
      </div>
    {/if}
  </div>
</Modal>

<style>
  .pi-body {
    padding: 14px 16px 16px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    scrollbar-gutter: stable;
  }

  .pi-msg {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
  }

  .pi-version {
    display: inline-flex;
    align-items: center;
    height: 14px;
    padding: 0 6px;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
  }

  /* ── Identity card ──────────────────────────────────────────────── */
  .pi-identity {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    padding: 12px 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .pi-identity-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .pi-name-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .pi-name {
    font-size: var(--font-size-md);
    color: var(--text-primary);
    font-weight: 600;
  }
  .pi-status {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    background: rgba(255,255,255,0.04);
    border: 1px solid var(--border-subtle);
  }
  .pi-status.on {
    color: var(--success);
    background: color-mix(in srgb, var(--success) 10%, transparent);
    border-color: color-mix(in srgb, var(--success) 35%, transparent);
  }
  .pi-desc {
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    line-height: 1.45;
  }
  .pi-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    font-size: 11px;
    color: var(--text-muted);
  }
  .pi-meta span { display: inline-flex; align-items: center; gap: 4px; }
  .pi-dot { color: var(--text-disabled); }

  .pi-repo {
    margin-top: 4px;
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 7px;
    background: var(--bg-overlay);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--accent);
    font-family: var(--font-code);
    font-size: 10px;
    cursor: pointer;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .pi-repo:hover {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }

  .pi-keywords { display: flex; flex-wrap: wrap; gap: 4px; }
  .pi-kw {
    font-size: 10px;
    padding: 1px 7px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--text-muted);
  }

  /* ── Sections ──────────────────────────────────────────────────── */
  .pi-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .pi-section-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
  }
  .pi-section-count {
    margin-left: 4px;
    font-size: 10px;
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-disabled);
    font-weight: 400;
  }
  .pi-section-actions {
    margin-left: auto;
    display: flex;
    gap: 4px;
  }

  .pi-empty {
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
    font-style: italic;
    padding: 4px 2px;
  }

  /* ── Schedulers ────────────────────────────────────────────────── */
  .pi-sched-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .pi-sched {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 10px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .pi-sched.running {
    border-color: color-mix(in srgb, var(--success) 35%, transparent);
    background: color-mix(in srgb, var(--success) 5%, transparent);
  }
  .pi-sched-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .pi-sched-action {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-primary);
    align-self: flex-start;
  }
  .pi-sched-trigger {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    color: var(--text-muted);
  }
  .pi-sched-flags { display: flex; flex-wrap: wrap; gap: 4px; }
  .pi-flag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 9px;
    padding: 1px 5px;
    border-radius: 999px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    color: var(--text-muted);
  }

  /* ── Permissions ─────────────────────────────────────────────── */
  .pi-perms { display: flex; gap: 5px; flex-wrap: wrap; }
  .pi-perm {
    font-size: 10px;
    padding: 2px 7px;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--text-secondary);
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .pi-perm.safe   { color: var(--success); border-color: rgba(95,173,86,0.4);  background: rgba(95,173,86,0.08); }
  .pi-perm.warn   { color: var(--warning); border-color: rgba(226,163,53,0.4); background: rgba(226,163,53,0.08); }
  .pi-perm.danger { color: var(--error);   border-color: rgba(199,84,80,0.4);  background: rgba(199,84,80,0.08); }

  /* ── Hooks ───────────────────────────────────────────────────── */
  .pi-hooks { display: flex; flex-wrap: wrap; gap: 4px; }
  .pi-hook {
    color: var(--accent);
    background: var(--bg-overlay);
    padding: 2px 7px;
    border-radius: 999px;
    font-family: var(--font-code);
    font-size: 10px;
    border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
  }

  /* ── Maintenance rows ────────────────────────────────────────── */
  .pi-row-action {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 10px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .pi-row-text { flex: 1; min-width: 0; }
  .pi-row-title { font-size: var(--font-size-sm); color: var(--text-primary); margin-bottom: 2px; }
  .pi-row-desc  { font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.45; }
</style>
