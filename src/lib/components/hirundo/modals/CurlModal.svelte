<script lang="ts">
  /**
   * CurlModal — cURL in both directions, one page each.
   *
   *  • **Paste**: a `curl …` (Chrome's "Copy as cURL" included, bash or cmd flavour) is parsed as you type and
   *    the request it would create is shown before anything is made;
   *  • **Copy**: the open request as a command, with its variables filled in or kept as `{{placeholders}}` so
   *    it can be shared without the values.
   */
  import { onMount } from 'svelte';
  import { ArrowLeftRight, ClipboardPaste, Copy } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import { curlExport, curlImport } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { HttpRequest } from '$lib/types/hirundo';
  import { methodColor } from '../request-model';

  let { onClose }: { onClose: () => void } = $props();

  const EXAMPLE = `curl -X POST https://api.example.com/items -H 'Content-Type: application/json' -d '{"name":"x"}'`;
  const shell = languageForPath('x.sh');

  let page = $state<'paste' | 'copy'>('paste');
  let text = $state('');
  let el = $state<HTMLTextAreaElement | undefined>(undefined);
  let parsed = $state<HttpRequest | null>(null);
  let parseError = $state('');

  // Parse as you type (after a short pause): the user sees what will be created before creating it.
  $effect(() => {
    const current = text;
    parsed = null;
    parseError = '';
    if (!current.trim()) return;
    const timer = setTimeout(() => {
      void curlImport(current).then((r) => { if (current === text) parsed = r; }).catch((e) => { if (current === text) parseError = String(e); });
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

  onMount(() => el?.focus());

  async function create() {
    if (!parsed) return;
    await hirundoStore.importCurl(text, hirundoStore.active?.parent ?? hirundoStore.tree[0]?.path ?? null);
    onClose();
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && page === 'paste') { e.preventDefault(); void create(); }
  }
</script>

<Modal {onClose} width="680px" height="520px" ariaLabel="cURL">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ArrowLeftRight size={14} />
      <span class="modal-title">cURL</span>
    </ModalHeader>
  {/snippet}

  <div class="cm" role="presentation" onkeydown={onKey}>
    <div class="cm-tabs">
      <Tabs
        items={[{ id: 'paste', label: 'Paste a command', icon: ClipboardPaste, iconSize: 13 }, { id: 'copy', label: 'Copy this request', icon: Copy, iconSize: 13, disabled: !hirundoStore.active }]}
        value={page}
        variant="underline"
        size="sm"
        ariaLabel="cURL pages"
        onSelect={(id) => (page = id as typeof page)}
      />
    </div>

    {#if page === 'paste'}
      <FormSection label="Command" hint="Multi-line commands with \ or ^ continuations are fine." first>
        <Textarea bind:value={text} bind:element={el} minRows={7} maxRows={12} framed placeholder={EXAMPLE} />
      </FormSection>
      <FormSection label="What will be created">
        {#if parseError}
          <Alert variant="error" compact text={parseError} />
        {:else if parsed}
          <div class="cm-preview">
            <span class="cm-method" style:color={methodColor(parsed.method)}>{parsed.method}</span>
            <code class="cm-url">{parsed.url}</code>
            <span class="cm-chips">
              {#if parsed.params.length}<Badge variant="chip" size="sm" label={`${parsed.params.length} params`} />{/if}
              {#if parsed.headers.length}<Badge variant="chip" size="sm" label={`${parsed.headers.length} headers`} />{/if}
              {#if parsed.body.kind !== 'none'}<Badge variant="chip" size="sm" label={`${parsed.body.kind} body`} />{/if}
              {#if parsed.auth.type !== 'inherit'}<Badge variant="chip" size="sm" label={`${parsed.auth.type} auth`} />{/if}
            </span>
          </div>
        {:else}
          <p class="cm-muted">Paste a command above and the request it makes shows here.</p>
        {/if}
      </FormSection>
    {:else}
      <FormSection label="Variables" first>
        <RadioGroup value={resolved} options={[{ value: 'resolved', label: 'Filled in', description: 'The values of the active environment' }, { value: 'kept', label: 'Kept as {{placeholders}}', description: 'Safe to share' }]} appearance="segment" size="sm" onchange={(v) => (resolved = v)} />
      </FormSection>
      <div class="cm-code"><CodeEditor value={command} language={shell} readOnly wrap lineNumbers={false} /></div>
    {/if}
  </div>

  {#snippet footer()}
    {#if page === 'paste'}
      <ModalFooter hint="to create the request">
        <Button variant="ghost" onclick={onClose}>Cancel</Button>
        <Button variant="primary" disabled={!parsed} onclick={() => void create()}>Create request</Button>
      </ModalFooter>
    {:else}
      <ModalFooter>
        <Button variant="ghost" onclick={onClose}>Close</Button>
        <CopyButton value={command} variant="inline" label="Copy command" toastSuccess="Copied as cURL" />
      </ModalFooter>
    {/if}
  {/snippet}
</Modal>

<style>
  .cm { display: flex; flex-direction: column; height: 100%; min-height: 0; padding-bottom: 8px; }
  .cm-tabs { padding: 0 12px; border-bottom: 1px solid var(--border-subtle); }
  .cm-preview { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: var(--font-size-sm); }
  .cm-method { font: 600 12px var(--font-mono); }
  .cm-url { font-family: var(--font-mono); font-size: var(--font-size-xs); word-break: break-all; }
  .cm-chips { display: inline-flex; gap: 4px; }
  .cm-muted { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .cm-code { flex: 1; min-height: 0; display: flex; margin: 8px 16px 0; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); overflow: hidden; }
  .cm-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
