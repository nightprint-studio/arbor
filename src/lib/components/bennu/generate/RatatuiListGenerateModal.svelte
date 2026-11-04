<script lang="ts">
  /**
   * Generate a ratatui list or table screen at the caret of a Rust file: the row type, selection state with
   * wrapping next / previous / first / last, the highlighted render and the navigation keys.
   */
  import { onMount } from 'svelte';
  import { Table2 } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import FieldRows from './FieldRows.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    name: string;
    kind: string;
    columns: { name: string; ty: string }[];
    title: string;
    vim_keys: boolean;
    activate: boolean;
    ratatui_minor: number;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('ratatui-list', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const KINDS = [
    { value: 'table', label: 'Table', description: 'A column per field' },
    { value: 'list', label: 'List', description: 'The first field' },
  ];
</script>

<GeneratorShell {session} icon={Table2} title="ratatui list or table screen" {onClose} height="620px">
  {#snippet form()}
    <FormSection label="Screen" first>
      <RadioGroup bind:value={o.kind} options={KINDS} appearance="segment" size="sm" block />
      <FormField label="Name"><Input block size="sm" ariaLabel="Screen name" bind:value={o.name} /></FormField>
      <FormField label="Block title"><Input block size="sm" ariaLabel="Title" bind:value={o.title} /></FormField>
    </FormSection>

    <FormSection label={o.kind === 'table' ? 'Columns' : 'Row fields'}>
      <FieldRows bind:items={o.columns} ariaLabel="Column" />
    </FormSection>

    <FormSection label="Keys">
      <Toggle bind:checked={o.vim_keys} size="sm" label="j / k / g / G" description="Besides the arrows, Home and End" />
      <Toggle bind:checked={o.activate} size="sm" label="Enter opens the row" />
      <FormField label="ratatui version">
        <NumberStepper value={o.ratatui_minor} min={20} max={60} size="sm" prefix="0." onchange={(v) => (o.ratatui_minor = v)} ariaLabel="ratatui minor version" />
      </FormField>
    </FormSection>
  {/snippet}
</GeneratorShell>
