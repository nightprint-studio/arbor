<script lang="ts">
  /**
   * VariableBanner — the variables this request uses that nothing defines, above the editor, each a button
   * that opens "Create variable" for it. It is the answer to "how do I give `{{baseUrl}}` a value?": the
   * request says what it needs, and one click defines it in the layer you pick. Nothing shows when everything
   * resolves.
   */
  import { Variable } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import type { RequestTab } from '$lib/stores/hirundo/workspace.svelte';
  import { pathParamNames, requestTexts, undefinedVariables } from './completions';

  let { tab, known }: { tab: RequestTab; known: ReadonlySet<string> } = $props();

  const r = $derived(tab.request);
  const missing = $derived(undefinedVariables(requestTexts(r), new Set([...known, ...pathParamNames(r)])));
</script>

{#if missing.length}
  <div class="vb">
    <Alert variant="warning" compact title="To fill in" text="These variables have no value yet — the request will not be sent until they do.">
      {#snippet actions()}
        {#each missing.slice(0, 6) as name (name)}
          <Button size="sm" variant="outline" tooltip={`Define {{${name}}}: choose where it lives and its value`} onclick={() => hirundoUiStore.open({ kind: 'create-variable', name, path: tab.path })}>
            {#snippet iconStart()}<Variable size={12} />{/snippet}
            {name}
          </Button>
        {/each}
      {/snippet}
    </Alert>
  </div>
{/if}

<style>
  .vb { padding: 0 12px 8px; }
</style>
