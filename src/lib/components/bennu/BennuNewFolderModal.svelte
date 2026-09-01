<script lang="ts">
  /**
   * BennuNewFolderModal — create a directory in the project tree, or a package.
   *
   * ## One field, several levels
   *
   * The name is a **path**, not a name: `assets/icons` makes two directories, because the
   * alternative is opening this dialog once per level — which is what you notice every single
   * time you scaffold a chain. Under a Java source root a dot separates too (`it.acme.web` is
   * three directories), since that is how the thing being named is written, and it is also how
   * the tree already draws it.
   *
   * Everywhere else the dot stays an ordinary character, so `.github` and `my.config` are one
   * folder each. Which rule applies is `asPackage`, decided by the caller from the same
   * source-root list that collapses package rows in the tree — one answer, not two that can
   * drift.
   *
   * The line under the field shows what will exist when you press Enter. That is the whole
   * reason a path in a name field is not a trick: you can see it become one.
   *
   * Levels that are already there are stepped through rather than refused — typing
   * `src/main/resources` where `src/main` exists creates `resources` and says so.
   *
   * Keyboard-first: the field auto-focuses, Enter creates, Esc cancels (Modal owns it).
   */
  import { FolderPlus, Package } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { newFolder } from '$lib/ipc/bennu/file-ops';

  let {
    /** The directory to create in (absolute). */
    dir,
    /** Whether `dir` is package territory — a Java source root, or inside one. */
    asPackage = false,
    onClose,
  }: { dir: string; asPackage?: boolean; onClose: () => void } = $props();

  let name = $state('');
  let busy = $state(false);

  /** The characters no filesystem takes. The separators are absent on purpose — they have
   *  already done their job by the time a segment is checked. */
  const INVALID = /[:*?"<>|]/;

  /**
   * The levels the typed name stands for.
   *
   * A second implementation of the backend's own split, and deliberately: this one exists to
   * *show* the answer while you type, and a round-trip per keystroke to be told what a slash
   * means would be a round-trip to learn nothing. The backend stays the authority — it is the
   * one that refuses, and it re-splits what it is given.
   */
  const segments = $derived(
    name
      .split(/[\\/]/)
      .flatMap((level) => (asPackage ? level.split('.') : [level]))
      .map((s) => s.trim())
      .filter(Boolean),
  );

  const problem = $derived.by(() => {
    if (!name.trim()) return null; // not an error, just nothing typed yet
    if (segments.length === 0) return 'Type a name';
    const bad = segments.find((s) => s === '.' || s === '..');
    if (bad) return 'A folder cannot be named “.” or “..”';
    const illegal = segments.find((s) => INVALID.test(s));
    if (illegal) return `“${illegal}” can't be a folder name: : * ? " < > | aren't allowed`;
    return null;
  });

  const canCreate = $derived(segments.length > 0 && !problem && !busy);

  /** Where the new folders will be, relative to the project — the preview line. */
  const dirRel = $derived(projectStore.relativePath(dir));
  const preview = $derived(
    (dirRel === '.' ? '' : `${dirRel}/`) + segments.join('/'),
  );

  const title = $derived(asPackage ? 'New Package' : 'New Folder');
  const placeholder = $derived(asPackage ? 'it.acme.web' : 'assets/icons');

  async function create() {
    if (!canCreate) return;
    const root = projectStore.project?.root;
    if (!root) return;
    busy = true;
    try {
      const res = await newFolder(root, dir, name.trim(), asPackage);
      // The tree first, then the reveal: the reveal searches the tree, so a row that has not
      // arrived yet is a row it reports as missing.
      await projectStore.refreshTree();
      bennuUiStore.focusInTree(res.path);
      if (res.existed) {
        toastStore.show(`${projectStore.relativePath(res.path)} already exists`, 'warning');
      } else if (res.created.length === 1) {
        toastStore.show(`Created ${projectStore.relativePath(res.created[0])}`, 'success');
      } else {
        toastStore.show(
          `Created ${res.created.length} folders · ${projectStore.relativePath(res.path)}`,
          'success',
        );
      }
      onClose();
    } catch (e) {
      toastStore.show(e instanceof Error ? e.message : String(e), 'error');
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

<Modal {onClose} width="480px" height="auto" padBody={false} ariaLabel="New folder">
  {#snippet header()}
    <ModalHeader {onClose}>
      {#if asPackage}<Package size={14} />{:else}<FolderPlus size={14} />{/if}
      <span class="modal-title">{title}</span>
    </ModalHeader>
  {/snippet}

  <div class="nd" onkeydown={onKey} role="presentation">
    <input
      class="nd-input"
      bind:this={nameEl}
      bind:value={name}
      {placeholder}
      spellcheck="false"
      autocomplete="off"
      aria-label={title}
      data-modal-autofocus
    />

    <!-- One line, always present: a note that appears and disappears would make the dialog
         jump as you type. -->
    <div class="nd-note">
      {#if problem}
        <span class="nd-bad">{problem}</span>
      {:else if segments.length > 1}
        <!-- The point of the whole dialog, said out loud: a slash (or a dot, in a package) is
             another level, and here is the chain it makes. -->
        <span class="nd-preview">{segments.length} folders · <code>{preview}</code></span>
      {:else if segments.length === 1}
        <span class="nd-preview"><code>{preview}</code></span>
      {:else}
        <span class="nd-hint">
          {asPackage
            ? 'A dot or a slash makes another level — it.acme.web is three folders.'
            : 'A slash makes another level — assets/icons is two folders.'}
        </span>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        size="sm"
        disabled={!canCreate}
        loading={busy}
        tooltip={{ content: 'Create', shortcut: 'Enter' }}
        onclick={() => void create()}
      >Create</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .nd { display: flex; flex-direction: column; gap: 8px; padding: 10px; min-height: 0; }
  .nd-input {
    padding: 6px 10px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: calc(var(--font-size-md) * 1.05);
  }
  .nd-input:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-subtle); }
  .nd-input::placeholder { color: var(--text-disabled); }
  .nd-note { min-height: 16px; font-size: 11px; display: flex; align-items: center; gap: 5px; }
  .nd-bad { color: var(--error); }
  .nd-hint { color: var(--text-disabled); }
  .nd-preview { color: var(--info); display: flex; align-items: center; gap: 5px; }
  .nd-preview code { font-family: var(--font-code); color: var(--text-secondary); }
</style>
