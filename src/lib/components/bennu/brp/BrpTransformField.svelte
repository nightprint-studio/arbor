<script lang="ts">
  /**
   * A `Transform` as three compact lines — position, rotation (angles), scale with a lock that scales all
   * three axes together. Every edit is a whole-member write (`.translation`, `.rotation`, `.scale`).
   */
  import { Lock, LockOpen } from 'lucide-svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { memberPath } from './brp-value';
  import BrpQuatField from './BrpQuatField.svelte';
  import BrpVectorField from './BrpVectorField.svelte';
  import { round } from './brp-math';

  let {
    value,
    path,
    readonly = false,
    onCommit,
  }: {
    value: { translation: number[]; rotation: number[]; scale: number[] };
    path: string;
    readonly?: boolean;
    onCommit: (path: string, value: unknown) => void;
  } = $props();

  let locked = $state(false);

  /** With the lock on, the axis that changed sets the ratio the others follow. */
  function setScale(p: string, next: unknown) {
    const to = next as number[];
    if (!locked) return onCommit(p, to);
    const at = to.findIndex((v, i) => v !== value.scale[i]);
    if (at < 0 || value.scale[at] === 0) return onCommit(p, to);
    const ratio = to[at] / value.scale[at];
    onCommit(p, value.scale.map((v, i) => (i === at ? to[at] : round(v * ratio))));
  }
</script>

<div class="tf">
  <div class="tf-row"><span class="tf-key">position</span><BrpVectorField value={value.translation} path={memberPath(path, 'translation')} {readonly} {onCommit} /></div>
  <div class="tf-row"><span class="tf-key">rotation</span><BrpQuatField value={value.rotation} path={memberPath(path, 'rotation')} {readonly} {onCommit} /></div>
  <div class="tf-row">
    <span class="tf-key">scale</span>
    <BrpVectorField value={value.scale} path={memberPath(path, 'scale')} {readonly} reset={1} onCommit={setScale} />
    <IconButton tooltip={locked ? 'Scale axes together: on' : 'Scale axes together: off'} size={20} active={locked} onclick={() => (locked = !locked)}>
      {#if locked}<Lock size={11} />{:else}<LockOpen size={11} />{/if}
    </IconButton>
  </div>
</div>

<style>
  .tf { display: flex; flex-direction: column; gap: 3px; }
  .tf-row { display: flex; align-items: center; gap: 8px; min-height: 22px; }
  .tf-key { flex: none; min-width: 52px; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
