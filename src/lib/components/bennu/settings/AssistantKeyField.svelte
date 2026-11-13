<script lang="ts">
  /**
   * One key in Imber's vault, as a settings row: a password field, Save, and whether a key is set (never its value).
   * The API key and the optional Admin key are both this; `provider` is the vault's account for the key.
   */
  import { onMount } from 'svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { deleteAssistantKey, hasAssistantKey, storeAssistantKey } from '$lib/ipc/bennu/assistant';

  let {
    provider,
    label,
    description,
    placeholder = 'sk-ant-…',
    onchange,
  }: {
    provider: string;
    label: string;
    description: string;
    placeholder?: string;
    /** A key was stored or removed. */
    onchange?: () => void;
  } = $props();

  let key = $state('');
  let saved = $state(false);

  onMount(() => {
    hasAssistantKey(provider).then((has) => (saved = has)).catch(() => {});
  });

  async function save() {
    if (!key.trim()) return;
    try {
      await storeAssistantKey(key, provider);
      saved = true;
      key = '';
      toastStore.show('Key stored in Imber’s vault.', 'success');
      onchange?.();
    } catch (e) {
      toastStore.show(`Could not store the key: ${e}`, 'error');
    }
  }

  async function remove() {
    try {
      await deleteAssistantKey(provider);
      saved = false;
      onchange?.();
    } catch (e) {
      toastStore.show(`Could not remove the key: ${e}`, 'error');
    }
  }
</script>

<FormRow {label} {description}>
  <div class="key">
    <Input type="password" bind:value={key} placeholder={saved ? '••••••••••••••••' : placeholder}
           onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') void save(); }} />
    <Button variant="primary" size="sm" disabled={!key.trim()} onclick={save}>Save</Button>
    {#if saved}
      <Badge variant="tone" tone="success" size="sm">Set</Badge>
      <Button variant="ghost" size="sm" onclick={remove}>Remove</Button>
    {:else}
      <Badge variant="tone" tone="neutral" size="sm">Not set</Badge>
    {/if}
  </div>
</FormRow>

<style>
  .key { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .key :global(input) { min-width: 220px; }
</style>
