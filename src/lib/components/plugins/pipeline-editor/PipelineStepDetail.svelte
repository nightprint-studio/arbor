<!--
  PipelineStepDetail — right column. Shows the plugin-provided detail form for
  the currently selected step (or stage), rendered through the parent's
  recursive `renderNode` snippet. Falls back to an empty-state message when
  no form is shipped.

  Show the detail panel whenever the plugin shipped a `step_detail_form`. The
  form can be either a step form (when selected_step_id is set) or a stage
  settings form (when only selected_stage_id is set, emitted by the plugin's
  edit_stage handler). Guarding on selected_step_id alone hid the stage
  settings form entirely.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { FormNode } from '$lib/types/plugin';

  interface Props {
    stepDetailForm?: FormNode[];
    emptyLabel?:     string;
    renderNode?:     Snippet<[FormNode]>;
  }
  let { stepDetailForm, emptyLabel, renderNode }: Props = $props();
</script>

<section class="pe-col pe-col-detail">
  {#if stepDetailForm && stepDetailForm.length > 0}
    <div class="pe-detail-body">
      {#if renderNode}
        {#each stepDetailForm as n, i (n.id ?? ('n' + i))}
          {@render renderNode(n)}
        {/each}
      {:else}
        <p class="pe-muted">Missing renderNode snippet — detail form cannot render.</p>
      {/if}
    </div>
  {:else}
    <div class="pe-detail-empty">
      <p class="pe-muted">{emptyLabel ?? 'Seleziona uno step per modificarne i parametri.'}</p>
    </div>
  {/if}
</section>
