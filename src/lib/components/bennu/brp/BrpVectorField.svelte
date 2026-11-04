<script lang="ts">
  /**
   * A short list of numbers as one compact row — `x y z` of a position, the corners of a rect. Each axis is a
   * scrub field (drag the letter, double-click it to reset); an edit sends the whole list.
   */
  import ScrubInput from '$lib/components/shared/ui/ScrubInput.svelte';
  import { axisName, type InspectHint } from './brp-widgets';
  import { round } from './brp-math';

  let {
    value,
    path,
    readonly = false,
    reset = 0,
    step = 0.05,
    hint,
    labels,
    onCommit,
  }: {
    value: number[];
    path: string;
    readonly?: boolean;
    /** What a double-click on an axis letter puts back (1 for a scale). */
    reset?: number;
    step?: number;
    hint?: InspectHint;
    labels?: string[];
    onCommit: (path: string, value: unknown) => void;
  } = $props();

  function set(i: number, v: number) {
    onCommit(path, value.map((n, j) => (j === i ? round(v) : n)));
  }
</script>

<span class="vf">
  {#each value as n, i (i)}
    <span class="vf-axis">
      <ScrubInput
        value={n}
        label={labels?.[i] ?? axisName(i)}
        {step}
        min={hint?.range?.[0]}
        max={hint?.range?.[1]}
        {reset}
        {readonly}
        onchange={(v) => set(i, v)}
      />
    </span>
  {/each}
</span>

<style>
  .vf { display: inline-flex; flex-wrap: wrap; gap: 2px 4px; }
  .vf-axis { display: inline-flex; }
</style>
