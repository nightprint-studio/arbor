<script lang="ts">
  /**
   * CurlModal — cURL in both directions.
   *
   *  • **Paste**: a `curl …` (Chrome's "Copy as cURL" included, bash or cmd flavour) in the shared `ImportShell`
   *    — parsed as you type, the request it would create shown on the right before anything is made;
   *  • **Copy**: the open request as a command, with its variables filled in or kept as `{{placeholders}}` so
   *    it can be shared without the values.
   */
  import { ArrowLeftRight, Copy } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import { curlExport, curlImport } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { HttpRequest } from '$lib/types/hirundo';
  import { methodColor } from '../request-model';
  import ImportShell from './ImportShell.svelte';
  import { lineError, type TextError } from './import-diagnostics';

  let { onClose }: { onClose: () => void } = $props();

  const EXAMPLE = `curl -X POST https://api.example.com/items \\
  -H 'Content-Type: application/json' \\
  -d '{"name":"x"}'
`;
  const shell = languageForPath('x.sh');

  let page = $state<'paste' | 'copy'>('paste');
  let text = $state('');
  let parsed = $state<HttpRequest | null>(null);
  let parseError = $state('');
  let errorAt = $state<TextError | null>(null);

  // Parse as you type (after a short pause): the user sees what will be created before creating it.
  $effect(() => {
    const current = text;
    parsed = null;
    parseError = '';
    errorAt = null;
    if (!current.trim()) return;
    const timer = setTimeout(() => {
      void curlImport(current)
        .then((r) => { if (current === text) parsed = r; })
        .catch((e) => { if (current === text) { parseError = String(e); errorAt = lineError(current, parseError); } });
    }, 250);
    return () => clearTimeout(timer);
  });

  let resolved = $state('resolved');
  let command = $state('');
  $effect(() => {
    const tab = hirundoStore.active;
    if (page !== 'copy' || !tab) return;
    void curlExport(hirundoStore.target(tab), resolved === 'resolved').then((c) => (command = c)).catch((e) => (command = String(e)));
  });

  async function create() {
    if (!parsed) return;
    await hirundoStore.importCurl(text, hirundoStore.active?.parent ?? hirundoStore.tree[0]?.path ?? null);
    onClose();
  }
</script>

{#if page === 'paste'}
  <ImportShell
    title="cURL"
    icon={ArrowLeftRight}
    subtitle="paste a command"
    language={shell}
    bind:text
    placeholder={EXAMPLE}
    modes={['paste', 'file', 'example']}
    example={EXAMPLE}
    fileTitle="Choose a file with a curl command"
    error={parseError}
    {errorAt}
    sideTitle="What will be created"
    primaryLabel="Create request"
    primaryDisabled={!parsed}
    hint="to create the request"
    onPrimary={() => void create()}
    {onClose}
  >
    {#snippet extra()}
      <Button variant="outline" disabled={!hirundoStore.active} tooltip="Copy the open request as a cURL command" onclick={() => (page = 'copy')}>
        {#snippet iconStart()}<Copy size={14} />{/snippet}
        Copy this request…
      </Button>
    {/snippet}
    {#snippet side()}
      {#if parsed}
        {@const p = parsed}
        <div class="cm-preview">
          <div class="cm-line"><span class="cm-method" style:color={methodColor(p.method)}>{p.method}</span><code class="cm-url">{p.url}</code></div>
          <span class="cm-chips">
            {#if p.params.length}<Badge variant="chip" size="sm" label={`${p.params.length} params`} />{/if}
            {#if p.headers.length}<Badge variant="chip" size="sm" label={`${p.headers.length} headers`} />{/if}
            {#if p.body.kind !== 'none'}<Badge variant="chip" size="sm" label={`${p.body.kind} body`} />{/if}
            {#if p.auth.type !== 'inherit'}<Badge variant="chip" size="sm" label={`${p.auth.type} auth`} />{/if}
          </span>
          {#each p.headers as h, i (i)}<div class="cm-kv"><span>{h.key}</span><code>{h.value}</code></div>{/each}
          {#if p.body.text}<pre class="cm-body">{p.body.text}</pre>{/if}
        </div>
      {:else}
        <div class="cm-empty"><EmptyState message="Nothing to import yet" description="Paste a command and the request it makes shows here." compact /></div>
      {/if}
    {/snippet}
  </ImportShell>
{:else}
  <Modal {onClose} width="680px" height="520px" ariaLabel="Copy as cURL">
    {#snippet header()}
      <ModalHeader {onClose}>
        <ArrowLeftRight size={14} />
        <span class="modal-title">Copy as cURL</span>
      </ModalHeader>
    {/snippet}
    <div class="cm">
      <FormSection label="Variables" first>
        <RadioGroup value={resolved} options={[{ value: 'resolved', label: 'Filled in', description: 'The values of the active environment' }, { value: 'kept', label: 'Kept as {{placeholders}}', description: 'Safe to share' }]} appearance="segment" size="sm" onchange={(v) => (resolved = v)} />
      </FormSection>
      <div class="cm-code"><CodeEditor value={command} language={shell} readOnly wrap lineNumbers={false} /></div>
    </div>
    {#snippet footer()}
      <ModalFooter>
        <Button variant="ghost" onclick={() => (page = 'paste')}>Back</Button>
        <CopyButton value={command} variant="inline" label="Copy command" toastSuccess="Copied as cURL" />
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .cm { display: flex; flex-direction: column; height: 100%; min-height: 0; padding-bottom: 8px; }
  .cm-preview { display: flex; flex-direction: column; gap: 8px; padding: 12px; }
  .cm-line { display: flex; align-items: baseline; gap: 8px; }
  .cm-method { font: 600 12px var(--font-mono); }
  .cm-url { font-family: var(--font-mono); font-size: var(--font-size-xs); word-break: break-all; }
  .cm-chips { display: inline-flex; gap: 4px; flex-wrap: wrap; }
  .cm-kv { display: flex; gap: 8px; font-size: var(--font-size-xs); }
  .cm-kv span { color: var(--text-muted); flex: none; }
  .cm-kv code { font-family: var(--font-mono); word-break: break-all; }
  .cm-body { margin: 0; padding: 8px 10px; font: var(--font-size-xs) var(--font-mono); background: var(--bg-elevated); border-radius: var(--radius-md); white-space: pre-wrap; word-break: break-all; max-height: 240px; overflow: auto; }
  .cm-empty { flex: 1; display: grid; place-items: center; }
  .cm-code { flex: 1; min-height: 0; display: flex; margin: 8px 16px 0; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); overflow: hidden; }
  .cm-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
