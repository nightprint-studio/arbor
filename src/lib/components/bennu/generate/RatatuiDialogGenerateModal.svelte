<script lang="ts">
  /**
   * Generate a ratatui overlay at the caret of a Rust file: a centred popup, a yes/no confirmation that
   * reports the answer, or a single-line text input with a cursor — drawn over the screen with `Clear`.
   */
  import { onMount } from 'svelte';
  import { MessageSquare } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    name: string;
    kind: string;
    title: string;
    width_percent: number;
    helper: boolean;
    ratatui_minor: number;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('ratatui-dialog', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const KINDS = [
    { value: 'confirm', label: 'Confirm', description: 'Yes / No, returns the answer' },
    { value: 'popup', label: 'Popup', description: 'A message over the screen' },
    { value: 'input', label: 'Text input', description: 'One line with a cursor' },
  ];
</script>

<GeneratorShell {session} icon={MessageSquare} title="ratatui popup, confirmation or input" {onClose} height="600px">
  {#snippet form()}
    <FormSection label="Kind" first>
      <RadioGroup bind:value={o.kind} options={KINDS} appearance="segment" size="sm" block />
      <FormField label="Type name"><Input block size="sm" ariaLabel="Type name" bind:value={o.name} /></FormField>
      <FormField label="Title"><Input block size="sm" ariaLabel="Title" bind:value={o.title} /></FormField>
    </FormSection>

    <FormSection label="Layout">
      <FormField label="Width (% of the screen)">
        <NumberStepper value={o.width_percent} min={10} max={100} step={5} size="sm" onchange={(v) => (o.width_percent = v)} ariaLabel="Width" />
      </FormField>
      <Toggle bind:checked={o.helper} size="sm" label="Write centered_rect" description="Leave off when the file already has one" />
      <FormField label="ratatui version">
        <NumberStepper value={o.ratatui_minor} min={20} max={60} size="sm" prefix="0." onchange={(v) => (o.ratatui_minor = v)} ariaLabel="ratatui minor version" />
      </FormField>
    </FormSection>
  {/snippet}
</GeneratorShell>
