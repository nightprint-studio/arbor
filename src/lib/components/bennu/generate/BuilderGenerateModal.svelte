<script lang="ts">
  /**
   * Generate a builder for the class at the caret — a nested fluent `Builder` inserted into the class
   * (with `toBuilder()` and required-field checks), or Lombok's `@Builder` wired onto it.
   */
  import { onMount } from 'svelte';
  import { Hammer } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import FieldChecklist from './FieldChecklist.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    style: string;
    fields: string[];
    to_builder: boolean;
    required: string[];
    factory: string;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('builder', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const STYLES = [
    { value: 'nested', label: 'Nested Builder', description: 'Written into the class' },
    { value: 'lombok', label: 'Lombok @Builder', description: 'An annotation on the class' },
  ];

  const items = $derived(
    (session.opened?.class?.fields ?? []).map((f) => ({ name: f.name, detail: f.type_name, badge: f.is_final ? 'final' : undefined })),
  );
  const selectable = $derived(items.filter((i) => o.fields.includes(i.name)));
</script>

<GeneratorShell {session} icon={Hammer} title="Builder" {onClose}>
  {#snippet form()}
    <FormSection label="Style" first>
      <RadioGroup bind:value={o.style} options={STYLES} appearance="segment" size="sm" block />
    </FormSection>

    {#if o.style === 'nested'}
      <FormSection label="Fields">
        <FieldChecklist {items} bind:selected={o.fields} ariaLabel="Fields" empty="Put the caret in a class." maxHeight={170} />
      </FormSection>
      <FormSection label="Required" hint="build() rejects null for these">
        <FieldChecklist items={selectable} bind:selected={o.required} ariaLabel="Required fields" empty="Select fields first." maxHeight={130} />
      </FormSection>
      <FormSection label="Options">
        <Toggle bind:checked={o.to_builder} size="sm" label="toBuilder()" description="Copy an instance back into a builder" />
        <FormField label="Factory method"><Input narrow size="sm" ariaLabel="Factory method" bind:value={o.factory} /></FormField>
      </FormSection>
    {/if}
  {/snippet}
</GeneratorShell>
