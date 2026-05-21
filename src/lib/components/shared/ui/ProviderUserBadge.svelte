<script lang="ts">
  import { Check, Copy } from 'lucide-svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Avatar URL — falls back to a circled monogram of the first letter of `name`/`secondary`. */
    avatarUrl?: string | null;
    /** Primary line — typically display name or login. */
    name: string;
    /** Secondary line — email, domain, @handle, etc. Optional. */
    secondary?: string | null;
    /** When true (default), clicking the name / secondary copies it to the clipboard. */
    copyable?: boolean;
  }

  let {
    avatarUrl,
    name,
    secondary,
    copyable = true,
  }: Props = $props();

  // Track which row was just copied so we can flash a tick.
  type CopyKey = 'name' | 'secondary';
  let copiedKey = $state<CopyKey | null>(null);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function copy(text: string, key: CopyKey) {
    if (!copyable || !text) return;
    try {
      await navigator.clipboard.writeText(text);
      copiedKey = key;
      uiStore.showToast('Copied to clipboard', 'success');
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => { copiedKey = null; }, 1400);
    } catch (e) {
      uiStore.showToast(`Copy failed: ${e}`, 'error');
    }
  }

  const initial = $derived((name || secondary || '?').trim()[0]?.toUpperCase() ?? '?');
</script>

<div class="user-badge">
  {#if avatarUrl}
    <img class="user-avatar" src={avatarUrl} alt="" />
  {:else}
    <span class="user-avatar-ph">{initial}</span>
  {/if}

  <div class="user-text">
    {#if copyable}
      <button
        type="button"
        class="copy-line name-line"
        class:copied={copiedKey === 'name'}
        onclick={() => copy(name, 'name')}
        use:tooltip={'Click to copy'}
        aria-label={`Copy ${name}`}
      >
        <span class="line-text">{name}</span>
        <span class="copy-glyph" aria-hidden="true">
          {#if copiedKey === 'name'}<Check size={11} />{:else}<Copy size={11} />{/if}
        </span>
      </button>

      {#if secondary}
        <button
          type="button"
          class="copy-line secondary-line"
          class:copied={copiedKey === 'secondary'}
          onclick={() => copy(secondary, 'secondary')}
          use:tooltip={'Click to copy'}
          aria-label={`Copy ${secondary}`}
        >
          <span class="line-text">{secondary}</span>
          <span class="copy-glyph" aria-hidden="true">
            {#if copiedKey === 'secondary'}<Check size={11} />{:else}<Copy size={11} />{/if}
          </span>
        </button>
      {/if}
    {:else}
      <div class="static-name">{name}</div>
      {#if secondary}<div class="static-secondary">{secondary}</div>{/if}
    {/if}
  </div>
</div>

<style>
  .user-badge {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0 2px;
    min-width: 0;
  }

  .user-avatar {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    object-fit: cover;
    flex-shrink: 0;
  }
  .user-avatar-ph {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--accent-subtle);
    color: var(--accent);
    font-size: 11px;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .user-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  /* Copyable text rows render as buttons but read as inline text. */
  .copy-line {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 6px 2px 4px;
    margin-left: -4px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: inherit;
    cursor: pointer;
    font-family: inherit;
    text-align: left;
    max-width: fit-content;
    transition: background var(--transition-fast),
                border-color var(--transition-fast),
                color var(--transition-fast);
  }
  .copy-line:hover {
    background: var(--bg-hover);
    border-color: var(--border-subtle);
  }
  .copy-line.copied {
    background: rgba(80, 200, 120, 0.10);
    border-color: rgba(80, 200, 120, 0.35);
    color: var(--success);
  }

  .line-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name-line { font-size: 12px; font-weight: 500; color: var(--text-primary); }
  .secondary-line { font-size: 10px; color: var(--text-muted); }

  .copy-glyph {
    display: inline-flex;
    align-items: center;
    opacity: 0;
    color: var(--text-muted);
    transition: opacity var(--transition-fast), color var(--transition-fast);
    flex-shrink: 0;
  }
  .copy-line:hover .copy-glyph { opacity: 0.85; color: var(--text-secondary); }
  .copy-line.copied .copy-glyph { opacity: 1; color: var(--success); }

  /* Non-copyable fallback — keeps the same vertical rhythm. */
  .static-name      { font-size: 12px; font-weight: 500; color: var(--text-primary); }
  .static-secondary { font-size: 10px; color: var(--text-muted); }
</style>
