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
  import Button from '$lib/components/shared/ui/Button.svelte';
  import JavaKindIcon from './JavaKindIcon.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { newFile, type NewFileKind } from '$lib/ipc/bennu/scaffold';

  let {
    dir,
    initialKind = 'class',
    onClose,
  }: { dir: string; initialKind?: NewFileKind; onClose: () => void } = $props();

  /** The Java shapes, in IntelliJ's order — the frequent ones first, not alphabetical.
   *  `iconKind` is what {@link JavaKindIcon} draws; an exception is a class and wears a
   *  class's ring, because that is what it is. */
  const JAVA_KINDS: { value: NewFileKind; label: string; iconKind: string }[] = [
    { value: 'class',      label: 'Class',      iconKind: 'class' },
    { value: 'interface',  label: 'Interface',  iconKind: 'interface' },
    { value: 'record',     label: 'Record',     iconKind: 'record' },
    { value: 'enum',       label: 'Enum',       iconKind: 'enum' },
    { value: 'annotation', label: 'Annotation', iconKind: 'annotation' },
    { value: 'exception',  label: 'Exception',  iconKind: 'class' },
  ];

  // The caller decides which shape this opens in: "New › Java Class" lands on a type, "New ›
  // File" on a plain one. Read once, at mount — it is the starting point, not a binding.
  let kind = $state<NewFileKind>(initialKind);
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

  /** Whether this dialog is choosing a Java type at all. `false` for "New › File", where the
   *  only question is the name — showing a kind list there would be offering a choice that
   *  was already made by the menu leaf you clicked. */
  const isJava = $derived(kind !== 'file');

  /**
   * The whole dialog is driven from the name field: it holds focus the entire time, and the
   * kind list is steered from there with ↑/↓. This is IntelliJ's behaviour and it is the
   * reason the dialog needs no tabbing — you type the name, and if it isn't a class you
   * arrow down to what it is. Enter creates from wherever you are.
   */
  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); void create(); return; }
    if (!isJava || (e.key !== 'ArrowDown' && e.key !== 'ArrowUp')) return;
    e.preventDefault();
    const at = JAVA_KINDS.findIndex((k) => k.value === kind);
    const next = (at + (e.key === 'ArrowDown' ? 1 : -1) + JAVA_KINDS.length) % JAVA_KINDS.length;
    kind = JAVA_KINDS[next].value;
  }

  let nameEl = $state<HTMLInputElement | undefined>();
  $effect(() => { nameEl?.focus(); });
</script>

<!-- `height: auto` rather than a number: the content is a name field and a fixed list of six,
     so the dialog knows its own size better than any figure written here — and one written
     here is only ever right at one font scale. `max-height: 100%` on the modal box still caps
     it, and the list keeps its own `overflow-y` for the day a kind is added.

     The body carries no padding of its own (`padBody={false}`) — the field and the list bring
     theirs, and a second inset around them pushed the dialog wider than its content. -->
<Modal {onClose} width="420px" height="auto" padBody={false} ariaLabel="New file">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FilePlus2 size={14} />
      <span class="modal-title">{isJava ? 'New Java Class' : 'New File'}</span>
      <span class="nf-dir">in <code>{dirLabel}</code></span>
    </ModalHeader>
  {/snippet}

  <div class="nf" onkeydown={onKey} role="presentation">
    <!-- The name first, and unlabelled: the dialog's title already says what is being made,
         and a field with one obvious purpose does not need a word above it repeating the
         placeholder. The icon marks which kind the name will become, live. -->
    <div class="nf-name">
      {#if isJava}<JavaKindIcon kind={JAVA_KINDS.find((k) => k.value === kind)?.iconKind ?? 'class'} />{/if}
      <input
        class="nf-input"
        bind:this={nameEl}
        bind:value={name}
        placeholder={isJava ? 'Name' : 'File name (with extension)'}
        spellcheck="false"
        autocomplete="off"
        aria-label="Name"
      />
    </div>

    {#if isJava}
      <ul class="nf-kinds" role="listbox" aria-label="Kind" tabindex="-1">
        {#each JAVA_KINDS as k (k.value)}
          <li>
            <button
              type="button"
              class="nf-kind"
              class:nf-kind-on={kind === k.value}
              role="option"
              aria-selected={kind === k.value}
              onclick={() => { kind = k.value; nameEl?.focus(); }}
            >
              <JavaKindIcon kind={k.iconKind} />
              <span>{k.label}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
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
  /* The dialog's own inset — one, here, rather than the body card's plus this one. */
  .nf { display: flex; flex-direction: column; gap: 8px; padding: 10px; min-height: 0; }
  /* The name row: the live kind icon sits inside the field's box, so the mark and the name
     read as one thing rather than as a field with a decoration beside it. */
  .nf-name {
    display: flex; align-items: center; gap: 8px;
    padding: 0 10px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-size: calc(var(--font-size-md) * 1.15);
  }
  .nf-name:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-subtle); }

  /* No border of its own: with the body card's outline right around it, a second one read as
     a box inside a box. The list is the dialog's main surface — it can just be it. */
  .nf-kinds {
    list-style: none; margin: 0; padding: 0;
    overflow-y: auto; min-height: 0;
  }
  .nf-kind {
    display: flex; align-items: center; gap: 8px;
    width: 100%;
    padding: 5px 8px;
    background: none; border: none; cursor: pointer;
    color: var(--text-primary);
    font-size: var(--font-size-md);
    text-align: left;
    border-radius: var(--radius-sm);
    /* The icon is sized from the row's own font, like everywhere else in the app, so the
       Appearance setting moves the list as one. */
  }
  .nf-kind:hover { background: var(--bg-hover); }
  /* The selected row is filled, not merely tinted: this list is steered with the arrow keys
     while focus is in the NAME field, so the row has to say "this is what Enter makes"
     loudly enough to be read out of the corner of your eye. */
  .nf-kind-on, .nf-kind-on:hover {
    background: var(--accent);
    color: var(--bg-base);
    /* The icon comes along: its kind hue is picked to read on a panel, not on the accent —
       and `annotation`'s hue IS the accent, so on this row it would be invisible. */
    --jki-color: currentColor;
  }
  .nf-dir { font-size: var(--font-size-xs); color: var(--text-muted); }
  .nf-dir code { font-family: var(--font-code); color: var(--text-secondary); font-size: var(--font-size-xs); }
  /* The border and focus ring live on `.nf-name` (the row) now, so the input is bare. */
  .nf-input { flex: 1; min-width: 0; padding: 8px 0; background: none; border: none; color: var(--text-primary); font-family: var(--font-code); font-size: var(--font-size-md); outline: none; }
  .nf-input::placeholder { color: var(--text-disabled); }
</style>
