<script lang="ts">
  import { Database, RefreshCw, Info, Timer, Cloud } from 'lucide-svelte';
  import { cacheStore } from '$lib/stores/cache.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { repoBrowserStore } from '$lib/stores/repoBrowser.svelte';
  import type { CacheConfig } from '$lib/types/config';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';

  // Local copy of config — synced to store on each change.
  let cfg = $state<CacheConfig>({ ...cacheStore.config });
  let saved    = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  // Per-keystroke validation handlers were dropped in favour of the
  // NumberStepper widget: it clamps to its `min`/`max` props on +/- and
  // on commit, then fires `onchange` with the final value. We just
  // route every commit through `persist` — no field-specific validators
  // needed.
  async function persist() {
    await cacheStore.saveConfig({ ...cfg }, () => tabsStore.activeTabId);
    saved = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => { saved = false; }, 2000);
  }

  function clearRepoBrowserCache() {
    repoBrowserStore.clearRepoCache();
    saved = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => { saved = false; }, 2000);
  }

  async function clearAll() {
    await cacheStore.clearAll();
    saved = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => { saved = false; }, 2000);
  }

  const stats = $derived(cacheStore.stats());
</script>

<div class="section-header">
  <h2>Cache</h2>
  <p>Per-tab data cache and background refresh scheduler.</p>
</div>

