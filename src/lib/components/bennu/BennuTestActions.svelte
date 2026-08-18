<script lang="ts">
  /**
   * The header buttons of a test run — run, rerun, rerun-failed, filter, sort, fold, clear.
   *
   * A component rather than a snippet inside the Run console because the console's header has
   * two sets of actions now (a program's and a test run's) and the one that is showing depends
   * on which tab you are on. Keeping each set whole, in its own file, is what stops
   * {@link BennuRunPanel} from becoming a header with two long conditional arms in it.
   *
   * The class names are the panel-shell ones (`ps-btn`) because these buttons live in the shell's
   * header slot and must look like the buttons either side of them.
   */
  import {
    ChevronsDownUp, ChevronsUpDown, Clock, EyeOff, Filter as FilterIcon, ListRestart, Play,
    RotateCw, Square, Trash2,
  } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { activeTestStore } from '$lib/stores/bennu/test-runner.svelte';
  import { bennuCargoTestStore } from '$lib/stores/bennu/cargo-tests.svelte';

  const store = $derived(activeTestStore());
  const root = $derived(projectStore.project?.root ?? '');
  /** `#[ignore]` is cargo's, and so is the only way to override it (`--include-ignored`). There is
   *  no Maven equivalent worth a button: a `@Disabled` test cannot be run without editing it. */
  const cargo = $derived(projectStore.isCargo);
</script>

{#if store.running}
  <!-- Red: it is the one action here that ends something. -->
  <button class="ps-btn ps-btn-danger" type="button" use:tooltip={'Stop'} aria-label="Stop the test run"
    onclick={() => void store.stop()}><Square size={12} /></button>
{:else}
  <button class="ps-btn" type="button" disabled={!root}
    use:tooltip={{ content: 'Run all tests', shortcut: 'Ctrl+Shift+F5' }} aria-label="Run all tests"
    onclick={() => void store.runAll(root)}><Play size={13} /></button>
  <button class="ps-btn" type="button" disabled={!store.hasResults}
    use:tooltip={{ content: 'Rerun', shortcut: 'Ctrl+F5' }} aria-label="Rerun the last test run"
    onclick={() => void store.rerun()}><RotateCw size={13} /></button>
  <button class="ps-btn" type="button" disabled={!store.hasFailures}
    use:tooltip={'Rerun failed tests'} aria-label="Rerun failed tests"
    onclick={() => void store.rerunFailed()}><ListRestart size={13} /></button>
{/if}
{#if cargo}
  <button
    class="ps-btn"
    class:ps-btn-active={bennuCargoTestStore.includeIgnored}
    type="button"
    use:tooltip={'Also run #[ignore]d tests'}
    aria-label="Also run ignored tests"
    aria-pressed={bennuCargoTestStore.includeIgnored}
    onclick={() => bennuCargoTestStore.setIncludeIgnored(!bennuCargoTestStore.includeIgnored)}
  ><EyeOff size={13} /></button>
{/if}
<button class="ps-btn" class:ps-btn-active={store.onlyFailed} type="button"
  use:tooltip={'Show only failed'} aria-label="Show only failed" aria-pressed={store.onlyFailed}
  onclick={() => store.setOnlyFailed(!store.onlyFailed)}><FilterIcon size={13} /></button>
<button class="ps-btn" class:ps-btn-active={store.sortByTime} type="button"
  use:tooltip={'Sort by duration'} aria-label="Sort by duration" aria-pressed={store.sortByTime}
  onclick={() => store.setSortByTime(!store.sortByTime)}><Clock size={13} /></button>
<button class="ps-btn" type="button" use:tooltip={'Collapse all'} aria-label="Collapse all"
  onclick={() => store.collapseAll()}><ChevronsDownUp size={13} /></button>
<button class="ps-btn" type="button" use:tooltip={'Expand all'} aria-label="Expand all"
  onclick={() => store.expandAll()}><ChevronsUpDown size={13} /></button>
<button class="ps-btn" type="button" disabled={store.running || !store.hasResults}
  use:tooltip={'Clear'} aria-label="Clear results" onclick={() => store.clear()}><Trash2 size={13} /></button>
