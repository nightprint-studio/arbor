<script lang="ts">
  /**
   * VariablesModal — the request exactly as it would be sent, with every variable resolved: the final URL,
   * the headers (inherited auth included), the body, and which variables have no value. The same value the
   * sender and "copy as cURL" use, so what is read here is what goes out.
   */
  import { Variable } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import PropertyList from '$lib/components/shared/ui/PropertyList.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import { requestPreview } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { Preview } from '$lib/types/hirundo';
  import { methodColor } from '../request-model';

  let { onClose }: { onClose: () => void } = $props();

  let preview = $state<Preview | null>(null);
  let error = $state('');

  $effect(() => {
    const tab = hirundoStore.active;
    if (!tab) return;
    void requestPreview(hirundoStore.target(tab)).then((p) => (preview = p)).catch((e) => (error = String(e)));
  });

  const bodyLanguage = $derived(languageForPath(preview?.body?.trimStart().startsWith('<') ? 'b.xml' : 'b.json'));
  const rows = $derived(preview?.headers.map(([k, v]) => ({ label: k, value: v, wrap: true })) ?? []);
</script>

<Modal {onClose} width="740px" height="560px" ariaLabel="Resolved request">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Variable size={14} />
      <span class="modal-title">Resolved request</span>
      {#if preview?.env}<Badge variant="chip" size="sm" color={preview.env.color} label={preview.env.name} />{/if}
    </ModalHeader>
  {/snippet}

  <div class="vm">
    {#if error}
      <Alert variant="error" text={error} />
    {:else if !preview}
      <Spinner block label="Resolving…" delay={0} />
    {:else}
      {#if preview.unresolved.length}
        <Alert variant="warning" compact title="No value for" text={preview.unresolved.map((n) => `{{${n}}}`).join(', ')} />
      {/if}
      {#if preview.missing_secrets.length}
        <Alert variant="warning" compact title="Secret not in the keychain" text={preview.missing_secrets.join(', ')} />
      {/if}
      <FormSection label="Request" first>
        <div class="vm-url"><b style:color={methodColor(preview.method)}>{preview.method}</b> <code>{preview.url}</code></div>
      </FormSection>
      <FormSection label="Headers" aside={countAside}>
        {#if rows.length}<PropertyList {rows} />{:else}<p class="vm-muted">No headers.</p>{/if}
      </FormSection>
      {#if preview.body}
        <FormSection label="Body">
          <div class="vm-body"><CodeEditor value={preview.body} language={bodyLanguage} readOnly wrap lineNumbers={false} /></div>
        </FormSection>
      {/if}
    {/if}
  </div>

  {#snippet countAside()}<span class="vm-muted">{rows.length}</span>{/snippet}

  {#snippet footer()}
    <ModalFooter>
      {#if preview}<CopyButton value={preview.url} variant="inline" label="Copy URL" toastSuccess="URL copied" />{/if}
      <Button variant="primary" onclick={onClose}>Close</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .vm { display: flex; flex-direction: column; gap: 4px; padding: 8px 16px 12px; font-size: var(--font-size-sm); overflow: auto; height: 100%; }
  .vm-url code { font-family: var(--font-mono); word-break: break-all; font-size: var(--font-size-xs); }
  .vm-muted { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .vm-body { height: 200px; display: flex; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); overflow: hidden; }
  .vm-body > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
