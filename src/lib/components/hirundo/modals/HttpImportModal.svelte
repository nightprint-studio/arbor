<script lang="ts">
  /**
   * HttpImportModal — a `.http` / `.rest` file (IntelliJ's HTTP Client format) becomes requests. Browse to the
   * file, or paste its text; the modal lists the requests it holds with what could not be carried over
   * (JavaScript response handlers are read and reported, never run), and the ticked ones are saved into a
   * collection. Given a *file*, `< ./body.json` bodies are read from beside it, and the environments of the
   * `http-client.env.json` next to it (and its private twin) can come along.
   */
  import { onMount } from 'svelte';
  import { FileText, FolderSearch } from 'lucide-svelte';
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
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { httpEnvImport, httpImport, requestSave } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { HttpEntry } from '$lib/types/hirundo';
  import { methodColor } from '../request-model';

  let { onClose }: { onClose: () => void } = $props();

  let source = $state('');
  let el = $state<HTMLTextAreaElement | undefined>(undefined);
  let browsing = $state(false);
  let reading = $state(false);
  let error = $state('');
  let entries = $state<HttpEntry[] | null>(null);
  let picked = $state<boolean[]>([]);
  let collection = $state(hirundoStore.active?.parent ?? hirundoStore.tree[0]?.path ?? '');
  let withEnvs = $state(true);
  let saving = $state(false);

  onMount(() => el?.focus());

  const collections = $derived(hirundoStore.tree.map((c) => ({ value: c.path, label: c.name })));
  const count = $derived(picked.filter(Boolean).length);
  /** The folder of the file, when the source is a path (one line, no braces). */
  const folder = $derived.by(() => {
    const s = source.trim();
    return s && !s.includes('\n') && !/^https?:/.test(s) ? s.replace(/[\\/][^\\/]*$/, '') : '';
  });

  async function read() {
    if (!source.trim() || reading) return;
    reading = true;
    error = '';
    entries = null;
    try {
      entries = (await httpImport(source)).entries;
      picked = entries.map(() => true);
    } catch (e) {
      error = String(e);
    } finally {
      reading = false;
    }
  }

  async function save() {
    const project = hirundoStore.project;
    if (!project || !entries || !collection || saving) return;
    saving = true;
    error = '';
    try {
      let first: string | null = null;
      for (const [i, entry] of entries.entries()) {
        if (!picked[i]) continue;
        const path = await requestSave(project, entry.request, { parent: collection });
        first ??= path;
      }
      if (withEnvs && folder) {
        const added = await httpEnvImport(project, folder).catch(() => 0);
        if (added) { await hirundoStore.refreshEnvs(); toastStore.show(`${added} environment${added === 1 ? '' : 's'} imported.`, 'success'); }
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
    if (browsing || !(e.ctrlKey || e.metaKey) || e.key !== 'Enter') return;
    e.preventDefault();
    if (entries) void save(); else void read();
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="700px" height="620px" ariaLabel="Import a .http file">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FileText size={14} />
      <span class="modal-title">Import a .http file</span>
    </ModalHeader>
  {/snippet}

  <div class="hi">
    <FormSection label="Source" hint="The path of a .http or .rest file, or its text." first>
      <Textarea bind:value={source} bind:element={el} minRows={5} maxRows={9} framed placeholder={'### Login\nPOST {{host}}/api/login\nContent-Type: application/json\n\n{ "user": "bob" }'} />
      <div class="hi-row"><Button variant="outline" size="sm" onclick={() => (browsing = true)}>{#snippet iconStart()}<FolderSearch size={13} />{/snippet}Browse…</Button></div>
    </FormSection>

    {#if error}<div class="hi-pad"><Alert variant="error" compact text={error} dismissible onclose={() => (error = '')} /></div>{/if}

    {#if entries}
      <FormSection label={`Found · ${entries.length}`}>
        {#if entries.length === 0}
          <EmptyState message="No request found" compact />
        {:else}
          <div class="hi-list">
            {#each entries as entry, i (i)}
              <div class="hi-row2">
                <Checkbox checked={picked[i]} onchange={(v) => (picked[i] = v)} ariaLabel={entry.title} />
                <span class="hi-method" style:color={methodColor(entry.request.method)}>{entry.request.method}</span>
                <b class="hi-name">{entry.title}</b>
                <code class="hi-url">{entry.request.url}</code>
                {#each entry.warnings as w (w)}<span class="hi-warn">{w}</span>{/each}
              </div>
            {/each}
          </div>
        {/if}
        {#if folder}<Checkbox checked={withEnvs} label="Also import the environments of http-client.env.json beside the file" onchange={(v) => (withEnvs = v)} />{/if}
        <FormField label="Save into"><Select value={collection} options={collections} onchange={(v) => (collection = v)} /></FormField>
      </FormSection>
    {/if}
  </div>

  {#snippet footer()}
    {#if entries}
      <ModalFooter hint="to import">
        <Button variant="ghost" onclick={() => (entries = null)}>Back</Button>
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

{#if browsing}
  <FileExplorerModal mode="file" extensions={['http', 'rest']} title="Choose the .http file" onConfirm={(p: string) => { browsing = false; source = p; void read(); }} onCancel={() => (browsing = false)} onClose={() => (browsing = false)} />
{/if}

<style>
  .hi { display: flex; flex-direction: column; gap: 4px; padding: 4px 16px 12px; overflow: auto; height: 100%; }
  .hi-row { display: flex; gap: 8px; margin-top: 6px; }
  .hi-pad { padding: 4px 0; }
  .hi-list { display: flex; flex-direction: column; max-height: 230px; overflow: auto; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); }
  .hi-row2 { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 4px 10px; border-bottom: 1px solid var(--border-subtle); }
  .hi-row2:last-child { border-bottom: none; }
  .hi-method { font: 600 10px var(--font-mono); width: 44px; }
  .hi-name { font-size: var(--font-size-sm); }
  .hi-url { font-family: var(--font-mono); font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 280px; }
  .hi-warn { flex-basis: 100%; font-size: var(--font-size-xs); color: var(--warning); padding-left: 52px; }
</style>
