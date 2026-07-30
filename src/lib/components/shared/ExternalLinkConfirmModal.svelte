<script lang="ts">
  /**
   * ExternalLinkConfirmModal — Chrome-style "open external link?" prompt.
   *
   * Shown when a generic external link (custom scheme, or http/https when web
   * links are enabled) is entered in the File Explorer address bar and its
   * scheme hasn't been remembered yet. The "Always allow <scheme> links"
   * checkbox lets the user skip the prompt for that scheme from then on.
   */
  import { ExternalLink } from 'lucide-svelte';
  import Modal       from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ModalFooter from './ModalFooter.svelte';
  import Button      from './ui/Button.svelte';

  let { url, scheme, onConfirm, onCancel }: {
    url:       string;
    scheme:    string;
    /** `remember` is true when the user ticked "Always allow <scheme> links". */
    onConfirm: (remember: boolean) => void;
    onCancel:  () => void;
  } = $props();

  let remember = $state(false);

  const isWeb = $derived(scheme === 'http' || scheme === 'https');
  const target = $derived(isWeb ? 'your default browser' : `the app associated with ${scheme}:// links`);

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); onConfirm(remember); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal onClose={onCancel} width="480px" ariaLabel="Open external link" zIndex="var(--z-menu)">
  {#snippet header()}
    <ModalHeader onClose={onCancel}>
      <ExternalLink size={14} class="ext-hdr-icon" />
      <span class="ext-title">Open external link?</span>
    </ModalHeader>
  {/snippet}

  <div class="ext-body">
    <p class="ext-lead">This link will open in {target}, outside Arbor.</p>
    <div class="ext-url" title={url}>{url}</div>
    <label class="ext-remember">
      <input type="checkbox" bind:checked={remember} />
      <span>Always allow <strong>{scheme}</strong> links from the address bar</span>
    </label>
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" size="sm" onclick={onCancel}>Cancel</Button>
      <Button variant="primary" size="sm" onclick={() => onConfirm(remember)}>Open link</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  :global(.ext-hdr-icon) { color: var(--accent); flex-shrink: 0; }
  .ext-title { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .ext-body { display: flex; flex-direction: column; gap: 12px; }
  .ext-lead { margin: 0; font-size: var(--font-size-sm); color: var(--text-secondary); line-height: 1.5; }
  .ext-url {
    font-family: var(--font-code); font-size: var(--font-size-sm); color: var(--text-primary);
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 8px 10px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ext-remember {
    display: flex; align-items: center; gap: 8px; cursor: pointer;
    font-size: var(--font-size-sm); color: var(--text-secondary); user-select: none;
  }
  .ext-remember input { accent-color: var(--accent); width: 14px; height: 14px; cursor: pointer; flex-shrink: 0; }
  .ext-remember strong { color: var(--text-primary); font-weight: 600; }
</style>
