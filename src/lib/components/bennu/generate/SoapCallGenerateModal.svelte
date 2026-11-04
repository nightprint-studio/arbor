<script lang="ts">
  /**
   * Generate a SOAP call from a pasted envelope — and, when you have it, the WSDL. The envelope's Body gives
   * the operation, the namespace and the arguments (every value in the request becomes a parameter); the WSDL
   * gives the address, the SOAPAction and the service and port names. Variants: the JDK HttpClient with the
   * envelope in a text block, Spring-WS, WebClient, CXF Dispatch or a generated CXF port, Apache HttpClient;
   * optionally with a WS-Security UsernameToken, and the response read by XPath or JAXB.
   */
  import { onMount } from 'svelte';
  import { FileCode2 } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import FieldRows from './FieldRows.svelte';
  import GeneratorShell from './GeneratorShell.svelte';

  interface Options {
    envelope: string;
    wsdl: string;
    variant: string;
    class_name: string;
    package: string;
    endpoint: string;
    soap_action: string;
    ws_security: boolean;
    response: string;
    response_fields: { name: string; ty: string }[];
    response_type: string;
    timeout_seconds: number;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('soap-call', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const VARIANTS = [
    { value: 'jdk', label: 'JDK HttpClient (text block)' },
    { value: 'webservicetemplate', label: 'Spring-WS WebServiceTemplate' },
    { value: 'webclient', label: 'Spring WebClient' },
    { value: 'cxf-dispatch', label: 'CXF / JAX-WS Dispatch' },
    { value: 'cxf-port', label: 'CXF generated port (wsimport)' },
    { value: 'apache', label: 'Apache HttpClient 5' },
  ];
  const RESPONSES = [
    { value: 'xpath', label: 'XPath' },
    { value: 'jaxb', label: 'JAXB' },
    { value: 'raw', label: 'Raw XML' },
  ];
</script>

<GeneratorShell {session} icon={FileCode2} title="SOAP call from an envelope" {onClose}>
  {#snippet form()}
    <FormSection label="Request" first>
      <FormField label="SOAP envelope" hint="Every value inside the operation becomes an argument">
        <Textarea framed minRows={8} maxRows={14} ariaLabel="SOAP envelope" placeholder="<soapenv:Envelope …>" bind:value={o.envelope} />
      </FormField>
      <FormField label="WSDL" optionalText="optional" hint="Gives the address, SOAPAction, service and port names">
        <Textarea framed minRows={3} maxRows={10} ariaLabel="WSDL" placeholder="<wsdl:definitions …>" bind:value={o.wsdl} />
      </FormField>
    </FormSection>

    <FormSection label="Call">
      <FormField label="With"><Select bind:value={o.variant} options={VARIANTS} size="sm" /></FormField>
      <FormField label="Endpoint"><Input block size="sm" placeholder="from the WSDL" ariaLabel="Endpoint" bind:value={o.endpoint} /></FormField>
      <FormField label="SOAPAction"><Input block size="sm" placeholder="from the WSDL" ariaLabel="SOAPAction" bind:value={o.soap_action} /></FormField>
      <Toggle bind:checked={o.ws_security} size="sm" label="WS-Security UsernameToken" description="Plain-text password, username and password as arguments of the constructor" />
      <FormField label="Timeout (seconds)">
        <NumberStepper value={o.timeout_seconds} min={1} max={3600} size="sm" onchange={(v) => (o.timeout_seconds = v)} ariaLabel="Timeout" />
      </FormField>
    </FormSection>

    {#if o.variant !== 'cxf-port'}
      <FormSection label="Response">
        <RadioGroup bind:value={o.response} options={RESPONSES} appearance="segment" size="sm" block />
        {#if o.response === 'xpath'}
          <FieldRows bind:items={o.response_fields} ariaLabel="Value" addLabel="Value" namePlaceholder="name" typePlaceholder="XPath, e.g. //*[local-name()='Temp']" />
        {:else if o.response === 'jaxb'}
          <FormField label="Unmarshal the first element of the Body to">
            <Input block size="sm" placeholder="WeatherResult" ariaLabel="Response type" bind:value={o.response_type} />
          </FormField>
        {/if}
      </FormSection>
    {/if}

    <FormSection label="Class">
      <FormField label="Name"><Input block size="sm" placeholder="derived from the operation" ariaLabel="Class name" bind:value={o.class_name} /></FormField>
      <FormField label="Package"><Input block size="sm" ariaLabel="Package" bind:value={o.package} /></FormField>
    </FormSection>
  {/snippet}
</GeneratorShell>
