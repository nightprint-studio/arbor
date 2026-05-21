<script lang="ts">
  import { AlertTriangle, Folder, Database, FileText, Package } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  let {
    pluginName,
    dependents,
    onConfirm,
    onCancel,
    busy = false,
  }: {
    pluginName: string;
    dependents: string[];
    onConfirm:  () => void;
    onCancel:   () => void;
    busy?:      boolean;
  } = $props();
</script>

<Modal onClose={onCancel} ariaLabel="Uninstall plugin?">
  {#snippet header()}
    <ModalHeader onClose={onCancel}>
      <span class="uc-icon"><AlertTriangle size={14} /></span>
      <span class="modal-title">Uninstall plugin?</span>
    </ModalHeader>
  {/snippet}

  <div class="uc-body">
    <p>
      You are about to permanently remove <strong>{pluginName}</strong>.
      This will delete:
    </p>

    <ul class="uc-list">
      <li><Folder size={11} /> the plugin folder under <code>plugins/{pluginName}/</code></li>
      <li><Database size={11} /> all global settings (<code>plugin_data/{pluginName}/</code>)</li>
      <li><FileText size={11} /> per-repo settings in every known repo (<code>.arbor/plugins/{pluginName}/</code>)</li>
      <li>the persisted enable/disable state for this plugin</li>
    </ul>

    {#if dependents.length > 0}
      <p class="uc-warning">
        <AlertTriangle size={11} />
        This plugin is required by {dependents.length === 1 ? '1 other enabled plugin' : `${dependents.length} other enabled plugins`}:
      </p>
      <ul class="uc-deps">
        {#each dependents as dep (dep)}
          <li><Package size={10} /> {dep}</li>
        {/each}
      </ul>
      <p class="uc-hint">They will likely break until you reinstall or replace this plugin.</p>
    {:else}
      <p class="uc-hint">This action cannot be undone.</p>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onCancel} disabled={busy}>Cancel</Button>
    <Button variant="danger" onclick={onConfirm} disabled={busy}>
      {busy ? 'Uninstalling…' : 'Uninstall'}
    </Button>
  {/snippet}
</Modal>

<style>
  .uc-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--error);
  }

  .uc-body { display: flex; flex-direction: column; gap: 10px; }
  .uc-body p {
    margin: 0;
    font-size: var(--font-size-sm);
    line-height: 1.55;
    color: var(--text-secondary);
  }
  .uc-body strong { color: var(--text-primary); }
  .uc-body code {
    background: var(--bg-overlay);
    padding: 0 4px;
    border-radius: var(--radius-sm);
    color: var(--accent);
    font-family: var(--font-code);
    font-size: 11px;
  }

  .uc-list, .uc-deps {
    margin: 0;
    padding: 4px 0 4px 6px;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .uc-list li, .uc-deps li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 4px;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
  }
  .uc-list { border-top: 1px dashed var(--border-subtle); border-bottom: 1px dashed var(--border-subtle); padding: 6px 0 6px 6px; }

  .uc-warning {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--warning);
    font-size: var(--font-size-sm);
  }

  .uc-hint {
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
  }
</style>
