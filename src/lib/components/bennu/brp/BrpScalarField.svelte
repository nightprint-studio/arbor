<script lang="ts">
  /**
   * A number a game's `#[inspect(...)]` hint says is more than a number: a slider over a range, an angle
   * stored in radians but edited in degrees, a date stored as seconds since the epoch.
   */
  import Input from '$lib/components/shared/ui/Input.svelte';
  import ScrubInput from '$lib/components/shared/ui/ScrubInput.svelte';
  import { round } from './brp-math';
  import { dateToSeconds, secondsToDate, type InspectHint } from './brp-widgets';

  let {
    kind,
    value,
    path,
    hint,
    readonly = false,
    onCommit,
  }: {
    kind: 'slider' | 'angle' | 'date';
    value: number;
    path: string;
    hint?: InspectHint;
    readonly?: boolean;
    onCommit: (path: string, value: unknown) => void;
  } = $props();

  const [lo, hi] = $derived(hint?.range ?? [0, 1]);
  const step = $derived(hint?.step ?? (hi - lo) / 100);

  const toDeg = (rad: number) => (rad * 180) / Math.PI;
  const toRad = (deg: number) => (deg * Math.PI) / 180;
</script>

{#if kind === 'slider'}
  <span class="sf">
    <input
      class="sf-range"
      type="range"
      min={lo}
      max={hi}
      {step}
      {value}
      disabled={readonly}
      aria-label={hint?.label ?? 'Value'}
      onchange={(e) => onCommit(path, round(Number((e.currentTarget as HTMLInputElement).value)))}
    />
    <span class="sf-num">
      <ScrubInput {value} {step} min={lo} max={hi} digits={3} unit={hint?.unit} {readonly} onchange={(v) => onCommit(path, round(v))} />
    </span>
  </span>
{:else if kind === 'angle'}
  <span class="sf-num">
    <ScrubInput value={toDeg(value)} unit="°" step={0.5} digits={1} min={hint?.range ? toDeg(hint.range[0]) : undefined} max={hint?.range ? toDeg(hint.range[1]) : undefined} {readonly} onchange={(d) => onCommit(path, round(toRad(d), 6))} />
  </span>
{:else}
  <span class="sf-date">
    <Input
      type="text"
      value={secondsToDate(value)}
      size="sm"
      quiet
      {readonly}
      placeholder="YYYY-MM-DD"
      ariaLabel="Date"
      onchange={(v) => { const s = dateToSeconds(v); if (s !== null) onCommit(path, s); }}
    />
  </span>
{/if}

<style>
  .sf { display: inline-flex; align-items: center; gap: 8px; min-width: 0; }
  .sf-range { width: 120px; accent-color: var(--accent); }
  .sf-num { display: inline-flex; }
  .sf-date { display: inline-flex; width: 110px; }
</style>
