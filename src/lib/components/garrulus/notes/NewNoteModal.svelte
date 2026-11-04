<script lang="ts">
  /**
   * New note — a title, optionally a type, and where it will land.
   *
   * The common case is `Ctrl+N`, a title, `Enter`: the path follows the title until
   * it is edited by hand, so nothing after the title needs touching. A type moves
   * the path into its folder and fills the note from its template (the backend
   * renders both — this dialog never builds a filename of its own for a typed
   * note, so the naming pattern has one implementation).
   *
   * An untyped note starts empty: Obsidian names a note after its file, and a
   * heading written for the user would be the first thing they delete.
   */
  import { FilePlus } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { renderTemplate } from '$lib/ipc/garrulus';
  import { garrulusNotesStore } from '$lib/stores/garrulus/notes.svelte';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';
  import {
    normalizeNotePath,
    notePathProblem,
    stemFromTitle,
    withNoteExtension,
  } from './note-naming';

  interface Props {
    /** The type chosen before the dialog opened ("New Bug"), or `null`. */
    typeId: string | null;
    onClose: () => void;
  }

  let { typeId: initialType, onClose }: Props = $props();

  const NO_TYPE = '';
  const RENDER_DEBOUNCE_MS = 120;

  let title = $state('');
  let typeId = $state(initialType ?? NO_TYPE);
  let pathInput = $state('');
  /** Once the path has been edited by hand the title stops rewriting it. */
  let pathTouched = $state(false);
  let titleEl = $state<HTMLInputElement | undefined>();
  let busy = $state(false);
  let renderSeq = 0;

  const typeOptions = $derived([
    { value: NO_TYPE, label: 'No type' },
    ...garrulusVaultStore.types.map((t) => ({ value: t.id, label: t.name })),
  ]);
  const taken = $derived(
    new Set(garrulusNotesStore.notes.flatMap((n) => (n.path ? [n.path] : []))),
  );

  const hasTitle = $derived(stemFromTitle(title).length > 0);
  const problem = $derived(
    pathInput.trim() && hasTitle ? notePathProblem(pathInput, taken) : null,
  );
  const canSubmit = $derived(hasTitle && !!pathInput.trim() && !problem && !busy);

  // No `autofocus` attribute (a11y): the title is focused once it exists.
  $effect(() => {
    titleEl?.focus();
  });

  // Follow title and type into the proposed path. The newest answer wins, so a slow
  // render for "Cra" cannot overwrite the path for "Crash".
  $effect(() => {
    const t = title;
    const type = typeId;
    if (pathTouched) return;
    const mine = ++renderSeq;
    if (!type) {
      const stem = stemFromTitle(t);
      pathInput = stem ? `${stem}.md` : '';
      return;
    }
    const timer = setTimeout(async () => {
      try {
        const rendered = await renderTemplate(type, t);
        if (mine === renderSeq && !pathTouched) pathInput = rendered.path;
      } catch {
        if (mine === renderSeq && !pathTouched) pathInput = '';
      }
    }, RENDER_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });

  async function submit() {
    if (!canSubmit) return;
    busy = true;
    try {
      const path = withNoteExtension(normalizeNotePath(pathInput));
      // The body is rendered again at the moment of creation rather than kept from
      // the preview: the title may have changed since the debounce last fired.
      const text = typeId ? (await renderTemplate(typeId, title)).text : '';
      if (await garrulusNotesStore.create(path, text)) onClose();
    } catch (e) {
      // Only the render can land here — `create` reports its own failures. The
      // dialog stays up with what was typed.
      console.error(e);
    } finally {
      busy = false;
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    void submit();
  }
</script>

<Modal {onClose} width="560px" height="400px" ariaLabel="New note">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FilePlus size={14} />
      <span class="modal-title">New note</span>
    </ModalHeader>
  {/snippet}

  <div class="gnn-body">
    <FormField label="Title">
      <Input
        bind:value={title}
        bind:element={titleEl}
        placeholder="What is this note about"
        onkeydown={onKeyDown}
      />
    </FormField>

    <FormField label="Type" hint="Decides the folder, the file name and what the note starts with.">
      <Select bind:value={typeId} options={typeOptions} fill searchable={typeOptions.length > 8} />
    </FormField>

    <FormField label="Path" error={problem} hint="Relative to the vault. Folders are created as needed.">
      <Input
        bind:value={pathInput}
        placeholder="folder/note.md"
        oninput={() => (pathTouched = true)}
        onkeydown={onKeyDown}
      />
    </FormField>
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onClose}>Cancel</Button>
    <Button variant="primary" disabled={!canSubmit} onclick={submit}>Create</Button>
  {/snippet}
</Modal>

<style>
  .gnn-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
