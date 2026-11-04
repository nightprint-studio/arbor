<script lang="ts">
  /**
   * BodyEditor — none / JSON / text / XML / form / SOAP. JSON, text and XML edit in the shared code editor
   * with the language's highlighting; a form body is the same key/value table the params use. A SOAP body is
   * an envelope: next to it the protocol version and the `SOAPAction`, from which the backend sets the right
   * `Content-Type` and `SOAPAction` header (an explicit header of yours still wins).
   */
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import type { Body, BodyKind, Schema, SoapSettings, SoapVersion } from '$lib/types/hirundo';
  import { jsonKeyCompletion } from './schema';
  import KeyValueEditor from './KeyValueEditor.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';

  let picking = $state(false);

  function addFile(path: string) {
    picking = false;
    const name = path.split(/[\\/]/).pop() ?? 'file';
    onchange({ ...body, form: [...body.form, { key: name.replace(/\.[^.]*$/, '') || 'file', value: `@${path}`, description: '', enabled: true }] });
  }
  import { SAMPLE_ENVELOPE } from '../request-model';

  interface Props {
    body: Body;
    soap: SoapSettings;
    onchange: (body: Body) => void;
    onsoap: (soap: SoapSettings) => void;
    bulk?: boolean;
    /** The shape of the endpoint's `@RequestBody`, when Bennu knows it: the JSON's keys complete from it. */
    requestSchema?: Schema | null;
  }

  let { body, soap, onchange, onsoap, bulk = false, requestSchema = null }: Props = $props();

  const KINDS = [
    { value: 'none', label: 'None' },
    { value: 'json', label: 'JSON' },
    { value: 'text', label: 'Text' },
    { value: 'xml', label: 'XML' },
    { value: 'form', label: 'Form' },
    { value: 'soap', label: 'SOAP' },
    { value: 'multipart', label: 'Multipart' },
  ];

  const language = $derived.by(() => {
    const base = languageForPath(body.kind === 'json' ? 'body.json' : body.kind === 'xml' || body.kind === 'soap' ? 'body.xml' : 'body.txt');
    return body.kind === 'json' && requestSchema ? { ...base, intel: { ...(base.intel ?? {}), completion: jsonKeyCompletion(() => requestSchema) } } : base;
  });

  function choose(kind: string) {
    // Switching to SOAP on an empty body offers an envelope to start from.
    const text = kind === 'soap' && !body.text.trim() ? SAMPLE_ENVELOPE : body.text;
    onchange({ ...body, kind: kind as BodyKind, text });
  }
</script>

<div class="body">
  <div class="body-kind">
    <RadioGroup value={body.kind} options={KINDS} appearance="segment" size="sm" onchange={choose} />
  </div>

  {#if body.kind === 'soap'}
    <div class="body-soap">
      <FormField label="Version">
        <RadioGroup value={soap.version} options={[{ value: '1.1', label: 'SOAP 1.1' }, { value: '1.2', label: 'SOAP 1.2' }]} appearance="segment" size="sm" onchange={(v) => onsoap({ ...soap, version: v as SoapVersion })} />
      </FormField>
      <FormField label="SOAPAction" hint={soap.version === '1.1' ? 'Sent as the SOAPAction header.' : 'Sent as the action of the content type.'}>
        <Input value={soap.action} placeholder="urn:Operation" oninput={(v) => onsoap({ ...soap, action: v })} />
      </FormField>
    </div>
  {/if}

  {#if body.kind === 'none'}
    <EmptyState message="This request has no body." compact />
  {:else if body.kind === 'form'}
    <KeyValueEditor rows={body.form} {bulk} describe={false} onchange={(rows) => onchange({ ...body, form: rows })} />
  {:else if body.kind === 'multipart'}
    <div class="body-sample"><span class="body-hint">A value starting with <code>@</code> is a file; the description is the part's content type.</span></div>
    <KeyValueEditor rows={body.form} {bulk} keyPlaceholder="Field" valuePlaceholder="Text, or @/path/to/file" onchange={(rows) => onchange({ ...body, form: rows })} />
    <div class="body-sample"><Button size="sm" variant="outline" onclick={() => (picking = true)}>Add a file…</Button></div>
  {:else}
    <div class="body-code">
      <CodeEditor value={body.text} {language} lineNumbers={false} wrap oninput={(text) => onchange({ ...body, text })} placeholder={body.kind === 'json' ? '{ }' : ''} />
    </div>
    {#if body.kind === 'soap' && !body.text.trim()}
      <div class="body-sample"><Button size="sm" variant="outline" onclick={() => onchange({ ...body, text: SAMPLE_ENVELOPE })}>Insert a sample envelope</Button></div>
    {/if}
  {/if}
</div>

{#if picking}
  <FileExplorerModal mode="file" title="Choose the file to send" onConfirm={addFile} onCancel={() => (picking = false)} onClose={() => (picking = false)} />
{/if}

<style>
  .body-hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .body { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .body-kind { padding: 8px 12px; }
  .body-soap { display: grid; grid-template-columns: auto 1fr; gap: 16px; align-items: end; padding: 0 12px 8px; }
  .body-code { flex: 1; min-height: 160px; display: flex; border-top: 1px solid var(--border-subtle); }
  .body-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
  .body-sample { padding: 8px 12px; }
</style>
