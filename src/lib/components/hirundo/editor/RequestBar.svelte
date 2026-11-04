<script lang="ts">
  /**
   * RequestBar — method, URL, Send. The URL shows the base plus the enabled params as one string, the way a
   * browser would; typing after `?` edits the params. Send says its shortcut in the tooltip (the pattern
   * every button of the app uses) rather than carrying a key cap that makes it wide.
   */
  import { Send, Square } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { hirundoStore, type RequestTab } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoCommands } from '../hirundo-commands';
  import { METHODS } from '$lib/types/hirundo';
  import UrlField from './UrlField.svelte';
  import { applyUrl, methodColor, urlWithQuery } from '../request-model';

  let { tab }: { tab: RequestTab } = $props();

  function onUrl(text: string) {
    applyUrl(tab.request, text);
    hirundoStore.touch(tab);
  }
</script>

<div class="rb">
  <div class="rb-method" style:--method={methodColor(tab.request.method)}>
    <Select
      value={tab.request.method}
      options={METHODS.map((m) => ({ value: m, label: m }))}
      onchange={(v) => { tab.request.method = v as typeof tab.request.method; hirundoStore.touch(tab); }}
    />
  </div>
  <div class="rb-url">
    <UrlField value={urlWithQuery(tab.request)} path={tab.path} oninput={onUrl} onsend={hirundoCommands.send} />
  </div>
  {#if tab.sending}
    <Button variant="danger" tooltip={{ content: 'Cancel the request', shortcut: 'Esc' }} onclick={hirundoCommands.cancel}>
      {#snippet iconStart()}<Square size={13} />{/snippet}
      Cancel
    </Button>
  {:else}
    <Button variant="primary" tooltip={{ content: 'Send the request', shortcut: 'Ctrl+Enter' }} onclick={hirundoCommands.send}>
      {#snippet iconStart()}<Send size={13} />{/snippet}
      Send
    </Button>
  {/if}
</div>

<style>
  .rb { display: flex; gap: 6px; align-items: center; padding: 8px 12px 6px; }
  .rb-method { width: 108px; flex: none; }
  .rb-method :global(button) { color: var(--method); font-family: var(--font-mono); font-weight: 600; }
  .rb-url { flex: 1; min-width: 0; }
</style>
