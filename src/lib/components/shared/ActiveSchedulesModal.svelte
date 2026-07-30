<script lang="ts">
  /**
   * ActiveSchedulesModal — read-only inspector for the shared
   * `arbor-scheduler` engine. Lists every currently-registered schedule
   * (plugin actions, marketplace auto-refresh, future pipeline timers, …)
   * grouped by namespace, with trigger cadence, enabled state, and the
   * focus-gating / fire-on-load opts.
   *
   * Open path: Command Palette only, for now. No keybinding yet.
   */
  import { onMount } from 'svelte';
  import {
    Clock, RefreshCw, Eye, Power, Zap, Inbox,
    Plug, Store, Gauge, Hourglass, CalendarClock, GitBranch,
  } from 'lucide-svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { listSchedules, type ScheduleSnapshot, type Trigger } from '$lib/ipc/schedules';

  let { onClose }: { onClose: () => void } = $props();

  let snapshots = $state<ScheduleSnapshot[]>([]);
  let loading   = $state(false);
  let error     = $state<string | null>(null);

  async function refresh() {
    loading = true;
    error   = null;
    try {
      snapshots = await listSchedules();
    } catch (e) {
      error     = String(e);
      snapshots = [];
    } finally {
      loading = false;
    }
  }

  onMount(refresh);

  // ── Grouping ──────────────────────────────────────────────────────────────
  //
  // Two-level grouping. Schedules registered by the plugin host live under
  // `namespace = "plugin:<plugin-name>"`, so we extract the plugin name
  // and route every plugin's schedules into a single "Plugins" super-group,
  // with the plugin name surfacing as a sub-heading. Non-plugin namespaces
  // (`marketplace`, future `pipeline`, …) become their own top-level
  // sections without sub-groups.

  type SubGroup = {
    /** Human label for the sub-heading. `null` when the namespace has no
     *  meaningful sub-level (e.g. `marketplace`). */
    label: string | null;
    items: ScheduleSnapshot[];
  };

  type Group = {
    /** Canonical kind used to pick icons and section labels. */
    kind:    'plugin' | 'marketplace' | 'pipeline' | 'other';
    title:   string;
    /** Sub-groups; always at least one entry. When there's no sub-level
     *  the single entry has `label = null` and the rows render flat. */
    subs:    SubGroup[];
    total:   number;
  };

  const groups = $derived.by<Group[]>(() => {
    const plugin: SubGroup[] = [];
    const pluginIndex = new Map<string, SubGroup>();
    const flat: Map<string, ScheduleSnapshot[]> = new Map();

    for (const s of snapshots) {
      const ns = s.key.namespace;
      if (ns.startsWith('plugin:')) {
        const pname = ns.slice('plugin:'.length) || '?';
        let sub = pluginIndex.get(pname);
        if (!sub) {
          sub = { label: pname, items: [] };
          pluginIndex.set(pname, sub);
          plugin.push(sub);
        }
        sub.items.push(s);
      } else if (ns === 'plugin') {
        // Bare `plugin` namespace fallback (no sub-name in key) — rare,
        // but keep the data visible rather than dropping it.
        let sub = pluginIndex.get('—');
        if (!sub) {
          sub = { label: '—', items: [] };
          pluginIndex.set('—', sub);
          plugin.push(sub);
        }
        sub.items.push(s);
      } else {
        const arr = flat.get(ns) ?? [];
        arr.push(s);
        flat.set(ns, arr);
      }
    }

    const out: Group[] = [];

    const sortItems = (xs: ScheduleSnapshot[]) =>
      xs.slice().sort((a, b) => a.key.name.localeCompare(b.key.name));

    if (flat.has('marketplace')) {
      out.push({
        kind:  'marketplace',
        title: 'Marketplace',
        subs:  [{ label: null, items: sortItems(flat.get('marketplace')!) }],
        total: flat.get('marketplace')!.length,
      });
      flat.delete('marketplace');
    }

    if (plugin.length > 0) {
      const subs = plugin
        .map(s => ({ label: s.label, items: sortItems(s.items) }))
        .sort((a, b) => (a.label ?? '').localeCompare(b.label ?? ''));
      out.push({
        kind:  'plugin',
        title: 'Plugins',
        subs,
        total: subs.reduce((acc, s) => acc + s.items.length, 0),
      });
    }

    if (flat.has('pipeline')) {
      out.push({
        kind:  'pipeline',
        title: 'Pipelines',
        subs:  [{ label: null, items: sortItems(flat.get('pipeline')!) }],
        total: flat.get('pipeline')!.length,
      });
      flat.delete('pipeline');
    }

    for (const [ns, items] of Array.from(flat.entries()).sort((a, b) => a[0].localeCompare(b[0]))) {
      out.push({
        kind:  'other',
        title: ns.charAt(0).toUpperCase() + ns.slice(1),
        subs:  [{ label: null, items: sortItems(items) }],
        total: items.length,
      });
    }

    return out;
  });

  // ── Trigger formatting ────────────────────────────────────────────────────

  /** Render a serde `Duration` ({ secs, nanos }) as a human cadence:
   *  "1s" / "30s" / "5m" / "1h 30m" / "2d 4h". Nanos are ignored — schedules
   *  are second-grained in practice. */
  function fmtDuration(d: { secs: number; nanos: number }): string {
    const total = d.secs;
    if (total < 60) return `${total}s`;
    const m = Math.floor(total / 60);
    if (m < 60) {
      const rs = total % 60;
      return rs ? `${m}m ${rs}s` : `${m}m`;
    }
    const h = Math.floor(m / 60);
    if (h < 24) {
      const rm = m % 60;
      return rm ? `${h}h ${rm}m` : `${h}h`;
    }
    const days = Math.floor(h / 24);
    const rh   = h % 24;
    return rh ? `${days}d ${rh}h` : `${days}d`;
  }

  function cadence(t: Trigger): string {
    switch (t.kind) {
      case 'fixed_rate':  return `every ${fmtDuration(t.interval)}`;
      case 'fixed_delay': return `every ${fmtDuration(t.delay)}`;
      case 'cron':        return t.expr;
    }
  }

  function triggerKindLabel(t: Trigger): string {
    switch (t.kind) {
      case 'fixed_rate':  return 'fixed-rate';
      case 'fixed_delay': return 'fixed-delay';
      case 'cron':        return 'cron';
    }
  }

  /** Tooltip describing the trigger's scheduling semantics — explicit for
   *  the non-obvious distinction between fixed-rate and fixed-delay. */
  function triggerTooltip(t: Trigger): string {
    switch (t.kind) {
      case 'fixed_rate':
        return 'Fixed rate — next fire = previous start + interval. A long handler collapses missed ticks into one.';
      case 'fixed_delay':
        return 'Fixed delay — next fire = previous end + delay. Handler runtime adds to the gap.';
      case 'cron':
        return `Cron — fires at wall-clock matches of "${t.expr}" (sec min hour dom mon dow).`;
    }
  }

  const totalCount = $derived(snapshots.length);
