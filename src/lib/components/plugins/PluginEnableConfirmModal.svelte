<script lang="ts">
  import { Power, Package, AlertTriangle } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import type { EnableBlocker } from '$lib/ipc/plugin';

  /**
   * Two-mode modal:
   *   • `blockers` non-empty → "can't enable" — explains which required
   *     dependencies are missing / unloadable. Single OK button.
   *   • `blockers` empty + `cascade.length > 1` → confirm cascade — lists
   *     the required deps that will be enabled alongside the target.
   *
   * The caller is expected to not open this modal when the cascade is just
   * `[pluginName]` (no extra work to disclose); in that case the click can
   * trigger `enablePlugin` directly.
   */
  let {
    pluginName,
    cascade,
    blockers,
    onConfirm,
    onCancel,
  }: {
    pluginName: string;
    /** Ordered list from `plugin_enable_preview` — deps first, target last. */
    cascade:    string[];
    blockers:   EnableBlocker[];
    onConfirm:  () => void;
    onCancel:   () => void;
  } = $props();

  const blocked = $derived(blockers.length > 0);
  const deps    = $derived(cascade.filter(n => n !== pluginName));
</script>

<Modal onClose={onCancel} width="520px" height="auto"
       ariaLabel={blocked ? 'Cannot enable plugin' : 'Enable plugin?'}>
  {#snippet header()}
    <ModalHeader onClose={onCancel}>
      {#if blocked}
        <span class="ec-icon ec-icon-error"><AlertTriangle size={14} /></span>
        <span class="modal-title">Cannot enable {pluginName}</span>
      {:else}
        <span class="ec-icon"><Power size={14} /></span>
        <span class="modal-title">Enable plugin?</span>
      {/if}
    </ModalHeader>
  {/snippet}

  <div class="ec-body">
    {#if blocked}
      <p>
        <strong>{pluginName}</strong> requires
        {blockers.length === 1 ? '1 dependency' : `${blockers.length} dependencies`}
        that {blockers.length === 1 ? "isn't" : "aren't"} available. Resolve the
        items below — install the missing plugins via the marketplace, or
        update them to a compatible version — then try again.
      </p>

      <ul class="ec-list ec-list-blockers">
        {#each blockers as b (b.name)}
          <li>
            <Package size={10} />
            <span class="ec-blocker-name">{b.name}</span>
            {#if b.version_req}<span class="ec-blocker-req">requires {b.version_req}</span>{/if}
            <span class="ec-blocker-reason">— {b.reason}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p>
        Enabling <strong>{pluginName}</strong> requires turning on
        {deps.length === 1 ? '1 other plugin' : `${deps.length} other plugins`}
        first:
      </p>

      <ul class="ec-list">
        {#each deps as d (d)}
          <li><Package size={10} /> {d}</li>
        {/each}
      </ul>

      <p class="ec-hint">
        Required dependencies are enabled in order before {pluginName} comes online.
      </p>
    {/if}
  </div>

  {#snippet footer()}
    {#if blocked}
      <Button variant="primary" onclick={onCancel}>OK</Button>
    {:else}
      <Button variant="secondary" onclick={onCancel}>Cancel</Button>
      <Button variant="primary" onclick={onConfirm}>
        Enable {cascade.length} {cascade.length === 1 ? 'plugin' : 'plugins'}
      </Button>
    {/if}
  {/snippet}
</Modal>

<style>
  .ec-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
  }
  .ec-icon-error { color: var(--error); }

  .ec-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .ec-body p {
    margin: 0;
    font-size: var(--font-size-sm);
    line-height: 1.55;
    color: var(--text-secondary);
  }
  .ec-body strong { color: var(--text-primary); }

  .ec-list {
    margin: 0;
    padding: 6px 0 6px 4px;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 160px;
    overflow-y: auto;
    border-top: 1px dashed var(--border-subtle);
    border-bottom: 1px dashed var(--border-subtle);
  }
  .ec-list li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    flex-wrap: wrap;
  }

  .ec-list-blockers li { gap: 5px; }
  .ec-blocker-name { font-weight: 500; }
  .ec-blocker-req {
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: 999px;
    font-family: var(--font-code);
  }
  .ec-blocker-reason { color: var(--error); font-size: 11px; }

  .ec-hint {
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
  }
</style>
