<script lang="ts">
  /**
   * Six severity counter cards laid out as a responsive grid. Reads its
   * data from the headline `SecuritySummary` and forwards card clicks to
   * the parent panel (which decides whether to open the detail modal).
   *
   * Thin wrapper around the generic `<CounterGrid>` — anything chart-like
   * lives in `shared/charts/`; here we only translate severity domain
   * shapes into the widget's `CounterItem[]`.
   */
  import CounterGrid, { type CounterItem } from '$lib/components/shared/charts/CounterGrid.svelte';
  import { SEVERITY_META, formatMedianAge } from './severity-meta';
  import { SEVERITY_ORDER, type SeverityCounts, type SeverityMedians, type Severity } from '$lib/types/security';

  interface Props {
    counts:    SeverityCounts;
    medians:   SeverityMedians;
    onSelect?: (severity: Severity) => void;
  }

  let { counts, medians, onSelect }: Props = $props();

  const items = $derived<CounterItem[]>(
    SEVERITY_ORDER.map((sev) => ({
      key:   sev,
      label: SEVERITY_META[sev].label,
      value: counts[sev],
      hint:  formatMedianAge(medians[sev]),
      color: SEVERITY_META[sev].color,
    })),
  );

  function onItemSelect(key: string) {
    onSelect?.(key as Severity);
  }
</script>

<CounterGrid {items} onSelect={onItemSelect} />
