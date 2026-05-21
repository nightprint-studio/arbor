<script lang="ts">
  /**
   * Thin wrapper around the generic `<LineChart>` that maps a
   * `VulnTimeSeries` (six severity buckets) into one `LineSeries` per
   * severity, and surfaces the 30 / 60 / 90 day range picker.
   *
   * Reload behaviour: changing `rangeDays` updates the store and triggers
   * a fresh `loadSummary(tabId)` call so the backend re-fetches the
   * matching window. This component is "smart" only in the sense that it
   * owns the picker; the actual chart rendering is fully generic.
   */
  import LineChart, { type LineSeries } from '$lib/components/shared/charts/LineChart.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';

  import { securityStore } from '$lib/stores/security.svelte';
  import { tabsStore }     from '$lib/stores/tabs.svelte';
  import { SEVERITY_ORDER, type VulnTimeSeries } from '$lib/types/security';
  import { SEVERITY_META } from './severity-meta';

  interface Props {
    timeSeries: VulnTimeSeries;
    height?:    number;
    /**
     * When non-empty, only the listed severities are drawn. Empty array
     * (the default) means "show every severity that has data" — same
     * behaviour as before the filter bar landed.
     */
    severityFilter?: import('$lib/types/security').Severity[];
  }

  let { timeSeries, height = 220, severityFilter = [] }: Props = $props();

  const RANGE_OPTIONS = [
    { value: '30', label: '30 days' },
    { value: '60', label: '60 days' },
    { value: '90', label: '90 days' },
  ];

  function onRangeChange(v: string) {
    const days = parseInt(v, 10) as 30 | 60 | 90;
    if (days !== 30 && days !== 60 && days !== 90) return;
    securityStore.setRangeDays(days);
    const tabId = tabsStore.activeTabId;
    if (tabId) securityStore.loadSummary(tabId);
  }

  // Build one series per severity. Each `TimePoint` becomes a (Date, count)
  // pair. Skip severities with zero across the entire window — the visual
  // noise hurts more than the consistency. When the user has narrowed the
  // dashboard down to specific severities, only those are drawn.
  const series = $derived.by<LineSeries[]>(() => {
    const allow = severityFilter.length > 0
      ? new Set<string>(severityFilter)
      : null;
    return SEVERITY_ORDER
      .filter((sev) => !allow || allow.has(sev))
      .map((sev) => {
        const points = timeSeries.points.map((tp) => ({
          x: new Date(tp.date),
          y: tp[sev],
        }));
        const allZero = points.every((p) => p.y === 0);
        return {
          id:     sev,
          label:  SEVERITY_META[sev].label,
          color:  SEVERITY_META[sev].color,
          points: allZero ? [] : points,
        };
      })
      .filter((s) => s.points.length > 0);
  });

  const yFormatter = (v: number) => (Number.isInteger(v) ? String(v) : '');
</script>

<div class="vts-wrap">
  <header class="vts-header">
    <h4 class="vts-title">Vulnerabilities over time</h4>
    <div class="vts-range">
      <Select
        value={String(securityStore.rangeDays)}
        options={RANGE_OPTIONS}
        onchange={onRangeChange}
        narrow
      />
    </div>
  </header>

  <LineChart
    series={series}
    xAxis={{ kind: 'time' }}
    yAxis={{ formatter: yFormatter, includeZero: true }}
    height={height}
  />
</div>

<style>
  .vts-wrap {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }
  .vts-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .vts-title {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .vts-range {
    flex-shrink: 0;
  }
</style>
