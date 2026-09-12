<script lang="ts">
  /**
   * Generate from one of the user's templates, on the class at the caret — see it, then write it.
   *
   * Where the result goes is the template's to say (`bennu.output`): members go into the class, as
   * one undo step; a file is created beside it; text is copied, or appended to a file you choose —
   * which is what configuration properties are, since the file they belong in is the project's
   * business. The caret is read once, when the dialog opens, and nothing is inserted into a buffer
   * that has changed since the preview was computed against it.
   */
  import { onMount } from 'svelte';
  import { ClipboardCopy, FilePlus2, FileText, Wand2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { baseName } from '$lib/utils/paths';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bufferOf } from '$lib/stores/bennu/buffer-wait';
  import { createFileFromTemplate, renderTemplate, type RenderedTemplate } from '$lib/ipc/bennu/templates';
  import { languageForPath } from '../languages';
  import BennuTemplateBar from './BennuTemplateBar.svelte';
  import BennuTemplateOutput from './BennuTemplateOutput.svelte';

  type Edit = { start: number; end: number; replacement: string };

  let {
    kind,
    caretContext,
    applyEdits,
    insertSnippet,
    onClose,
  }: {
    kind: 'class' | 'config-properties';
    caretContext: () => { source: string; offset: number } | null;
    applyEdits: (edits: readonly Edit[]) => void;
    /** Insert into the open buffer with its tab stops armed — for a template that asked for them.
     *  Absent, or given no stops, the insertion goes through `applyEdits` like any other. */
    insertSnippet?: (
      offset: number,
      text: string,
      stops: readonly { start: number; end: number; group?: number }[],
    ) => void;
    onClose: () => void;
  } = $props();

  interface Origin { root: string; file: string; source: string; offset: number }

  let origin = $state<Origin | null>(null);
  let template = $state<string | null>(null);
  let rendered = $state<RenderedTemplate | null>(null);
  let error = $state<string | null>(null);
  let rendering = $state(false);
  let writing = $state(false);
  let appending = $state(false);

  onMount(() => {
    const root = projectStore.project?.root;
    const file = projectStore.activeFilePath;
    const ctx = caretContext();
    if (!root || !file || !file.toLowerCase().endsWith('.java') || !ctx) {
      error = 'Put the caret in a Java class first.';
      return;
    }
    origin = { root, file, source: ctx.source, offset: ctx.offset };
  });

  $effect(() => {
    const at = origin;
    const chosen = template;
    if (!at) return;
    rendering = true;
    renderTemplate({ root: at.root, kind, template: chosen, file: at.file, source: at.source, offset: at.offset })
      .then((answer) => { if (at === origin && chosen === template) { rendered = answer; error = null; } })
      .catch((e) => { if (at === origin && chosen === template) { rendered = null; error = String(e); } })
      .finally(() => { rendering = false; });
  });

  const title = $derived(kind === 'config-properties' ? 'Configuration properties' : 'Generate from a template');
  const shown = $derived(rendered ? rendered.insertion?.text ?? rendered.text : '');
  const language = $derived(
    languageForPath(rendered?.output === 'text' && kind === 'config-properties' ? 'application.yml' : 'Preview.java'),
  );

  const byteLength = (text: string) => new TextEncoder().encode(text).length;

  /** Members into the class, as one undo step — refused when the buffer moved since the preview. */
  async function insertMembers(result: RenderedTemplate, at: Origin) {
    const insertion = result.insertion;
    if (!insertion) return;
    await projectStore.openFile(at.file);
    const buffer = await bufferOf(at.file, at.source, () => caretContext()?.source ?? null);
    if (buffer !== at.source) {
      toastStore.show('The class changed since the preview — generate again', 'info');
      const ctx = caretContext();
      if (ctx) origin = { ...at, source: ctx.source, offset: ctx.offset };
      return;
    }
    const stops = result.insertion_stops ?? [];
    if (stops.length && insertSnippet) {
      // The imports first: they are edits above the insertion point, and applying them afterwards
      // would move the stops that were just armed.
      if (result.import_edits.length) applyEdits(result.import_edits);
      const shift = result.import_edits
        .filter((e) => e.start <= insertion.offset)
        .reduce((n, e) => n + byteLength(e.replacement) - (e.end - e.start), 0);
      insertSnippet(insertion.offset + shift, insertion.text, stops);
    } else {
      // The imports with the members, against the same text — one undo step for both.
      applyEdits([{ start: insertion.offset, end: insertion.offset, replacement: insertion.text }, ...result.import_edits]);
    }
    toastStore.show('Generated — save the file to keep it', 'success');
    onClose();
  }

  async function createFile(result: RenderedTemplate, at: Origin) {
    if (!result.file) return;
    if (result.exists) {
      await projectStore.openFile(result.file);
      onClose();
      return;
    }
    await createFileFromTemplate(at.root, result.file, result.text);
    await projectStore.openFile(result.file);
    void projectStore.refreshTree();
    toastStore.show(`Created ${baseName(result.file)}`, 'success');
    onClose();
  }

  async function primary() {
    const result = rendered;
    const at = origin;
    if (!result || !at || writing) return;
    writing = true;
    try {
      if (result.output === 'members') await insertMembers(result, at);
      else if (result.output === 'file') await createFile(result, at);
      else if (await copyToClipboard(result.text)) {
        toastStore.show('Copied', 'success');
        onClose();
      }
    } catch (e) {
      toastStore.show(`Couldn't write it: ${e}`, 'error');
    } finally {
      writing = false;
    }
  }

  /** The text at the end of a file, in its buffer — the file's own unsaved edits stay where they are. */
  async function appendTo(path: string) {
    appending = false;
    const result = rendered;
    if (!result) return;
    try {
      await projectStore.openFile(path);
      const buffer = await bufferOf(path, null, () => caretContext()?.source ?? null);
      if (buffer === null) {
        toastStore.show('The file did not open', 'error');
        return;
      }
      const separator = buffer === '' || buffer.endsWith('\n') ? '' : '\n';
      const end = byteLength(buffer);
      applyEdits([{ start: end, end, replacement: separator + result.text }]);
      toastStore.show(`Appended to ${baseName(path)} — save it to keep the change`, 'success');
      onClose();
    } catch (e) {
      toastStore.show(`Couldn't append: ${e}`, 'error');
    }
  }

  const primaryLabel = $derived(
    !rendered
      ? 'Generate'
      : rendered.output === 'members'
        ? 'Insert into the class'
        : rendered.output === 'file'
          ? rendered.exists ? 'Open the existing file' : 'Create the file'
          : 'Copy',
  );

  function onWindowKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && !appending) {
      e.preventDefault();
      void primary();
    }
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<Modal {onClose} width="820px" height="600px" padBody={false} ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Wand2 size={14} />
      <span class="modal-title">{title}</span>
      {#if origin}<span class="file" use:tooltip={origin.file}>{baseName(origin.file)}</span>{/if}
    </ModalHeader>
  {/snippet}

  <div class="body">
    <div class="top">
      <FormField label="Template">
        <BennuTemplateBar {kind} value={template} onchange={(next) => (template = next)} />
      </FormField>
    </div>
    {#if error}
      <div class="alert"><Alert variant="error" compact text={error} /></div>
    {/if}
    {#if rendered}
      <BennuTemplateOutput {rendered} root={origin?.root ?? null} text={shown} {language} />
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="hint">Nothing is written until you choose — Ctrl+Enter</span>
      <div class="buttons">
        <Button variant="ghost" onclick={onClose}>Cancel</Button>
        {#if rendered?.output === 'text'}
          <Button variant="ghost" onclick={() => (appending = true)}>
            {#snippet iconStart()}<FileText size={13} />{/snippet}
            Append to a file…
          </Button>
        {/if}
        <Button variant="primary" loading={writing || rendering} disabled={!rendered} onclick={() => void primary()}>
          {#snippet iconStart()}
            {#if rendered?.output === 'text'}<ClipboardCopy size={13} />{:else}<FilePlus2 size={13} />{/if}
          {/snippet}
          {primaryLabel}
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

{#if appending}
  <FileExplorerModal
    mode="file"
    extensions={kind === 'config-properties' ? ['yml', 'yaml', 'properties'] : undefined}
    title="Append to…"
    initialPath={origin?.root}
    onConfirm={(path) => void appendTo(path)}
    onCancel={() => (appending = false)}
    onClose={() => (appending = false)}
  />
{/if}

<style>
  .body { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .top { padding: 10px 12px 6px; }
  .alert { padding: 0 12px 6px; }
  .file { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted); }
  .hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .buttons { display: flex; align-items: center; gap: 6px; }
</style>
