<script lang="ts">
  /**
   * TypeEditor — one environment **type**: its name, colour, whether it asks before sending a method that
   * changes things, and the variables every environment of the type starts from.
   */
  import ColorSwatch from '$lib/components/shared/ui/ColorSwatch.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import type { EnvType } from '$lib/types/hirundo';
  import KeyValueEditor from '../editor/KeyValueEditor.svelte';
  import SecretValues from './SecretValues.svelte';

  interface Props {
    type: EnvType;
    /** A new type's id follows its name until it is saved; after that it is a file name and stays. */
    isNew: boolean;
    onrename: (name: string) => void;
  }

  let { type = $bindable(), isNew, onrename }: Props = $props();

  let nameEl = $state<HTMLInputElement | undefined>(undefined);
  $effect(() => { if (isNew) { nameEl?.focus(); nameEl?.select(); } });
</script>

<div class="te">
  <FormField label="Name">
    <Input value={type.name} bind:element={nameEl} oninput={onrename} />
  </FormField>
  {#if !isNew}<p class="te-id">id <code>{type.id}</code></p>{/if}
  <FormField label="Colour" hint="Tints the title bar and the request bar of every environment of this type.">
    <div class="te-color">
      <ColorSwatch color={type.color} chipSize={26} onchange={(c) => (type.color = c)} />
      <Input value={type.color} narrow oninput={(v) => (type.color = v)} />
    </div>
  </FormField>
  <Toggle
    checked={type.confirm_mutating}
    label="Ask before changing anything"
    description="POST, PUT, PATCH and DELETE need a confirmation here. Reading is never asked about."
    onchange={(v) => (type.confirm_mutating = v)}
  />
  <FormField label="Variables" hint="Defaults for every environment of this type; an environment overrides them one by one.">
    <KeyValueEditor rows={type.vars} allowSecret onchange={(rows) => (type.vars = rows)} />
  </FormField>
  <SecretValues scope="type" id={type.id} rows={type.vars} />
</div>

<style>
  .te { display: flex; flex-direction: column; gap: 12px; padding: 12px; }
  .te-color { display: flex; align-items: center; gap: 8px; }
  .te-id { margin: -6px 0 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .te-id code { font-family: var(--font-mono); }
</style>
