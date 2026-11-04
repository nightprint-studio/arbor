<script lang="ts">
  /**
   * SecretValues — the values of the variables marked `@secret` in one layer. A secret has no value in any
   * file: it goes from this field straight to the OS keychain through the shell (never through the backend),
   * under an account named by project, layer and variable — so staging's `token` and prod's are two secrets.
   */
  import { Check } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { deleteSecret, hasSecret, secretAccount, storeSecret } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { SECRET_MARK, type KeyValue, type VarScope } from '$lib/types/hirundo';

  interface Props {
    scope: VarScope;
    /** The layer's id: environment id, type id, collection path… */
    id: string;
    rows: KeyValue[];
  }

  let { scope, id, rows }: Props = $props();

  const names = $derived(rows.filter((r) => r.value === SECRET_MARK && r.key.trim()).map((r) => r.key.trim()));
  let present = $state<Record<string, boolean>>({});
  let typed = $state<Record<string, string>>({});

  async function account(name: string): Promise<string> {
    return secretAccount(hirundoStore.project ?? '', scope, id, name);
  }

  $effect(() => {
    const list = names;
    void (async () => {
      const next: Record<string, boolean> = {};
      for (const name of list) {
        try { next[name] = await hasSecret(await account(name)); } catch { next[name] = false; }
      }
      present = next;
    })();
  });

  async function store(name: string) {
    try {
      await storeSecret(await account(name), typed[name] ?? '');
      present[name] = !!typed[name];
      typed[name] = '';
      toastStore.show(present[name] ? `Secret “${name}” stored in the keychain.` : `Secret “${name}” removed.`, 'success');
    } catch (e) {
      toastStore.show(String(e), 'error');
    }
  }

  async function forget(name: string) {
    try {
      await deleteSecret(await account(name));
      present[name] = false;
    } catch (e) {
      toastStore.show(String(e), 'error');
    }
  }
</script>

{#if names.length}
  <div class="sv">
    {#each names as name (name)}
      <FormField label={name}>
        <div class="sv-row">
          <Input type="password" value={typed[name] ?? ''} placeholder={present[name] ? '•••••••• (stored — type to replace)' : 'Value (goes to the keychain)'} oninput={(v) => (typed[name] = v)} />
          <Button size="sm" variant="primary" disabled={!typed[name]} onclick={() => void store(name)}>Store</Button>
          {#if present[name]}
            <Badge variant="tone" tone="success" size="sm">{#snippet icon()}<Check size={10} />{/snippet}stored</Badge>
            <Button size="sm" variant="ghost" onclick={() => void forget(name)}>Forget</Button>
          {:else}
            <Badge variant="tone" tone="warning" size="sm" label="not set" />
          {/if}
        </div>
      </FormField>
    {/each}
  </div>
{/if}

<style>
  .sv { display: flex; flex-direction: column; gap: 8px; padding: 8px 12px 12px; border-top: 1px solid var(--border-subtle); }
  .sv-row { display: flex; align-items: center; gap: 6px; }
  .sv-row > :global(:first-child) { flex: 1; min-width: 0; }
</style>
