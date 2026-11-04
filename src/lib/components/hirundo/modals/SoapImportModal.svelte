<script lang="ts">
  /**
   * SoapImportModal — SOAP requests from a WSDL or an envelope, in the shared `ImportShell`. Paste a WSDL or an
   * envelope, read one from a file, or give the URL of a WSDL (fetched by the backend); the list on the right is
   * what it found — one request per operation of a WSDL, one for an envelope. A WSDL's schema is not read, so an
   * operation starts with an empty body element; an envelope keeps the body it came with.
   */
  import { Download } from 'lucide-svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import { requestSave, soapImport } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { SoapImport } from '$lib/types/hirundo';
  import { SAMPLE_ENVELOPE } from '../request-model';
  import ImportShell from './ImportShell.svelte';
  import { xmlError, type TextError } from './import-diagnostics';

  let { onClose }: { onClose: () => void } = $props();

  const language = languageForPath('service.xml');

  let text = $state('');
  let fromUrl = $state<string | null>(null);
  let reading = $state(false);
  let error = $state('');
  let errorAt = $state<TextError | null>(null);
  let found = $state<SoapImport | null>(null);
  let picked = $state<boolean[]>([]);
  let collection = $state(hirundoStore.active?.parent ?? hirundoStore.tree[0]?.path ?? '');
  let saving = $state(false);

  const collections = $derived(hirundoStore.tree.map((c) => ({ value: c.path, label: c.name })));
  const count = $derived(picked.filter(Boolean).length);

  async function read(source: string, shownAs: string | null = null) {
    reading = true;
    error = '';
    errorAt = null;
    fromUrl = shownAs;
    try {
      found = await soapImport(source);
      picked = found.requests.map(() => true);
      if (shownAs) fromUrl = `${shownAs} — ${found.requests.length} operation${found.requests.length === 1 ? '' : 's'} found.`;
    } catch (e) {
      found = null;
      error = String(e);
      errorAt = shownAs ? null : xmlError(source);
    } finally {
      reading = false;
    }
  }

  // A pasted WSDL or envelope is read as it is typed; a URL waits for Fetch.
  $effect(() => {
    const t = text;
    if (!t.trim()) { found = null; picked = []; error = ''; errorAt = null; return; }
    if (/^https?:\/\/\S+$/.test(t.trim())) return;
    const timer = setTimeout(() => void read(t), 350);
    return () => clearTimeout(timer);
  });

  async function save() {
    const project = hirundoStore.project;
    if (!project || !found || !collection || saving) return;
    saving = true;
    try {
      let first: string | null = null;
      for (const [i, request] of found.requests.entries()) {
        if (!picked[i]) continue;
        const path = await requestSave(project, request, { parent: collection });
        first ??= path;
      }
      await hirundoStore.refreshTree();
      if (first) await hirundoStore.openRequest(first, false);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<ImportShell
  title="Import SOAP"
  icon={Download}
  subtitle="WSDL or envelope"
  {language}
  bind:text
  placeholder="Paste a WSDL or a SOAP envelope here, or give the URL of a WSDL."
  modes={['paste', 'file', 'url', 'example']}
  example={SAMPLE_ENVELOPE}
  fileExtensions={['wsdl', 'xml']}
  fileTitle="Choose the WSDL or envelope"
  urlPlaceholder="https://service.example/ws?wsdl"
  onFetch={(u) => void read(u, u)}
  {fromUrl}
  {reading}
  {error}
  {errorAt}
  sideTitle="Found"
  sideCount={found?.requests.length || undefined}
  primaryLabel={`Add ${count} request${count === 1 ? '' : 's'}`}
  primaryDisabled={count === 0 || !collection}
  primaryLoading={saving}
  onPrimary={() => void save()}
  {onClose}
>
  {#snippet side()}
    {#if !found || found.requests.length === 0}
      <div class="si-empty"><EmptyState message={found ? 'No operation found' : 'Nothing to import yet'} description="The operations of a WSDL, or the request of an envelope, appear here as soon as it reads." compact /></div>
    {:else}
      <div class="si-list">
        {#each found.requests as r, i (i)}
          <div class="si-row">
            <Checkbox checked={picked[i]} label={r.name} onchange={(v) => (picked[i] = v)} />
            <span class="si-meta">{r.soap.version}{r.soap.action ? ` · ${r.soap.action}` : ''}</span>
          </div>
        {/each}
        {#each found.notes as note (note)}<p class="si-note">{note}</p>{/each}
      </div>
      <div class="si-target">
        <FormSection label="Where"><FormField label="Save into"><Select value={collection} options={collections} onchange={(v) => (collection = v)} /></FormField></FormSection>
      </div>
    {/if}
  {/snippet}
</ImportShell>

<style>
  .si-empty { flex: 1; display: grid; place-items: center; }
  .si-list { flex: 1; min-height: 0; overflow: auto; }
  .si-row { display: flex; flex-direction: column; gap: 2px; padding: 6px 12px; border-bottom: 1px solid var(--border-subtle); }
  .si-meta { font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-mono); padding-left: 26px; }
  .si-note { margin: 8px 12px 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .si-target { border-top: 1px solid var(--border-subtle); padding: 0 12px 8px; }
</style>
