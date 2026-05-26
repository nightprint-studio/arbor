<!--
  MarkdownEditorModal — Obsidian-style live preview markdown editor.

  Behaviour:
    · Opens via `markdownStore.openFile({ path, … })` — usually wired from
      the Files panel context menu on .md / .markdown rows.
    · The editor renders markdown inline in the same pane: bold/italic/
      headings/code/blockquote/links are styled live, and the surrounding
      markup characters are concealed on every line OTHER than the one
      the cursor is on. Switching between source and rendered view is
      automatic — exactly like Obsidian.
    · Default mode is Edit; the eye button in the header toggles read-only
      when the user just wants to scroll the rendered output.
    · `Ctrl/Cmd+S` saves; closing while dirty prompts via ConfirmModal.

  Backed by `createMarkdownExtensions` (CodeMirror 6 + Lezer markdown),
  defined in `$lib/utils/markdown-editor`.
-->
<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { FileText, Save, Eye, Pencil, Loader2 } from 'lucide-svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';

  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ModalFooter from './ModalFooter.svelte';
  import ConfirmModal from './ConfirmModal.svelte';
  import Button from './ui/Button.svelte';
  import Spinner from './ui/Spinner.svelte';
  import StateBlock from './ui/StateBlock.svelte';
  import {
    createMarkdownExtensions,
    makeMarkdownCompartments,
  } from '$lib/utils/markdown-editor';
  import { markdownStore } from '$lib/stores/markdown.svelte';
  import { fsReadTextFile, fsWriteTextFile } from '$lib/ipc/fs';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // ── Component state ─────────────────────────────────────────────────────

  let hostEl   = $state<HTMLDivElement | undefined>(undefined);
  let view     = $state<EditorView | null>(null);
  const compartments = makeMarkdownCompartments();

  let loading      = $state(true);
  let loadError    = $state<string | null>(null);
  let savedText    = $state('');         // last text on disk
  let currentText  = $state('');         // text in the editor
  let saving       = $state(false);
  let readOnly     = $state(false);
  let askDiscard   = $state(false);      // ConfirmModal flag

  const dirty = $derived(currentText !== savedText);

  const path     = $derived(markdownStore.path);
  const filename = $derived(markdownStore.filename);

  // ── Load file when the modal opens for a new path ───────────────────────

  $effect(() => {
    const p = path;
    if (!p) return;
    void loadFile(p);
  });

  async function loadFile(p: string) {
    loading   = true;
    loadError = null;
    // Tear down any previous editor first — switching files starts fresh
    // (the `bind:this` slot is going to be re-created when the `{#else}`
    // branch re-renders below, so a stale `view` would leak).
    untrack(() => {
      view?.destroy();
      view = null;
    });
    try {
      const text = await fsReadTextFile(p);
      savedText   = text;
      currentText = text;
    } catch (err) {
      loadError = `${err}`;
      loading   = false;
      return;
    }
    // Flip the conditional render BEFORE awaiting tick, so the editor host
    // div is in the DOM by the time we mount CodeMirror onto it.
    loading = false;
    await tick();
    mountEditor();
  }

  function mountEditor() {
    if (!hostEl) return;
    view = new EditorView({
      parent: hostEl,
      state: EditorState.create({
        doc: currentText,
        extensions: [
          createMarkdownExtensions({ readOnly }, compartments),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) {
              currentText = u.state.doc.toString();
            }
          }),
        ],
      }),
    });
    // Focus the editor so typing flows immediately.
    queueMicrotask(() => view?.focus());
  }

  // Apply read-only toggle without rebuilding the editor.
  $effect(() => {
    const ro = readOnly;
    if (!view) return;
    view.dispatch({
      effects: compartments.readOnly.reconfigure(
        EditorState.readOnly.of(ro),
      ),
    });
  });

  onMount(() => {
    return () => { view?.destroy(); view = null; };
  });

  // ── Save flow ───────────────────────────────────────────────────────────

  async function save() {
    const p = path;
    if (!p || saving) return;
    if (!dirty) return;
    saving = true;
    try {
      await fsWriteTextFile(p, currentText);
      savedText = currentText;
      uiStore.showToast(`Saved ${filename ?? 'file'}`, 'success', 1500);
    } catch (err) {
      uiStore.showToast(`Save failed: ${err}`, 'error');
    } finally {
      saving = false;
    }
  }

  // ── Close / dirty guard ─────────────────────────────────────────────────

  function tryClose() {
    if (dirty) { askDiscard = true; return; }
    markdownStore.close();
  }

  function discardAndClose() {
    askDiscard = false;
    markdownStore.close();
  }

  // ── Keyboard: Ctrl/Cmd+S → Save ─────────────────────────────────────────

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      void save();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Modal
  onClose={tryClose}
  width="min(1100px, 95vw)"
  height="min(820px, 92vh)"
  ariaLabel={filename ?? 'Markdown editor'}
  padBody={false}
