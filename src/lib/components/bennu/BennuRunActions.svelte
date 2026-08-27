<script lang="ts">
  /**
   * The header buttons of a launched program — stop, rerun, edit configurations, tidy up.
   *
   * The counterpart of {@link BennuTestActions}: the Run console's header carries one set or
   * the other depending on the tab, and each set lives whole in its own file so the panel is
   * not a header with two long conditional arms inside it.
   *
   * The class names are the panel-shell ones (`ps-btn`) because these live in the shell's header
   * slot and must look like the buttons either side of them.
   */
  import { Square, Trash2, RotateCw, SlidersHorizontal } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
</script>

<!-- The tab in front, not "a run": several programs can be going at once, and Stop in this
     header is about the transcript underneath it. -->
{#if bennuRunStore.activeIsLive}
  <!-- Red: it is the one action here that ends something, and the only one you cannot take back. -->
  <button
    class="ps-btn ps-btn-danger"
    type="button"
    use:tooltip={'Stop the program'}
    aria-label="Stop the program"
    disabled={bennuRunStore.stopping}
    onclick={() => void bennuRunStore.stop()}
  >
    <Square size={12} />
  </button>
{/if}
<!-- Repeats THIS tab's run, into a new tab — so ⟳ on an old transcript reruns what that
     transcript was, which is the reason you were looking at it. -->
<button
  class="ps-btn"
  type="button"
  use:tooltip={'Rerun this'}
  aria-label="Rerun this"
  disabled={!bennuRunStore.canRerun || bennuRunStore.building}
  onclick={() => void bennuRunStore.rerunApp()}
>
  <RotateCw size={13} />
</button>
<button
  class="ps-btn"
  type="button"
  use:tooltip={'Edit run configurations'}
  aria-label="Edit run configurations"
  onclick={() => bennuUiStore.openRunConfig()}
>
  <SlidersHorizontal size={13} />
</button>
<!-- Closes the finished runs. The live one stays: tidying the console is not a way to kill a
     program, and Stop is right there. -->
<button
  class="ps-btn"
  type="button"
  use:tooltip={'Close the finished runs'}
  aria-label="Close the finished runs"
  disabled={!bennuRunStore.tabs.some((t) => !t.live)}
  onclick={() => bennuRunStore.clearRun()}
>
  <Trash2 size={13} />
</button>