</script>

<Modal {onClose} width="600px" height="min(640px, 88vh)" padBody={false} ariaLabel="Active schedules">
  {#snippet header()}
    <ModalHeader onClose={onClose}>
      <Clock size={14} class="header-icon" />
      <span class="modal-title">Active Schedules</span>
      {#if totalCount > 0}
        <span class="total-pill">{totalCount}</span>
      {/if}
      {#snippet actions()}
        <button
          class="header-btn"
          onclick={refresh}
          disabled={loading}
          use:tooltip={'Refresh'}
          aria-label="Refresh"
        >
          {#if loading}
            <Spinner size={12} />
          {:else}
            <RefreshCw size={13} />
          {/if}
        </button>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="body">
    {#if error}
      <div class="error-block">
        <span class="error-title">Failed to load schedules</span>
        <code class="error-msg">{error}</code>
      </div>
    {:else if loading && snapshots.length === 0}
      <div class="loading"><Spinner size={16} /></div>
    {:else if snapshots.length === 0}
      <div class="empty">
        <div class="empty-icon"><Inbox size={28} /></div>
        <span class="empty-title">No active schedules</span>
        <span class="empty-desc">
          Plugin timers and the marketplace auto-refresh appear here once
          they're registered.
        </span>
      </div>
    {:else}
      {#each groups as g (g.kind + ':' + g.title)}
        <section class="group">
          <header class="group-head">
            <span class="group-icon group-icon-{g.kind}">
              {#if g.kind === 'plugin'}
                <Plug size={11} />
              {:else if g.kind === 'marketplace'}
                <Store size={11} />
              {:else if g.kind === 'pipeline'}
                <GitBranch size={11} />
              {:else}
                <Clock size={11} />
              {/if}
            </span>
            <span class="group-label">{g.title}</span>
            <span class="group-rule"></span>
            <span class="group-count">{g.total}</span>
          </header>

          {#each g.subs as sub (sub.label ?? '__flat__')}
            {#if sub.label}
              <div class="sub-head">
                <span class="sub-label">{sub.label}</span>
                <span class="sub-rule"></span>
              </div>
            {/if}
            <ul class="rows">
              {#each sub.items as s (s.key.namespace + '::' + s.key.name)}
                <li class="row" class:disabled={!s.enabled}>
                  <span
                    class="trig-chip trig-{s.trigger.kind}"
                    use:tooltip={triggerTooltip(s.trigger)}
                    aria-label={triggerKindLabel(s.trigger)}
                  >
                    {#if s.trigger.kind === 'fixed_rate'}
                      <Gauge size={13} />
                    {:else if s.trigger.kind === 'fixed_delay'}
                      <Hourglass size={13} />
                    {:else}
                      <CalendarClock size={13} />
                    {/if}
                  </span>

                  <div class="row-body">
                    <div class="row-line-1">
                      <span class="row-name" title={s.key.name}>{s.key.name}</span>
                    </div>
                    <div class="row-line-2">
                      <span class="row-kind">{triggerKindLabel(s.trigger)}</span>
                    </div>
                  </div>

                  <div class="row-right">
                    <span class="row-cadence">{cadence(s.trigger)}</span>
                    <div class="row-flags">
                      {#if !s.enabled}
                        <span class="flag flag-disabled" use:tooltip={'Disabled — parked until re-enabled'} aria-label="Disabled">
                          <Power size={10} />
                        </span>
                      {/if}
                      {#if s.only_when_focused}
                        <span class="flag flag-focus" use:tooltip={'Focus-gated — skipped while the window is in the background'} aria-label="Focus-gated">
                          <Eye size={10} />
                        </span>
                      {/if}
                      {#if s.fire_on_load}
                        <span class="flag flag-onload" use:tooltip={'Fires once immediately when registered'} aria-label="Fires on load">
                          <Zap size={10} />
                        </span>
                      {/if}
                    </div>
                  </div>
                </li>
              {/each}
            </ul>
          {/each}
        </section>
      {/each}
    {/if}
  </div>
</Modal>

<style>
  /* ── Header pill ──────────────────────────────────────────────────────── */
  :global(.header-icon) { color: var(--text-muted); flex-shrink: 0; }

  .total-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 16px;
    padding: 0 5px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-secondary);
    font-size: var(--font-size-2xs);
    font-weight: 600;
    margin-left: 2px;
  }

  .header-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .header-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-primary);
  }
  .header-btn:disabled { cursor: default; opacity: 0.5; }

  /* ── Body shell ───────────────────────────────────────────────────────── */
  .body {
    height: 100%;
    overflow-y: auto;
    padding: 6px 0 10px;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
  }

  .loading {
    display: flex;
    justify-content: center;
    align-items: center;
    padding: 48px 0;
  }

  /* ── Empty state ──────────────────────────────────────────────────────── */
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 56px 32px 48px;
    text-align: center;
  }
  .empty-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-disabled);
    margin-bottom: 2px;
  }
  .empty-title {
    font-size: var(--font-size-md);
    font-weight: 500;
    color: var(--text-secondary);
  }
  .empty-desc {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    max-width: 360px;
    line-height: 1.5;
  }

  /* ── Error state ──────────────────────────────────────────────────────── */
  .error-block {
    margin: 12px 14px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .error-title {
    font-size: var(--font-size-xs);
    font-weight: 600;
    color: var(--error);
  }
  .error-msg {
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    font-family: var(--font-code);
    word-break: break-all;
  }

  /* ── Top-level group ─────────────────────────────────────────────────── */
  .group { padding: 6px 0 10px; }
  .group + .group { padding-top: 12px; }

  .group-head {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 14px 8px;
  }
  .group-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
  }
  .group-icon-marketplace {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
  }
  .group-icon-plugin {
    background: color-mix(in srgb, var(--info) 14%, transparent);
    color: var(--info);
  }
  .group-icon-pipeline {
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--success);
  }
  .group-icon-other {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-muted);
  }

  .group-label {
    font-size: var(--font-size-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .group-rule {
    flex: 1;
    height: 1px;
    background: var(--border-subtle);
  }
  .group-count {
    color: var(--text-disabled);
    font-size: var(--font-size-2xs);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }

  /* ── Sub-heading (per-plugin) ────────────────────────────────────────── */
  .sub-head {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 2px 14px 4px 22px;
  }
  .sub-label {
    font-size: var(--font-size-2xs);
    font-weight: 500;
    color: var(--text-muted);
    font-family: var(--font-code);
    flex-shrink: 0;
  }
  .sub-rule {
    flex: 1;
    height: 1px;
    background: color-mix(in srgb, var(--border-subtle) 60%, transparent);
  }

  /* ── Rows ────────────────────────────────────────────────────────────── */
  .rows {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 14px 7px 14px;
    border-left: 2px solid transparent;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .row:hover {
    background: rgba(255, 255, 255, 0.025);
    border-left-color: var(--accent);
  }
  .row.disabled { opacity: 0.55; }

  /* Trigger-kind icon chip — colored tint reflects the scheduling model. */
  .trig-chip {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
    border: 1px solid transparent;
  }
  .trig-fixed_rate {
    background: color-mix(in srgb, var(--success) 12%, transparent);
    border-color: color-mix(in srgb, var(--success) 28%, transparent);
    color: var(--success);
  }
  .trig-fixed_delay {
    background: color-mix(in srgb, var(--info) 12%, transparent);
    border-color: color-mix(in srgb, var(--info) 28%, transparent);
    color: var(--info);
  }
  .trig-cron {
    background: color-mix(in srgb, var(--warning) 12%, transparent);
    border-color: color-mix(in srgb, var(--warning) 28%, transparent);
    color: var(--warning);
  }

  .row-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .row-line-1 {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .row-name {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    font-family: var(--font-code);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }
  .row-line-2 {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }
  .row-kind {
    font-size: var(--font-size-3xs);
    color: var(--text-disabled);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 600;
  }

  .row-right {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .row-cadence {
    font-size: var(--font-size-xs);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }

  .row-flags {
    display: flex;
    gap: 3px;
    align-items: center;
  }
  .flag {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 1px solid transparent;
    flex-shrink: 0;
  }
  .flag-disabled {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-disabled);
    border-color: var(--border-subtle);
  }
  .flag-focus {
    background: color-mix(in srgb, var(--info) 14%, transparent);
    color: var(--info);
    border-color: color-mix(in srgb, var(--info) 32%, transparent);
  }
  .flag-onload {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 32%, transparent);
  }
</style>
