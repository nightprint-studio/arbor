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
   * ## A package is a name, not a place
   *
   * In package territory the field opens **pre-filled with the package you were on**, and every
   * level of it is editable — the folders are then created from the **source root**, not from the
   * row that was selected.
   *
   * That is the whole of what "New Package on `it.acme.web` can only make children of
   * `it.acme.web`" was missing. A sibling (`it.acme.model`) and a package one level up
   * (`it.other`) are things you want roughly as often as a child, and with the prefix fixed the
   * only way to either was to walk up the tree first and open this dialog somewhere else — for a
   * dialog whose entire subject is a dotted name you could have simply typed. It is also what
   * IntelliJ does, which matters here because it is what the muscle memory expects.
   *
   * Outside package territory the field stays empty and the folders are created **in** `dir`: a
   * directory chain is a place, and `assets/icons` typed on `src/` means one inside the other.
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
  import { packageOfDir, sourceRootOf } from './package-tree';

  let {
    /** The directory to create in (absolute). */
    dir,
    /** Whether `dir` is package territory — a Java source root, or inside one. */
    asPackage = false,
    onClose,
  }: { dir: string; asPackage?: boolean; onClose: () => void } = $props();

  /**
   * Where the levels are created.
   *
   * For a package that is the **source root**: the typed name is the whole package, so it has to
   * be resolved from the root the packages hang off, or deleting a segment would create a folder
   * called `it.acme` next to the one it was meant to replace. Falls back to `dir` when the path is
   * not under a recognised source root — which is also what a plain folder always uses.
   */
  const base = $derived((asPackage ? sourceRootOf(dir) : null) ?? dir);

  /** The package `dir` stands for, dotted. `''` at the source root (the default package). */
  const currentPackage = $derived(asPackage ? (packageOfDir(dir) ?? '') : '');

  // Pre-filled with where you are, with a trailing dot so the common case — a child — is Enter
  // after one word, and the uncommon ones are a few Backspaces. Not `$derived`: this is the field's
  // starting value, and the user edits it.
  let name = $state(currentPackage ? `${currentPackage}.` : '');
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
  const baseRel = $derived(projectStore.relativePath(base));
  const preview = $derived(
    (baseRel === '.' ? '' : `${baseRel}/`) + segments.join('/'),
  );

  /** The package the typed name stands for — what the preview says in the language the dialog is
   *  actually in. Empty outside package territory. */
  const packagePreview = $derived(asPackage ? segments.join('.') : '');

  const title = $derived(asPackage ? 'New Package' : 'New Folder');
  const placeholder = $derived(asPackage ? 'it.acme.web' : 'assets/icons');

  async function create() {
    if (!canCreate) return;
    const root = projectStore.project?.root;
    if (!root) return;
    busy = true;
    try {
      const res = await newFolder(root, base, name.trim(), asPackage);
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
  // Focused with the caret at the END, never selected: the pre-filled prefix is the thing you
  // usually want to keep, and a selection would make the first keystroke delete it.
  $effect(() => {
    const el = nameEl;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  });
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
      {:else if asPackage && segments.length > 0}
        <!-- In package territory the useful preview is the PACKAGE, not the folder chain: it is
             what the name means, and the only way to see that a deleted segment moved you up a
             level rather than sideways. The path follows it, because that is where it lands. -->
        <span class="nd-preview">
          <code>{packagePreview}</code>
          <span class="nd-path">{preview}</span>
        </span>
      {:else if segments.length > 1}
        <!-- The point of the whole dialog, said out loud: a slash is another level, and here is
             the chain it makes. -->
        <span class="nd-preview">{segments.length} folders · <code>{preview}</code></span>
      {:else if segments.length === 1}
        <span class="nd-preview"><code>{preview}</code></span>
      {:else}
        <span class="nd-hint">
          {asPackage
            ? 'The whole package — edit any part of it to create a sibling or one further up.'
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
  .nd-path { color: var(--text-disabled); font-family: var(--font-code); }
</style>
