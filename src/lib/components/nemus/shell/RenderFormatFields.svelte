<script lang="ts" module>
  /**
   * Single source of truth for the offline-render format options (sample rate,
   * bit depth). Shared by the Settings → Render panel (which persists them as the
   * global defaults) and the Export options dialog (which overrides them for one
   * export) so the two never drift.
   */
  export const RENDER_RATE_OPTIONS = [44_100, 48_000, 88_200, 96_000].map(
    (r) => ({ value: r, label: `${r / 1000} kHz` }),
  );
  export const RENDER_DEPTH_OPTIONS = [
    { value: 'int24', label: '24-bit integer' },
    { value: 'float32', label: '32-bit float' },
  ];
</script>

<script lang="ts">
  /**
   * The three offline-render format controls (sample rate · bit depth · reverb
   * tail) as a reusable set of `FormRow`s. Presentational: the caller owns the
   * values and where they're written (global config vs. a per-export override).
   */
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';

  interface Props {
    sampleRate: number;
    bitDepth: string;
    tail: number;
    onSampleRate: (v: number) => void;
    onBitDepth: (v: string) => void;
    onTail: (v: number) => void;
  }
  let { sampleRate, bitDepth, tail, onSampleRate, onBitDepth, onTail }: Props = $props();
</script>

<FormRow label="Sample rate">
  <Select value={sampleRate} options={RENDER_RATE_OPTIONS} onchange={(v) => onSampleRate(Number(v))} />
</FormRow>
<FormRow label="Bit depth">
  <Select value={bitDepth} options={RENDER_DEPTH_OPTIONS} onchange={onBitDepth} />
</FormRow>
<FormRow label="Reverb tail" description="Extra seconds rendered after the last event so reverb / delay tails aren't cut.">
  <NumberStepper value={tail} min={0} step={0.5} narrow suffix="s" onchange={onTail} ariaLabel="Reverb tail seconds" />
</FormRow>
