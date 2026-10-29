<script lang="ts">
  /**
   * One field of a generator's form, drawn from its description — the wizard never knows which
   * generator it is showing, only which kinds of field exist (text, select, toggle, multi).
   */
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import OptionPicker from '$lib/components/shared/ui/OptionPicker.svelte';
  import type { FieldValue, GeneratorField } from '$lib/ipc/bennu/new-project';

  let {
    field,
    value,
    error = null,
    loading = false,
    onchange,
  }: {
    field: GeneratorField;
    value: FieldValue | undefined;
    error?: string | null;
    /** The options are still being fetched (remote generators). */
    loading?: boolean;
    onchange: (value: FieldValue) => void;
  } = $props();

  const text = $derived(typeof value === 'string' ? value : '');
  const flag = $derived(value === true);
  const list = $derived(Array.isArray(value) ? value : []);
  const selectOptions = $derived(field.options.map((o) => ({ value: o.value, label: o.label })));
</script>

{#if field.kind === 'toggle'}
  <div class="toggle-cell">
    <Toggle checked={flag} label={field.label} description={field.hint} onchange={(v) => onchange(v)} />
    {#if error}<span class="err">{error}</span>{/if}
  </div>
{:else if field.kind === 'multi'}
  <FormField label={field.label} hint={field.hint} {error}>
    <OptionPicker
      options={field.options}
      selected={list}
      ariaLabel={field.label}
      placeholder={`Search ${field.label.toLowerCase()}…`}
      {loading}
      height={230}
      onchange={(v) => onchange(v)}
    />
  </FormField>
{:else if field.kind === 'select'}
  <FormField label={field.label} hint={field.hint} {error}>
    {#if field.options.length}
      <Select value={text} options={selectOptions} fill searchable={field.options.length > 8} ariaLabel={field.label} onchange={(v) => onchange(v)} />
    {:else}
      <Input value={text} block placeholder={field.placeholder} {error} oninput={(v) => onchange(v)} />
    {/if}
  </FormField>
{:else}
  <FormField label={field.label} hint={field.hint} {error}>
    <Input value={text} block placeholder={field.placeholder} {error} oninput={(v) => onchange(v)} />
  </FormField>
{/if}

<style>
  .toggle-cell { display: flex; flex-direction: column; gap: 3px; padding-top: 4px; }
  .err { font-size: var(--font-size-xs); color: var(--error); }
</style>
