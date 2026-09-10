<script lang="ts">
  /**
   * BennuPasteModal — where a copied file lands, and what it will be called.
   *
   * ## Why pasting a class asks a question
   *
   * Because "paste" on a class almost always means *make me another one of these*, and the second
   * one needs a different name. Pasting into the same package with the same name is not even
   * possible, so a dialog that only reported the collision would be a dialog you dismiss and
   * reopen. It opens with the name already filled and the caret before the extension, so the common
   * case is: type a suffix, Enter.
   *
   * ## What it tells you that the tree cannot
   *
   * The **package** the copy will declare — which is the thing that silently goes wrong when a
   * class is duplicated by hand, and the reason this is not `cp`. When the copy leaves its old
   * package, the line under the field says the imports will be fixed too; that is a promise worth
   * making explicitly, because the alternative reading of a successful paste is "it copied the
   * bytes and I now have errors to chase".
   *
   * Keyboard-first: the field auto-focuses, Enter pastes, Esc cancels (Modal owns it).
   */
  import { ClipboardPaste } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { pasteFiles, pastePlan, type PasteItem } from '$lib/ipc/bennu/scaffold';

  let {
    /** The copied files, absolute. */
    sources,
    /** The directory they are being pasted into, absolute. */
    targetDir,
    onClose,
  }: { sources: string[]; targetDir: string; onClose: () => void } = $props();

  let items = $state<PasteItem[]>([]);
  let loading = $state(true);
  let name = $state('');
  let busy = $state(false);

  /** The single-file case — the only one where a name means anything. */
  const single = $derived(items.length === 1 ? items[0] : null);
  const refused = $derived(items.find((i) => i.refused)?.refused ?? null);

  $effect(() => {
    const root = projectStore.project?.root;
    if (!root) return;
    let cancelled = false;
    void pastePlan(root, sources, targetDir)
      .then((plan) => {
        if (cancelled) return;
        items = plan.items;
        const one = plan.items.length === 1 ? plan.items[0] : null;
        // A collision needs a different name to be typed; without one the file keeps its own,
        // which is right for a paste into another package.
        name = one ? (one.collides ? suggest(one.name) : one.name) : '';
        loading = false;
      })
      .catch((e) => {
        if (cancelled) return;
        toastStore.show(`Could not read the copy: ${e}`, 'error');
        loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  /** `Order.java` → `OrderCopy.java`. The suffix rather than a number because a duplicate is
   *  almost always renamed properly a moment later, and `OrderCopy` reads as a thing to rename
   *  while `Order2` reads as a thing somebody meant. */
  function suggest(fileName: string): string {
    const dot = fileName.lastIndexOf('.');
    return dot > 0 ? `${fileName.slice(0, dot)}Copy${fileName.slice(dot)}` : `${fileName}Copy`;
  }

  const changingPackage = $derived(
    !!single?.java && !!single.package && single.package !== packageOfSource(single.source),
  );

  /** The package the source file sits in, read off its path the way the backend reads it. */
  function packageOfSource(path: string): string {
    const m = path.replace(/\\/g, '/').match(/\/src\/(?:main|test)\/(?:java|kotlin)\/(.+)\/[^/]+$/);
    return m ? m[1].replace(/\//g, '.') : '';
  }

  const problem = $derived.by(() => {
    if (refused) return refused;
    if (!single) return null;
    const typed = name.trim();
    if (!typed) return 'Type a name';
    if (/[\\/:*?"<>|]/.test(typed)) return 'A file name can’t contain \\ / : * ? " < > |';
    if (typed !== single.name && single.java && !/^[A-Za-z][A-Za-z0-9_$]*\.java$/.test(typed))
      return 'A Java file is named after the type it declares';
    return null;
  });

  const canPaste = $derived(!loading && items.length > 0 && !problem && !busy);

  async function paste() {
    if (!canPaste) return;
    const root = projectStore.project?.root;
    if (!root) return;
    busy = true;
    try {
      const res = await pasteFiles(root, sources, targetDir, single ? name.trim() : '');
      await projectStore.refreshTree();
      if (res.written[0]) {
        bennuUiStore.focusInTree(res.written[0]);
        await projectStore.openFile(res.written[0]);
      }
      toastStore.show(
        res.imports_added.length
          ? `Pasted ${res.written.length === 1 ? name.trim() : `${res.written.length} files`} · ${res.imports_added.length} import${res.imports_added.length === 1 ? '' : 's'} added`
          : `Pasted ${res.written.length === 1 ? name.trim() : `${res.written.length} files`}`,
        'success',
      );
      onClose();
    } catch (e) {
      toastStore.show(e instanceof Error ? e.message : String(e), 'error');
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      void paste();
    }
  }

  let nameEl = $state<HTMLInputElement | undefined>();
  // Focused with the type name selected and the extension left out of the selection: renaming is
  // the point, and `.java` is never the part you are changing.
  $effect(() => {
    const el = nameEl;
    if (!el || loading || !single) return;
    el.focus();
    const dot = el.value.lastIndexOf('.');
    el.setSelectionRange(0, dot > 0 ? dot : el.value.length);
  });
</script>

<Modal {onClose} width="500px" height="auto" padBody={false} ariaLabel="Paste">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ClipboardPaste size={14} />
      <span class="modal-title">{single?.java ? 'Copy Class' : 'Paste'}</span>
    </ModalHeader>
  {/snippet}

  <div class="pm" onkeydown={onKey} role="presentation">
    {#if loading}
      <div class="pm-loading">Reading…</div>
    {:else if refused}
      <Alert variant="warning" compact title="Nothing to paste">{refused}</Alert>
    {:else if single}
      <input
        class="pm-input"
        bind:this={nameEl}
        bind:value={name}
        spellcheck="false"
        autocomplete="off"
        aria-label="Name"
        data-modal-autofocus
      />
      <div class="pm-note">
        {#if problem}
          <span class="pm-bad">{problem}</span>
        {:else if single.java && single.package}
          <span class="pm-preview">
            <code>package {single.package};</code>
            {#if changingPackage}
              <span class="pm-path">· imports fixed for what it can no longer see</span>
            {/if}
          </span>
        {:else}
          <span class="pm-preview"><code>{projectStore.relativePath(targetDir)}/{name}</code></span>
        {/if}
      </div>
      {#if single.collides}
        <Alert variant="warning" compact text="A file called {single.name} is already there — this one needs a different name." />
      {/if}
    {:else}
      <div class="pm-many">
        {items.length} files into <code>{projectStore.relativePath(targetDir)}</code>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        size="sm"
        disabled={!canPaste}
        loading={busy}
        tooltip={{ content: 'Paste', shortcut: 'Enter' }}
        onclick={() => void paste()}
      >Paste</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .pm { display: flex; flex-direction: column; gap: 8px; padding: 10px; min-height: 0; }
  .pm-input {
    padding: 6px 10px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: calc(var(--font-size-md) * 1.05);
  }
  .pm-input:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-subtle); }
  .pm-loading, .pm-many { color: var(--text-disabled); font-size: 12px; padding: 4px 0; }
  .pm-many code { font-family: var(--font-code); color: var(--text-secondary); }
  .pm-note { min-height: 16px; font-size: 11px; display: flex; align-items: center; gap: 5px; }
  .pm-bad { color: var(--error); }
  .pm-preview { color: var(--info); display: flex; align-items: center; gap: 5px; }
  .pm-preview code { font-family: var(--font-code); color: var(--text-secondary); }
  .pm-path { color: var(--text-disabled); }
</style>
