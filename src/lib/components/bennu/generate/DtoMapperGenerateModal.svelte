<script lang="ts">
  /**
   * Generate a DTO and its mapper from the entity at the caret — a record, Lombok or plain class for the
   * DTO, and a MapStruct interface or a hand-written mapper (static, or a Spring component).
   */
  import { onMount } from 'svelte';
  import { Shuffle } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import FieldChecklist from './FieldChecklist.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    dto_name: string;
    mapper_name: string;
    package: string;
    entity_package: string;
    entity_name: string;
    fields: string[];
    dto_kind: string;
    mapper: string;
    to_entity: boolean;
    list_methods: boolean;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('dto-mapper', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const DTO_KINDS = [
    { value: 'auto', label: 'Auto (record, Lombok or class)' },
    { value: 'record', label: 'Record' },
    { value: 'lombok', label: 'Lombok @Data class' },
    { value: 'class', label: 'Plain class' },
  ];
  const MAPPERS = [
    { value: 'auto', label: 'Auto (MapStruct when present)' },
    { value: 'mapstruct', label: 'MapStruct interface' },
    { value: 'component', label: 'Spring @Component' },
    { value: 'manual', label: 'Static utility class' },
    { value: 'none', label: 'No mapper' },
  ];

  const items = $derived(
    (session.opened?.class?.fields ?? []).map((f) => ({ name: f.name, detail: f.type_name, badge: f.id ? 'id' : undefined })),
  );
</script>

<GeneratorShell {session} icon={Shuffle} title="DTO and mapper" {onClose}>
  {#snippet form()}
    <FormSection label="Fields" first>
      <FieldChecklist {items} bind:selected={o.fields} ariaLabel="Fields" empty="Put the caret in the entity to map." maxHeight={200} />
    </FormSection>

    <FormSection label="DTO">
      <FormField label="Name"><Input block size="sm" ariaLabel="DTO name" bind:value={o.dto_name} /></FormField>
      <FormField label="Shape"><Select bind:value={o.dto_kind} options={DTO_KINDS} size="sm" /></FormField>
    </FormSection>

    <FormSection label="Mapper">
      <FormField label="Style"><Select bind:value={o.mapper} options={MAPPERS} size="sm" /></FormField>
      {#if o.mapper !== 'none'}
        <FormField label="Name"><Input block size="sm" ariaLabel="Mapper name" bind:value={o.mapper_name} /></FormField>
        <Toggle bind:checked={o.to_entity} size="sm" label="Map back to the entity" />
        <Toggle bind:checked={o.list_methods} size="sm" label="List methods" />
      {/if}
    </FormSection>

    <FormSection label="Package">
      <Input block size="sm" ariaLabel="Package" bind:value={o.package} />
    </FormSection>
  {/snippet}
</GeneratorShell>
