<script lang="ts">
  /**
   * Thin wrapper around the generic `<GaugeChart>` that hardcodes the
   * security risk-score colour stops + label mapping. Anything chart-y
   * lives in `shared/charts/GaugeChart.svelte`; here we only care about
   * domain semantics (Low/Medium/High/Critical).
   */
  import GaugeChart, { type GaugeSegment } from '$lib/components/shared/charts/GaugeChart.svelte';
  import type { RiskScore } from '$lib/types/security';

  interface Props {
    score: RiskScore;
    size?: 'sm' | 'md' | 'lg';
  }

  let { score, size = 'md' }: Props = $props();

  // Cutoffs match the GitLab risk-band reading (0-25 / 25-50 / 50-75 / 75-100).
  // Colours pulled from the severity tokens so the gauge lives in the
  // same palette as the counter cards / pills.
  const RISK_SEGMENTS: GaugeSegment[] = [
    { from: 0,  to: 25,  color: 'var(--severity-info)'     },
    { from: 25, to: 50,  color: 'var(--severity-medium)'   },
    { from: 50, to: 75,  color: 'var(--severity-high)'     },
    { from: 75, to: 100, color: 'var(--severity-critical)' },
  ];

  const formatScore = (v: number) => v.toFixed(1);
</script>

<GaugeChart
  value={score.value}
  min={0}
  max={100}
  segments={RISK_SEGMENTS}
  label={`${score.label} risk`}
  size={size}
  formatValue={formatScore}
/>
