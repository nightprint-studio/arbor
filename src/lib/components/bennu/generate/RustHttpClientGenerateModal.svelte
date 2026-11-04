<script lang="ts">
  /**
   * Generate a Rust HTTP client from an endpoint description — reqwest (async or blocking) or ureq, serde
   * structs for the payloads, an error enum (thiserror or hand-written) and bearer / basic / API-key
   * authentication, written at the caret of the open Rust file.
   */
  import { onMount } from 'svelte';
  import { Globe, Plus, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import FieldRows from './FieldRows.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Field { name: string; ty: string }
  interface Endpoint { name: string; method: string; path: string; query: Field[]; headers: Field[]; body: string; returns: string }
  interface Options {
    library: string;
    client_name: string;
    error_name: string;
    endpoints: Endpoint[];
    structs: { name: string; fields: Field[] }[];
    auth: string;
    api_key_header: string;
    errors: string;
    timeout_seconds: number;
    rename_all: string;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('rust-http-client', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const LIBRARIES = [
    { value: 'reqwest', label: 'reqwest (async)' },
    { value: 'reqwest-blocking', label: 'reqwest (blocking)' },
    { value: 'ureq', label: 'ureq (blocking, light)' },
  ];
  const AUTH = [
    { value: 'none', label: 'None' },
    { value: 'bearer', label: 'Bearer' },
    { value: 'basic', label: 'Basic' },
    { value: 'api-key', label: 'API key' },
  ];
  const ERRORS = [
    { value: 'thiserror', label: 'thiserror' },
    { value: 'std', label: 'Hand-written' },
  ];
  const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map((m) => ({ value: m, label: m }));
  const RENAMES = ['', 'camelCase', 'snake_case', 'PascalCase', 'kebab-case', 'SCREAMING_SNAKE_CASE'].map((v) => ({ value: v, label: v || 'Field names as written' }));

  function addEndpoint() {
    o.endpoints = [...o.endpoints, { name: '', method: 'GET', path: '/', query: [], headers: [], body: '', returns: '' }];
  }
  function addStruct() {
    o.structs = [...o.structs, { name: '', fields: [] }];
  }
</script>

<GeneratorShell {session} icon={Globe} title="Rust HTTP client" {onClose}>
  {#snippet form()}
    <FormSection label="Client" first>
      <FormField label="Library"><Select bind:value={o.library} options={LIBRARIES} size="sm" /></FormField>
      <FormField label="Client type"><Input block size="sm" ariaLabel="Client type" bind:value={o.client_name} /></FormField>
      <FormField label="Error type"><Input block size="sm" ariaLabel="Error type" bind:value={o.error_name} /></FormField>
      <FormField label="Errors written with"><RadioGroup bind:value={o.errors} options={ERRORS} appearance="segment" size="sm" block /></FormField>
      <FormField label="Authentication"><RadioGroup bind:value={o.auth} options={AUTH} appearance="segment" size="sm" block /></FormField>
      {#if o.auth === 'api-key'}
        <FormField label="Header"><Input block size="sm" ariaLabel="API key header" bind:value={o.api_key_header} /></FormField>
      {/if}
      <FormField label="Timeout (seconds)">
        <NumberStepper value={o.timeout_seconds} min={1} max={3600} size="sm" onchange={(v) => (o.timeout_seconds = v)} ariaLabel="Timeout" />
      </FormField>
    </FormSection>

    <FormSection label="Payload types">
      <FormField label="JSON field names"><Select bind:value={o.rename_all} options={RENAMES} size="sm" /></FormField>
      {#each o.structs as s, i (i)}
        <div class="rc-card">
          <div class="rc-head">
            <Input size="sm" block placeholder="StructName" ariaLabel={`Struct ${i + 1}`} bind:value={s.name} />
            <IconButton tooltip="Remove struct" size={12} onclick={() => (o.structs = o.structs.filter((_, n) => n !== i))}><X size={12} /></IconButton>
          </div>
          <FieldRows bind:items={s.fields} ariaLabel={`Struct ${i + 1} field`} />
        </div>
      {/each}
      <div>
        <Button variant="ghost" size="sm" onclick={addStruct}>
          {#snippet iconStart()}<Plus size={12} />{/snippet}
          Struct
        </Button>
      </div>
    </FormSection>

    <FormSection label="Endpoints">
      {#each o.endpoints as e, i (i)}
        <div class="rc-card">
          <div class="rc-head">
            <Select bind:value={e.method} options={METHODS} size="sm" narrow ariaLabel={`Method ${i + 1}`} />
            <Input size="sm" block placeholder="/orders/{'{id}'}" ariaLabel={`Path ${i + 1}`} bind:value={e.path} />
            <IconButton tooltip="Remove endpoint" size={12} onclick={() => (o.endpoints = o.endpoints.filter((_, n) => n !== i))}><X size={12} /></IconButton>
          </div>
          <Input size="sm" block placeholder="method_name" ariaLabel={`Method name ${i + 1}`} bind:value={e.name} />
          <div class="rc-pair">
            <Input size="sm" block placeholder="Body type" ariaLabel={`Body ${i + 1}`} bind:value={e.body} />
            <Input size="sm" block placeholder="Returns" ariaLabel={`Returns ${i + 1}`} bind:value={e.returns} />
          </div>
          <FieldRows bind:items={e.query} ariaLabel={`Endpoint ${i + 1} query`} addLabel="Query" namePlaceholder="param" typePlaceholder="Option<&str>" />
          <FieldRows bind:items={e.headers} ariaLabel={`Endpoint ${i + 1} header`} addLabel="Header" namePlaceholder="X-Header" typePlaceholder="&str" defaultType="&str" />
        </div>
      {/each}
      <div>
        <Button variant="ghost" size="sm" onclick={addEndpoint}>
          {#snippet iconStart()}<Plus size={12} />{/snippet}
          Endpoint
        </Button>
      </div>
    </FormSection>
  {/snippet}
</GeneratorShell>

<style>
  .rc-card { display: flex; flex-direction: column; gap: 6px; padding: 7px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); }
  .rc-head { display: flex; gap: 4px; align-items: center; }
  .rc-head > :global(.input-wrap) { flex: 1; min-width: 0; }
  .rc-pair { display: grid; grid-template-columns: 1fr 1fr; gap: 4px; }
</style>
