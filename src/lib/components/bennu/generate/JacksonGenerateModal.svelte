<script lang="ts">
  /**
   * Generate a Jackson serializer / deserializer for a type — as a JSON object field by field or as one
   * string — registered through Spring Boot's `@JsonComponent`, a `Module` bean, or `@JsonSerialize` /
   * `@JsonDeserialize` on the type (or a field of the class at the caret).
   */
  import { onMount } from 'svelte';
  import { Braces } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    type_name: string;
    type_package: string;
    mode: string;
    representation: string;
    registration: string;
    naming: string;
    include_nulls: boolean;
    to_string_expr: string;
    from_string_expr: string;
    class_name: string;
    package: string;
    field: string;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('jackson-serde', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const MODES = [
    { value: 'both', label: 'Both' },
    { value: 'ser', label: 'Serializer' },
    { value: 'de', label: 'Deserializer' },
  ];
  const REPRESENTATIONS = [
    { value: 'object', label: 'JSON object', description: 'One property per field' },
    { value: 'string', label: 'JSON string', description: 'A single text value' },
  ];
  const REGISTRATIONS = [
    { value: 'json-component', label: '@JsonComponent (Spring Boot)' },
    { value: 'module-bean', label: 'Module bean' },
    { value: 'annotation', label: '@JsonSerialize / @JsonDeserialize' },
    { value: 'none', label: 'I register it myself' },
  ];
  const NAMINGS = [
    { value: 'as-is', label: 'As in Java' },
    { value: 'snake_case', label: 'snake_case' },
    { value: 'kebab-case', label: 'kebab-case' },
    { value: 'camelCase', label: 'camelCase' },
  ];

  const fieldOptions = $derived([
    { value: '', label: `On the type ${o.type_name || ''}`.trim() },
    ...(session.opened?.class?.fields ?? []).map((f) => ({ value: f.name, label: `On field ${f.name}` })),
  ]);
</script>

<GeneratorShell {session} icon={Braces} title="Jackson serializer / deserializer" {onClose}>
  {#snippet form()}
    <FormSection label="Type" first>
      <FormField label="Type to (de)serialize" hint="The class at the caret by default">
        <Input block size="sm" ariaLabel="Type" bind:value={o.type_name} />
      </FormField>
      <FormField label="Generate"><RadioGroup bind:value={o.mode} options={MODES} appearance="segment" size="sm" block /></FormField>
      <FormField label="Written as"><RadioGroup bind:value={o.representation} options={REPRESENTATIONS} appearance="segment" size="sm" block /></FormField>
    </FormSection>

    {#if o.representation === 'object'}
      <FormSection label="Properties">
        <FormField label="Naming"><Select bind:value={o.naming} options={NAMINGS} size="sm" /></FormField>
        <Toggle bind:checked={o.include_nulls} size="sm" label="Write null properties" />
      </FormSection>
    {:else}
      <FormSection label="Text">
        <FormField label="Write" hint="`value` is the object"><Input block size="sm" ariaLabel="Write expression" bind:value={o.to_string_expr} /></FormField>
        <FormField label="Read" hint="`text` is the string"><Input block size="sm" ariaLabel="Read expression" bind:value={o.from_string_expr} /></FormField>
      </FormSection>
    {/if}

    <FormSection label="Registration">
      <FormField label="Register with"><Select bind:value={o.registration} options={REGISTRATIONS} size="sm" /></FormField>
      {#if o.registration === 'annotation' && (session.opened?.class?.fields.length ?? 0) > 0}
        <FormField label="Annotate"><Select bind:value={o.field} options={fieldOptions} size="sm" searchable /></FormField>
      {/if}
    </FormSection>

    <FormSection label="Class">
      <FormField label="Name"><Input block size="sm" placeholder={`${o.type_name || 'Type'}Json`} ariaLabel="Class name" bind:value={o.class_name} /></FormField>
      <FormField label="Package"><Input block size="sm" ariaLabel="Package" bind:value={o.package} /></FormField>
    </FormSection>
  {/snippet}
</GeneratorShell>
