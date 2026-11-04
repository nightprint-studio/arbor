<script lang="ts">
  /**
   * A rotation. Shown as Euler angles in degrees — what a person reads — with a toggle for the raw
   * quaternion. An angle edit rebuilds the quaternion from all three angles and sends it whole.
   */
  import ToggleButton from '$lib/components/shared/ui/ToggleButton.svelte';
  import ScrubInput from '$lib/components/shared/ui/ScrubInput.svelte';
  import BrpVectorField from './BrpVectorField.svelte';
  import { eulerToQuat, quatToEuler, round, type Euler, type Quat } from './brp-math';

  let {
    value,
    path,
    readonly = false,
    onCommit,
  }: {
    value: number[];
    path: string;
    readonly?: boolean;
    onCommit: (path: string, value: unknown) => void;
  } = $props();

  let raw = $state(false);
  const euler = $derived(quatToEuler(value as Quat));

  function setAngle(i: number, deg: number) {
    const next = [...euler] as Euler;
    next[i] = deg;
    onCommit(path, eulerToQuat(next).map((n) => round(n, 6)));
  }
</script>

<span class="qf">
  {#if raw}
    <BrpVectorField {value} {path} {readonly} reset={0} step={0.01} {onCommit} />
  {:else}
    <span class="qf-angles">
      {#each euler as deg, i (i)}
        <span class="qf-angle">
          <ScrubInput value={deg} label={'xyz'[i]} unit="°" step={0.5} digits={1} reset={0} {readonly} onchange={(v) => setAngle(i, v)} />
        </span>
      {/each}
    </span>
  {/if}
  <ToggleButton pressed={raw} label="q" title={raw ? 'Show as angles' : 'Show as quaternion'} compact onclick={() => (raw = !raw)} />
</span>

<style>
  .qf { display: inline-flex; align-items: center; gap: 4px; flex-wrap: wrap; }
  .qf-angles { display: inline-flex; gap: 4px; }
  .qf-angle { display: inline-flex; }
</style>
