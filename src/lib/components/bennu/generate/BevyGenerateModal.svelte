<script lang="ts">
  /**
   * Generate a Bevy skeleton at the caret of a Rust file — a component, resource, event, state, a system
   * with its query and resources, or a plugin that registers them.
   */
  import { onMount } from 'svelte';
  import { Boxes, Plus, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import GeneratorShell from './GeneratorShell.svelte';
  import StringListEditor from './StringListEditor.svelte';

  interface Field { name: string; ty: string }
  interface Options {
    kind: string;
    name: string;
    fields: Field[];
    query: string[];
    filters: string[];
    resources: string[];
    commands: boolean;
    update_systems: string[];
    startup_systems: string[];
    init_resources: string[];
    events: string[];
    bevy_minor: number;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('bevy-skeleton', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const KINDS = [
    { value: 'component', label: 'Component' },
    { value: 'resource', label: 'Resource' },
    { value: 'event', label: 'Event / message' },
    { value: 'state', label: 'State' },
    { value: 'system', label: 'System' },
    { value: 'plugin', label: 'Plugin' },
  ];
  const data = $derived(['component', 'resource', 'event'].includes(o.kind));
</script>

<GeneratorShell {session} icon={Boxes} title="Bevy component, system or plugin" {onClose} height="640px">
  {#snippet form()}
    <FormSection label="What" first>
      <FormField label="Kind"><Select bind:value={o.kind} options={KINDS} size="sm" /></FormField>
      <FormField label={o.kind === 'system' ? 'Function name' : o.kind === 'plugin' ? 'Plugin name' : 'Type name'}>
        <Input block size="sm" ariaLabel="Name" bind:value={o.name} />
      </FormField>
      <FormField label="Bevy version" hint="0.17 renames buffered events to messages">
        <NumberStepper value={o.bevy_minor} min={10} max={40} size="sm" onchange={(v) => (o.bevy_minor = v)} ariaLabel="Bevy minor version" prefix="0." />
      </FormField>
    </FormSection>

    {#if data || o.kind === 'state'}
      <FormSection label={o.kind === 'state' ? 'Variants' : 'Fields'}>
        {#each o.fields as f, i (i)}
          <div class="bv-row" class:single={o.kind === 'state'}>
            <Input size="sm" block placeholder={o.kind === 'state' ? 'variant' : 'name'} ariaLabel={`Field ${i + 1} name`} bind:value={f.name} />
            {#if o.kind !== 'state'}
              <Input size="sm" block placeholder="f32" ariaLabel={`Field ${i + 1} type`} bind:value={f.ty} />
            {/if}
            <IconButton tooltip="Remove" size={12} onclick={() => (o.fields = o.fields.filter((_, n) => n !== i))}><X size={12} /></IconButton>
          </div>
        {/each}
        <div>
          <Button variant="ghost" size="sm" onclick={() => (o.fields = [...o.fields, { name: '', ty: 'f32' }])}>
            {#snippet iconStart()}<Plus size={12} />{/snippet}
            {o.kind === 'state' ? 'Variant' : 'Field'}
          </Button>
        </div>
      </FormSection>
    {/if}

    {#if o.kind === 'system'}
      <FormSection label="Query" hint="&Transform, &mut Velocity …">
        <StringListEditor bind:items={o.query} placeholder="&mut Transform" addLabel="Component" ariaLabel="Component" />
      </FormSection>
      <FormSection label="Filters" hint="Player, or !Dead">
        <StringListEditor bind:items={o.filters} placeholder="Player" addLabel="Filter" ariaLabel="Filter" />
      </FormSection>
      <FormSection label="Resources" hint="Time, or mut Score">
        <StringListEditor bind:items={o.resources} placeholder="Time" addLabel="Resource" ariaLabel="Resource" />
      </FormSection>
      <Toggle bind:checked={o.commands} size="sm" label="Commands" />
    {/if}

    {#if o.kind === 'plugin'}
      <FormSection label="Update systems">
        <StringListEditor bind:items={o.update_systems} placeholder="move_player" addLabel="System" ariaLabel="Update system" />
      </FormSection>
      <FormSection label="Startup systems">
        <StringListEditor bind:items={o.startup_systems} placeholder="setup" addLabel="System" ariaLabel="Startup system" />
      </FormSection>
      <FormSection label="Resources">
        <StringListEditor bind:items={o.init_resources} placeholder="Score" addLabel="Resource" ariaLabel="Resource" />
      </FormSection>
      <FormSection label="Events">
        <StringListEditor bind:items={o.events} placeholder="Hit" addLabel="Event" ariaLabel="Event" />
      </FormSection>
    {/if}
  {/snippet}
</GeneratorShell>

<style>
  .bv-row { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto; gap: 4px; align-items: center; }
  .bv-row.single { grid-template-columns: minmax(0, 1fr) auto; }
</style>
