<script lang="ts">
  /**
   * Generate a JPA AttributeConverter — dates as text, durations as millis, enums by name or code,
   * booleans as Y/N, lists as CSV, JSON columns, AES-GCM encrypted strings, or any pair of expressions —
   * registered with `autoApply` or wired onto a field of the class at the caret with `@Convert`.
   */
  import { onMount } from 'svelte';
  import { ArrowLeftRight } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    kind: string;
    java_type: string;
    db_type: string;
    class_name: string;
    package: string;
    auto_apply: boolean;
    null_safe: boolean;
    field: string;
    separator: string;
    true_value: string;
    false_value: string;
    enum_mode: string;
    code_getter: string;
    key_env: string;
    to_expr: string;
    from_expr: string;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('attribute-converter', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const KINDS = [
    { value: 'date-string', label: 'Date / time → text (ISO-8601)' },
    { value: 'duration-millis', label: 'Duration → milliseconds' },
    { value: 'enum', label: 'Enum → name or code' },
    { value: 'boolean-char', label: 'Boolean → Y / N' },
    { value: 'csv-list', label: 'List<String> → separated text' },
    { value: 'json', label: 'Object → JSON column' },
    { value: 'encrypted', label: 'String → encrypted (AES-GCM)' },
    { value: 'custom', label: 'Custom expressions' },
  ];
  const DATE_TYPES = ['LocalDate', 'LocalDateTime', 'LocalTime', 'Instant', 'OffsetDateTime', 'ZonedDateTime'].map((v) => ({ value: v, label: v }));
  const ENUM_MODES = [
    { value: 'name', label: 'Name' },
    { value: 'code', label: 'Code accessor' },
  ];
  const CODE_TYPES = ['String', 'Integer', 'Long'].map((v) => ({ value: v, label: v }));
  /** What a kind fixes about the entity-side type; empty means the user types it. */
  const FIXED_TYPE: Record<string, string> = {
    'duration-millis': 'Duration',
    'csv-list': 'List<String>',
    'boolean-char': 'Boolean',
    encrypted: 'String',
  };

  const fieldOptions = $derived([
    { value: '', label: 'No field — just the converter' },
    ...(session.opened?.class?.fields ?? []).map((f) => ({ value: f.name, label: `${f.name}  ·  ${f.type_name}` })),
  ]);

  function pickKind(kind: string) {
    if (kind === 'date-string' && !DATE_TYPES.some((t) => t.value === o.java_type)) o.java_type = 'LocalDate';
    else if (FIXED_TYPE[kind]) o.java_type = FIXED_TYPE[kind];
    else if (DATE_TYPES.some((t) => t.value === o.java_type) || Object.values(FIXED_TYPE).includes(o.java_type)) o.java_type = '';
  }
</script>

<GeneratorShell {session} icon={ArrowLeftRight} title="JPA attribute converter" {onClose}>
  {#snippet form()}
    <FormSection label="Conversion" first>
      <FormField label="Kind">
        <Select bind:value={o.kind} options={KINDS} size="sm" onchange={pickKind} />
      </FormField>

      {#if o.kind === 'date-string'}
        <FormField label="Type"><Select bind:value={o.java_type} options={DATE_TYPES} size="sm" /></FormField>
      {:else if o.kind === 'enum'}
        <FormField label="Enum"><Input block size="sm" placeholder="OrderStatus" ariaLabel="Enum type" bind:value={o.java_type} /></FormField>
        <FormField label="Stored as"><RadioGroup bind:value={o.enum_mode} options={ENUM_MODES} appearance="segment" size="sm" block /></FormField>
        {#if o.enum_mode === 'code'}
          <FormField label="Accessor"><Input block size="sm" ariaLabel="Code accessor" bind:value={o.code_getter} /></FormField>
          <FormField label="Column type"><Select bind:value={o.db_type} options={CODE_TYPES} size="sm" /></FormField>
        {/if}
      {:else if o.kind === 'json'}
        <FormField label="Type" hint="Map<String, Object>, List<Item>, a class…">
          <Input block size="sm" ariaLabel="Type" bind:value={o.java_type} />
        </FormField>
      {:else if o.kind === 'csv-list'}
        <FormField label="Separator"><Input narrow size="sm" ariaLabel="Separator" bind:value={o.separator} /></FormField>
      {:else if o.kind === 'boolean-char'}
        <FormField label="True / false stored as">
          <div class="cv-pair">
            <Input narrow size="sm" ariaLabel="True value" bind:value={o.true_value} />
            <Input narrow size="sm" ariaLabel="False value" bind:value={o.false_value} />
          </div>
        </FormField>
      {:else if o.kind === 'encrypted'}
        <FormField label="Key environment variable" hint="Holds a Base64-encoded AES key"><Input block size="sm" ariaLabel="Key variable" bind:value={o.key_env} /></FormField>
      {:else if o.kind === 'custom'}
        <FormField label="Entity type"><Input block size="sm" ariaLabel="Entity type" bind:value={o.java_type} /></FormField>
        <FormField label="Column type"><Input block size="sm" ariaLabel="Column type" bind:value={o.db_type} /></FormField>
        <FormField label="To the column" hint="`attribute` is the entity value"><Input block size="sm" ariaLabel="To column" bind:value={o.to_expr} /></FormField>
        <FormField label="To the entity" hint="`dbData` is the column value"><Input block size="sm" ariaLabel="To entity" bind:value={o.from_expr} /></FormField>
      {/if}
    </FormSection>

    <FormSection label="Use">
      <Toggle bind:checked={o.auto_apply} size="sm" label="Apply to every attribute of this type" description="autoApply = true" />
      {#if !o.auto_apply}
        <FormField label="Annotate field" hint="Adds @Convert to the field in the open class">
          <Select bind:value={o.field} options={fieldOptions} size="sm" searchable />
        </FormField>
      {/if}
      <Toggle bind:checked={o.null_safe} size="sm" label="Pass null through" />
    </FormSection>

    <FormSection label="Class">
      <FormField label="Name"><Input block size="sm" placeholder="derived from the conversion" ariaLabel="Class name" bind:value={o.class_name} /></FormField>
      <FormField label="Package"><Input block size="sm" ariaLabel="Package" bind:value={o.package} /></FormField>
    </FormSection>
  {/snippet}
</GeneratorShell>

<style>
  .cv-pair { display: flex; gap: 6px; }
</style>
