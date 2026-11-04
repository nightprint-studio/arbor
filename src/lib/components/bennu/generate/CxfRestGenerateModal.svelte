<script lang="ts">
  /**
   * Convert a CXF / JAX-RS client to a modern one. Paste the interface (or resource class) with its
   * `@Path`, `@GET`, `@PathParam`, `@QueryParam`, `@HeaderParam`, `@Consumes` and `@Produces`, and choose the
   * library — the Java ones of the HTTP client generator, or Rust `reqwest` / `ureq` — with XML bodies kept
   * (Spring converters, Jackson XML, JAXB, or `quick-xml` in Rust).
   */
  import { onMount } from 'svelte';
  import { RefreshCw } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    source: string;
    target: string;
    content: string;
    xml_binding: string;
    class_name: string;
    package: string;
    auth: string;
    errors: string;
    timeout_seconds: number;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('cxf-rest', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const TARGETS = [
    { value: 'restclient', label: 'Java · RestClient' },
    { value: 'webclient', label: 'Java · WebClient' },
    { value: 'resttemplate', label: 'Java · RestTemplate' },
    { value: 'httpinterface', label: 'Java · @HttpExchange interface' },
    { value: 'feign', label: 'Java · OpenFeign' },
    { value: 'jdk', label: 'Java · java.net.http' },
    { value: 'okhttp', label: 'Java · OkHttp' },
    { value: 'retrofit', label: 'Java · Retrofit' },
    { value: 'reqwest', label: 'Rust · reqwest (async)' },
    { value: 'reqwest-blocking', label: 'Rust · reqwest (blocking)' },
    { value: 'ureq', label: 'Rust · ureq' },
  ];
  const CONTENT = [
    { value: 'auto', label: 'As the code says' },
    { value: 'xml', label: 'XML' },
    { value: 'json', label: 'JSON' },
  ];
  const BINDINGS = [
    { value: 'jaxb', label: 'JAXB' },
    { value: 'jackson', label: 'Jackson XML' },
  ];
  const AUTH = [
    { value: 'none', label: 'None' },
    { value: 'bearer', label: 'Bearer' },
    { value: 'basic', label: 'Basic' },
    { value: 'api-key', label: 'API key' },
  ];

  const rust = $derived(o.target === 'reqwest' || o.target === 'reqwest-blocking' || o.target === 'ureq');
</script>

<GeneratorShell {session} icon={RefreshCw} title="CXF REST client → modern client" {onClose}>
  {#snippet form()}
    <FormSection label="CXF code" first>
      <FormField label="JAX-RS interface or resource class" hint="@Path, @GET/@POST…, @PathParam, @QueryParam, @HeaderParam, @Consumes, @Produces">
        <Textarea framed minRows={10} maxRows={18} ariaLabel="CXF source" placeholder={'@Path("/orders") public interface OrderResource { … }'} bind:value={o.source} />
      </FormField>
    </FormSection>

    <FormSection label="Target">
      <FormField label="Write it as"><Select bind:value={o.target} options={TARGETS} size="sm" /></FormField>
      <FormField label="Bodies"><RadioGroup bind:value={o.content} options={CONTENT} appearance="segment" size="sm" block /></FormField>
      {#if !rust && o.content !== 'json'}
        <FormField label="XML binding" hint="JAXB-annotated classes work with both">
          <RadioGroup bind:value={o.xml_binding} options={BINDINGS} appearance="segment" size="sm" block />
        </FormField>
      {/if}
      <FormField label="Authentication"><RadioGroup bind:value={o.auth} options={AUTH} appearance="segment" size="sm" block /></FormField>
      <FormField label="Timeout (seconds)">
        <NumberStepper value={o.timeout_seconds} min={1} max={3600} size="sm" onchange={(v) => (o.timeout_seconds = v)} ariaLabel="Timeout" />
      </FormField>
    </FormSection>

    <FormSection label={rust ? 'Rust file' : 'Class'}>
      <FormField label="Name"><Input block size="sm" placeholder="derived from the interface" ariaLabel="Client name" bind:value={o.class_name} /></FormField>
      {#if !rust}
        <FormField label="Package"><Input block size="sm" ariaLabel="Package" bind:value={o.package} /></FormField>
      {/if}
    </FormSection>
  {/snippet}
</GeneratorShell>
