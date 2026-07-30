<script lang="ts">
  /**
   * BennuNewFileModal — scaffold a new file in a project directory (the tree "New…" action).
   *
   * Pick a kind (Java class/interface/enum/record/annotation, JSP, XML, plain file) and a name; the
   * backend resolves the content (a Java template with the **package inferred** from the target
   * directory, a JSP/XML header, or empty) + the final path. We write it (encoding-aware), open it,
   * refresh the tree and reveal it. Refuses to overwrite an existing file.
   *
   * Keyboard-first: the name auto-focuses; Esc cancels (Modal owns it); Enter / Ctrl+Enter create.
   */
  import { FilePlus2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { newFile, type NewFileKind } from '$lib/ipc/bennu/scaffold';

  let { dir, onClose }: { dir: string; onClose: () => void } = $props();

  const KINDS: { value: NewFileKind; label: string }[] = [
    { value: 'class', label: 'Java Class' },
    { value: 'interface', label: 'Java Interface' },
    { value: 'enum', label: 'Java Enum' },
    { value: 'record', label: 'Java Record' },
    { value: 'annotation', label: 'Java Annotation' },
    { value: 'jsp', label: 'JSP Page' },
    { value: 'xml', label: 'XML File' },
    { value: 'file', label: 'File' },
  ];

  let kind = $state<NewFileKind>('class');
  let name = $state('');
  let busy = $state(false);

  const dirLabel = $derived(dir.split(/[\\/]/).filter(Boolean).slice(-2).join('/'));
  const canCreate = $derived(name.trim().length > 0 && !busy);

  async function create() {
    if (!canCreate) return;
    busy = true;
    try {
      const res = await newFile(dir, name.trim(), kind);
      if (!res) { toastStore.show('Could not create the file', 'error'); return; }
      if (res.exists) {
        toastStore.show(`A file named “${res.path.split('/').pop()}” already exists`, 'warning');
        return;
      }
      await projectStore.saveText(res.path, res.content);
      await projectStore.openFile(res.path);
      projectStore.refreshTree();
      bennuUiStore.revealActiveInTree();
      onClose();
    } catch {
      toastStore.show('Could not create the file', 'error');
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); void create(); }
  }

  let nameEl = $state<HTMLInputElement | undefined>();
  $effect(() => { nameEl?.focus(); });
</script>

<Modal {onClose} width="480px" height="300px" ariaLabel="New file">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FilePlus2 size={14} />
      <span class="modal-title">New file</span>
      <span class="nf-dir">in <code>{dirLabel}</code></span>
    </ModalHeader>
  {/snippet}

  <div class="nf" onkeydown={onKey} role="presentation">
    <label class="nf-field">
      <span class="nf-label">Kind</span>
      <Select bind:value={kind} options={KINDS} onchange={(v) => (kind = v as NewFileKind)} />
    </label>
    <label class="nf-field">
      <span class="nf-label">Name</span>
      <input
        class="nf-input"
        bind:this={nameEl}
        bind:value={name}
        placeholder={kind === 'file' ? 'file name (with extension)' : 'name'}
        spellcheck="false"
        autocomplete="off"
      />
    </label>
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
      <Button variant="primary" size="sm" disabled={!canCreate} loading={busy}
        tooltip={{ content: 'Create', shortcut: 'Enter' }} onclick={create}>Create</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .nf { display: flex; flex-direction: column; gap: 14px; padding: 4px 2px; }
  .nf-field { display: flex; flex-direction: column; gap: 6px; }
  .nf-label { font-size: var(--font-size-2xs); font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--text-muted); }
  .nf-dir { font-size: var(--font-size-xs); color: var(--text-muted); }
  .nf-dir code { font-family: var(--font-code); color: var(--text-secondary); font-size: var(--font-size-xs); }
  .nf-input { width: 100%; padding: 7px 10px; background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius-md); color: var(--text-primary); font-family: var(--font-code); font-size: var(--font-size-md); outline: none; }
  .nf-input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-subtle); }
  .nf-input::placeholder { color: var(--text-disabled); }
</style>
