<script lang="ts">
  /**
   * Generate a ratatui key map at the caret of a Rust file: an `Action` enum, `from_key` over crossterm's
   * `KeyEvent`, and labels and descriptions for a help line — from key specs such as `q, ctrl+c` or `f5`.
   */
  import { onMount } from 'svelte';
  import { Keyboard, Plus, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    enum_name: string;
    actions: { name: string; keys: string; description: string }[];
    help: boolean;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('ratatui-keymap', host);
  onMount(() => void session.open());
  const o = $derived(session.options);
</script>

<GeneratorShell {session} icon={Keyboard} title="ratatui key map" {onClose} height="620px">
  {#snippet form()}
    <FormSection label="Enum" first>
      <FormField label="Name"><Input block size="sm" ariaLabel="Enum name" bind:value={o.enum_name} /></FormField>
      <Toggle bind:checked={o.help} size="sm" label="Help labels" description="ALL, key_label() and description()" />
    </FormSection>

    <FormSection label="Actions" hint="Keys: q, esc, enter, space, up, f5, ctrl+s, alt+x, shift+tab — separate alternatives with commas">
      {#each o.actions as a, i (i)}
        <div class="km-row">
          <Input size="sm" block placeholder="Action" ariaLabel={`Action ${i + 1}`} bind:value={a.name} />
          <Input size="sm" block placeholder="q, ctrl+c" ariaLabel={`Keys ${i + 1}`} bind:value={a.keys} />
          <IconButton tooltip="Remove" size={12} onclick={() => (o.actions = o.actions.filter((_, n) => n !== i))}><X size={12} /></IconButton>
          <div class="km-desc"><Input size="sm" block placeholder="Description" ariaLabel={`Description ${i + 1}`} bind:value={a.description} /></div>
        </div>
      {/each}
      <div>
        <Button variant="ghost" size="sm" onclick={() => (o.actions = [...o.actions, { name: '', keys: '', description: '' }])}>
          {#snippet iconStart()}<Plus size={12} />{/snippet}
          Action
        </Button>
      </div>
    </FormSection>
  {/snippet}
</GeneratorShell>

<style>
  .km-row { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto; gap: 4px; align-items: center; padding-bottom: 4px; }
  .km-desc { grid-column: 1 / -1; }
</style>
