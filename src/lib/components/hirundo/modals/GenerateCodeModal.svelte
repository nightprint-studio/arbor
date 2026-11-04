<script lang="ts">
  /** GenerateCodeModal — the open request as code (cURL, Rust, Java, JavaScript), variables resolved. */
  import { Code2 } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import { requestPreview } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { Preview } from '$lib/types/hirundo';
  import { generate, LANGS, type Lang } from './codegen';

  let { onClose }: { onClose: () => void } = $props();

  let lang = $state<Lang>('curl');
  let preview = $state<Preview | null>(null);
  let error = $state('');

  $effect(() => {
    const tab = hirundoStore.active;
    if (!tab) return;
    void requestPreview(hirundoStore.target(tab)).then((p) => (preview = p)).catch((e) => (error = String(e)));
  });

  const code = $derived(preview ? generate(lang, preview) : '');
  const language = $derived(languageForPath(LANGS.find((l) => l.id === lang)?.file ?? 'x.txt'));

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void navigator.clipboard.writeText(code).then(onClose); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="780px" height="560px" ariaLabel="Generate code">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Code2 size={14} />
      <span class="modal-title">Generate code</span>
      {#if preview}<span class="gc-sub">{preview.method} {preview.url}</span>{/if}
    </ModalHeader>
  {/snippet}

  <div class="gc">
    <div class="gc-tabs"><Tabs items={LANGS.map((l) => ({ id: l.id, label: l.label }))} value={lang} variant="underline" size="sm" ariaLabel="Language" onSelect={(id) => (lang = id as Lang)} /></div>
    {#if error}
      <div class="gc-pad"><Alert variant="error" text={error} /></div>
    {:else if !preview}
      <Spinner block label="Resolving the request…" delay={0} />
    {:else}
      {#if preview.unresolved.length}
        <div class="gc-pad"><Alert variant="warning" compact text={`No value for ${preview.unresolved.map((n) => `{{${n}}}`).join(', ')}: they are left as written.`} /></div>
      {/if}
      <div class="gc-code"><CodeEditor value={code} {language} readOnly wrap lineNumbers={false} /></div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter hint="to copy and close">
      <Button variant="ghost" onclick={onClose}>Close</Button>
      <CopyButton value={code} variant="inline" label="Copy" toastSuccess="Code copied" />
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .gc { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .gc-tabs { padding: 0 12px; border-bottom: 1px solid var(--border-subtle); }
  .gc-pad { padding: 10px 12px 0; }
  .gc-sub { font-size: var(--font-size-xs); color: var(--text-muted); margin-left: 8px; max-width: 420px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gc-code { flex: 1; min-height: 0; display: flex; }
  .gc-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
