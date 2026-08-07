<script lang="ts">
  /**
   * Rename a file, and say what that does to the code before it happens.
   *
   * ## The preview is the point
   *
   * Renaming `parser.rs` is not a filesystem operation: something declares `mod parser;` and other
   * files say `use crate::parser::…`, and a rename that moves only the file leaves a project that
   * does not build. So the dialog asks the language server what the rename implies — the same
   * `willRenameFiles` question the rename itself asks — and shows the answer *while you type the new
   * name*. "3 files will be updated" is the difference between a rename you can trust and one you run
   * and then go looking for the damage.
   *
   * The preview is debounced and best-effort: no server, a language that has no such notion, or a
   * server still indexing all read as "nothing to update", and the rename still happens. It never
   * blocks the dialog.
   *
   * ## Keyboard
   *
   * The name field is the whole dialog: it auto-focuses with the **base name selected** (IntelliJ's
   * behaviour — the extension is almost never what you are changing), Enter renames, Esc cancels.
   */
  import { FileType2, FileCode2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { lspWillRename } from '$lib/ipc/bennu/lsp';
  import { baseName } from '$lib/utils/paths';

  let {
    /** Absolute path of the file to rename. */
    path,
    onClose,
  }: { path: string; onClose: () => void } = $props();

  const currentName = $derived(baseName(path));
  const parent = $derived(path.replace(/[\\/][^\\/]*$/, ''));

  // The starting point, not a binding: the dialog is mounted for one file and unmounted when it
  // closes, so `path` cannot change under it.
  // svelte-ignore state_referenced_locally
  let name = $state(baseName(path));
  let busy = $state(false);

  /** How many files the rename would edit, and which — `null` until an answer arrives. */
  let implied = $state<{ files: string[]; edits: number } | null>(null);
  let checking = $state(false);

  /** Characters no filesystem accepts in a name, plus the separators — a name is a NAME here, so a
   *  path typed into this field is a mistake rather than a move. */
  const INVALID = /[\\/:*?"<>|]/;
  const trimmed = $derived(name.trim());
  const problem = $derived.by(() => {
    if (!trimmed) return 'Type a name';
    if (INVALID.test(trimmed)) return 'A name cannot contain \\ / : * ? " < > |';
    if (trimmed === currentName) return null; // not an error, just nothing to do
    return null;
  });
  const canRename = $derived(!!trimmed && !problem && trimmed !== currentName && !busy);

  /**
   * Ask what the rename would imply, on a debounce.
   *
   * Keyed on the target name, and only when it is a name worth asking about: an invalid one and the
   * current one both have nothing to answer. The result is dropped when the name has moved on, so a
   * slow answer cannot describe a name you have already changed.
   */
  $effect(() => {
    const target = trimmed;
    if (!target || problem || target === currentName) {
      implied = null;
      return;
    }
    let cancelled = false;
    checking = true;
    const t = setTimeout(() => {
      void lspWillRename(path, `${parent}/${target}`)
        .then((edits) => {
          if (cancelled) return;
          const files = [...new Set(edits.map((e) => e.file))];
          implied = { files, edits: edits.length };
        })
        .catch(() => { if (!cancelled) implied = null; })
        .finally(() => { if (!cancelled) checking = false; });
    }, 350);
    return () => { cancelled = true; checking = false; clearTimeout(t); };
  });

  async function rename() {
    if (!canRename) return;
    busy = true;
    try {
      const newPath = await projectStore.renameFile(path, trimmed);
      const n = implied?.files.length ?? 0;
      toastStore.show(
        n ? `Renamed to “${baseName(newPath)}” · updated ${n} file${n === 1 ? '' : 's'}`
          : `Renamed to “${baseName(newPath)}”`,
        'success',
      );
      onClose();
    } catch (e) {
      toastStore.show(String(e instanceof Error ? e.message : e), 'error');
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); void rename(); }
  }

  /**
   * Focus the field with the base name selected.
   *
   * Once, at mount — a re-select on every keystroke would fight the typing. The extension stays out
   * of the selection because changing it is the rare case, and when it IS the case the caret is
   * already one End away.
   */
  let nameEl = $state<HTMLInputElement | undefined>();
  let focused = false;
  $effect(() => {
    if (focused || !nameEl) return;
    focused = true;
    nameEl.focus();
    const dot = currentName.lastIndexOf('.');
    nameEl.setSelectionRange(0, dot > 0 ? dot : currentName.length);
  });
</script>

<Modal {onClose} width="480px" height="auto" padBody={false} ariaLabel="Rename file">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FileType2 size={14} />
      <span class="modal-title">Rename File</span>
    </ModalHeader>
  {/snippet}

  <div class="rf" onkeydown={onKey} role="presentation">
    <input
      class="rf-input"
      bind:this={nameEl}
      bind:value={name}
      spellcheck="false"
      autocomplete="off"
      aria-label="New name"
      data-modal-autofocus
    />

    <div class="rf-note">
      {#if problem}
        <span class="rf-bad">{problem}</span>
      {:else if checking}
        <span class="rf-checking"><Spinner size={11} /> Checking what refers to it…</span>
      {:else if implied && implied.files.length > 0}
        <!-- The whole reason the dialog waits: a Rust rename moves a `mod` declaration and every
             `use` path through it, and those are the files it will touch. -->
        <span class="rf-impl">
          {implied.edits} change{implied.edits === 1 ? '' : 's'} in
          {implied.files.length} file{implied.files.length === 1 ? '' : 's'} will be applied with it
        </span>
      {:else if trimmed !== currentName}
        <span class="rf-plain">Nothing else refers to this file by name.</span>
      {/if}
    </div>

    {#if implied && implied.files.length > 0}
      <ul class="rf-files">
        {#each implied.files as f (f)}
          <li><FileCode2 size={11} /> <span title={f}>{baseName(f)}</span></li>
        {/each}
      </ul>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        size="sm"
        disabled={!canRename}
        loading={busy}
        tooltip={{ content: 'Rename', shortcut: 'Enter' }}
        onclick={() => void rename()}
      >Rename</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .rf { display: flex; flex-direction: column; gap: 8px; padding: 10px; min-height: 0; }
  .rf-input {
    padding: 6px 10px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: calc(var(--font-size-md) * 1.05);
  }
  .rf-input:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-subtle); }
  /* One line, always present: a note that appears and disappears would make the dialog jump as
     you type. */
  .rf-note { min-height: 16px; font-size: 11px; display: flex; align-items: center; gap: 5px; }
  .rf-bad { color: var(--error); }
  .rf-checking, .rf-plain { color: var(--text-disabled); display: flex; align-items: center; gap: 5px; }
  .rf-impl { color: var(--info); }
  .rf-files {
    list-style: none; margin: 0; padding: 4px 6px;
    max-height: 120px; overflow-y: auto;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-size: 11px;
    color: var(--text-secondary);
  }
  .rf-files li { display: flex; align-items: center; gap: 5px; padding: 1px 0; }
</style>