>
  {#snippet header()}
    <ModalHeader onClose={tryClose}>
      {#snippet children()}
        <span class="header-icon"><FileText size={14} /></span>
        <span class="modal-title">{filename ?? 'Markdown'}</span>
        {#if dirty}<span class="dirty-pill" use:tooltip={'Unsaved changes'}>● Unsaved</span>{/if}
      {/snippet}
      {#snippet actions()}
        <button
          class="hdr-btn"
          class:active={readOnly}
          onclick={() => (readOnly = !readOnly)}
          aria-pressed={readOnly}
          use:tooltip={readOnly ? 'Switch to Edit mode' : 'Switch to Read-only mode'}
        >
          {#if readOnly}<Pencil size={13} />{:else}<Eye size={13} />{/if}
        </button>
      {/snippet}
    </ModalHeader>
  {/snippet}

  {#if loading}
    <div class="state-wrap">
      <StateBlock tone="loading" label="Loading…">
        {#snippet spinner()}<Spinner size={20} />{/snippet}
      </StateBlock>
    </div>
  {:else if loadError}
    <div class="state-wrap">
      <StateBlock tone="error" label={`Could not open file: ${loadError}`} />
    </div>
  {:else}
    <div class="editor-host" bind:this={hostEl}></div>
  {/if}

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="footer-status">
        {#if saving}
          <Loader2 size={12} class="spin" /> Saving…
        {:else if dirty}
          <span class="muted">Press <kbd>Ctrl</kbd>+<kbd>S</kbd> to save</span>
        {:else if !loading && !loadError}
          <span class="muted">All changes saved</span>
        {/if}
      </span>
      <span class="footer-buttons">
        <Button variant="secondary" onclick={tryClose}>Close</Button>
        <Button variant="primary" onclick={save} disabled={!dirty || saving}>
          <Save size={13} />
          Save
        </Button>
      </span>
    </ModalFooter>
  {/snippet}
</Modal>

{#if askDiscard}
  <ConfirmModal
    title="Discard unsaved changes?"
    message={`"${filename ?? 'This file'}" has unsaved changes that will be lost.`}
    variant="warning"
    confirmLabel="Discard"
    cancelLabel="Keep editing"
    onConfirm={discardAndClose}
    onCancel={() => (askDiscard = false)}
  />
{/if}

<style>
  .header-icon { color: var(--accent); display: flex; }

  .dirty-pill {
    margin-left: 8px;
    padding: 1px 8px;
    border-radius: 999px;
    background: rgba(255,196,77,0.12);
    color: var(--warning, #ffc44d);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.3px;
  }

  .hdr-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .hdr-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .hdr-btn.active {
    background: var(--accent-subtle);
    border-color: rgba(77,120,204,0.3);
    color: var(--accent);
  }

  .state-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
  }

  .editor-host {
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .editor-host :global(.cm-editor) {
    height: 100%;
    background: var(--bg-base);
  }

  .footer-status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .footer-status .muted { color: var(--text-muted); }
  .footer-status kbd {
    font-family: var(--font-code);
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 3px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
  }

  .footer-buttons {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  :global(.spin) { animation: spin 0.9s linear infinite; }
  @keyframes spin {
    from { transform: rotate(0deg); } to { transform: rotate(360deg); }
  }
</style>
