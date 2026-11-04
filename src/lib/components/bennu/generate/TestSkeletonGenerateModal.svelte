<script lang="ts">
  /**
   * Generate a test class for the class at the caret — a MockMvc slice for a controller (one call per
   * known endpoint), a Mockito unit test for a service, a `@DataJpaTest` for a repository, or plain JUnit —
   * written under `src/test/java` in the same package.
   */
  import { onMount } from 'svelte';
  import { FlaskConical } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import FieldChecklist from './FieldChecklist.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    kind: string;
    junit: string;
    assertions: string;
    methods: string[];
    class_name: string;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('test-skeleton', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const KINDS = [
    { value: 'auto', label: 'Auto (from the class)' },
    { value: 'controller', label: 'Controller — @WebMvcTest + MockMvc' },
    { value: 'service', label: 'Service — Mockito' },
    { value: 'repository', label: 'Repository — @DataJpaTest' },
    { value: 'plain', label: 'Plain JUnit' },
  ];
  const JUNIT = [{ value: '5', label: 'JUnit 5' }, { value: '4', label: 'JUnit 4' }];
  const ASSERTIONS = [{ value: 'assertj', label: 'AssertJ' }, { value: 'junit', label: 'JUnit' }];

  const items = $derived((session.opened?.class?.methods ?? []).map((m) => ({ name: m })));
</script>

<GeneratorShell {session} icon={FlaskConical} title="Test class" {onClose}>
  {#snippet form()}
    <FormSection label="Test" first>
      <FormField label="Kind"><Select bind:value={o.kind} options={KINDS} size="sm" /></FormField>
      <FormField label="JUnit"><RadioGroup bind:value={o.junit} options={JUNIT} appearance="segment" size="sm" block /></FormField>
      <FormField label="Assertions"><RadioGroup bind:value={o.assertions} options={ASSERTIONS} appearance="segment" size="sm" block /></FormField>
    </FormSection>

    <FormSection label="Methods" hint="A controller test covers its known endpoints instead">
      <FieldChecklist {items} bind:selected={o.methods} ariaLabel="Methods" empty="No methods found in the class." maxHeight={200} />
    </FormSection>

    <FormSection label="Class">
      <Input block size="sm" placeholder={`${session.opened?.class?.name ?? 'Class'}Test`} ariaLabel="Class name" bind:value={o.class_name} />
    </FormSection>
  {/snippet}
</GeneratorShell>
