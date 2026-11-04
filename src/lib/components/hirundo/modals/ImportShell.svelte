<script lang="ts">
  /**
   * ImportShell — the one layout of every import: the **source is the main thing** (a code editor that fills
   * the modal), the ways to bring it in are a bar on top (Paste · From file… · From URL · Example), what will
   * be created is a sidebar on the right, and the primary action is in the footer. cURL, SOAP/WSDL, `.http`
   * and OpenAPI differ only in the language, the parser behind them and what the sidebar lists; everything
   * else is here, once.
   *
   * A parse error is shown in a bar under the editor and — when the host gives its position — marked in the
   * text itself.
   */
  import type { Component, Snippet } from 'svelte';
  import { ClipboardPaste, FileSearch, Globe, Sparkles } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import type { EditorDiagnostic, LanguageDescriptor } from '$lib/components/shared/ui/code-editor/types';
  import { fsReadTextFile } from '$lib/ipc/fs';
  import { toDiagnostic, type TextError } from './import-diagnostics';

  export type ImportMode = 'paste' | 'file' | 'url' | 'example';

  interface Props {
    title: string;
    icon: Component<any>;
    subtitle?: string;
    language: LanguageDescriptor;
    /** The source text (bindable): what the editor holds. */
    text: string;
    placeholder?: string;
    /** Which ways in are offered; `example` needs {@link example}, `url` needs {@link onFetch}. */
    modes?: ImportMode[];
    example?: string;
    fileExtensions?: string[];
    fileTitle?: string;
    /** A file was chosen (its text is already in the editor): the host may need its folder. */
    onFile?: (path: string) => void;
    urlPlaceholder?: string;
    /** "Fetch" in the URL mode; the host reads the URL itself (the backend does the request). */
    onFetch?: (url: string) => void;
    /** The text came from a URL the host read: the editor shows a note instead of text. */
    fromUrl?: string | null;
    reading?: boolean;
    /** The error, in a bar; with {@link errorAt} also marked in the text. */
    error?: string;
    errorAt?: TextError | null;
    sideTitle: string;
    sideCount?: number;
    /** What will be created. */
    side: Snippet;
    primaryLabel: string;
    primaryDisabled?: boolean;
    primaryLoading?: boolean;
    hint?: string;
    /** Extra controls at the right end of the source bar. */
    extra?: Snippet;
    onPrimary: () => void;
    onClose: () => void;
  }

  let {
    title, icon: Icon, subtitle, language, text = $bindable(), placeholder, modes = ['paste', 'file', 'example'], example = '',
    fileExtensions, fileTitle = 'Choose the file', onFile, urlPlaceholder = 'https://…', onFetch, fromUrl = null,
    reading = false, error = '', errorAt = null, sideTitle, sideCount, side, primaryLabel, primaryDisabled = false,
    primaryLoading = false, hint = 'to import', extra, onPrimary, onClose,
  }: Props = $props();

  const LABELS: Record<ImportMode, { label: string; icon: Component<any> }> = {
    paste: { label: 'Paste', icon: ClipboardPaste },
    file: { label: 'From file…', icon: FileSearch },
    url: { label: 'From URL', icon: Globe },
    example: { label: 'Example', icon: Sparkles },
  };

  let mode = $state<ImportMode>('paste');
  let browsing = $state(false);
  let url = $state('');

  const tabs = $derived(modes.map((m) => ({ id: m, label: LABELS[m].label, icon: LABELS[m].icon, iconSize: 14 })));
  const diagnostics = $derived<EditorDiagnostic[]>(errorAt && text ? [toDiagnostic(text, errorAt)] : []);

  function choose(id: string) {
    mode = id as ImportMode;
    if (mode === 'file') browsing = true;
    else if (mode === 'example') text = example;
  }

  async function fromFile(path: string) {
    browsing = false;
    try {
      text = await fsReadTextFile(path);
      onFile?.(path);
    } catch {
      /* the host shows read errors through its own parse of the text */
    }
  }

  function onKey(e: KeyboardEvent) {
    if (browsing || !(e.ctrlKey || e.metaKey) || e.key !== 'Enter') return;
    e.preventDefault();
    if (!primaryDisabled) onPrimary();
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="min(1180px, 96vw)" height="min(720px, 90vh)" padBody={false} ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Icon size={14} />
      <span class="modal-title">{title}</span>
      {#if subtitle}<span class="is-sub">{subtitle}</span>{/if}
    </ModalHeader>
  {/snippet}

  <div class="is">
    <div class="is-source">
      <Tabs items={tabs} value={mode} variant="panel" size="md" ariaLabel="Where the source comes from" onSelect={choose} />
      {#if mode === 'url'}
        <div class="is-url">
          <Input value={url} placeholder={urlPlaceholder} oninput={(v) => (url = v)} onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); onFetch?.(url); } }} />
          <Button variant="primary" loading={reading} disabled={!url.trim()} onclick={() => onFetch?.(url)}>Fetch</Button>
        </div>
      {:else if mode === 'file'}
        <Button variant="outline" onclick={() => (browsing = true)}>Choose another file…</Button>
      {/if}
      {#if extra}<div class="is-extra">{@render extra()}</div>{/if}
    </div>

    <div class="is-main">
      <div class="is-editor">
        <PaneHeader title="Source" loading={reading} />
        <div class="is-code">
          {#if fromUrl}
            <EmptyState message="Read from the URL" description={fromUrl} compact />
          {:else}
            <CodeEditor value={text} {language} {diagnostics} lineNumbers={false} wrap oninput={(v) => (text = v)} {placeholder} />
          {/if}
        </div>
        {#if error}<div class="is-err"><Alert variant="error" compact text={error} /></div>{/if}
      </div>

      <aside class="is-side">
        <PaneHeader title={sideTitle} count={sideCount} />
        <div class="is-side-body">{@render side()}</div>
      </aside>
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter {hint}>
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" loading={primaryLoading} disabled={primaryDisabled} onclick={onPrimary}>{primaryLabel}</Button>
    </ModalFooter>
  {/snippet}
</Modal>

{#if browsing}
  <FileExplorerModal mode="file" extensions={fileExtensions} title={fileTitle} onConfirm={(p: string) => void fromFile(p)} onCancel={() => (browsing = false)} onClose={() => (browsing = false)} />
{/if}

<style>
  .is { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .is-sub { font-size: var(--font-size-xs); color: var(--text-muted); margin-left: 8px; }
  .is-source { display: flex; align-items: center; gap: 12px; padding: 8px 16px; border-bottom: 1px solid var(--border-subtle); background: var(--bg-elevated); }
  .is-url { display: flex; gap: 8px; flex: 1; min-width: 0; }
  .is-url > :global(:first-child) { flex: 1; min-width: 0; }
  .is-extra { margin-left: auto; display: flex; gap: 8px; }
  .is-main { flex: 1; min-height: 0; display: flex; }
  .is-editor { flex: 1; min-width: 0; display: flex; flex-direction: column; border-right: 1px solid var(--border-subtle); }
  .is-code { flex: 1; min-height: 0; display: flex; }
  .is-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
  .is-err { padding: 6px 12px; border-top: 1px solid var(--border-subtle); }
  .is-side { width: 400px; flex: none; display: flex; flex-direction: column; min-height: 0; background: var(--bg-base); }
  .is-side-body { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: auto; }
</style>
