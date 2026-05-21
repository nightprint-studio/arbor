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

  const title = $derived(scope === 'remote' ? 'Elimina tag su origin' : 'Elimina tag locale');
  const buttonLabel = $derived(scope === 'remote' ? 'Elimina locale + origin' : 'Elimina localmente');
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
        Stai per eliminare il tag <strong>{tagName}</strong> sia in locale
        <em>sia su</em> <code>origin</code>.
      </p>
      <p class="irreversible">
        L'azione è <strong>irreversibile</strong>: il tag verrà rimosso dal remote
        e non potrà essere recuperato a meno che qualcun altro non lo abbia ancora
        in locale. Eventuali release o build agganciati a questo tag potrebbero
        diventare irraggiungibili.
      </p>
    {:else}
      <p class="message">
        Stai per eliminare il tag <strong>{tagName}</strong> dal repository locale.
      </p>
      <p class="irreversible">
        Il tag continuerà a esistere sul remote (se vi è stato pushato) e tornerà
        al prossimo <code>fetch --tags</code>. Per rimuoverlo definitivamente usa
        <em>Elimina locale + origin</em>.
      </p>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onCancel}>Annulla</Button>
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
    font-size: 11px;
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
  }

  .message :global(code) {
    font-family: var(--font-code);
    font-size: 11px;
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
  .irreversible :global(code) { font-family: var(--font-code); font-size: 10px; }
</style>
