<script lang="ts">
  /**
   * The bottom tool window — **one panel at a time**, the one whose rail button opened it.
   *
   * It used to be a tabbed container: a shared header carrying a five-tab strip, with each
   * section's own bar underneath. That is two rows of chrome for one panel, and it made the rail
   * buttons and the tab strip two ways of saying the same thing, each needing to be kept in step
   * with the other. Corvus and Picus have had the other arrangement all along — a button opens
   * *its* panel, and the panel owns its title, its count, its actions and its close button — and
   * this is now that. Build and Problems are the one deliberate exception: two readings of the
   * same run, so they share a panel (see {@link BennuBuildProblemsPanel}).
   *
   * Which is why there is so little here. The sections stay MOUNTED and hidden rather than being
   * destroyed on a switch: a terminal session is a live PTY and a build log is a scroll position,
   * and neither should end because you looked at the TODOs. The exception is Forms, which is
   * scoped to the active file and re-analysed on every switch anyway — mounting it while hidden
   * would burn an include-graph walk for nothing.
   */
  import BennuBuildProblemsPanel from './BennuBuildProblemsPanel.svelte';
  import BennuRunPanel from './BennuRunPanel.svelte';
  import BennuTerminalView from './BennuTerminalView.svelte';
  import BennuTodoPanel from './BennuTodoPanel.svelte';
  import BennuHierarchyPanel from './BennuHierarchyPanel.svelte';
  import BennuFormsPanel from './BennuFormsPanel.svelte';
  import BennuCatalogPanel from './BennuCatalogPanel.svelte';
  import { isFrameworkCatalog } from './framework-catalogs';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

  const active = $derived(bennuUiStore.bottomPanel ?? 'problems');
  const buildish = $derived(active === 'build' || active === 'problems');
  // The framework catalogs share one component, parameterised by id — and, like Forms,
  // are mounted only while shown: their rows come from a store that caches per project,
  // so nothing is lost by unmounting and nothing is fetched while hidden.
  const catalog = $derived(isFrameworkCatalog(active) ? active : null);
</script>

<div class="dock">
  <div class="dock-section" class:hidden={!buildish}>
    <BennuBuildProblemsPanel />
  </div>
  <!-- Stays mounted for the same reason the terminal does: there is a LIVE PROCESS behind it.
       Unmounting would drop the console's scroll, its buffer and the input box mid-run — and a
       test run, which lives in here too, is minutes long and streaming. -->
  <div class="dock-section" class:hidden={active !== 'run'}>
    <BennuRunPanel />
  </div>
  <div class="dock-section" class:hidden={active !== 'todos'}>
    <BennuTodoPanel />
  </div>
  <!-- Stays mounted while hidden, like the two above: the tree cost several round-trips to build,
       one level at a time, and unmounting it would throw that away — then looking at the Problems
       list and coming back would mean building it again from the caret, which has since moved. -->
  <div class="dock-section" class:hidden={active !== 'hierarchy'}>
    <BennuHierarchyPanel />
  </div>
  {#if active === 'forms'}
    <div class="dock-section">
      <BennuFormsPanel dock />
    </div>
  {/if}
  {#if catalog}
    <div class="dock-section">
      {#key catalog}<BennuCatalogPanel id={catalog} />{/key}
    </div>
  {/if}
  <div class="dock-section" class:hidden={active !== 'terminal'}>
    <BennuTerminalView />
  </div>
</div>

<style>
  .dock {
    display: flex; flex-direction: column;
    height: 100%; width: 100%; min-height: 0;
    background: var(--bg-base);
    position: relative;
    overflow: hidden;
  }
  .dock-section {
    position: absolute; inset: 0;
    display: flex; flex-direction: column; min-height: 0;
  }
  .dock-section.hidden { display: none; }
</style>
