<!--
  FormNodePipelineEditor — host wrapper around the dedicated
  PluginPipelineEditor 3-column workflow editor. The editor renders
  detail-form children via the recursive `renderNode` snippet so any
  FormNode tree (with switches, fields, tabs) works inside a step's
  detail panel.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import PluginPipelineEditor from '$lib/components/plugins/PluginPipelineEditor.svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import type { FormNode } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';

  interface Props {
    node:       FormNode;
    ctx:        FormNodeCtx;
    renderNode: Snippet<[FormNode]>;
  }
  let { node, ctx, renderNode }: Props = $props();

  const pe = $derived(node as any);
</script>

<div class="pf-pipeline-editor-host {(node as any).class ?? ''}" style={(node as any).style}>
  <PluginPipelineEditor
    editor={pe}
    {renderNode}
    iconMap={PLUGIN_ICONS}
    fireAction={(action, extra) => ctx.handleButtonAction(action, false, extra)}
  />
</div>
