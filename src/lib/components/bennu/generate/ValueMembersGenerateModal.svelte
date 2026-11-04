<script lang="ts">
  /**
   * Generate equals, hashCode and toString for the class at the caret — over every field, the id alone
   * (the default for a JPA entity), or a chosen set; with a pattern-matching or `getClass()` check.
   */
  import { onMount } from 'svelte';
  import { Hash } from 'lucide-svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import FieldChecklist from './FieldChecklist.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    equals: boolean;
    hash_code: boolean;
    to_string: boolean;
    fields: string[];
    class_check: string;
    use_getters: boolean;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('value-members', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const CHECKS = [
    { value: 'auto', label: 'Auto', description: 'Pattern matching on Java 16+, getClass() for entities' },
    { value: 'instanceof', label: 'instanceof' },
    { value: 'get-class', label: 'getClass()' },
  ];

  const items = $derived(
    (session.opened?.class?.fields ?? []).map((f) => ({ name: f.name, detail: f.type_name, badge: f.id ? 'id' : undefined })),
  );
</script>

<GeneratorShell {session} icon={Hash} title="equals, hashCode and toString" {onClose}>
  {#snippet form()}
    <FormSection label="Methods" first>
      <Toggle bind:checked={o.equals} size="sm" label="equals" />
      <Toggle bind:checked={o.hash_code} size="sm" label="hashCode" />
      <Toggle bind:checked={o.to_string} size="sm" label="toString" />
    </FormSection>

    <FormSection label="Fields">
      <FieldChecklist {items} bind:selected={o.fields} ariaLabel="Fields" empty="Put the caret in a class." maxHeight={220} />
    </FormSection>

    <FormSection label="Options">
      {#if o.equals}
        <RadioGroup bind:value={o.class_check} options={CHECKS} appearance="segment" size="sm" block />
      {/if}
      <Toggle bind:checked={o.use_getters} size="sm" label="Read through getters" />
    </FormSection>
  {/snippet}
</GeneratorShell>
