<script lang="ts">
  /**
   * RequestToolbar — the icon row under the URL. Every button is a verb that also lives in the command
   * palette and (where it has one) a shortcut; the tooltip says which. Save lives in the title bar, bulk edit
   * in the tables, history in the activity bar — each where the thing it acts on is.
   */
  import { ArrowLeftRight, Braces, Code2, Copy, CornerUpRight, GitCompare, ListTree, Pin, Settings2 } from 'lucide-svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import type { RequestTab } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoDetected } from '$lib/stores/hirundo/detected.svelte';
  import { hirundoCommands as c, detectedEndpointOf } from '../hirundo-commands';
  import type { VarInfo } from './completions';
  import VariablesUsed from './VariablesUsed.svelte';

  let { tab, vars }: { tab: RequestTab; vars: readonly VarInfo[] } = $props();
  /** The controller button only exists when this request is one of the endpoints Bennu found. */
  const fromCode = $derived(!!hirundoDetected.groups.length && !!detectedEndpointOf(tab));
</script>

<div class="tb" role="toolbar" aria-label="Request actions" tabindex="-1">
  <IconButton tooltip="Duplicate" shortcut="Ctrl+D" size={28} onclick={c.duplicate}><Copy size={14} /></IconButton>
  <i class="tb-sep"></i>
  <IconButton tooltip="cURL — paste one, or copy this request as one" shortcut="Ctrl+Shift+U" size={28} onclick={c.curl}><ArrowLeftRight size={14} /></IconButton>
  <IconButton tooltip="Generate code" size={28} onclick={() => hirundoUiStore.open({ kind: 'generate-code' })}><Code2 size={14} /></IconButton>
  <i class="tb-sep"></i>
  <IconButton tooltip="Format body" shortcut="Ctrl+Shift+F" size={28} onclick={c.formatBody}><Braces size={14} /></IconButton>
  <VariablesUsed {tab} {vars} />
  <IconButton tooltip="Resolved variables — the value of each, from where" shortcut="Ctrl+Shift+V" size={28} onclick={c.variables}><ListTree size={14} /></IconButton>
  <i class="tb-sep"></i>
  <IconButton tooltip="Compare with the previous response" size={28} disabled={!tab.previous} onclick={c.diff}><GitCompare size={14} /></IconButton>
  <IconButton tooltip="Save the response as an example (in Docs)" size={28} disabled={!tab.response} onclick={c.saveResponseAsExample}><Pin size={14} /></IconButton>
  {#if fromCode}
    <i class="tb-sep"></i>
    <IconButton tooltip="Go to the controller (Bennu found this endpoint in the code)" size={28} onclick={() => void c.goToController()}><CornerUpRight size={14} /></IconButton>
  {/if}
  <i class="tb-sep"></i>
  <IconButton tooltip="Request settings — timeout, redirects, TLS" size={28} onclick={c.requestSettings}><Settings2 size={14} /></IconButton>
</div>

<style>
  .tb { display: flex; align-items: center; gap: 2px; padding: 2px 12px 8px; flex-wrap: wrap; }
  .tb-sep { width: 1px; height: 16px; background: var(--border); margin: 0 4px; }
</style>
