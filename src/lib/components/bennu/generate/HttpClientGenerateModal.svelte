<script lang="ts">
  /**
   * Generate an HTTP client from Spring endpoints — pick the controller methods of the app (from the
   * request-mapping index) and get a typed client for RestClient, WebClient, RestTemplate, an
   * `@HttpExchange` interface, OpenFeign, `java.net.http`, OkHttp or Retrofit, with the DTOs, the
   * authentication, the error handling and the base-URL property.
   */
  import { onMount } from 'svelte';
  import { Network } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import FieldChecklist, { type ChecklistItem } from './FieldChecklist.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    library: string;
    endpoints: string[];
    class_name: string;
    package: string;
    base_url_property: string;
    base_url_default: string;
    auth: string;
    api_key_header: string;
    dtos: string;
    errors: string;
    timeout_seconds: number;
    reactive: boolean;
    records: boolean;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('http-client', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const LIBRARIES = [
    { value: 'restclient', label: 'RestClient (Spring 6.1+)' },
    { value: 'webclient', label: 'WebClient (reactive)' },
    { value: 'resttemplate', label: 'RestTemplate' },
    { value: 'httpinterface', label: '@HttpExchange interface' },
    { value: 'feign', label: 'OpenFeign' },
    { value: 'jdk', label: 'java.net.http.HttpClient' },
    { value: 'okhttp', label: 'OkHttp' },
    { value: 'retrofit', label: 'Retrofit' },
  ];
  const AUTH = [
    { value: 'none', label: 'None' },
    { value: 'bearer', label: 'Bearer' },
    { value: 'basic', label: 'Basic' },
    { value: 'api-key', label: 'API key' },
  ];
  const DTOS = [
    { value: 'reuse', label: 'Reuse the server’s', description: 'Import the controller’s types — a shared module' },
    { value: 'generate', label: 'Generate copies', description: 'Records or classes next to the client' },
  ];
  const ERRORS = [
    { value: 'exception', label: 'Exception', description: 'A generated exception with the status and body' },
    { value: 'default', label: 'Library default' },
  ];

  const items = $derived<ChecklistItem[]>(
    (session.opened?.endpoints ?? []).map((e) => ({
      name: e.id,
      label: e.path,
      badge: e.methods[0] ?? 'GET',
      detail: e.handler,
      group: e.controller,
    })),
  );
</script>

<GeneratorShell {session} icon={Network} title="HTTP client from Spring endpoints" {onClose}>
  {#snippet form()}
    <FormSection label="Library" first>
      <Select bind:value={o.library} options={LIBRARIES} size="sm" />
      {#if o.library === 'webclient'}
        <Toggle bind:checked={o.reactive} size="sm" label="Return Mono / Flux" description="Off blocks and returns plain values" />
      {/if}
    </FormSection>

    <FormSection label="Endpoints">
      {#if items.length === 0}
        <FormField hint="No Spring request mappings were found in this project.">
          <span></span>
        </FormField>
      {:else}
        <FieldChecklist
          {items}
          bind:selected={o.endpoints}
          ariaLabel="Endpoints"
          maxHeight={240}
        />
      {/if}
    </FormSection>

    <FormSection label="Connection">
      <FormField label="Property prefix" hint="The base URL is read from <prefix>.base-url">
        <Input block size="sm" ariaLabel="Property prefix" bind:value={o.base_url_property} />
      </FormField>
      <FormField label="Default base URL"><Input block size="sm" ariaLabel="Default base URL" bind:value={o.base_url_default} /></FormField>
      <FormField label="Authentication"><RadioGroup bind:value={o.auth} options={AUTH} appearance="segment" size="sm" block /></FormField>
      {#if o.auth === 'api-key'}
        <FormField label="Header"><Input block size="sm" ariaLabel="API key header" bind:value={o.api_key_header} /></FormField>
      {/if}
      <FormField label="Timeout (seconds)" hint="0 keeps the library’s default">
        <NumberStepper value={o.timeout_seconds} min={0} max={600} size="sm" onchange={(v) => (o.timeout_seconds = v)} ariaLabel="Timeout" />
      </FormField>
    </FormSection>

    <FormSection label="Types and errors">
      <FormField label="DTOs"><RadioGroup bind:value={o.dtos} options={DTOS} appearance="segment" size="sm" block /></FormField>
      {#if o.dtos === 'generate'}
        <Toggle bind:checked={o.records} size="sm" label="Records when Java allows" />
      {/if}
      <FormField label="Errors"><RadioGroup bind:value={o.errors} options={ERRORS} appearance="segment" size="sm" block /></FormField>
    </FormSection>

    <FormSection label="Class">
      <FormField label="Name"><Input block size="sm" placeholder="derived from the controller" ariaLabel="Class name" bind:value={o.class_name} /></FormField>
      <FormField label="Package"><Input block size="sm" ariaLabel="Package" bind:value={o.package} /></FormField>
    </FormSection>
  {/snippet}
</GeneratorShell>
