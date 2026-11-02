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
  import { Square, Unplug, Trash2, RotateCw, SlidersHorizontal } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';

  /** The tab in front is a debugger attached to a JVM we did not start: its Stop is a detach. */
  const attached = $derived(bennuRunStore.activeTab?.spec.kind === 'attach');
  const stopLabel = $derived(
    attached ? 'Detach — the program keeps running' : 'Stop the program',
  );
</script>

<!-- The tab in front, not "a run": several programs can be going at once, and Stop in this
     header is about the transcript underneath it. -->
{#if bennuRunStore.activeIsLive}
  <!-- Red: it is the one action here that ends something, and the only one you cannot take back.
       Not red for an attached JVM: detaching ends nothing but the debugger. -->
  <button
    class="ps-btn"
    class:ps-btn-danger={!attached}
    type="button"
    use:tooltip={stopLabel}
    aria-label={stopLabel}
    disabled={bennuRunStore.stopping}
    onclick={() => void bennuRunStore.stop()}
  >
    {#if attached}<Unplug size={13} />{:else}<Square size={12} />{/if}
  </button>
{/if}
<!-- Repeats THIS tab's run, into a new tab — so ⟳ on an old transcript reruns what that
     transcript was, which is the reason you were looking at it. -->
<IconButton
  tooltip="Rerun this"
  disabled={!bennuRunStore.canRerun || bennuRunStore.building}
  onclick={() => void bennuRunStore.rerunApp()}
>
  <RotateCw size={13} />
</IconButton>
<IconButton tooltip="Edit run configurations" onclick={() => bennuUiStore.openRunConfig()}>
  <SlidersHorizontal size={13} />
</IconButton>
<!-- Closes the finished runs. The live one stays: tidying the console is not a way to kill a
     program, and Stop is right there. -->
<IconButton
  tooltip="Close the finished runs"
  disabled={!bennuRunStore.tabs.some((t) => !t.live)}
  onclick={() => bennuRunStore.clearRun()}
>
  <Trash2 size={13} />
</IconButton>
