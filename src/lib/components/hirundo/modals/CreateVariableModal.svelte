<script lang="ts">
  /**
   * CreateVariableModal — "this `{{name}}` has no value": say where it should live and what it is. The choice
   * of layer is the whole point of the cascade made visible: this environment (staging's baseUrl), every
   * environment of a type, the collection (shared by everyone who clones the repo), or this machine only
   * (never committed). A secret goes to the OS keychain and leaves no value in any file.
   */
  import { onMount } from 'svelte';
  import { Variable } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { environmentSave, envTypesSave, localSave, metaGet, metaSave, secretAccount, storeSecret } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { SECRET_MARK, type KeyValue, type VarScope } from '$lib/types/hirundo';

  interface Props {
    /** The name to define (from the `{{name}}` that has no value). */
    name?: string;
    /** The path of the request it is used in: its collection becomes a choice. */
    requestPath?: string | null;
    onClose: () => void;
  }

  let { name: initialName = '', requestPath = null, onClose }: Props = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(initialName);
  let value = $state('');
  let secret = $state(false);
  let saving = $state(false);
  let nameEl = $state<HTMLInputElement | undefined>(undefined);
  let valueEl = $state<HTMLInputElement | undefined>(undefined);

  const env = $derived(hirundoStore.activeEnv);
  const type = $derived(hirundoStore.activeEnvType);
  const collection = $derived(requestPath ? requestPath.split('/')[0] : hirundoStore.tree[0]?.path ?? null);

  const options = $derived([
    ...(env ? [{ value: 'environment', label: `This environment — ${env.name}`, description: 'Shared in the repository; only this environment has it' }] : []),
    ...(type ? [{ value: 'type', label: `Every ${type.name} environment`, description: 'The same value for all environments of this type' }] : []),
    ...(collection ? [{ value: 'collection', label: `The collection — ${collection}`, description: 'Every request in it, in every environment' }] : []),
    { value: 'local', label: 'This machine only', description: 'Not committed — your own token, your own port' },
  ]);
  // svelte-ignore state_referenced_locally
  let scope = $state<string>(env ? 'environment' : collection ? 'collection' : 'local');

  const valid = $derived(/^[\w.-]+$/.test(name.trim()));

  onMount(() => (initialName ? valueEl : nameEl)?.focus());

  async function add(rows: KeyValue[], row: KeyValue): Promise<KeyValue[]> {
    return [...rows.filter((r) => r.key !== row.key), row];
  }

  async function create() {
    const project = hirundoStore.project;
    if (!project || !valid || saving) return;
    saving = true;
    const key = name.trim();
    const row: KeyValue = { key, value: secret ? SECRET_MARK : value, description: '', enabled: true };
    try {
      let layerId = '';
      if (scope === 'environment' && env) {
        layerId = env.id;
        await environmentSave(project, { ...env, vars: await add(env.vars, row) });
      } else if (scope === 'type' && type) {
        layerId = type.id;
        await envTypesSave(project, hirundoStore.envs.types.map((t) => (t.id === type.id ? { ...t, vars: [...t.vars.filter((v) => v.key !== key), row] } : t)));
      } else if (scope === 'collection' && collection) {
        layerId = collection;
        const meta = await metaGet(project, collection);
        await metaSave(project, collection, { ...meta, vars: await add(meta.vars, row) });
      } else {
        layerId = 'local';
        const local = hirundoStore.envs.local;
        await localSave(project, { ...local, vars: await add(local.vars, row) });
      }
      if (secret && value) await storeSecret(await secretAccount(project, scope as VarScope, layerId, key), value);
      await hirundoStore.refreshEnvs();
      toastStore.show(`“${key}” defined.`, 'success');
      onClose();
    } catch (e) {
      toastStore.show(String(e), 'error');
    } finally {
      saving = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void create(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="560px" height="520px" ariaLabel="Create variable">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Variable size={14} />
      <span class="modal-title">Create variable</span>
    </ModalHeader>
  {/snippet}

  <div class="cv">
    <FormSection label="Variable" hint="Used in a request as {'{{'}name{'}}'}." first>
      <FormField label="Name"><Input value={name} bind:element={nameEl} placeholder="baseUrl" oninput={(v) => (name = v)} /></FormField>
      <FormField label="Value" hint={secret ? 'Goes to the OS keychain, never to a file.' : undefined}>
        <Input value={value} bind:element={valueEl} type={secret ? 'password' : 'text'} placeholder={name === 'baseUrl' ? 'https://api.example.com' : 'Value'} oninput={(v) => (value = v)} />
      </FormField>
      <Toggle checked={secret} label="Secret" description="A token or a password: kept in the keychain" onchange={(v) => (secret = v)} />
    </FormSection>
    <FormSection label="Where it lives" hint="The first layer that defines a name wins: this machine, then the environment, its type, then the collection.">
      <RadioGroup value={scope} {options} size="sm" onchange={(v) => (scope = String(v))} />
    </FormSection>
  </div>

  {#snippet footer()}
    <ModalFooter hint="to create">
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" loading={saving} disabled={!valid} onclick={() => void create()}>Create variable</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .cv { display: flex; flex-direction: column; padding: 4px 16px 12px; }
</style>
