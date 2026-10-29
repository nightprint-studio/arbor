<script lang="ts">
  /**
   * An answer opened as a Markdown document: read it rendered, edit it with the live-preview editor, copy
   * it, or save it to a file.
   *
   * It is a working copy — what the assistant said is not changed, and nothing here is sent back to it.
   * The point is the moment an answer turns out to be something worth keeping: a design note, a
   * checklist, an explanation for a README. Editing happens in the same editor Bennu's `.md` tabs use,
   * so it behaves the way a Markdown file already does.
   */
  import { Copy, Eye, FilePen, Pencil, Save } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import MarkdownEditor from '$lib/components/shared/ui/MarkdownEditor.svelte';
  import MarkdownView from '$lib/components/shared/ui/MarkdownView.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { fsWriteTextFile } from '$lib/ipc/fs';
  import { copyToClipboard } from '$lib/utils/clipboard';

  let { text, onClose }: { text: string; onClose: () => void } = $props();

  // A working copy: starts as the answer and is the user's from then on.
  let draft = $state(text);
  let editing = $state(false);
  let picking = $state(false);

  async function save(path: string) {
    picking = false;
    try {
      await fsWriteTextFile(path, draft);
      toastStore.show(`Saved to ${path}`, 'success');
    } catch (e) {
      toastStore.show(`Could not save: ${e}`, 'error');
    }
  }
</script>

<Modal {onClose} width="760px" height="640px" padBody={false} bodyBorder ariaLabel="Answer as a Markdown document">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FilePen size={14} />
      <span class="title">Answer as a document</span>
    </ModalHeader>
  {/snippet}

  <div class="body">
    {#if editing}
      <MarkdownEditor docKey="assistant-answer" text={draft} onChange={(t) => (draft = t)} autofocus />
    {:else}
      <div class="reader"><MarkdownView text={draft} /></div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <Button variant="ghost" size="sm" onclick={() => (editing = !editing)}>
        {#if editing}<Eye size={12} /> Read{:else}<Pencil size={12} /> Edit{/if}
      </Button>
      <div class="actions">
        <Button variant="ghost" size="sm" onclick={() => copyToClipboard(draft, { successToast: 'Copied' })}><Copy size={12} /> Copy</Button>
        <Button variant="secondary" size="sm" onclick={() => (picking = true)}><Save size={12} /> Save as…</Button>
        <Button variant="secondary" size="sm" onclick={onClose}>Close</Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

{#if picking}
  <FileExplorerModal
    mode="save"
    title="Save the answer"
    extensions={['md']}
    initialFilename="answer.md"
    onConfirm={save}
    onCancel={() => (picking = false)}
  />
{/if}

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .body { height: 100%; min-height: 0; display: flex; flex-direction: column; }
  .body :global(.cm-editor) { height: 100%; background: var(--bg-base); }
  .reader { flex: 1; overflow-y: auto; padding: 16px 20px; }
  .actions { display: flex; gap: 8px; }
</style>
