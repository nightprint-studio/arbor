<script lang="ts">
  /**
   * The centre column once a vault is open: the tabs, the note, and the two or
   * three facts about it worth showing above the text.
   *
   * It owns no editing of its own — `NoteEditor` mounts the shared markdown
   * editor and `garrulusNotesStore` owns the bytes. What lives here is the part
   * that is neither: which note is in front, when a save happens, and what
   * closing a note with unsaved changes should ask.
   *
   * **Saving.** `Ctrl+S`, and losing focus. The keystroke is the one the user
   * reaches for and the blur is the one that catches them when they do not; both
   * are no-ops on a note that has not changed, so neither writes for the sake of
   * writing. Nothing saves on mount, on a timer, or on switching tabs — switching
   * away from a dirty note is a blur, and the blur is what saves it.
   *
   * **The keyboard listener.** The shell owns the window's key handler and this
   * component is not allowed to edit it, so the note-scoped chords are bound here
   * behind the same focus gate the shell uses. They belong in
   * `GarrulusShell.onKeyDown` beside `Ctrl+B` and the section digits — see the
   * note in the task summary — and moving them there is a delete, not a rewrite.
   */
  import { Eye, Pencil, Save } from 'lucide-svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import CloseNoteModal from './CloseNoteModal.svelte';
  import NoteEditor from './NoteEditor.svelte';
  import NoteTabStrip from './NoteTabStrip.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { garrulusNotesStore } from '$lib/stores/garrulus/notes.svelte';
  import { garrulusUiStore } from '$lib/stores/garrulus/ui.svelte';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';

  /** Reading mode — the whole note renders, because nothing holds a caret.
   *  Session-shaped and shared by every tab, like the eye button in the shared
   *  markdown modal. */
  let reading = $state(false);
  /** The tab a close was asked for while it had unsaved bytes. */
  let closing = $state<string | null>(null);
  let editorRef = $state<{ focus: () => void } | null>(null);

  const open = $derived(garrulusNotesStore.open);
  const note = $derived(garrulusNotesStore.active);
  const dirty = $derived(note != null && note.text !== note.saved);
  /** The tab the close dialog is about — read off the open notes rather than the
   *  catalogue, which a note created outside this window may not be in yet. */
  const closingNote = $derived(open.find((n) => n.path === closing) ?? null);

  /** Absolute path of the note, which is what resolves `![](./img.png)`. Built
   *  with the root's own separator so a Windows vault stays addressable. */
  const docPath = $derived.by(() => {
    const root = garrulusVaultStore.root;
    if (!root || !note) return null;
    const sep = root.includes('\\') ? '\\' : '/';
    const rel = sep === '\\' ? note.path.replace(/\//g, '\\') : note.path;
    return `${root.replace(/[\\/]+$/, '')}${sep}${rel}`;
  });

  /** Word count, without building an array of every word: this runs on each
   *  keystroke of a document that can be tens of thousands of characters, and the
   *  garbage is the only part of it that would be expensive. */
  function countWords(text: string): number {
    let count = 0;
    let inWord = false;
    for (let i = 0; i < text.length; i += 1) {
      const c = text.charCodeAt(i);
      if (c === 32 || c === 9 || c === 10 || c === 13) inWord = false;
      else if (!inWord) {
        inWord = true;
        count += 1;
      }
    }
    return count;
  }

  const words = $derived(note ? countWords(note.text) : 0);

  function save() {
    void garrulusNotesStore.save();
  }

  /** Close a tab, asking first when it would drop an edit. */
  function requestClose(path: string) {
    if (!garrulusNotesStore.close(path)) closing = path;
  }

  async function saveAndClose() {
    const path = closing;
    closing = null;
    if (!path) return;
    await garrulusNotesStore.save(path);
    // Only if the write landed: a failed save that closed the tab anyway would
    // be the same lost edit by a longer route.
    if (!garrulusNotesStore.isDirty(path)) garrulusNotesStore.close(path);
  }

  function discardAndClose() {
    const path = closing;
    closing = null;
    if (path) garrulusNotesStore.close(path, true);
  }

  function onKeyDown(e: KeyboardEvent) {
    if (!surfaceStore.hasFocus('garrulus')) return;
    if (garrulusUiStore.anyModalOpen || closing) return;
    if (!(e.ctrlKey || e.metaKey)) return;
    const key = e.key.toLowerCase();

    if (key === 's' && !e.shiftKey) {
      // Ctrl+Shift+S is Sync now, and the shell owns it.
      e.preventDefault();
      save();
      return;
    }
    if (key === 'e' && !e.shiftKey) {
      e.preventDefault();
      reading = !reading;
      return;
    }
    if (key === 'w' && !e.shiftKey) {
      e.preventDefault();
      if (note) requestClose(note.path);
      return;
    }
    if (e.key === 'Tab' && open.length > 1) {
      e.preventDefault();
      garrulusNotesStore.cycle(e.shiftKey ? -1 : 1);
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="gnw">
  {#if open.length > 1}
    <NoteTabStrip
      {open}
      activePath={garrulusNotesStore.activePath}
      onSelect={(path) => {
        garrulusNotesStore.activate(path);
        editorRef?.focus();
      }}
      onClose={requestClose}
    />
  {/if}

  {#if !note}
    <StateBlock
      tone="neutral"
      label={garrulusNotesStore.notes.length === 0
        ? `${garrulusVaultStore.name ?? 'This vault'} has no notes yet.`
        : 'Pick a note in the sidebar — Ctrl+1 puts you there.'}
    />
  {:else if note.loading}
    <StateBlock tone="loading" label="Opening {note.title}…">
      {#snippet spinner()}<Spinner size={20} />{/snippet}
    </StateBlock>
  {:else if note.error}
    <StateBlock tone="error" label="Could not open {note.path} — {note.error}" />
  {:else}
    <div class="gnw-head">
      <span class="gnw-path" title={note.path}>{note.path}</span>
      <span class="gnw-spacer"></span>
      <span class="gnw-count">{words} {words === 1 ? 'word' : 'words'}</span>
      <span class="gnw-state">
        {#if note.saving}
          <Spinner size={11} /> Saving…
        {:else if dirty}
          <Save size={11} /> Unsaved — <Kbd keys={['Ctrl', 'S']} />
        {:else}
          Saved
        {/if}
      </span>
      <button
        type="button"
        class="gnw-btn"
        class:active={reading}
        aria-pressed={reading}
        onclick={() => (reading = !reading)}
        use:tooltip={{
          content: reading ? 'Back to editing' : 'Reading mode — the whole note renders',
          shortcut: 'Ctrl+E',
        }}
      >
        {#if reading}<Pencil size={13} />{:else}<Eye size={13} />{/if}
      </button>
    </div>

    <NoteEditor
      bind:this={editorRef}
      notePath={note.path}
      revision={note.revision}
      text={note.text}
      {docPath}
      readOnly={reading}
      onChange={(path, text) => garrulusNotesStore.setText(path, text)}
      onBlur={(path) => void garrulusNotesStore.save(path)}
    />
  {/if}
</div>

{#if closingNote}
  <CloseNoteModal
    title={closingNote.title}
    path={closingNote.path}
    busy={closingNote.saving}
    onSave={saveAndClose}
    onDiscard={discardAndClose}
    onCancel={() => (closing = null)}
  />
{/if}

<style>
  /* The state blocks are shared widgets with no class prop, and here they are
     the whole body of a flex column — `height: 100%` would overflow past the tab
     strip. One rule, rather than a variant on the widget for one caller. */
  .gnw :global(.state-block) {
    flex: 1;
    min-height: 0;
  }

  .gnw {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  /* The note's own bar: where it lives on the left, what state it is in on the
     right. The formatting toolbar the mockup shows sits here too, once the
     shared editor exposes the commands it would drive. */
  .gnw-head {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
    height: 28px;
    padding: 0 8px 0 12px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }

  .gnw-path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
  }

  .gnw-spacer { flex: 1; }

  .gnw-count { flex: none; color: var(--text-disabled); }

  .gnw-state {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
  }

  .gnw-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    flex: none;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .gnw-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .gnw-btn.active {
    background: var(--accent-subtle);
    border-color: color-mix(in srgb, var(--accent) 30%, transparent);
    color: var(--accent);
  }
</style>
