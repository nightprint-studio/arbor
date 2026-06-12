<script module lang="ts">
  /** Which edge the panel docks to — picks the resize axis + slide transition. */
  export type PanelOrientation = 'left' | 'right' | 'bottom';
</script>

<script lang="ts">
  /**
   * PanelCard — a floating bg-base "card" wrapping a `ResizablePanel`, with the
   * IDE open/close slide built in. This is the docked-panel primitive shared by
   * the main window and the nemus window: drop it inside an `{#if}` and it
   * animates itself in/out (width for left/right, height for bottom) while the
   * neighbours reflow.
   *
   *   {#if showLeft}
   *     <PanelCard orientation="left" initialSize={240} minSize={170} maxSize={460}>
   *       <MyPanel />
   *     </PanelCard>
   *   {/if}
   *
   * The optional transition-event callbacks mirror Svelte's element events so a
   * host can react to the open/close lifecycle (e.g. signalling "panel ready"
   * once the intro finishes).
   *
   * Each card carries `data-panel={orientation}` so a host can target a specific
   * dock from outside (e.g. AppShell's F6 focus-zone cycling).
   */
  import type { Snippet } from 'svelte';
  import ResizablePanel from '$lib/components/layout/ResizablePanel.svelte';
  import { sidebarSlide, bottomSlide } from '$lib/utils/panel-transitions';
  import { animStore } from '$lib/stores/animations.svelte';

  interface Props {
    orientation?: PanelOrientation;
    initialSize: number;
    minSize?: number;
    maxSize?: number;
    onResize?: (size: number) => void;
    /** Animate the open/close slide (default true). */
    animate?: boolean;
    /** Transition duration in ms (default `animStore.dPanel`). */
    duration?: number;
    children: Snippet;
    onintrostart?: () => void;
    onintroend?: () => void;
    onoutrostart?: () => void;
    onoutroend?: () => void;
  }

  let {
    orientation = 'left',
    initialSize,
    minSize,
    maxSize,
    onResize,
    animate = true,
    duration,
    children,
    onintrostart,
    onintroend,
    onoutrostart,
    onoutroend,
  }: Props = $props();

  const isBottom = $derived(orientation === 'bottom');
  const direction = $derived(isBottom ? 'vertical' : 'horizontal');
  // Left docks with the handle on its right (no reverse); right + bottom dock
  // with the handle on the inner edge (reverse) — same as the hand-rolled
  // panels this replaces.
  const reverse = $derived(orientation === 'right' || orientation === 'bottom');
  const dur = $derived(duration ?? animStore.dPanel);
</script>

{#snippet body()}
  <ResizablePanel {direction} {initialSize} {minSize} {maxSize} {onResize} {reverse}>
    {@render children()}
  </ResizablePanel>
{/snippet}

{#if !animate}
  <div class="pc-card" class:pc-bottom={isBottom} data-panel={orientation}>{@render body()}</div>
{:else if isBottom}
  <!-- `|global`: the `{#if}` that mounts/unmounts this card lives in the HOST
       (AppShell / NemusShell), an ancestor block across the component boundary.
       A local transition only fires for its own block's create/destroy, so it
       would never play here — global makes it react to the ancestor toggle. -->
  <div
    class="pc-card pc-bottom"
    data-panel={orientation}
    transition:bottomSlide|global={{ duration: dur }}
    {onintrostart}
    {onintroend}
    {onoutrostart}
    {onoutroend}
  >{@render body()}</div>
{:else}
  <div
    class="pc-card"
    data-panel={orientation}
    transition:sidebarSlide|global={{ duration: dur }}
    {onintrostart}
    {onintroend}
    {onoutrostart}
    {onoutroend}
  >{@render body()}</div>
{/if}

<style>
  /* Side panel: full-height card, shrinks to the ResizablePanel's width. */
  .pc-card {
    display: flex;
    height: 100%;
    flex-shrink: 0;
    overflow: hidden;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }
  /* Bottom panel: full-width card stacking downward. */
  .pc-card.pc-bottom {
    flex-direction: column;
    width: 100%;
    height: auto;
  }
</style>
