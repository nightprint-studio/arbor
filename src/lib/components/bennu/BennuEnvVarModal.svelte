<script lang="ts">
  /**
   * BennuEnvVarModal — a configuration key, shown as the environment variable that overrides it.
   *
   * Opened from the right-click menu on a Spring property file. It **never writes**: the whole
   * reason to ask is that the value is about to live somewhere else — a deployment descriptor, a
   * compose file, a CI secret — so the answer is something to copy, not an edit.
   *
   * Four forms rather than just the name, because the name alone still leaves the quoting to be
   * got right by hand and the four places this gets pasted quote differently. The rendering is
   * the backend's ({@link springEnvVar}); this file only lays it out.
   */
  import { Variable } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import type { EnvVarView } from '$lib/ipc/bennu/ext';

  let { view, onClose }: { view: EnvVarView; onClose: () => void } = $props();

  // The bare name has its own place at the top; the rest are the paste-ready lines. Matched by
  // label rather than by position, so the backend can add a form without moving this one.
  const lines = $derived(view.forms.filter(([label]) => label !== 'Name'));
</script>

<Modal {onClose} width="560px" height="420px" padBody={false} ariaLabel="Environment override">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Variable size={14} />
      <span class="modal-title">Environment override</span>
      <span class="hdr-key">{view.key}</span>
    </ModalHeader>
  {/snippet}

  <div class="env-body">
    <p class="env-lede">
      Spring reads this property from the variable below when it is set in the environment.
      Nothing here is written to the file.
    </p>

    <div class="env-name">
      <span class="env-name-text">{view.name}</span>
      <CopyButton value={view.name} variant="icon" title="Copy the variable name" toastSuccess="Name copied" />
    </div>

    <ul class="env-forms">
      {#each lines as [label, text] (label)}
        <li class="env-form">
          <span class="env-label">{label}</span>
          <code class="env-text">{text}</code>
          <CopyButton value={text} variant="icon" title={`Copy the ${label} line`} toastSuccess="Copied" />
        </li>
      {/each}
    </ul>

    <p class="env-note">
      Dashes are removed and dots become underscores — <code>show-sql</code> is
      <code>SHOWSQL</code>, not <code>SHOW_SQL</code>. That single rule is why this is worth
      computing rather than typing.
    </p>
  </div>
</Modal>

<style>
  .hdr-key {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-muted);
  }

  .env-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    overflow-y: auto;
  }
  .env-lede {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-muted);
  }
  .env-lede code,
  .env-note code {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-primary);
  }

  /* The name is the answer; everything under it is packaging. */
  .env-name {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .env-name-text {
    flex: 1;
    min-width: 0;
    font-family: var(--font-code);
    font-size: 15px;
    font-weight: 600;
    color: var(--syntax-field, #9876aa);
    word-break: break-all;
  }

  .env-forms {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .env-form {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .env-label {
    flex: 0 0 72px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .env-text {
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    white-space: nowrap;
    font-family: var(--font-code);
    font-size: 12px;
    color: var(--text-primary);
  }

  .env-note {
    margin: 0;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-disabled);
  }
</style>
