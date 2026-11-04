<script lang="ts">
  /**
   * Generate a Tauri command at the caret of a Rust file — the `#[tauri::command]` function, its
   * `generate_handler!` line, and a typed `invoke()` wrapper for the frontend (arguments in camelCase, the
   * way Tauri hands them over).
   */
  import { onMount } from 'svelte';
  import { Plug, Plus, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Param { name: string; ty: string }
  interface Options {
    name: string;
    params: Param[];
    returns: string;
    fallible: boolean;
    async_fn: boolean;
    state: string;
    app_handle: boolean;
    window: boolean;
    public: boolean;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('tauri-command', host);
  onMount(() => void session.open());
  const o = $derived(session.options);
</script>

<GeneratorShell {session} icon={Plug} title="Tauri command" {onClose} height="620px">
  {#snippet form()}
    <FormSection label="Command" first>
      <FormField label="Name"><Input block size="sm" ariaLabel="Command name" bind:value={o.name} /></FormField>
      <FormField label="Returns"><Input block size="sm" placeholder="() for nothing" ariaLabel="Return type" bind:value={o.returns} /></FormField>
      <Toggle bind:checked={o.fallible} size="sm" label="Result<T, String>" />
      <Toggle bind:checked={o.async_fn} size="sm" label="async" />
      <Toggle bind:checked={o.public} size="sm" label="pub" />
    </FormSection>

    <FormSection label="Arguments">
      {#each o.params as p, i (i)}
        <div class="tc-row">
          <Input size="sm" block placeholder="name" ariaLabel={`Argument ${i + 1} name`} bind:value={p.name} />
          <Input size="sm" block placeholder="Type" ariaLabel={`Argument ${i + 1} type`} bind:value={p.ty} />
          <IconButton tooltip="Remove" size={12} onclick={() => (o.params = o.params.filter((_, n) => n !== i))}><X size={12} /></IconButton>
        </div>
      {/each}
      <div>
        <Button variant="ghost" size="sm" onclick={() => (o.params = [...o.params, { name: '', ty: 'String' }])}>
          {#snippet iconStart()}<Plus size={12} />{/snippet}
          Argument
        </Button>
      </div>
    </FormSection>

    <FormSection label="Injected">
      <FormField label="Managed state" hint="Taken as tauri::State<'_, T>">
        <Input block size="sm" placeholder="AppState" ariaLabel="Managed state" bind:value={o.state} />
      </FormField>
      <Toggle bind:checked={o.app_handle} size="sm" label="AppHandle" />
      <Toggle bind:checked={o.window} size="sm" label="Window" />
    </FormSection>
  {/snippet}
</GeneratorShell>

<style>
  .tc-row { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr) auto; gap: 4px; align-items: center; }
</style>
