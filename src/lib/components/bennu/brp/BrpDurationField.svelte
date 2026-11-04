<script lang="ts">
  /** A `Duration` (`{secs, nanos}`) as seconds with a unit. */
  import ScrubInput from '$lib/components/shared/ui/ScrubInput.svelte';
  import { durationToSeconds, secondsToDuration } from './brp-widgets';

  let {
    value,
    path,
    readonly = false,
    onCommit,
  }: {
    value: { secs: number; nanos: number };
    path: string;
    readonly?: boolean;
    onCommit: (path: string, value: unknown) => void;
  } = $props();
</script>

<span class="df">
  <ScrubInput value={durationToSeconds(value)} unit="s" step={0.05} min={0} digits={3} {readonly} onchange={(s) => onCommit(path, secondsToDuration(s))} />
</span>

<style>
  .df { display: inline-flex; }
</style>
