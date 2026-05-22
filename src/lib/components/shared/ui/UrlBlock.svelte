<script lang="ts">
  /**
   * UrlBlock — labelled monospace display for a URL (or any opaque
   * identifier the user needs to read verbatim).
   *
   * Used by deep-link modals (action confirm, clone consent, disabled
   * notice) to show the incoming `arbor://` target URL in a consistent
   * way.  Wraps long values; never truncates with ellipsis since the user
   * is expected to read the whole thing to verify origin.
   *
   *   <UrlBlock label="Repository" value="https://github.com/foo/bar.git" />
   *   <UrlBlock label="Deep link"  value={url} copyable />
   *
   * `copyable` adds a small copy-to-clipboard button on the right.
   */
  import { Copy, Check } from 'lucide-svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    label,
    value,
    copyable = false,
  }: {
    label?:    string;
    value:     string;
    copyable?: boolean;
  } = $props();

  let copied = $state(false);

  async function copy() {
    if (await copyToClipboard(value)) {
      copied = true;
      setTimeout(() => { copied = false; }, 1200);
    }
  }
</script>

<div class="urlblock">
  {#if label}
    <span class="urlblock-label">{label}</span>
  {/if}
  <div class="urlblock-row">
    <code class="urlblock-code">{value}</code>
    {#if copyable}
      <button
        type="button"
        class="urlblock-copy"
        onclick={copy}
        use:tooltip={copied ? 'Copied!' : 'Copy'}
        aria-label="Copy"
      >
        {#if copied}
          <Check size={12} />
        {:else}
          <Copy size={12} />
        {/if}
      </button>
    {/if}
  </div>
</div>

<style>
  .urlblock {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .urlblock-label {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .urlblock-row {
    display: flex;
    align-items: stretch;
    gap: 0;
    min-width: 0;
  }
  .urlblock-code {
    flex: 1;
    font-family: var(--font-code);
    font-size: 11.5px;
    color: var(--text-secondary);
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 6px 10px;
    word-break: break-all;
    line-height: 1.45;
    min-width: 0;
  }
  .urlblock-row :global(.urlblock-copy + .urlblock-code),
  .urlblock-row .urlblock-code:has(+ .urlblock-copy) {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
    border-right: none;
  }
  .urlblock-copy {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .urlblock-copy:hover {
    background: var(--bg-hover);
    color: var(--text-secondary);
  }
</style>
