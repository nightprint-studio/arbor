<script lang="ts">
  /**
   * RequestToolbar — the icon row under the URL. Every button is a verb that also lives in the command
   * palette and (where it has one) a shortcut; the tooltip says which.
   */
  import { ArrowLeftRight, Braces, Code2, Copy, CornerUpRight, Link, Sparkles, FileDown, GitCompare, History, ListChecks, Pin, Save, Settings2, Variable } from 'lucide-svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import type { RequestTab } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoCommands as c } from '../hirundo-commands';

  let { tab }: { tab: RequestTab } = $props();
  const dirty = $derived(hirundoStore.dirty(tab));
</script>

<div class="tb" role="toolbar" aria-label="Request actions" tabindex="-1">
  <IconButton tooltip="Save" shortcut="Ctrl+S" size={28} active={dirty} onclick={c.save}><Save size={14} /></IconButton>
  <IconButton tooltip="Save as…" size={28} onclick={c.saveAs}><FileDown size={14} /></IconButton>
  <IconButton tooltip="Duplicate" shortcut="Ctrl+D" size={28} onclick={c.duplicate}><Copy size={14} /></IconButton>
  <i class="tb-sep"></i>
  <IconButton tooltip="cURL — paste one, or copy this request as one" shortcut="Ctrl+Shift+U" size={28} onclick={c.curl}><ArrowLeftRight size={14} /></IconButton>
  <IconButton tooltip="Generate code" size={28} onclick={() => hirundoUiStore.open({ kind: 'generate-code' })}><Code2 size={14} /></IconButton>
  <i class="tb-sep"></i>
  <IconButton tooltip="Bulk edit" shortcut="Ctrl+Shift+B" size={28} active={hirundoUiStore.bulk} onclick={c.toggleBulk}><ListChecks size={14} /></IconButton>
  <IconButton tooltip="Format body" shortcut="Ctrl+Shift+F" size={28} onclick={c.formatBody}><Braces size={14} /></IconButton>
  <IconButton tooltip="Resolved variables" shortcut="Ctrl+Shift+V" size={28} onclick={c.variables}><Variable size={14} /></IconButton>
  <i class="tb-sep"></i>
  <IconButton tooltip="Compare with the previous response" size={28} disabled={!tab.previous} onclick={c.diff}><GitCompare size={14} /></IconButton>
  <IconButton tooltip="Save the response as an example (in Docs)" size={28} disabled={!tab.response} onclick={c.saveResponseAsExample}><Pin size={14} /></IconButton>
  <IconButton tooltip="History" shortcut="Ctrl+5" size={28} onclick={() => hirundoUiStore.toggleRight('history')}><History size={14} /></IconButton>
  <i class="tb-sep"></i>
  <IconButton tooltip="Go to the controller (Bennu)" size={28} onclick={() => void c.goToController()}><CornerUpRight size={14} /></IconButton>
  <IconButton tooltip="Copy the request file path" size={28} onclick={c.copyLink}><Link size={14} /></IconButton>
  <IconButton tooltip="Ask Imber about this request" size={28} onclick={() => void c.askImber()}><Sparkles size={14} /></IconButton>
  <i class="tb-sep"></i>
  <IconButton tooltip="Request settings — timeout, redirects, TLS" size={28} onclick={c.requestSettings}><Settings2 size={14} /></IconButton>
</div>

<style>
  .tb { display: flex; align-items: center; gap: 2px; padding: 2px 12px 8px; flex-wrap: wrap; }
  .tb-sep { width: 1px; height: 16px; background: var(--border); margin: 0 4px; }
</style>
