<script lang="ts">
  import { AlertTriangle, Package } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  /**
   * Cascade-disable confirmation. `pluginName` is the user-initiated target,
   * `cascade` is the full leaves-first list returned by `plugin_disable_preview`
   * (which already includes `pluginName` as its last element). We render the
   * dependents on their own so the user sees what extra plugins will be
   * disabled alongside the explicit click.
   */
  let {
    pluginName,
    cascade,
    onConfirm,
    onCancel,
  }: {
    pluginName: string;
    cascade:    string[];
    onConfirm:  () => void;
    onCancel:   () => void;
  } = $props();

  const dependents = $derived(cascade.filter(n => n !== pluginName));
</script>

<Modal onClose={onCancel} width="520px" height="auto" ariaLabel="Disable plugin?">
  {#snippet header()}
    <ModalHeader onClose={onCancel}>
      <span class="dc-icon"><AlertTriangle size={14} /></span>
      <span class="modal-title">Disable plugin?</span>
    </ModalHeader>
  {/snippet}

  <div class="dc-body">
    <p>
      <strong>{pluginName}</strong> is required by
      {dependents.length === 1 ? '1 other enabled plugin' : `${dependents.length} other enabled plugins`}.
      They will be disabled along with it so they don't keep running against a
      missing dependency.
    </p>

    <ul class="dc-list">
      {#each dependents as dep (dep)}
        <li><Package size={10} /> {dep}</li>
      {/each}
    </ul>

    <p class="dc-hint">
      Re-enabling <strong>{pluginName}</strong> later won't automatically
      re-enable these — flip them back on individually when you're ready.
    </p>
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onCancel}>Cancel</Button>
    <Button variant="danger" onclick={onConfirm}>
      Disable {cascade.length} {cascade.length === 1 ? 'plugin' : 'plugins'}
    </Button>
  {/snippet}
</Modal>

<style>
  .dc-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--error);
  }

  .dc-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .dc-body p {
    margin: 0;
    font-size: var(--font-size-sm);
    line-height: 1.55;
    color: var(--text-secondary);
  }
  .dc-body strong { color: var(--text-primary); }

  .dc-list {
    margin: 0;
    padding: 6px 0 6px 4px;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 140px;
    overflow-y: auto;
    border-top: 1px dashed var(--border-subtle);
    border-bottom: 1px dashed var(--border-subtle);
  }
  .dc-list li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
  }

  .dc-hint {
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
  }
</style>
