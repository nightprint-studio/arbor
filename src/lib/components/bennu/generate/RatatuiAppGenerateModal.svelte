<script lang="ts">
  /**
   * Generate a ratatui application at the caret of a Rust file: terminal init/restore, the event loop, an
   * `App` with its state, `draw` and key handling — for ratatui 0.28+ or the manual crossterm set-up before.
   */
  import { onMount } from 'svelte';
  import { Terminal } from 'lucide-svelte';
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
    title: string;
    layout: string;
    tick_ms: number;
    mouse: boolean;
    state: { name: string; ty: string }[];
    ratatui_minor: number;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('ratatui-app', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const LAYOUTS = [
    { value: 'header-body-footer', label: 'Header · body · footer' },
    { value: 'single', label: 'One block' },
  ];
</script>

<GeneratorShell {session} icon={Terminal} title="ratatui application" {onClose} height="640px">
  {#snippet form()}
    <FormSection label="App" first>
      <FormField label="Type name"><Input block size="sm" ariaLabel="App type" bind:value={o.name} /></FormField>
      <FormField label="Title"><Input block size="sm" ariaLabel="Title" bind:value={o.title} /></FormField>
      <FormField label="Layout"><RadioGroup bind:value={o.layout} options={LAYOUTS} appearance="segment" size="sm" block /></FormField>
    </FormSection>

    <FormSection label="Loop">
      <FormField label="Tick (ms)" hint="0 waits for input only">
        <NumberStepper value={o.tick_ms} min={0} max={5000} step={50} size="sm" onchange={(v) => (o.tick_ms = v)} ariaLabel="Tick" />
      </FormField>
      <Toggle bind:checked={o.mouse} size="sm" label="Mouse events" />
      <FormField label="ratatui version">
        <NumberStepper value={o.ratatui_minor} min={20} max={60} size="sm" prefix="0." onchange={(v) => (o.ratatui_minor = v)} ariaLabel="ratatui minor version" />
      </FormField>
    </FormSection>

    <FormSection label="State">
      <FieldRows bind:items={o.state} ariaLabel="State field" />
    </FormSection>
  {/snippet}
</GeneratorShell>
