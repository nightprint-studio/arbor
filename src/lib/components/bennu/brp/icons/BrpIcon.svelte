<script lang="ts">
  /**
   * One icon of the Bevy Remote set (see `icon-set.ts`): 16×16 grid, 1.5 stroke, `currentColor`, no fill. It
   * takes the colour and, through `size`, the size of what it sits beside.
   */
  import { ICONS, type BrpIconName } from './icon-set';

  let { name, size = 14 }: { name: BrpIconName; size?: number } = $props();

  const prims = $derived(ICONS[name]);
</script>

<svg
  class="bi"
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#each prims as prim, i (i)}
    {#if 'p' in prim}
      <path d={prim.p} />
    {:else if 'c' in prim}
      <circle cx={prim.c[0]} cy={prim.c[1]} r={prim.c[2]} />
    {:else}
      <rect x={prim.r[0]} y={prim.r[1]} width={prim.r[2]} height={prim.r[3]} rx={prim.r[4]} />
    {/if}
  {/each}
</svg>

<style>
  .bi { display: block; flex: none; }
</style>
