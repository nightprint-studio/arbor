<script lang="ts">
  /**
   * Generate a ratatui widget at the caret of a Rust file: a `Widget` (also for `&T`), a `StatefulWidget`
   * with its state, or a component with key handling and its own `render`.
   */
  import { onMount } from 'svelte';
  import { LayoutPanelLeft } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import FieldRows from './FieldRows.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    name: string;
    kind: string;
    fields: { name: string; ty: string }[];
    bordered: boolean;
    by_ref: boolean;
    keys: boolean;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('ratatui-widget', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const KINDS = [
    { value: 'widget', label: 'Widget', description: 'Rendered into a Buffer' },
    { value: 'stateful', label: 'Stateful', description: 'With a state the caller owns' },
    { value: 'component', label: 'Component', description: 'Key handling and render(frame, area)' },
  ];
</script>

<GeneratorShell {session} icon={LayoutPanelLeft} title="ratatui widget or component" {onClose} height="620px">
  {#snippet form()}
    <FormSection label="Kind" first>
      <RadioGroup bind:value={o.kind} options={KINDS} appearance="segment" size="sm" block />
      <FormField label="Name"><Input block size="sm" ariaLabel="Widget name" bind:value={o.name} /></FormField>
    </FormSection>

    <FormSection label="Fields">
      <FieldRows bind:items={o.fields} ariaLabel="Field" />
    </FormSection>

    <FormSection label="Options">
      <Toggle bind:checked={o.bordered} size="sm" label="Bordered block with a title" />
      {#if o.kind === 'widget'}
        <Toggle bind:checked={o.by_ref} size="sm" label="Also implement for &T" description="Drawn without a clone" />
      {/if}
      {#if o.kind === 'component'}
        <Toggle bind:checked={o.keys} size="sm" label="handle_key" />
      {/if}
    </FormSection>
  {/snippet}
</GeneratorShell>
