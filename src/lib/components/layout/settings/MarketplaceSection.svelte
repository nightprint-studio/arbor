<script lang="ts">
  /**
   * Marketplace settings — tuning knobs for the catalog auto-refresh
   * scheduler that runs in the Rust backend (`marketplace/scheduler.rs`).
   *
   * The scheduler exists because Arbor is designed to stay open for long
   * stretches: a one-shot fetch on modal open isn't enough. The user may
   * never open the modal but still want a fresh catalog when they do.
   * This section lets them tune cadence + grain or disable the scheduler
   * entirely.
   */
  import { onMount } from 'svelte';
  import { Store, RefreshCw, Info, Timer } from 'lucide-svelte';
  import Toggle        from '$lib/components/shared/ui/Toggle.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Select        from '$lib/components/shared/ui/Select.svelte';
  import FormRow       from '$lib/components/shared/ui/FormRow.svelte';
  import { uiStore }   from '$lib/stores/ui.svelte';
  import {
    getMarketplaceRefreshHours,
    setMarketplaceRefreshHours,
    refreshRegistry,
  } from '$lib/ipc/marketplace';
  import { invoke } from '@tauri-apps/api/core';

  // The poll-minutes get/set commands aren't part of the catalog helpers
  // so we call them inline. Keeps the IPC module focused on catalog ops.
  function getPollMinutes(): Promise<number> {
    return invoke<number>('marketplace_get_poll_minutes');
  }
  function setPollMinutes(minutes: number): Promise<void> {
    return invoke<void>('marketplace_set_poll_minutes', { minutes });
  }

  let enabled        = $state(true);
  let refreshHours   = $state(24);
  let pollMinutes    = $state(10);
  let loaded         = $state(false);
  let saving         = $state(false);
  let refreshingNow  = $state(false);

  onMount(async () => {
    try {
      const [hours, poll] = await Promise.all([
        getMarketplaceRefreshHours(),
        getPollMinutes(),
      ]);
      enabled      = hours !== null && hours > 0;
      refreshHours = hours ?? 24;
      pollMinutes  = poll;
    } catch (err) {
      uiStore.showToast(`Could not load marketplace settings: ${err}`, 'error');
    } finally {
      loaded = true;
    }
  });

  async function persistEnabled() {
    if (!loaded || saving) return;
    saving = true;
    try {
      await setMarketplaceRefreshHours(enabled ? refreshHours : null);
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      saving = false;
    }
  }

  async function persistRefreshHours() {
    if (!loaded || saving || !enabled) return;
    saving = true;
    try {
      await setMarketplaceRefreshHours(refreshHours);
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      saving = false;
    }
  }

  async function persistPollMinutes() {
    if (!loaded || saving) return;
    saving = true;
    try {
      await setPollMinutes(pollMinutes);
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      saving = false;
    }
  }

  async function refreshNow() {
    if (refreshingNow) return;
    refreshingNow = true;
    try {
      await refreshRegistry();
      uiStore.showToast('Marketplace catalog refreshed.', 'success');
    } catch (err) {
      uiStore.showToast(`Refresh failed: ${err}`, 'error');
    } finally {
      refreshingNow = false;
    }
  }
</script>

<div class="section-header">
  <h2>Marketplace</h2>
  <p>Auto-refresh scheduler for the plugin &amp; theme catalog.</p>
</div>

<!-- ── Enable scheduler ─────────────────────────────────────────────────── -->
<div class="card">
  <div class="card-section-title"><Store size={12} /> Background Scheduler</div>

  <FormRow
    label="Enable scheduler"
    description="Periodically refreshes the marketplace catalog so it's already fresh when you open the modal. When disabled, the catalog is only refreshed on demand via the Refresh button."
  >
    <Toggle
      bind:checked={enabled}
      onchange={persistEnabled}
      ariaLabel="Enable marketplace scheduler"
      disabled={!loaded}
    />
  </FormRow>

  {#if enabled}
    <FormRow
      label="Refresh interval"
      description="How often to fetch a fresh catalog from arbor-extensions. Lowering this hits GitHub more often; raising it lets the cached catalog age before the next pull."
    >
      <Select
        value={refreshHours}
        options={[
          { value: 1,   label: 'Every 1h'  },
          { value: 6,   label: 'Every 6h'  },
          { value: 24,  label: 'Every 24h (default)' },
          { value: 72,  label: 'Every 3d'  },
          { value: 168, label: 'Every 7d'  },
        ]}
        onchange={(v) => { refreshHours = Number(v); void persistRefreshHours(); }}
      />
    </FormRow>

    <FormRow
      label="Poll cadence"
      description="How often the scheduler wakes up to check whether a refresh is due (minutes, 1–60). 10min is the sweet spot — finer values waste cycles checking a 24h interval, larger values add latency to settings changes."
    >
      <NumberStepper
        bind:value={pollMinutes}
        min={1}
        max={60}
        step={1}
        ariaLabel="Polling cadence (minutes)"
        onchange={persistPollMinutes}
      />
      <span class="unit-label">min</span>
    </FormRow>
  {/if}
</div>

<!-- ── Refresh now ──────────────────────────────────────────────────────── -->
<div class="card">
  <div class="card-section-title"><RefreshCw size={12} /> Manual</div>

  <FormRow
    label="Refresh now"
    description="Bypass the cache and fetch the catalog immediately. Same as the Refresh button in the marketplace modal."
  >
    <button class="btn-ghost" onclick={refreshNow} disabled={refreshingNow}>
      {#if refreshingNow}
        <Timer size={12} /> Refreshing…
      {:else}
        <RefreshCw size={12} /> Refresh marketplace
      {/if}
    </button>
  </FormRow>
</div>

<!-- ── Info ─────────────────────────────────────────────────────────────── -->
<div class="info-box">
  <Info size={13} />
  <span>
    The scheduler is intentionally low-priority: it polls quietly, skips when the cache is
    fresh, and never blocks the UI. The full catalog refresh on
    <code>raw.githubusercontent.com</code> is at most a couple of hundred KB of JSON / TOML —
    even hourly refreshes are negligible bandwidth.
  </span>
</div>

<style>
  .unit-label {
    font-size: 11px;
    color: var(--text-muted);
    margin-left: 4px;
  }
</style>
