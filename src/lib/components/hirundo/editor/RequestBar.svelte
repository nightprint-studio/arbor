<script lang="ts">
  /**
   * RequestBar — method, URL, Send. The URL shows the base plus the enabled params as one string, the way a
   * browser would; typing after `?` edits the params. Send says its shortcut in the tooltip (the pattern
   * every button of the app uses) rather than carrying a key cap that makes it wide.
   */
  import { Send, Square } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import MethodSelect from './MethodSelect.svelte';
  import { hirundoStore, type RequestTab } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoCommands } from '../hirundo-commands';
  import UrlField from './UrlField.svelte';
  import { pathParamNames } from './completions';
  import { applyUrl, urlWithQuery } from '../request-model';

  let { tab }: { tab: RequestTab } = $props();

  function onUrl(text: string) {
    applyUrl(tab.request, text);
    hirundoStore.touch(tab);
  }
</script>

<div class="rb">
  <MethodSelect value={tab.request.method} onchange={(m) => { tab.request.method = m; hirundoStore.touch(tab); }} />
  <div class="rb-url">
    <UrlField value={urlWithQuery(tab.request)} path={tab.path} extraKnown={pathParamNames(tab.request)} oninput={onUrl} onsend={hirundoCommands.send} />
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
  .rb { display: flex; gap: 8px; align-items: center; padding: 12px 12px 8px; }
  /* One height for the three controls of the bar: method, URL, Send. */
  .rb :global(.btn) { height: 32px; }
  .rb-url { flex: 1; min-width: 0; }
</style>
