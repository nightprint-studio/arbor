<script lang="ts">
  /**
   * Live statistics of the game: frame-time percentiles, a graph of the last frames against a 60 fps
   * budget, every diagnostic the game registered, and the window. Polls once a second while shown.
   */
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import PropertyList, { type PropertyRow } from '$lib/components/shared/ui/PropertyList.svelte';
  import Sparkline from '$lib/components/shared/ui/Sparkline.svelte';
  import { brpDriveStore as drive } from '$lib/stores/bennu/brp-drive.svelte';

  const POLL_MS = 1000;
  /** A 60 fps frame, in ms. */
  const BUDGET_MS = 1000 / 60;

  $effect(() => {
    void drive.pollStats();
    const timer = setInterval(() => void drive.pollStats(), POLL_MS);
    return () => clearInterval(timer);
  });

  const fmt = (v: number | null | undefined, digits = 1) => (v === null || v === undefined ? '—' : v.toFixed(digits));

  const frameRows = $derived.by<PropertyRow[]>(() => {
    const f = drive.stats?.frame;
    if (!f) return [];
    return [
      { label: 'FPS', value: fmt(f.fps, 0) },
      { label: 'Average', value: `${fmt(f.avg_ms)} ms` },
      { label: 'Median (p50)', value: `${fmt(f.p50_ms)} ms` },
      { label: 'p95', value: `${fmt(f.p95_ms)} ms` },
      { label: 'p99', value: `${fmt(f.p99_ms)} ms` },
      { label: 'Worst', value: `${fmt(f.max_ms)} ms` },
      { label: 'Frames measured', value: String(f.frames) },
    ];
  });

  const otherRows = $derived.by<PropertyRow[]>(() => {
    const s = drive.stats;
    if (!s) return [];
    const rows: PropertyRow[] = (s.diagnostics ?? []).map((d) => ({
      label: d.path,
      value: `${fmt(d.smoothed ?? d.value, 2)}${d.suffix ?? ''}`,
    }));
    if (s.window) rows.push({ label: 'Window', value: `${s.window.width}×${s.window.height} (scale ${s.window.scale_factor})` });
    if (s.uptime_s !== null) rows.push({ label: 'Uptime', value: `${Math.round(s.uptime_s)} s` });
    return rows;
  });
</script>

<div class="st">
  <PaneHeader title="Statistics" />
  <div class="st-body">
    {#if drive.statsError}
      <Alert variant="warning" compact text={drive.statsError} />
    {:else if !drive.stats}
      <EmptyState message="Waiting for the game…" />
    {:else}
      <h4 class="st-sub">Frame time</h4>
      <Sparkline values={drive.stats.frame_ms_recent} reference={BUDGET_MS} referenceLabel="60 fps budget (16.7 ms)" ariaLabel="Frame time of the last frames, in milliseconds" />
      <div class="st-cols">
        <div>
          <h4 class="st-sub">Percentiles</h4>
          <PropertyList rows={frameRows} />
        </div>
        {#if otherRows.length > 0}
          <div>
            <h4 class="st-sub">Diagnostics</h4>
            <PropertyList rows={otherRows} />
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .st { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .st-body { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 10px; display: flex; flex-direction: column; gap: 8px; }
  .st-cols { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 4px 24px; }
  .st-sub { margin: 4px 0 0; font-size: var(--font-size-xs); font-weight: 600; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.04em; }
</style>
