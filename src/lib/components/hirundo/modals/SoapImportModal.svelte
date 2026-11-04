<script lang="ts">
  /**
   * SoapImportModal — SOAP requests from a WSDL or an envelope. Paste a WSDL, a SOAP envelope, or the URL of
   * a WSDL (fetched by the backend); the modal lists what it found — one request per operation of a WSDL, one
   * for an envelope — and the ones ticked are saved into a collection. A WSDL's schema is not read, so an
   * operation starts with an empty body element; an envelope keeps the body it came with.
   */
  import { onMount } from 'svelte';
  import { Download } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { requestSave, soapImport } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { SoapImport } from '$lib/types/hirundo';

  let { onClose }: { onClose: () => void } = $props();

  let source = $state('');
  let el = $state<HTMLTextAreaElement | undefined>(undefined);
  let reading = $state(false);
  let error = $state('');
  let found = $state<SoapImport | null>(null);
  let picked = $state<boolean[]>([]);
  let collection = $state(hirundoStore.active?.parent ?? hirundoStore.tree[0]?.path ?? '');
  let saving = $state(false);

  onMount(() => el?.focus());

  const collections = $derived(hirundoStore.tree.map((c) => ({ value: c.path, label: c.name })));
  const count = $derived(picked.filter(Boolean).length);

  async function read() {
    if (!source.trim() || reading) return;
    reading = true;
    error = '';
    found = null;
    try {
      found = await soapImport(source);
      picked = found.requests.map(() => true);
    } catch (e) {
      error = String(e);
    } finally {
      reading = false;
    }
  }

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

  function onKey(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.key !== 'Enter') return;
    e.preventDefault();
    if (found) void save(); else void read();
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="680px" height="600px" ariaLabel="Import SOAP">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Download size={14} />
      <span class="modal-title">Import SOAP</span>
      <span class="si-sub">WSDL or envelope</span>
    </ModalHeader>
  {/snippet}

  <div class="si">
    <FormSection label="Source" hint="A WSDL, a SOAP envelope, or the URL of a WSDL." first>
      <Textarea bind:value={source} bind:element={el} minRows={5} maxRows={9} framed placeholder="https://service.example/ws?wsdl" />
    </FormSection>

    {#if error}<div class="si-pad"><Alert variant="error" compact text={error} dismissible onclose={() => (error = '')} /></div>{/if}

    {#if found}
      <FormSection label={`Found · ${found.requests.length}`}>
        {#if found.requests.length === 0}
          <EmptyState message="No operation found" compact />
        {:else}
          <div class="si-list">
            {#each found.requests as r, i (i)}
              <div class="si-row">
                <Checkbox checked={picked[i]} label={r.name} onchange={(v) => (picked[i] = v)} />
                <span class="si-meta">{r.soap.version}{r.soap.action ? ` · ${r.soap.action}` : ''}</span>
              </div>
            {/each}
          </div>
        {/if}
        {#each found.notes as note (note)}<p class="si-note">{note}</p>{/each}
        <FormField label="Save into"><Select value={collection} options={collections} onchange={(v) => (collection = v)} /></FormField>
      </FormSection>
    {/if}
  </div>

  {#snippet footer()}
    {#if found}
      <ModalFooter hint="to save">
        <Button variant="ghost" onclick={() => (found = null)}>Back</Button>
        <Button variant="primary" loading={saving} disabled={count === 0 || !collection} onclick={() => void save()}>{`Add ${count} request${count === 1 ? '' : 's'}`}</Button>
      </ModalFooter>
    {:else}
      <ModalFooter hint="to read">
        <Button variant="ghost" onclick={onClose}>Cancel</Button>
        <Button variant="primary" loading={reading} disabled={!source.trim()} onclick={() => void read()}>Read</Button>
      </ModalFooter>
    {/if}
  {/snippet}
</Modal>

<style>
  .si { display: flex; flex-direction: column; gap: 4px; padding: 4px 16px 12px; overflow: auto; height: 100%; }
  .si-pad { padding: 4px 0; }
  .si-sub { font-size: var(--font-size-xs); color: var(--text-muted); margin-left: 8px; }
  .si-list { display: flex; flex-direction: column; max-height: 200px; overflow: auto; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); }
  .si-row { display: flex; align-items: center; gap: 12px; padding: 4px 10px; border-bottom: 1px solid var(--border-subtle); }
  .si-row:last-child { border-bottom: none; }
  .si-meta { font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-mono); }
  .si-note { margin: 4px 0 0; font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
