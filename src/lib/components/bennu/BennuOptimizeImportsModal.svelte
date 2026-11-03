<script lang="ts">
  /**
   * BennuOptimizeImportsModal — Optimize imports over a folder: the whole project, a module, or any
   * directory of the tree.
   *
   * It opens already working: the plan for the folder is built on open and again whenever the folder
   * changes, so the common case is open → read the list → Ctrl+Enter. Nothing is written until then,
   * and the backend refuses any file that changed after the plan was made.
   *
   * Files with unsaved edits are left out and said so — optimizing the disk copy of a file whose
   * buffer says something else would either lose the edits or fight them.
   */
  import { Package, FileCode2, FolderOpen } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import {
    previewOptimizeImports, applyOptimizeImports, type ImportsPlan,
  } from '$lib/ipc/bennu/optimize-imports';

  interface Props {
    /** The folder to open on. */
    scope: string;
    onClose: () => void;
  }

  let { scope: initialScope, onClose }: Props = $props();

  const root = $derived(projectStore.project?.root ?? '');
  let scope = $state(initialScope);
  let plan = $state<ImportsPlan | null>(null);
  let loading = $state(true);
  let applying = $state(false);
  let error = $state<string | null>(null);
  let picking = $state(false);
  let seq = 0;

  const files = $derived(plan?.files ?? []);
  const removedTotal = $derived(files.reduce((n, f) => n + f.removed, 0));
  const scopeLabel = $derived(projectStore.relativePath(scope) || '(whole project)');

  // One plan per folder; a superseded answer is dropped rather than shown under the wrong folder.
  $effect(() => {
    const folder = scope;
    const mine = ++seq;
    loading = true;
    error = null;
    plan = null;
    previewOptimizeImports(root, folder, projectStore.dirtyPaths)
      .then((p) => { if (mine === seq) plan = p; })
      .catch((e) => { if (mine === seq) error = String(e); })
      .finally(() => { if (mine === seq) loading = false; });
  });

  async function apply() {
    if (!plan || !files.length || applying) return;
    applying = true;
    try {
      const done = await applyOptimizeImports(
        root,
        files.map((f) => ({ file: f.file, digest: f.digest, after: f.after })),
      );
      // Open tabs show the old text; reload them (they are clean — dirty ones were left out).
      await Promise.all(done.written.map((f) => projectStore.reload(f)));
      if (done.refused.length) {
        toastStore.show(
          `${done.written.length} optimized, ${done.refused.length} left alone (changed meanwhile)`, 'warning',
        );
      } else {
        toastStore.show(`Optimized imports in ${done.written.length} ${done.written.length === 1 ? 'file' : 'files'}`, 'success');
      }
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      applying = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void apply();
    }
  }

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }
</script>

<Modal width="680px" height="520px" {onClose}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Package size={14} />
      <span class="modal-title">Optimize imports</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="oi" onkeydown={onKeydown}>
    <div class="oi-scope">
      <span class="oi-scope-label">In</span>
      <code class="oi-scope-path">{scopeLabel}</code>
      <Button variant="outline" size="sm" onclick={() => (picking = true)}>
        <FolderOpen size={12} /> Change…
      </Button>
      {#if scope !== root}
        <Button variant="ghost" size="sm" onclick={() => (scope = root)}>Whole project</Button>
      {/if}
    </div>

    {#if error}
      <Alert variant="error">{error}</Alert>
    {:else if loading}
      <div class="oi-center"><Spinner /> <span>Reading the Java files…</span></div>
    {:else if plan}
      {#if plan.skipped.length}
        <Alert variant="warning">
          {plan.skipped.length} {plan.skipped.length === 1 ? 'file has' : 'files have'} unsaved edits and
          {plan.skipped.length === 1 ? 'is' : 'are'} left out: {plan.skipped.map(baseName).join(', ')}.
        </Alert>
      {/if}
      {#if files.length === 0}
        <Alert variant="info">
          Nothing to change in {plan.scanned} Java {plan.scanned === 1 ? 'file' : 'files'}. Files with a comment
          among their imports are left alone.
        </Alert>
      {:else}
        <p class="oi-lead">
          {files.length} of {plan.scanned} Java {plan.scanned === 1 ? 'file' : 'files'} would change —
          {removedTotal} {removedTotal === 1 ? 'import' : 'imports'} removed, the rest put in order.
        </p>
        <div class="oi-list">
          {#each files as f (f.file)}
            <div class="oi-row">
              <FileCode2 size={12} />
              <span class="oi-rel">{f.rel}</span>
              <span class="oi-count">{f.removed ? `−${f.removed}` : 'reordered'}</span>
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        disabled={!files.length || applying}
        loading={applying}
        tooltip={{ content: 'Write the changes', shortcut: 'Ctrl+Enter' }}
        onclick={() => void apply()}
      >Optimize</Button>
    </ModalFooter>
  {/snippet}
</Modal>

{#if picking}
  <FileExplorerModal
    mode="folder"
    title="Optimize imports in…"
    initialPath={scope}
    onConfirm={(path) => { picking = false; scope = path.replace(/\\/g, '/'); }}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

<style>
  .oi {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    min-height: 0;
    height: 100%;
  }
  .oi-scope { display: flex; align-items: center; gap: 8px; }
  .oi-scope-label { color: var(--text-muted); font-size: var(--font-size-sm); }
  .oi-scope-path {
    flex: 1;
    min-width: 0;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .oi-center {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 32px 0;
    color: var(--text-muted);
  }
  .oi-lead { margin: 0; }
  .oi-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .oi-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 10px;
    font-size: var(--font-size-xs);
  }
  .oi-row :global(svg) { color: var(--text-disabled); flex: none; }
  .oi-rel {
    flex: 1;
    min-width: 0;
    font-family: var(--font-code);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .oi-count {
    flex: none;
    font-family: var(--font-code);
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
  }
</style>
