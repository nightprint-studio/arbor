<script lang="ts">
  /**
   * Above a built-in template open in the editor: why it cannot be typed into, and the way to a copy of it
   * that can.
   */
  import { Copy } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { templateFileParts, templateKindOfPath } from './template-paths';
  import BennuNewTemplateModal from './BennuNewTemplateModal.svelte';

  let { path }: { path: string } = $props();

  const kind = $derived(templateKindOfPath(path));
  const name = $derived(templateFileParts(path).name);
  let copying = $state(false);
</script>

<div class="bt">
  <Alert variant="info" compact text="A built-in template, read-only — copy it to change it.">
    {#snippet actions()}
      {#if kind}
        <Button variant="ghost" size="xs" onclick={() => (copying = true)}>
          {#snippet iconStart()}<Copy size={12} />{/snippet}
          Copy into a new template…
        </Button>
      {/if}
    {/snippet}
  </Alert>
</div>

{#if copying && kind}
  <BennuNewTemplateModal {kind} from={name} onClose={() => (copying = false)} />
{/if}

<style>
  .bt { padding: 4px 8px; }
</style>