<!-- ── Cache enable ──────────────────────────────────────────────────────── -->
<div class="card">
  <div class="card-section-title"><Database size={12} /> Data Cache</div>

  <FormRow label="Enable cache" description="Store graph and branch data per tab so switching tabs is instant">
    <Toggle bind:checked={cfg.enabled} onchange={persist} ariaLabel="Enable cache" />
  </FormRow>

  {#if cfg.enabled}
    <FormRow label="Max cached tabs" description="LRU eviction kicks in when this limit is exceeded (1–50)">
      <NumberStepper
        bind:value={cfg.max_tabs}
        min={1}
        max={50}
        ariaLabel="Max cached tabs"
        onchange={persist}
      />
    </FormRow>

    <FormRow label="Currently cached" description="Live tab and commit-detail snapshots in memory">
      <span class="stat-chip">{stats.cachedTabs} tab{stats.cachedTabs !== 1 ? 's' : ''}</span>
      <span class="stat-chip">{stats.cachedCommits} commit{stats.cachedCommits !== 1 ? 's' : ''}</span>
      <button class="btn-ghost" onclick={clearAll}>Clear all</button>
    </FormRow>
  {/if}
</div>

<!-- ── Close repo on evict ──────────────────────────────────────────────── -->
<div class="card">
  <div class="card-section-title"><Database size={12} /> Memory Management</div>

  <FormRow
    label="Free git handle on eviction"
    description="When a tab's cache is evicted, also drop the internal git2 repository handle to release libgit2 memory (pack indexes, ref cache). The repo is transparently re-opened on next access."
  >
    <Toggle bind:checked={cfg.close_repo_on_evict} onchange={persist} ariaLabel="Free git handle on eviction" />
  </FormRow>
</div>

<!-- ── Auto-refresh scheduler ───────────────────────────────────────────── -->
<div class="card">
  <div class="card-section-title"><RefreshCw size={12} /> Auto-Refresh Scheduler</div>

  <FormRow label="Enable scheduler" description="Periodically checks the active tab for remote changes (only while app is focused)">
    <Toggle bind:checked={cfg.scheduler_enabled} onchange={persist} ariaLabel="Enable scheduler" />
  </FormRow>

  {#if cfg.scheduler_enabled}
    <FormRow label="Check interval" description="How often to compare the repo fingerprint (seconds, min 5)">
      <NumberStepper
        bind:value={cfg.refresh_interval_secs}
        min={5}
        step={5}
        ariaLabel="Auto-refresh check interval (seconds)"
        onchange={persist}
      />
      <span class="unit-label">s</span>
    </FormRow>

    <div class="card-row-note">
      <span>
        The scheduler compares a lightweight fingerprint (HEAD SHA + all refs) — if nothing has
        changed the UI is <strong>not</strong> refreshed. Only a real push, merge, or new tag
        will trigger a reload.
      </span>
    </div>
  {/if}
</div>

<!-- ── Idle eviction ─────────────────────────────────────────────────────── -->
<div class="card">
  <div class="card-section-title"><Timer size={12} /> Idle Cache Eviction</div>

  <FormRow label="Enable auto-eviction" description="Automatically free cache for background tabs that haven't been used for a while">
    <Toggle bind:checked={cfg.auto_evict_enabled} onchange={persist} ariaLabel="Enable auto-eviction" />
  </FormRow>

  {#if cfg.auto_evict_enabled}
    <FormRow
      label="Minimum tabs to keep"
      description="Always keep the N most recently used tabs in cache, regardless of idle time. The active tab counts toward this total (1–20)"
    >
      <NumberStepper
        bind:value={cfg.min_cached_tabs}
        min={1}
        max={20}
        ariaLabel="Minimum cached tabs"
        onchange={persist}
      />
      <span class="unit-label">tab{cfg.min_cached_tabs !== 1 ? 's' : ''}</span>
    </FormRow>

    <FormRow label="Idle threshold" description="Seconds of inactivity before a tab's cache is cleared (min 30)">
      <NumberStepper
        bind:value={cfg.tab_idle_secs}
        min={30}
        step={30}
        ariaLabel="Idle threshold (seconds)"
        onchange={persist}
      />
      <span class="unit-label">s</span>
    </FormRow>

    <FormRow label="Check interval" description="How often the eviction scheduler runs (seconds, min 10)">
      <NumberStepper
        bind:value={cfg.evict_check_interval_secs}
        min={10}
        step={5}
        ariaLabel="Eviction check interval (seconds)"
        onchange={persist}
      />
      <span class="unit-label">s</span>
    </FormRow>

    <div class="card-row-note">
      <span>
        The <strong>active tab</strong> is never evicted. Cache is reclaimed only for background
        tabs idle longer than the threshold. The check runs even when the app is not focused.
      </span>
    </div>
  {/if}
</div>

<!-- ── Repository Browser cache ─────────────────────────────────────────── -->
<div class="card">
  <div class="card-section-title"><Cloud size={12} /> Repository Browser</div>

  <FormRow
    label="Cache TTL"
    description="How long to cache the GitHub / GitLab repo list before it's refetched. Listing 200+ projects can take 30s+; the cache makes reopening the modal instant. Stale entries within the TTL are still shown immediately while a refresh runs in the background. Set to 0 to disable caching."
  >
    <NumberStepper
      bind:value={cfg.repo_browser_ttl_secs}
      min={0}
      step={60}
      ariaLabel="Repository Browser cache TTL (seconds)"
      onchange={persist}
    />
    <span class="unit-label">s</span>
  </FormRow>

  <FormRow label="Cached repo lists" description="Drop the on-disk repo list cache for every connected provider">
    <button class="btn-ghost" onclick={clearRepoBrowserCache}>Clear repo browser cache</button>
  </FormRow>
</div>

<!-- ── Info ─────────────────────────────────────────────────────────────── -->
<div class="info-box">
  <Info size={13} />
  <span>
    Per-tab cached data is session-only and cleared when you close the app. The Repository
    Browser cache is persisted to localStorage. Tickets and issue tracker data are never cached.
    The last-refresh time appears in the status bar next to the branch name.
  </span>
</div>

{#if saved}
  <p class="saved-label">Saved</p>
{/if}

<style>
  .stat-chip {
    font-size: 11px;
    font-family: var(--font-code);
    color: var(--text-secondary);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 1px 8px;
  }

  .unit-label {
    font-size: 11px;
    color: var(--text-muted);
    margin-left: 4px;
  }
</style>
