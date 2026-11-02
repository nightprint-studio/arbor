<script lang="ts">
  import { AlertTriangle } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  type Scope = 'local' | 'remote';

  let {
    tagName,
    scope,
    onConfirm,
    onCancel,
  }: {
    tagName: string;
    /** 'local'  → wipes only the local ref.
     *  'remote' → also pushes a delete refspec to origin (more destructive). */
    scope: Scope;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  const title = $derived(scope === 'remote' ? 'Delete tag on origin' : 'Delete local tag');
  const buttonLabel = $derived(scope === 'remote' ? 'Delete local + origin' : 'Delete locally');
</script>

<Modal onClose={onCancel} ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {title} onClose={onCancel} />
  {/snippet}
  <div class="body">
    <div class="warning-icon" class:remote={scope === 'remote'}>
      <AlertTriangle size={32} />
    </div>

    {#if scope === 'remote'}
      <p class="message">
        You are about to delete the tag <strong>{tagName}</strong> both locally
        <em>and on</em> <code>origin</code>.
      </p>
      <p class="irreversible">
        This is <strong>irreversible</strong>: the tag is removed from the remote and cannot
        be recovered unless someone else still has it locally. Releases or builds
        pinned to this tag may become unreachable.
      </p>
    {:else}
      <p class="message">
        You are about to delete the tag <strong>{tagName}</strong> from the local repository.
      </p>
      <p class="irreversible">
        The tag still exists on the remote (if it was pushed) and comes back on the next
        <code>fetch --tags</code>. To remove it for good use
        <em>Delete local + origin</em>.
      </p>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onCancel}>Cancel</Button>
    {#if scope === 'remote'}
      <Button variant="primary" color="var(--error)" onclick={onConfirm}>{buttonLabel}</Button>
    {:else}
      <Button variant="danger" onclick={onConfirm}>{buttonLabel}</Button>
    {/if}
  {/snippet}
</Modal>

<style>
  .body {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    text-align: center;
    padding: 8px 4px 4px;
  }

  .warning-icon {
    color: var(--warning);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .warning-icon.remote { color: var(--error); }

  .message {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    margin: 0;
    line-height: 1.5;
  }

  .message strong {
    font-weight: 600;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
  }

  .message :global(code) {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }

  .irreversible {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    margin: 0;
    line-height: 1.5;
    max-width: 360px;
  }
  .irreversible :global(strong) {
    color: var(--error);
    font-weight: 600;
  }
  .irreversible :global(em),
  .irreversible :global(code) {
    color: var(--text-secondary);
  }
  .irreversible :global(code) { font-family: var(--font-code); font-size: var(--font-size-2xs); }
</style>
