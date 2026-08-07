<script lang="ts">
  /**
   * What a macro at the caret expands to.
   *
   * ## Why a modal and not a panel
   *
   * You go and look at an expansion, read it, and close it. It is not something you keep open beside
   * the code the way a problem list or a hierarchy is — and it is *wide*: generated code is long
   * lines of derived impls, which a 280px side panel would wrap into porridge.
   *
   * ## What it can and cannot do — stated, not implied
   *
   * The server hands back the expansion as **text**, not as a document it knows. So:
   *
   *   * there is no go-to, no hover and no completion inside it, and it is read-only. It is coloured
   *     by the Rust mode alone (`rustTextLanguage`) — attaching the server's intelligence to a buffer
   *     it has never heard of would be inventing answers;
   *   * a macro *inside* the expansion cannot be expanded from here. To go a level deeper you point
   *     at it in the real file. The note in the footer says so, because discovering it by clicking
   *     and getting nothing teaches that the feature is broken;
   *   * the expansion is **recursive** — rust-analyzer's one method expands all the way down, and
   *     there is no single-step form of it in the protocol. Labelled as such rather than offering a
   *     "step" control that would do the same thing.
   *
   * Re-expand is for the other half of that: you moved the caret in the file behind the modal (or
   * edited it), and want the expansion of what is there now.
   */
  import { FileCode2, RotateCw, Copy } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { rustTextLanguage } from './languages';

  let {
    /** The macro's name, from the server. */
    name,
    /** The expanded source. */
    expansion,
    /** Ask again at the caret as it is now. Omit to hide the control. */
    onReexpand,
    onClose,
  }: {
    name: string;
    expansion: string;
    onReexpand?: () => Promise<void> | void;
    onClose: () => void;
  } = $props();

  let busy = $state(false);

  async function reexpand() {
    if (!onReexpand || busy) return;
    busy = true;
    try {
      await onReexpand();
    } finally {
      busy = false;
    }
  }

  function copy() {
    // Best-effort — clipboard can be denied (permission / focus).
    void navigator.clipboard?.writeText(expansion)
      .then(() => toastStore.show('Expansion copied', 'success'))
      .catch(() => toastStore.show('Could not reach the clipboard', 'warning'));
  }

  const lines = $derived(expansion ? expansion.split('\n').length : 0);
</script>

<Modal {onClose} width="760px" height="560px" padBody={false} ariaLabel="Macro expansion">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FileCode2 size={14} />
      <span class="modal-title">Expand macro</span>
      <span class="mx-name">{name}</span>
      <span class="mx-kind">recursive</span>
    </ModalHeader>
  {/snippet}

  <div class="mx-body">
    {#if busy}
      <div class="mx-state"><Spinner size={13} /> Expanding…</div>
    {:else if !expansion.trim()}
      <EmptyState message="The server expanded this macro to nothing." />
    {:else}
      <!-- The real editor, read-only: the same colours, the same font and the same line numbers as
           the buffer behind the modal. See `CodePreview`'s note on why a lookalike drifts. -->
      <CodeEditor
        value={expansion}
        language={rustTextLanguage}
        readOnly
        lineNumbers
        wrap={false}
      />
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter>
      <span class="mx-note">
        Text, not a file — no go-to inside it. Expand a nested macro from the source instead.
      </span>
      <span class="mx-lines">{lines} line{lines === 1 ? '' : 's'}</span>
      <span class="mx-spacer"></span>
      {#if onReexpand}
        <Button variant="ghost" disabled={busy} onclick={() => void reexpand()}>
          {#snippet iconStart()}<RotateCw size={13} />{/snippet}
          Re-expand at caret
        </Button>
      {/if}
      <Button variant="ghost" onclick={copy}>
        {#snippet iconStart()}<Copy size={13} />{/snippet}
        Copy
      </Button>
      <Button variant="primary" onclick={onClose}>Close</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  /* The macro's own name, in the header beside the title — the answer to "which one did I ask
     about", which matters once you have re-expanded twice. */
  .mx-name {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--text-secondary);
  }
  .mx-kind {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-disabled);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 0 4px;
  }
  .mx-body {
    display: flex; flex-direction: column;
    height: 100%; min-height: 0;
    overflow: hidden;
  }
  .mx-state {
    display: flex; align-items: center; gap: 6px;
    padding: 10px;
    font-size: 11.5px;
    color: var(--text-muted);
  }
  .mx-note {
    font-size: 10.5px;
    color: var(--text-disabled);
  }
  .mx-lines {
    font-size: 10.5px;
    color: var(--text-muted);
  }
  .mx-spacer { flex: 1; }
</style>
