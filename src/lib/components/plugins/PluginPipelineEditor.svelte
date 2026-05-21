<!--
  PluginPipelineEditor.svelte

  First-class Svelte component for editing a pipeline-style profile from a
  plugin. Renders a 3-column layout (palette · sequence · detail) that is far
  more compact and native-feeling than what the generic form renderer can
  produce out of primitives.

  The component is stateless w.r.t. the profile data: it receives everything
  as props and emits plugin actions for every mutation via `firePluginAction`.
  Search filtering is handled client-side for responsiveness; all structural
  mutations (add / remove / move / select) round-trip through the plugin.

  After the Phase 4 god-object refactor the body lives in three sub-renderers
  under `pipeline-editor/`:
    PipelinePalette.svelte   — search input + collapsible op categories
    PipelineSequence.svelte  — breadcrumb + stages + recursive step rows
    PipelineStepDetail.svelte — detail form (renders parent's renderNode)

  Each sub-component owns its private state (palette: search query + cat
  collapsed map; sequence: per-branch expression buffer) and talks to the
  plugin via the same `fireAction` callback this dispatcher receives.

  Expected `editor` shape — see `pipeline-editor/types.ts` for the full
  surface (stages, operations, search_query, selected_step_id,
  selected_stage_id, step_detail_form, empty_label, breadcrumb, actions).
-->

<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { FormNode } from '$lib/types/plugin';

  import PipelinePalette    from './pipeline-editor/PipelinePalette.svelte';
  import PipelineSequence   from './pipeline-editor/PipelineSequence.svelte';
  import PipelineStepDetail from './pipeline-editor/PipelineStepDetail.svelte';

  import type { EditorProps, FireAction } from './pipeline-editor/types';
  import './pipeline-editor/pipeline-editor-styles.css';

  interface Props {
    editor:      EditorProps;
    renderNode?: Snippet<[FormNode]>;
    iconMap?:    Record<string, any>;
    fireAction:  FireAction;
  }
  let { editor, renderNode, iconMap, fireAction }: Props = $props();

  // Lua's empty `{}` arrives as a JS object, not an array — `?? []` only
  // catches null/undefined, so for-of crashes on otherwise-empty profiles.
  // Coerce both array-shaped props once at the top, then forward the safe
  // derivatives to the sub-components. See feedback_lua_empty_table_arrays.md.
  const stages     = $derived(Array.isArray(editor.stages)     ? editor.stages     : []);
  const operations = $derived(Array.isArray(editor.operations) ? editor.operations : []);
</script>

<div class="pe-root">
  <PipelinePalette
    searchQuery={editor.search_query}
    {operations}
    actions={editor.actions}
    {iconMap}
    {fireAction}
  />

  <PipelineSequence
    {stages}
    {operations}
    selectedStepId={editor.selected_step_id}
    selectedStageId={editor.selected_stage_id}
    selectedBranchId={editor.selected_branch_id}
    breadcrumb={editor.breadcrumb}
    hideAddStage={editor.hide_add_stage}
    actions={editor.actions}
    {iconMap}
    {fireAction}
  />

  <PipelineStepDetail
    stepDetailForm={editor.step_detail_form}
    emptyLabel={editor.empty_label}
    {renderNode}
  />
</div>
