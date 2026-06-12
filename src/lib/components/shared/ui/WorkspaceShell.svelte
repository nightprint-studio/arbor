<script lang="ts">
  /**
   * WorkspaceShell — the IntelliJ-style workspace frame shared by the main
   * window and the nemus window.
   *
   * Renders the `bg-elevated` `.workspace` strip with an (optional) activity
   * rail flush on each edge and the inset `.panels` container in between (the
   * 4-px gaps reveal the elevated bg under the floating `bg-base` cards). The
   * rails slide in/out with `barSlide`; the panels inside are the host's
   * responsibility — drop `PanelCard`s in the `panels` snippet to get the
   * matching open/close slide.
   *
   * It deliberately owns ONLY the workspace, not the outer `.shell` (title bar
   * + footer + window overlays): those diverge too much between the two windows
   * (the main window stacks ~30 modals/overlays in its shell) to share cleanly.
   *
   *   <WorkspaceShell showLeftRail={...} showRightRail={...}>
   *     {#snippet leftRail()}<ActivityBar .../>{/snippet}
   *     {#snippet rightRail()}<ActivityBar side="right" .../>{/snippet}
   *     {#snippet panels()}
   *       {#if showLeft}<PanelCard orientation="left" ...>…</PanelCard>{/if}
   *       <div class="main-col">…</div>
   *       {#if showRight}<PanelCard orientation="right" ...>…</PanelCard>{/if}
   *     {/snippet}
   *   </WorkspaceShell>
   */
  import type { Snippet } from 'svelte';
  import { barSlide } from '$lib/utils/panel-transitions';
  import { animStore } from '$lib/stores/animations.svelte';

  interface Props {
    /** Left activity rail. Omit → no left rail. */
    leftRail?: Snippet;
    /** Right activity rail. Omit → no right rail. */
    rightRail?: Snippet;
    /** The inset panel arrangement (sidebars, main column, bottom panel). */
    panels: Snippet;
    /** Show the left rail (default: shown whenever `leftRail` is provided). */
    showLeftRail?: boolean;
    /** Show the right rail (default: shown whenever `rightRail` is provided). */
    showRightRail?: boolean;
    /**
     * Content rendered before the rails inside `.workspace` (e.g. the main
     * window's edge-hover peek triggers for the hidden-rail mode).
     */
    beforeRails?: Snippet;
    /** Rail slide duration in ms (default `animStore.dPanel`). */
    railDuration?: number;
  }

  let {
    leftRail,
    rightRail,
    panels,
    showLeftRail,
    showRightRail,
    beforeRails,
    railDuration,
  }: Props = $props();

  const showLeft = $derived(showLeftRail ?? !!leftRail);
  const showRight = $derived(showRightRail ?? !!rightRail);
  const dur = $derived(railDuration ?? animStore.dPanel);
</script>

<div class="workspace">
  {#if beforeRails}{@render beforeRails()}{/if}

  {#if leftRail && showLeft}
    <div class="ab-slot" transition:barSlide={{ duration: dur }}>
      {@render leftRail()}
    </div>
  {/if}

  <!-- Inset panels container: the gaps reveal the workspace bg (IntelliJ-style). -->
  <div class="panels">
    {@render panels()}
  </div>

  {#if rightRail && showRight}
    <div class="ab-slot" transition:barSlide={{ duration: dur }}>
      {@render rightRail()}
    </div>
  {/if}
</div>

<style>
  .workspace {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-elevated);
  }

  /* Inset container holding the sidebars + main column. The gap + bottom/side
     padding leave the elevated bg visible around the floating bg-base cards. */
  .panels {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    gap: 4px;
    padding: 0 4px 4px 4px;
  }

  /* Rail wrapper — the slide transition collapses this box's width. */
  .ab-slot {
    display: flex;
    flex-shrink: 0;
    overflow: hidden;
  }
</style>
