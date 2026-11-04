<script lang="ts">
  /** EnvEditor — one environment: its name, its type, and the variables that override the type's. */
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import type { EnvType, Environment } from '$lib/types/hirundo';
  import KeyValueEditor from '../editor/KeyValueEditor.svelte';
  import SecretValues from './SecretValues.svelte';

  interface Props {
    env: Environment;
    types: EnvType[];
    isNew: boolean;
    onrename: (name: string) => void;
  }

  let { env = $bindable(), types, isNew, onrename }: Props = $props();

  let nameEl = $state<HTMLInputElement | undefined>(undefined);
  // A new environment starts with its name selected: type to replace it.
  $effect(() => { if (isNew) { nameEl?.focus(); nameEl?.select(); } });

  const options = $derived([{ value: '', label: 'No type' }, ...types.map((t) => ({ value: t.id, label: t.name }))]);
  const inherited = $derived(types.find((t) => t.id === env.type_id)?.vars.filter((v) => v.key) ?? []);
</script>

<div class="ee">
  <div class="ee-row">
    <FormField label="Name"><Input value={env.name} bind:element={nameEl} oninput={onrename} /></FormField>
    <FormField label="Type"><Select value={env.type_id} {options} onchange={(v) => (env.type_id = v)} /></FormField>
  </div>
  {#if !isNew}<p class="ee-id">id <code>{env.id}</code></p>{/if}
  {#if inherited.length}
    <p class="ee-inh">From the type: {inherited.map((v) => v.key).join(', ')} — set a variable of the same name below to override it.</p>
  {/if}
  <FormField label="Variables">
    <KeyValueEditor rows={env.vars} allowSecret onchange={(rows) => (env.vars = rows)} />
  </FormField>
  <SecretValues scope="environment" id={env.id} rows={env.vars} />
</div>

<style>
  .ee { display: flex; flex-direction: column; gap: 12px; padding: 12px; }
  .ee-row { display: grid; grid-template-columns: 1fr 200px; gap: 12px; }
  .ee-id, .ee-inh { margin: -6px 0 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .ee-id code { font-family: var(--font-mono); }
</style>
