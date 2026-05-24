<script lang="ts">
  /**
   * "Authorize in browser" inline form, used by every browser-redirect OAuth
   * provider (GitLab, Linear, Jira). Mirrors the look of the GitHub
   * device-flow form but stays simpler — just a hint, the brand-coloured CTA,
   * and a cancel button.
   *
   * Not used by GitHub itself: its device flow has a distinct UI with the
   * one-time code copy/open buttons, kept inline in the host section.
   */
  import { XCircle } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';

  interface Props {
    /** True while the browser tab is open and Arbor is awaiting the
     *  redirect callback. */
    waiting: boolean;
    /** Error string to show below the buttons; empty/undefined hides it. */
    error?: string;
    /** Brand-coloured CSS background applied to the primary button — pass
     *  the literal value, e.g. `"var(--brand-gitlab)"`. */
    brandColor: string;
    /** Hint paragraph shown while `waiting === false`. */
    hintIdle: string;
    /** Hint paragraph shown while `waiting === true`. */
    hintWaiting: string;
    /** Button label while idle, e.g. `"Authorize with GitLab"`. */
    idleLabel: string;
    /** Button label while waiting, e.g. `"Waiting for browser…"`. */
    busyLabel: string;
    /** Fired when the user clicks the primary authorise button. */
    onAuthorize: () => void;
    /** Fired when the user clicks Cancel. */
    onCancel: () => void;
  }

  let {
    waiting,
    error,
    brandColor,
    hintIdle,
    hintWaiting,
    idleLabel,
    busyLabel,
    onAuthorize,
    onCancel,
  }: Props = $props();
</script>

<div class="inline-form">
  <p class="form-hint">{waiting ? hintWaiting : hintIdle}</p>
  <div class="inline-form-row">
    <button class="btn-save" style:background={brandColor} onclick={onAuthorize} disabled={waiting}>
      {#if waiting}<Spinner size={11} color="#fff" />{/if}
      {waiting ? busyLabel : idleLabel}
    </button>
    <button class="btn-cancel" onclick={onCancel}>Cancel</button>
  </div>
  {#if error}
    <div class="provider-error"><XCircle size={12} />{error}</div>
  {/if}
</div>

<style>
  .inline-form { display: flex; flex-direction: column; gap: 8px; }
  .inline-form-row { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; }

  .form-hint {
    font-size: 10.5px;
    color: var(--text-muted);
    margin: 0;
    line-height: 1.5;
  }

  .btn-save {
    padding: 5px 14px;
    border: none;
    border-radius: var(--radius-sm);
    color: #fff;
    font-family: var(--font-ui-sans);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: filter var(--transition-fast);
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .btn-save:disabled { opacity: 0.45; cursor: not-allowed; }
  .btn-save:not(:disabled):hover { filter: brightness(1.12); }

  .btn-cancel {
    padding: 5px 10px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-muted);
    cursor: pointer;
    transition: all var(--transition-fast);
    white-space: nowrap;
  }
  .btn-cancel:hover { background: var(--bg-hover); color: var(--text-primary); }

  .provider-error {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--error, #f87171);
  }
</style>
