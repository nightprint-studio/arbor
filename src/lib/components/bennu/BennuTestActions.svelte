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
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { activeTestRoot, activeTestStore, testsAreCargo } from '$lib/stores/bennu/test-runner.svelte';
  import { bennuCargoTestStore } from '$lib/stores/bennu/cargo-tests.svelte';

  const store = $derived(activeTestStore());
  const root = $derived(activeTestRoot() ?? '');
  /** `#[ignore]` is cargo's, and so is the only way to override it (`--include-ignored`). There is
   *  no Maven equivalent worth a button: a `@Disabled` test cannot be run without editing it. */
  const cargo = $derived(testsAreCargo());
</script>

{#if store.running}
  <!-- Red: it is the one action here that ends something. -->
  <button class="ps-btn ps-btn-danger" type="button" use:tooltip={'Stop'} aria-label="Stop the test run"
    onclick={() => void store.stop()}><Square size={12} /></button>
{:else}
  <IconButton tooltip="Run all tests" shortcut="Ctrl+Shift+F5" disabled={!root}
    onclick={() => void store.runAll(root)}><Play size={13} /></IconButton>
  <IconButton tooltip="Rerun" shortcut="Ctrl+F5" disabled={!store.hasResults}
    onclick={() => void store.rerun()}><RotateCw size={13} /></IconButton>
  <IconButton tooltip="Rerun failed tests" disabled={!store.hasFailures}
    onclick={() => void store.rerunFailed()}><ListRestart size={13} /></IconButton>
{/if}
{#if cargo}
  <IconButton
    tooltip="Also run #[ignore]d tests"
    active={bennuCargoTestStore.includeIgnored}
    onclick={() => bennuCargoTestStore.setIncludeIgnored(!bennuCargoTestStore.includeIgnored)}
  ><EyeOff size={13} /></IconButton>
{/if}
<IconButton tooltip="Show only failed" active={store.onlyFailed}
  onclick={() => store.setOnlyFailed(!store.onlyFailed)}><FilterIcon size={13} /></IconButton>
<IconButton tooltip="Sort by duration" active={store.sortByTime}
  onclick={() => store.setSortByTime(!store.sortByTime)}><Clock size={13} /></IconButton>
<IconButton tooltip="Collapse all" onclick={() => store.collapseAll()}><ChevronsDownUp size={13} /></IconButton>
<IconButton tooltip="Expand all" onclick={() => store.expandAll()}><ChevronsUpDown size={13} /></IconButton>
<IconButton tooltip="Clear" disabled={store.running || !store.hasResults}
  onclick={() => store.clear()}><Trash2 size={13} /></IconButton>
